use crate::models::{Entry, ReadingStats};
use crate::services::article_service;
use rusqlite::Connection;

pub fn list_entries(conn: &Connection, feed_id: Option<i64>) -> Result<Vec<Entry>, String> {
    let retention_days: i64 = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'read_retention_days'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let retention_clause = if retention_days > 0 {
        format!(
            "AND (e.is_read = 0 OR e.read_at IS NULL OR e.read_at > datetime('now', '-{} days'))",
            retention_days
        )
    } else {
        String::new()
    };

    let select = "SELECT e.id, e.feed_id, e.guid, e.title, e.link, e.summary, e.summary_source, e.author,
            e.published_at, e.publication_date, e.source, e.fetched_at, e.is_read, e.read_at,
            t_title.translated_text,
            t_summary.translated_text,
            e.affiliation
     FROM entries e
     LEFT JOIN translations t_title ON t_title.entry_id = e.id AND t_title.field = 'title' AND length(trim(t_title.translated_text)) > 0
     LEFT JOIN translations t_summary ON t_summary.entry_id = e.id AND t_summary.field = 'summary' AND length(trim(t_summary.translated_text)) > 0";

    let sql = if feed_id.is_some() {
        format!(
            "{} WHERE e.feed_id = ?1 {} ORDER BY e.published_at DESC, e.fetched_at DESC",
            select, retention_clause
        )
    } else {
        format!(
            "{} WHERE 1=1 {} ORDER BY e.published_at DESC, e.fetched_at DESC LIMIT 200",
            select, retention_clause
        )
    };

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("查询失败: {}", e))?;

    let entries = if let Some(fid) = feed_id {
        stmt.query_map([fid], map_entry)
            .map_err(|e| format!("查询失败: {}", e))?
            .filter_map(|r| r.ok())
            .collect()
    } else {
        stmt.query_map([], map_entry)
            .map_err(|e| format!("查询失败: {}", e))?
            .filter_map(|r| r.ok())
            .collect()
    };

    Ok(entries)
}

fn map_entry(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
    let summary: Option<String> = row.get(5)?;
    let metadata = article_service::extract_rss_metadata(summary.as_deref());
    let publication_date: Option<String> = row.get(9)?;
    let source: Option<String> = row.get(10)?;
    let affiliation_raw: Option<String> = row.get(16)?;
    let affiliation = affiliation_raw.map(|s| article_service::dedupe_repeated(&s));

    Ok(Entry {
        id: row.get(0)?,
        feed_id: row.get(1)?,
        guid: row.get(2)?,
        title: row.get(3)?,
        link: row.get(4)?,
        summary: if metadata.is_metadata_only {
            None
        } else {
            summary
        },
        summary_source: row.get(6)?,
        author: row.get(7)?,
        published_at: row.get(8)?,
        publication_date: publication_date.or(metadata.publication_date),
        source: source.or(metadata.source),
        affiliation,
        fetched_at: row.get(11)?,
        is_read: row.get(12)?,
        read_at: row.get(13)?,
        title_translated: row.get(14)?,
        summary_translated: row.get(15)?,
    })
}

/// Compute reading stats from the immutable `reading_events` log. Decoupled
/// from `entries` so feed deletion and retention-based pruning never erase a
/// user's historical stats.
pub fn reading_stats(conn: &Connection) -> Result<ReadingStats, String> {
    let total_entries: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM reading_events WHERE kind = 'fetched'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("统计抓取数失败: {}", e))?;

    let total_read: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM reading_events WHERE kind = 'read'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("统计已读数失败: {}", e))?;

    let mut day_stmt = conn
        .prepare(
            "SELECT date(occurred_at) AS day, COUNT(*)
             FROM reading_events
             WHERE kind = 'read' AND occurred_at IS NOT NULL
             GROUP BY day
             ORDER BY day",
        )
        .map_err(|e| format!("统计每日阅读失败: {}", e))?;
    let day_counts: Vec<(String, i64)> = day_stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
        .map_err(|e| format!("统计每日阅读失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    // Per-feed read counts. The snapshot title is the latest-known feed name
    // captured at event time, so deleted feeds still surface a recognizable
    // label in the top-5 ranking.
    let mut feed_stmt = conn
        .prepare(
            "SELECT
                feed_id,
                (SELECT feed_title_snapshot
                   FROM reading_events r2
                  WHERE r2.feed_id = r.feed_id AND r2.feed_title_snapshot IS NOT NULL
                  ORDER BY r2.occurred_at DESC LIMIT 1) AS title_snapshot,
                COUNT(*)
             FROM reading_events r
             WHERE kind = 'read' AND feed_id IS NOT NULL
             GROUP BY feed_id
             ORDER BY COUNT(*) DESC",
        )
        .map_err(|e| format!("统计订阅源阅读失败: {}", e))?;
    let feed_read_counts: Vec<(i64, Option<String>, i64)> = feed_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("统计订阅源阅读失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(ReadingStats {
        total_entries,
        total_read,
        day_counts,
        feed_read_counts,
    })
}

pub fn set_entry_read(conn: &Connection, entry_id: i64, is_read: bool) -> Result<(), String> {
    // Only the 0→1 transition produces a `read` event — marking an already-read
    // article doesn't double-count, and unmarking never retracts the historical
    // fact that it was once read.
    if is_read {
        let affected = conn
            .execute(
                "UPDATE entries
                 SET is_read = 1,
                     read_at = COALESCE(read_at, datetime('now'))
                 WHERE id = ?1 AND is_read = 0",
                rusqlite::params![entry_id],
            )
            .map_err(|e| format!("更新已读状态失败: {}", e))?;

        if affected > 0 {
            let _ = conn.execute(
                "INSERT INTO reading_events (kind, feed_id, feed_title_snapshot, entry_id)
                 SELECT 'read', e.feed_id, f.title, e.id
                 FROM entries e LEFT JOIN feeds f ON f.id = e.feed_id
                 WHERE e.id = ?1",
                rusqlite::params![entry_id],
            );
        }
    } else {
        conn.execute(
            "UPDATE entries
             SET is_read = 0, read_at = NULL
             WHERE id = ?1",
            rusqlite::params![entry_id],
        )
        .map_err(|e| format!("更新已读状态失败: {}", e))?;
    }
    Ok(())
}
