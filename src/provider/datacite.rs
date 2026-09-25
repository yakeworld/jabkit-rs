use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

/// DataCite (dataset DOIs, Zenodo, Figshare, Dryad, ...).
/// fetch_by_id uses Content-Negotiation: Accept: application/x-bibtex.
pub struct DataCite;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DCSearch {
    data: Vec<DCItem>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DCItem {
    id: Option<String>,
    attributes: Option<DCAttrs>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DCAttrs {
    doi: Option<String>,
    titles: Option<Vec<DCTitle>>,
    creators: Option<Vec<DCCreator>>,
    publisher: Option<String>,
    publication_year: Option<serde_json::Value>,
    resource_type: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DCTitle {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DCCreator {
    name: Option<String>,
}

fn map_type(rt: &str) -> EntryType {
    let l = rt.to_lowercase();
    if l.contains("dataset") || l.contains("software") {
        EntryType::Misc
    } else {
        EntryType::Article
    }
}

fn opt_year(v: &serde_json::Value) -> Option<String> {
    v.as_u64().map(|n| n.to_string())
}

/// Parse minimal BibTeX text (single entry) into BibEntry.
fn parse_bibtex(text: &str) -> Option<BibEntry> {
    let start = text.find('{')?;
    let inner = &text[start + 1..];
    // entry_type{key,
    let comma = inner.find(',')?;
    let ty_str = inner[..comma].trim();
    let et = match ty_str {
        "article" => EntryType::Article,
        "book" => EntryType::Book,
        "inproceedings" | "conference" => EntryType::InProceedings,
        "proceedings" => EntryType::Proceedings,
        _ => EntryType::Misc,
    };
    let mut e = BibEntry::new(et);
    // walk field=value pairs, value may be {braced} or "quoted" or number
    let rest: Vec<char> = inner[comma + 1..].chars().collect();
    let n = rest.len();
    let mut i = 0;
    while i < n {
        // skip to field name
        while i < n && !rest[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i >= n {
            break;
        }
        let name_start = i;
        while i < n && (rest[i].is_ascii_alphanumeric() || rest[i] == '-') {
            i += 1;
        }
        let name: String = rest[name_start..i].iter().collect();
        // skip to '='
        while i < n && rest[i] != '=' {
            i += 1;
        }
        if i >= n {
            break;
        }
        i += 1; // skip '='
        let value = if i < n && rest[i] == '{' {
            let depth_start = i;
            let mut depth = 0;
            while i < n {
                if rest[i] == '{' {
                    depth += 1;
                } else if rest[i] == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                i += 1;
            }
            rest[depth_start + 1..i].iter().collect()
        } else if i < n && rest[i] == '"' {
            i += 1;
            let vs = i;
            while i < n && rest[i] != '"' {
                i += 1;
            }
            rest[vs..i].iter().collect()
        } else {
            let vs = i;
            while i < n && rest[i] != ',' && rest[i] != '}' {
                i += 1;
            }
            let s: String = rest[vs..i].iter().collect();
            s.trim().to_string()
        };
        let field = match name.to_lowercase().as_str() {
            "title" => Field::Title,
            "author" => Field::Author,
            "journal" => Field::Journal,
            "year" | "date" => Field::Year,
            "publisher" | "institution" | "school" => Field::Publisher,
            "doi" => Field::Doi,
            "url" => Field::Url,
            "volume" => Field::Volume,
            "number" => Field::Issue,
            "pages" => Field::Pages,
            _ => continue,
        };
        if !value.trim().is_empty() {
            e.set_field(field, value.trim().to_string());
        }
    }
    Some(e)
}

#[async_trait]
impl Provider for DataCite {
    fn name(&self) -> &'static str { "DataCite" }
    fn key_env(&self) -> Option<&'static str> { None }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://api.datacite.org/dois?query={}&pageSize={}",
            urlencoding(query),
            limit.min(20)
        );
        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit/0.1")
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("DataCite {}: {}", resp.status(), resp.text().await?);
        }
        let d: DCSearch = resp.json().await?;
        let entries: Vec<crate::bibtex::BibEntry> = d
            .data
            .into_iter()
            .take(limit)
            .filter_map(|it| {
                let a = it.attributes?;
                let title = a.titles.and_then(|t| t.into_iter().next())?.title?;
                let mut e = BibEntry::new(map_type(a.resource_type.as_deref().unwrap_or("dataset")));
                e.set_field(Field::Title, title);
                if let Some(c) = a.creators {
                    let names: Vec<String> = c.iter().filter_map(|x| x.name.clone()).collect();
                    if !names.is_empty() {
                        e.set_field(Field::Author, names.join(" and "));
                    }
                }
                if let Some(doi) = &a.doi {
                    e.set_field(Field::Doi, doi.clone());
                }
                if let Some(y) = a.publication_year.as_ref().and_then(opt_year) {
                    e.set_field(Field::Year, y);
                }
                if let Some(p) = &a.publisher {
                    e.set_field(Field::Publisher, p.clone());
                }
                if let Some(u) = &a.url {
                    e.set_field(Field::Url, u.clone());
                }
                Some(e)
            })
            .collect();
        let total = entries.len();
        Ok(SearchResult {
            entries,
            total_found: total,
        })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let doi = if id.starts_with("doi:") {
            id[4..].to_string()
        } else if id.contains("10.") {
            id.to_string()
        } else {
            format!("10.5281/zenodo.{}", id)
        };
        let url = format!("https://api.datacite.org/dois/{}", urlencoding(&doi));
        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit/0.1")
            .header("Accept", "application/x-bibtex")
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("DataCite fetch_by_id {}: {}", resp.status(), resp.text().await?);
        }
        let text = resp.text().await?;
        parse_bibtex(&text).ok_or_else(|| anyhow::anyhow!("DataCite BibTeX parse failed for {}", doi))
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
