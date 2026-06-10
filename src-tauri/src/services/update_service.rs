// GitHub-based update checker.
//
// Hits `api.github.com/repos/<owner>/<repo>/releases/latest`, compares the
// release `tag_name` against `CARGO_PKG_VERSION`, and returns a structured
// payload the UI can render. No code-signed in-app updater — we just notify
// the user and point them at the GitHub release page.
//
// The repo coordinate is fixed at compile time. If a fork wants to point
// elsewhere, change `REPO_OWNER` / `REPO_NAME` and rebuild.

use crate::models::UpdateInfo;
use reqwest::Client;
use serde::Deserialize;
use tracing::warn;

const REPO_OWNER: &str = "itsdrchen";
const REPO_NAME: &str = "Cento";

fn api_url() -> String {
    format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        REPO_OWNER, REPO_NAME
    )
}

fn releases_page_url() -> String {
    format!("https://github.com/{}/{}/releases", REPO_OWNER, REPO_NAME)
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

pub async fn check() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    let client = Client::builder()
        .user_agent(format!(
            "Cento/{} (https://github.com/{}/{})",
            current_version, REPO_OWNER, REPO_NAME
        ))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("无法创建网络客户端: {}", e))?;

    let response = client
        .get(api_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("无法访问 GitHub: {}", e))?;

    let status = response.status();

    // 404 = repo has zero releases. Treat as "you're on the latest" instead
    // of an error so the UI shows a clean "已是最新版" line.
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
        // Rate limit returns 403 — give the user a friendly hint rather than
        // raw HTTP.
        if status.as_u16() == 403 {
            return Err("GitHub 接口请求频率过高，请稍后再试".to_string());
        }
        return Err(format!("GitHub 返回 {}", status.as_u16()));
    }

    let release: GhRelease = response
        .json()
        .await
        .map_err(|e| format!("解析 GitHub 响应失败: {}", e))?;

    let latest_version = release
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

    let asset_url = release
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(want_ext))
        .map(|a| a.browser_download_url.clone());

    Ok(UpdateInfo {
        current_version,
        latest_version,
        has_update,
        release_url: release.html_url,
        release_notes: release.body,
        asset_url,
    })
}

/// Loose semver-style compare: split on `.`, compare numerically per component.
/// Non-numeric suffixes (e.g. `0.2.0-beta.1`) are stripped before parsing so
/// `0.2.0-beta.1` is treated as `0.2.0` — good enough for the kind of tagging
/// Cento will use, and never reports a downgrade as an update.
fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .map(|p| {
                // strip pre-release/build suffix
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

    match check().await {
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
