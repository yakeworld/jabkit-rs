use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Inspire;

#[derive(Debug, Deserialize)]
struct InspireResponse {
    hits: InspireHits,
}

#[derive(Debug, Deserialize)]
struct InspireHits {
    hits: Vec<InspireHit>,
    total: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct InspireHit {
    metadata: InspireMetadata,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct InspireMetadata {
    titles: Option<Vec<InspireTitle>>,
    authors: Option<Vec<InspireAuthor>>,
    imprints: Option<Vec<InspireImprint>>,
    dois: Option<Vec<InspireDoi>>,
    arxiv_eprints: Option<Vec<InspireArxiv>>,
    publication_info: Option<Vec<InspirePubInfo>>,
    citeable: Option<bool>,
    document_type: Option<Vec<String>>,
    abstract_text: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct InspireTitle {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct InspireAuthor {
    full_name: Option<String>,
    affiliations: Option<Vec<InspireAffiliation>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct InspireAffiliation {
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InspireImprint {
    date: Option<String>,
    publisher: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InspireDoi {
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InspireArxiv {
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InspirePubInfo {
    journal_title: Option<String>,
    journal_volume: Option<String>,
    page_artid: Option<String>,
    year: Option<i32>,
}

#[async_trait]
impl Provider for Inspire {
    fn name(&self) -> &'static str { "INSPIRE" }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://inspirehep.net/api/literature?q={}&size={}",
            urlencoding(query), limit.min(100)
        );
        let resp = reqwest::Client::new()
            .get(&url).header("User-Agent", "jabkit-rs/0.1")
            .send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("INSPIRE {}: {}", resp.status(), resp.text().await?);
        }
        let d: InspireResponse = resp.json().await?;
        let total = d.hits.total.and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let entries = d.hits.hits.into_iter().filter_map(|h| {
            let m = h.metadata;
            let mut e = BibEntry::new(EntryType::Article);
            if let Some(t) = m.titles.and_then(|t| t.into_iter().next()) {
                e.set_field(Field::Title, t.title.unwrap_or_default());
            }
            if let Some(a) = m.authors {
                let names: Vec<String> = a.into_iter().filter_map(|a| a.full_name).collect();
                e.set_field(Field::Author, names.join(" and "));
            }
            if let Some(imprints) = m.imprints {
                if let Some(imp) = imprints.into_iter().next() {
                    if let Some(d) = imp.date {
                        if d.len() >= 4 { e.set_field(Field::Year, d[..4].to_string()); }
                    }
                    e.set_field(Field::Publisher, imp.publisher.unwrap_or_default());
                }
            }
            if let Some(dois) = m.dois {
                if let Some(d) = dois.into_iter().next() {
                    e.set_field(Field::Doi, d.value.unwrap_or_default());
                }
            }
            if let Some(arxiv) = m.arxiv_eprints {
                if let Some(a) = arxiv.into_iter().next() {
                    e.set_field(Field::Eprint, a.value.unwrap_or_default());
                }
            }
            if let Some(pubinfo) = m.publication_info {
                if let Some(p) = pubinfo.into_iter().next() {
                    e.set_field(Field::Journal, p.journal_title.unwrap_or_default());
                    e.set_field(Field::Volume, p.journal_volume.unwrap_or_default());
                    e.set_field(Field::Pages, p.page_artid.unwrap_or_default());
                    if let Some(y) = p.year {
                        e.set_field(Field::Year, y.to_string());
                    }
                }
            }
            Some(e)
        }).collect();

        Ok(SearchResult { entries, total_found: total })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let url = format!("https://inspirehep.net/api/literature?q={}&size=1", urlencoding(id));
        let resp = reqwest::Client::new()
            .get(&url).header("User-Agent", "jabkit-rs/0.1")
            .send().await?;
        let d: InspireResponse = resp.json().await?;
        let h = d.hits.hits.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No INSPIRE entry: {}", id))?;
        let m = h.metadata;
        let mut e = BibEntry::new(EntryType::Article);
        if let Some(t) = m.titles.and_then(|t| t.into_iter().next()) {
            e.set_field(Field::Title, t.title.unwrap_or_default());
        }
        if let Some(a) = m.authors {
            let names: Vec<String> = a.into_iter().filter_map(|a| a.full_name).collect();
            e.set_field(Field::Author, names.join(" and "));
        }
        if let Some(dois) = m.dois {
            if let Some(d) = dois.into_iter().next() {
                e.set_field(Field::Doi, d.value.unwrap_or_default());
            }
        }
        if let Some(imprints) = m.imprints {
            if let Some(imp) = imprints.into_iter().next() {
                if let Some(d) = imp.date {
                    if d.len() >= 4 { e.set_field(Field::Year, d[..4].to_string()); }
                }
            }
        }
        Ok(e)
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => r.push(b as char),
            b' ' => r.push_str("%20"),
            _ => r.push_str(&format!("%{:02X}", b)),
        }
    }
    r
}
