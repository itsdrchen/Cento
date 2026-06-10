use crate::models::DeepSeekSettings;
use rusqlite::Connection;

const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
const DEFAULT_MODEL: &str = "deepseek-v4-flash";
const DEFAULT_PROMPT: &str = "你是一个专业的学术与新闻翻译助手。你的任务是将英文 RSS 标题和摘要翻译成简洁、准确的中文。\n\n翻译规则：\n1. 准确优先：专业术语必须使用学术界通用的中文译法。如果某个术语没有公认译法，保留英文原文并用括号简要解释。\n2. 人名不翻译：所有人名保留英文原文，不做音译。\n3. 机构与期刊名：优先使用官方中文名（如 \"Nature\" → \"《自然》\"）。没有官方中文名则保留英文。\n4. 简洁：标题翻译控制在 30 个汉字以内。摘要翻译保留所有关键信息，但删除冗余的修饰语、套话和背景铺垫。\n5. 语体风格：学术内容使用正式学术语言；新闻内容使用标准新闻语言。不添加任何原文中没有的意见、评价或补充说明。\n6. HTML 标签：如果原文包含 HTML 标签（如 <p>、<a>、<em>），移除它们，只翻译纯文本内容。\n7. 仅返回翻译结果：不要在回复中包含原文、解释、备注或任何其他内容。只输出翻译后的中文文本。";

fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .ok()
}

fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        [key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_settings(conn: &Connection) -> DeepSeekSettings {
    DeepSeekSettings {
        api_key: get_setting(conn, "api_key").unwrap_or_default(),
        base_url: get_setting(conn, "base_url").unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        model: get_setting(conn, "model").unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        system_prompt: get_setting(conn, "system_prompt")
            .unwrap_or_else(|| DEFAULT_PROMPT.to_string()),
        read_retention_days: get_setting(conn, "read_retention_days")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    }
}

pub fn save_settings(conn: &Connection, settings: &DeepSeekSettings) -> Result<(), String> {
    set_setting(conn, "api_key", &settings.api_key)?;
    set_setting(conn, "base_url", &settings.base_url)?;
    set_setting(conn, "model", &settings.model)?;
    set_setting(conn, "system_prompt", &settings.system_prompt)?;
    set_setting(conn, "read_retention_days", &settings.read_retention_days.to_string())?;
    Ok(())
}
