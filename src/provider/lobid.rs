use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Lobid;

#[derive(Deserialize)]
struct LobidResp {
    member: Vec<LobidMember>,
    totalItems: Option<usize>,
}
#[derive(Deserialize)]
struct LobidMember {
    title: Option<String>,
    contribution: Option<Vec<LobidContribution>>,
    publication: Option<Vec<LobidPublication>>,
    issued: Option<String>,
    doi: Option<String>,
    extent: Option<String>,
}
#[derive(Deserialize)]
struct LobidContribution {
    agent: Option<LobidAgent>,
    _type: Option<Vec<String>>,
}
#[derive(Deserialize)]
struct LobidAgent {
    label: Option<String>,
}
#[derive(Deserialize)]
struct LobidPublication {
    publishedBy: Option<Vec<LobidPublishedBy>>,
}
#[derive(Deserialize)]
struct LobidPublishedBy {
    label: Option<String>,
}

#[async_trait]
impl Provider for Lobid {
    fn name(&self) -> &'static str { "LOBID" }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://lobid.org/resources/search?q={}&size={}&format=json",
            urlencoding(query), limit.min(100)
        );
        let resp = reqwest::Client::new().get(&url).header("User-Agent", "jabkit-rs/0.1").send().await?;
        if !resp.status().is_success() { anyhow::bail!("LOBID {}: {}", resp.status(), resp.text().await?); }
        let d: LobidResp = resp.json().await?;
        let entries: Vec<BibEntry> = d.member.into_iter().map(|m| {
            let mut e = BibEntry::new(EntryType::Misc);
            e.set_field(Field::Title, m.title.unwrap_or_default());
            if let Some(c) = m.contribution {
                let n: Vec<String> = c.into_iter().filter_map(|c| c.agent).filter_map(|a| a.label).collect();
                e.set_field(Field::Author, n.join(" and "));
            }
            if let Some(y) = m.issued { e.set_field(Field::Year, y); }
            if let Some(p) = m.publication {
                let pubs: Vec<String> = p.into_iter()
                    .filter_map(|p| p.publishedBy)
                    .flatten()
                    .filter_map(|pb| pb.label)
                    .collect();
                e.set_field(Field::Publisher, pubs.join("; "));
            }
            e.set_field(Field::Doi, m.doi.unwrap_or_default());
            e
        }).collect();
        let total_found = d.totalItems.unwrap_or(entries.len());
        Ok(SearchResult { entries, total_found: total_found })
    }
    async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> {
        anyhow::bail!("LOBID ID lookup not supported")
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() { match b {
        b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'/' => r.push(b as char),
        b' ' => r.push_str("%20"), _ => r.push_str(&format!("%{:02X}", b)),
    }} r
}
