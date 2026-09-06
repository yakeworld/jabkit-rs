use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Springer { api_key: Option<String> }
impl Springer {
    pub fn new(key: Option<String>) -> Self { Self { api_key: key } }
}

#[derive(Deserialize)]
struct SprResp { records: Vec<SprRecord>, result: Option<SprResult> }
#[derive(Deserialize)]
struct SprResult { total: Option<usize> }
#[derive(Deserialize)]
#[allow(non_snake_case, dead_code)]
struct SprRecord {
    title: Option<String>, creators: Option<Vec<SprCreator>>,
    publicationName: Option<String>, publicationDate: Option<String>,
    volume: Option<String>, number: Option<String>, startingPage: Option<String>,
    doi: Option<String>, url: Option<SprUrl>, #[serde(rename = "abstract")] abs: Option<String>,
    publisher: Option<String>, genre: Option<String>,
}
#[derive(Deserialize)]
struct SprCreator { creator: Option<String> }
#[derive(Deserialize)]
#[allow(dead_code)]
struct SprUrl { value: Option<String> }

#[async_trait]
impl Provider for Springer {
    fn name(&self) -> &'static str { "Springer" }
    fn key_env(&self) -> Option<&'static str> { Some("SPRINGER_API_KEY") }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let key = self.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("Springer requires SPRINGER_API_KEY"))?;
        let url = format!(
            "https://api.springernature.com/meta/v2/json?q={}&api_key={}&s={}",
            urlencoding(query), key, limit.min(50)
        );
        let resp = super::http_client().get(&url).header("User-Agent", "jabkit-rs/0.1").send().await?;
        if !resp.status().is_success() { anyhow::bail!("Springer {}: {}", resp.status(), resp.text().await?); }
        let d: SprResp = resp.json().await?;
        let total = d.result.and_then(|r| r.total).unwrap_or(d.records.len());
        let entries = d.records.into_iter().map(|r| {
            let mut e = BibEntry::new(match r.genre.as_deref() {
                Some("book") => EntryType::Book, _ => EntryType::Article,
            });
            e.set_field(Field::Title, r.title.unwrap_or_default());
            if let Some(c) = r.creators {
                let n: Vec<String> = c.into_iter().filter_map(|c| c.creator).collect();
                e.set_field(Field::Author, n.join(" and "));
            }
            e.set_field(Field::Journal, r.publicationName.unwrap_or_default());
            if let Some(d) = r.publicationDate { if d.len() >= 4 { e.set_field(Field::Year, d[..4].to_string()); } }
            e.set_field(Field::Volume, r.volume.unwrap_or_default());
            e.set_field(Field::Issue, r.number.unwrap_or_default());
            e.set_field(Field::Pages, r.startingPage.unwrap_or_default());
            e.set_field(Field::Doi, r.doi.unwrap_or_default());
            e.set_field(Field::Abstract, r.abs.unwrap_or_default());
            e.set_field(Field::Publisher, r.publisher.unwrap_or_default());
            e
        }).collect();
        Ok(SearchResult { entries, total_found: total })
    }
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let key = self.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("Springer requires API key"))?;
        let doi = id.trim().strip_prefix("https://doi.org/").unwrap_or(id);
        let url = format!("https://api.springernature.com/meta/v2/json?q=doi:{}&api_key={}", doi, key);
        let resp = super::http_client().get(&url).header("User-Agent", "jabkit-rs/0.1").send().await?;
        let d: SprResp = resp.json().await?;
        let r = d.records.into_iter().next().ok_or_else(|| anyhow::anyhow!("No Springer result"))?;
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Title, r.title.unwrap_or_default());
        e.set_field(Field::Doi, r.doi.unwrap_or_default());
        Ok(e)
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() { match b {
        b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'/' => r.push(b as char),
        b' ' => r.push_str("%20"), _ => r.push_str(&format!("%{:02X}", b)),
    }} r
}
