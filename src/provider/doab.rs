use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Doab;

#[derive(Deserialize)]
struct DoabResp { results: Vec<DoabResult>, total: Option<usize> }
#[derive(Deserialize)]
struct DoabResult { bibjson: DoabBibjson }
#[derive(Deserialize)]
struct DoabBibjson {
    title: Option<String>, author: Option<Vec<DoabAuthor>>,
    year: Option<String>, doi: Option<String>,
    publisher: Option<String>,
}
#[derive(Deserialize)]
struct DoabAuthor { name: Option<String> }

#[async_trait]
impl Provider for Doab {
    fn name(&self) -> &'static str { "DOAB" }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!("https://www.doabooks.org/api/v2/doab?query={}&pageSize={}", urlencoding(query), limit.min(50));
        let resp = reqwest::Client::new()
            .get(&url).header("User-Agent", "jabkit-rs/0.1").send().await?;
        if !resp.status().is_success() { anyhow::bail!("DOAB {}: {}", resp.status(), resp.text().await?); }
        let d: DoabResp = resp.json().await?;
        let entries: Vec<BibEntry> = d.results.into_iter().map(|r| {
            let b = r.bibjson; let mut e = BibEntry::new(EntryType::Book);
            e.set_field(Field::Title, b.title.unwrap_or_default());
            if let Some(a) = b.author { let n: Vec<String> = a.into_iter().filter_map(|a| a.name).collect(); e.set_field(Field::Author, n.join(" and ")); }
            e.set_field(Field::Year, b.year.unwrap_or_default()); e.set_field(Field::Doi, b.doi.unwrap_or_default());
            e.set_field(Field::Publisher, b.publisher.unwrap_or_default()); e
        }).collect();
        let total_found = d.total.unwrap_or(entries.len());
        Ok(SearchResult { entries, total_found: total_found })
    }
    async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> { anyhow::bail!("DOAB ID lookup not supported") }
}
fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() { match b {
        b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'/' => r.push(b as char),
        b' ' => r.push_str("%20"), _ => r.push_str(&format!("%{:02X}", b)),
    }} r
}
