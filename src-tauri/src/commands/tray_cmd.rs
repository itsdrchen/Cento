use crate::services::notify;
use tauri::AppHandle;

const TRAY_ID: &str = "cento-tray";

#[tauri::command]
pub fn update_tray_unread(app: AppHandle, count: i64) -> Result<(), String> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(()); // Tray was disabled by the user, no-op.
    };
    // macOS shows the tray title as text next to the icon. Empty string hides
    // it (the icon stays); a number shows the unread badge inline.
    let title = if count > 0 { Some(count.to_string()) } else { None };
    tray.set_title(title.as_deref()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_tray_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    tray.set_visible(visible).map_err(|e| e.to_string())?;
    Ok(())
}

/// Send a test banner through exactly the same Rust-side notification path the
/// scheduler uses. Routing through Rust (rather than the frontend
/// `window.__TAURI__.notification` global) means: (1) we get the same OS-level
/// permission state as the production scheduler — so a working test really
/// proves background banners will appear, and (2) `withGlobalTauri` doesn't
/// need to expose the plugin's JS API for the button to work.
#[tauri::command]
pub fn send_test_notification(app: AppHandle) -> Result<(), String> {
    notify::show(
        &app,
        "Cento 测试通知",
        "如果你看到了这条通知，说明 macOS 系统通知已正常工作。",
    )
}
