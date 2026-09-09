use crate::core::aws_client::{CreateTokenOutcome, SsoOidcClient};
use crate::core::storage;
use crate::error::LimenError;
use crate::models::auth::{SessionState, SsoTokens};
use chrono::{Duration, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SessionManager {
    state: Arc<RwLock<SessionState>>,
    active_tokens: Arc<RwLock<Option<SsoTokens>>>,
    mock_client: Option<Arc<dyn SsoOidcClient>>,
    cancel_flag: Arc<AtomicBool>,
    ignore_blur_flag: Arc<AtomicBool>,
}

impl SessionManager {
    pub fn new(
        mock_client: Option<Arc<dyn SsoOidcClient>>,
        ignore_blur_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            state: Arc::new(RwLock::new(SessionState::LoggedOut)),
            active_tokens: Arc::new(RwLock::new(None)),
            mock_client,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            ignore_blur_flag,
        }
    }

    async fn get_client(&self, region: &str) -> Arc<dyn SsoOidcClient> {
        if let Some(ref mock) = self.mock_client {
            return Arc::clone(mock);
        }
        Arc::new(crate::core::aws_client::AwsSsoOidcClient::new(region).await)
    }

    pub async fn get_state(&self) -> SessionState {
        self.state.read().await.clone()
    }

    pub async fn get_tokens(&self) -> Option<SsoTokens> {
        self.active_tokens.read().await.clone()
    }

    pub async fn ensure_active_token(&self) -> Result<SsoTokens, LimenError> {
        let current = match self.get_tokens().await {
            Some(t) => t,
            None => {
                let session_state = self.get_state().await;
                if let SessionState::Active { start_url, .. } = session_state {
                    match storage::get_tokens(&start_url).await? {
                        Some(t) => t,
                        None => return Err(LimenError::NotAuthenticated),
                    }
                } else {
                    return Err(LimenError::NotAuthenticated);
                }
            }
        };

        // Proactively refresh if token expires within 15 minutes and refresh token is available
        if current.expires_at < Utc::now() + Duration::minutes(15)
            && !current.refresh_token.is_empty()
        {
            tracing::info!("Access token expiring within 15m; proactively refreshing session");
            if let Ok(()) = self.refresh_active_session().await {
                if let Some(refreshed) = self.get_tokens().await {
                    return Ok(refreshed);
                }
            }
        }

        Ok(current)
    }

    pub async fn restore_last_session(&self) -> Result<(), LimenError> {
        if let Ok(Some(last)) = storage::get_last_session().await {
            if let Ok(Some(tokens)) = storage::get_tokens(&last.start_url).await {
                if tokens.expires_at > Utc::now() + Duration::minutes(1) {
                    *self.active_tokens.write().await = Some(tokens.clone());
                    *self.state.write().await = SessionState::Active {
                        expires_at: tokens.expires_at,
                        start_url: tokens.start_url,
                        region: tokens.region,
                    };
                    tracing::info!("Restored active session for {}", last.start_url);
                } else {
                    *self.state.write().await = SessionState::Expired {
                        reason: "Session expired. Please sign in again.".to_string(),
                    };
                }
            }
        }
        Ok(())
    }

    pub async fn begin_login(
        &self,
        start_url: String,
        region: String,
    ) -> Result<SessionState, LimenError> {
        let clean_url = start_url.trim().to_string();
        if !clean_url.starts_with("https://") {
            return Err(LimenError::Aws {
                code: "InvalidRequestException".to_string(),
                message: "Start URL must start with https://".to_string(),
                retryable: false,
            });
        }

        let _ = storage::save_last_session(&clean_url, &region).await;

        self.cancel_flag.store(false, Ordering::SeqCst);
        *self.state.write().await = SessionState::Registering;

        let client = self.get_client(&region).await;

        // 1. Get or register client
        let reg = match storage::get_client_registration(&clean_url).await? {
            Some(cached) => cached,
            None => {
                let fresh = client.register_client("limen", &clean_url).await?;
                storage::save_client_registration(&clean_url, &fresh).await?;
                fresh
            }
        };

        // 2. Start device authorization
        let auth_resp = client
            .start_device_authorization(&reg.client_id, &reg.client_secret, &clean_url)
            .await?;

        // 3. Open browser safely while suppressing window blur hide (never during automated tests or mock mode)
        if !cfg!(test) && self.mock_client.is_none() {
            if let Some(uri_complete) = &auth_resp.verification_uri_complete {
                self.ignore_blur_flag.store(true, Ordering::SeqCst);
                let _ = open::that_detached(uri_complete);
                let ignore_blur = Arc::clone(&self.ignore_blur_flag);
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    ignore_blur.store(false, Ordering::SeqCst);
                });
            }
        }

        let expires_at = Utc::now() + Duration::seconds(auth_resp.expires_in as i64);
        let awaiting_state = SessionState::AwaitingApproval {
            user_code: auth_resp.user_code.clone(),
            verification_uri: auth_resp.verification_uri.clone(),
            expires_at,
        };
        *self.state.write().await = awaiting_state.clone();

        // 4. Spawn background token poll loop
        let state_clone = Arc::clone(&self.state);
        let tokens_clone = Arc::clone(&self.active_tokens);
        let client_clone = Arc::clone(&client);
        let cancel_flag = Arc::clone(&self.cancel_flag);
        let device_code = auth_resp.device_code;
        let client_id = reg.client_id;
        let client_secret = reg.client_secret;
        let mut interval_secs = auth_resp.interval.max(1) as u64;
        let target_url = clean_url;
        let target_region = region;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;

                if cancel_flag.load(Ordering::SeqCst) {
                    tracing::info!("Device flow polling cancelled");
                    break;
                }

                if Utc::now() > expires_at {
                    *state_clone.write().await = SessionState::Expired {
                        reason: "Device code expired".to_string(),
                    };
                    break;
                }

                match client_clone
                    .create_token_device_code(&client_id, &client_secret, &device_code)
                    .await
                {
                    Ok(CreateTokenOutcome::Success {
                        access_token,
                        refresh_token,
                        expires_in,
                    }) => {
                        let token_expires_at = Utc::now() + Duration::seconds(expires_in as i64);

                        let tokens = SsoTokens {
                            access_token,
                            refresh_token: refresh_token.unwrap_or_default(),
                            expires_at: token_expires_at,
                            region: target_region.clone(),
                            start_url: target_url.clone(),
                        };

                        if let Err(e) = storage::save_tokens(&tokens).await {
                            tracing::error!(error = ?e, "Failed to save tokens to local storage");
                        }

                        *tokens_clone.write().await = Some(tokens);
                        *state_clone.write().await = SessionState::Active {
                            expires_at: token_expires_at,
                            start_url: target_url,
                            region: target_region,
                        };

                        tracing::info!("Authentication active! Scheduled proactive refresh");
                        break;
                    }
                    Ok(CreateTokenOutcome::AuthorizationPending) => {
                        // Keep polling
                    }
                    Ok(CreateTokenOutcome::SlowDown) => {
                        interval_secs += 5;
                    }
                    Err(err) => {
                        tracing::error!(error = ?err, "Device code polling failed");
                        *state_clone.write().await = SessionState::Failed {
                            message: err.to_string(),
                        };
                        break;
                    }
                }
            }
        });

        Ok(awaiting_state)
    }

    pub async fn cancel_login(&self) -> Result<(), LimenError> {
        self.cancel_flag.store(true, Ordering::SeqCst);
        *self.state.write().await = SessionState::LoggedOut;
        Ok(())
    }

    pub async fn logout(&self) -> Result<(), LimenError> {
        let current_tokens = self.active_tokens.read().await.clone();
        if let Some(tokens) = current_tokens {
            let _ = storage::delete_tokens(&tokens.start_url).await;
        }

        *self.active_tokens.write().await = None;
        *self.state.write().await = SessionState::LoggedOut;
        Ok(())
    }

    pub async fn refresh_active_session(&self) -> Result<(), LimenError> {
        let current_tokens = match self.active_tokens.read().await.clone() {
            Some(t) => t,
            None => return Err(LimenError::NotAuthenticated),
        };

        let reg = match storage::get_client_registration(&current_tokens.start_url).await? {
            Some(r) => r,
            None => {
                *self.state.write().await = SessionState::Expired {
                    reason: "Client registration expired".to_string(),
                };
                return Err(LimenError::SessionExpired);
            }
        };

        *self.state.write().await = SessionState::Refreshing;

        let client = self.get_client(&current_tokens.region).await;

        match client
            .create_token_refresh(
                &reg.client_id,
                &reg.client_secret,
                &current_tokens.refresh_token,
            )
            .await
        {
            Ok(CreateTokenOutcome::Success {
                access_token,
                refresh_token,
                expires_in,
            }) => {
                let new_expires_at = Utc::now() + Duration::seconds(expires_in as i64);
                let rotated_refresh = refresh_token.unwrap_or(current_tokens.refresh_token);

                let new_tokens = SsoTokens {
                    access_token,
                    refresh_token: rotated_refresh,
                    expires_at: new_expires_at,
                    region: current_tokens.region.clone(),
                    start_url: current_tokens.start_url.clone(),
                };

                storage::save_tokens(&new_tokens).await?;
                *self.active_tokens.write().await = Some(new_tokens);
                *self.state.write().await = SessionState::Active {
                    expires_at: new_expires_at,
                    start_url: current_tokens.start_url,
                    region: current_tokens.region,
                };

                Ok(())
            }
            Ok(_) => Err(LimenError::internal("Unexpected outcome during refresh")),
            Err(e) => {
                *self.state.write().await = SessionState::Expired {
                    reason: e.to_string(),
                };
                Err(e)
            }
        }
    }
}
