use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    Article,
    Book,
    InBook,
    InProceedings,
    Proceedings,
    Misc,
}

impl EntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryType::Article => "article",
            EntryType::Book => "book",
            EntryType::InBook => "inbook",
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

fn format_bibtex_value(value: &str, field: &Field) -> String {
    match field {
        Field::Author | Field::Title | Field::Journal => {
            // Preserve existing braces, add outer braces
            if value.starts_with('{') && value.ends_with('}') {
                value.to_string()
            } else {
                format!("{{{}}}", value)
            }
        }
        Field::Abstract => {
            // Abstract often long, wrap in braces
            format!("{{{}}}", value)
        }
        _ => {
            // Numbers/DOIs don't need braces in most styles
            format!("{{{}}}", value)
        }
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
