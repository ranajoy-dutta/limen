use crate::error::LimenError;
use crate::models::auth::SessionState;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn begin_login(
    app_state: State<'_, AppState>,
    start_url: String,
    region: String,
) -> Result<SessionState, LimenError> {
    app_state.session_manager.begin_login(start_url, region).await
}

#[tauri::command]
pub async fn cancel_login(app_state: State<'_, AppState>) -> Result<(), LimenError> {
    app_state.session_manager.cancel_login().await
}

#[tauri::command]
pub async fn get_session_state(
    app_state: State<'_, AppState>,
) -> Result<SessionState, LimenError> {
    Ok(app_state.session_manager.get_state().await)
}

#[tauri::command]
pub async fn logout(app_state: State<'_, AppState>) -> Result<(), LimenError> {
    app_state.session_manager.logout().await
}

#[tauri::command]
pub async fn get_last_session() -> Result<Option<crate::core::storage::LastSessionInfo>, LimenError> {
    crate::core::storage::get_last_session().await
}
