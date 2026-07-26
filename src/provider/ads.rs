use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Ads { api_key: Option<String> }
impl Ads {
    pub fn new(key: Option<String>) -> Self { Self { api_key: key } }
}

#[derive(Deserialize)]
struct AdsResp { response: AdsResponse }
#[derive(Deserialize)]
#[allow(non_snake_case)]
struct AdsResponse {
    docs: Vec<AdsDoc>, numFound: Option<usize>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct AdsDoc {
    title: Option<Vec<String>>, author: Option<Vec<String>>,
    bibcode: Option<String>, doi: Option<Vec<String>>,
    year: Option<String>, #[serde(rename = "pub")] pub_field: Option<String>, volume: Option<String>,
    issue: Option<String>, page: Option<Vec<String>>,
    abstract_text: Option<Vec<String>>, publisher: Option<String>,
}

#[async_trait]
impl Provider for Ads {
    fn name(&self) -> &'static str { "ADS" }
    fn key_env(&self) -> Option<&'static str> { Some("ADS_API_KEY") }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let key = self.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("ADS requires ADS_API_KEY"))?;
        let url = format!("https://api.adsabs.harvard.edu/v1/search/query?q={}&rows={}", urlencoding(query), limit.min(100));
        let resp = reqwest::Client::new().get(&url)
            .header("User-Agent", "jabkit-rs/0.1")
            .header("Authorization", format!("Bearer {}", key))
            .send().await?;
        if !resp.status().is_success() { anyhow::bail!("ADS {}: {}", resp.status(), resp.text().await?); }
        let d: AdsResp = resp.json().await?;
        let total = d.response.numFound.unwrap_or(d.response.docs.len());
        let entries = d.response.docs.into_iter().map(|d| {
            let mut e = BibEntry::new(EntryType::Article);
            e.set_field(Field::Title, d.title.and_then(|t| t.into_iter().next()).unwrap_or_default());
            if let Some(a) = d.author { e.set_field(Field::Author, a.join(" and ")); }
            e.set_field(Field::Journal, d.pub_field.unwrap_or_default());
            e.set_field(Field::Year, d.year.unwrap_or_default());
            e.set_field(Field::Volume, d.volume.unwrap_or_default());
            e.set_field(Field::Issue, d.issue.unwrap_or_default());
            e.set_field(Field::Pages, d.page.and_then(|p| p.into_iter().next()).unwrap_or_default());
            if let Some(dois) = d.doi { if let Some(doi) = dois.into_iter().next() { e.set_field(Field::Doi, doi); } }
            e.set_field(Field::Publisher, d.publisher.unwrap_or_default());
            e
        }).collect();
        Ok(SearchResult { entries, total_found: total })
    }
    async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> {
        anyhow::bail!("ADS ID lookup not yet implemented")
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() { match b {
        b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'/' => r.push(b as char),
        b' ' => r.push_str("%20"), _ => r.push_str(&format!("%{:02X}", b)),
    }} r
}
