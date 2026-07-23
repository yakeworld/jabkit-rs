use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Gvk;

#[derive(Deserialize)]
struct GvkResp {
    #[serde(rename = "searchRetrieveResponse")] resp: GvkSearchRetrieve,
}
#[derive(Deserialize)]
struct GvkSearchRetrieve {
    records: Option<Vec<GvkRecord>>,
    #[serde(rename = "numberOfRecords")] total: Option<usize>,
}
#[derive(Deserialize)]
struct GvkRecord {
    #[serde(rename = "recordSchema")] _schema: Option<String>,
    #[serde(rename = "recordPacking")] _packing: Option<String>,
    recordData: Option<GvkRecordData>,
}
#[derive(Deserialize)]
struct GvkRecordData {
    #[serde(rename = "dc:title")] title: Option<String>,
    #[serde(rename = "dc:creator")] creator: Option<GvkMultiValue>,
    #[serde(rename = "dc:date")] date: Option<String>,
    #[serde(rename = "dc:publisher")] publisher: Option<String>,
    #[serde(rename = "dc:identifier")] identifier: Option<GvkMultiValue>,
    #[serde(rename = "dc:source")] source: Option<String>,
    #[serde(rename = "dc:type")] _type: Option<String>,
}
#[derive(Deserialize)]
struct GvkMultiValue {
    #[serde(default)]
    value: Vec<String>,
}

#[async_trait]
impl Provider for Gvk {
    fn name(&self) -> &'static str { "GVK" }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://gso.gbv.de/sru/DB=2.1?query=pica.all%3D{}&version=2.0&operation=searchRetrieve&recordSchema=dc&maximumRecords={}",
            urlencoding(query), limit.min(100)
        );
        let resp = reqwest::Client::new().get(&url).header("User-Agent", "jabkit-rs/0.1").send().await?;
        if !resp.status().is_success() { anyhow::bail!("GVK {}: {}", resp.status(), resp.text().await?); }
        let d: GvkResp = quick_xml::de::from_str(&resp.text().await?)?;
        let records = d.resp.records.unwrap_or_default();
        let entries: Vec<BibEntry> = records.into_iter().filter_map(|r| {
            let rd = r.recordData?;
            let mut e = BibEntry::new(EntryType::Misc);
            e.set_field(Field::Title, rd.title.unwrap_or_default());
            if let Some(c) = rd.creator { e.set_field(Field::Author, c.value.join(" and ")); }
            e.set_field(Field::Year, rd.date.unwrap_or_default());
            e.set_field(Field::Publisher, rd.publisher.unwrap_or_default());
            e.set_field(Field::Journal, rd.source.unwrap_or_default());
            Some(e)
        }).collect();
        let total_found = d.resp.total.unwrap_or(entries.len());
        Ok(SearchResult { entries, total_found: total_found })
    }
    async fn fetch_by_id(&self, _id: &str) -> Result<BibEntry> {
        anyhow::bail!("GVK ID lookup not supported")
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() { match b {
        b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'/' => r.push(b as char),
        b' ' => r.push_str("%20"), _ => r.push_str(&format!("%{:02X}", b)),
    }} r
}
