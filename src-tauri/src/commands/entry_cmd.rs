use crate::db::DbState;
use crate::models::{Entry, ReadingStats};
use crate::services::{article_service, entry_service};
use tauri::State;
use tracing::{info, warn};

#[tauri::command]
pub fn list_entries(state: State<DbState>, feed_id: Option<i64>) -> Result<Vec<Entry>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    entry_service::list_entries(&conn, feed_id)
}

#[tauri::command]
pub fn set_entry_read(state: State<DbState>, entry_id: i64, is_read: bool) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    entry_service::set_entry_read(&conn, entry_id, is_read)
}

#[tauri::command]
pub fn get_reading_stats(state: State<DbState>) -> Result<ReadingStats, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    entry_service::reading_stats(&conn)
}

#[tauri::command]
pub async fn fetch_abstract(
    state: State<'_, DbState>,
    entry_id: i64,
) -> Result<Option<String>, String> {
    let (title, cached_summary) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let (title, summary): (String, Option<String>) = conn
            .query_row(
                "SELECT title, summary FROM entries WHERE id = ?1",
                [entry_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| format!("文章不存在: {}", e))?;
        (title, summary)
    };

    if let Some(summary) = cached_summary {
        let metadata = article_service::extract_rss_metadata(Some(&summary));
        if !metadata.is_metadata_only {
            return Ok(Some(summary));
        }
    }

    let abstract_result = article_service::fetch_abstract(&title).await?;

    if let Some(ref result) = abstract_result {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE entries SET summary = ?1, summary_source = ?2 WHERE id = ?3",
            rusqlite::params![&result.text, &result.source, entry_id],
        )
        .map_err(|e| format!("保存 Abstract 失败: {}", e))?;
    }

    Ok(abstract_result.map(|result| result.text))
}

#[tauri::command]
pub async fn fetch_affiliation(
    state: State<'_, DbState>,
    entry_id: i64,
) -> Result<Option<String>, String> {
    let (link, guid, title, summary, cached): (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    ) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT link, guid, title, summary, affiliation FROM entries WHERE id = ?1",
            [entry_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|e| format!("文章不存在: {}", e))?
    };

    if let Some(text) = cached.as_deref() {
        let cleaned = article_service::dedupe_repeated(text);
        if !cleaned.is_empty() {
            if cleaned != text {
                // Cached value from an older build had the doubled-text bug — repair it in place.
                let conn = state.conn.lock().map_err(|e| e.to_string())?;
                let _ = conn.execute(
                    "UPDATE entries SET affiliation = ?1 WHERE id = ?2",
                    rusqlite::params![&cleaned, entry_id],
                );
                info!(entry_id, "affiliation 缓存已去重");
            } else {
                info!(entry_id, "affiliation 命中缓存");
            }
            return Ok(Some(cleaned));
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Cento/0.1 (https://github.com/itsdrchen/Cento)")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let pmid = if let Some(p) = article_service::extract_pmid_from_link(&link) {
        info!(entry_id, pmid = %p, "PMID 来自 link");
        Some(p)
    } else if let Some(p) = article_service::extract_pmid_from_guid(&guid) {
        info!(entry_id, pmid = %p, "PMID 来自 guid");
        Some(p)
    } else if let Some(p) = summary
        .as_deref()
        .and_then(article_service::extract_pmid_from_text)
    {
        info!(entry_id, pmid = %p, "PMID 来自 summary");
        Some(p)
    } else {
        match article_service::find_pubmed_pmid_by_title(&client, &title).await {
            Ok(Some(p)) => {
                info!(entry_id, pmid = %p, "PMID 来自 title 搜索");
                Some(p)
            }
            Ok(None) => {
                warn!(entry_id, %link, %guid, %title, "无法定位 PMID");
                None
            }
            Err(e) => {
                warn!(entry_id, error = %e, "title 搜索 PMID 失败");
                None
            }
        }
    };

    let Some(pmid) = pmid else {
        return Ok(None);
    };

    let affiliation =
        article_service::fetch_pubmed_first_affiliation(&client, &pmid).await?;

    match affiliation.as_deref() {
        Some(text) => info!(entry_id, pmid = %pmid, chars = text.len(), "affiliation 已获取"),
        None => warn!(entry_id, pmid = %pmid, "PubMed XML 无 Affiliation 节点"),
    }

    if let Some(ref text) = affiliation {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE entries SET affiliation = ?1 WHERE id = ?2",
            rusqlite::params![text, entry_id],
        )
        .map_err(|e| format!("保存 affiliation 失败: {}", e))?;
    }

    Ok(affiliation)
}
