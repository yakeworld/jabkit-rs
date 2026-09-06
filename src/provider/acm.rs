use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Acm { api_key: Option<String> }
impl Acm {
    pub fn new(key: Option<String>) -> Self { Self { api_key: key } }
}

#[derive(Deserialize)]
struct AcmResp { records: Option<Vec<AcmRecord>>, total: Option<usize>, }
#[derive(Deserialize)]
#[allow(non_snake_case)]
struct AcmRecord {
    title: Option<String>, author: Option<Vec<AcmAuthor>>,
    publication: Option<String>, publicationDate: Option<String>,
    volume: Option<String>, issue: Option<String>, pages: Option<String>,
    doi: Option<String>, abstract_text: Option<String>, publisher: Option<String>,
}
#[derive(Deserialize)]
struct AcmAuthor { name: Option<String> }

#[async_trait]
impl Provider for Acm {
    fn name(&self) -> &'static str { "ACM" }
    fn key_env(&self) -> Option<&'static str> { Some("ACM_API_KEY") }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let key = self.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("ACM requires ACM_API_KEY"))?;
        let url = format!(
            "https://api.acm.org/api/v1/search?query={}&pageSize={}",
            urlencoding(query), limit.min(50)
        );
        let resp = super::http_client().get(&url)
            .header("User-Agent", "jabkit-rs/0.1")
            .header("ApiKey", key)
            .send().await?;
        if !resp.status().is_success() { anyhow::bail!("ACM {}: {}", resp.status(), resp.text().await?); }
        let d: AcmResp = resp.json().await?;
        let records = d.records.unwrap_or_default();
        let entries: Vec<BibEntry> = records.into_iter().map(|r| {
            let mut e = BibEntry::new(EntryType::Article);
            e.set_field(Field::Title, r.title.unwrap_or_default());
            if let Some(a) = r.author {
                let n: Vec<String> = a.into_iter().filter_map(|a| a.name).collect();
                e.set_field(Field::Author, n.join(" and "));
            }
            e.set_field(Field::Journal, r.publication.unwrap_or_default());
            if let Some(d) = r.publicationDate { if d.len() >= 4 { e.set_field(Field::Year, d[..4].to_string()); } }
            e.set_field(Field::Volume, r.volume.unwrap_or_default());
            e.set_field(Field::Issue, r.issue.unwrap_or_default());
            e.set_field(Field::Pages, r.pages.unwrap_or_default());
            e.set_field(Field::Doi, r.doi.unwrap_or_default());
            e.set_field(Field::Abstract, r.abstract_text.unwrap_or_default());
            e.set_field(Field::Publisher, r.publisher.unwrap_or_default());
            e
        }).collect();
        let total_found = d.total.unwrap_or(entries.len());
        Ok(SearchResult { entries, total_found: total_found })
    }
    async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> {
        anyhow::bail!("ACM ID lookup: use Crossref instead")
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() { match b {
        b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'/' => r.push(b as char),
        b' ' => r.push_str("%20"), _ => r.push_str(&format!("%{:02X}", b)),
    }} r
}
