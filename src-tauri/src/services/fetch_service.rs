use crate::models::{Feed, FeedFetchResult, FetchResult};
use crate::services::{article_service, feed_service, settings_service};
use rusqlite::Connection;
use std::sync::Mutex;
use tracing::{info, warn};

pub async fn fetch_all_feeds(conn_mutex: &Mutex<Connection>) -> Result<FetchResult, String> {
    let (feeds, settings) = {
        let conn = conn_mutex.lock().map_err(|e| e.to_string())?;
        let feeds = crate::services::feed_service::list_feeds(&conn)?;
        let s = settings_service::get_settings(&conn);
        (feeds, s)
    };

    // We allow fetching without an API key — only the translation pipeline
    // requires it. New entries land in the DB, pipeline will pick them up
    // once the user configures the key.
    let _ = &settings;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Cento/0.1")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let mut result = FetchResult {
        total_feeds: feeds.len(),
        new_entries: 0,
        translated_titles: 0,
        fetched_summaries: 0,
        translated_summaries: 0,
        errors: Vec::new(),
        feeds: Vec::with_capacity(feeds.len()),
    };

    for feed in &feeds {
        info!(feed_id = feed.id, url = %feed.url, "开始刷新订阅源");
        match process_feed(conn_mutex, &client, feed).await {
            Ok(entries) => {
                result.new_entries += entries;
                result.feeds.push(FeedFetchResult {
                    feed_id: feed.id,
                    feed_title: feed.title.clone().unwrap_or_else(|| feed.url.clone()),
                    new_entries: entries,
                });
                {
                    let conn = conn_mutex.lock().map_err(|e| e.to_string())?;
                    let _ = feed_service::mark_feed_fetched(&conn, feed.id);
                }
                info!(feed_id = feed.id, new_entries = entries, "订阅源刷新完成");
            }
            Err(e) => {
                warn!(feed_id = feed.id, error = %e, "订阅源刷新失败");
                result.errors.push(format!("{}: {}", feed.url, e));
            }
        }
    }

    Ok(result)
}

/// Fetch only feeds whose configured interval has elapsed since `last_fetched_at`.
/// Used by the background scheduler so we don't refresh every feed on every tick.
pub async fn fetch_due_feeds(conn_mutex: &Mutex<Connection>) -> Result<FetchResult, String> {
    let all_feeds = {
        let conn = conn_mutex.lock().map_err(|e| e.to_string())?;
        feed_service::list_feeds(&conn)?
    };

    let due: Vec<Feed> = all_feeds.into_iter().filter(is_feed_due).collect();

    let mut result = FetchResult {
        total_feeds: due.len(),
        new_entries: 0,
        translated_titles: 0,
        fetched_summaries: 0,
        translated_summaries: 0,
        errors: Vec::new(),
        feeds: Vec::with_capacity(due.len()),
    };

    if due.is_empty() {
        return Ok(result);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Cento/0.1")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    for feed in &due {
        info!(feed_id = feed.id, url = %feed.url, "调度刷新订阅源");
        match process_feed(conn_mutex, &client, feed).await {
            Ok(entries) => {
                result.new_entries += entries;
                result.feeds.push(FeedFetchResult {
                    feed_id: feed.id,
                    feed_title: feed.title.clone().unwrap_or_else(|| feed.url.clone()),
                    new_entries: entries,
                });
                {
                    let conn = conn_mutex.lock().map_err(|e| e.to_string())?;
                    let _ = feed_service::mark_feed_fetched(&conn, feed.id);
                }
                info!(feed_id = feed.id, new_entries = entries, "订阅源刷新完成");
            }
            Err(e) => {
                warn!(feed_id = feed.id, error = %e, "订阅源刷新失败");
                result.errors.push(format!("{}: {}", feed.url, e));
            }
        }
    }

    Ok(result)
}

fn is_feed_due(feed: &Feed) -> bool {
    if feed.refresh_interval == "manual" {
        return false;
    }
    let Some(window_secs) = interval_to_secs(&feed.refresh_interval) else {
        return false;
    };
    let Some(last) = feed.last_fetched_at.as_deref() else {
        return true; // never fetched → due
    };
    // SQLite `datetime('now')` stores "YYYY-MM-DD HH:MM:SS" in UTC.
    let Some(last_secs) = parse_sqlite_utc(last) else {
        return true;
    };
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    (now_secs - last_secs) >= window_secs as i64
}

fn interval_to_secs(s: &str) -> Option<u64> {
    Some(match s {
        "15m" => 15 * 60,
        "1h" => 60 * 60,
        "12h" => 12 * 60 * 60,
        "1d" => 24 * 60 * 60,
        "3d" => 3 * 24 * 60 * 60,
        "1w" => 7 * 24 * 60 * 60,
        _ => return None,
    })
}

fn parse_sqlite_utc(s: &str) -> Option<i64> {
    // Format: "YYYY-MM-DD HH:MM:SS"
    if s.len() < 19 {
        return None;
    }
    let year: i32 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: u32 = s.get(11..13)?.parse().ok()?;
    let min: u32 = s.get(14..16)?.parse().ok()?;
    let sec: u32 = s.get(17..19)?.parse().ok()?;
    Some(days_from_civil(year, month, day) * 86400
        + hour as i64 * 3600
        + min as i64 * 60
        + sec as i64)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    // Howard Hinnant's algorithm — inverse of civil_from_days.
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let m = m as i64;
    let d = d as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = ((153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1) as u64;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

async fn process_feed(
    conn_mutex: &Mutex<Connection>,
    client: &reqwest::Client,
    feed: &Feed,
) -> Result<usize, String> {
    let response = client
        .get(&feed.url)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    let parsed = feed_rs::parser::parse(&bytes[..]).map_err(|e| format!("RSS 解析失败: {}", e))?;
    info!(
        feed_id = feed.id,
        entry_count = parsed.entries.len(),
        "RSS 解析完成"
    );

    let feed_title = parsed.title.map(|t| t.content);

    {
        let conn = conn_mutex.lock().map_err(|e| e.to_string())?;
        if let Some(ref title) = feed_title {
            let _ = conn.execute(
                "UPDATE feeds
                 SET title = ?1
                 WHERE id = ?2 AND (title IS NULL OR trim(title) = '' OR title = url)",
                rusqlite::params![title, feed.id],
            );
        }
    }

    let mut new_entries = 0usize;

    for entry in &parsed.entries {
        let guid = entry.id.clone();

        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| "(无标题)".to_string());

        let link = entry
            .links
            .first()
            .map(|l| l.href.clone())
            .unwrap_or_default();

        let summary = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|t| t.content.clone()));
        let metadata = article_service::extract_rss_metadata(summary.as_deref());

        let author = if entry.authors.is_empty() {
            None
        } else {
            Some(
                entry
                    .authors
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };

        let published_at = entry.published.or(entry.updated).map(|d| d.to_rfc3339());

        let conn = conn_mutex.lock().map_err(|e| e.to_string())?;

        let inserted = conn
            .execute(
                "INSERT OR IGNORE INTO entries
                 (feed_id, guid, title, link, summary, summary_source, author, published_at, publication_date, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    feed.id,
                    &guid,
                    &title,
                    link,
                    summary.as_deref(),
                    summary.as_ref().map(|_| "rss"),
                    author,
                    published_at,
                    metadata.publication_date.as_deref(),
                    metadata.source.as_deref()
                ],
            )
            .map_err(|e| format!("入库失败: {}", e))?;

        if inserted > 0 {
            new_entries += 1;
            let entry_id = conn.last_insert_rowid();
            // Log an immutable "fetched" event so the stats survive feed deletion
            // or retention-based entry pruning. feed_title may be None on the very
            // first fetch of a brand-new feed (UPDATE above runs in a sibling lock
            // scope), in which case the snapshot stays NULL — the stats query
            // falls back to the live feeds table when present.
            let _ = conn.execute(
                "INSERT INTO reading_events (kind, feed_id, feed_title_snapshot, entry_id)
                 VALUES ('fetched', ?1, ?2, ?3)",
                rusqlite::params![feed.id, feed_title.as_deref(), entry_id],
            );
        } else {
            conn.execute(
                "UPDATE entries
                 SET summary = COALESCE(NULLIF(summary, ''), ?1),
                     summary_source = CASE
                         WHEN (summary IS NULL OR trim(summary) = '') AND ?1 IS NOT NULL THEN 'rss'
                         ELSE summary_source
                     END,
                     publication_date = COALESCE(NULLIF(publication_date, ''), ?2),
                     source = COALESCE(NULLIF(source, ''), ?3)
                 WHERE feed_id = ?4 AND guid = ?5",
                rusqlite::params![
                    summary.as_deref(),
                    metadata.publication_date.as_deref(),
                    metadata.source.as_deref(),
                    feed.id,
                    &guid
                ],
            )
            .map_err(|e| format!("更新文章摘要失败: {}", e))?;
        }
    }

    Ok(new_entries)
}
