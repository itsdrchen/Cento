use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

fn schema_sql() -> &'static str {
    "
    CREATE TABLE IF NOT EXISTS feeds (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        url         TEXT NOT NULL UNIQUE,
        title       TEXT,
        description TEXT,
        created_at  TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS entries (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        feed_id      INTEGER NOT NULL,
        guid         TEXT NOT NULL,
        title        TEXT NOT NULL,
        link         TEXT NOT NULL,
        summary      TEXT,
        summary_source TEXT,
        author       TEXT,
        published_at TEXT,
        fetched_at   TEXT NOT NULL DEFAULT (datetime('now')),
        is_read      INTEGER NOT NULL DEFAULT 0,
        read_at      TEXT,
        UNIQUE(feed_id, guid),
        FOREIGN KEY (feed_id) REFERENCES feeds(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS translations (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        entry_id        INTEGER NOT NULL,
        field           TEXT NOT NULL CHECK(field IN ('title', 'summary')),
        original_text   TEXT NOT NULL,
        translated_text TEXT NOT NULL,
        model           TEXT,
        created_at      TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(entry_id, field),
        FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS cost_log (
        id                       INTEGER PRIMARY KEY AUTOINCREMENT,
        month                    TEXT NOT NULL,
        model                    TEXT NOT NULL,
        prompt_cache_hit_tokens  INTEGER NOT NULL DEFAULT 0,
        prompt_cache_miss_tokens INTEGER NOT NULL DEFAULT 0,
        completion_tokens        INTEGER NOT NULL DEFAULT 0,
        UNIQUE(month, model)
    );

    CREATE TABLE IF NOT EXISTS briefings (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        period        TEXT NOT NULL,
        title         TEXT NOT NULL,
        lead_in       TEXT NOT NULL,
        content       TEXT NOT NULL,
        article_count INTEGER NOT NULL,
        feed_count    INTEGER NOT NULL,
        generated_at  TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- Immutable append-only log driving reading stats. Deliberately NOT
    -- foreign-keyed to feeds/entries: deleting a feed (or pruning entries via
    -- read_retention_days) must NOT erase the historical record of what the
    -- user fetched and read.
    CREATE TABLE IF NOT EXISTS reading_events (
        id                  INTEGER PRIMARY KEY AUTOINCREMENT,
        kind                TEXT NOT NULL CHECK(kind IN ('fetched', 'read')),
        feed_id             INTEGER,
        feed_title_snapshot TEXT,
        entry_id            INTEGER,
        occurred_at         TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_reading_events_kind_date
        ON reading_events(kind, occurred_at);
    CREATE INDEX IF NOT EXISTS idx_reading_events_kind_feed
        ON reading_events(kind, feed_id);

    PRAGMA foreign_keys = ON;

    INSERT OR IGNORE INTO settings (key, value) VALUES ('base_url', 'https://api.deepseek.com');
    INSERT OR IGNORE INTO settings (key, value) VALUES ('model', 'deepseek-v4-flash');
    "
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| format!("读取表结构失败: {}", e))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("读取表结构失败: {}", e))?;

    for existing in columns {
        if existing.map_err(|e| format!("读取表结构失败: {}", e))? == column {
            return Ok(());
        }
    }

    conn.execute(
        &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition),
        [],
    )
    .map_err(|e| format!("迁移数据库失败: {}", e))?;
    Ok(())
}

pub fn initialize(app_data_dir: PathBuf) -> Result<DbState, String> {
    fs::create_dir_all(&app_data_dir).map_err(|e| format!("无法创建数据目录: {}", e))?;

    let db_path = app_data_dir.join("cento.db");

    let conn = Connection::open(&db_path).map_err(|e| format!("无法打开数据库: {}", e))?;

    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| format!("无法设置 WAL 模式: {}", e))?;

    conn.execute_batch(schema_sql())
        .map_err(|e| format!("无法创建表: {}", e))?;

    ensure_column(&conn, "entries", "publication_date", "TEXT")?;
    ensure_column(&conn, "entries", "source", "TEXT")?;
    ensure_column(&conn, "entries", "summary_source", "TEXT")?;
    ensure_column(&conn, "entries", "is_read", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&conn, "entries", "read_at", "TEXT")?;
    ensure_column(&conn, "entries", "affiliation", "TEXT")?;
    ensure_column(&conn, "feeds", "refresh_interval", "TEXT NOT NULL DEFAULT '1d'")?;
    ensure_column(&conn, "feeds", "notify", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&conn, "feeds", "last_fetched_at", "TEXT")?;
    conn.execute(
        "UPDATE entries
         SET summary_source = 'rss'
         WHERE summary_source IS NULL AND summary IS NOT NULL AND trim(summary) <> ''",
        [],
    )
    .map_err(|e| format!("回填摘要来源失败: {}", e))?;
    conn.execute("DELETE FROM settings WHERE key = 'elsevier_api_key'", [])
        .map_err(|e| format!("清理旧设置失败: {}", e))?;

    backfill_reading_events(&conn)?;

    Ok(DbState {
        conn: Mutex::new(conn),
    })
}

/// One-shot backfill for pre-existing installs: derive `reading_events` rows
/// from whatever is currently in `entries`. Gated by a settings flag so it
/// never runs twice and never duplicates events for fresh installs.
fn backfill_reading_events(conn: &Connection) -> Result<(), String> {
    let already: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'reading_events_backfilled'",
            [],
            |row| row.get(0),
        )
        .ok();
    if already.as_deref() == Some("1") {
        return Ok(());
    }

    conn.execute(
        "INSERT INTO reading_events (kind, feed_id, feed_title_snapshot, entry_id, occurred_at)
         SELECT 'fetched', e.feed_id, f.title, e.id, COALESCE(e.fetched_at, datetime('now'))
         FROM entries e LEFT JOIN feeds f ON f.id = e.feed_id",
        [],
    )
    .map_err(|e| format!("回填抓取事件失败: {}", e))?;

    conn.execute(
        "INSERT INTO reading_events (kind, feed_id, feed_title_snapshot, entry_id, occurred_at)
         SELECT 'read', e.feed_id, f.title, e.id, e.read_at
         FROM entries e LEFT JOIN feeds f ON f.id = e.feed_id
         WHERE e.is_read = 1 AND e.read_at IS NOT NULL",
        [],
    )
    .map_err(|e| format!("回填阅读事件失败: {}", e))?;

    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('reading_events_backfilled', '1')",
        [],
    )
    .map_err(|e| format!("设置回填标志失败: {}", e))?;

    Ok(())
}
