mod commands;
mod db;
mod models;
mod services;

use commands::{
    briefing_cmd, entry_cmd, feed_cmd, fetch_cmd, pubmed_cmd, settings_cmd, translate_cmd,
    tray_cmd, update_cmd,
};
use services::scheduler;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tracing_subscriber::{fmt, EnvFilter};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("cento=info,cento_lib=info")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("无法获取应用数据目录");

            let db_state = db::initialize(app_data_dir)
                .map_err(|e| Box::new(std::io::Error::other(e)))?;

            app.manage(db_state);

            // ── macOS menu-bar tray icon ──
            let show_item = MenuItemBuilder::with_id("tray-show", "打开 Cento").build(app)?;
            let quit_item = MenuItemBuilder::with_id("tray-quit", "退出").build(app)?;
            let tray_menu = MenuBuilder::new(app).items(&[&show_item, &quit_item]).build()?;

            // Use a dedicated monochrome silhouette as the menu-bar icon. The
            // square app icon, even with `icon_as_template`, produces a flat
            // square on the menu bar because macOS only looks at alpha for
            // template images and the full-color art has no alpha holes.
            // `tray-icon@2x.png` is a 44×44 PNG of just the "C" arc.
            //
            // `icon_as_template(true)` is a macOS-only concept (template
            // images automatically invert for dark menu bar). On Windows the
            // tray icon stays full-color in the system tray, so we skip it
            // there.
            let tray_builder = TrayIconBuilder::with_id("cento-tray")
                .icon(tauri::include_image!("icons/tray-icon@2x.png"));

            #[cfg(target_os = "macos")]
            let tray_builder = tray_builder.icon_as_template(true);

            let _ = tray_builder
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "tray-show" => focus_main_window(app),
                    "tray-quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        focus_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Hide-to-tray: intercept the red close button so the process keeps
            // running (and the scheduler keeps ticking) instead of quitting.
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            // Start the background refresh scheduler + the weekly update
            // checker. Both run independently on Tauri's async runtime.
            scheduler::start(app.handle().clone());
            scheduler::start_update_checker(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            briefing_cmd::list_briefings,
            briefing_cmd::generate_briefing,
            briefing_cmd::delete_briefing,
            entry_cmd::fetch_abstract,
            entry_cmd::fetch_affiliation,
            entry_cmd::get_reading_stats,
            entry_cmd::list_entries,
            entry_cmd::set_entry_read,
            feed_cmd::add_feed,
            feed_cmd::list_feeds,
            feed_cmd::delete_feed,
            feed_cmd::rename_feed,
            feed_cmd::set_feed_interval,
            feed_cmd::set_feed_notify,
            fetch_cmd::fetch_all_feeds,
            fetch_cmd::start_translation_pipeline,
            pubmed_cmd::build_pubmed_rss_url,
            pubmed_cmd::natural_to_pubmed_query,
            settings_cmd::get_settings,
            settings_cmd::save_settings,
            settings_cmd::test_connection,
            settings_cmd::fetch_deepseek_balance,
            translate_cmd::translate_summary,
            translate_cmd::open_url,
            translate_cmd::get_cost_summary,
            tray_cmd::update_tray_unread,
            tray_cmd::set_tray_visible,
            tray_cmd::send_test_notification,
            update_cmd::check_for_update,
            update_cmd::get_app_version,
            update_cmd::get_update_prefs,
            update_cmd::set_update_auto_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
