use crate::db::DbState;
use crate::models::FetchResult;
use crate::services::{fetch_service, scheduler, translation_pipeline};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn fetch_all_feeds(
    app: AppHandle,
    state: State<'_, DbState>,
) -> Result<FetchResult, String> {
    let result = fetch_service::fetch_all_feeds(&state.conn).await?;
    // Mirror the scheduler's behavior: fire banner notifications for any feed
    // that opted in and picked up new entries. Manual refresh and background
    // refresh end up looking identical to the user.
    scheduler::dispatch_notifications(&app, &result);
    // Kick off translation in the background so the UI can render new
    // entries immediately while translations stream in via events.
    translation_pipeline::spawn(app);
    Ok(result)
}

/// Manual trigger — also fired on app startup so pre-existing entries with
/// missing translations get backfilled the first time we run with this
/// feature. Idempotent: the pipeline filters out anything already translated.
#[tauri::command]
pub fn start_translation_pipeline(app: AppHandle) -> Result<(), String> {
    translation_pipeline::spawn(app);
    Ok(())
}
