pub mod arxiv;
pub mod crossref;
pub mod openalex;
pub mod pubmed;
pub mod semantic_scholar;

use anyhow::Result;
use async_trait::async_trait;

use crate::bibtex::BibEntry;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub entries: Vec<BibEntry>,
    pub total_found: usize,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult>;
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry>;
}

pub fn all_providers(api_keys: &crate::keyring::ApiKeys) -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(crossref::CrossRef),
        Box::new(semantic_scholar::SemanticScholar::new(
            api_keys.semantic_scholar.clone(),
        )),
        Box::new(pubmed::PubMed::new(api_keys.pubmed.clone())),
        Box::new(arxiv::ArXiv),
        Box::new(openalex::OpenAlex::new(api_keys.openalex.clone())),
    ]
}
