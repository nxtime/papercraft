//! API keys for the model providers Pi runs on, kept in the OS secret store (Keychain, Credential
//! Manager, Secret Service) and handed to Pi only as environment variables of its process.

/// A provider Pi reads a key for from the environment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub env: &'static str,
    /// Where the user gets a free key.
    pub key_url: &'static str,
    /// Shown next to the key: what the free tier allows.
    pub note: &'static str,
}

pub const PROVIDERS: [Provider; 5] = [
    Provider { id: "openrouter", name: "OpenRouter", env: "OPENROUTER_API_KEY", key_url: "https://openrouter.ai/keys", note: ":free models, 50 requests a day (1000 after buying 10 credits)" },
    Provider { id: "groq", name: "Groq", env: "GROQ_API_KEY", key_url: "https://console.groq.com/keys", note: "Free tier with per-minute limits" },
    Provider { id: "gemini", name: "Gemini", env: "GEMINI_API_KEY", key_url: "https://aistudio.google.com/apikey", note: "Free tier on Flash models" },
    Provider { id: "nvidia", name: "NVIDIA NIM", env: "NVIDIA_API_KEY", key_url: "https://build.nvidia.com/settings/api-keys", note: "Free tier with per-minute limits" },
    Provider { id: "cerebras", name: "Cerebras", env: "CEREBRAS_API_KEY", key_url: "https://cloud.cerebras.ai", note: "Free tier with daily limits" },
];

/// `(env var, key)` for each provider `get` has a key for. Never log the result.
pub fn env(get: impl Fn(&Provider) -> Option<String>) -> Vec<(String, String)> {
    PROVIDERS.iter().filter_map(|p| get(p).map(|k| (p.env.to_owned(), k))).collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub use store::{forget, get, set};

#[cfg(not(target_arch = "wasm32"))]
mod store {
    use super::Provider;

    const SERVICE: &str = "PaperCraft";

    fn entry(p: &Provider) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, &format!("pi:{}", p.id)).map_err(|e| e.to_string())
    }

    pub fn get(p: &Provider) -> Option<String> {
        entry(p).ok()?.get_password().ok().map(|k| k.trim().to_owned()).filter(|k| !k.is_empty())
    }

    pub fn set(p: &Provider, key: &str) -> Result<(), String> {
        entry(p)?.set_password(key.trim()).map_err(|e| e.to_string())
    }

    pub fn forget(p: &Provider) -> Result<(), String> {
        match entry(p)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_providers_with_a_key_reach_the_environment() {
        let env = env(|p| (p.id == "openrouter").then(|| "sk-or-test".to_owned()));
        assert_eq!(env, [("OPENROUTER_API_KEY".to_owned(), "sk-or-test".to_owned())]);
    }
}
