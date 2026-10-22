//! API endpoints for the chat API backend: a name, a wire format, a base URL and a model. Keys
//! live in the OS keychain under the endpoint's base URL, never in settings files.

use serde::{Deserialize, Serialize};

pub const CLAUDE_HOST: &str = "api.anthropic.com";
pub const DEFAULT_ENDPOINT_ID: &str = "anthropic";
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiFormat {
    /// Messages API: `POST {base}/v1/messages`, `x-api-key`.
    #[default]
    Anthropic,
    /// Chat Completions: `POST {base}/chat/completions`, `Authorization: Bearer`.
    OpenAi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub format: ApiFormat,
    pub base_url: String,
    pub model: String,
}

/// The built-in `Anthropic` endpoint, so the first-run path stays one key away.
impl Default for Endpoint {
    fn default() -> Self {
        Self {
            id: DEFAULT_ENDPOINT_ID.into(),
            name: "Anthropic".into(),
            format: ApiFormat::Anthropic,
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
        }
    }
}

impl Endpoint {
    /// A blank endpoint for the settings "Add" button.
    pub fn blank() -> Self {
        Self { id: format!("ep-{:08x}", fastrand::u32(..)), name: "New endpoint".into(), format: ApiFormat::OpenAi, base_url: String::new(), model: String::new() }
    }

    /// Claude-only request fields (effort, thinking display, fallbacks, eager input streaming,
    /// cache breakpoints) are sent only to this host.
    pub fn is_claude_host(&self) -> bool {
        host(&self.base_url).as_deref() == Some(CLAUDE_HOST)
    }

    /// A chat can use it: URL and model are set.
    pub fn is_complete(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.model.trim().is_empty()
    }

    /// `anthropic.com` needs a key; local and third-party servers may not.
    pub fn needs_key(&self) -> bool {
        self.is_claude_host()
    }

    pub fn url(&self) -> String {
        let base = self.base_url.trim().trim_end_matches('/');
        match self.format {
            ApiFormat::Anthropic => format!("{base}/v1/messages"),
            ApiFormat::OpenAi => format!("{base}/chat/completions"),
        }
    }
}

/// Trimmed, no trailing slash, scheme and host lowercased (the path keeps its case).
pub fn normalize_url(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    let Some((scheme, rest)) = u.split_once("://") else { return u.to_lowercase() };
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    let sep = if path.is_empty() { "" } else { "/" };
    format!("{}://{}{sep}{path}", scheme.to_lowercase(), authority.to_lowercase())
}

/// Lowercase host without userinfo or port.
pub fn host(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let hostport = authority.rsplit('@').next()?;
    let host = hostport.split(':').next()?.to_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Keychain user name for an endpoint's key.
pub fn key_user(base_url: &str) -> String {
    format!("api-key:{}", normalize_url(base_url))
}

#[cfg(not(target_arch = "wasm32"))]
pub use keys::{delete_key, get_key, set_key};

#[cfg(not(target_arch = "wasm32"))]
mod keys {
    use super::*;

    const KEY_SERVICE: &str = "PaperCraft";
    /// The entry used before endpoints existed; still read for `api.anthropic.com`.
    const LEGACY_KEY_USER: &str = "anthropic-api-key";
    const KEY_ENV: &str = "ANTHROPIC_API_KEY";

    fn entry(user: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEY_SERVICE, user).map_err(|e| e.to_string())
    }

    fn read(user: &str) -> Option<String> {
        entry(user).ok()?.get_password().ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
    }

    fn remove(user: &str) -> Result<(), String> {
        match entry(user)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// The stored key, for `api.anthropic.com` also the legacy entry, then `ANTHROPIC_API_KEY`.
    /// Never log the result.
    pub fn get_key(ep: &Endpoint) -> Option<String> {
        let stored = read(&key_user(&ep.base_url));
        if !ep.is_claude_host() {
            return stored;
        }
        stored.or_else(|| read(LEGACY_KEY_USER)).or_else(|| std::env::var(KEY_ENV).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))
    }

    /// Saves under the endpoint's base URL; a legacy entry is dropped (the key moved).
    pub fn set_key(ep: &Endpoint, key: &str) -> Result<(), String> {
        entry(&key_user(&ep.base_url))?.set_password(key.trim()).map_err(|e| e.to_string())?;
        if ep.is_claude_host() {
            let _ = remove(LEGACY_KEY_USER);
        }
        Ok(())
    }

    pub fn delete_key(ep: &Endpoint) -> Result<(), String> {
        remove(&key_user(&ep.base_url))?;
        if ep.is_claude_host() {
            remove(LEGACY_KEY_USER)?;
        }
        Ok(())
    }
}

/// `(input, output)` USD per million tokens for models in the Claude price table; None for any
/// other model (the cost line then shows tokens only).
pub fn price(model: &str) -> Option<(f64, f64)> {
    if !model.starts_with("claude") {
        return None;
    }
    match model {
        m if m.contains("haiku") => Some((0.10, 0.50)),
        m if m.contains("sonnet") => Some((2.0, 10.0)),
        m if m.contains("opus") => Some((4.0, 20.0)),
        _ => None,
    }
}

const TOKENS_PER_MTOK: f64 = 1_000_000.0;
const CACHE_READ_FACTOR: f64 = 0.1;
const CACHE_WRITE_FACTOR: f64 = 1.25;

/// Estimated USD for `u` on `model` (cache reads at 10% of input, writes at 125%); None for a
/// model without a price.
pub fn cost(model: &str, u: &super::Usage) -> Option<f64> {
    let (pin, pout) = price(model)?;
    let input = u.input as f64 + u.cache_read as f64 * CACHE_READ_FACTOR + u.cache_write as f64 * CACHE_WRITE_FACTOR;
    Some((input * pin + u.output as f64 * pout) / TOKENS_PER_MTOK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_ignores_userinfo_port_and_lookalikes() {
        for (url, h) in [
            ("https://api.anthropic.com", Some("api.anthropic.com")),
            ("HTTPS://API.Anthropic.com/v1/", Some("api.anthropic.com")),
            ("https://api.anthropic.com:443/x", Some("api.anthropic.com")),
            ("https://api.anthropic.com@evil.example/x", Some("evil.example")),
            ("https://api.anthropic.com.evil.example", Some("api.anthropic.com.evil.example")),
            ("http://127.0.0.1:11434/v1", Some("127.0.0.1")),
            ("not a url", None),
        ] {
            assert_eq!(host(url).as_deref(), h, "{url}");
        }
        let claude = |u: &str| Endpoint { base_url: u.into(), ..Default::default() }.is_claude_host();
        assert!(claude("https://api.anthropic.com/") && !claude("https://api.anthropic.com.evil.example") && !claude("http://127.0.0.1:1"));
    }

    #[test]
    fn keyring_user_is_the_normalized_base_url() {
        assert_eq!(key_user("https://api.anthropic.com"), "api-key:https://api.anthropic.com");
        assert_eq!(key_user("  HTTPS://API.Anthropic.com/  "), "api-key:https://api.anthropic.com");
        assert_eq!(key_user("http://127.0.0.1:11434/v1/"), "api-key:http://127.0.0.1:11434/v1");
        assert_eq!(key_user("https://h.example/Path/X"), "api-key:https://h.example/Path/X");
    }

    #[test]
    fn urls_per_format() {
        let mut e = Endpoint { base_url: "https://h.example/anthropic/".into(), ..Default::default() };
        assert_eq!(e.url(), "https://h.example/anthropic/v1/messages");
        e.format = ApiFormat::OpenAi;
        e.base_url = "http://127.0.0.1:11434/v1".into();
        assert_eq!(e.url(), "http://127.0.0.1:11434/v1/chat/completions");
    }

    #[test]
    fn price_only_for_claude_models() {
        let u = crate::chat::Usage { input: 1_000_000, output: 1_000_000, cache_read: 1_000_000, cache_write: 0, cost_usd: None };
        assert!((cost("claude-opus-5-5", &u).unwrap() - 24.4).abs() < 1e-9);
        assert!((cost("claude-sonnet-5-5", &u).unwrap() - 12.2).abs() < 1e-9);
        assert!((cost("claude-haiku-5-5", &u).unwrap() - 0.61).abs() < 1e-9);
        assert_eq!(cost("MiniMax-M3.1-Flash", &u), None);
        assert_eq!(cost("llama3", &u), None);
    }
}
