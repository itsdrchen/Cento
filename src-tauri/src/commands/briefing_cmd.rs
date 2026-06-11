// Tauri command wrappers for the AI briefing service.
//
// `briefing_service` was implemented earlier but not exposed; the frontend
// already calls `invoke('list_briefings')` / `invoke('generate_briefing')`
// (see `loadBriefings` and `generateBriefingNow` in src/main.js). These
// thin wrappers wire it up.

use crate::db::DbState;
use crate::models::Briefing;
use crate::services::{briefing_service, notify};
use tauri::{AppHandle, State};
use tracing::warn;

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
    app: AppHandle,
    state: State<'_, DbState>,
    custom_prompt: Option<String>,
) -> Result<Briefing, String> {
    let briefing = briefing_service::generate_briefing(state.inner(), custom_prompt).await?;

    // Fire a system banner so the user notices when the briefing is generated
    // in the background by the frontend auto-scheduler (`briefingSchedulerTick`
    // in main.js) — that path can fire while the window is hidden or behind
    // other apps. We notify on manual runs too for consistency with the
    // fetch notification pattern in `scheduler.rs`; the user already sees an
    // in-app status pill but a banner is harmless and confirms the run.
    //
    // Prefer the lead-in (1-2 sentence editorial summary) since it's the
    // most informative one-line preview. Fall back to the title when the
    // model didn't produce one.
    let body = if briefing.lead_in.trim().is_empty() {
        briefing.title.clone()
    } else {
        briefing.lead_in.clone()
    };
    if let Err(e) = notify::show(&app, "Cento AI 简报已生成", &body) {
        warn!(error = %e, "发送简报通知失败");
    }

    Ok(briefing)
}
