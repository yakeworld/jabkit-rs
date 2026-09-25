use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::resilient::{get_resilient, KeyPlacement};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Ieee { api_keys: Vec<String> }
impl Ieee {
    pub fn new(keys: Vec<String>) -> Self { Self { api_keys: keys } }
}

#[derive(Deserialize)]
struct IeeeResp {
    #[serde(default)] articles: Vec<IeeeArticle>,
    #[serde(default, deserialize_with = "deserialize_total")] total_records: Option<usize>,
}

fn deserialize_total<'de, D>(d: D) -> Result<Option<usize>, D::Error>
where D: serde::Deserializer<'de> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::String(s) => s.parse().ok(),
        serde_json::Value::Number(n) => n.as_u64().map(|n| n as usize),
        _ => None,
    })
}

fn deserialize_opt_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where D: serde::Deserializer<'de> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct IeeeArticle {
    #[serde(default, deserialize_with = "deserialize_opt_string")] title: Option<String>,
    authors: Option<IeeeAuthors>,
    #[serde(default, deserialize_with = "deserialize_opt_string")] publication_title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_string")] publication_year: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_string")] volume: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_string")] issue: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_string")] start_page: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_string")] doi: Option<String>,
    #[serde(rename = "abstract", default, deserialize_with = "deserialize_opt_string")] abstract_text: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_string")] publisher: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_string")] article_number: Option<String>,
}

#[derive(Deserialize)]
struct IeeeAuthors { authors: Vec<IeeeAuthor> }
#[derive(Deserialize)]
struct IeeeAuthor { full_name: Option<String> }

#[async_trait]
impl Provider for Ieee {
    fn name(&self) -> &'static str { "IEEE" }
    fn key_env(&self) -> Option<&'static str> { Some("IEEE_API_KEY") }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let base = format!(
            "https://ieeexploreapi.ieee.org/api/v1/search/articles?querytext={}&max_records={}",
            urlencoding(query), limit.min(100)
        );
        let resp = get_resilient(
            &|k| format!("{base}&apikey={k}"),
            "jabkit-rs/0.1",
            &self.api_keys,
            KeyPlacement::Baked,
        )
        .await?;
        if !resp.status().is_success() { anyhow::bail!("IEEE {}: {}", resp.status(), resp.text().await?); }
        let d: IeeeResp = resp.json().await?;
        let total = d.total_records.unwrap_or(d.articles.len());
        let entries = d.articles.into_iter().map(|a| {
            let mut e = BibEntry::new(EntryType::Article);
            e.set_field(Field::Title, a.title.unwrap_or_default());
            if let Some(aus) = a.authors {
                let n: Vec<String> = aus.authors.into_iter().filter_map(|a| a.full_name).collect();
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
        }).collect();
        Ok(SearchResult { entries, total_found: total })
    }
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let doi = id.trim().strip_prefix("https://doi.org/").unwrap_or(id);
        let base = format!(
            "https://ieeexploreapi.ieee.org/api/v1/search/articles?doi={}",
            doi
        );
        let resp = get_resilient(
            &|k| format!("{base}&apikey={k}"),
            "jabkit-rs/0.1",
            &self.api_keys,
            KeyPlacement::Baked,
        )
        .await?;
        let d: IeeeResp = resp.json().await?;
        let a = d.articles.into_iter().next().ok_or_else(|| anyhow::anyhow!("No IEEE result for {}", id))?;
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Title, a.title.unwrap_or_default());
        e.set_field(Field::Doi, a.doi.unwrap_or_default());
        e.set_field(Field::Year, a.publication_year.unwrap_or_default());
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
