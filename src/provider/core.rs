use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Core {
    api_key: Option<String>,
}

impl Core {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CoreResponse {
    total_hits: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
    results: Vec<CoreResult>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CoreResult {
    title: Option<String>,
    doi: Option<String>,
    year: Option<i64>,
    authors: Option<Vec<CoreAuthor>>,
    journal_name: Option<String>,
    publisher: Option<String>,
    document_type: Option<String>,
    abstract_text: Option<String>,
    language: Option<CoreLanguage>,
    outputs: Option<Vec<String>>,
    citation_count: Option<i64>,
    date_published: Option<String>,
    fields_of_study: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CoreAuthor {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CoreLanguage {
    code: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct CoreQuery {
    q: String,
    limit: i64,
    offset: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<Vec<String>>,
}

#[async_trait]
impl Provider for Core {
    fn name(&self) -> &'static str { "CORE" }
    fn key_env(&self) -> Option<&'static str> { Some("CORE_API_KEY") }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let body = CoreQuery {
            q: query.to_string(),
            limit: limit.min(100) as i64,
            offset: 0,
            fields: Some(vec![
                "title".into(), "doi".into(), "year".into(), "authors".into(),
                "journalName".into(), "publisher".into(), "documentType".into(),
                "abstract".into(), "language".into(), "outputs".into(),
                "citationCount".into(), "datePublished".into(), "fieldsOfStudy".into(),
            ]),
        };

        let client = super::http_client();
        let mut req = client
            .post("https://api.core.ac.uk/v3/search/works")
            .header("Content-Type", "application/json")
            .json(&body);

        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let s = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("CORE API {}: {}", s, text);
        }

        let cr: CoreResponse = resp.json().await?;
        let total_found = cr.total_hits.unwrap_or(0) as usize;

        let entries: Vec<BibEntry> = cr.results.into_iter().filter_map(|r| {
            let doi = r.doi.as_deref().unwrap_or("").to_string();
            let pmid = r.outputs.as_ref()
                .and_then(|o| o.first())
                .and_then(|u| {
                    if u.starts_with("https://api.core.ac.uk/v3/outputs/") {
                        u.trim_start_matches("https://api.core.ac.uk/v3/outputs/").to_string().into()
                    } else { None }
                });

            let mut entry = BibEntry::new(EntryType::Article);
            entry.set_field(Field::Title, r.title.unwrap_or_default());
            if let Some(y) = r.year {
                entry.set_field(Field::Year, y.to_string());
            }
            if let Some(d) = r.date_published {
                if d.len() >= 4 {
                    entry.set_field(Field::Year, d[..4].to_string());
                }
            }
            if let Some(authors) = r.authors {
                let names: Vec<String> = authors.into_iter().filter_map(|a| a.name).collect();
                entry.set_field(Field::Author, names.join(" and "));
            }
            entry.set_field(Field::Journal, r.journal_name.unwrap_or_default());
            entry.set_field(Field::Publisher, r.publisher.unwrap_or_default());
            entry.set_field(Field::Doi, doi);
            entry.set_field(Field::Abstract, r.abstract_text.unwrap_or_default());
            if let Some(c) = r.citation_count {
                entry.set_field(Field::Note, format!("cited:{}", c));
            }
            Some(entry)
        }).collect();

        Ok(SearchResult { entries, total_found })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let client = super::http_client();
        let mut req = client.get(format!("https://api.core.ac.uk/v3/outputs/{}", id));
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("CORE fetch_by_id {}: {}", resp.status(), resp.text().await?);
        }
        anyhow::bail!("CORE fetch_by_id not fully implemented; use search instead")
    }
}
