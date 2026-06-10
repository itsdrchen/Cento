// Tauri command wrappers for the AI briefing service.
//
// `briefing_service` was implemented earlier but not exposed; the frontend
// already calls `invoke('list_briefings')` / `invoke('generate_briefing')`
// (see `loadBriefings` and `generateBriefingNow` in src/main.js). These
// thin wrappers wire it up.

use crate::db::DbState;
use crate::models::Briefing;
use crate::services::briefing_service;
use tauri::State;

/// Read every previously-generated briefing from the DB. Cheap — pure SQL.
#[tauri::command]
pub fn list_briefings(state: State<'_, DbState>) -> Result<Vec<Briefing>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    briefing_service::list_briefings(&conn)
}

/// Delete a single briefing by id (right-click menu in the briefing list).
#[tauri::command]
pub fn delete_briefing(state: State<'_, DbState>, id: i64) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    briefing_service::delete_briefing(&conn, id)
}

/// Compose a fresh briefing from the last 7 days of articles via DeepSeek
/// and persist it. Async because it makes an HTTP call out to DeepSeek; the
/// frontend renders a spinner while it's in flight. Requires the user to
/// have configured an API key (the service surfaces a clear error otherwise).
///
/// `custom_prompt` is the editorial-guidance text from the user's "AI 简报 →
/// Prompt" editor (lives in localStorage on the frontend, passed through
/// here). If `None` or whitespace-only, the service uses its built-in
/// default — see `DEFAULT_BRIEFING_GUIDANCE` in `briefing_service`.
#[tauri::command]
pub async fn generate_briefing(
    state: State<'_, DbState>,
    custom_prompt: Option<String>,
) -> Result<Briefing, String> {
    briefing_service::generate_briefing(state.inner(), custom_prompt).await
}
