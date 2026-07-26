use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct SemanticScholar {
    api_key: Option<String>,
}

impl SemanticScholar {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct S2Response {
    data: Vec<S2Paper>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    next: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct S2Paper {
    paper_id: Option<String>,
    title: Option<String>,
    year: Option<i32>,
    authors: Option<Vec<S2Author>>,
    journal: Option<S2Journal>,
    external_ids: Option<S2ExternalIds>,
    abstract_text: Option<Vec<String>>,
    citation_count: Option<i32>,
    publication_types: Option<Vec<String>>,
    publication_date: Option<String>,
    venue: Option<String>,
    url: Option<String>,
    reference_count: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct S2Author {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct S2Journal {
    name: Option<String>,
    pages: Option<String>,
    volume: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct S2ExternalIds {
    doi: Option<String>,
    arxiv: Option<String>,
    pmid: Option<String>,
    pubmed: Option<String>,
    mag: Option<String>,
    corr_id: Option<String>,
}

#[async_trait]
impl Provider for SemanticScholar {
    fn name(&self) -> &'static str {
        "SemanticScholar"
    }
    fn key_env(&self) -> Option<&'static str> { Some("S2_API_KEY") }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let fields = [
            "title", "year", "authors", "journal", "externalIds",
            "abstract", "citationCount", "publicationTypes",
            "publicationDate", "venue", "url", "referenceCount",
        ]
        .join(",");

        let url = format!(
            "https://api.semanticscholar.org/graph/v1/paper/search?query={}&limit={}&fields={}",
            urlencoding(query),
            limit.min(100),
            fields
        );

        let client = reqwest::Client::new();
        let mut req = client.get(&url).header("User-Agent", "jabkit/0.1");
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await?;
            anyhow::bail!("SemanticScholar API {}: {}", status, body);
        }

        let s2: S2Response = resp.json().await?;

        let entries: Vec<BibEntry> = s2
            .data
            .into_iter()
            .filter_map(|p| {
                let doi = p
                    .external_ids
                    .as_ref()
                    .and_then(|id| id.doi.clone())
                    .unwrap_or_default();
                let pmid = p
                    .external_ids
                    .as_ref()
                    .and_then(|id| id.pubmed.clone())
                    .or_else(|| {
                        p.external_ids
                            .as_ref()
                            .and_then(|id| id.pmid.clone())
                    });
                let arxiv = p
                    .external_ids
                    .as_ref()
                    .and_then(|id| id.arxiv.clone());

                let mut entry = BibEntry::new(EntryType::Article);
                entry.set_field(Field::Title, p.title.unwrap_or_default());
                entry.set_field(Field::Year, p.year.map(|y| y.to_string()).unwrap_or_default());

                if let Some(j) = p.journal {
                    entry.set_field(Field::Journal, j.name.unwrap_or_default());
                    entry.set_field(Field::Volume, j.volume.unwrap_or_default());
                    entry.set_field(Field::Pages, j.pages.unwrap_or_default());
                }

                if let Some(venue) = p.venue {
                    if entry.get(Field::Journal).map(|s| s.is_empty()).unwrap_or(true) {
                        entry.set_field(Field::Journal, venue);
                    }
                }

                if let Some(authors) = p.authors {
                    let names: Vec<String> =
                        authors.into_iter().filter_map(|a| a.name).collect();
                    entry.set_field(Field::Author, names.join(" and "));
                }

                if !doi.is_empty() {
                    entry.set_field(Field::Doi, doi);
                }
                if let Some(pmid) = pmid {
                    entry.set_field(Field::Pmid, pmid);
                }
                if let Some(arxiv) = arxiv {
                    entry.set_field(Field::Eprint, arxiv);
                }
                if let Some(abs) = p.abstract_text {
                    entry.set_field(Field::Abstract, abs.join(" "));
                }
                if let Some(date) = p.publication_date {
                    if date.len() >= 4 {
                        entry.set_field(Field::Year, date[..4].to_string());
                    }
                }

                Some(entry)
            })
            .collect();

        let total = entries.len();
        Ok(SearchResult {
            entries,
            total_found: total,
        })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let url = format!(
            "https://api.semanticscholar.org/graph/v1/paper/{}?fields=title,year,authors,journal,externalIds,abstract,venue",
            id
        );
        let client = reqwest::Client::new();
        let mut req = client.get(&url).header("User-Agent", "jabkit/0.1");
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("S2 fetch_by_id {}: {}", resp.status(), resp.text().await?);
        }

        let mut entry = BibEntry::new(EntryType::Article);
        // Single-object response differs from search — parse minimally
        let v: serde_json::Value = resp.json().await?;

        if let Some(t) = v.get("title").and_then(|t| t.as_str()) {
            entry.set_field(Field::Title, t.to_string());
        }
        if let Some(y) = v.get("year").and_then(|y| y.as_i64()) {
            entry.set_field(Field::Year, y.to_string());
        }
        if let Some(doi) = v
            .get("externalIds")
            .and_then(|e| e.get("DOI"))
            .and_then(|d| d.as_str())
        {
            entry.set_field(Field::Doi, doi.to_string());
        }
        if let Some(j) = v.get("journal").and_then(|j| j.get("name")).and_then(|n| n.as_str()) {
            entry.set_field(Field::Journal, j.to_string());
        }
        if let Some(authors) = v.get("authors").and_then(|a| a.as_array()) {
            let names: Vec<String> = authors
                .iter()
                .filter_map(|a| a.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect();
            entry.set_field(Field::Author, names.join(" and "));
        }

        Ok(entry)
    }
}

fn urlencoding(s: &str) -> String {
    s.split(' ')
        .map(|part| urlencoding_inner(part))
        .collect::<Vec<_>>()
        .join("%20")
}

fn urlencoding_inner(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push_str("%20"),
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}
