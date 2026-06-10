// macOS notification helper.
//
// Why we don't just call `app.notification().builder().show()` everywhere:
// macOS's UNUserNotificationCenter (what `tauri-plugin-notification` is on top
// of) refuses to deliver banners from any binary that isn't inside a signed
// `.app/Contents/` bundle. `npm run tauri dev` runs the raw debug binary out
// of `target/debug/`, so the plugin's `show()` returns Ok but nothing ever
// reaches the screen — exactly the symptom we saw.
//
// Workaround: `osascript -e 'display notification …'` rides on Script
// Editor's permission, which is granted by default on macOS. Banners show
// up immediately, attributed to "Script Editor" in the footer instead of
// "Cento". Not perfect for production polish, but the only way to get a
// reliable banner in dev mode without bundling and re-signing every change.
//
// When the user ships a real `tauri build` bundle, we'll prefer the native
// plugin so the footer reads "Cento". Bundle detection: the executable
// path of a properly bundled macOS app lives under `Foo.app/Contents/MacOS/`.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tracing::warn;

const CENTO_BUNDLE_ID: &str = "io.github.itsdrchen.cento";

fn is_in_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.contains(".app/Contents/")))
        .unwrap_or(false)
}

/// Look up `terminal-notifier` once. Cached so we don't shell out to `which`
/// on every notification.
fn terminal_notifier_path() -> Option<&'static str> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let out = std::process::Command::new("which")
                .arg("terminal-notifier")
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let path = String::from_utf8(out.stdout).ok()?;
            let path = path.trim().to_string();
            (!path.is_empty()).then_some(path)
        })
        .as_deref()
}

fn show_via_terminal_notifier(bin: &str, title: &str, body: &str) -> Result<(), String> {
    // `-sender io.github.itsdrchen.cento` spoofs the bundle identifier so the banner
    // footer shows Cento's icon + name (the real Cento.app must exist on
    // this machine — the bundled production build registers it via
    // LaunchServices when launched at least once).
    let output = std::process::Command::new(bin)
        .args([
            "-title",
            title,
            "-message",
            body,
            "-sender",
            CENTO_BUNDLE_ID,
        ])
        .output()
        .map_err(|e| format!("启动 terminal-notifier 失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("terminal-notifier 失败: {}", stderr.trim()));
    }
    Ok(())
}

fn show_via_osascript(title: &str, body: &str) -> Result<(), String> {
    // AppleScript string literals are delimited by `"` and use `\"` for an
    // embedded quote. Backslashes also need escaping for the same reason.
    let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"display notification "{}" with title "{}""#,
        escape(body),
        escape(title)
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("启动 osascript 失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("osascript 失败: {}", stderr.trim()));
    }
    Ok(())
}

fn show_via_plugin(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| format!("发送通知失败: {}", e))
}

/// Show a system banner. Picks the right channel based on whether we're
/// running from a real `.app` bundle (use plugin → "Cento" attribution) or a
/// raw dev binary (use osascript → "Script Editor" attribution, but actually
/// visible).
pub fn show(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    if is_in_app_bundle() {
        // Production path. Try the plugin; if for some reason it fails (eg.
        // user denied permission), fall back to osascript so the user still
        // sees *something*.
        match show_via_plugin(app, title, body) {
            Ok(()) => Ok(()),
            Err(e) => {
                warn!(error = %e, "插件通知失败，回退到 osascript");
                show_via_osascript(title, body)
            }
        }
    } else {
        // Dev path. UNUserNotificationCenter silently drops banners from the
        // raw debug binary, so we have to spawn a helper that already has
        // notification privileges.
        //
        // Preference order:
        //   1. `terminal-notifier` (if installed) — supports `-sender`, so
        //      the banner footer reads "Cento" with our app icon. This is
        //      the only way to get proper attribution in dev without
        //      bundling.
        //   2. `osascript` — always available, but the banner is attributed
        //      to "Script Editor" with its icon.
        if let Some(bin) = terminal_notifier_path() {
            return show_via_terminal_notifier(bin, title, body);
        }
        show_via_osascript(title, body)
    }
}
