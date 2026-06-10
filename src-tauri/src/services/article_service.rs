use serde_json::Value;
use tracing::{info, warn};

const BROWSER_UA: &str = "Cento/0.1 (https://github.com/itsdrchen/Cento)";

#[derive(Debug, Clone, Default)]
pub struct RssMetadata {
    pub publication_date: Option<String>,
    pub source: Option<String>,
    pub is_metadata_only: bool,
}

#[derive(Debug, Clone)]
pub struct AbstractFetchResult {
    pub text: String,
    pub source: String,
}

pub fn extract_rss_metadata(summary: Option<&str>) -> RssMetadata {
    let Some(summary) = summary else {
        return RssMetadata::default();
    };

    let publication_date = extract_paragraph_value(summary, "Publication date:");
    let source = extract_source(summary);
    let lower = summary.to_lowercase();
    let has_metadata_marker = lower.contains("publication date:")
        || lower.contains("source:")
        || lower.contains("author(s):");
    let word_count = clean_text(summary).split_whitespace().count();
    let is_metadata_only = has_metadata_marker && word_count < 40;

    RssMetadata {
        publication_date,
        source,
        is_metadata_only,
    }
}

#[tracing::instrument(skip_all, fields(title = %title))]
pub async fn fetch_abstract(title: &str) -> Result<Option<AbstractFetchResult>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(BROWSER_UA)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    info!("尝试从 Semantic Scholar 获取 Abstract");
    match fetch_semantic_scholar_abstract(&client, title).await {
        Ok(Some(text)) => {
            info!("Semantic Scholar 返回 Abstract");
            return Ok(Some(AbstractFetchResult {
                text,
                source: "semantic_scholar".to_string(),
            }));
        }
        Ok(None) => info!("Semantic Scholar 未匹配到可用 Abstract"),
        Err(err) => warn!(error = %err, "Semantic Scholar 获取 Abstract 失败"),
    }

    info!("尝试从 PubMed 获取 Abstract");
    match fetch_pubmed_abstract(&client, title).await {
        Ok(Some(text)) => {
            info!("PubMed 返回 Abstract");
            return Ok(Some(AbstractFetchResult {
                text,
                source: "pubmed".to_string(),
            }));
        }
        Ok(None) => info!("PubMed 未匹配到可用 Abstract"),
        Err(err) => warn!(error = %err, "PubMed 获取 Abstract 失败"),
    }

    info!("所有公共索引均未返回 Abstract");
    Ok(None)
}

#[tracing::instrument(skip(client), fields(title = %title))]
async fn fetch_semantic_scholar_abstract(
    client: &reqwest::Client,
    title: &str,
) -> Result<Option<String>, String> {
    let response = client
        .get("https://api.semanticscholar.org/graph/v1/paper/search")
        .query(&[
            ("query", title),
            ("limit", "5"),
            ("fields", "title,abstract"),
        ])
        .send()
        .await
        .map_err(|e| format!("请求 Semantic Scholar 失败: {}", e))?;

    let status = response.status();
    info!(status = status.as_u16(), "Semantic Scholar HTTP 响应");
    if !status.is_success() {
        return Ok(None);
    }

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("解析 Semantic Scholar 响应失败: {}", e))?;

    let Some(items) = body.get("data").and_then(|v| v.as_array()) else {
        return Ok(None);
    };

    for item in items {
        let item_title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if !titles_match(title, item_title) {
            info!(candidate = %item_title, "Semantic Scholar 标题未匹配");
            continue;
        }
        if let Some(abstract_text) = item.get("abstract").and_then(|v| v.as_str()) {
            let cleaned = clean_text(abstract_text);
            if looks_like_abstract(&cleaned) {
                return Ok(Some(cleaned));
            }
        }
    }

    Ok(None)
}

#[tracing::instrument(skip(client), fields(title = %title))]
async fn fetch_pubmed_abstract(
    client: &reqwest::Client,
    title: &str,
) -> Result<Option<String>, String> {
    let term = format!("{}[Title]", clean_text(title));
    let search_response = client
        .get("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi")
        .query(&[
            ("db", "pubmed"),
            ("term", term.as_str()),
            ("retmode", "json"),
            ("retmax", "5"),
        ])
        .send()
        .await
        .map_err(|e| format!("请求 PubMed ESearch 失败: {}", e))?;

    let status = search_response.status();
    info!(status = status.as_u16(), "PubMed ESearch HTTP 响应");
    if !status.is_success() {
        return Ok(None);
    }

    let search_body: Value = search_response
        .json()
        .await
        .map_err(|e| format!("解析 PubMed ESearch 响应失败: {}", e))?;

    let ids = search_body
        .get("esearchresult")
        .and_then(|v| v.get("idlist"))
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    if ids.is_empty() {
        return Ok(None);
    }

    let id_list = ids.join(",");
    let fetch_response = client
        .get("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi")
        .query(&[
            ("db", "pubmed"),
            ("id", id_list.as_str()),
            ("retmode", "xml"),
        ])
        .send()
        .await
        .map_err(|e| format!("请求 PubMed EFetch 失败: {}", e))?;

    let status = fetch_response.status();
    info!(status = status.as_u16(), "PubMed EFetch HTTP 响应");
    if !status.is_success() {
        return Ok(None);
    }

    let xml = fetch_response
        .text()
        .await
        .map_err(|e| format!("读取 PubMed EFetch 响应失败: {}", e))?;

    Ok(extract_pubmed_abstract(&xml, title))
}

/// Fetch the first `<Affiliation>` from PubMed for a given PMID. The first
/// affiliation in a PubMed article XML is the first author's primary
/// institution, which is what users want shown under the title.
#[tracing::instrument(skip(client))]
pub async fn fetch_pubmed_first_affiliation(
    client: &reqwest::Client,
    pmid: &str,
) -> Result<Option<String>, String> {
    let response = client
        .get("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi")
        .query(&[("db", "pubmed"), ("id", pmid), ("retmode", "xml")])
        .send()
        .await
        .map_err(|e| format!("请求 PubMed EFetch 失败: {}", e))?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let xml = response
        .text()
        .await
        .map_err(|e| format!("读取 PubMed EFetch 响应失败: {}", e))?;

    Ok(extract_first_affiliation(&xml))
}

fn extract_first_affiliation(xml: &str) -> Option<String> {
    let doc = parse_pubmed_xml(xml)?;
    let article = doc
        .descendants()
        .find(|node| node.has_tag_name("Article"))?;
    let aff = article
        .descendants()
        .find(|node| node.has_tag_name("Affiliation"))
        .map(node_text)?;
    (!aff.is_empty()).then_some(aff)
}

/// Extract a PMID from a PubMed entry link. Handles every form PubMed RSS
/// actually emits: bare `pubmed.ncbi.nlm.nih.gov/<id>/`, the same with UTM
/// tracking query params, and the legacy `ncbi.nlm.nih.gov/pubmed/<id>` path.
pub fn extract_pmid_from_link(link: &str) -> Option<String> {
    let lower = link.to_lowercase();
    for needle in ["pubmed.ncbi.nlm.nih.gov/", "/pubmed/"] {
        if let Some(pos) = lower.find(needle) {
            let after = &link[pos + needle.len()..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return Some(digits);
            }
        }
    }
    None
}

/// PubMed RSS guids are typically `pubmed:<PMID>` (feed-rs preserves the raw
/// guid string). Some feeds emit a bare numeric id.
pub fn extract_pmid_from_guid(guid: &str) -> Option<String> {
    let trimmed = guid.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = trimmed
        .rsplit(':')
        .next()
        .unwrap_or(trimmed)
        .trim_matches('/');
    if !candidate.is_empty() && candidate.chars().all(|c| c.is_ascii_digit()) {
        Some(candidate.to_string())
    } else {
        None
    }
}

/// Last-ditch: scan the RSS description for `PMID: <digits>`. PubMed RSS
/// frequently appends this to the citation block.
pub fn extract_pmid_from_text(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let pos = lower.find("pmid:")? + "pmid:".len();
    let after = &text[pos..];
    let digits: String = after
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// Use PubMed ESearch as a fallback when the URL/guid/description don't yield
/// a PMID. Returns the top hit when its title matches the entry title.
#[tracing::instrument(skip(client))]
pub async fn find_pubmed_pmid_by_title(
    client: &reqwest::Client,
    title: &str,
) -> Result<Option<String>, String> {
    let term = format!("{}[Title]", clean_text(title));
    let response = client
        .get("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi")
        .query(&[
            ("db", "pubmed"),
            ("term", term.as_str()),
            ("retmode", "json"),
            ("retmax", "1"),
        ])
        .send()
        .await
        .map_err(|e| format!("PubMed ESearch 请求失败: {}", e))?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("解析 PubMed ESearch 响应失败: {}", e))?;

    let id = body
        .get("esearchresult")
        .and_then(|v| v.get("idlist"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(id.filter(|s| !s.is_empty()))
}

// PubMed EFetch XML carries a <!DOCTYPE ...> declaration. roxmltree rejects
// DTDs by default — we have to opt in or every PubMed response would silently
// fail to parse.
fn parse_pubmed_xml(xml: &str) -> Option<roxmltree::Document<'_>> {
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(xml, opts).ok()
}

fn extract_pubmed_abstract(xml: &str, title: &str) -> Option<String> {
    let doc = parse_pubmed_xml(xml)?;

    for article in doc
        .descendants()
        .filter(|node| node.has_tag_name("Article"))
    {
        let article_title = article
            .descendants()
            .find(|node| node.has_tag_name("ArticleTitle"))
            .map(node_text)
            .unwrap_or_default();

        if !article_title.is_empty() && !titles_match(title, &article_title) {
            info!(candidate = %article_title, "PubMed 标题未匹配");
            continue;
        }

        let parts = article
            .descendants()
            .filter(|node| node.has_tag_name("AbstractText"))
            .filter_map(|node| {
                let text = node_text(node);
                (!text.is_empty()).then_some(text)
            })
            .collect::<Vec<_>>();

        if parts.is_empty() {
            continue;
        }

        let abstract_text = parts.join(" ");
        if looks_like_abstract(&abstract_text) {
            return Some(abstract_text);
        }
    }

    None
}

fn node_text(node: roxmltree::Node) -> String {
    // Only collect Text-type descendants. Element nodes' `.text()` returns
    // their first text child, which then gets visited again as a real Text
    // node — collecting both duplicates every fragment.
    clean_text(
        &node
            .descendants()
            .filter(|child| child.is_text())
            .filter_map(|child| child.text())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn extract_paragraph_value(html: &str, label: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let label_lower = label.to_lowercase();
    let start = lower.find(&label_lower)?;
    let after_label = start + label.len();
    let end = lower[after_label..]
        .find("</p>")
        .map(|idx| after_label + idx)
        .unwrap_or(html.len());
    let value = clean_text(&html[after_label..end]);
    (!value.is_empty()).then_some(value)
}

fn extract_source(html: &str) -> Option<String> {
    let lower = html.to_lowercase();

    let raw = if let Some(pos) = lower.find("source:") {
        let after = pos + "source:".len();
        let end = lower[after..]
            .find("</p>")
            .map(|i| after + i)
            .unwrap_or(html.len());
        clean_text(&html[after..end])
    } else {
        let open = lower.find("<p")?;
        let after_open = lower[open..].find('>').map(|i| open + i + 1)?;
        let close = lower[after_open..].find("</p>").map(|i| after_open + i)?;
        clean_text(&html[after_open..close])
    };

    if raw.is_empty() {
        return None;
    }

    let journal = trim_journal_name(&raw);
    (!journal.is_empty()).then_some(journal)
}

// PubMed citations look like "Lung. 2026 Jun 1;204(1):69-74. doi: ...".
// We want just the journal name — the part before the first period followed
// by a digit (year). Abbreviations like "J. Ethnopharmacol." keep their
// internal periods because those are followed by letters, not digits.
// ScienceDirect "Phytomedicine, Volume 156" is split on the comma.
fn trim_journal_name(citation: &str) -> String {
    let bytes = citation.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b';' || b == b'|' || b == b',' {
            return citation[..i].trim().to_string();
        }
        if b == b'.' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j < bytes.len() && bytes[j].is_ascii_digit() {
                return citation[..i].trim().to_string();
            }
        }
        i += 1;
    }
    citation.trim().to_string()
}

fn titles_match(left: &str, right: &str) -> bool {
    let left = normalize_title(left);
    let right = normalize_title(right);
    !left.is_empty()
        && !right.is_empty()
        && (left == right || left.contains(&right) || right.contains(&left))
}

fn normalize_title(title: &str) -> String {
    strip_html(title)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn looks_like_abstract(text: &str) -> bool {
    let lower = text.to_lowercase();
    text.split_whitespace().count() >= 25
        && !lower.contains("publication date:")
        && !lower.contains("author(s):")
        && !lower.contains("science direct")
}

/// Collapse a string that is exactly the same content repeated twice
/// (separated by whitespace) back to a single copy. Older builds doubled
/// every PubMed XML node's text by visiting element + text-child both, and
/// those bad values are still cached in user SQLite — this lets us serve a
/// clean string without forcing a re-fetch.
pub fn dedupe_repeated(text: &str) -> String {
    let trimmed = text.trim();
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let n = words.len();
    if n >= 2 && n.is_multiple_of(2) {
        let half = n / 2;
        if words[..half] == words[half..] {
            return words[..half].join(" ");
        }
    }
    trimmed.to_string()
}

fn clean_text(text: &str) -> String {
    decode_html_entities(&strip_html(text))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_html_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_sciencedirect_metadata_without_treating_it_as_abstract() {
        let html = "<p>Publication date: July 2026</p><p>Source: Phytomedicine, Volume 156</p><p>Author(s): Someone</p>";
        let metadata = extract_rss_metadata(Some(html));

        assert!(metadata.is_metadata_only);
        assert_eq!(metadata.publication_date.as_deref(), Some("July 2026"));
        assert_eq!(
            metadata.source.as_deref(),
            Some("Phytomedicine")
        );
    }

    #[test]
    fn keeps_structured_abstract_even_without_abstract_label() {
        let html = "<p>Publication date: July 2026</p><p>Source: Phytomedicine, Volume 156</p><p>Author(s): Zhang et al.</p><p>Background: Macrophage polarization plays a central role in inflammatory disease progression and tissue injury repair.</p><p>Methods: We performed transcriptomic profiling, pathway analysis, and validation experiments in cellular and animal models.</p><p>Results: The treatment significantly reduced inflammatory signaling and restored epithelial barrier integrity.</p><p>Conclusions: These findings support further translational investigation.</p>";
        let metadata = extract_rss_metadata(Some(html));

        assert!(!metadata.is_metadata_only);
        assert_eq!(metadata.publication_date.as_deref(), Some("July 2026"));
        assert_eq!(
            metadata.source.as_deref(),
            Some("Phytomedicine")
        );
    }

    #[test]
    fn extracts_journal_from_pubmed_citation_leading_paragraph() {
        let html = "<p>Lung. 2026 Jun 1;204(1):69-74. doi: 10.1007/s00408-026-00750-2. Epub 2026 May 28.</p><p><b>ABSTRACT</b></p><p>Background ...</p>";
        let metadata = extract_rss_metadata(Some(html));
        assert_eq!(metadata.source.as_deref(), Some("Lung"));
    }

    #[test]
    fn keeps_abbreviated_journal_name_with_internal_periods() {
        let html = "<p>J. Ethnopharmacol. 2026 Jul 1;334:118765. doi: 10.1016/j.jep.2026.118765.</p>";
        let metadata = extract_rss_metadata(Some(html));
        assert_eq!(metadata.source.as_deref(), Some("J. Ethnopharmacol"));
    }

    #[test]
    fn extracts_first_author_affiliation_from_pubmed_xml() {
        let xml = r#"
            <PubmedArticleSet>
              <PubmedArticle>
                <MedlineCitation>
                  <Article>
                    <AuthorList>
                      <Author>
                        <LastName>Zhang</LastName>
                        <ForeName>Huiling</ForeName>
                        <AffiliationInfo>
                          <Affiliation>Senior Department of Pediatrics, Chinese PLA General Hospital, Beijing 100700, China.</Affiliation>
                        </AffiliationInfo>
                      </Author>
                      <Author>
                        <LastName>Yang</LastName>
                        <ForeName>Guang</ForeName>
                        <AffiliationInfo>
                          <Affiliation>Department of Critical Care Medicine, Southern Medical University, Guangzhou 510080, China.</Affiliation>
                        </AffiliationInfo>
                      </Author>
                    </AuthorList>
                  </Article>
                </MedlineCitation>
              </PubmedArticle>
            </PubmedArticleSet>
        "#;
        let aff = extract_first_affiliation(xml).expect("affiliation extracted");
        assert!(aff.starts_with("Senior Department of Pediatrics"));
        assert!(aff.contains("Chinese PLA General Hospital"));
    }

    #[test]
    fn extract_pmid_handles_pubmed_links() {
        assert_eq!(
            extract_pmid_from_link("https://pubmed.ncbi.nlm.nih.gov/40123456/").as_deref(),
            Some("40123456")
        );
        assert_eq!(
            extract_pmid_from_link("https://pubmed.ncbi.nlm.nih.gov/40123456").as_deref(),
            Some("40123456")
        );
        // The real shape PubMed RSS emits — bug we just fixed.
        assert_eq!(
            extract_pmid_from_link(
                "https://pubmed.ncbi.nlm.nih.gov/42193470/?utm_source=Affymetrix&utm_medium=feed&utm_campaign=None_search"
            )
            .as_deref(),
            Some("42193470")
        );
        assert_eq!(
            extract_pmid_from_link("https://www.ncbi.nlm.nih.gov/pubmed/42193470").as_deref(),
            Some("42193470")
        );
        assert_eq!(
            extract_pmid_from_link("https://www.sciencedirect.com/science/article/pii/abc"),
            None
        );
    }

    #[test]
    fn extract_pmid_from_guid_handles_pubmed_prefix() {
        assert_eq!(
            extract_pmid_from_guid("pubmed:42193470").as_deref(),
            Some("42193470")
        );
        assert_eq!(extract_pmid_from_guid("42193470").as_deref(), Some("42193470"));
        assert_eq!(extract_pmid_from_guid("doi:10.3390/foo"), None);
        assert_eq!(extract_pmid_from_guid(""), None);
    }

    #[test]
    fn dedupe_repeated_collapses_doubled_strings() {
        let doubled = "Senior Department of Pediatrics, Chinese PLA General Hospital, Beijing 100700, China. Senior Department of Pediatrics, Chinese PLA General Hospital, Beijing 100700, China.";
        assert_eq!(
            dedupe_repeated(doubled),
            "Senior Department of Pediatrics, Chinese PLA General Hospital, Beijing 100700, China."
        );
        // Non-duplicated input passes through unchanged.
        assert_eq!(dedupe_repeated("Lung Research Institute"), "Lung Research Institute");
        // Single word, two different words, empty — all unchanged.
        assert_eq!(dedupe_repeated("solo"), "solo");
        assert_eq!(dedupe_repeated("apple banana"), "apple banana");
        assert_eq!(dedupe_repeated(""), "");
    }

    #[test]
    fn extract_pmid_from_text_finds_pmid_label() {
        assert_eq!(
            extract_pmid_from_text("Lung. 2026 Jun 1;204(1):69. PMID: 42193470").as_deref(),
            Some("42193470")
        );
        assert_eq!(
            extract_pmid_from_text("PMID:42193470 doi: 10.1/abc").as_deref(),
            Some("42193470")
        );
        assert_eq!(extract_pmid_from_text("no pmid here"), None);
    }

    #[test]
    fn extracts_pubmed_abstract_from_xml() {
        let xml = r#"
            <PubmedArticleSet>
              <PubmedArticle>
                <MedlineCitation>
                  <Article>
                    <ArticleTitle>Gualou total alkaloids improve lipid metabolism and alleviate atherosclerosis by inhibiting hepatic PLA2G2A-mediated LPC/LPA axis</ArticleTitle>
                    <Abstract>
                      <AbstractText>Background: Atherosclerosis remains a major cardiovascular disease with complex lipid metabolic abnormalities and inflammatory activation.</AbstractText>
                      <AbstractText>Methods: This study evaluated Gualou total alkaloids using lipidomics, molecular biology, and animal models to characterize pathway regulation.</AbstractText>
                      <AbstractText>Results: Treatment reduced hepatic PLA2G2A activity, suppressed the LPC/LPA axis, and improved vascular lesion burden.</AbstractText>
                      <AbstractText>Conclusions: These findings suggest that Gualou total alkaloids may improve lipid metabolism and alleviate atherosclerosis.</AbstractText>
                    </Abstract>
                  </Article>
                </MedlineCitation>
              </PubmedArticle>
            </PubmedArticleSet>
        "#;

        let abstract_text = extract_pubmed_abstract(
            xml,
            "Gualou total alkaloids improve lipid metabolism and alleviate atherosclerosis by inhibiting hepatic PLA2G2A-mediated LPC/LPA axis",
        )
        .expect("abstract should be extracted");

        assert!(abstract_text.contains("PLA2G2A"));
        assert!(abstract_text.contains("atherosclerosis"));
    }
}
