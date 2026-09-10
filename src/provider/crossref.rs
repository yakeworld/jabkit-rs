use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct CrossRef;

#[derive(Debug, Deserialize)]
struct CrossrefResponse {
    message: CrossrefMessage,
}

#[derive(Debug, Deserialize)]
struct CrossrefMessage {
    items: Vec<CrossrefItem>,
    #[serde(default)]
    total_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
struct CrossrefItem {
    title: Option<Vec<String>>,
    author: Option<Vec<CrossrefAuthor>>,
    #[serde(rename = "container-title")]
    container_title: Option<Vec<String>>,
    volume: Option<String>,
    page: Option<String>,
    #[serde(rename = "published-print")]
    published_print: Option<CrossrefDate>,
    #[serde(rename = "published-online")]
    published_online: Option<CrossrefDate>,
    #[serde(rename = "issued")]
    issued: Option<CrossrefDate>,
    DOI: Option<String>,
    PMID: Option<String>,
    #[serde(rename = "type")]
    pub_type: Option<String>,
    subtitle: Option<Vec<String>>,
    ISSN: Option<Vec<String>>,
    URL: Option<String>,
    abstract_text: Option<String>,
    publisher: Option<String>,
    issue: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CrossrefAuthor {
    given: Option<String>,
    family: Option<String>,
    sequence: Option<String>,
    affiliation: Option<Vec<CrossrefAffiliation>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CrossrefAffiliation {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrossrefDate {
    #[serde(rename = "date-parts")]
    date_parts: Option<Vec<Vec<Option<i32>>>>,
}

#[async_trait]
impl Provider for CrossRef {
    fn name(&self) -> &'static str {
        "Crossref"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://api.crossref.org/works?query={}&rows={}",
            urlencoding(query),
            limit.min(100)
        );

        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit/0.1 (mailto:yakeworld@gmail.com)")
            .send()
            .await?;

        if !resp.status().is_success() {
            let s = resp.status();
            let body = resp.text().await?;
            anyhow::bail!("Crossref API {}: {}", s, body);
        }

        let cr: CrossrefResponse = resp.json().await?;
        let total_found = cr.message.total_results.unwrap_or(0);

        let entries: Vec<BibEntry> = cr
            .message
            .items
            .into_iter()
            .map(|item| {
                let mut entry = BibEntry::new(type_from_crossref(&item.pub_type));

                // Title
                if let Some(titles) = item.title {
                    let t = titles.join(". ");
                    entry.set_field(Field::Title, t);
                }
                // Subtitle appended to title if present
                if let Some(subs) = item.subtitle {
                    let existing = entry.get(Field::Title).unwrap_or_default();
                    if !subs.is_empty() {
                        entry.set_field(Field::Title, format!("{}: {}", existing, subs.join(". ")));
                    }
                }

                // Authors
                if let Some(authors) = item.author {
                    let names: Vec<String> = authors
                        .into_iter()
                        .map(|a| {
                            let family = a.family.unwrap_or_default();
                            let given = a.given.unwrap_or_default();
                            if given.is_empty() {
                                family
                            } else {
                                format!("{}, {}", family, given)
                            }
                        })
                        .collect();
                    entry.set_field(Field::Author, names.join(" and "));
                }

                set_container(&mut entry, &item.container_title);
                entry.set_field(Field::Volume, item.volume.unwrap_or_default());
                entry.set_field(Field::Issue, item.issue.unwrap_or_default());
                entry.set_field(Field::Pages, item.page.unwrap_or_default());
                entry.set_field(Field::Publisher, item.publisher.unwrap_or_default());

                if let Some(d) = item
                    .issued
                    .or(item.published_print)
                    .or(item.published_online)
                {
                    if let Some(parts) = d.date_parts {
                        if let Some(p) = parts.first() {
                            if let Some(Some(year)) = p.first() {
                                entry.set_field(Field::Year, year.to_string());
                            }
                        }
                    }
                }

                if let Some(doi) = item.DOI {
                    entry.set_field(Field::Doi, doi);
                }
                if let Some(pmid) = item.PMID {
                    entry.set_field(Field::Pmid, pmid);
                }
                if let Some(abs_bits) = item.abstract_text {
                    // CrossRef returns abstract as HTML bits
                    let clean = abs_bits.replace("<jats:p>", "").replace("</jats:p>", "");
                    entry.set_field(Field::Abstract, clean);
                }

                entry
            })
            .collect();

        Ok(SearchResult {
            entries,
            total_found,
        })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        // id = DOI
        let doi = id.trim().strip_prefix("https://doi.org/").unwrap_or(id);
        let url = format!("https://api.crossref.org/works/{}", urlencoding(doi));

        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit/0.1 (mailto:yakeworld@gmail.com)")
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Crossref fetch_by_id {}: {}",
                resp.status(),
                resp.text().await?
            );
        }

        // Single-work response: {"message": {...work object...}} — no "items" wrapper
        let raw: serde_json::Value = resp.json().await?;
        let msg = raw
            .get("message")
            .ok_or_else(|| anyhow::anyhow!("Missing 'message' in CrossRef response"))?;

        let item: CrossrefItem = serde_json::from_value(msg.clone())
            .context("Failed to parse CrossRef single-item response")?;

        let mut entry = BibEntry::new(type_from_crossref(&item.pub_type));

        if let Some(titles) = item.title {
            entry.set_field(Field::Title, titles.join(". "));
        }
        // Subtitle appended to title if present
        if let Some(subs) = item.subtitle {
            let existing = entry.get(Field::Title).unwrap_or_default();
            if !subs.is_empty() {
                entry.set_field(Field::Title, format!("{}: {}", existing, subs.join(". ")));
            }
        }
        if let Some(authors) = item.author {
            let names: Vec<String> = authors
                .into_iter()
                .map(|a| {
                    let family = a.family.unwrap_or_default();
                    let given = a.given.unwrap_or_default();
                    if given.is_empty() {
                        family
                    } else {
                        format!("{}, {}", family, given)
                    }
                })
                .collect();
            entry.set_field(Field::Author, names.join(" and "));
        }

        set_container(&mut entry, &item.container_title);
        entry.set_field(Field::Volume, item.volume.unwrap_or_default());
        entry.set_field(Field::Issue, item.issue.unwrap_or_default());
        entry.set_field(Field::Pages, item.page.unwrap_or_default());
        entry.set_field(Field::Publisher, item.publisher.unwrap_or_default());

        if let Some(d) = item
            .issued
            .or(item.published_print)
            .or(item.published_online)
        {
            if let Some(parts) = d.date_parts {
                if let Some(p) = parts.first() {
                    if let Some(Some(year)) = p.first() {
                        entry.set_field(Field::Year, year.to_string());
                    }
                }
            }
        }

        if let Some(doi) = item.DOI {
            entry.set_field(Field::Doi, doi);
        }
        if let Some(pmid) = item.PMID {
            entry.set_field(Field::Pmid, pmid);
        }
        if let Some(abs_bits) = item.abstract_text {
            // CrossRef returns abstract as HTML bits
            let clean = abs_bits.replace("<jats:p>", "").replace("</jats:p>", "");
            entry.set_field(Field::Abstract, clean);
        }

        Ok(entry)
    }
}

fn type_from_crossref(typ: &Option<String>) -> EntryType {
    match typ.as_deref() {
        Some("journal-article") => EntryType::Article,
        // A book chapter with its own title + book container → incollection
        // (its standard field set includes `booktitle`).
        Some("book-chapter") => EntryType::InCollection,
        Some("book") | Some("monograph") => EntryType::Book,
        Some("proceedings-article") => EntryType::InProceedings,
        Some("proceedings") => EntryType::Proceedings,
        Some("dissertation") | Some("thesis") => EntryType::Misc,
        Some("dataset") => EntryType::Misc,
        // Unknown Crossref type: do NOT pretend it is a journal article; fall
        // back to Misc and keep the original type visible in a note.
        _ => EntryType::Misc,
    }
}

/// Set the container-title field based on entry type.
/// Book chapters / in-collections → `booktitle`; articles → `journal`;
/// unknown types (Misc) keep the container as a `note` so it is not lost.
fn set_container(entry: &mut BibEntry, container_title: &Option<Vec<String>>) {
    if let Some(ct) = container_title {
        let joined = ct.join("; ");
        if joined.is_empty() {
            return;
        }
        match entry.entry_type {
            EntryType::InCollection => entry.set_field(Field::Booktitle, joined),
            EntryType::Book => {
                // A container title for a whole book is usually the series —
                // keep it as a note rather than a journal name.
                entry.set_field(Field::Note, joined)
            }
            EntryType::Article | EntryType::InProceedings | EntryType::Proceedings => {
                entry.set_field(Field::Journal, joined)
            }
            EntryType::Misc => {
                // Unknown type: do not pretend the container is a journal.
                entry.set_field(Field::Note, joined)
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bibtex::Field;

    #[test]
    fn test_crossref_book_chapter_maps_to_incollection() {
        // book-chapter with its own title + book container → incollection
        // (standard field set includes booktitle; the earlier inbook mapping
        // risked dropping the book name because traditional BibTeX's inbook
        // field set does not include booktitle).
        let json = r#"{
            "title": ["Chapter One"],
            "container-title": ["Handbook of Vestibular Research"],
            "type": "book-chapter",
            "author": [{"given": "Jane", "family": "Doe"}],
            "published-print": {"date-parts": [[2023]]},
            "DOI": "10.1234/chap.1"
        }"#;
        let meta: CrossrefItem = serde_json::from_str(json).unwrap();
        let etype = type_from_crossref(&meta.pub_type);
        assert_eq!(etype, EntryType::InCollection);
        // Container title must route to booktitle for incollection.
        let mut entry = BibEntry::new(etype.clone());
        entry.set_field(Field::Title, "Chapter One".to_string());
        set_container(&mut entry, &meta.container_title);
        let bib = entry.to_bibtex();
        assert!(bib.contains("@incollection"), "got: {}", bib);
        assert!(
            bib.contains("booktitle = {Handbook of Vestibular Research}"),
            "got: {}",
            bib
        );
    }

    #[test]
    fn test_crossref_unknown_type_falls_back_to_misc_not_article() {
        // An unknown Crossref type must not be rendered as a journal article.
        let json = r#"{
            "title": ["Some Odd Work"],
            "container-title": ["Odd Series"],
            "type": "reference-entry",
            "author": [{"given": "J", "family": "X"}],
            "published-print": {"date-parts": [[2020]]},
            "DOI": "10.1234/odd.1"
        }"#;
        let meta: CrossrefItem = serde_json::from_str(json).unwrap();
        let etype = type_from_crossref(&meta.pub_type);
        assert_eq!(etype, EntryType::Misc);
        let mut entry = BibEntry::new(etype.clone());
        entry.set_field(Field::Title, "Some Odd Work".to_string());
        set_container(&mut entry, &meta.container_title);
        let bib = entry.to_bibtex();
        assert!(bib.contains("@misc"), "got: {}", bib);
        // Unknown-type container goes to note, not journal.
        assert!(bib.contains("note = {Odd Series}"), "got: {}", bib);
    }
}
