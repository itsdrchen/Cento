use crate::db::DbState;
use crate::services::{cost_service, settings_service, translate_service};
use tauri::{AppHandle, Emitter, State};

fn emit_cost_updated(app: &AppHandle, state: &State<'_, DbState>) {
    let summary = {
        let Ok(conn) = state.conn.lock() else { return; };
        match cost_service::current_month_summary(&conn) {
            Ok(s) => s,
            Err(_) => return,
        }
    };
    let _ = app.emit("cost-updated", &summary);
}

#[tauri::command]
pub async fn translate_summary(
    app: AppHandle,
    state: State<'_, DbState>,
    entry_id: i64,
) -> Result<String, String> {
    let (summary, settings) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let cached: Option<String> = conn
            .query_row(
                "SELECT translated_text FROM translations
                 WHERE entry_id = ?1 AND field = 'summary' AND length(trim(translated_text)) > 0",
                [entry_id],
                |row| row.get(0),
            )
            .ok();
        if let Some(c) = cached {
            return Ok(c);
        }
        let s: Option<String> = conn
            .query_row(
                "SELECT summary FROM entries WHERE id = ?1",
                [entry_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        let summary = s.ok_or("该文章没有摘要")?;
        let metadata = crate::services::article_service::extract_rss_metadata(Some(&summary));
        if metadata.is_metadata_only {
            return Err("该文章尚未获取到真正的 Abstract".to_string());
        }
        let settings = settings_service::get_settings(&conn);
        (summary, settings)
    };

    if settings.api_key.is_empty() {
        return Err("请先在设置中配置 API Key".to_string());
    }

    let output = translate_service::translate_text(&settings, &summary).await?;

    {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO translations (entry_id, field, original_text, translated_text, model)
             VALUES (?1, 'summary', ?2, ?3, ?4)",
            rusqlite::params![entry_id, &summary, &output.content, &settings.model],
        )
        .map_err(|e| format!("保存摘要翻译失败: {}", e))?;
        let _ = cost_service::record_usage(&conn, &settings.model, &output.usage);
    }
    emit_cost_updated(&app, &state);

    Ok(output.content)
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("无法打开链接: {}", e))?;
    Ok(())
}

/// Return the current month's aggregated token usage + computed CNY. Drives
/// the bottom-left cost meter. The frontend calls this on startup and the
/// pipeline emits `cost-updated` with the same payload after each successful
/// translation, so the meter stays live without polling.
#[tauri::command]
pub fn get_cost_summary(
    state: State<'_, DbState>,
) -> Result<crate::models::CostSummary, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    cost_service::current_month_summary(&conn)
}
