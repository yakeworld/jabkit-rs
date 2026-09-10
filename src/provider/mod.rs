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
    fn key_env(&self) -> Option<&'static str> {
        None
    }
    /// True if this provider is a registered placeholder with no working API.
    /// Used by `list-providers` to label it so users don't assume it works.
    fn is_stub(&self) -> bool {
        false
    }
    async fn search(&self, query: &str, limit: usize) -> Result<SearchResult>;
    async fn fetch_by_id(&self, id: &str) -> Result<BibEntry>;
}

/// Shared HTTP client with a 30s request timeout and 10s connect timeout.
/// All providers use this (a process-wide singleton) instead of
/// `reqwest::Client::new()`, so every request carries the timeout/proxy
/// configuration and we do not rebuild a client per request.
///
/// If building the client fails we DO NOT silently fall back to
/// `Client::new()` (which would drop the timeouts and proxy settings) — we
/// panic with a clear message, because a missing timeout means a slow or
/// unresponsive upstream API can hang the process forever.
pub fn http_client() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .expect(
                    "failed to build HTTP client (timeout/proxy config could not be applied); \
                 refusing to fall back to a client without timeouts",
                )
        })
        .clone()
}

/// Default user agent for upstream API calls.
pub const USER_AGENT: &str = "jabkit/0.1 (mailto:yakeworld@gmail.com)";

/// Bounded, deadline-aware GET that deserialises a JSON body.
///
/// Retry policy (idempotent GET only):
/// - 429 (rate limit) / 5xx (server error) → retry with exponential backoff
/// - 4xx (auth / client error) → fail immediately, no retry
/// - network error (timeout / connect) → retry
///
/// At most `max_attempts` tries, capped by an overall `deadline` so a slow or
/// unresponsive upstream can never hang the process. This is the single place
/// where retry semantics live, so providers that use it share the same
/// behaviour (Astra P1: "unified behaviour, not just unified code shape").
pub async fn get_json<T: serde::de::DeserializeOwned>(
    url: &str,
    max_attempts: u32,
    deadline: std::time::Duration,
) -> Result<T> {
    use std::time::{Duration, Instant};
    let client = http_client();
    let started = Instant::now();
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        // Enforce the overall deadline before each attempt.
        if started.elapsed() >= deadline {
            anyhow::bail!(
                "GET {} exceeded {}s deadline after {} attempt(s)",
                url,
                deadline.as_secs(),
                attempt - 1
            );
        }
        let send_result = client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await;
        // Classify the outcome. `Some(resp)` = success; `Err` = failure whose
        // message is tagged retryable or not.
        let outcome: Result<reqwest::Response, anyhow::Error> = match send_result {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    Ok(resp)
                } else {
                    let body = resp.text().await.unwrap_or_default();
                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
                    {
                        Err(anyhow::anyhow!("__retryable__ {}: {}", status, body))
                    } else {
                        Err(anyhow::anyhow!("{}: {}", status, body))
                    }
                }
            }
            Err(e) => Err(e.into()),
        };

        match outcome {
            Ok(resp) => {
                return resp
                    .json()
                    .await
                    .map_err(|e| anyhow::anyhow!("parse JSON: {}", e));
            }
            Err(e) => {
                let msg = e.to_string();
                let retryable = msg.contains("__retryable__")
                    || msg.contains("error sending request")
                    || msg.contains("timeout")
                    || msg.contains("connect");
                if !retryable || attempt >= max_attempts {
                    anyhow::bail!("GET {} failed: {}", url, msg.replace("__retryable__ ", ""));
                }
                // Exponential backoff: ~1s, ~2s, ~4s ...
                let backoff = Duration::from_millis((1000u64) << attempt.saturating_sub(1));
                tokio::time::sleep(backoff).await;
            }
        }
    }
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
        Box::new(core::Core::new(keys.get("CORE_API_KEY").map(String::from))),
        Box::new(semantic_scholar::SemanticScholar::new(
            keys.get("S2_API_KEY").map(String::from),
        )),
        Box::new(pubmed::PubMed::new(
            keys.get("PUBMED_API_KEY").map(String::from),
        )),
        Box::new(openalex::OpenAlex::new(
            keys.get("OPENALEX_API_KEY").map(String::from),
        )),
        Box::new(ieee::Ieee::new(keys.get("IEEE_API_KEY").map(String::from))),
        Box::new(springer::Springer::new(
            keys.get("SPRINGER_API_KEY").map(String::from),
        )),
        Box::new(scopus::Scopus::new(
            keys.get("SCOPUS_API_KEY").map(String::from),
        )),
        Box::new(acm::Acm::new(keys.get("ACM_API_KEY").map(String::from))),
        Box::new(ads::Ads::new(keys.get("ADS_API_KEY").map(String::from))),
        Box::new(unpaywall::Unpaywall::new(
            keys.get("UNPAYWALL_EMAIL").map(String::from),
        )),
        Box::new(stubs::Biodiversity::new(
            keys.get("BIODIVERSITY_KEY").map(String::from),
        )),
    ]
}
