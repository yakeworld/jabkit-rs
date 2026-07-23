use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "jabkit",
    version,
    about = "Academic literature search CLI",
    after_help = "Providers (25):
  No key needed (6):
    Crossref, arXiv, DBLP, DOAJ, EuropePMC, INSPIRE
  Registered, no public API (9):
    CiteSeerX, ISIDORE, zbMATH, MathSciNet, ResearchGate, ScholarArchive
    GVK, LOBID, DOAB
  Key required (10):
    S2_API_KEY        SemanticScholar
    PUBMED_API_KEY    Medline/PubMed
    OPENALEX_API_KEY  OpenAlex
    IEEE_API_KEY      IEEE
    SPRINGER_API_KEY  Springer
    SCOPUS_API_KEY    Scopus
    ACM_API_KEY       ACM
    ADS_API_KEY       ADS (NASA)
    UNPAYWALL_EMAIL   Unpaywall
    BIODIVERSITY_KEY  BiodiversityHL

API Key priority: env var > .env file > GNOME Keyring (Linux)
  Run 'jabkit init' to create a .env template.

Use 'list-providers' to check which keys are configured.
Pipe --porcelain output to lit-import for dedup + PDF download."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable debug output
    #[arg(short = 'd', long = "debug", global = true)]
    pub debug: bool,

    /// Script-friendly output (BibTeX only). Pipe to lit-import.
    #[arg(short = 'p', long = "porcelain", global = true)]
    pub porcelain: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Search academic databases (26 providers)
    Fetch {
        #[arg(long, short)]
        provider: String,
        #[arg(long, short)]
        query: String,
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Convert DOI(s) to BibTeX (via Crossref)
    DoiToBibtex {
        #[arg(required = true)]
        dois: Vec<String>,
    },
    /// List available providers with key status
    ListProviders,
    /// Fetch by ID (DOI, PMID, arXiv ID, etc.)
    GetById {
        #[arg(long, short)]
        provider: String,
        #[arg(long, short)]
        id: String,
    },
    /// Create .env template file in current directory
    Init,
}
