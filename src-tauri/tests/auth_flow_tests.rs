use limen_lib::core::aws_client::MockSsoOidcClient;
use limen_lib::core::session::SessionManager;
use limen_lib::core::storage::{account_key, hash_start_url};
use limen_lib::error::LimenError;
use limen_lib::models::auth::SessionState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[tokio::test]
async fn test_auth_state_flow_to_active() {
    let mock_client = Arc::new(MockSsoOidcClient::new(2)); // 2 pending polls before success
    let ignore_blur = Arc::new(AtomicBool::new(false));
    let session_mgr = SessionManager::new(Some(mock_client), ignore_blur);

    assert_eq!(session_mgr.get_state().await, SessionState::LoggedOut);

    let start_state = session_mgr
        .begin_login(
            "https://test-portal.awsapps.com/start".to_string(),
            "us-east-1".to_string(),
        )
        .await
        .expect("begin_login should succeed");

    match start_state {
        SessionState::AwaitingApproval {
            ref user_code,
            ref verification_uri,
            ..
        } => {
            assert_eq!(user_code, "ABCD-1234");
            assert!(verification_uri.contains("awsapps.com"));
        }
        other => panic!("Unexpected state: {:?}", other),
    }

    // Wait for polling loop to complete
    let mut final_state = session_mgr.get_state().await;
    for _ in 0..50 {
        if let SessionState::Active { .. } = final_state {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        final_state = session_mgr.get_state().await;
    }

    match final_state {
        SessionState::Active {
            start_url, region, ..
        } => {
            assert_eq!(start_url, "https://test-portal.awsapps.com/start");
            assert_eq!(region, "us-east-1");
        }
        other => panic!("Expected Active state, got {:?}", other),
    }

    // Check active tokens in memory
    let tokens = session_mgr
        .get_tokens()
        .await
        .expect("Tokens should be active");
    assert_eq!(tokens.access_token, "mock-access-token-xyz");
    assert_eq!(tokens.refresh_token, "mock-refresh-token-rotated-1");

    // Test Proactive Refresh
    session_mgr
        .refresh_active_session()
        .await
        .expect("Refresh should succeed");

    let refreshed_tokens = session_mgr
        .get_tokens()
        .await
        .expect("Refreshed tokens should exist");
    assert_eq!(
        refreshed_tokens.access_token,
        "mock-refreshed-access-token-new"
    );
    assert_eq!(
        refreshed_tokens.refresh_token,
        "mock-refresh-token-rotated-2"
    );

    // Test Logout
    session_mgr.logout().await.expect("Logout should succeed");
    assert_eq!(session_mgr.get_state().await, SessionState::LoggedOut);
    assert!(session_mgr.get_tokens().await.is_none());
}

#[tokio::test]
async fn test_auth_cancel_flow() {
    let mock_client = Arc::new(MockSsoOidcClient::new(10));
    let ignore_blur = Arc::new(AtomicBool::new(false));
    let session_mgr = SessionManager::new(Some(mock_client), ignore_blur);

    session_mgr
        .begin_login(
            "https://test-portal.awsapps.com/start".to_string(),
            "us-east-1".to_string(),
        )
        .await
        .expect("begin_login should succeed");

    session_mgr
        .cancel_login()
        .await
        .expect("Cancel should succeed");
    assert_eq!(session_mgr.get_state().await, SessionState::LoggedOut);
}

#[tokio::test]
async fn test_refresh_failure_transitions_to_expired() {
    let mock_client = Arc::new(MockSsoOidcClient::new(0)); // 0 pending polls -> immediate success on first poll

    let ignore_blur = Arc::new(AtomicBool::new(false));
    let session_mgr = SessionManager::new(
        Some(Arc::clone(&mock_client) as Arc<dyn limen_lib::core::aws_client::SsoOidcClient>),
        ignore_blur,
    );

    session_mgr
        .begin_login(
            "https://test-portal.awsapps.com/start".to_string(),
            "us-east-1".to_string(),
        )
        .await
        .expect("begin_login should succeed");

    // Wait until login completes to Active
    for _ in 0..50 {
        if let SessionState::Active { .. } = session_mgr.get_state().await {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    assert!(matches!(
        session_mgr.get_state().await,
        SessionState::Active { .. }
    ));

    // Now tell mock client that refresh should fail
    mock_client
        .refresh_should_fail
        .store(true, Ordering::SeqCst);

    let refresh_res = session_mgr.refresh_active_session().await;
    assert!(refresh_res.is_err());

    let state = session_mgr.get_state().await;
    match state {
        SessionState::Expired { .. } => {}
        other => panic!("Expected Expired state, got {:?}", other),
    }
}

#[test]
fn test_storage_key_schemes() {
    let url = "https://my-company.awsapps.com/start";
    let hash = hash_start_url(url);
    assert_eq!(hash.len(), 64);

    let key = account_key(url);
    assert_eq!(key, format!("tenant:{hash}:tokens"));

    let client_key_1 = limen_lib::core::storage::hash_client_key(url, "us-east-1");
    let client_key_2 = limen_lib::core::storage::hash_client_key(url, "us-east-2");
    assert_ne!(client_key_1, client_key_2);
}

#[tokio::test]
async fn test_begin_login_invalid_url_sets_failed_state() {
    let mock_client = Arc::new(MockSsoOidcClient::new(0));
    let ignore_blur = Arc::new(AtomicBool::new(false));
    let session_mgr = SessionManager::new(Some(mock_client), ignore_blur);

    let res = session_mgr
        .begin_login(
            "invalid-url-without-https".to_string(),
            "us-east-1".to_string(),
        )
        .await;
    assert!(res.is_err());

    let state = session_mgr.get_state().await;
    match state {
        SessionState::Failed { message } => {
            assert!(message.contains("https://"));
        }
        other => panic!("Expected Failed state, got {:?}", other),
    }

    // Cancel should reset to LoggedOut
    session_mgr.cancel_login().await.unwrap();
    assert_eq!(session_mgr.get_state().await, SessionState::LoggedOut);
}

#[test]
fn test_error_serialization_discriminated_union() {
    let err = LimenError::Aws {
        code: "InvalidGrantException".to_string(),
        message: "Token has expired".to_string(),
        retryable: false,
    };

    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("\"kind\":\"Aws\""));
    assert!(json.contains("\"code\":\"InvalidGrantException\""));

    let not_auth = LimenError::NotAuthenticated;
    let not_auth_json = serde_json::to_string(&not_auth).unwrap();
    assert_eq!(not_auth_json, "{\"kind\":\"NotAuthenticated\"}");
}
