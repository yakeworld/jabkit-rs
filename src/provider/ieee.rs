use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

pub struct Ieee {
    api_key: Option<String>,
}
impl Ieee {
    pub fn new(key: Option<String>) -> Self {
        Self { api_key: key }
    }
}

#[derive(Deserialize)]
struct IeeeResp {
    #[serde(default)]
    articles: Vec<IeeeArticle>,
    #[serde(default)]
    total_records: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct IeeeArticle {
    title: Option<String>,
    authors: Option<IeeeAuthors>,
    publication_title: Option<String>,
    publication_year: Option<String>,
    volume: Option<String>,
    issue: Option<String>,
    start_page: Option<String>,
    doi: Option<String>,
    abstract_text: Option<String>,
    publisher: Option<String>,
    article_number: Option<String>,
}

#[derive(Deserialize)]
struct IeeeAuthors {
    authors: Vec<IeeeAuthor>,
}
#[derive(Deserialize)]
struct IeeeAuthor {
    full_name: Option<String>,
}

#[async_trait]
impl Provider for Ieee {
    fn name(&self) -> &'static str {
        "IEEE"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("IEEE_API_KEY")
    }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let key = self
            .api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("IEEE requires IEEE_API_KEY"))?;
        let url = format!(
            "https://ieeexploreapi.ieee.org/api/v1/search/articles?querytext={}&max_records={}&apikey={}",
            urlencoding(query), limit.min(100), key
        );
        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit-rs/0.1")
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("IEEE {}: {}", resp.status(), resp.text().await?);
        }
        let d: IeeeResp = resp.json().await?;
        let total = d
            .total_records
            .unwrap_or_default()
            .parse()
            .unwrap_or(d.articles.len());
        let entries = d
            .articles
            .into_iter()
            .map(|a| {
                let mut e = BibEntry::new(EntryType::Article);
                e.set_field(Field::Title, a.title.unwrap_or_default());
                if let Some(aus) = a.authors {
                    let n: Vec<String> = aus
                        .authors
                        .into_iter()
                        .filter_map(|a| a.full_name)
                        .collect();
                    e.set_field(Field::Author, n.join(" and "));
                }
                e.set_field(Field::Journal, a.publication_title.unwrap_or_default());
                e.set_field(Field::Year, a.publication_year.unwrap_or_default());
                e.set_field(Field::Volume, a.volume.unwrap_or_default());
                e.set_field(Field::Issue, a.issue.unwrap_or_default());
                e.set_field(Field::Pages, a.start_page.unwrap_or_default());
                e.set_field(Field::Doi, a.doi.unwrap_or_default());
                e.set_field(Field::Abstract, a.abstract_text.unwrap_or_default());
                e.set_field(Field::Publisher, a.publisher.unwrap_or_default());
                e
            })
            .collect();
        Ok(SearchResult {
            entries,
            total_found: total,
        })
    }
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let key = self
            .api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("IEEE requires API key"))?;
        let doi = id.trim().strip_prefix("https://doi.org/").unwrap_or(id);
        let url = format!(
            "https://ieeexploreapi.ieee.org/api/v1/search/articles?doi={}&apikey={}",
            doi, key
        );
        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit-rs/0.1")
            .send()
            .await?;
        let d: IeeeResp = resp.json().await?;
        let a = d
            .articles
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No IEEE result for {}", id))?;
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Title, a.title.unwrap_or_default());
        e.set_field(Field::Doi, a.doi.unwrap_or_default());
        e.set_field(Field::Year, a.publication_year.unwrap_or_default());
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
