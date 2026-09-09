use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};
use anyhow::Result;
use async_trait::async_trait;

pub struct CiteSeer;
pub struct Isidore;
pub struct ZbMath;
pub struct MathSciNet;
pub struct ResearchGate;
pub struct ScholarArchive;
pub struct Biodiversity {
    api_key: Option<String>,
}
impl Biodiversity {
    pub fn new(key: Option<String>) -> Self {
        Self { api_key: key }
    }
}
pub struct Gvk;
pub struct Lobid;
pub struct Doab;

macro_rules! stub_provider {
    ($name:ident, $display:literal, $key_env:expr) => {
        #[async_trait]
        impl Provider for $name {
            fn name(&self) -> &'static str {
                $display
            }
            fn key_env(&self) -> Option<&'static str> {
                $key_env
            }
            async fn search(&self, _query: &str, _limit: usize) -> Result<SearchResult> {
                anyhow::bail!("{}: no public search API available", $display)
            }
            async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> {
                anyhow::bail!("{}: no ID lookup available", $display)
            }
        }
    };
}

stub_provider!(CiteSeer, "CiteSeerX", None);
stub_provider!(Isidore, "ISIDORE", None);
stub_provider!(ZbMath, "zbMATH", None);
stub_provider!(MathSciNet, "MathSciNet", None);
stub_provider!(ResearchGate, "ResearchGate", None);
stub_provider!(ScholarArchive, "ScholarArchive", None);
stub_provider!(Gvk, "GVK", None);
stub_provider!(Lobid, "LOBID", None);
stub_provider!(Doab, "DOAB", None);

#[async_trait]
impl Provider for Biodiversity {
    fn name(&self) -> &'static str {
        "BiodiversityHL"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("BIODIVERSITY_KEY")
    }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let key = self
            .api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("BiodiversityHL requires BIODIVERSITY_KEY"))?;
        let url = format!(
            "http://www.biodiversitylibrary.org/api2/GetSearchResults?q={}&limit={}&format=json&apikey={}",
            urlencoding(query), limit.min(50), key
        );
        let resp = super::http_client()
            .get(&url)
            .header("User-Agent", "jabkit-rs/0.1")
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("BiodiversityHL {}: {}", resp.status(), resp.text().await?);
        }
        let v: serde_json::Value = resp.json().await?;
        let entries: Vec<BibEntry> = v
            .pointer("/Result")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        let mut e = BibEntry::new(EntryType::Article);
                        let t = item
                            .get("Title")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default();
                        e.set_field(Field::Title, t.to_string());
                        let a = item
                            .get("Author")
                            .and_then(|a| a.as_str())
                            .unwrap_or_default();
                        e.set_field(Field::Author, a.to_string());
                        let y = item
                            .get("Year")
                            .and_then(|y| y.as_str())
                            .unwrap_or_default();
                        e.set_field(Field::Year, y.to_string());
                        e
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total = entries.len();
        Ok(SearchResult {
            entries,
            total_found: total,
        })
    }
    async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> {
        anyhow::bail!("BiodiversityHL ID lookup not supported")
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
