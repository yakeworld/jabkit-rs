use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    Article,
    Book,
    InProceedings,
    Proceedings,
    Misc,
}

impl EntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryType::Article => "article",
            EntryType::Book => "book",
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

    /// Generate a citation key from first author + year
    pub fn generate_key(&mut self) {
        let author = self.fields.get(&Field::Author).cloned().unwrap_or_default();
        let year = self.fields.get(&Field::Year).cloned().unwrap_or_default();

        let first_author = author
            .split(" and ")
            .next()
            .unwrap_or("unknown")
            .split(',')
            .next()
            .unwrap_or("unknown")
            .trim()
            .to_lowercase()
            .replace(' ', "_")
            .replace('-', "_");

        let year_part = if year.len() >= 4 { &year[..4] } else { &year };

        if !first_author.is_empty() && !year_part.is_empty() {
            self.citation_key = format!("{}{}", first_author, year_part);
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
