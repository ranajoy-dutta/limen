use crate::error::LimenError;
use crate::models::accounts::ActiveProfile;
use chrono::{TimeZone, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn aws_credentials_path() -> Result<PathBuf, LimenError> {
    let aws_dir = crate::core::storage::get_home_dir()?.join(".aws");
    if !aws_dir.exists() {
        fs::create_dir_all(&aws_dir).map_err(|e| LimenError::ConfigFile {
            path: aws_dir.display().to_string(),
            reason: e.to_string(),
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&aws_dir, fs::Permissions::from_mode(0o700));
        }
    }
    Ok(aws_dir.join("credentials"))
}

fn atomic_write_credentials_file(
    creds_file: &std::path::Path,
    content: &str,
) -> Result<(), LimenError> {
    let parent = creds_file
        .parent()
        .ok_or_else(|| LimenError::internal("Missing credentials parent directory"))?;
    let tmp_file = parent.join(format!(".credentials.tmp.{}", std::process::id()));

    fs::write(&tmp_file, content).map_err(|e| LimenError::ConfigFile {
        path: tmp_file.display().to_string(),
        reason: format!("Failed to write temporary credentials file: {e}"),
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&tmp_file, fs::Permissions::from_mode(0o600)) {
            tracing::warn!("Failed to set permissions on {}: {e}", tmp_file.display());
        }
    }

    fs::rename(&tmp_file, creds_file).map_err(|e| LimenError::ConfigFile {
        path: creds_file.display().to_string(),
        reason: format!("Failed to atomically replace ~/.aws/credentials: {e}"),
    })?;

    Ok(())
}

fn active_profiles_path() -> Result<PathBuf, LimenError> {
    let dir = crate::core::storage::get_home_dir()?
        .join(".config")
        .join("limen");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    Ok(dir.join("active_profiles.json"))
}

pub async fn load_active_profiles() -> Result<HashMap<String, ActiveProfile>, LimenError> {
    let path = active_profiles_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }

    tokio::task::spawn_blocking(move || {
        let content = fs::read_to_string(&path).map_err(|e| LimenError::ConfigFile {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

        let map: HashMap<String, ActiveProfile> =
            serde_json::from_str(&content).unwrap_or_default();
        let now = Utc::now();
        // Filter out expired profiles
        let valid: HashMap<String, ActiveProfile> = map
            .into_iter()
            .filter(|(_, p)| p.expires_at > now)
            .collect();

        Ok(valid)
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error reading active profiles: {e}")))?
}

pub async fn save_active_profiles(
    profiles: &HashMap<String, ActiveProfile>,
) -> Result<(), LimenError> {
    let path = active_profiles_path()?;
    let data = serde_json::to_string_pretty(profiles)
        .map_err(|e| LimenError::internal(format!("Failed to serialize active profiles: {e}")))?;

    tokio::task::spawn_blocking(move || {
        fs::write(&path, data).map_err(|e| LimenError::ConfigFile {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }

        Ok::<(), LimenError>(())
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error writing active profiles: {e}")))??;

    Ok(())
}

pub async fn activate_role(
    access_token: &str,
    region: &str,
    account_id: &str,
    account_name: &str,
    role_name: &str,
    custom_profile_name: Option<String>,
    set_as_default: bool,
) -> Result<ActiveProfile, LimenError> {
    let target_profile = match custom_profile_name {
        Some(name) => {
            let sanitized: String = name
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
                .collect();
            let trimmed = sanitized.trim_matches(|c| c == '-' || c == '_' || c == '.');
            if trimmed.is_empty() {
                "default".to_string()
            } else {
                trimmed.to_lowercase()
            }
        }
        None => "default".to_string(),
    };

    let (access_key_id, secret_access_key, session_token, expires_at) =
        if std::env::var("LIMEN_MOCK_AUTH").unwrap_or_default() == "1" {
            (
                "ASIAPRODMOCKKEY12345".to_string(),
                "mockSecretAccessKey987654321".to_string(),
                "mockSessionTokenABCDEF".to_string(),
                Utc::now() + chrono::Duration::hours(1),
            )
        } else {
            let region_provider = aws_config::Region::new(region.to_string());
            let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(region_provider)
                .load()
                .await;
            let sso_client = aws_sdk_sso::Client::new(&config);

            let resp = sso_client
                .get_role_credentials()
                .access_token(access_token)
                .account_id(account_id)
                .role_name(role_name)
                .send()
                .await
                .map_err(|e| LimenError::Aws {
                    code: "GetRoleCredentialsError".to_string(),
                    message: format!("Failed to get credentials for role {role_name}: {e}"),
                    retryable: false,
                })?;

            let creds = resp.role_credentials().ok_or_else(|| LimenError::Aws {
                code: "MissingCredentials".to_string(),
                message: "AWS SSO returned empty role credentials".to_string(),
                retryable: false,
            })?;

            let access_key = creds.access_key_id().unwrap_or_default().to_string();
            let secret_key = creds.secret_access_key().unwrap_or_default().to_string();
            let token = creds.session_token().unwrap_or_default().to_string();
            let exp_millis = creds.expiration();
            let expires = Utc
                .timestamp_millis_opt(exp_millis)
                .single()
                .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(1));

            (access_key, secret_key, token, expires)
        };

    // Write to ~/.aws/credentials: writes [{target_profile}] and optionally [default]
    tokio::task::spawn_blocking({
        let access_key_id = access_key_id.clone();
        let secret_access_key = secret_access_key.clone();
        let session_token = session_token.clone();
        let region = region.to_string();
        let profile_name = target_profile.clone();

        move || -> Result<(), LimenError> {
            let creds_file = aws_credentials_path()?;
            let existing_content = if creds_file.exists() {
                fs::read_to_string(&creds_file).unwrap_or_default()
            } else {
                String::new()
            };

            let mut updated_lines = Vec::new();
            let mut skip_section = false;

            for line in existing_content.lines() {
                let trimmed = line.trim();
                if (set_as_default && trimmed == "[default]")
                    || trimmed == format!("[{profile_name}]")
                {
                    skip_section = true;
                    continue;
                } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    skip_section = false;
                }

                if !skip_section {
                    updated_lines.push(line.to_string());
                }
            }

            // Append updated [{profile_name}]
            let new_profile = format!(
                "[{profile_name}]\naws_access_key_id = {access_key_id}\naws_secret_access_key = {secret_access_key}\naws_session_token = {session_token}\nregion = {region}"
            );
            updated_lines.push(new_profile);

            // If set_as_default is enabled, also append [default]
            if set_as_default {
                let new_default = format!(
                    "[default]\naws_access_key_id = {access_key_id}\naws_secret_access_key = {secret_access_key}\naws_session_token = {session_token}\nregion = {region}"
                );
                updated_lines.push(new_default);
            }

            let output = updated_lines.join("\n") + "\n";
            atomic_write_credentials_file(&creds_file, &output)?;
            Ok(())
        }
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error writing AWS credentials: {e}")))??;

    let active_profile = ActiveProfile {
        profile_name: target_profile,
        account_id: account_id.to_string(),
        account_name: account_name.to_string(),
        role_name: role_name.to_string(),
        expires_at,
    };

    // Save to active profiles map
    let mut current_profiles = load_active_profiles().await.unwrap_or_default();
    current_profiles.insert(active_profile.profile_name.clone(), active_profile.clone());
    save_active_profiles(&current_profiles).await?;

    Ok(active_profile)
}

pub async fn deactivate_role(profile_name: &str) -> Result<(), LimenError> {
    let profile_name = profile_name.to_string();

    tokio::task::spawn_blocking({
        let profile_name = profile_name.clone();
        move || -> Result<(), LimenError> {
            let creds_file = aws_credentials_path()?;
            if !creds_file.exists() {
                return Ok(());
            }

            let existing = fs::read_to_string(&creds_file).unwrap_or_default();
            let mut updated_lines = Vec::new();
            let mut skip_section = false;

            for line in existing.lines() {
                let trimmed = line.trim();
                if trimmed == format!("[{profile_name}]") {
                    skip_section = true;
                    continue;
                } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    skip_section = false;
                }

                if !skip_section {
                    updated_lines.push(line.to_string());
                }
            }

            let output = updated_lines.join("\n") + "\n";
            atomic_write_credentials_file(&creds_file, &output)?;
            Ok(())
        }
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error deactivating profile: {e}")))??;

    let mut current = load_active_profiles().await.unwrap_or_default();
    current.remove(&profile_name);
    save_active_profiles(&current).await?;

    Ok(())
}
