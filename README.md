# jabkit-rs

**Academic literature search CLI** — Rust rewrite of jabkit.

Single binary, sub-millisecond startup (vs ~15s Java original). Supports 5 academic providers with BibTeX output.

## Features

- 🚀 **Sub-ms startup** — Rust native binary, no JVM, no Gradle
- 📦 **Single binary** ~3.5MB (vs 176MB JRE bundle)
- 🔍 **5 providers**: Semantic Scholar, PubMed/MEDLINE, Crossref, arXiv, OpenAlex
- 📝 **BibTeX output** — pipe directly to `lit-import`
- 🔑 **API key chain**: env var → `.env` file → GNOME Keyring
- 🔄 **DOI→BibTeX** converter

## Installation

### Linux

```bash
# Download binary
curl -sL https://github.com/yakeworld/jabkit-rs/releases/latest/download/jabkit-linux-x86_64 -o jabkit-rs
chmod +x jabkit-rs
sudo mv jabkit-rs /usr/local/bin/
```

### Windows

Download `jabkit.exe` from [Releases](https://github.com/yakeworld/jabkit-rs/releases).

## Quick Start

```bash
# Search academic literature
jabkit-rs fetch -q "3D eye movement nystagmus" -n 20

# Convert DOI to BibTeX
jabkit-rs doi-to-bibtex 10.1016/j.cmpb.2023.107526

# List available providers and key status
jabkit-rs list-providers

# Fetch by ID
jabkit-rs get-by-id --doi 10.1007/s00417-023-06145-3
jabkit-rs get-by-id --pmid 37488184
jabkit-rs get-by-id --arxiv 2306.12345
```

## Usage

### `fetch` — Search academic databases

```bash
jabkit-rs fetch [OPTIONS] -q <QUERY>

Options:
  -q, --query <QUERY>    Search query [required]
  -n, --num <NUM>        Results per provider [default: 10]
  -p, --provider <PROV>  Provider(s): s2, pubmed, crossref, arxiv, openalex, all [default: all]
  -y, --year <YEAR>      Filter by year (e.g. "2024-" or "2023-2025")
  -o, --output <FILE>    Output file (default: stdout / BibTeX)
  --journal <JOURNAL>    Filter by journal name
```

### `doi-to-bibtex` — DOI to BibTeX

```bash
jabkit-rs doi-to-bibtex <DOI> [DOI...]
jabkit-rs doi-to-bibtex 10.1007/s00417-023-06145-3 10.1038/s41598-023-37339-8
```

### `get-by-id` — Fetch by identifier

```bash
jabkit-rs get-by-id --doi <DOI>
jabkit-rs get-by-id --pmid <PMID>
jabkit-rs get-by-id --arxiv <ARXIV_ID>
jabkit-rs get-by-id --s2id <S2_PAPER_ID>
```

### `list-providers` — Show provider status

```bash
jabkit-rs list-providers
```

Output:
```
Crossref         FREE  ✓ no key needed
SemanticScholar  KEY   ✓ configured (S2_API_KEY)
Medline/PubMed   KEY   ✓ configured (PUBMED_API_KEY)
arXiv            FREE  ✓ no key needed
OpenAlex         KEY   ✓ configured (OPENALEX_API_KEY)
```

## API Keys

| Provider | Key | Source |
|:---------|:----|:-------|
| SemanticScholar | `S2_API_KEY` | https://semanticscholar.org/account |
| PubMed/MEDLINE | `PUBMED_API_KEY` | https://ncbi.nlm.nih.gov/account |
| OpenAlex | `OPENALEX_API_KEY` | https://openalex.org/account |
| Crossref | Free, no key needed | — |
| arXiv | Free, no key needed | — |

Key resolution priority:
1. Environment variable
2. `.env` file in current directory
3. GNOME Keyring (service: `org.jabref.customapikeys`)

### Setting keys

```bash
# Environment variable (Linux/macOS)
export S2_API_KEY="your-key-here"

# Environment variable (Windows PowerShell)
$env:S2_API_KEY = "your-key-here"

# GNOME Keyring (Linux only)
secret-tool store --label="S2_API_KEY" service org.jabref.customapikeys account S2_API_KEY
```

## Porcelain Mode (script-friendly)

```bash
jabkit-rs fetch -p s2 -q "3D eye" -n 5 --porcelain
# Output: BibTeX only, no log lines. Pipe to lit-import:
jabkit-rs fetch -q "vestibular" --porcelain | lit-import --bib -
```

## Build from Source

```bash
git clone https://github.com/yakeworld/jabkit-rs.git
cd jabkit-rs
cargo build --release
# Binary at: target/release/jabkit
```

### Cross-compile for Windows

```bash
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
# Binary at: target/x86_64-pc-windows-gnu/release/jabkit.exe
```

## Architecture

```
src/
├── main.rs          # Entry point + CLI parsing
├── cli.rs           # Subcommand routing
├── bibtex.rs        # BibTeX generation
├── keyring.rs       # API key resolution
└── provider/
    ├── mod.rs
    ├── semantic_scholar.rs  # Semantic Scholar API v2
    ├── pubmed.rs            # NCBI E-Utilities
    ├── crossref.rs          # Crossref REST API
    ├── arxiv.rs             # arXiv OAI-PMH
    └── openalex.rs          # OpenAlex REST API
```

## License

MIT
