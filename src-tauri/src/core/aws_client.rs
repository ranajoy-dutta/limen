use crate::error::LimenError;
use crate::models::auth::ClientRegistration;
use async_trait::async_trait;
use aws_sdk_ssooidc::error::ProvideErrorMetadata;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: i32,
    pub interval: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CreateTokenOutcome {
    Success {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: i32,
    },
    AuthorizationPending,
    SlowDown,
}

#[async_trait]
pub trait SsoOidcClient: Send + Sync {
    async fn register_client(
        &self,
        client_name: &str,
        start_url: &str,
    ) -> Result<ClientRegistration, LimenError>;

    async fn start_device_authorization(
        &self,
        client_id: &str,
        client_secret: &str,
        start_url: &str,
    ) -> Result<DeviceAuthResponse, LimenError>;

    async fn create_token_device_code(
        &self,
        client_id: &str,
        client_secret: &str,
        device_code: &str,
    ) -> Result<CreateTokenOutcome, LimenError>;

    async fn create_token_refresh(
        &self,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<CreateTokenOutcome, LimenError>;
}

pub struct AwsSsoOidcClient {
    client: aws_sdk_ssooidc::Client,
}

impl AwsSsoOidcClient {
    pub async fn new(region: &str) -> Self {
        let region_provider = aws_config::Region::new(region.to_string());
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region_provider)
            .load()
            .await;
        let client = aws_sdk_ssooidc::Client::new(&config);
        Self { client }
    }
}

#[async_trait]
impl SsoOidcClient for AwsSsoOidcClient {
    async fn register_client(
        &self,
        client_name: &str,
        start_url: &str,
    ) -> Result<ClientRegistration, LimenError> {
        let resp = self
            .client
            .register_client()
            .client_name(client_name)
            .client_type("public")
            .scopes("sso:account:access")
            .grant_types("urn:ietf:params:oauth:grant-type:device_code")
            .grant_types("refresh_token")
            .issuer_url(start_url)
            .send()
            .await
            .map_err(|e| map_sdk_error("RegisterClient", &e))?;

        let client_id = resp.client_id().unwrap_or_default().to_string();
        let client_secret = resp.client_secret().unwrap_or_default().to_string();
        let client_id_issued_at = resp.client_id_issued_at();
        let client_secret_expires_at = resp.client_secret_expires_at();

        Ok(ClientRegistration {
            client_id,
            client_secret,
            client_id_issued_at,
            client_secret_expires_at,
        })
    }

    async fn start_device_authorization(
        &self,
        client_id: &str,
        client_secret: &str,
        start_url: &str,
    ) -> Result<DeviceAuthResponse, LimenError> {
        let resp = self
            .client
            .start_device_authorization()
            .client_id(client_id)
            .client_secret(client_secret)
            .start_url(start_url)
            .send()
            .await
            .map_err(|e| map_sdk_error("StartDeviceAuthorization", &e))?;

        Ok(DeviceAuthResponse {
            device_code: resp.device_code().unwrap_or_default().to_string(),
            user_code: resp.user_code().unwrap_or_default().to_string(),
            verification_uri: resp.verification_uri().unwrap_or_default().to_string(),
            verification_uri_complete: resp.verification_uri_complete().map(|s| s.to_string()),
            expires_in: resp.expires_in(),
            interval: resp.interval(),
        })
    }

    async fn create_token_device_code(
        &self,
        client_id: &str,
        client_secret: &str,
        device_code: &str,
    ) -> Result<CreateTokenOutcome, LimenError> {
        let result = self
            .client
            .create_token()
            .client_id(client_id)
            .client_secret(client_secret)
            .grant_type("urn:ietf:params:oauth:grant-type:device_code")
            .device_code(device_code)
            .send()
            .await;

        match result {
            Ok(resp) => Ok(CreateTokenOutcome::Success {
                access_token: resp.access_token().unwrap_or_default().to_string(),
                refresh_token: resp.refresh_token().map(|s| s.to_string()),
                expires_in: resp.expires_in(),
            }),
            Err(e) => {
                if let Some(err_code) = e.code() {
                    match err_code {
                        "AuthorizationPendingException" => Ok(CreateTokenOutcome::AuthorizationPending),
                        "SlowDownException" => Ok(CreateTokenOutcome::SlowDown),
                        "ExpiredTokenException" => Err(LimenError::Aws {
                            code: err_code.to_string(),
                            message: "Verification code expired. Please restart sign in.".to_string(),
                            retryable: false,
                        }),
                        "AccessDeniedException" => Err(LimenError::Aws {
                            code: err_code.to_string(),
                            message: "Access request was denied by user or policy.".to_string(),
                            retryable: false,
                        }),
                        _ => Err(map_sdk_error("CreateToken", &e)),
                    }
                } else {
                    Err(map_sdk_error("CreateToken", &e))
                }
            }
        }
    }

    async fn create_token_refresh(
        &self,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<CreateTokenOutcome, LimenError> {
        let result = self
            .client
            .create_token()
            .client_id(client_id)
            .client_secret(client_secret)
            .grant_type("refresh_token")
            .refresh_token(refresh_token)
            .send()
            .await;

        match result {
            Ok(resp) => Ok(CreateTokenOutcome::Success {
                access_token: resp.access_token().unwrap_or_default().to_string(),
                refresh_token: resp.refresh_token().map(|s| s.to_string()),
                expires_in: resp.expires_in(),
            }),
            Err(e) => {
                if let Some(code) = e.code() {
                    if code == "InvalidGrantException" || code == "UnauthorizedException" {
                        return Err(LimenError::SessionExpired);
                    }
                }
                Err(map_sdk_error("CreateTokenRefresh", &e))
            }
        }
    }
}

fn map_sdk_error<E: ProvideErrorMetadata + std::fmt::Display>(op: &str, err: &E) -> LimenError {
    let code = err.code().unwrap_or("Unknown").to_string();
    let message = err.message().unwrap_or(&err.to_string()).to_string();

    tracing::error!(operation = op, code = %code, message = %message, "AWS SDK error");

    if code == "AccessDeniedException" {
        LimenError::Aws {
            code,
            message: "Access denied by AWS IAM Identity Center".to_string(),
            retryable: false,
        }
    } else if code == "SlowDownException" || code == "ThrottlingException" {
        LimenError::Throttled { retry_after: Some(5) }
    } else {
        LimenError::Aws {
            code,
            message,
            retryable: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Offline Mock Implementation for Deterministic Local Testing
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MockSsoOidcClient {
    pub pending_polls_remaining: std::sync::Arc<std::sync::atomic::AtomicU32>,
    pub refresh_should_fail: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl MockSsoOidcClient {
    pub fn new(polls_before_success: u32) -> Self {
        Self {
            pending_polls_remaining: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(
                polls_before_success,
            )),
            refresh_should_fail: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl SsoOidcClient for MockSsoOidcClient {
    async fn register_client(
        &self,
        client_name: &str,
        _start_url: &str,
    ) -> Result<ClientRegistration, LimenError> {
        let now = Utc::now().timestamp();
        Ok(ClientRegistration {
            client_id: format!("mock-client-id-for-{client_name}"),
            client_secret: "mock-client-secret-12345".to_string(),
            client_id_issued_at: now,
            client_secret_expires_at: now + 90 * 86400,
        })
    }

    async fn start_device_authorization(
        &self,
        _client_id: &str,
        _client_secret: &str,
        _start_url: &str,
    ) -> Result<DeviceAuthResponse, LimenError> {
        Ok(DeviceAuthResponse {
            device_code: "mock-device-code-abc-123".to_string(),
            user_code: "ABCD-1234".to_string(),
            verification_uri: "https://view.awsapps.com/start/#/device".to_string(),
            verification_uri_complete: None,
            expires_in: 900,
            interval: 1, // fast interval for testing
        })
    }

    async fn create_token_device_code(
        &self,
        _client_id: &str,
        _client_secret: &str,
        _device_code: &str,
    ) -> Result<CreateTokenOutcome, LimenError> {
        use std::sync::atomic::Ordering;
        let remaining = self.pending_polls_remaining.load(Ordering::SeqCst);
        if remaining > 0 {
            self.pending_polls_remaining.fetch_sub(1, Ordering::SeqCst);
            Ok(CreateTokenOutcome::AuthorizationPending)
        } else {
            Ok(CreateTokenOutcome::Success {
                access_token: "mock-access-token-xyz".to_string(),
                refresh_token: Some("mock-refresh-token-rotated-1".to_string()),
                expires_in: 28800, // 8 hours
            })
        }
    }

    async fn create_token_refresh(
        &self,
        _client_id: &str,
        _client_secret: &str,
        _refresh_token: &str,
    ) -> Result<CreateTokenOutcome, LimenError> {
        use std::sync::atomic::Ordering;
        if self.refresh_should_fail.load(Ordering::SeqCst) {
            Err(LimenError::SessionExpired)
        } else {
            Ok(CreateTokenOutcome::Success {
                access_token: "mock-refreshed-access-token-new".to_string(),
                refresh_token: Some("mock-refresh-token-rotated-2".to_string()),
                expires_in: 28800,
            })
        }
    }
}
