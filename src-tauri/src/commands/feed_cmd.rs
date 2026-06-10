use crate::db::DbState;
use crate::models::Feed;
use crate::services::feed_service;
use tauri::State;

#[tauri::command]
pub fn add_feed(state: State<DbState>, url: String) -> Result<Feed, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    feed_service::add_feed(&conn, &url)
}

#[tauri::command]
pub fn list_feeds(state: State<DbState>) -> Result<Vec<Feed>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    feed_service::list_feeds(&conn)
}

#[tauri::command]
pub fn delete_feed(state: State<DbState>, id: i64) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    feed_service::delete_feed(&conn, id)
}

#[tauri::command]
pub fn rename_feed(state: State<DbState>, id: i64, name: String) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    feed_service::rename_feed(&conn, id, &name)
}

#[tauri::command]
pub fn set_feed_interval(state: State<DbState>, id: i64, interval: String) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    feed_service::set_feed_interval(&conn, id, &interval)
}

#[tauri::command]
pub fn set_feed_notify(state: State<DbState>, id: i64, notify: bool) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    feed_service::set_feed_notify(&conn, id, notify)
}
