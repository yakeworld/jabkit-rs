use anyhow::{Context, Result};
use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::Reader;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct ArXiv;

#[async_trait]
impl Provider for ArXiv {
    fn name(&self) -> &'static str {
        "arXiv"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let q = urlencoding(query);
        let url = format!(
            "https://export.arxiv.org/api/query?search_query=all:{}&max_results={}&sortBy=relevance&sortOrder=descending",
            q, limit.min(100)
        );

        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit/0.1")
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("arXiv API {}: {}", resp.status(), resp.text().await?);
        }

        let xml = resp.text().await?;
        let entries = parse_arxiv_xml(&xml)?;

        Ok(SearchResult {
            total_found: entries.len(),
            entries,
        })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let arxiv_id = id.trim().strip_prefix("arXiv:").unwrap_or(id);
        let url = format!("https://export.arxiv.org/api/query?id_list={}", arxiv_id);

        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit/0.1")
            .send()
            .await?;

        let xml = resp.text().await?;
        let mut entries = parse_arxiv_xml(&xml)?;
        entries.pop().context("No arXiv entry found")
    }
}

#[cfg(test)]
pub fn parse_arxiv_xml_test(xml: &str) -> Result<Vec<BibEntry>> {
    parse_arxiv_xml_impl(xml)
}

fn parse_arxiv_xml(xml: &str) -> Result<Vec<BibEntry>> {
    parse_arxiv_xml_impl(xml)
}

fn parse_arxiv_xml_impl(xml: &str) -> Result<Vec<BibEntry>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut in_entry = false;
    let mut current = BibEntry::new(EntryType::Article);
    let mut in_id = false;
    let mut in_title = false;
    let mut in_summary = false;
    let mut in_author = false;
    let mut in_name = false;
    let mut in_given = false;
    let mut in_family = false;
    let mut in_published = false;
    let mut _in_updated = false;
    let mut in_doi = false;
    let _in_link = false;
    let _in_categories = false;
    let mut in_journal_ref = false;
    let mut in_comment = false;

    let mut author_given = String::new();
    let mut author_family = String::new();
    let mut authors: Vec<String> = Vec::new();
    let mut current_tag = Vec::new();
    let mut link_href = String::new();
    let mut link_title = String::new();

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_tag.push(tag.clone());

                if matches!(e.name().as_ref(), b"entry") {
                    in_entry = true;
                    current = BibEntry::new(EntryType::Article);
                    authors = Vec::new();
                    link_href.clear();
                    link_title.clear();
                }

                // Check for link[@title="doi"]
                if tag == "link" {
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        let val = String::from_utf8_lossy(&attr.value);
                        if key == "href" {
                            link_href = val.to_string();
                        }
                        if key == "title" {
                            link_title = val.to_string();
                        }
                    }
                    if link_title == "doi" {
                        in_doi = true;
                    }
                }

                match tag.as_str() {
                    "id" if in_entry => in_id = true,
                    "title" if in_entry => in_title = true,
                    "summary" if in_entry => in_summary = true,
                    "author" if in_entry => in_author = true,
                    "name" if in_author => in_name = true,
                    "given_name" if in_author => in_given = true,
                    "family_name" if in_author => in_family = true,
                    "published" if in_entry => in_published = true,
                    "updated" if in_entry => _in_updated = true,
                    "arxiv:journal_ref" | "journal_ref" if in_entry => in_journal_ref = true,
                    "arxiv:comment" | "comment" if in_entry => in_comment = true,
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_tag.pop();

                match tag.as_str() {
                    "entry" => {
                        if !authors.is_empty() {
                            current.set_field(Field::Author, authors.join(" and "));
                        }
                        entries.push(current.clone());
                        in_entry = false;
                    }
                    "id" => in_id = false,
                    "title" => in_title = false,
                    "summary" => in_summary = false,
                    "author" => {
                        if !author_family.is_empty() {
                            if author_given.is_empty() {
                                authors.push(author_family.clone());
                            } else {
                                authors.push(format!("{}, {}", author_family, author_given));
                            }
                        }
                        author_given.clear();
                        author_family.clear();
                        in_author = false;
                    }
                    "name" => in_name = false,
                    "given_name" => in_given = false,
                    "family_name" => in_family = false,
                    "published" => in_published = false,
                    "updated" => _in_updated = false,
                    "arxiv:journal_ref" | "journal_ref" => in_journal_ref = false,
                    "arxiv:comment" | "comment" => in_comment = false,
                    _ => {}
                }

                if tag == "link" {
                    if in_doi {
                        current.set_field(Field::Doi, link_href.clone());
                    }
                    in_doi = false;
                    link_href.clear();
                    link_title.clear();
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                if in_id && in_entry {
                    // Format: http://arxiv.org/abs/XXXX.XXXXX
                    let id = text.trim();
                    let clean = if let Some(pos) = id.find("/abs/") {
                        id[pos + 5..].to_string()
                    } else {
                        id.to_string()
                    };
                    current.set_field(Field::Eprint, clean);
                    current.set_field(Field::Note, "arXiv".to_string());
                }
                if in_title && in_entry {
                    let t = text.replace('\n', " ").trim().to_string();
                    current.set_field(Field::Title, t);
                }
                if in_summary && in_entry {
                    let abs = text.replace('\n', " ").trim().to_string();
                    current.set_field(Field::Abstract, abs);
                }
                if in_name && in_author {
                    // Full name in <name> tag
                    let n = text.trim().to_string();
                    if let Some(comma) = n.find(',') {
                        author_family = n[..comma].trim().to_string();
                        author_given = n[comma + 1..].trim().to_string();
                    } else {
                        author_family = n;
                    }
                }
                if in_given && in_author {
                    author_given = text.trim().to_string();
                }
                if in_family && in_author {
                    author_family = text.trim().to_string();
                }
                if in_published && in_entry {
                    // Format: 2024-01-15T00:00:00Z
                    let d = text.trim();
                    if d.len() >= 4 {
                        current.set_field(Field::Year, d[..4].to_string());
                    }
                }
                if in_journal_ref && in_entry {
                    current.set_field(Field::Journal, text.trim().to_string());
                }
                if in_comment && in_entry {
                    let existing_note = current.get(Field::Note).unwrap_or_default().to_string();
                    let c = text.trim().to_string();
                    let note = if existing_note.is_empty() {
                        c
                    } else {
                        format!("{}; {}", existing_note, c)
                    };
                    current.set_field(Field::Note, note);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => anyhow::bail!("XML parse error: {}", e),
            _ => {}
        }
        buf.clear();
    }

    Ok(entries)
}

fn urlencoding(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}
