# jabkit-rs

**Academic literature search CLI** — Rust rewrite of jabkit.

Single binary, fast startup (vs ~15s Java original). Supports 26 academic providers with BibTeX output.

## Features

- 🚀 **Fast startup** — Rust native binary, no JVM, no Gradle
- 📦 **Single binary** ~4.0MB (vs 176MB JRE bundle)
- 🔍 **26 providers**: Crossref, arXiv, DBLP, DOAJ, EuropePMC, INSPIRE, SemanticScholar, Medline/PubMed, OpenAlex, Scopus, CORE, IEEE, Springer, ACM, ADS, Unpaywall, BiodiversityHL + 9 registered stubs
- 📝 **BibTeX output** — pipe directly to `lit-import`
- 🔑 **API key chain**: env var → `.env` file → GNOME Keyring
- 🔄 **DOI→BibTeX** converter
- 🌐 **Proxy support**: `--proxy socks5h://host:port` for all HTTP requests

## Installation

### Linux

```bash
# Download binary
curl -sL https://github.com/yakeworld/jabkit-rs/releases/latest/download/jabkit-linux-x86_64 -o jabkit

# Verify checksum (published as jabkit-linux-x86_64.sha256 in the same release)
sha256sum -c jabkit-linux-x86_64.sha256

chmod +x jabkit
sudo mv jabkit /usr/local/bin/

# Or build from source
git clone https://github.com/yakeworld/jabkit-rs.git
cd jabkit-rs
cargo build --release
# Binary at: target/release/jabkit
```

### Windows

Download `jabkit-windows-x86_64.exe` from [Releases](https://github.com/yakeworld/jabkit-rs/releases), rename to `jabkit.exe`.

### macOS

Download `jabkit-macos-aarch64` (Apple Silicon) or `jabkit-macos-x86_64` (Intel) from [Releases](https://github.com/yakeworld/jabkit-rs/releases).

## Quick Start

```bash
# Search academic literature (provider is required)
jabkit fetch --provider Crossref -q "3D eye movement nystagmus" --limit 20

# Search arXiv
jabkit fetch --provider arXiv -q "nystagmus" --limit 10

# Convert DOI to BibTeX
jabkit doi-to-bibtex 10.1016/j.cmpb.2023.107526

# List available providers and key status
jabkit list-providers

# Fetch by ID
jabkit get-by-id --provider Crossref --id 10.1007/s00417-023-06145-3
jabkit get-by-id --provider "Medline/PubMed" --id 37488184
jabkit get-by-id --provider arXiv --id 2306.12345
```

## Usage

### `fetch` — Search academic databases

```bash
jabkit fetch [OPTIONS] -q <QUERY>

Options:
  -q, --query <QUERY>       Search query [required]
  --provider <PROVIDER>     Provider name [required, see list-providers]
  --limit <LIMIT>           Max results [default: 20]
  -p, --porcelain           BibTeX only, no log lines (global flag)
  --proxy <URL>             Proxy URL (global flag)
```

### `doi-to-bibtex` — DOI to BibTeX (via Crossref)

```bash
jabkit doi-to-bibtex <DOI> [DOI...]
jabkit doi-to-bibtex 10.1007/s00417-023-06145-3 10.1038/s41598-023-37339-8

# Fail the whole command if ANY DOI fails (for scripted batch ingest)
jabkit doi-to-bibtex --strict 10.1007/s00417-023-06145-3 10.9999/fake.doi
```

Exit codes: `0` = all DOIs succeeded, or some failed but at least one succeeded (default; failures go to stderr). `1` = all DOIs failed, or any DOI failed with `--strict`. Use `--porcelain` for BibTeX-only stdout.

### `get-by-id` — Fetch by identifier

```bash
jabkit get-by-id --provider <PROVIDER> --id <ID>
jabkit get-by-id --provider Crossref --id 10.1007/s00417-023-06145-3
jabkit get-by-id --provider "Medline/PubMed" --id 37488184
jabkit get-by-id --provider arXiv --id 2306.12345
```

### `doctor` — Diagnose configuration

```bash
jabkit doctor
```

Shows, without ever printing key values: whether `secret-tool` (GNOME
keyring) is available, where each loaded key came from (`env` vs `keyring`),
the current `https_proxy`, whether the HTTP client (with its timeout/proxy
configuration) builds, and the implemented/stub provider split. Use it when a
provider unexpectedly reports "no key" or to confirm a key landed in the right
place.

```
secret-tool (GNOME keyring): not found (keyring fallback disabled)

Key sources:
  S2_API_KEY = env
  OPENALEX_API_KEY = keyring
  PUBMED_API_KEY = MISSING (PubMed)

HTTP client: ok (30s timeout, 10s connect)
Providers registered: 26 (17 implemented, 9 stub)
```

### `list-providers` — Show provider status

```bash
jabkit list-providers
```

Output:
```
Available providers:
  Crossref         FREE  ✓ no key needed
  arXiv            FREE  ✓ no key needed
  DBLP             FREE  ✓ no key needed
  DOAJ             FREE  ✓ no key needed
  EuropePMC        FREE  ✓ no key needed
  INSPIRE          FREE  ✓ no key needed
  SemanticScholar  KEY   ✓ configured (S2_API_KEY)
  Medline/PubMed   KEY   ✓ configured (PUBMED_API_KEY)
  OpenAlex         KEY   ✓ configured (OPENALEX_API_KEY)
  Scopus           KEY   ✓ configured (SCOPUS_API_KEY)
  ...
```

### `init` — Create .env template

```bash
jabkit init
```

Creates `.env` in current directory with all known key names as comments.

## API Keys

| Provider | Key | Source |
|:---------|:----|:-------|
| SemanticScholar | `S2_API_KEY` | https://semanticscholar.org/account |
| PubMed/MEDLINE | `PUBMED_API_KEY` | https://ncbi.nlm.nih.gov/account |
| OpenAlex | `OPENALEX_API_KEY` | https://openalex.org/account |
| Crossref | Free, no key needed | — |
| arXiv | Free, no key needed | — |
| DBLP | Free, no key needed | — |
| DOAJ | Free, no key needed | — |
| EuropePMC | Free, no key needed | — |
| INSPIRE | Free, no key needed | — |
| CORE | `CORE_API_KEY` | https://core.ac.uk/services/api |
| IEEE | `IEEE_API_KEY` | https://ieeexplore.ieee.org |
| Springer | `SPRINGER_API_KEY` | https://api.springer.com |
| Scopus | `SCOPUS_API_KEY` | https://www.elsevier.com |
| ACM | `ACM_API_KEY` | https://dl.acm.org |
| ADS (NASA) | `ADS_API_KEY` | https://ui.adsabs.harvard.edu |
| Unpaywall | `UNPAYWALL_EMAIL` | Your email (free) |
| BiodiversityHL | `BIODIVERSITY_KEY` | https://biodiversitylibrary.org |

Key resolution priority:
1. Environment variable
2. `.env` file in current directory
3. `~/.jabkit.env` (home fallback)
4. GNOME Keyring (service: `org.jabref.customapikeys`)

### Setting keys

```bash
# Environment variable (Linux/macOS)
export S2_API_KEY="your-key-here"

# Environment variable (Windows PowerShell)
$env:S2_API_KEY = "your-key-here"

# .env file
echo 'S2_API_KEY=your-key-here' > .env

# GNOME Keyring (Linux only)
secret-tool store --label="S2_API_KEY" service org.jabref.customapikeys account S2_API_KEY
```

## Porcelain Mode (script-friendly)

```bash
jabkit fetch --provider Crossref -q "3D eye" --limit 5 --porcelain
# Output: BibTeX only, no log lines. Pipe to lit-import:
jabkit fetch --provider Crossref -q "vestibular" --porcelain | lit-import --bib -
```

## Proxy Support

```bash
jabkit --proxy socks5h://100.65.157.17:9050 fetch --provider Crossref -q "test"
```

Routes all HTTP requests through the specified SOCKS5/HTTP proxy.

## Architecture

```
src/
├── main.rs          # Entry point + CLI routing
├── cli.rs           # Subcommand definitions
├── bibtex.rs        # BibTeX generation
├── keyring.rs       # API key resolution (env → .env → keyring)
└── provider/
    ├── mod.rs       # Provider trait + shared HTTP client (30s timeout)
    ├── crossref.rs  # Crossref REST API
    ├── arxiv.rs     # arXiv Atom XML API
    ├── dblp.rs      # DBLP API
    ├── doaj.rs      # DOAJ API
    ├── europe_pmc.rs# Europe PMC API
    ├── inspire.rs   # INSPIRE API
    ├── semantic_scholar.rs  # Semantic Scholar API v2
    ├── pubmed.rs    # NCBI E-Utilities
    ├── openalex.rs  # OpenAlex REST API
    ├── scopus.rs    # Scopus API
    ├── core.rs      # CORE API
    ├── ieee.rs      # IEEE Xplore
    ├── springer.rs  # Springer Link
    ├── acm.rs       # ACM Digital Library
    ├── ads.rs       # ADS (NASA)
    ├── unpaywall.rs # Unpaywall
    └── stubs.rs     # Registered but no public API (9 providers)
```

## Building

```bash
cargo build --release
# Binary at: target/release/jabkit

# Run tests
cargo test --release

# Cross-compile for Windows
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
# Binary at: target/x86_64-pc-windows-gnu/release/jabkit.exe
```

## License

MIT
