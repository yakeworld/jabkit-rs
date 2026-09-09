use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

pub struct Unpaywall {
    email: Option<String>,
}
impl Unpaywall {
    pub fn new(email: Option<String>) -> Self {
        Self { email }
    }
}

#[derive(Deserialize)]
struct UpwResp {
    results: Vec<UpwResult>,
    total: Option<usize>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct UpwResult {
    doi: Option<String>,
    title: Option<String>,
    authors: Option<Vec<UpwAuthor>>,
    year: Option<i32>,
    journal_name: Option<String>,
    publisher: Option<String>,
    genre: Option<String>,
    is_oa: Option<bool>,
    best_oa_location: Option<UpwLocation>,
}
#[derive(Deserialize)]
struct UpwAuthor {
    name: Option<String>,
}
#[derive(Deserialize)]
struct UpwLocation {
    url_for_pdf: Option<String>,
}

#[async_trait]
impl Provider for Unpaywall {
    fn name(&self) -> &'static str {
        "Unpaywall"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("UNPAYWALL_EMAIL")
    }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let email = self
            .email
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Unpaywall requires UNPAYWALL_EMAIL"))?;
        let url = format!(
            "https://api.unpaywall.org/v2/search?query={}&page_size={}&email={}",
            urlencoding(query),
            limit.min(100),
            email
        );
        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit-rs/0.1")
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Unpaywall {}: {}", resp.status(), resp.text().await?);
        }
        let d: UpwResp = resp.json().await?;
        let entries: Vec<BibEntry> = d
            .results
            .into_iter()
            .map(|r| {
                let mut e = BibEntry::new(EntryType::Article);
                e.set_field(Field::Title, r.title.unwrap_or_default());
                if let Some(a) = r.authors {
                    let n: Vec<String> = a.into_iter().filter_map(|a| a.name).collect();
                    e.set_field(Field::Author, n.join(" and "));
                }
                e.set_field(Field::Doi, r.doi.unwrap_or_default());
                e.set_field(Field::Journal, r.journal_name.unwrap_or_default());
                if let Some(y) = r.year {
                    e.set_field(Field::Year, y.to_string());
                }
                e.set_field(Field::Publisher, r.publisher.unwrap_or_default());
                if let Some(oaloc) = r.best_oa_location {
                    if let Some(pdf) = oaloc.url_for_pdf {
                        e.set_field(Field::Url, pdf);
                    }
                }
                e
            })
            .collect();
        let total_found = d.total.unwrap_or(entries.len());
        Ok(SearchResult {
            entries,
            total_found,
        })
    }
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let email = self
            .email
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Unpaywall requires email"))?;
        let doi = id.trim().strip_prefix("https://doi.org/").unwrap_or(id);
        let url = format!("https://api.unpaywall.org/v2/{}?email={}", doi, email);
        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit-rs/0.1")
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Unpaywall {}: {}", resp.status(), resp.text().await?);
        }
        let d: serde_json::Value = resp.json().await?;
        let mut e = BibEntry::new(EntryType::Article);
        if let Some(t) = d.get("title").and_then(|t| t.as_str()) {
            e.set_field(Field::Title, t.to_string());
        }
        if let Some(doi) = d.get("doi").and_then(|d| d.as_str()) {
            e.set_field(Field::Doi, doi.to_string());
        }
        Ok(e)
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                r.push(b as char)
            }
            b' ' => r.push_str("%20"),
            _ => r.push_str(&format!("%{:02X}", b)),
        }
    }
    r
}
