use anyhow::Result;
use std::collections::HashMap;
use std::process::Command;

/// API keys for all providers. Lookup by env var name.
#[derive(Clone, Default)]
pub struct ApiKeys {
    semantic_scholar: Option<String>,
    pubmed: Option<String>,
    openalex: Option<String>,
    extra: HashMap<String, String>,
}

impl std::fmt::Debug for ApiKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("ApiKeys");
        s.field("semantic_scholar", &self.semantic_scholar.as_ref().map(|_| "***"));
        s.field("pubmed", &self.pubmed.as_ref().map(|_| "***"));
        s.field("openalex", &self.openalex.as_ref().map(|_| "***"));
        let extra: std::collections::BTreeMap<&String, &str> =
            self.extra.iter().map(|(k, _)| (k, "***")).collect();
        s.field("extra", &extra);
        s.finish()
    }
}

impl ApiKeys {
    /// All known env var names for API keys
    const ALL_ENV_KEYS: &'static [&'static str] = &[
        "CORE_API_KEY", "S2_API_KEY", "SEMANTIC_SCHOLAR_API_KEY", "PUBMED_API_KEY", "OPENALEX_API_KEY",
        "IEEE_API_KEY", "SCOPUS_API_KEY", "SPRINGER_API_KEY",
        "ACM_API_KEY", "ADS_API_KEY", "UNPAYWALL_EMAIL",
        "BIODIVERSITY_KEY",
    ];

    /// Load keys from environment variables
    pub fn from_env() -> Self {
        let mut extra = HashMap::new();
        for key in Self::ALL_ENV_KEYS {
            if let Ok(v) = std::env::var(key) {
                if !v.is_empty() {
                    extra.insert(key.to_string(), v);
                }
            }
        }
        Self {
            semantic_scholar: extra.remove("S2_API_KEY"),
            pubmed: extra.remove("PUBMED_API_KEY"),
            openalex: extra.remove("OPENALEX_API_KEY"),
            extra,
        }
    }

    /// Fallback: try GNOME Keyring for the 3 well-known JabRef keys
    pub fn with_keyring_fallback(self) -> Self {
        let mut keys = self;
        if keys.semantic_scholar.is_none() {
            keys.semantic_scholar = secret_tool_lookup("SemanticScholar");
        }
        if keys.pubmed.is_none() {
            keys.pubmed = secret_tool_lookup("Medline/PubMed");
        }
        if keys.openalex.is_none() {
            keys.openalex = secret_tool_lookup("OpenAlex");
        }
        keys
    }

    /// Get a key by its env var name
    pub fn get(&self, env_name: &str) -> Option<&str> {
        match env_name {
            "S2_API_KEY" => self.semantic_scholar.as_deref(),
            "PUBMED_API_KEY" => self.pubmed.as_deref(),
            "OPENALEX_API_KEY" => self.openalex.as_deref(),
            _ => self.extra.get(env_name).map(|s| s.as_str()),
        }
    }

    /// Check if any key is set
    pub fn has_any(&self) -> bool {
        self.semantic_scholar.is_some()
            || self.pubmed.is_some()
            || self.openalex.is_some()
            || !self.extra.is_empty()
    }
}

/// Access GNOME Keyring via `secret-tool` CLI.
/// Supports both plaintext keys and JabRef AES-encrypted blobs.
fn secret_tool_lookup(account: &str) -> Option<String> {
    let output = Command::new("secret-tool")
        .args(["lookup", "service", "org.jabref.customapikeys", "account", account])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8(output.stdout).ok()?;
    let raw = raw.trim().to_string();
    if raw.is_empty() {
        return None;
    }
    // Try JabRef AES decryption first
    if let Ok(plain) = decrypt_jabref(&raw) {
        if !plain.is_empty() {
            return Some(plain);
        }
    }
    Some(raw)
}

/// JabRef AES/CBC/PKCS5Padding decryption
fn decrypt_jabref(ciphertext_b64: &str) -> Result<String> {
    use sha2::{Digest, Sha256};
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok().map(|s| s.trim().to_string())
        .unwrap_or_else(|| "localhost".to_string());
    let user = std::env::var("USER").unwrap_or_else(|_| "yakeworld".to_string());
    let key_str = format!("{}-{}", user, hostname);

    let mut hasher = Sha256::new();
    hasher.update(key_str.as_bytes());
    let hash = hasher.finalize();
    let key = &hash[..16];
    let iv = b"ThisIsA128BitKey";

    let ciphertext = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(ciphertext_b64)
            .map_err(|e| anyhow::anyhow!("Base64: {}", e))?
    };

    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
    let mut buf = ciphertext.clone();
    let pt = Aes128CbcDec::new(key.into(), iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| anyhow::anyhow!("AES: {:?}", e))?;
    Ok(String::from_utf8(pt.to_vec())?)
}
