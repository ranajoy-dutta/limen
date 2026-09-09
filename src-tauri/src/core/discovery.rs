use crate::error::LimenError;
use crate::models::accounts::{Account, Role};
use std::fs;
use std::path::PathBuf;

fn cache_dir() -> Result<PathBuf, LimenError> {
    let dir = crate::core::storage::get_home_dir()?
        .join(".config")
        .join("limen")
        .join("cache");
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

pub async fn get_cached_accounts(start_url_hash: &str) -> Result<Option<Vec<Account>>, LimenError> {
    let dir = cache_dir()?;
    let file_path = dir.join(format!("{}.json", start_url_hash));
    if !file_path.exists() {
        return Ok(None);
    }

    tokio::task::spawn_blocking(move || {
        let content = fs::read_to_string(&file_path).map_err(|e| LimenError::ConfigFile {
            path: file_path.display().to_string(),
            reason: e.to_string(),
        })?;

        let accounts: Vec<Account> =
            serde_json::from_str(&content).map_err(|e| LimenError::ConfigFile {
                path: file_path.display().to_string(),
                reason: format!("Corrupt account cache: {e}"),
            })?;

        Ok(Some(accounts))
    })
    .await
    .map_err(|e| LimenError::internal(format!("Join error reading account cache: {e}")))?
}

pub async fn save_cached_accounts(
    start_url_hash: &str,
    accounts: &[Account],
) -> Result<(), LimenError> {
    let dir = cache_dir()?;
    let file_path = dir.join(format!("{}.json", start_url_hash));
    let data = serde_json::to_string_pretty(accounts)
        .map_err(|e| LimenError::internal(format!("Failed to serialize accounts: {e}")))?;

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
    .map_err(|e| LimenError::internal(format!("Join error writing account cache: {e}")))??;

    Ok(())
}

pub async fn fetch_accounts(
    access_token: &str,
    region: &str,
    start_url_hash: &str,
) -> Result<Vec<Account>, LimenError> {
    if std::env::var("LIMEN_MOCK_AUTH").unwrap_or_default() == "1" {
        let mock_accounts = vec![
            Account {
                account_id: "111122223333".to_string(),
                account_name: "Production".to_string(),
                email: Some("prod-team@example.com".to_string()),
                roles: vec![
                    Role {
                        role_name: "AdministratorAccess".to_string(),
                        account_id: "111122223333".to_string(),
                    },
                    Role {
                        role_name: "ViewOnlyAccess".to_string(),
                        account_id: "111122223333".to_string(),
                    },
                ],
            },
            Account {
                account_id: "444455556666".to_string(),
                account_name: "Development".to_string(),
                email: Some("dev-team@example.com".to_string()),
                roles: vec![
                    Role {
                        role_name: "PowerUserAccess".to_string(),
                        account_id: "444455556666".to_string(),
                    },
                    Role {
                        role_name: "ReadOnly".to_string(),
                        account_id: "444455556666".to_string(),
                    },
                ],
            },
        ];
        save_cached_accounts(start_url_hash, &mock_accounts).await?;
        return Ok(mock_accounts);
    }

    let region_provider = aws_config::Region::new(region.to_string());
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await;
    let sso_client = aws_sdk_sso::Client::new(&config);

    let mut accounts = Vec::new();
    let mut next_token: Option<String> = None;

    loop {
        let mut req = sso_client.list_accounts().access_token(access_token);

        if let Some(token) = next_token {
            req = req.next_token(token);
        }

        let resp = req.send().await.map_err(|e| LimenError::Aws {
            code: "ListAccountsError".to_string(),
            message: format!("Failed to list accounts: {e}"),
            retryable: true,
        })?;

        if let Some(ref account_list) = resp.account_list {
            for acct in account_list {
                let account_id = acct.account_id().unwrap_or_default().to_string();
                let account_name = acct.account_name().unwrap_or_default().to_string();
                let email = acct.email_address().map(|s| s.to_string());

                // Fetch roles for this account
                let mut roles = Vec::new();
                let mut role_next_token: Option<String> = None;

                loop {
                    let mut role_req = sso_client
                        .list_account_roles()
                        .access_token(access_token)
                        .account_id(&account_id);

                    if let Some(r_token) = role_next_token {
                        role_req = role_req.next_token(r_token);
                    }

                    match role_req.send().await {
                        Ok(role_resp) => {
                            if let Some(ref role_list) = role_resp.role_list {
                                for r in role_list {
                                    let role_name = r.role_name().unwrap_or_default().to_string();
                                    roles.push(Role {
                                        role_name,
                                        account_id: account_id.clone(),
                                    });
                                }
                            }
                            role_next_token = role_resp.next_token().map(|s| s.to_string());
                            if role_next_token.is_none() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(account_id = %account_id, error = %e, "Failed to list roles for account");
                            break;
                        }
                    }
                }

                accounts.push(Account {
                    account_id,
                    account_name,
                    email,
                    roles,
                });
            }
        }

        next_token = resp.next_token().map(|s| s.to_string());
        if next_token.is_none() {
            break;
        }
    }

    for acct in &mut accounts {
        acct.roles.sort_by_key(|a| a.role_name.to_lowercase());
    }
    accounts.sort_by_key(|a| a.account_name.to_lowercase());

    save_cached_accounts(start_url_hash, &accounts).await?;
    Ok(accounts)
}
