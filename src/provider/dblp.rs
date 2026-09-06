use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

pub struct Dblp;

#[derive(Debug, Deserialize)]
struct DblpResponse {
    result: DblpResult,
}

#[derive(Debug, Deserialize)]
struct DblpResult {
    hits: DblpHits,
}

#[derive(Debug, Deserialize)]
struct DblpHits {
    #[serde(default)]
    hit: Vec<DblpHit>,
    #[serde(default)]
    total: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DblpHit {
    info: DblpInfo,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DblpInfo {
    title: Option<String>,
    authors: Option<DblpAuthors>,
    year: Option<String>,
    doi: Option<String>,
    venue: Option<String>,
    volume: Option<String>,
    number: Option<String>,
    pages: Option<String>,
    publisher: Option<String>,
    #[serde(rename = "type")]
    pub_type: Option<String>,
    ee: Option<String>,
    url: Option<String>,
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DblpAuthors {
    #[serde(deserialize_with = "deserialize_author_list")]
    author: Vec<DblpAuthorValue>,
}

fn deserialize_author_list<'de, D>(deserializer: D) -> Result<Vec<DblpAuthorValue>, D::Error>
where D: serde::Deserializer<'de> {
    use serde::de;
    use std::fmt;
    struct AuthorListVisitor;
    impl<'de> de::Visitor<'de> for AuthorListVisitor {
        type Value = Vec<DblpAuthorValue>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a single author object or an array of author objects")
        }
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where A: de::SeqAccess<'de> {
            let mut v = Vec::new();
            while let Some(e) = seq.next_element::<DblpAuthorValue>()? {
                v.push(e);
            }
            Ok(v)
        }
        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where A: de::MapAccess<'de> {
            let a = DblpAuthorValue::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(vec![a])
        }
    }
    deserializer.deserialize_any(AuthorListVisitor)
}

#[derive(Debug, Deserialize)]
struct DblpAuthorValue {
    text: Option<String>,
}

#[async_trait]
impl Provider for Dblp {
    fn name(&self) -> &'static str { "DBLP" }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let url = format!(
            "https://dblp.org/search/publ/api?q={}&h={}&format=json",
            urlencoding(query), limit.min(100)
        );
        let resp = super::http_client()
            .get(&url).header("User-Agent", "jabkit-rs/0.1")
            .send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("DBLP {}: {}", resp.status(), resp.text().await?);
        }

        let d: DblpResponse = resp.json().await?;
        let total = d.result.hits.total.unwrap_or_default().parse().unwrap_or(0);
        let entries = d.result.hits.hit.into_iter().filter_map(|h| {
            let i = h.info;
            let mut e = BibEntry::new(type_from_dblp(&i.pub_type));
            e.set_field(Field::Title, i.title.unwrap_or_default());
            if let Some(a) = i.authors {
                let names: Vec<String> = a.author.into_iter().filter_map(|a| a.text).collect();
                e.set_field(Field::Author, names.join(" and "));
            }
            e.set_field(Field::Year, i.year.unwrap_or_default());
            e.set_field(Field::Journal, i.venue.unwrap_or_default());
            e.set_field(Field::Volume, i.volume.unwrap_or_default());
            e.set_field(Field::Issue, i.number.unwrap_or_default());
            e.set_field(Field::Pages, i.pages.unwrap_or_default());
            e.set_field(Field::Doi, i.doi.unwrap_or_default());
            e.set_field(Field::Publisher, i.publisher.unwrap_or_default());
            e.set_field(Field::Url, i.ee.or(i.url).unwrap_or_default());
            Some(e)
        }).collect();

        Ok(SearchResult { entries, total_found: total })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        // DBLP key lookup: https://dblp.org/pid/... or DOI
        let url = format!("https://dblp.org/search/publ/api?q={}&h=1&format=json", urlencoding(id));
        let resp = super::http_client()
            .get(&url).header("User-Agent", "jabkit-rs/0.1")
            .send().await?;
        let d: DblpResponse = resp.json().await?;
        let hit = d.result.hits.hit.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No DBLP entry for: {}", id))?;
        let i = hit.info;
        let mut e = BibEntry::new(type_from_dblp(&i.pub_type));
        e.set_field(Field::Title, i.title.unwrap_or_default());
        if let Some(a) = i.authors {
            let names: Vec<String> = a.author.into_iter().filter_map(|a| a.text).collect();
            e.set_field(Field::Author, names.join(" and "));
        }
        e.set_field(Field::Year, i.year.unwrap_or_default());
        e.set_field(Field::Doi, i.doi.unwrap_or_default());
        Ok(e)
    }
}

fn type_from_dblp(t: &Option<String>) -> EntryType {
    match t.as_deref() {
        Some("Journal Articles") | Some("article") => EntryType::Article,
        Some("Conference Papers") | Some("inproceedings") => EntryType::InProceedings,
        Some("Books") | Some("book") => EntryType::Book,
        _ => EntryType::Article,
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
