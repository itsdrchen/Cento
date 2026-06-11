// Update checker.
//
// Reads a JSON manifest from
//   https://github.com/<owner>/<repo>/releases/latest/download/latest.json
// which GitHub serves as a CDN-backed stable redirect to the `latest.json`
// asset attached to the most recent published (non-prerelease) release.
// The manifest is produced by `.github/workflows/publish-manifest.yml`
// whenever a release is published.
//
// Why this URL instead of api.github.com:
//   The anonymous REST API limits to 60 requests/hour/IP. Behind shared
//   egress (CGNAT, corporate NAT, VPN, mobile carrier — common in China)
//   that budget gets split across many users, so a non-trivial fraction
//   of update checks would fail with 403 "rate limit exceeded". The
//   `releases/latest/download/` redirect has no such per-IP cap, so the
//   check stays reliable no matter how the user's network is configured.
//
// Manifest may be missing or stale (CDN propagation, brand-new release
// before the workflow finishes, GitHub outage). We cache the last good
// result in the `settings` table and fall back to it on any failure, so
// the UI never shows a raw error to the user under normal conditions.

use crate::db::DbState;
use crate::models::UpdateInfo;
use reqwest::Client;
use rusqlite::Connection;
use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tracing::warn;

const REPO_OWNER: &str = "itsdrchen";
const REPO_NAME: &str = "Cento";

const CACHE_KEY: &str = "update_release_cache";

fn manifest_url() -> String {
    format!(
        "https://github.com/{}/{}/releases/latest/download/latest.json",
        REPO_OWNER, REPO_NAME
    )
}

fn releases_page_url() -> String {
    format!("https://github.com/{}/{}/releases", REPO_OWNER, REPO_NAME)
}

#[derive(Deserialize)]
struct ReleaseManifest {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    #[serde(default)]
    assets: Vec<ManifestAsset>,
}

#[derive(Deserialize)]
struct ManifestAsset {
    name: String,
    browser_download_url: String,
}

pub async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    let client = Client::builder()
        .user_agent(format!(
            "Cento/{} (https://github.com/{}/{})",
            current_version, REPO_OWNER, REPO_NAME
        ))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("无法创建网络客户端: {}", e))?;

    let response = match client.get(manifest_url()).send().await {
        Ok(r) => r,
        Err(e) => {
            // Network failed — surface the cache if we have one.
            if let Some(info) = read_cache(app, &current_version) {
                warn!(error = %e, "更新检查网络失败，使用缓存结果");
                return Ok(info);
            }
            return Err(format!("无法访问 GitHub: {}", e));
        }
    };

    let status = response.status();

    // 404 = no published release with a `latest.json` asset yet. Treat as
    // "you're on the latest" so first-run users see a clean state instead
    // of an error.
    if status.as_u16() == 404 {
        return Ok(UpdateInfo {
            current_version: current_version.clone(),
            latest_version: current_version,
            has_update: false,
            release_url: releases_page_url(),
            release_notes: None,
            asset_url: None,
        });
    }

    if !status.is_success() {
        if let Some(info) = read_cache(app, &current_version) {
            warn!("GitHub 返回 {}，使用缓存结果", status.as_u16());
            return Ok(info);
        }
        return Err(format!("GitHub 返回 {}", status.as_u16()));
    }

    let manifest: ReleaseManifest = match response.json().await {
        Ok(m) => m,
        Err(e) => {
            if let Some(info) = read_cache(app, &current_version) {
                warn!(error = %e, "解析更新清单失败，使用缓存结果");
                return Ok(info);
            }
            return Err(format!("解析更新清单失败: {}", e));
        }
    };

    let latest_version = manifest
        .tag_name
        .trim()
        .trim_start_matches('v')
        .to_string();
    let has_update = is_newer(&latest_version, &current_version);

    // Pick the installer matching the running OS. Falls back to None if the
    // release doesn't have an asset for this platform — UI then shows the
    // GitHub release page link instead of a direct download.
    #[cfg(target_os = "macos")]
    let want_ext = ".dmg";
    #[cfg(target_os = "windows")]
    let want_ext = ".msi";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let want_ext = ".AppImage";

    let asset_url = manifest
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(want_ext))
        .map(|a| a.browser_download_url.clone());

    let info = UpdateInfo {
        current_version,
        latest_version,
        has_update,
        release_url: manifest.html_url,
        release_notes: manifest.body,
        asset_url,
    };

    write_cache(app, &info);

    Ok(info)
}

/// Loose semver-style compare: split on `.`, compare numerically per component.
/// Non-numeric suffixes (e.g. `0.2.0-beta.1`) are stripped before parsing so
/// `0.2.0-beta.1` is treated as `0.2.0` — good enough for the kind of tagging
/// Cento will use, and never reports a downgrade as an update.
fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .map(|p| {
                let cut = p
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(p.len());
                p[..cut].parse().unwrap_or(0)
            })
            .collect()
    };
    let a = parse(candidate);
    let b = parse(current);
    let n = a.len().max(b.len());
    for i in 0..n {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        if av != bv {
            return av > bv;
        }
    }
    false
}

// ── Cache helpers ───────────────────────────────────────

fn read_cache(app: &AppHandle, current_version: &str) -> Option<UpdateInfo> {
    let state = app.state::<DbState>();
    let conn = state.conn.lock().ok()?;
    let raw = read_setting(&conn, CACHE_KEY)?;
    let mut info: UpdateInfo = serde_json::from_str(&raw).ok()?;
    // `has_update` was computed against the version that was current when
    // the cache was written. If the app has been upgraded since, that
    // decision is stale — recompute.
    info.current_version = current_version.to_string();
    info.has_update = is_newer(&info.latest_version, current_version);
    Some(info)
}

fn write_cache(app: &AppHandle, info: &UpdateInfo) {
    let state = app.state::<DbState>();
    let Ok(conn) = state.conn.lock() else { return };
    if let Ok(json) = serde_json::to_string(info) {
        let _ = write_setting(&conn, CACHE_KEY, &json);
    }
}

fn read_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn write_setting(conn: &Connection, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        [key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn detects_newer_minor() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
    }
    #[test]
    fn rejects_same_or_older() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }
    #[test]
    fn handles_prerelease_suffix() {
        assert!(!is_newer("0.1.0-beta.1", "0.1.0"));
        assert!(is_newer("0.2.0-beta.1", "0.1.0"));
    }
}

/// Run a check and, if an update is found, fire a system notification.
/// Caller is responsible for deciding *when* to run this; see
/// `scheduler::start_update_checker`.
pub async fn check_and_notify_if_update(app: &tauri::AppHandle) {
    use tauri::Emitter;

    match check(app).await {
        Ok(info) => {
            // Always emit to the frontend so the about card can refresh its
            // "last checked" timestamp and version line.
            let _ = app.emit("update-checked", &info);
            if info.has_update {
                let body = format!(
                    "Cento {} 已发布，当前版本 {}。前往设置 → 其他设置查看下载。",
                    info.latest_version, info.current_version
                );
                if let Err(e) = crate::services::notify::show(app, "Cento 有新版本", &body) {
                    warn!(error = %e, "发送更新通知失败");
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "更新检查失败");
        }
    }
}
