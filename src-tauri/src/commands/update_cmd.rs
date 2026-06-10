use crate::db::DbState;
use crate::models::UpdateInfo;
use crate::services::update_service;
use tauri::State;

/// User-initiated update check (settings → 其他设置 → 检查更新).
#[tauri::command]
pub async fn check_for_update(state: State<'_, DbState>) -> Result<UpdateInfo, String> {
    let info = update_service::check().await?;
    // Stamp the time so the auto-checker's "7-day cadence" stays honest even
    // when the user manually triggers a check in between.
    if let Ok(conn) = state.conn.lock() {
        let _ = conn.execute(
            "INSERT INTO settings (key, value) VALUES ('update_last_checked', datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = datetime('now')",
            [],
        );
    }
    Ok(info)
}

/// Read-only: app version, baked in at compile time from Cargo.toml.
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Read the user's auto-check preference and the timestamp of the last
/// successful check (RFC3339 / SQLite `datetime('now')` format).
#[tauri::command]
pub fn get_update_prefs(state: State<'_, DbState>) -> Result<UpdatePrefs, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let enabled = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'update_check_enabled'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(|v| v != "0")
        // Auto-check is opt-OUT, not opt-in: most users want the heads-up.
        .unwrap_or(true);
    let last = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'update_last_checked'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();
    Ok(UpdatePrefs {
        auto_check_enabled: enabled,
        last_checked_at: last,
    })
}

#[tauri::command]
pub fn set_update_auto_check(state: State<'_, DbState>, enabled: bool) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let value = if enabled { "1" } else { "0" };
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('update_check_enabled', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [value],
    )
    .map_err(|e| format!("保存更新检查偏好失败: {}", e))?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdatePrefs {
    pub auto_check_enabled: bool,
    pub last_checked_at: Option<String>,
}
