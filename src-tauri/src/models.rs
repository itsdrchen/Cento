use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feed {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub refresh_interval: String,
    pub notify: bool,
    pub last_fetched_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: i64,
    pub feed_id: i64,
    pub guid: String,
    pub title: String,
    pub link: String,
    pub summary: Option<String>,
    pub summary_source: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub publication_date: Option<String>,
    pub source: Option<String>,
    pub affiliation: Option<String>,
    pub fetched_at: String,
    pub is_read: bool,
    pub read_at: Option<String>,
    pub title_translated: Option<String>,
    pub summary_translated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekSettings {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub system_prompt: String,
    pub read_retention_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekBalanceInfo {
    pub currency: String,
    pub total_balance: String,
    pub granted_balance: String,
    pub topped_up_balance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekBalance {
    pub is_available: bool,
    pub balance_infos: Vec<DeepSeekBalanceInfo>,
}

/// Token usage returned by DeepSeek's `/chat/completions` (the `usage` block).
/// Cache-hit input is billed at a quarter of cache-miss input, so we keep the
/// two separated — averaging them out would significantly inflate the
/// reported cost for short, repetitive prompts (like Cento's system prompt).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_cache_hit_tokens: i64,
    pub prompt_cache_miss_tokens: i64,
    pub completion_tokens: i64,
}

/// One row of the per-model cost breakdown returned by `get_cost_summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdownRow {
    pub model: String,
    pub prompt_cache_hit_tokens: i64,
    pub prompt_cache_miss_tokens: i64,
    pub completion_tokens: i64,
    pub cny: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub month: String,
    pub total_cny: f64,
    pub breakdown: Vec<CostBreakdownRow>,
}

// AI briefing payloads — surfaced via `commands::briefing_cmd`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingCounts {
    pub articles: i64,
    pub feeds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Briefing {
    pub id: i64,
    pub period: String,
    pub title: String,
    pub lead_in: String,
    pub content: String,
    pub counts: BriefingCounts,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingStats {
    pub total_entries: i64,
    pub total_read: i64,
    pub day_counts: Vec<(String, i64)>,
    /// Per-feed read counts. Tuple: (feed_id, snapshot_title, count). The
    /// snapshot lets the UI still show a name for feeds the user has since
    /// deleted — the frontend prefers the live feed title when available and
    /// falls back to the snapshot otherwise.
    pub feed_read_counts: Vec<(i64, Option<String>, i64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedFetchResult {
    pub feed_id: i64,
    pub feed_title: String,
    pub new_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub release_notes: Option<String>,
    /// First `.dmg` asset attached to the release, if any. Frontend uses it
    /// for the "下载安装包" direct link; missing → fall back to `release_url`.
    pub asset_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    pub total_feeds: usize,
    pub new_entries: usize,
    pub translated_titles: usize,
    pub fetched_summaries: usize,
    pub translated_summaries: usize,
    pub errors: Vec<String>,
    pub feeds: Vec<FeedFetchResult>,
}
