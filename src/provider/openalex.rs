use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct OpenAlex {
    api_key: Option<String>,
}

impl OpenAlex {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[derive(Debug, Deserialize)]
struct OAResponse {
    results: Vec<OAWork>,
    meta: OAMeta,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OAMeta {
    count: usize,
    #[serde(default)]
    page: Option<usize>,
    #[serde(default)]
    per_page: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OAWork {
    id: Option<String>,
    title: Option<String>,
    authorships: Option<Vec<OAAuthorship>>,
    publication_year: Option<i32>,
    primary_location: Option<OALocation>,
    doi: Option<String>,
    cited_by_count: Option<i32>,
    #[serde(rename = "type")]
    work_type: Option<String>,
    abstract_inverted_index: Option<serde_json::Value>,
    open_access: Option<OAOpenAccess>,
    primary_topic: Option<OATopic>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OAAuthorship {
    author: Option<OAAuthor>,
    author_position: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OAAuthor {
    display_name: Option<String>,
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OALocation {
    source: Option<OASource>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OASource {
    display_name: Option<String>,
    issn_l: Option<String>,
    issn: Option<Vec<String>>,
    type_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OAOpenAccess {
    is_oa: Option<bool>,
    oa_url: Option<String>,
    any_repository_has_fulltext: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OATopic {
    display_name: Option<String>,
    id: Option<String>,
    subfield: Option<OASubfield>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OASubfield {
    display_name: Option<String>,
}

#[async_trait]
impl Provider for OpenAlex {
    fn name(&self) -> &'static str {
        "OpenAlex"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("OPENALEX_API_KEY")
    }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://api.openalex.org/works?search={}&per_page={}&sort=relevance_score:desc",
            urlencoding(query),
            limit.min(200)
        );

        let client = super::http_client();
        let mut req = client.get(&url).header("User-Agent", "jabkit/0.1");
        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("OpenAlex {}: {}", resp.status(), resp.text().await?);
        }

        let oa: OAResponse = resp.json().await?;

        let entries: Vec<BibEntry> = oa
            .results
            .into_iter()
            .map(|r| {
                let mut entry = BibEntry::new(type_from_oa(&r.work_type));

                entry.set_field(Field::Title, r.title.unwrap_or_default());

                if let Some(authorships) = r.authorships {
                    let names: Vec<String> = authorships
                        .into_iter()
                        .filter_map(|a| a.author)
                        .filter_map(|a| a.display_name)
                        .collect();
                    entry.set_field(Field::Author, names.join(" and "));
                }

                if let Some(year) = r.publication_year {
                    entry.set_field(Field::Year, year.to_string());
                }

                if let Some(loc) = r.primary_location {
                    if let Some(src) = loc.source {
                        entry.set_field(Field::Journal, src.display_name.unwrap_or_default());
                    }
                }

                if let Some(doi) = r.doi {
                    entry.set_field(Field::Doi, doi);
                }

                entry
            })
            .collect();

        Ok(SearchResult {
            total_found: oa.meta.count,
            entries,
        })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let url = format!("https://api.openalex.org/works/{}", urlencoding(id));
        let client = super::http_client();
        let resp = client
            .get(&url)
            .header("User-Agent", "jabkit/0.1")
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "OpenAlex fetch_by_id {}: {}",
                resp.status(),
                resp.text().await?
            );
        }

        let r: OAWork = resp.json().await?;
        let mut entry = BibEntry::new(type_from_oa(&r.work_type));
        entry.set_field(Field::Title, r.title.unwrap_or_default());

        if let Some(authorships) = r.authorships {
            let names: Vec<String> = authorships
                .into_iter()
                .filter_map(|a| a.author)
                .filter_map(|a| a.display_name)
                .collect();
            entry.set_field(Field::Author, names.join(" and "));
        }
        if let Some(year) = r.publication_year {
            entry.set_field(Field::Year, year.to_string());
        }
        if let Some(loc) = r.primary_location {
            if let Some(src) = loc.source {
                entry.set_field(Field::Journal, src.display_name.unwrap_or_default());
            }
        }
        if let Some(doi) = r.doi {
            entry.set_field(Field::Doi, doi);
        }

        Ok(entry)
    }
}

fn type_from_oa(typ: &Option<String>) -> EntryType {
    match typ.as_deref() {
        Some("article") | Some("journal-article") => EntryType::Article,
        Some("book-chapter") => EntryType::InBook,
        Some("book") => EntryType::Book,
        Some("proceedings") => EntryType::Proceedings,
        Some("dataset") => EntryType::Misc,
        Some("dissertation") | Some("thesis") => EntryType::Misc,
        _ => EntryType::Article,
    }
}

fn urlencoding(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
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
