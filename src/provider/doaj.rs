use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Doaj;

#[derive(Debug, Deserialize)]
struct DoajResponse {
    results: Vec<DoajResult>,
    total: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct DoajResult {
    bibjson: DoajBibjson,
}

#[derive(Debug, Deserialize)]
struct DoajBibjson {
    title: Option<String>,
    author: Option<Vec<DoajAuthor>>,
    year: Option<String>,
    journal: Option<DoajJournal>,
    doi: Option<String>,
    volume: Option<String>,
    number: Option<String>,
    pages: Option<String>,
    abstract_text: Option<String>,
    link: Option<Vec<DoajLink>>,
    issn: Option<String>,
    eissn: Option<String>,
    publisher: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoajAuthor {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoajJournal {
    name: Option<String>,
    issn: Option<String>,
    volume: Option<String>,
    number: Option<String>,
    pages: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoajLink {
    url: Option<String>,
}

#[async_trait]
impl Provider for Doaj {
    fn name(&self) -> &'static str { "DOAJ" }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://doaj.org/api/v2/search/articles/{}?pageSize={}",
            urlencoding(query), limit.min(100)
        );
        let resp = reqwest::Client::new()
            .get(&url).header("User-Agent", "jabkit-rs/0.1")
            .send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("DOAJ {}: {}", resp.status(), resp.text().await?);
        }
        let d: DoajResponse = resp.json().await?;
        let total = d.total.unwrap_or(d.results.len());
        let entries = d.results.into_iter().map(|r| {
            let b = r.bibjson;
            let mut e = BibEntry::new(EntryType::Article);
            e.set_field(Field::Title, b.title.unwrap_or_default());
            if let Some(a) = b.author {
                let names: Vec<String> = a.into_iter().filter_map(|a| a.name).collect();
                e.set_field(Field::Author, names.join(" and "));
            }
            e.set_field(Field::Year, b.year.unwrap_or_default());
            e.set_field(Field::Doi, b.doi.unwrap_or_default());
            if let Some(j) = b.journal {
                e.set_field(Field::Journal, j.name.unwrap_or_default());
            }
            e.set_field(Field::Volume, b.volume.unwrap_or_default());
            e.set_field(Field::Issue, b.number.unwrap_or_default());
            e.set_field(Field::Pages, b.pages.unwrap_or_default());
            e.set_field(Field::Abstract, b.abstract_text.unwrap_or_default());
            e.set_field(Field::Publisher, b.publisher.unwrap_or_default());
            e
        }).collect();

        Ok(SearchResult { entries, total_found: total })
    }

    async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> {
        anyhow::bail!("DOAJ does not support ID lookup")
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => r.push(b as char),
            b' ' => r.push_str("%20"),
            _ => r.push_str(&format!("%{:02X}", b)),
        }
    }
    r
}
