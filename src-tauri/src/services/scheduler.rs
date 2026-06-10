// Background refresh scheduler.
//
// Spawned once from `lib.rs` on app startup. Ticks every `TICK_SECS` seconds
// (independent of the webview, so it keeps running when the window is hidden
// or closed-to-tray). On each tick:
//   1. Asks `fetch_service::fetch_due_feeds` to refresh anything whose
//      configured per-feed interval has elapsed since its last fetch.
//   2. For each feed with `notify=1` that picked up new entries, fires a
//      native banner notification via `tauri-plugin-notification`.
//   3. Kicks the translation pipeline so the new entries get translated.
//
// One in-flight tick at a time: an `AtomicBool` guards against overlapping
// runs (which would also cause UNIQUE-constraint churn in the DB).

use crate::db::DbState;
use crate::models::FetchResult;
use crate::services::{fetch_service, notify, translation_pipeline, update_service};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

const TICK_SECS: u64 = 60;
const STARTUP_DELAY_SECS: u64 = 5;

/// How long to wait before the first update check after startup. Gives the
/// app a moment to settle (DB init, scheduler tick) and avoids piling network
/// I/O on top of the cold-start window.
const UPDATE_STARTUP_DELAY_SECS: u64 = 30;
/// We wake up periodically to *consider* running an update check; the actual
/// "do you need to check?" decision is gated on `update_last_checked` in the
/// settings table so the cadence survives app restarts.
const UPDATE_REEVAL_SECS: u64 = 60 * 60 * 6; // 6 hours
/// Effective minimum gap between two update checks.
const UPDATE_INTERVAL_SECS: i64 = 60 * 60 * 24 * 7; // 7 days

pub fn start(app: AppHandle) {
    let running = Arc::new(AtomicBool::new(false));
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(STARTUP_DELAY_SECS)).await;
        loop {
            tick(&app, &running).await;
            tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
        }
    });
}

async fn tick(app: &AppHandle, running: &Arc<AtomicBool>) {
    if running.swap(true, Ordering::SeqCst) {
        // A previous tick is still in progress; skip this one.
        return;
    }
    let state = app.state::<DbState>();
    let result = fetch_service::fetch_due_feeds(&state.conn).await;
    match result {
        Ok(r) if r.total_feeds == 0 => {
            // Nothing was due — quiet tick, no events.
        }
        Ok(r) => {
            info!(
                total = r.total_feeds,
                new = r.new_entries,
                errors = r.errors.len(),
                "调度刷新完成"
            );
            dispatch_notifications(app, &r);
            // Tell the UI to reload entries so it stays in sync.
            let _ = app.emit("scheduler-refreshed", &r);
            // Translate any new entries.
            translation_pipeline::spawn(app.clone());
        }
        Err(e) => {
            warn!(error = %e, "调度刷新失败");
        }
    }
    running.store(false, Ordering::SeqCst);
}

/// Fire one banner per feed that (a) has `notify=1` and (b) picked up new
/// entries. Called by both the scheduler tick and the manual refresh command,
/// so a user sees the same notifications regardless of which path triggered
/// the fetch.
pub fn dispatch_notifications(app: &AppHandle, result: &FetchResult) {
    if result.feeds.is_empty() {
        return;
    }
    // Build a feed-id → notify lookup from the DB so we don't trust stale
    // localStorage state.
    let state = app.state::<DbState>();
    let notify_ids: std::collections::HashSet<i64> = {
        let Ok(conn) = state.conn.lock() else {
            warn!("无法锁定数据库以查询通知偏好");
            return;
        };
        let Ok(mut stmt) = conn.prepare("SELECT id FROM feeds WHERE notify = 1") else {
            warn!("无法准备通知偏好查询");
            return;
        };
        stmt.query_map([], |row| row.get::<_, i64>(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    let candidates: Vec<_> = result
        .feeds
        .iter()
        .filter(|f| f.new_entries > 0 && notify_ids.contains(&f.feed_id))
        .collect();

    info!(
        notify_feeds = notify_ids.len(),
        candidate_count = candidates.len(),
        "准备发送通知"
    );

    // If we have notify-enabled feeds but zero candidates, log per-feed
    // diagnostics so the user can see *why* (no new articles vs. bell off).
    // This is the most common confusion: scheduler ran, nothing was new,
    // therefore nothing to notify about.
    if candidates.is_empty() {
        for f in &result.feeds {
            let bell_on = notify_ids.contains(&f.feed_id);
            info!(
                feed_id = f.feed_id,
                feed = %f.feed_title,
                new_entries = f.new_entries,
                bell_on,
                "未发送通知的原因"
            );
        }
        return;
    }

    for f in candidates {
        let body = format!("{} 已更新 {} 篇新文章", f.feed_title, f.new_entries);
        match notify::show(app, "Cento", &body) {
            Ok(_) => info!(feed_id = f.feed_id, "已发送通知"),
            Err(e) => warn!(error = %e, "发送通知失败"),
        }
    }
}

// ── Weekly GitHub update checker ──────────────────────────────────────────
//
// Lives in its own background task with a long sleep cycle. Persistence
// model: the last successful check time is stored in `settings` so the 7-day
// cadence survives across app restarts (vs. a naive `sleep(7 days)` that
// resets on every launch).

/// Start the background updater. Idempotent on the DB schema — `settings`
/// always exists. Reads `update_check_enabled` (default true) every cycle so
/// the user's preference takes effect without needing a restart.
pub fn start_update_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Let the app finish warming up before the first network hit.
        tokio::time::sleep(Duration::from_secs(UPDATE_STARTUP_DELAY_SECS)).await;
        loop {
            if should_check_now(&app) {
                update_service::check_and_notify_if_update(&app).await;
                mark_checked(&app);
            }
            tokio::time::sleep(Duration::from_secs(UPDATE_REEVAL_SECS)).await;
        }
    });
}

fn should_check_now(app: &AppHandle) -> bool {
    let state = app.state::<DbState>();
    let Ok(conn) = state.conn.lock() else {
        return false;
    };
    // Bail if the user disabled auto-check.
    let enabled: bool = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'update_check_enabled'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(|v| v != "0")
        .unwrap_or(true);
    if !enabled {
        return false;
    }
    // Pull the recorded last-check timestamp and compare to "now" in SQL —
    // simpler and more correct than juggling chrono just for one diff.
    let elapsed: Option<i64> = conn
        .query_row(
            "SELECT CAST((julianday('now') -
                          julianday(COALESCE(
                              (SELECT value FROM settings WHERE key='update_last_checked'),
                              '1970-01-01'
                          ))) * 86400 AS INTEGER)",
            [],
            |row| row.get(0),
        )
        .ok();
    elapsed.map(|s| s >= UPDATE_INTERVAL_SECS).unwrap_or(true)
}

fn mark_checked(app: &AppHandle) {
    let state = app.state::<DbState>();
    let Ok(conn) = state.conn.lock() else { return };
    let _ = conn.execute(
        "INSERT INTO settings (key, value) VALUES ('update_last_checked', datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = datetime('now')",
        [],
    );
}
