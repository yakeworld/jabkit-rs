use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};
use crate::resilient::{get_resilient, KeyPlacement};

pub struct GoogleBooks {
    api_keys: Vec<String>,
}

impl GoogleBooks {
    pub fn new(api_keys: Vec<String>) -> Self {
        Self { api_keys }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GBResponse {
    items: Vec<GBItem>,
    #[serde(rename = "totalItems")]
    total_items: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GBItem {
    #[serde(rename = "volumeInfo")]
    volume_info: Option<GBVolumeInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GBVolumeInfo {
    title: Option<String>,
    subtitle: Option<String>,
    authors: Option<Vec<String>>,
    publisher: Option<String>,
    #[serde(rename = "publishedDate")]
    published_date: Option<String>,
    description: Option<String>,
    #[serde(rename = "industryIdentifiers")]
    industry_identifiers: Option<Vec<GBIdentifier>>,
    #[serde(rename = "printType")]
    print_type: Option<String>,
    #[serde(rename = "canonicalVolumeLink")]
    canonical_volume_link: Option<String>,
    #[serde(rename = "previewLink")]
    preview_link: Option<String>,
    #[serde(rename = "pageCount")]
    page_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GBIdentifier {
    #[serde(rename = "type")]
    ident_type: Option<String>,
    identifier: Option<String>,
}

#[async_trait]
impl Provider for GoogleBooks {
    fn name(&self) -> &'static str { "GoogleBooks" }
    fn key_env(&self) -> Option<&'static str> { Some("GOOGLE_BOOKS_API_KEY") }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let base = format!(
            "https://www.googleapis.com/books/v1/volumes?q={}&maxResults={}",
            urlencoding(query),
            limit.min(40)
        );
        let resp = get_resilient(
            &|k| format!("{base}&key={k}"),
            "jabkit/0.1",
            &self.api_keys,
            KeyPlacement::Baked,
        )
        .await?;
        if !resp.status().is_success() {
            anyhow::bail!("GoogleBooks {}: {}", resp.status(), resp.text().await?);
        }

        let d: GBResponse = resp.json().await?;
        let total = d
            .total_items
            .and_then(|v| v.as_u64())
            .unwrap_or(d.items.len() as u64) as usize;

        let entries = d
            .items
            .into_iter()
            .filter_map(|it| it.volume_info)
            .map(|v| {
                let mut e = BibEntry::new(EntryType::Book);
                e.set_field(Field::Title, v.title.unwrap_or_default());
                if let Some(a) = v.authors {
                    e.set_field(Field::Author, a.join(" and "));
                }
                if let Some(p) = v.publisher {
                    e.set_field(Field::Publisher, p);
                }
                if let Some(d) = v.published_date {
                    if d.len() >= 4 {
                        e.set_field(Field::Year, d[..4].to_string());
                    }
                }
                if let Some(link) = v.canonical_volume_link {
                    e.set_field(Field::Url, link);
                }
                e
            })
            .collect();

        Ok(SearchResult {
            entries,
            total_found: total,
        })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        // id = 书名查询 或 ISBN
        let q = if id.starts_with("ISBN:") {
            id.to_string()
        } else if id.len() == 10 || id.len() == 13 {
            format!("ISBN:{}", id)
        } else {
            id.to_string()
        };
        let base = format!(
            "https://www.googleapis.com/books/v1/volumes?q={}&maxResults=1",
            urlencoding(&q)
        );
        let resp = get_resilient(
            &|k| format!("{base}&key={k}"),
            "jabkit/0.1",
            &self.api_keys,
            KeyPlacement::Baked,
        )
        .await?;
        if !resp.status().is_success() {
            anyhow::bail!("GoogleBooks fetch_by_id {}: {}", resp.status(), resp.text().await?);
        }
        let d: GBResponse = resp.json().await?;
        let v = d
            .items
            .into_iter()
            .next()
            .and_then(|it| it.volume_info)
            .ok_or_else(|| anyhow::anyhow!("No GoogleBooks entry for: {}", id))?;
        let mut e = BibEntry::new(EntryType::Book);
        e.set_field(Field::Title, v.title.unwrap_or_default());
        if let Some(a) = v.authors {
            e.set_field(Field::Author, a.join(" and "));
        }
        if let Some(p) = v.publisher {
            e.set_field(Field::Publisher, p);
        }
        if let Some(d) = v.published_date {
            if d.len() >= 4 {
                e.set_field(Field::Year, d[..4].to_string());
            }
        }
        Ok(e)
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                r.push(b as char);
            }
            b' ' => r.push_str("%20"),
            _ => r.push_str(&format!("%{:02X}", b)),
        }
    }
    r
}
