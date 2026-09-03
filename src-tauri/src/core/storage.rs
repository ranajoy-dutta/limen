use crate::error::LimenError;
use crate::models::auth::{ClientRegistration, SsoTokens};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

pub fn hash_start_url(start_url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(start_url.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn account_key(start_url: &str) -> String {
    format!("tenant:{}:tokens", hash_start_url(start_url))
}

pub fn get_home_dir() -> Result<PathBuf, LimenError> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home));
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.trim().is_empty() {
            return Ok(PathBuf::from(profile));
        }
    }
    Err(LimenError::internal(
        "Could not determine user home directory (neither HOME nor USERPROFILE is set)",
    ))
}

fn tokens_dir() -> Result<PathBuf, LimenError> {
    let dir = get_home_dir()?.join(".config").join("limen").join("tokens");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| LimenError::ConfigFile {
            path: dir.display().to_string(),
            reason: e.to_string(),
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }
    }
    Ok(dir)
}

pub async fn save_tokens(tokens: &SsoTokens) -> Result<(), LimenError> {
    let dir = tokens_dir()?;
    let file_path = dir.join(format!("{}.json", hash_start_url(&tokens.start_url)));
    let data = serde_json::to_string_pretty(tokens)
        .map_err(|e| LimenError::internal(format!("Failed to serialize tokens: {e}")))?;

    tokio::task::spawn_blocking(move || {
        fs::write(&file_path, data).map_err(|e| LimenError::ConfigFile {
            path: file_path.display().to_string(),
            reason: e.to_string(),
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600));
        }

        Ok::<(), LimenError>(())
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error writing tokens: {e}")))??;

    Ok(())
}

pub async fn get_tokens(start_url: &str) -> Result<Option<SsoTokens>, LimenError> {
    let dir = tokens_dir()?;
    let file_path = dir.join(format!("{}.json", hash_start_url(start_url)));

    if !file_path.exists() {
        return Ok(None);
    }

    tokio::task::spawn_blocking(move || {
        let content = fs::read_to_string(&file_path).map_err(|e| LimenError::ConfigFile {
            path: file_path.display().to_string(),
            reason: e.to_string(),
        })?;

        let tokens: SsoTokens = serde_json::from_str(&content).map_err(|e| {
            LimenError::ConfigFile {
                path: file_path.display().to_string(),
                reason: format!("JSON parse error: {e}"),
            }
        })?;

        Ok(Some(tokens))
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error reading tokens: {e}")))?
}

pub async fn delete_tokens(start_url: &str) -> Result<(), LimenError> {
    let dir = tokens_dir()?;
    let file_path = dir.join(format!("{}.json", hash_start_url(start_url)));

    if file_path.exists() {
        tokio::task::spawn_blocking(move || {
            let _ = fs::remove_file(&file_path);
            Ok::<(), LimenError>(())
        })
        .await
        .map_err(|e| LimenError::internal(format!("Join error deleting tokens: {e}")))?
    } else {
        Ok(())
    }
}

fn client_cache_dir() -> Result<PathBuf, LimenError> {
    let dir = get_home_dir()?.join(".config").join("limen").join("clients");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| LimenError::ConfigFile {
            path: dir.display().to_string(),
            reason: e.to_string(),
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }
    }
    Ok(dir)
}

pub async fn save_client_registration(
    start_url: &str,
    registration: &ClientRegistration,
) -> Result<(), LimenError> {
    let dir = client_cache_dir()?;
    let file_path = dir.join(format!("{}.json", hash_start_url(start_url)));
    let data = serde_json::to_string_pretty(registration)
        .map_err(|e| LimenError::internal(format!("Failed to serialize client registration: {e}")))?;

    tokio::task::spawn_blocking(move || {
        fs::write(&file_path, data).map_err(|e| LimenError::ConfigFile {
            path: file_path.display().to_string(),
            reason: e.to_string(),
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600));
        }

        Ok::<(), LimenError>(())
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error writing client registration: {e}")))??;

    Ok(())
}

pub async fn get_client_registration(
    start_url: &str,
) -> Result<Option<ClientRegistration>, LimenError> {
    let dir = client_cache_dir()?;
    let file_path = dir.join(format!("{}.json", hash_start_url(start_url)));

    if !file_path.exists() {
        return Ok(None);
    }

    tokio::task::spawn_blocking(move || {
        let content = fs::read_to_string(&file_path).map_err(|e| LimenError::ConfigFile {
            path: file_path.display().to_string(),
            reason: e.to_string(),
        })?;

        let reg: ClientRegistration = serde_json::from_str(&content).map_err(|e| {
            LimenError::ConfigFile {
                path: file_path.display().to_string(),
                reason: format!("JSON parse error: {e}"),
            }
        })?;

        let now = chrono::Utc::now().timestamp();
        if reg.client_secret_expires_at <= now {
            let _ = fs::remove_file(&file_path);
            return Ok(None);
        }

        Ok(Some(reg))
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error reading client registration: {e}")))?
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LastSessionInfo {
    pub start_url: String,
    pub region: String,
}

pub async fn save_last_session(start_url: &str, region: &str) -> Result<(), LimenError> {
    let dir = get_home_dir()?.join(".config").join("limen");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| LimenError::ConfigFile {
            path: dir.display().to_string(),
            reason: e.to_string(),
        })?;
    }
    let file_path = dir.join("last_session.json");
    let info = LastSessionInfo {
        start_url: start_url.to_string(),
        region: region.to_string(),
    };
    let data = serde_json::to_string_pretty(&info)
        .map_err(|e| LimenError::internal(format!("Failed to serialize last session: {e}")))?;

    tokio::task::spawn_blocking(move || {
        fs::write(&file_path, data).map_err(|e| LimenError::ConfigFile {
            path: file_path.display().to_string(),
            reason: e.to_string(),
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600));
        }
        Ok::<(), LimenError>(())
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error writing last session: {e}")))??;

    Ok(())
}

pub async fn get_last_session() -> Result<Option<LastSessionInfo>, LimenError> {
    let file_path = get_home_dir()?.join(".config").join("limen").join("last_session.json");
    if !file_path.exists() {
        return Ok(None);
    }

    tokio::task::spawn_blocking(move || {
        let content = fs::read_to_string(&file_path).map_err(|e| LimenError::ConfigFile {
            path: file_path.display().to_string(),
            reason: e.to_string(),
        })?;
        let info: LastSessionInfo = serde_json::from_str(&content).map_err(|e| LimenError::ConfigFile {
            path: file_path.display().to_string(),
            reason: format!("JSON parse error: {e}"),
        })?;
        Ok(Some(info))
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error reading last session: {e}")))?
}
