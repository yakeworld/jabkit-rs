use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Scopus { api_key: Option<String> }
impl Scopus {
    pub fn new(key: Option<String>) -> Self { Self { api_key: key } }
}

#[derive(Deserialize)]
struct ScoResp { #[serde(rename = "search-results")] search_results: Option<ScoResults> }
#[derive(Deserialize)]
struct ScoResults {
    #[serde(rename = "entry")] entry: Vec<ScoEntry>,
    #[serde(rename = "opensearch:totalResults")] total: Option<String>,
}
#[derive(Deserialize)]
struct ScoEntry {
    #[serde(rename = "dc:title")] title: Option<String>,
    #[serde(rename = "dc:creator")] creator: Option<String>,
    #[serde(rename = "prism:publicationName")] journal: Option<String>,
    #[serde(rename = "prism:coverDate")] date: Option<String>,
    #[serde(rename = "prism:volume")] volume: Option<String>,
    #[serde(rename = "prism:issueIdentifier")] issue: Option<String>,
    #[serde(rename = "prism:pageRange")] pages: Option<String>,
    #[serde(rename = "prism:doi")] doi: Option<String>,
    #[serde(rename = "dc:description")] description: Option<String>,
    #[serde(rename = "prism:aggregationType")] agg_type: Option<String>,
    author: Option<Vec<ScoAuthor>>,
}
#[derive(Deserialize)]
struct ScoAuthor { #[serde(rename = "$")] name: Option<String> }

#[async_trait]
impl Provider for Scopus {
    fn name(&self) -> &'static str { "Scopus" }
    fn key_env(&self) -> Option<&'static str> { Some("SCOPUS_API_KEY") }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let key = self.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("Scopus requires SCOPUS_API_KEY"))?;
        let url = format!(
            "https://api.elsevier.com/content/search/scopus?query={}&count={}&apiKey={}",
            urlencoding(query), limit.min(25), key
        );
        let resp = reqwest::Client::new().get(&url).header("User-Agent", "jabkit-rs/0.1")
            .header("X-ELS-APIKey", key).send().await?;
        if !resp.status().is_success() { anyhow::bail!("Scopus {}: {}", resp.status(), resp.text().await?); }
        let d: ScoResp = resp.json().await?;
        let sr = d.search_results.ok_or_else(|| anyhow::anyhow!("Scopus: empty response"))?;
        let total = sr.total.and_then(|t| t.parse().ok()).unwrap_or(sr.entry.len());
        let entries = sr.entry.into_iter().map(|e| {
            let mut entry = BibEntry::new(match e.agg_type.as_deref() {
                Some("Book") => EntryType::Book, _ => EntryType::Article,
            });
            entry.set_field(Field::Title, e.title.unwrap_or_default());
            if let Some(a) = e.author {
                let n: Vec<String> = a.into_iter().filter_map(|a| a.name).collect();
                entry.set_field(Field::Author, n.join(" and "));
            } else { entry.set_field(Field::Author, e.creator.unwrap_or_default()); }
            entry.set_field(Field::Journal, e.journal.unwrap_or_default());
            if let Some(d) = e.date { if d.len() >= 4 { entry.set_field(Field::Year, d[..4].to_string()); } }
            entry.set_field(Field::Volume, e.volume.unwrap_or_default());
            entry.set_field(Field::Issue, e.issue.unwrap_or_default());
            entry.set_field(Field::Pages, e.pages.unwrap_or_default());
            entry.set_field(Field::Doi, e.doi.unwrap_or_default());
            entry.set_field(Field::Abstract, e.description.unwrap_or_default());
            entry
        }).collect();
        Ok(SearchResult { entries, total_found: total })
    }
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        anyhow::bail!("Scopus ID lookup not yet implemented; use Crossref")
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() { match b {
        b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'/' => r.push(b as char),
        b' ' => r.push_str("%20"), _ => r.push_str(&format!("%{:02X}", b)),
    }} r
}
