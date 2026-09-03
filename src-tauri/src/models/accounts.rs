use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub role_name: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub account_id: String,
    pub account_name: String,
    pub email: Option<String>,
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveProfile {
    pub profile_name: String,
    pub account_id: String,
    pub account_name: String,
    pub role_name: String,
    pub expires_at: DateTime<Utc>,
}
