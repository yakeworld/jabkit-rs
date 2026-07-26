use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct PubMed {
    api_key: Option<String>,
}

impl PubMed {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[derive(Debug, Deserialize)]
struct ESearchResponse {
    // XML? No, E-utilities can return JSON
    esearchresult: ESearchResult,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ESearchResult {
    idlist: Vec<String>,
    #[serde(default)]
    count: Option<String>,
    #[serde(default)]
    retmax: Option<String>,
    #[serde(default)]
    retstart: Option<String>,
    #[serde(default)]
    querytranslation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ESummaryResponse {
    result: ESummaryResult,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ESummaryResult {
    uids: Vec<String>,
    #[serde(flatten)]
    papers: std::collections::HashMap<String, ESummaryPaper>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct ESummaryPaper {
    uid: Option<String>,
    title: Option<String>,
    authors: Option<Vec<ESummaryAuthor>>,
    source: Option<String>,
    pubdate: Option<String>,
    volume: Option<String>,
    issue: Option<String>,
    pages: Option<String>,
    elocationid: Option<String>,
    doi: Option<String>,
    issn: Option<String>,
    pmcid: Option<String>,
    attributes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ESummaryAuthor {
    name: Option<String>,
    authtype: Option<String>,
    clusterid: Option<String>,
}

#[async_trait]
impl Provider for PubMed {
    fn name(&self) -> &'static str {
        "Medline/PubMed"
    }
    fn key_env(&self) -> Option<&'static str> { Some("PUBMED_API_KEY") }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        // Step 1: esearch to get PMIDs
        let mut search_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&term={}&retmax={}&retmode=json",
            urlencoding(query),
            limit.min(100)
        );
        if let Some(key) = &self.api_key {
            search_url.push_str(&format!("&api_key={}", key));
        }

        let client = reqwest::Client::new();
        let resp = client
            .get(&search_url)
            .header("User-Agent", "jabkit/0.1")
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("PubMed esearch {}: {}", resp.status(), resp.text().await?);
        }

        let search_resp: ESearchResponse = resp.json().await.context("Failed to parse esearch response")?;
        let pmids = search_resp.esearchresult.idlist;
        let total_found = search_resp
            .esearchresult
            .count
            .unwrap_or_default()
            .parse::<usize>()
            .unwrap_or(pmids.len());

        if pmids.is_empty() {
            return Ok(SearchResult {
                entries: vec![],
                total_found: 0,
            });
        }

        // Step 2: esummary to get details
        let ids = pmids.join(",");
        let mut summary_url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json",
            ids
        );
        if let Some(key) = &self.api_key {
            summary_url.push_str(&format!("&api_key={}", key));
        }

        let resp2 = client
            .get(&summary_url)
            .header("User-Agent", "jabkit/0.1")
            .send()
            .await?;

        if !resp2.status().is_success() {
            anyhow::bail!("PubMed esummary {}: {}", resp2.status(), resp2.text().await?);
        }

        let summary_resp: ESummaryResponse = resp2.json().await.context("Failed to parse esummary")?;

        let entries: Vec<BibEntry> = pmids
            .into_iter()
            .filter_map(|pmid| {
                let paper = summary_resp.result.papers.get(&pmid)?;
                let mut entry = BibEntry::new(EntryType::Article);

                entry.set_field(Field::Pmid, paper.uid.clone().unwrap_or(pmid));
                entry.set_field(Field::Title, paper.title.clone().unwrap_or_default());
                entry.set_field(Field::Journal, paper.source.clone().unwrap_or_default());
                entry.set_field(Field::Volume, paper.volume.clone().unwrap_or_default());
                entry.set_field(Field::Issue, paper.issue.clone().unwrap_or_default());
                entry.set_field(Field::Pages, paper.pages.clone().unwrap_or_default());

                if let Some(doi) = &paper.doi {
                    entry.set_field(Field::Doi, doi.clone());
                }

                // PubDate -> Year
                if let Some(pubdate) = &paper.pubdate {
                    let year = pubdate.split(' ').next().unwrap_or("");
                    if year.len() >= 4 && year.chars().all(|c| c.is_ascii_digit()) {
                        entry.set_field(Field::Year, year.to_string());
                    }
                }

                // Authors
                if let Some(authors) = &paper.authors {
                    let names: Vec<String> = authors
                        .iter()
                        .filter_map(|a| a.name.clone())
                        .collect();
                    if !names.is_empty() {
                        entry.set_field(Field::Author, names.join(" and "));
                    }
                }

                Some(entry)
            })
            .collect();

        Ok(SearchResult { entries, total_found })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let pmid = id.trim().strip_prefix("PMID:").unwrap_or(id);
        let mut url = format!(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json",
            pmid
        );
        if let Some(key) = &self.api_key {
            url.push_str(&format!("&api_key={}", key));
        }

        let resp = reqwest::Client::new()
            .get(&url)
            .header("User-Agent", "jabkit/0.1")
            .send()
            .await?;

        let s: ESummaryResponse = resp.json().await?;

        let paper = s
            .result
            .papers
            .get(pmid)
            .context("PMID not found")?;

        let mut entry = BibEntry::new(EntryType::Article);
        entry.set_field(Field::Pmid, paper.uid.clone().unwrap_or(pmid.to_string()));
        entry.set_field(Field::Title, paper.title.clone().unwrap_or_default());
        entry.set_field(Field::Journal, paper.source.clone().unwrap_or_default());
        entry.set_field(Field::Volume, paper.volume.clone().unwrap_or_default());
        entry.set_field(Field::Issue, paper.issue.clone().unwrap_or_default());
        entry.set_field(Field::Pages, paper.pages.clone().unwrap_or_default());

        if let Some(doi) = &paper.doi {
            entry.set_field(Field::Doi, doi.clone());
        }
        if let Some(pubdate) = &paper.pubdate {
            if let Some(year) = pubdate.split_whitespace().next() {
                if year.len() >= 4 {
                    entry.set_field(Field::Year, year.to_string());
                }
            }
        }
        if let Some(authors) = &paper.authors {
            let names: Vec<String> = authors.iter().filter_map(|a| a.name.clone()).collect();
            if !names.is_empty() {
                entry.set_field(Field::Author, names.join(" and "));
            }
        }

        Ok(entry)
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
