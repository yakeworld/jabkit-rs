use anyhow::Result;
use std::process::Command;

/// API keys for various providers
#[derive(Debug, Clone, Default)]
pub struct ApiKeys {
    pub semantic_scholar: Option<String>,
    pub pubmed: Option<String>,
    pub openalex: Option<String>,
}

impl ApiKeys {
    /// Load keys from environment variables
    pub fn from_env() -> Self {
        Self {
            semantic_scholar: std::env::var("S2_API_KEY").ok().filter(|s| !s.is_empty()),
            pubmed: std::env::var("PUBMED_API_KEY").ok().filter(|s| !s.is_empty()),
            openalex: std::env::var("OPENALEX_API_KEY").ok().filter(|s| !s.is_empty()),
        }
    }

    /// Load keys from system keyring via secret-tool CLI
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

    // Fallback: plaintext key
    Some(raw)
}

/// JabRef AES/CBC/PKCS5Padding decryption
/// Key = SHA-256("{user}-{hostname}")[:16], IV = b"ThisIsA128BitKey"
fn decrypt_jabref(ciphertext_b64: &str) -> Result<String> {
    use sha2::{Digest, Sha256};

    let hostname = get_hostname();
    let user = std::env::var("USER").unwrap_or_else(|_| "yakeworld".to_string());
    let key_str = format!("{}-{}", user, hostname);

    let mut hasher = Sha256::new();
    hasher.update(key_str.as_bytes());
    let hash = hasher.finalize();
    let key = &hash[..16];

    let ciphertext = base64_decode(ciphertext_b64)?;
    let iv = b"ThisIsA128BitKey";

    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

    let mut buf = ciphertext.clone();
    let pt = Aes128CbcDec::new(key.into(), iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| anyhow::anyhow!("AES decryption failed: {:?}", e))?;

    Ok(String::from_utf8(pt.to_vec())?)
}

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| anyhow::anyhow!("Base64 decode failed: {}", e))
}

fn get_hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "localhost".to_string())
}
