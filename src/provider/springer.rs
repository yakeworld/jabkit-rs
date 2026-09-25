use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::resilient::{get_resilient, KeyPlacement};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Springer { api_keys: Vec<String> }
impl Springer {
    pub fn new(keys: Vec<String>) -> Self { Self { api_keys: keys } }
}

#[derive(Deserialize)]
struct SprResp { records: Vec<SprRecord>, result: Option<Vec<SprResultItem>> }
#[derive(Deserialize)]
struct SprResultItem { total: Option<String> }
#[derive(Deserialize)]
#[allow(non_snake_case, dead_code)]
struct SprRecord {
    #[serde(default)] title: Option<String>,
    #[serde(default)] creators: Option<Vec<SprCreator>>,
    #[serde(default)] bookEditors: Option<Vec<SprBookEditor>>,
    #[serde(default)] publicationName: Option<String>,
    #[serde(default)] publicationDate: Option<String>,
    #[serde(default)] volume: Option<String>,
    #[serde(default)] number: Option<String>,
    #[serde(default)] startingPage: Option<String>,
    #[serde(default)] doi: Option<String>,
    #[serde(default)] identifier: Option<String>,
    #[serde(default)] url: Option<Vec<SprUrl>>,
    #[serde(rename = "abstract", default)] abs: Option<String>,
    #[serde(default)] publisher: Option<String>,
    #[serde(default)] genre: Option<serde_json::Value>,
    #[serde(default)] contentType: Option<String>,
    #[serde(default)] openaccess: Option<String>,
    #[serde(default)] conferenceInfo: Option<Vec<serde_json::Value>>,
    #[serde(default)] keyword: Option<Vec<String>>,
    #[serde(default)] subjects: Option<Vec<serde_json::Value>>,
    #[serde(default)] disciplines: Option<Vec<serde_json::Value>>,
    #[serde(default)] sntsubjects: Option<Vec<serde_json::Value>>,
    #[serde(default)] printIsbn: Option<String>,
    #[serde(default)] electronicIsbn: Option<String>,
    #[serde(default)] isbn: Option<String>,
    #[serde(default)] seriesId: Option<String>,
    #[serde(default)] onlineDate: Option<String>,
    #[serde(default)] copyright: Option<String>,
    #[serde(default)] language: Option<String>,
    #[serde(default)] publicationType: Option<String>,
    #[serde(default)] publisherName: Option<String>,
    #[serde(default)] query: Option<String>,
    #[serde(default)] apiMessage: Option<String>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
struct SprBookEditor { bookEditor: Option<String> }
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
        let base = format!(
            "https://api.springernature.com/meta/v2/json?q={}&s={}",
            urlencoding(query), limit.min(50)
        );
        let resp = get_resilient(
            &|k| format!("{base}&api_key={k}"),
            "jabkit-rs/0.1",
            &self.api_keys,
            KeyPlacement::Baked,
        )
        .await?;
        if !resp.status().is_success() { anyhow::bail!("Springer {}: {}", resp.status(), resp.text().await?); }
        let d: SprResp = resp.json().await?;
        let total = d.result.as_ref()
            .and_then(|items| items.first())
            .and_then(|i| i.total.as_ref())
            .and_then(|t| t.parse::<usize>().ok())
            .unwrap_or(d.records.len());
        let entries = d.records.into_iter().map(|r| {
            let genre_str = r.genre.as_ref().and_then(|g| g.as_str()).unwrap_or("");
            let is_book = genre_str.contains("book") || genre_str.contains("Book");
            let mut e = BibEntry::new(if is_book { EntryType::Book } else { EntryType::Article });
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
            // DOI: 从 identifier 字段提取 (格式 "doi:10.xxxx")，回退到 doi 字段
            let doi = r.identifier.as_ref()
                .and_then(|id| id.strip_prefix("doi:").map(String::from))
                .or_else(|| r.doi.clone())
                .unwrap_or_default();
            e.set_field(Field::Doi, doi);
            e.set_field(Field::Abstract, r.abs.unwrap_or_default());
            e.set_field(Field::Publisher, r.publisher.unwrap_or_default());
            e
        }).collect();
        Ok(SearchResult { entries, total_found: total })
    }
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let doi = id.trim().strip_prefix("https://doi.org/").unwrap_or(id);
        let base = format!(
            "https://api.springernature.com/meta/v2/json?q=doi:{}",
            doi
        );
        let resp = get_resilient(
            &|k| format!("{base}&api_key={k}"),
            "jabkit-rs/0.1",
            &self.api_keys,
            KeyPlacement::Baked,
        )
        .await?;
        let d: SprResp = resp.json().await?;
        let r = d.records.into_iter().next().ok_or_else(|| anyhow::anyhow!("No Springer result"))?;
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Title, r.title.unwrap_or_default());
        let out_doi = r.identifier.as_ref()
            .and_then(|id| id.strip_prefix("doi:").map(String::from))
            .or_else(|| r.doi.clone())
            .unwrap_or_default();
        e.set_field(Field::Doi, out_doi);
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
