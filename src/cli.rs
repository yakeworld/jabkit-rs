use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "jabkit",
    version,
    about = "Academic literature search CLI",
    after_help = "\
Providers:
  Crossref         Free, no key needed
  SemanticScholar  Set S2_API_KEY or store in GNOME Keyring
  Medline/PubMed   Set PUBMED_API_KEY or store in GNOME Keyring
  arXiv            Free, no key needed
  OpenAlex         Set OPENALEX_API_KEY or store in GNOME Keyring

API Keys (priority: env var > GNOME Keyring):
  S2_API_KEY=...       SemanticScholar
  PUBMED_API_KEY=...   Medline/PubMed
  OPENALEX_API_KEY=... OpenAlex

GNOME Keyring entries (org.jabref.customapikeys) are auto-decrypted.
Pipe --porcelain output to lit-import for dedup + PDF download."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable debug output
    #[arg(short = 'd', long = "debug", global = true)]
    pub debug: bool,

    /// Script-friendly output (BibTeX only, no logging). Pipe to lit-import.
    #[arg(short = 'p', long = "porcelain", global = true)]
    pub porcelain: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Search academic databases
    Fetch {
        /// Provider name (Crossref, SemanticScholar, Medline/PubMed, arXiv, OpenAlex)
        #[arg(long, short)]
        provider: String,

        /// Search query
        #[arg(long, short)]
        query: String,

        /// Max results (default: 20)
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Convert DOI(s) to BibTeX (via Crossref)
    DoiToBibtex {
        /// One or more DOIs
        #[arg(required = true)]
        dois: Vec<String>,
    },

    /// List available providers with key status
    ListProviders,

    /// Fetch by ID (DOI, PMID, arXiv ID, S2 PaperID)
    GetById {
        /// Provider name
        #[arg(long, short)]
        provider: String,

        /// ID
        #[arg(long, short)]
        id: String,
    },
}
