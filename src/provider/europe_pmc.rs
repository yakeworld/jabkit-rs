use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct EuropePmc;

#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
struct EpmcResponse {
    resultList: EpmcResultList,
    hitCount: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
struct EpmcResultList {
    result: Vec<EpmcResult>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
struct EpmcResult {
    title: Option<String>,
    authorString: Option<String>,
    authorList: Option<EpmcAuthorList>,
    journalTitle: Option<String>,
    pubYear: Option<String>,
    volume: Option<String>,
    issue: Option<String>,
    pageInfo: Option<String>,
    doi: Option<String>,
    pmid: Option<String>,
    pmcid: Option<String>,
    abstractText: Option<String>,
    publisher: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EpmcAuthorList {
    author: Vec<EpmcAuthor>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
struct EpmcAuthor {
    fullName: Option<String>,
}

#[async_trait]
impl Provider for EuropePmc {
    fn name(&self) -> &'static str { "EuropePMC" }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://www.ebi.ac.uk/europepmc/webservices/rest/search?query={}&pageSize={}&format=json",
            urlencoding(query), limit.min(100)
        );
        let resp = super::http_client()
            .get(&url).header("User-Agent", "jabkit-rs/0.1")
            .send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("EuropePMC {}: {}", resp.status(), resp.text().await?);
        }
        let d: EpmcResponse = resp.json().await?;
        let total = d.hitCount.unwrap_or(d.resultList.result.len());
        let entries = d.resultList.result.into_iter().map(|r| {
            let mut e = BibEntry::new(EntryType::Article);
            e.set_field(Field::Title, r.title.unwrap_or_default());
            if let Some(al) = r.authorList {
                let names: Vec<String> = al.author.into_iter().filter_map(|a| a.fullName).collect();
                if !names.is_empty() {
                    e.set_field(Field::Author, names.join(" and "));
                }
            } else {
                e.set_field(Field::Author, r.authorString.unwrap_or_default());
            }
            e.set_field(Field::Journal, r.journalTitle.unwrap_or_default());
            e.set_field(Field::Year, r.pubYear.unwrap_or_default());
            e.set_field(Field::Volume, r.volume.unwrap_or_default());
            e.set_field(Field::Issue, r.issue.unwrap_or_default());
            e.set_field(Field::Pages, r.pageInfo.unwrap_or_default());
            e.set_field(Field::Doi, r.doi.unwrap_or_default());
            e.set_field(Field::Pmid, r.pmid.unwrap_or_default());
            e.set_field(Field::Abstract, r.abstractText.unwrap_or_default());
            e.set_field(Field::Publisher, r.publisher.unwrap_or_default());
            e
        }).collect();

        Ok(SearchResult { entries, total_found: total })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let url = format!(
            "https://www.ebi.ac.uk/europepmc/webservices/rest/search?query={}:{}&format=json",
            if id.starts_with("PMC") { "PMCID" } else { "EXT_ID" },
            urlencoding(id)
        );
        let resp = super::http_client()
            .get(&url).header("User-Agent", "jabkit-rs/0.1")
            .send().await?;
        let d: EpmcResponse = resp.json().await?;
        let r = d.resultList.result.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No EuropePMC entry: {}", id))?;
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Title, r.title.unwrap_or_default());
        if let Some(al) = r.authorList {
            let names: Vec<String> = al.author.into_iter().filter_map(|a| a.fullName).collect();
            e.set_field(Field::Author, names.join(" and "));
        }
        e.set_field(Field::Journal, r.journalTitle.unwrap_or_default());
        e.set_field(Field::Year, r.pubYear.unwrap_or_default());
        e.set_field(Field::Doi, r.doi.unwrap_or_default());
        e.set_field(Field::Pmid, r.pmid.unwrap_or_default());
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
