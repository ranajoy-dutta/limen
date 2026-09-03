use crate::core::{credentials, discovery, storage};
use crate::error::LimenError;
use crate::models::accounts::{Account, ActiveProfile};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_accounts(app_state: State<'_, AppState>) -> Result<Vec<Account>, LimenError> {
    let tokens = app_state.session_manager.ensure_active_token().await?;
    let start_url_hash = storage::hash_start_url(&tokens.start_url);

    if let Ok(Some(cached)) = discovery::get_cached_accounts(&start_url_hash).await {
        if !cached.is_empty() {
            return Ok(cached);
        }
    }

    discovery::fetch_accounts(&tokens.access_token, &tokens.region, &start_url_hash).await
}

#[tauri::command]
pub async fn refresh_accounts(app_state: State<'_, AppState>) -> Result<Vec<Account>, LimenError> {
    let tokens = app_state.session_manager.ensure_active_token().await?;
    let start_url_hash = storage::hash_start_url(&tokens.start_url);
    discovery::fetch_accounts(&tokens.access_token, &tokens.region, &start_url_hash).await
}

#[tauri::command]
pub async fn activate_role(
    app_state: State<'_, AppState>,
    account_id: String,
    account_name: String,
    role_name: String,
    custom_profile_name: Option<String>,
    set_as_default: Option<bool>,
) -> Result<ActiveProfile, LimenError> {
    let tokens = app_state.session_manager.ensure_active_token().await?;

    let active_profile = credentials::activate_role(
        &tokens.access_token,
        &tokens.region,
        &account_id,
        &account_name,
        &role_name,
        custom_profile_name,
        set_as_default.unwrap_or(false),
    )
    .await?;

    *app_state.active_profile.write().await = Some(active_profile.clone());
    Ok(active_profile)
}

#[tauri::command]
pub async fn get_active_profiles() -> Result<Vec<ActiveProfile>, LimenError> {
    let map = credentials::load_active_profiles().await?;
    let mut list: Vec<ActiveProfile> = map.into_values().collect();
    list.sort_by(|a, b| a.profile_name.cmp(&b.profile_name));
    Ok(list)
}

#[tauri::command]
pub async fn get_active_profile(
    app_state: State<'_, AppState>,
) -> Result<Option<ActiveProfile>, LimenError> {
    Ok(app_state.active_profile.read().await.clone())
}

#[tauri::command]
pub async fn deactivate_role(
    app_state: State<'_, AppState>,
    profile_name: String,
) -> Result<(), LimenError> {
    credentials::deactivate_role(&profile_name).await?;
    let mut guard = app_state.active_profile.write().await;
    if let Some(ref active) = *guard {
        if active.profile_name == profile_name {
            *guard = None;
        }
    }
    Ok(())
}
