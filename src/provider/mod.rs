pub mod acm;
pub mod ads;
pub mod arxiv;
pub mod core;
pub mod crossref;
pub mod dblp;
pub mod doaj;
pub mod europe_pmc;
pub mod ieee;
pub mod inspire;
pub mod openalex;
pub mod pubmed;
pub mod scopus;
pub mod semantic_scholar;
pub mod springer;
pub mod stubs;
pub mod unpaywall;

use anyhow::Result;
use async_trait::async_trait;

use crate::bibtex::BibEntry;
use crate::keyring::ApiKeys;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub entries: Vec<BibEntry>,
    pub total_found: usize,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    fn key_env(&self) -> Option<&'static str> { None }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult>;
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry>;
}

pub fn all_providers(keys: &ApiKeys) -> Vec<Box<dyn Provider>> {
    vec![
        // Free, no key needed
        Box::new(crossref::CrossRef),
        Box::new(arxiv::ArXiv),
        Box::new(dblp::Dblp),
        Box::new(doaj::Doaj),
        Box::new(europe_pmc::EuropePmc),
        Box::new(inspire::Inspire),
        // Stubs: registered but API unavailable
        Box::new(stubs::CiteSeer),
        Box::new(stubs::Isidore),
        Box::new(stubs::ZbMath),
        Box::new(stubs::MathSciNet),
        Box::new(stubs::ResearchGate),
        Box::new(stubs::ScholarArchive),
        Box::new(stubs::Gvk),
        Box::new(stubs::Lobid),
        Box::new(stubs::Doab),
        // Key required
        Box::new(core::Core::new(
            keys.get("CORE_API_KEY").map(String::from),
        )),
        Box::new(semantic_scholar::SemanticScholar::new(
            keys.get("S2_API_KEY").map(String::from),
        )),
        Box::new(pubmed::PubMed::new(
            keys.get("PUBMED_API_KEY").map(String::from),
        )),
        Box::new(openalex::OpenAlex::new(
            keys.get("OPENALEX_API_KEY").map(String::from),
        )),
        Box::new(ieee::Ieee::new(
            keys.get("IEEE_API_KEY").map(String::from),
        )),
        Box::new(springer::Springer::new(
            keys.get("SPRINGER_API_KEY").map(String::from),
        )),
        Box::new(scopus::Scopus::new(
            keys.get("SCOPUS_API_KEY").map(String::from),
        )),
        Box::new(acm::Acm::new(
            keys.get("ACM_API_KEY").map(String::from),
        )),
        Box::new(ads::Ads::new(
            keys.get("ADS_API_KEY").map(String::from),
        )),
        Box::new(unpaywall::Unpaywall::new(
            keys.get("UNPAYWALL_EMAIL").map(String::from),
        )),
        Box::new(stubs::Biodiversity::new(
            keys.get("BIODIVERSITY_KEY").map(String::from),
        )),
    ]
}
