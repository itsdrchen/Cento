use crate::db::DbState;
use crate::models::{DeepSeekBalance, DeepSeekSettings};
use crate::services::{settings_service, translate_service};
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<DbState>) -> Result<DeepSeekSettings, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    Ok(settings_service::get_settings(&conn))
}

#[tauri::command]
pub fn save_settings(state: State<DbState>, settings: DeepSeekSettings) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    settings_service::save_settings(&conn, &settings)
}

#[tauri::command]
pub async fn test_connection(settings: DeepSeekSettings) -> Result<bool, String> {
    if settings.api_key.is_empty() {
        return Err("请先填写 API Key".to_string());
    }

    translate_service::test_connection(&settings).await
}

/// Read the API key from the DB and query DeepSeek's official `/user/balance`
/// endpoint. The frontend renders the returned numbers in the settings panel.
#[tauri::command]
pub async fn fetch_deepseek_balance(
    state: State<'_, DbState>,
) -> Result<DeepSeekBalance, String> {
    let settings = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        settings_service::get_settings(&conn)
    };
    translate_service::fetch_balance(&settings).await
}
