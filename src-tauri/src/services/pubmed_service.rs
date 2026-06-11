// PubMed RSS feed URL builder
//
// PubMed does NOT allow predicting the RSS token client-side — each feed
// gets a server-issued opaque token. The "Create RSS" button on the PubMed
// search page POSTs to `/create-rss-feed-url/` to obtain it.
//
// Flow (mirrors what the PubMed UI does):
//   1. GET `https://pubmed.ncbi.nlm.nih.gov/?term=<query>` → receive
//      csrfmiddlewaretoken (hidden form input) + session cookies.
//   2. POST `https://pubmed.ncbi.nlm.nih.gov/create-rss-feed-url/` with
//      form fields `csrfmiddlewaretoken`, `name`, `limit`, `term`, header
//      `X-CSRFToken`, and the session cookies → receive JSON
//      `{"rss_feed_url": "https://pubmed.ncbi.nlm.nih.gov/rss/search/<token>/?…"}`.

use crate::models::DeepSeekSettings;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

const PUBMED_BASE: &str = "https://pubmed.ncbi.nlm.nih.gov";
const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Cento/0.1";

// Per-call client (cookie store must be fresh for each request so we
// don't leak PubMed sessions between unrelated calls).
fn build_client() -> Result<Client, String> {
    Client::builder()
        .cookie_store(true)
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP client init failed: {}", e))
}

#[derive(Deserialize)]
struct CreateRssResponse {
    rss_feed_url: Option<String>,
}

/// Build a real PubMed RSS feed URL by asking PubMed's own create-rss
/// endpoint to issue a token. Returns the full feed URL on success.
pub async fn build_rss_url(query: &str, limit: u32) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("检索式不能为空".to_string());
    }
    let limit = limit.clamp(5, 100);

    let client = build_client()?;
    let search_url = format!("{}/?term={}", PUBMED_BASE, urlencoding::encode_pubmed(query));

    // Step 1: GET search page to obtain CSRF token + session cookies.
    let html = client
        .get(&search_url)
        .send()
        .await
        .map_err(|e| format!("PubMed 搜索页请求失败: {}", e))?
        .error_for_status()
        .map_err(|e| format!("PubMed 搜索页返回错误: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取搜索页内容失败: {}", e))?;

    let csrf = extract_csrf(&html).ok_or_else(|| {
        "未能从 PubMed 页面提取 CSRF token（PubMed 可能改版了，请反馈）".to_string()
    })?;

    // Step 2: POST to /create-rss-feed-url/ — let the same client carry
    // the cookies from step 1.
    let feed_name = derive_feed_name(query);
    let post_url = format!("{}/create-rss-feed-url/", PUBMED_BASE);
    let form = [
        ("csrfmiddlewaretoken", csrf.as_str()),
        ("name", feed_name.as_str()),
        ("limit", &limit.to_string()),
        ("term", query),
    ];

    let resp = client
        .post(&post_url)
        .header("Referer", &search_url)
        .header("X-CSRFToken", &csrf)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("PubMed RSS 生成请求失败: {}", e))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取 PubMed 响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "PubMed 返回 {}：{}",
            status.as_u16(),
            body.chars().take(200).collect::<String>()
        ));
    }

    let parsed: CreateRssResponse = serde_json::from_str(&body)
        .map_err(|e| format!("PubMed 响应不是 JSON: {} — {}", e, body))?;

    parsed
        .rss_feed_url
        .ok_or_else(|| "PubMed 响应缺少 rss_feed_url 字段".to_string())
}

// ── Natural language → PubMed query ────────────

// Forced model for NL → query conversion. We deliberately ignore the user's
// general chat-model setting here: deepseek-v4-flash is tuned for throughput
// and trips on multi-constraint Chinese prompts (journals + MeSH + dates) by
// finishing with `finish_reason: length` before any output is emitted. The
// "pro" variant handles the structured-reasoning step reliably.
const NL_QUERY_MODEL: &str = "deepseek-v4-pro";

// Generous ceiling. PubMed queries are short, but the model may emit a few
// tokens of internal reasoning before the final string, and complex requests
// (5+ journals OR'd together, multiple MeSH groups, date filters) can easily
// push the actual query past 500 tokens. 1500 leaves a safety margin without
// being wasteful.
const NL_QUERY_MAX_TOKENS: u32 = 1500;

const NL_TO_PUBMED_PROMPT: &str = "\
You are a PubMed search expert. Convert a researcher's natural-language request \
(which may be a single complete sentence in Chinese or English, not just keywords) \
into a valid PubMed advanced-search query string.

Understand intent before translating:
- Identify the core topic(s), target journal(s), publication type(s), and time window from the sentence as a whole.
- Map informal Chinese terms to their standard English biomedical vocabulary (e.g. 巨噬细胞 → macrophage, 铁死亡 → ferroptosis, 肿瘤微环境 → tumor microenvironment, 单细胞测序 → single-cell sequencing).
- Map journal aliases to PubMed-recognized titles: Nature/自然 → Nature; Science/科学 → Science; Cell/细胞 → Cell; NEJM → \"N Engl J Med\"; Lancet/柳叶刀 → Lancet; JAMA → JAMA; Nat Med → \"Nat Med\"; etc. Group multiple journals with OR inside parentheses.
- Interpret time hints: 最新/近期/recent → \"last 1 years\"[dp]; 近 N 年 → \"last N years\"[dp]; 近 N 天 → \"last N days\"[dp]. If a time hint is implied but not explicit (e.g. 最新), default to \"last 2 years\"[dp].

Syntax rules:
- Field tags: [Title], [Title/Abstract], [Author], [Journal], [Publication Type], [MeSH Terms], [dp]
- Booleans uppercase: AND, OR, NOT
- Parentheses for grouping; quotes around multi-word phrases and journal abbreviations.
- Topic terms default to [Title/Abstract] when no field is given, with [MeSH Terms] OR'd in for well-known concepts.
- Clinical trials: \"clinical trial\"[Publication Type] OR \"randomized controlled trial\"[Publication Type]
- Reviews: \"review\"[Publication Type] OR \"systematic review\"[Publication Type] OR \"meta-analysis\"[Publication Type]
- Journal filter form: (\"Nature\"[Journal] OR \"Science\"[Journal] OR \"Cell\"[Journal])

Example:
Input: 帮我检索发表在 nature、science、cell 上的关于巨噬细胞以及铁死亡相关的最新的文章
Output: (\"Nature\"[Journal] OR \"Science\"[Journal] OR \"Cell\"[Journal]) AND ((macrophage[Title/Abstract] OR macrophages[MeSH Terms]) AND (ferroptosis[Title/Abstract] OR ferroptosis[MeSH Terms])) AND \"last 2 years\"[dp]

Output format:
- Return ONLY the PubMed query string on a single line. No markdown, no explanation, no surrounding quotes.
- If the request is ambiguous, make your best guess and still return a query.";

pub async fn natural_language_to_query(
    settings: &DeepSeekSettings,
    text: &str,
) -> Result<String, String> {
    let url = format!(
        "{}/chat/completions",
        settings.base_url.trim_end_matches('/')
    );

    let body = serde_json::json!({
        "model": NL_QUERY_MODEL,
        "messages": [
            {"role": "system", "content": NL_TO_PUBMED_PROMPT},
            {"role": "user", "content": text}
        ],
        "temperature": 0.1,
        "max_tokens": NL_QUERY_MAX_TOKENS
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", settings.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("AI 请求发送失败（网络或超时）: {}", e))?;

    let status = response.status();
    let response_body: Value = response
        .json()
        .await
        .map_err(|e| format!("解析 AI 响应失败（非 JSON）: {}", e))?;

    if !status.is_success() {
        let error_type = response_body["error"]["type"].as_str().unwrap_or("");
        let error_msg = response_body["error"]["message"]
            .as_str()
            .unwrap_or("未知错误");
        let detail = if error_type.is_empty() {
            String::new()
        } else {
            format!(" [{}]", error_type)
        };
        return Err(format!("API 返回 {} {}{}", status.as_u16(), error_msg, detail));
    }

    let content = response_body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| {
            let snippet = serde_json::to_string(&response_body)
                .unwrap_or_else(|_| "无法序列化".to_string());
            let truncated: String = snippet.chars().take(300).collect();
            format!("AI 响应格式异常（模型 {}），响应: {}", NL_QUERY_MODEL, truncated)
        })?
        .trim()
        .to_string();

    let finish_reason = response_body["choices"][0]["finish_reason"]
        .as_str()
        .unwrap_or("");

    if content.is_empty() {
        return Err(format!(
            "AI 返回空结果（模型 {}, finish_reason: {}）。请尝试简化检索描述，或在「设置 → 翻译」中确认 base_url/API Key 支持该模型。",
            NL_QUERY_MODEL,
            if finish_reason.is_empty() { "未知" } else { finish_reason }
        ));
    }

    // `length` means the model was still generating when it hit max_tokens.
    // The partial string is usually still a valid PubMed expression (the
    // tail just gets clipped), so surface it instead of failing outright.
    // PubMed itself will tell the user if the syntax is broken.
    if finish_reason == "length" {
        tracing::warn!(
            model = NL_QUERY_MODEL,
            max_tokens = NL_QUERY_MAX_TOKENS,
            "NL→PubMed: response truncated at max_tokens, returning partial query"
        );
    }

    Ok(content)
}

/// Extract the `csrfmiddlewaretoken` hidden input value from PubMed's HTML.
fn extract_csrf(html: &str) -> Option<String> {
    // Looks like: name="csrfmiddlewaretoken" value="XXX"
    let needle = "name=\"csrfmiddlewaretoken\"";
    let idx = html.find(needle)?;
    let tail = &html[idx..];
    let value_start = tail.find("value=\"")? + "value=\"".len();
    let after_value = &tail[value_start..];
    let value_end = after_value.find('"')?;
    Some(after_value[..value_end].to_string())
}

/// Derive a feed name from the query when the user didn't supply one.
/// PubMed limits the name to 200 chars and disallows `" & = < > /`.
fn derive_feed_name(query: &str) -> String {
    let cleaned: String = query
        .chars()
        .map(|c| match c {
            '"' | '&' | '=' | '<' | '>' | '/' => ' ',
            _ => c,
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.len() > 200 {
        trimmed.chars().take(200).collect()
    } else {
        trimmed.to_string()
    }
}

// Minimal URL-encoding helper — we don't pull a new crate for this.
mod urlencoding {
    pub fn encode_pubmed(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char);
                }
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{:02X}", b)),
            }
        }
        out
    }
}

