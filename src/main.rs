mod bibtex;
mod cli;
mod keyring;
mod provider;

use crate::bibtex::{deduplicate_keys, BibEntry};
use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
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
    s.push_str("#CORE_API_KEY=\n");
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

    // Apply proxy if specified — reqwest reads https_proxy/http_proxy env vars automatically
    if let Some(proxy) = &cli.proxy {
        std::env::set_var("https_proxy", proxy);
        std::env::set_var("http_proxy", proxy);
        std::env::set_var("all_proxy", proxy);
    }

    if !cli.porcelain {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp(None)
            .init();
    }

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
            let mut entries = result.entries;
            for e in entries.iter_mut() {
                e.generate_key();
            }
            deduplicate_keys(&mut entries);
            for entry in &entries {
                println!("{}", entry.to_bibtex());
            }
        }

        Commands::DoiToBibtex { dois, strict } => {
            let cr = providers
                .iter()
                .find(|p| p.name() == "Crossref")
                .ok_or_else(|| anyhow::anyhow!("Crossref provider not loaded"))?;
            let mut entries: Vec<BibEntry> = Vec::new();
            let mut failures = 0usize;
            for doi in dois {
                if !cli.porcelain {
                    log::info!("Fetching DOI: {}", doi);
                }
                match cr.fetch_by_id(doi).await {
                    Ok(mut entry) => {
                        entry.generate_key();
                        entries.push(entry);
                    }
                    Err(e) => {
                        failures += 1;
                        if cli.porcelain {
                            eprintln!("ERROR: {}: {}", doi, e);
                        } else {
                            log::error!("{}: {}", doi, e);
                        }
                    }
                }
            }
            if !entries.is_empty() {
                deduplicate_keys(&mut entries);
                for entry in &entries {
                    println!("{}", entry.to_bibtex());
                }
            }
            if failures > 0 {
                eprintln!("doi-to-bibtex: {failures}/{} DOI(s) failed", dois.len());
                if *strict || failures == dois.len() {
                    anyhow::bail!("{} of {} DOI(s) failed", failures, dois.len());
                }
            }
        }

        Commands::ListProviders => {
            let has_keys = api_keys.has_any();
            let has_dotenv = std::path::Path::new(".env").exists()
                || home::home_dir()
                    .map(|h| h.join(".jabkit.env").exists())
                    .unwrap_or(false);

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
            let selected = providers
                .iter()
                .find(|p| p.name().eq_ignore_ascii_case(provider))
                .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider))?;
            match selected.fetch_by_id(id).await {
                Ok(mut entry) => {
                    entry.generate_key();
                    println!("{}", entry.to_bibtex());
                }
                Err(e) => {
                    anyhow::bail!("{}: {}", provider, e);
                }
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

#[cfg(test)]
mod tests {
    use crate::bibtex::{BibEntry, EntryType, Field};
    use crate::cli::{Cli, Commands};
    use clap::Parser;

    #[test]
    fn test_fetch_command() {
        let cli = Cli::try_parse_from([
            "jabkit",
            "fetch",
            "--provider",
            "Crossref",
            "--query",
            "BPPV",
            "--limit",
            "5",
        ])
        .unwrap();
        match &cli.command {
            Commands::Fetch {
                provider,
                query,
                limit,
            } => {
                assert_eq!(provider, "Crossref");
                assert_eq!(query, "BPPV");
                assert_eq!(*limit, 5);
            }
            _ => panic!("expected Fetch command"),
        }
    }

    #[test]
    fn test_fetch_default_limit() {
        let cli =
            Cli::try_parse_from(["jabkit", "fetch", "--provider", "arXiv", "--query", "test"])
                .unwrap();
        match &cli.command {
            Commands::Fetch { limit, .. } => assert_eq!(*limit, 20),
            _ => panic!("expected Fetch command"),
        }
    }

    #[test]
    fn test_proxy_global_arg() {
        let cli = Cli::try_parse_from([
            "jabkit",
            "--proxy",
            "socks5h://tor:9050",
            "fetch",
            "--provider",
            "Crossref",
            "--query",
            "x",
        ])
        .unwrap();
        assert_eq!(cli.proxy.as_deref(), Some("socks5h://tor:9050"));
    }

    #[test]
    fn test_porcelain_global_arg() {
        let cli = Cli::try_parse_from([
            "jabkit",
            "--porcelain",
            "fetch",
            "--provider",
            "Crossref",
            "--query",
            "x",
        ])
        .unwrap();
        assert!(cli.porcelain);
    }

    #[test]
    fn test_list_providers_command() {
        let cli = Cli::try_parse_from(["jabkit", "list-providers"]).unwrap();
        assert!(matches!(cli.command, Commands::ListProviders));
    }

    #[test]
    fn test_init_command() {
        let cli = Cli::try_parse_from(["jabkit", "init"]).unwrap();
        assert!(matches!(cli.command, Commands::Init));
    }

    #[test]
    fn test_doi_to_bibtex_command() {
        let cli = Cli::try_parse_from(["jabkit", "doi-to-bibtex", "10.1234/test"]).unwrap();
        match &cli.command {
            Commands::DoiToBibtex { dois, strict } => {
                assert_eq!(dois[0], "10.1234/test");
                assert!(!strict, "strict should default to false");
            }
            _ => panic!("expected DoiToBibtex"),
        }
    }

    #[test]
    fn test_doi_to_bibtex_strict_flag() {
        let cli =
            Cli::try_parse_from(["jabkit", "doi-to-bibtex", "--strict", "10.1234/test"]).unwrap();
        match &cli.command {
            Commands::DoiToBibtex { strict, .. } => assert!(*strict),
            _ => panic!("expected DoiToBibtex"),
        }
    }

    #[test]
    fn test_get_by_id_command() {
        let cli = Cli::try_parse_from([
            "jabkit",
            "get-by-id",
            "--provider",
            "PubMed",
            "--id",
            "12345678",
        ])
        .unwrap();
        match &cli.command {
            Commands::GetById { provider, id } => {
                assert_eq!(provider, "PubMed");
                assert_eq!(id, "12345678");
            }
            _ => panic!("expected GetById"),
        }
    }

    #[test]
    fn test_bibtex_entry_article() {
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Author, "Smith, John and Doe, Jane".into());
        e.set_field(Field::Title, "A Test Paper".into());
        e.set_field(Field::Journal, "Test Journal".into());
        e.set_field(Field::Year, "2024".into());
        e.set_field(Field::Doi, "10.1234/test.2024".into());
        e.generate_key();
        let bib = e.to_bibtex();
        assert!(bib.starts_with("@article{smith2024,"));
        assert!(bib.contains("{Smith, John and Doe, Jane"));
        assert!(bib.contains("{A Test Paper"));
        assert!(bib.contains("{10.1234/test.2024"));
    }

    #[test]
    fn test_bibtex_key_generation() {
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Author, "Yang, Xiaokai".into());
        e.set_field(Field::Year, "2026".into());
        e.generate_key();
        assert_eq!(e.citation_key, "yang2026");
    }

    #[test]
    fn test_bibtex_key_no_author_with_doi() {
        let mut e = BibEntry::new(EntryType::Misc);
        e.set_field(Field::Title, "No Author Paper".into());
        e.set_field(Field::Doi, "10.1234/test.2024".into());
        e.generate_key();
        assert_eq!(e.citation_key, "doi_101234test2024");
    }

    #[test]
    fn test_bibtex_key_no_author_no_doi() {
        let mut e = BibEntry::new(EntryType::Misc);
        e.set_field(Field::Title, "No Author Paper".into());
        e.generate_key();
        assert_eq!(e.citation_key, "unknown");
    }

    #[test]
    fn test_bibtex_key_cjk_author_with_doi() {
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Author, "王, 晓凯".into());
        e.set_field(Field::Year, "2024".into());
        e.set_field(Field::Doi, "10.3760/cma.j.2024.01.001".into());
        e.generate_key();
        // CJK stripped → no ASCII author → DOI fallback
        assert_eq!(e.citation_key, "doi_103760cmaj202401001");
    }

    #[test]
    fn test_bibtex_key_cjk_author_no_doi() {
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Author, "王, 晓凯".into());
        e.set_field(Field::Year, "2024".into());
        e.generate_key();
        assert_eq!(e.citation_key, "nauthor_2024");
    }

    #[test]
    fn test_inbook_rendering() {
        let mut e = BibEntry::new(EntryType::InBook);
        e.set_field(Field::Author, "Smith, John".into());
        e.set_field(Field::Title, "A Book Chapter".into());
        e.set_field(Field::Booktitle, "Great Encyclopedia".into());
        e.set_field(Field::Year, "2020".into());
        e.set_field(Field::Pages, "4194-4199".into());
        e.set_field(Field::Publisher, "Springer".into());
        e.generate_key();
        let bib = e.to_bibtex();
        assert!(bib.starts_with("@inbook{smith2020,"));
        assert!(bib.contains("booktitle = {Great Encyclopedia}"));
        assert!(!bib.contains("journal = "));
    }

    #[test]
    fn test_bibtex_field_set_empty() {
        let mut e = BibEntry::new(EntryType::Article);
        e.set_field(Field::Title, "Valid Title".into());
        e.set_field(Field::Abstract, "".into());
        assert!(e.get(Field::Abstract).is_none());
        assert_eq!(e.get(Field::Title), Some("Valid Title"));
    }

    #[test]
    fn test_arxiv_eprint_not_overwritten() {
        use crate::provider::arxiv::parse_arxiv_xml_test;
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
  <entry>
    <id>http://arxiv.org/abs/2306.12345v2</id>
    <title>Test Paper</title>
    <summary>A summary.</summary>
    <author><name><family_name>Smith</family_name><given_name>John</given_name></name></author>
    <published>2023-06-21T00:00:00Z</published>
    <link title="pdf" href="http://arxiv.org/pdf/2306.12345v2" rel="related" type="application/pdf"/>
  </entry>
</feed>"#;
        let entries = parse_arxiv_xml_test(xml).unwrap();
        assert_eq!(entries.len(), 1);
        let eprint = entries[0].get(Field::Eprint).unwrap().to_string();
        assert!(
            eprint.starts_with("2306.12345"),
            "eprint should be the real arXiv ID, got: {}",
            eprint
        );
        assert_ne!(
            eprint, "arXiv",
            "eprint must not be the literal string 'arXiv'"
        );
    }

    #[test]
    fn test_citation_key_dedup() {
        // Use the shared production function instead of a copied algorithm
        let mut entries = vec![
            {
                let mut e = BibEntry::new(EntryType::Article);
                e.citation_key = "yang2026".into();
                e
            },
            {
                let mut e = BibEntry::new(EntryType::Article);
                e.citation_key = "yang2026".into();
                e
            },
            {
                let mut e = BibEntry::new(EntryType::Article);
                e.citation_key = "yang2026".into();
                e
            },
            {
                let mut e = BibEntry::new(EntryType::Article);
                e.citation_key = "smith2026".into();
                e
            },
        ];
        crate::bibtex::deduplicate_keys(&mut entries);
        assert_eq!(entries[0].citation_key, "yang2026");
        assert_eq!(entries[1].citation_key, "yang2026a");
        assert_eq!(entries[2].citation_key, "yang2026b");
        assert_eq!(entries[3].citation_key, "smith2026");
        // All keys unique
        let unique: std::collections::HashSet<_> =
            entries.iter().map(|e| &e.citation_key).collect();
        assert_eq!(unique.len(), 4, "all citation keys must be unique");
    }
}
