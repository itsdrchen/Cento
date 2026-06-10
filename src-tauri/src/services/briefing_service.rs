// AI briefing service — generates weekly Chinese-language summaries of recent
// articles via DeepSeek. Surfaced through `commands::briefing_cmd`, driven by
// the "AI 简报" panel in the frontend (`loadBriefings` / `generateBriefingNow`).

use crate::db::DbState;
use crate::models::{Briefing, BriefingCounts, DeepSeekSettings};
use crate::services::{settings_service, translate_service};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

const BRIEFING_WINDOW_DAYS: i64 = 7;
const MAX_ARTICLES_IN_PROMPT: usize = 60;

/// Editorial guidance — the *how to write* part. Replaceable by the user via
/// the 「AI 简报 → Prompt」 textarea. Mirrors `DEFAULT_BRIEFING_PROMPT` in
/// main.js so what the user sees in the editor matches what's actually used
/// when they haven't customized it.
const DEFAULT_BRIEFING_GUIDANCE: &str = "你是一位资深的科技报道编辑，专长是把一周内的前沿学术文献整理成面向研究者的高质量中文综述。请阅读用户提供的文献（标题、来源期刊、摘要），把它们汇总成一份**结构清晰、信息密度高、可读性强**的中文文献简报，写作风格参考《Nature Briefing》《知社学术圈》等科技前沿报道。

## 整体结构

1. **开篇导语（2-3 句）**：概括本期主线 —— 哪些方向延续了上期的热度、出现了哪些值得关注的新动向、整体脉络是什么。简练有力，避免套话。

2. **按主题分组**：将文献按研究方向归类，例如「机器学习与预后建模」「生物标志物与诊断」「治疗策略与临床试验」「新机制与基础研究」「单细胞 / 空间转录组」等。每组 2-5 条 bullet，组数控制在 3-6 个。使用 `## ` Markdown 标题。

3. **每条 bullet 40-80 字**，必须包含：
   - 一句话核心发现
   - **关键数值**：AUC、HR、95% CI、样本量、p-value、效应量等具体指标（如原文有则必须保留）
   - **方法 / 创新点**：使用了什么方法、相比已有研究有什么突破
   - 在 bullet 末尾用 `[n]` 标注对应文献编号

4. **💡 重点关注**（一个 `### ` 小节）：选出本期最值得关注的 1-3 篇文献，每篇 100-150 字，按以下顺序展开：
   - 研究背景与目标（1 句）
   - 主要方法（1-2 句）
   - 关键结果（带具体数值，1-2 句）
   - 临床 / 学术意义（1 句）

5. **趋势与启发**（一个 `## ` 小节）：从本期文献中提炼出 1-3 个跨研究的趋势或启发，比如：
   - 多篇论文是否都指向某个新兴方向？
   - 某个方法学（如单细胞测序、扩散模型、多模态学习）是否被多个团队同时采用？
   - 对临床转化或下一步研究的启发是什么？
   每点 1-2 句，给出**具体**判断而不是泛泛而谈。

## 风格要求

- 专业但不晦涩的中文，技术名词保留英文（PD-L1、CTLA-4、ResNet、GPT-4 等）
- 数据具体到数字，论断必须有文献支撑
- 避免「重要」「突破性」「划时代」「革命性」等空泛词；用具体的数值和对比代替
- 不添加原文没有的信息或主观评价
- Markdown 格式：`##` 主题分组、`###` 重点关注与趋势启发、`-` bullet、`**加粗**` 突出关键词与数值
- 整体长度 600-1200 字（取决于文献数量）";

/// Output schema — **always** appended after the editorial guidance, even when
/// the user supplies a custom prompt. This guarantees the response parses into
/// `(title, lead_in, content)` regardless of what the user wrote.
const BRIEFING_OUTPUT_FORMAT: &str = "\n\n---\n\n返回 **严格的 JSON**（不要 Markdown 代码块包裹整个 JSON）：
{
  \"title\": \"本期主标题（不超过 25 字，体现核心主题，例如「单细胞测序加速肿瘤异质性研究」）\",
  \"lead_in\": \"导读段，1-2 句，不超过 50 字，简明扼要点出本期亮点\",
  \"content\": \"完整简报正文（Markdown 格式，包含开篇导语、`##` 主题分组、`### 💡 重点关注`、`## 趋势与启发`）\"
}";

#[derive(Deserialize)]
struct ParsedBriefing {
    title: String,
    lead_in: String,
    content: String,
}

pub fn list_briefings(conn: &Connection) -> Result<Vec<Briefing>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, period, title, lead_in, content, article_count, feed_count, generated_at
             FROM briefings
             ORDER BY generated_at DESC, id DESC",
        )
        .map_err(|e| format!("查询简报失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Briefing {
                id: row.get(0)?,
                period: row.get(1)?,
                title: row.get(2)?,
                lead_in: row.get(3)?,
                content: row.get(4)?,
                counts: BriefingCounts {
                    articles: row.get(5)?,
                    feeds: row.get(6)?,
                },
                generated_at: row.get(7)?,
            })
        })
        .map_err(|e| format!("查询简报失败: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Remove a briefing by id. No-op success when the id doesn't exist so the
/// frontend can call this idempotently after optimistic UI updates.
pub fn delete_briefing(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM briefings WHERE id = ?1", [id])
        .map_err(|e| format!("删除简报失败: {}", e))?;
    Ok(())
}

/// Generate a briefing from entries fetched in the last week.
/// Reads the DB → builds the prompt → calls DeepSeek → parses → saves → returns
/// the new briefing. The DB lock is dropped before each `.await`.
///
/// `custom_guidance` lets the frontend pass the user's edited prompt from
/// 「AI 简报 → Prompt」. If `None` or empty, the default guidance is used.
/// The output-format requirement is always appended regardless of what the
/// user wrote, so parsing stays bulletproof.
pub async fn generate_briefing(
    state: &DbState,
    custom_guidance: Option<String>,
) -> Result<Briefing, String> {
    let (entries, settings, period_label) = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let settings = settings_service::get_settings(&conn);
        if settings.api_key.is_empty() {
            return Err("请先在设置中配置 DeepSeek API Key".to_string());
        }
        let entries = collect_recent_entries(&conn)?;
        let label = build_period_label();
        (entries, settings, label)
    };

    if entries.is_empty() {
        return Err("最近 7 天内没有可用于简报的文章".to_string());
    }

    let feed_count = entries
        .iter()
        .map(|e| e.feed_id)
        .collect::<std::collections::HashSet<_>>()
        .len() as i64;
    let article_count = entries.len() as i64;

    let user_prompt = format_entries_for_prompt(&entries);
    let guidance = custom_guidance
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BRIEFING_GUIDANCE);
    let system_prompt = format!("{}{}", guidance, BRIEFING_OUTPUT_FORMAT);

    info!(
        articles = article_count,
        feeds = feed_count,
        custom_prompt = custom_guidance.is_some(),
        "生成简报：发送至 DeepSeek"
    );

    let raw = call_deepseek_for_briefing(&settings, &system_prompt, &user_prompt).await?;
    let parsed = parse_briefing_json(&raw)?;

    let id = {
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO briefings (period, title, lead_in, content, article_count, feed_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                &period_label,
                &parsed.title,
                &parsed.lead_in,
                &parsed.content,
                article_count,
                feed_count,
            ],
        )
        .map_err(|e| format!("保存简报失败: {}", e))?;
        conn.last_insert_rowid()
    };

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let row = conn
        .query_row(
            "SELECT id, period, title, lead_in, content, article_count, feed_count, generated_at
             FROM briefings WHERE id = ?1",
            [id],
            |row| {
                Ok(Briefing {
                    id: row.get(0)?,
                    period: row.get(1)?,
                    title: row.get(2)?,
                    lead_in: row.get(3)?,
                    content: row.get(4)?,
                    counts: BriefingCounts {
                        articles: row.get(5)?,
                        feeds: row.get(6)?,
                    },
                    generated_at: row.get(7)?,
                })
            },
        )
        .map_err(|e| format!("读取新简报失败: {}", e))?;
    Ok(row)
}

struct EntryForBriefing {
    feed_id: i64,
    title_zh: Option<String>,
    title_en: String,
    journal: Option<String>,
    summary_zh: Option<String>,
}

fn collect_recent_entries(conn: &Connection) -> Result<Vec<EntryForBriefing>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT e.feed_id,
                    e.title,
                    e.source,
                    t_title.translated_text,
                    t_summary.translated_text
             FROM entries e
             LEFT JOIN translations t_title
               ON t_title.entry_id = e.id AND t_title.field = 'title'
              AND length(trim(t_title.translated_text)) > 0
             LEFT JOIN translations t_summary
               ON t_summary.entry_id = e.id AND t_summary.field = 'summary'
              AND length(trim(t_summary.translated_text)) > 0
             WHERE e.fetched_at >= datetime('now', ?1)
             ORDER BY e.published_at DESC, e.fetched_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("查询近期文章失败: {}", e))?;

    let window = format!("-{} days", BRIEFING_WINDOW_DAYS);
    let rows = stmt
        .query_map(rusqlite::params![window, MAX_ARTICLES_IN_PROMPT as i64], |row| {
            Ok(EntryForBriefing {
                feed_id: row.get(0)?,
                title_en: row.get(1)?,
                journal: row.get(2)?,
                title_zh: row.get(3)?,
                summary_zh: row.get(4)?,
            })
        })
        .map_err(|e| format!("查询近期文章失败: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn format_entries_for_prompt(entries: &[EntryForBriefing]) -> String {
    let mut buf = String::with_capacity(entries.len() * 200);
    buf.push_str("以下是近 7 天抓取的文献（已按时间倒序）：\n\n");
    for (i, e) in entries.iter().enumerate() {
        let n = i + 1;
        let journal = e.journal.as_deref().unwrap_or("—");
        let title = e.title_zh.as_deref().unwrap_or(&e.title_en);
        let summary = e
            .summary_zh
            .as_deref()
            .unwrap_or("（暂无中文摘要）")
            .chars()
            .take(400)
            .collect::<String>();
        buf.push_str(&format!(
            "[{}] 《{}》 — {}\n   摘要：{}\n\n",
            n, journal, title, summary
        ));
    }
    buf
}

fn build_period_label() -> String {
    // Simple inclusive 7-day label using local time.
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let end = format_ymd(secs);
    let start = format_ymd(secs - (BRIEFING_WINDOW_DAYS - 1) * 86400);
    format!("{} → {}", start, end)
}

fn format_ymd(epoch_secs: i64) -> String {
    // Date math without external deps: convert to civil date via days-since-epoch.
    let days = epoch_secs.div_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

// Howard Hinnant's date algorithm — civil_from_days (epoch = 1970-01-01).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (y + if m <= 2 { 1 } else { 0 }) as i32;
    (y, m, d)
}

async fn call_deepseek_for_briefing(
    settings: &DeepSeekSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let url = format!(
        "{}/chat/completions",
        settings.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": settings.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
        "temperature": 0.4,
        "max_tokens": 3500,
        "response_format": {"type": "json_object"},
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", settings.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("DeepSeek 请求失败: {}", e))?;

    let status = resp.status();
    let response_body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 DeepSeek 响应失败: {}", e))?;

    if !status.is_success() {
        let msg = response_body["error"]["message"]
            .as_str()
            .unwrap_or("未知错误");
        return Err(format!("DeepSeek {} — {}", status.as_u16(), msg));
    }

    let content = response_body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "DeepSeek 响应缺少 content 字段".to_string())?
        .to_string();
    let _ = &translate_service::translate_text; // keep crate import path used
    Ok(content)
}

fn parse_briefing_json(raw: &str) -> Result<ParsedBriefing, String> {
    // Some models wrap JSON in ```json fences even when asked not to. Strip them.
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    match serde_json::from_str::<ParsedBriefing>(cleaned) {
        Ok(p) if !p.title.trim().is_empty() && !p.content.trim().is_empty() => Ok(p),
        Ok(_) => Err("简报 JSON 缺少必要字段".to_string()),
        Err(e) => {
            warn!(error = %e, "简报 JSON 解析失败");
            Err(format!("简报 JSON 解析失败: {}", e))
        }
    }
}
