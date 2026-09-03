use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Error, Serialize, PartialEq)]
#[serde(tag = "kind", content = "detail")]
pub enum LimenError {
    #[error("not signed in")]
    NotAuthenticated,

    #[error("session expired")]
    SessionExpired,

    #[error("network unavailable")]
    Network { retryable: bool },

    #[error("AWS rejected the request: {code}")]
    Aws {
        code: String,
        message: String,
        retryable: bool,
    },

    #[error("throttled by AWS")]
    Throttled { retry_after: Option<u64> },

    #[error("keychain error: {reason}")]
    Keychain { reason: String },

    #[error("could not update config file at {path}: {reason}")]
    ConfigFile { path: String, reason: String },

    #[error("operation cancelled")]
    Cancelled,

    #[error("internal error: {id}")]
    Internal { id: String },
}

impl LimenError {
    pub fn internal(msg: impl std::fmt::Display) -> Self {
        let id = format!("{:x}", md5_or_simple_hash(&msg.to_string()));
        tracing::error!(id = %id, error = %msg, "Internal error occurred");
        Self::Internal { id }
    }
}

fn md5_or_simple_hash(input: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}
