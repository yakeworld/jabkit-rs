//! Resilient HTTP: key-pool rotation + exit rotation + bounded retry.
//!
//! Design (2026-09-26, user directive: "key 轮换, IP 限制加出口轮替, 针对所有源"):
//! - 429 is usually IP-level (measured 2026-09: S2 blocks the egress IP, rotating
//!   keys alone does not help). Strategy rotates BOTH dimensions:
//!   * API key pool: env vars may hold a comma-separated pool (S2_API_KEY=k1,k2).
//!     A 429/5xx advances to the next key.
//!   * Exit: "direct" + every comma-separated JABKIT_PROXY entry. Once all keys
//!     have been tried on the current exit, switch exit (and for Tor SOCKS exits
//!     request a NEWNYM circuit via the Tor control port) before retrying.
//! - Retry budget: bounded by JABKIT_MAX_RETRIES (default 8, 0..=16) with
//!   exponential backoff 1s/2s/4s/8s (capped) so wall time stays sane.
//! - Non-429/5xx statuses fail fast. Network errors count as retryable.
//!
//! Usage: `get_resilient(&|key| url, "jabkit/0.1", keys, KeyPlacement::Header("x-api-key"))`.
//! The closure receives the current key ("" when keyless) and returns the URL —
//! this uniformly covers header keys (closure ignores `key`), `Authorization:
//! Bearer` (Header), and URL-embedded keys (`apikey=`/`key=`/`email=`, the
//! closure bakes the key into the URL with KeyPlacement::Baked).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use reqwest::Response;

/// Where the current key is applied by `get_resilient`.
pub enum KeyPlacement {
    /// `header(name, key)` when key non-empty — e.g. `x-api-key`
    Header(&'static str),
    /// `Authorization: Bearer <key>` when key non-empty
    Bearer,
    /// Key is baked into the URL by the closure; no header is added.
    Baked,
}

struct Exit {
    label: String,
    proxy: Option<String>,
    tor_control: Option<String>, // host:port of Tor control port
}

fn parse_tor_control(proxy: &str) -> Option<String> {
    // Only SOCKS5(S) proxies on the well-known Tor port get circuit rotation.
    if !(proxy.starts_with("socks5://") || proxy.starts_with("socks5h://")) {
        return None;
    }
    let rest = proxy
        .trim_start_matches("socks5h://")
        .trim_start_matches("socks5://");
    let (host, port) = rest.split_once(':')?;
    if port == "9050" {
        Some(format!("{host}:9051"))
    } else {
        None
    }
}

fn exits() -> Vec<Exit> {
    let mut v = vec![Exit {
        label: "direct".into(),
        proxy: None,
        tor_control: None,
    }];
    if let Ok(p) = std::env::var("JABKIT_PROXY") {
        for e in p.split(',') {
            let e = e.trim();
            if e.is_empty() {
                continue;
            }
            if v.iter().any(|x| x.proxy.as_deref() == Some(e)) {
                continue;
            }
            v.push(Exit {
                label: e.to_string(),
                proxy: Some(e.to_string()),
                tor_control: parse_tor_control(e),
            });
        }
    }
    v
}

/// Cached reqwest clients per exit (proxy config is baked into the client).
fn client_cache() -> &'static Mutex<HashMap<String, reqwest::Client>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, reqwest::Client>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn client_for(exit: &Exit) -> reqwest::Client {
    let mut cache = client_cache().lock().unwrap();
    if !cache.contains_key(&exit.label) {
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10));
        if let Some(proxy) = &exit.proxy {
            match reqwest::Proxy::all(proxy) {
                Ok(p) => builder = builder.proxy(p),
                Err(e) => eprintln!("JABKIT_PROXY {proxy:?} invalid: {e}"),
            }
        }
        cache.insert(
            exit.label.clone(),
            builder.build().unwrap_or_else(|_| reqwest::Client::new()),
        );
    }
    cache[&exit.label].clone()
}

/// Send `SIGNAL NEWNYM` to a Tor control port (new circuit = new egress IP).
/// Bounded to 13s; failure just means the next request reuses the circuit.
fn tor_newnym(control: &str) {
    let control = control.to_string();
    use std::io::{Read, Write};
    let result = std::thread::spawn(move || -> std::io::Result<()> {
        let addr: std::net::SocketAddr = control
            .parse()
            .map_err(|e: std::net::AddrParseError| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
        let mut stream = std::net::TcpStream::connect_timeout(
            &addr,
            Duration::from_secs(3),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut greeting = [0u8; 128];
        let n = stream.read(&mut greeting)?;
        if !&greeting[..n].starts_with(b"250+") {
            return Ok(());
        }
        let _ = stream.write_all(b"AUTHENTICATE\r\n");
        let _ = stream.read(&mut greeting);
        let _ = stream.write_all(b"SIGNAL NEWNYM\r\n");
        let _ = stream.read(&mut greeting);
        Ok(())
    })
    .join();
    if let Err(e) = result {
        eprintln!("resilient: NEWNYM failed: {e:?}");
    }
}

fn is_retryable(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

fn advance_exit(exit_idx: &mut usize, exits: &[Exit]) {
    *exit_idx = (*exit_idx + 1) % exits.len();
    let cur = &exits[*exit_idx];
    if let Some(control) = &cur.tor_control {
        eprintln!("resilient: switching exit -> {control} (NEWNYM)");
        tor_newnym(control);
    } else {
        eprintln!("resilient: switching exit -> {}", cur.label);
    }
}

/// Perform a GET with key + exit rotation and bounded retry.
///
/// Returns the first non-retryable response. Retry policy:
/// - 429: key-level + IP-level → rotate key first, then exit (NEWNYM on Tor).
///   Bounded; on exhaustion returns the LAST response.
/// - 401/403: key invalid (key-level, not IP-level) → rotate to the next key
///   and continue, so a dead key anywhere in the pool is skipped without
///   burning the whole retry budget on exits.
/// - Network errors: retried like 429; if no response was ever obtained, the
///   last network error is returned.
pub async fn get_resilient<F>(
    url_for: &F,
    ua: &str,
    keys: &[String],
    placement: KeyPlacement,
) -> Result<Response, reqwest::Error>
where
    F: Fn(&str) -> String,
{
    let exits = exits();
    let max_retries = std::env::var("JABKIT_MAX_RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8)
        .clamp(0, 16);

    let mut key_idx = 0usize;
    let mut exit_idx = 0usize;
    let mut last_resp: Option<Response> = None;
    let mut last_err: Option<reqwest::Error> = None;

    for attempt in 0..=max_retries {
        let exit = &exits[exit_idx % exits.len()];
        let client = client_for(exit);
        let key = if keys.is_empty() {
            ""
        } else {
            keys[key_idx % keys.len()].as_str()
        };
        let url = url_for(key);
        let mut req = client.get(&url).header("User-Agent", ua);
        match placement {
            KeyPlacement::Header(h) if !key.is_empty() => req = req.header(h, key),
            KeyPlacement::Bearer if !key.is_empty() => {
                req = req.header("Authorization", format!("Bearer {key}"))
            }
            _ => {}
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if !is_retryable(status) {
                    // 401/403 = key invalid (key-level, not IP-level). With a
                    // multi-key pool, rotate to the next key and skip the dead
                    // one — don't return immediately, and don't burn the budget
                    // on exit switching either.
                    if (status.as_u16() == 401 || status.as_u16() == 403)
                        && keys.len() > 1
                    {
                        key_idx += 1;
                        last_resp = Some(resp);
                        continue;
                    }
                    return Ok(resp);
                }
                // Retryable status (429/5xx): keep the response to report on
                // budget exhaustion. Body left undrained — 429 bodies are
                // small; the cost is one non-pooled connection per attempt.
                last_resp = Some(resp);
            }
            Err(e) => last_err = Some(e),
        }

        if attempt == max_retries {
            break;
        }

        // Rotation order: exhaust the key pool on the current exit FIRST
        // (cheap); only after a full key cycle switch exit — 429 is usually
        // IP-level, not key-level.
        if keys.len() <= 1 {
            advance_exit(&mut exit_idx, &exits);
        } else {
            key_idx += 1;
            if key_idx == keys.len() {
                key_idx = 0;
                advance_exit(&mut exit_idx, &exits);
            }
        }

        let backoff = (1u64 << attempt.min(3)).min(8);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
    }

    match last_resp {
        Some(r) => Ok(r),
        // Defensive: the loop body always records a response or a network
        // error on attempt 0, so at least one of last_resp/last_err is Some.
        None => Err(last_err.expect("resilient: no response and no error recorded")),
    }
}

/// POST variant of `get_resilient` (JSON body) — e.g. CORE.
pub async fn post_resilient_json<B>(
    url: &str,
    ua: &str,
    keys: &[String],
    placement: KeyPlacement,
    body: &B,
) -> Result<Response, reqwest::Error>
where
    B: serde::Serialize + ?Sized,
{
    let exits = exits();
    let max_retries = std::env::var("JABKIT_MAX_RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8)
        .clamp(0, 16);

    let mut key_idx = 0usize;
    let mut exit_idx = 0usize;
    let mut last_resp: Option<Response> = None;
    let mut last_err: Option<reqwest::Error> = None;

    for attempt in 0..=max_retries {
        let exit = &exits[exit_idx % exits.len()];
        let client = client_for(exit);
        let key = if keys.is_empty() {
            ""
        } else {
            keys[key_idx % keys.len()].as_str()
        };
        let mut req = client.post(url).header("User-Agent", ua).json(body);
        match placement {
            KeyPlacement::Header(h) if !key.is_empty() => req = req.header(h, key),
            KeyPlacement::Bearer if !key.is_empty() => {
                req = req.header("Authorization", format!("Bearer {key}"))
            }
            _ => {}
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if !is_retryable(status) {
                    // 401/403 = key invalid (key-level, not IP-level). With a
                    // multi-key pool, rotate to the next key and skip the dead
                    // one — don't return immediately, and don't burn the budget
                    // on exit switching either.
                    if (status.as_u16() == 401 || status.as_u16() == 403)
                        && keys.len() > 1
                    {
                        key_idx += 1;
                        last_resp = Some(resp);
                        continue;
                    }
                    return Ok(resp);
                }
                // Retryable status (429/5xx): keep the response to report on
                // budget exhaustion. Body left undrained — 429 bodies are
                // small; the cost is one non-pooled connection per attempt.
                last_resp = Some(resp);
            }
            Err(e) => last_err = Some(e),
        }

        if attempt == max_retries {
            break;
        }

        if keys.len() <= 1 {
            advance_exit(&mut exit_idx, &exits);
        } else {
            key_idx += 1;
            if key_idx == keys.len() {
                key_idx = 0;
                advance_exit(&mut exit_idx, &exits);
            }
        }

        let backoff = (1u64 << attempt.min(3)).min(8);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
    }

    match last_resp {
        Some(r) => Ok(r),
        // Defensive: the loop body always records a response or a network
        // error on attempt 0, so at least one of last_resp/last_err is Some.
        None => Err(last_err.expect("resilient: no response and no error recorded")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_exit_always_first() {
        let e = exits();
        assert_eq!(e[0].label, "direct");
        assert!(e[0].proxy.is_none());
    }

    #[test]
    fn tor_control_parsed_only_for_9050_socks() {
        assert_eq!(
            parse_tor_control("socks5h://100.65.157.17:9050"),
            Some("100.65.157.17:9051".into())
        );
        assert_eq!(parse_tor_control("socks5h://127.0.0.1:1080"), None);
        assert_eq!(parse_tor_control("http://127.0.0.1:8118"), None);
        assert_eq!(
            parse_tor_control("socks5://127.0.0.1:9050"),
            Some("127.0.0.1:9051".into())
        );
    }

    #[test]
    fn retryable_classification() {
        assert!(is_retryable(reqwest::StatusCode::from_u16(429).unwrap()));
        assert!(is_retryable(reqwest::StatusCode::INTERNAL_SERVER_ERROR));
        assert!(!is_retryable(reqwest::StatusCode::NOT_FOUND));
        assert!(!is_retryable(reqwest::StatusCode::UNAUTHORIZED));
    }
}
