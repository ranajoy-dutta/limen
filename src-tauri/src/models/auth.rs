use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ClientRegistration {
    pub client_id: String,
    pub client_secret: String,
    pub client_id_issued_at: i64,
    pub client_secret_expires_at: i64,
}

impl std::fmt::Debug for ClientRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientRegistration")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("client_id_issued_at", &self.client_id_issued_at)
            .field("client_secret_expires_at", &self.client_secret_expires_at)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SsoTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub region: String,
    pub start_url: String,
}

impl std::fmt::Debug for SsoTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SsoTokens")
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("region", &self.region)
            .field("start_url", &self.start_url)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "data")]
pub enum SessionState {
    LoggedOut,
    Registering,
    AwaitingApproval {
        user_code: String,
        verification_uri: String,
        expires_at: DateTime<Utc>,
    },
    Active {
        expires_at: DateTime<Utc>,
        start_url: String,
        region: String,
    },
    Refreshing,
    Expired {
        reason: String,
    },
    Failed {
        message: String,
    },
}
