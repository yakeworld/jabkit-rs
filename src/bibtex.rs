use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    Article,
    Book,
    InCollection,
    InProceedings,
    Proceedings,
    Misc,
}

impl EntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryType::Article => "article",
            EntryType::Book => "book",
            EntryType::InCollection => "incollection",
            EntryType::InProceedings => "inproceedings",
            EntryType::Proceedings => "proceedings",
            EntryType::Misc => "misc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Field {
    Author,
    Title,
    Journal,
    Booktitle,
    Year,
    Volume,
    Issue,
    Pages,
    Doi,
    Pmid,
    Eprint,
    Abstract,
    Publisher,
    Note,
    Url,
    #[allow(dead_code)]
    Keywords,
}

impl Field {
    pub fn as_str(&self) -> &'static str {
        match self {
            Field::Author => "author",
            Field::Title => "title",
            Field::Journal => "journal",
            Field::Booktitle => "booktitle",
            Field::Year => "year",
            Field::Volume => "volume",
            Field::Issue => "number",
            Field::Pages => "pages",
            Field::Doi => "doi",
            Field::Pmid => "pmid",
            Field::Eprint => "eprint",
            Field::Abstract => "abstract",
            Field::Publisher => "publisher",
            Field::Note => "note",
            Field::Url => "url",
            Field::Keywords => "keywords",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BibEntry {
    pub entry_type: EntryType,
    pub citation_key: String,
    fields: BTreeMap<Field, String>,
}

impl BibEntry {
    pub fn new(entry_type: EntryType) -> Self {
        Self {
            entry_type,
            citation_key: String::new(),
            fields: BTreeMap::new(),
        }
    }

    pub fn set_field(&mut self, field: Field, value: String) {
        if !value.is_empty() {
            self.fields.insert(field, value);
        }
    }

    pub fn get(&self, field: Field) -> Option<&str> {
        self.fields.get(&field).map(|s| s.as_str())
    }

    /// Generate a citation key from first author + year.
    /// CJK / non-ASCII authors fall back to a DOI-derived key (if available)
    /// so that LaTeX can compile the key without UTF-8 surprises.
    pub fn generate_key(&mut self) {
        let author = self.fields.get(&Field::Author).cloned().unwrap_or_default();
        let year = self.fields.get(&Field::Year).cloned().unwrap_or_default();

        let first_author = author
            .split(" and ")
            .next()
            .unwrap_or("")
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

        // Sanitize: keep only ASCII alphanumerics, spaces, hyphens
        let ascii_author: String = first_author
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
            .collect();

        let author_part = ascii_author.to_lowercase().replace([' ', '-'], "_");

        let year_part = if year.len() >= 4 { &year[..4] } else { &year };

        if !author_part.is_empty() && !year_part.is_empty() {
            self.citation_key = format!("{}{}", author_part, year_part);
        } else if let Some(doi) = self.fields.get(&Field::Doi) {
            // No usable author (missing, all-CJK, etc.) → derive key from DOI
            let doi_clean: String = doi
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            self.citation_key = format!("doi_{}", doi_clean);
        } else if !year_part.is_empty() {
            self.citation_key = format!("nauthor_{}", year_part);
        } else {
            self.citation_key = "unknown".to_string();
        }
    }

    /// Render as BibTeX string
    pub fn to_bibtex(&self) -> String {
        let mut s = String::new();
        let key = if self.citation_key.is_empty() {
            "unknown"
        } else {
            &self.citation_key
        };

        s.push_str(&format!("@{}{{{},\n", self.entry_type.as_str(), key));

        for (field, value) in &self.fields {
            let formatted = format_bibtex_value(value, field);
            s.push_str(&format!("  {} = {},\n", field.as_str(), formatted));
        }

        s.push_str("}\n");
        s
    }
}

/// Make the value safe as a BibTeX field body by neutralising *unpaired*
/// braces.
///
/// Traditional BibTeX (bibtex.web field scanner) counts `{` and `}` for
/// grouping and a backslash does NOT stop that count. So a stray `{` in a
/// value leaves the group open and the scanner swallows the following fields
/// (verified: `title = {x \{ y}` → `Illegal end of database file` + empty
/// title/journal/year). Escaping with `\{` therefore does not fix it.
///
/// The safe representation of a literal brace that contains NO brace
/// characters is `\textbraceleft` / `\textbraceright` (TeX commands that render
/// as `{` / `}` in a LaTeX consumer and are inert to BibTeX's brace counter).
/// Already-balanced groups (e.g. `{N}ystagmus` capital-protection) are left
/// intact, so legitimate structure survives.
fn escape_unbalanced_braces(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let n = chars.len();
    let mut unpaired_open = vec![false; n]; // this `{` has no matching `}`
    let mut unpaired_close = vec![false; n]; // this `}` has no matching `{`

    // Backward pass: a `{` is unpaired if no `}` to its right can close it.
    let mut open_to_close: i32 = 0;
    for i in (0..n).rev() {
        match chars[i] {
            '}' => open_to_close += 1,
            '{' => {
                if open_to_close > 0 {
                    open_to_close -= 1; // this `{` will be closed
                } else {
                    unpaired_open[i] = true;
                }
            }
            _ => {}
        }
    }
    // Forward pass: a `}` is unpaired if no `{` to its left can open it.
    let mut close_to_open: i32 = 0;
    for i in 0..n {
        match chars[i] {
            '{' => close_to_open += 1,
            '}' => {
                if close_to_open > 0 {
                    close_to_open -= 1; // this `}` closes an earlier `{`
                } else {
                    unpaired_close[i] = true;
                }
            }
            _ => {}
        }
    }

    let mut out = String::with_capacity(value.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        match c {
            // Unpaired braces become brace-free TeX commands so BibTeX's brace
            // counter stays balanced and the field body is well-formed.
            '{' if unpaired_open[i] => out.push_str("\\textbraceleft"),
            '}' if unpaired_close[i] => out.push_str("\\textbraceright"),
            _ => out.push(c),
        }
    }
    out
}

fn format_bibtex_value(value: &str, field: &Field) -> String {
    // All field values pass through brace-balancing so a stray upstream brace
    // cannot corrupt the surrounding BibTeX.
    let safe = escape_unbalanced_braces(value);
    match field {
        Field::Author | Field::Title | Field::Journal => {
            if safe.starts_with('{') && safe.ends_with('}') {
                safe
            } else {
                format!("{{{safe}}}")
            }
        }
        _ => format!("{{{safe}}}"),
    }
}

/// Render multiple entries as a single BibTeX string
#[allow(dead_code)]
pub fn entries_to_bibtex(entries: &[BibEntry]) -> String {
    let mut result = String::new();
    for entry in entries {
        result.push_str(&entry.to_bibtex());
        result.push('\n');
    }
    result
}

/// Deduplicate citation keys within a batch.
/// First occurrence keeps its key; subsequent ones get a letter suffix:
/// `yang2026` → `yang2026`, `yang2026a`, `yang2026b`, …
/// Returns the entries with keys already mutated.
pub fn deduplicate_keys(entries: &mut [BibEntry]) {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for entry in entries.iter_mut() {
        let base = entry.citation_key.clone();
        let count = seen.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            let suffix = char::from_u32((b'a' as u32) + (*count - 2) as u32).unwrap_or('z');
            entry.citation_key = format!("{}{}", base, suffix);
        }
    }
}
