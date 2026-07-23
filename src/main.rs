mod bibtex;
mod cli;
mod keyring;
mod provider;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use provider::Provider;
use std::io::Write;

fn load_dotenv() {
    if dotenvy::dotenv().is_ok() {
        return;
    }
    if let Some(home) = home::home_dir() {
        let path = home.join(".jabkit.env");
        if path.exists() {
            let _ = dotenvy::from_path(&path);
        }
    }
}

fn generate_dotenv_template() -> String {
    let mut s = String::new();
    s.push_str("# jabkit API Keys\n");
    s.push_str("# Uncomment and fill in your keys, or set as environment variables.\n");
    s.push_str("# Priority: env var > .env file > GNOME Keyring (Linux)\n\n");
    s.push_str("# General\n");
    s.push_str("#S2_API_KEY=\n");
    s.push_str("#PUBMED_API_KEY=\n");
    s.push_str("#OPENALEX_API_KEY=\n\n");
    s.push_str("# Commercial / subscription\n");
    s.push_str("#IEEE_API_KEY=\n");
    s.push_str("#SPRINGER_API_KEY=\n");
    s.push_str("#SCOPUS_API_KEY=\n");
    s.push_str("#ACM_API_KEY=\n\n");
    s.push_str("# Free registration\n");
    s.push_str("#ADS_API_KEY=\n");
    s.push_str("#UNPAYWALL_EMAIL=your@email.com\n");
    s.push_str("#BIODIVERSITY_KEY=\n");
    s
}

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();

    let cli = Cli::parse();

    if !cli.porcelain {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp(None)
            .init();
    }

    let api_keys = keyring::ApiKeys::from_env().with_keyring_fallback();
    let providers = provider::all_providers(&api_keys);

    match &cli.command {
        Commands::Fetch { provider, query, limit } => {
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
            for entry in &result.entries {
                let mut e = entry.clone();
                e.generate_key();
                print!("{}", e.to_bibtex());
            }
        }

        Commands::DoiToBibtex { dois } => {
            let cr = providers.iter().find(|p| p.name() == "Crossref")
                .ok_or_else(|| anyhow::anyhow!("Crossref provider not loaded"))?;
            for doi in dois {
                if !cli.porcelain { log::info!("Fetching DOI: {}", doi); }
                match cr.fetch_by_id(doi).await {
                    Ok(mut entry) => { entry.generate_key(); print!("{}", entry.to_bibtex()); }
                    Err(e) => { if cli.porcelain { eprintln!("ERROR: {}: {}", doi, e); } else { log::error!("{}: {}", doi, e); } }
                }
            }
        }

        Commands::ListProviders => {
            let has_keys = api_keys.has_any();
            let has_dotenv = std::path::Path::new(".env").exists()
                || home::home_dir().map(|h| h.join(".jabkit.env").exists()).unwrap_or(false);

            println!("Available providers:");
            for p in &providers {
                let key_status = match p.key_env() {
                    Some(env) => match api_keys.get(env) {
                        Some(_) => " [key]",
                        None => " [no key]",
                    },
                    None => "",
                };
                println!("  {}{}", p.name(), key_status);
            }

            if has_keys {
                println!("\nKeys loaded");
            } else if !has_dotenv {
                println!("\nNo .env found. Run 'jabkit init' to create one.");
            } else {
                println!("\nSet keys in .env file or as environment variables.");
            }
            println!("Run 'jabkit init' to generate a .env template.");
        }

        Commands::GetById { provider, id } => {
            let selected = providers.iter()
                .find(|p| p.name().eq_ignore_ascii_case(provider))
                .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider))?;
            match selected.fetch_by_id(id).await {
                Ok(mut entry) => { entry.generate_key(); print!("{}", entry.to_bibtex()); }
                Err(e) => { anyhow::bail!("{}: {}", provider, e); }
            }
        }

        Commands::Init => {
            let path = std::path::Path::new(".env");
            if path.exists() {
                log::warn!(".env already exists in current directory.");
                return Ok(());
            }
            let template = generate_dotenv_template();
            let mut f = std::fs::File::create(path)?;
            f.write_all(template.as_bytes())?;
            println!("Created .env in {}", std::env::current_dir()?.display());
            println!("Edit it to add your API keys, then run 'jabkit list-providers' to verify.");
        }
    }

    Ok(())
}
