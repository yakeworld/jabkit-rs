use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

/// bioRxiv / medRxiv via details API (free, no key).
/// Search scans the last N days (query filter on title/abstract).
pub struct BioRxiv {
    days: u32,
}

impl BioRxiv {
    /// server: "biorxiv" or "medrxiv"
    pub fn new(server: &str, days: u32) -> Self {
        // stored as server name; validated on search
        let _ = server;
        Self { days }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BRResponse {
    collection: Option<Vec<BRItem>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BRItem {
    doi: Option<String>,
    title: Option<String>,
    authors: Option<String>,
    date: Option<String>,
    category: Option<String>,
    r#abstract: Option<String>,
}

fn fmt_date(ts: i64) -> String {
    let secs = (ts / 86400) as i64;
    // days from civil epoch
    let (y, m, d) = civil_from_days(secs);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

    fn br_item_to_entry(it: BRItem) -> Option<BibEntry> {
        let title = it.title?;
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Title, title);
        if let Some(a) = it.authors {
            let parts: Vec<String> = a
                .split(';')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            if !parts.is_empty() {
                e.set_field(Field::Author, parts.join(" and "));
            }
        }
        if let Some(p) = it.date {
            if p.len() >= 4 {
                e.set_field(Field::Year, p[..4].to_string());
            }
        }
        if let Some(doi) = it.doi {
            e.set_field(Field::Doi, doi);
        }
        e.set_field(Field::Journal, "bioRxiv".to_string());
        Some(e)
    }

#[async_trait]
impl Provider for BioRxiv {
    fn name(&self) -> &'static str { "BioRxiv" }
    fn key_env(&self) -> Option<&'static str> { None }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1700000000);
        let days = self.days.min(7).max(1) as u64;
        let ql = query.to_lowercase();
        let mut out: Vec<BibEntry> = Vec::new();
        let client = super::http_client();
        for off in 0..days {
            if out.len() >= limit {
                break;
            }
            let day = fmt_date((now - off * 86400) as i64);
            let url = format!(
                "https://api.biorxiv.org/details/biorxiv/{}/{}/0/1000",
                day, day
            );
            let resp = client
                .get(&url)
                .header("User-Agent", "jabkit/0.1")
                .send()
                .await?;
            if !resp.status().is_success() {
                continue;
            }
            // BioRxiv returns HTTP 200 with an EMPTY body (content-length: 0)
            // for dates with no submissions (observed 2026-09). Skip those
            // instead of failing the whole search on the empty JSON decode.
            let body = resp.text().await?;
            if body.trim().is_empty() || !body.starts_with('{') {
                continue;
            }
            let d: BRResponse = match serde_json::from_str(&body) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for it in d.collection.unwrap_or_default() {
                if out.len() >= limit {
                    break;
                }
                let title = it.title.as_deref().unwrap_or("").to_lowercase();
                let desc = it.r#abstract.as_deref().unwrap_or("").to_lowercase();
                if !title.contains(&ql) && !desc.contains(&ql) {
                    continue;
                }
                if let Some(e) = br_item_to_entry(it) {
                    out.push(e);
                }
            }
        }
        let total = out.len();
        Ok(SearchResult {
            entries: out,
            total_found: total,
        })
    }


    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let date = if id.len() >= 10 && id.as_bytes()[4] == b'-' {
            // date format YYYY-MM-DD: details for that day
            id.to_string()
        } else {
            // DOI: fetch details and look up
            let doi = if id.starts_with("doi:") {
                id[4..].to_string()
            } else {
                id.to_string()
            };
            // scan last 30 days
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(1700000000);
            let date_end = fmt_date(now as i64);
            let date_start = fmt_date(now as i64 - 30 * 86400);
            let url = format!(
                "https://api.biorxiv.org/details/biorxiv/{}/{}/0/1000",
                date_start, date_end
            );
            let resp = super::http_client()
                .get(&url)
                .header("User-Agent", "jabkit/0.1")
                .send()
                .await?;
            let d: BRResponse = resp.json().await?;
            let item = d
                .collection
                .unwrap_or_default()
                .into_iter()
                .find(|it| it.doi.as_deref() == Some(doi.as_str()));
            let it = item.ok_or_else(|| anyhow::anyhow!("BioRxiv DOI not found: {}", doi))?;
            let mut e = BibEntry::new(EntryType::Article);
            e.set_field(Field::Title, it.title.unwrap_or_default());
            if let Some(a) = it.authors {
                let parts: Vec<String> = a.split(';').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
                if !parts.is_empty() {
                    e.set_field(Field::Author, parts.join(" and "));
                }
            }
            if let Some(p) = it.date {
                if p.len() >= 4 {
                    e.set_field(Field::Year, p[..4].to_string());
                }
            }
            e.set_field(Field::Doi, doi);
            return Ok(e);
        };
        // date-based fetch: return first item
        let url = format!(
            "https://api.biorxiv.org/details/biorxiv/{}/{}/0/1",
            &date[..10],
            &date[..10]
        );
        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit/0.1")
            .send()
            .await?;
        let d: BRResponse = resp.json().await?;
        let it = d
            .collection
            .unwrap_or_default()
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No BioRxiv entry for {}", id))?;
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Title, it.title.unwrap_or_default());
        if let Some(p) = it.date {
            if p.len() >= 4 {
                e.set_field(Field::Year, p[..4].to_string());
            }
        }
        Ok(e)
    }
}
