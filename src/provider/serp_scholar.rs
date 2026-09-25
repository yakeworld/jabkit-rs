use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};
use crate::resilient::{get_resilient, KeyPlacement};

/// Google Scholar via SerpAPI (engine=google_scholar).
pub struct SerpScholar {
    api_keys: Vec<String>,
}

impl SerpScholar {
    pub fn new(api_keys: Vec<String>) -> Self {
        Self { api_keys }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SSResponse {
    organic_results: Option<Vec<SSResult>>,
    scholar_results: Option<Vec<SSResult>>,
    #[serde(rename = "total_results")]
    total: Option<serde_json::Value>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SSResult {
    title: Option<String>,
    #[serde(rename = "publication_info")]
    pub_info: Option<SSPubInfo>,
    authors: Option<Vec<SSAuthor>>,
    abstract_: Option<serde_json::Value>,
    #[serde(rename = "link")]
    link: Option<String>,
    #[serde(rename = "scholar_cluster")]
    _cluster: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SSPubInfo {
    summary: Option<String>,
    authors: Option<Vec<SSAuthor>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SSAuthor {
    name: Option<String>,
}

fn pub_summary_fields(summary: &str) -> (String, String) {
    // "JM Furman, SP Cass - New England Journal of Medicine, 1999 - Mass Medical Soc"
    // middle = venue, year (last segment = publisher, may be absent)
    let after_authors = summary.split(" - ").nth(1).unwrap_or("");
    let middle = match after_authors.rsplit_once(" - ") {
        Some((m, _)) => m,
        None => after_authors,
    };
    let parts: Vec<&str> = middle.splitn(2, ',').collect();
    let venue = parts.first().copied().unwrap_or("").trim().to_string();
    let year = parts
        .get(1)
        .map(|y| {
            let t = y.trim();
            t.chars().take_while(|c| c.is_ascii_digit()).collect::<String>()
        })
        .unwrap_or_default();
    (venue, year)
}

fn opt_year(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::String(s) => {
            // "1998", "1998-2003", "1998 · Cited by 1234"
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.len() == 4 {
                Some(digits)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[async_trait]
impl Provider for SerpScholar {
    fn name(&self) -> &'static str { "SerpScholar" }
    fn key_env(&self) -> Option<&'static str> { Some("SERP_API_KEY") }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        if self.api_keys.is_empty() {
            anyhow::bail!("SerpScholar requires SERP_API_KEY");
        }
        let base = format!(
            "https://serpapi.com/search?engine=google_scholar&q={}&num={}",
            urlencoding(query),
            limit.min(20)
        );
        let resp = get_resilient(
            &|k| format!("{base}&api_key={k}"),
            "jabkit/0.1",
            &self.api_keys,
            KeyPlacement::Baked,
        )
        .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("SerpScholar {}: {}", status, &text[..text.len().min(200)]);
        }
        let d: SSResponse = serde_json::from_str(&text)?;
        if let Some(err) = &d.error {
            anyhow::bail!("SerpScholar API error: {}", err);
        }
        let items = d
            .organic_results
            .or(d.scholar_results)
            .unwrap_or_default();
        let total = d
            .total
            .and_then(|v| v.as_u64())
            .unwrap_or(items.len() as u64) as usize;

        let entries = items
            .iter()
            .take(limit)
            .filter_map(|r| {
                let title = r.title.as_ref()?;
                if title.is_empty() {
                    return None;
                }
                let mut e = BibEntry::new(EntryType::Article);
                e.set_field(Field::Title, title.clone());
                if let Some(pi) = &r.pub_info {
                    if let Some(summary) = &pi.summary {
                        let (venue, year) = pub_summary_fields(summary);
                        if !venue.is_empty() {
                            e.set_field(Field::Journal, venue);
                        }
                        if !year.is_empty() {
                            e.set_field(Field::Year, year);
                        }
                        // authors from publication_info (preferred, ordered)
                        if let Some(a) = &pi.authors {
                            let names: Vec<String> =
                                a.iter().filter_map(|x| x.name.clone()).collect();
                            if !names.is_empty() {
                                e.set_field(Field::Author, names.join(" and "));
                            }
                        }
                    }
                }
                if e.get(Field::Author).map_or(true, |a| a.is_empty()) {
                    if let Some(a) = &r.authors {
                        let names: Vec<String> =
                            a.iter().filter_map(|x| x.name.clone()).collect();
                        if !names.is_empty() {
                            e.set_field(Field::Author, names.join(" and "));
                        }
                    }
                }
                if let Some(l) = &r.link {
                    e.set_field(Field::Url, l.clone());
                }
                Some(e)
            })
            .collect();

        Ok(SearchResult {
            entries,
            total_found: total,
        })
    }

    async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> {
        anyhow::bail!("SerpScholar does not support fetch_by_id (Scholar has no stable IDs)");
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() { match b {
        b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'/' => r.push(b as char),
        b' ' => r.push_str("%20"), _ => r.push_str(&format!("%{:02X}", b)),
    }} r
}
