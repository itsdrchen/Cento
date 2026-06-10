use crate::db::DbState;
use crate::services::{pubmed_service, settings_service};
use tauri::State;

#[tauri::command]
pub async fn build_pubmed_rss_url(query: String, limit: u32) -> Result<String, String> {
    pubmed_service::build_rss_url(&query, limit).await
}

#[tauri::command]
pub async fn natural_to_pubmed_query(
    state: State<'_, DbState>,
    text: String,
) -> Result<String, String> {
    let settings = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        settings_service::get_settings(&conn)
    };

    if settings.api_key.is_empty() {
        return Err("请先在设置中配置 DeepSeek API Key".to_string());
    }

    pubmed_service::natural_language_to_query(&settings, &text).await
}
