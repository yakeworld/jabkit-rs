mod bibtex;
mod cli;
mod keyring;
mod provider;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use provider::Provider;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.porcelain {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp(None)
            .init();
    }

    // Load API keys from env first, fallback to system keyring
    let api_keys = keyring::ApiKeys::from_env().with_keyring_fallback();

    let providers = provider::all_providers(&api_keys);

    match &cli.command {
        Commands::Fetch {
            provider,
            query,
            limit,
        } => {
            let selected = providers
                .iter()
                .find(|p| p.name().eq_ignore_ascii_case(provider))
                .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}. Use 'jabkit list-providers' to see available providers.", provider))?;

            if !cli.porcelain {
                log::info!("Searching {} for '{}'...", selected.name(), query);
            }

            let result = selected.search(query, *limit).await?;

            if !cli.porcelain {
                log::info!("Found {} results", result.total_found);
            }

            // Generate citation keys and output BibTeX
            for entry in &result.entries {
                let mut e = entry.clone();
                e.generate_key();
                print!("{}", e.to_bibtex());
            }
        }

        Commands::DoiToBibtex { dois } => {
            // CrossRef is the canonical DOI → BibTeX provider
            let cr = providers
                .iter()
                .find(|p| p.name() == "Crossref")
                .ok_or_else(|| anyhow::anyhow!("Crossref provider not loaded"))?;

            for doi in dois {
                if !cli.porcelain {
                    log::info!("Fetching DOI: {}", doi);
                }
                match cr.fetch_by_id(doi).await {
                    Ok(mut entry) => {
                        entry.generate_key();
                        print!("{}", entry.to_bibtex());
                    }
                    Err(e) => {
                        if cli.porcelain {
                            eprintln!("ERROR: {}: {}", doi, e);
                        } else {
                            log::error!("{}: {}", doi, e);
                        }
                    }
                }
            }
        }

        Commands::ListProviders => {
            println!("Available providers:");
            for p in &providers {
                let s2_key = if api_keys.semantic_scholar.is_some() {
                    "key"
                } else {
                    "no key"
                };
                let pm_key = if api_keys.pubmed.is_some() {
                    "key"
                } else {
                    "no key"
                };
                let oa_key = if api_keys.openalex.is_some() {
                    "key"
                } else {
                    "no key"
                };
                let key_info = match p.name() {
                    "SemanticScholar" => format!(" [{}]", s2_key),
                    "Medline/PubMed" => format!(" [{}]", pm_key),
                    "OpenAlex" => format!(" [{}]", oa_key),
                    _ => String::new(),
                };
                println!("  {}{}", p.name(), key_info);
            }
            println!("\nSet API keys via environment:");
            println!("  S2_API_KEY       SemanticScholar");
            println!("  PUBMED_API_KEY   Medline/PubMed");
            println!("  OPENALEX_API_KEY OpenAlex");
            println!("\nOr store in system keyring (org.jabref.customapikeys).");
        }

        Commands::GetById { provider, id } => {
            let selected = providers
                .iter()
                .find(|p| p.name().eq_ignore_ascii_case(provider))
                .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider))?;

            match selected.fetch_by_id(id).await {
                Ok(mut entry) => {
                    entry.generate_key();
                    print!("{}", entry.to_bibtex());
                }
                Err(e) => {
                    anyhow::bail!("{}: {}", provider, e);
                }
            }
        }
    }

    Ok(())
}
