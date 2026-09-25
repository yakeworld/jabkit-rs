use anyhow::Result;
use async_trait::async_trait;
use std::sync::LazyLock;

use super::{Provider, SearchResult};
use crate::bibtex::{BibEntry, EntryType, Field};

/// LibGen (libgen.li and mirrors) — keyword search of the open-access PDF corpus.
/// JSON API dead ("No Request keys"); only the index.php HTML route works.
/// GFW networks MUST use JABKIT_PROXY (direct = timeout).
/// Value: full-text *coverage* probe — which hits actually have a downloadable
/// PDF, so callers can skip doi-fetch cascade probing on the rest.
///
/// HTML row layout (verified 2026-09-16, columns[] = d):
///   <td><b><a href="series.php?id=N">JOURNAL </a>
///          <a ... href="edition.php?id=E"><i> YEAR-mo vol. V iss. I</i></a> pp.P1—P2</b><br>
///          <a ... href="edition.php?id=E">TITLE<span></span> <i>DOI: 10.x/y</i></a><br>...
///   <td>AUTHORS</td>  (next cell after the meta cell)
///
/// Book library: same row layout, but search MUST carry `curtab=e`
/// (the default search only covers the SCI-article library — books are
/// invisible without it). Book meta anchor carries a green-font ISBN
/// (" 9781718502...") instead of "vol. V iss. I".
pub struct LibGen {
    mirror: String,
    books: bool,
}

impl LibGen {
    pub fn new(mirror: &str) -> Self {
        Self {
            mirror: mirror.to_string(),
            books: false,
        }
    }

    pub fn new_books(mirror: &str) -> Self {
        Self {
            mirror: mirror.to_string(),
            books: true,
        }
    }

    async fn get(&self, url: &str) -> Result<String> {
        let resp = super::http_client()
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64)")
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "LibGen {}: {}",
                resp.status(),
                resp.text().await?.chars().take(120).collect::<String>()
            );
        }
        Ok(resp.text().await?)
    }

    fn search_url(&self, query: &str) -> String {
        let tab = if self.books { "curtab=e&" } else { "" };
        format!(
            "https://{}/index.php?{}req={}&columns%5B%5D=d",
            self.mirror,
            tab,
            urlencoding(query)
        )
    }
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&nbsp;", " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ed_re() -> &'static regex::Regex {
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"edition\.php\?id=(\d+)"#).unwrap());
    &RE
}

fn journal_re() -> &'static regex::Regex {
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"series\.php\?id=\d+">([^<]+)<"#).unwrap());
    &RE
}

fn meta_re() -> &'static regex::Regex {
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"<i>\s*([^<]+)</i>"#).unwrap());
    &RE
}

fn title_re() -> &'static regex::Regex {
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| {
            regex::Regex::new(r#"href="edition\.php\?id=(\d+)">([^<]+)"#).unwrap()
        });
    &RE
}

fn doi_re() -> &'static regex::Regex {
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r#"DOI: (10\.[0-9A-Za-z./-]+)"#).unwrap());
    &RE
}

/// Parse the meta cell into (edition_id, title, journal, year, pages, doi, author).
fn parse_cell(cell: &str, author: &str) -> Option<(String, String, String, String, String, String)> {
    let ed = ed_re()
        .captures(cell)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())?;
    // Title anchor = the LAST edition.php anchor whose href id differs from
    // the meta anchor's edition id (meta anchor carries "<i> YEAR vol...</i>").
    let mut title_anchors: Vec<String> = title_re()
        .captures_iter(cell)
        .map(|c| {
            let t = strip_tags(c.get(2).unwrap().as_str());
            // title anchor embeds "<i>DOI: 10.x/y</i>" after <span> — cut at marker
            t.find("DOI: ")
                .map(|p| t[..p].trim().to_string())
                .unwrap_or_else(|| t.trim().to_string())
        })
        .filter(|t| {
            !t.is_empty()
                // meta anchor text = "2012-aug 22 vol. 122 iss. 12" — starts with
                // a 4-digit year and contains "vol."; drop it
                && !t.chars().take(4).all(|c| c.is_ascii_digit() || c == '-')
                && !t.contains("vol.")
        })
        .collect();
    let title = title_anchors.pop().unwrap_or_default();
    let journal = journal_re()
        .captures(cell)
        .and_then(|c| c.get(1))
        .map(|m| strip_tags(m.as_str()))
        .unwrap_or_default();
    let (year, pages) = meta_re()
        .captures(cell)
        .and_then(|c| c.get(1))
        .map(|m| {
            let s = m.as_str();
            let year: String = s.chars().take(4).collect();
            let pages = s
                .find("pp.")
                .map(|p| strip_tags(&s[p + 3..]))
                .unwrap_or_default();
            (year, pages)
        })
        .unwrap_or_default();
    let doi = doi_re()
        .captures(cell)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    Some((ed, title, journal, year, pages, doi))
}

fn isbn_re() -> &'static regex::Regex {
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(9[0-9]{1,10})").unwrap());
    &RE
}

fn parse_rows(html: &str, limit: usize, books: bool) -> Vec<BibEntry> {
    let mut out = Vec::new();
    // rows = split by <tr>; each result row's cells split by <td.
    // The meta cell contains BOTH series.php and edition.php.
    for row in html.split("<tr>").skip(1) {
        let tds: Vec<&str> = row.split("<td").collect();
        let meta_i = tds
            .iter()
            .position(|t| t.contains("series.php") && t.contains("edition.php"));
        let Some(mi) = meta_i else {
            continue;
        };
        let author = strip_tags(tds.get(mi + 1).unwrap_or(&""));
        let cell = tds[mi];
        let Some(fields) = parse_cell(cell, &author) else {
            continue;
        };
        let (ed, title, journal, year, pages, doi) = fields;
        if title.is_empty() {
            continue;
        }
        // Book rows: ISBN in the green-font meta anchor
        let isbn = if books {
            isbn_re()
                .captures(cell)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .filter(|s| s.len() >= 10 && s.len() <= 13)
                .unwrap_or_default()
        } else {
            String::new()
        };
        let entry_type = if books {
            EntryType::Book
        } else {
            EntryType::Article
        };
        let mut e = BibEntry::new(entry_type);
        e.set_field(Field::Title, title);
        if books {
            if !journal.is_empty() {
                e.set_field(Field::Publisher, journal);
            }
        } else if !journal.is_empty() {
            e.set_field(Field::Journal, journal);
        }
        if !author.is_empty() {
            e.set_field(Field::Author, author);
        }
        if year.len() == 4 {
            e.set_field(Field::Year, year);
        }
        if !pages.is_empty() {
            e.set_field(Field::Pages, pages);
        }
        if !doi.is_empty() {
            e.set_field(Field::Doi, doi);
        }
        if !isbn.is_empty() {
            e.set_field(Field::Note, format!("ISBN: {}", isbn));
        }
        out.push(e);
        if out.len() >= limit {
            break;
        }
    }
    out
}

#[async_trait]
impl Provider for LibGen {
    fn name(&self) -> &'static str {
        if self.books { "LibGenBooks" } else { "LibGen" }
    }
    fn key_env(&self) -> Option<&'static str> {
        None
    }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult> {
        let html = self.get(&self.search_url(query)).await?;
        let entries = parse_rows(&html, limit, self.books);
        let total = entries.len();
        Ok(SearchResult {
            entries,
            total_found: total,
        })
    }

    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry> {
        let q = id.strip_prefix("doi:").unwrap_or(id);
        let html = self.get(&self.search_url(q)).await?;
        // Prefer DOI match via search
        let mut entries = parse_rows(&html, 20, self.books);
        // find matching entry (DOI or title)
        let entry = entries
            .iter()
            .find(|e| {
                let d = e.get(Field::Doi).unwrap_or_default();
                d == q
                    || e.get(Field::Title)
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&q.to_lowercase())
            })
            .cloned();
        if let Some(e) = entry {
            return Ok(e);
        }
        // fallback: return first result if query matched nothing precise
        if let Some(e) = entries.into_iter().next() {
            return Ok(e);
        }
        anyhow::bail!("LibGen no result for {}", id)
    }
}

fn urlencoding(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                r.push(b as char)
            }
            b' ' => r.push_str("%20"),
            _ => r.push_str(&format!("%{:02X}", b)),
        }
    }
    r
}
