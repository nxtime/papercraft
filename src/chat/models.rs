//! The models an agent can run: read from the agent's own CLI where it has one (`opencode models
//! --verbose` gives names, prices, context sizes and image input without starting a session), else
//! from the model option of its ACP sessions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One model an agent offers. Prices are US dollars per million tokens.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentModel {
    /// What the agent takes to select it, e.g. `opencode/big-pickle`.
    pub id: String,
    pub name: String,
    pub provider: String,
    pub cost_in: Option<f64>,
    pub cost_out: Option<f64>,
    pub context: Option<u64>,
    pub image: Option<bool>,
    /// Model-specific reasoning variants offered by the agent.
    pub variants: Vec<String>,
}

impl AgentModel {
    /// "Free", "$0.30 / $1.20", or "" when the price is unknown.
    pub fn price(&self) -> String {
        match (self.cost_in, self.cost_out) {
            (Some(i), Some(o)) if i == 0.0 && o == 0.0 => "Free".into(),
            (Some(i), Some(o)) => format!("${i:.2} / ${o:.2}"),
            _ => String::new(),
        }
    }

    /// Costs nothing to run: priced at 0, an OpenRouter `:free` variant, or served by a provider
    /// whose API keys come with a free tier.
    pub fn is_free(&self) -> bool {
        matches!((self.cost_in, self.cost_out), (Some(i), Some(o)) if i == 0.0 && o == 0.0)
            || self.id.ends_with(":free")
            || FREE_TIER_PROVIDERS.contains(&self.provider.as_str())
    }
}

/// The chat's model when it runs on whichever free model works ("Free (auto)").
pub const FREE_AUTO: &str = "free-auto";
/// Models a "Free (auto)" turn may move to after its first one fails.
pub const MAX_FALLBACKS: usize = 2;

/// Pi providers whose keys have a free tier. ponytail: per provider, because `pi --list-models`
/// gives no prices; a paid model there runs on the free tier's limits.
const FREE_TIER_PROVIDERS: [&str; 4] = ["groq", "cerebras", "nvidia", "google"];

/// The free models of `catalog` minus `skip`, the most capable first: larger context, then image
/// input. The order "Free (auto)" tries them in.
pub fn free_order(catalog: &[AgentModel], skip: &[String]) -> Vec<String> {
    let mut free: Vec<&AgentModel> = catalog.iter().filter(|m| m.is_free() && !skip.contains(&m.id)).collect();
    free.sort_by_key(|m| (std::cmp::Reverse(m.context.unwrap_or(0)), m.image != Some(true)));
    free.into_iter().map(|m| m.id.clone()).collect()
}

/// ACP config option values `(id, name)`; a name like "Ollama Cloud/GLM-5.3" carries its provider.
pub fn from_options(options: &[(String, String)]) -> Vec<AgentModel> {
    options
        .iter()
        .map(|(id, name)| {
            let (provider, name) = name.split_once('/').unwrap_or_else(|| (id.split_once('/').map_or("", |(p, _)| p), name));
            AgentModel { id: id.clone(), name: name.to_owned(), provider: provider.to_owned(), ..Default::default() }
        })
        .collect()
}

/// `fresh` becomes the list; what it does not say (price, context, a provider name) is kept from
/// the old entry with the same id. True when anything changed.
pub fn merge(list: &mut Vec<AgentModel>, fresh: Vec<AgentModel>) -> bool {
    let merged: Vec<AgentModel> = fresh
        .into_iter()
        .map(|mut m| {
            if let Some(old) = list.iter().find(|o| o.id == m.id) {
                if m.provider.is_empty() {
                    m.provider.clone_from(&old.provider);
                }
                m.cost_in = m.cost_in.or(old.cost_in);
                m.cost_out = m.cost_out.or(old.cost_out);
                m.context = m.context.or(old.context);
                m.image = m.image.or(old.image);
                if m.variants.is_empty() {
                    m.variants.clone_from(&old.variants);
                }
            }
            m
        })
        .collect();
    let changed = *list != merged;
    *list = merged;
    changed
}

/// `opencode models --verbose`: a `provider/model` line, then the model as pretty JSON, per model.
pub fn parse_opencode(out: &str) -> Vec<AgentModel> {
    let mut models = vec![];
    let mut lines = out.lines();
    while let Some(head) = lines.next() {
        let Some((provider, _)) = head.trim().split_once('/').filter(|_| !head.starts_with([' ', '{', '}'])) else { continue };
        let mut body = String::new();
        for l in lines.by_ref() {
            body.push_str(l);
            if l == "}" {
                break;
            }
        }
        let Ok(v) = serde_json::from_str::<Value>(&body) else { continue };
        let num = |p: &str| v.pointer(p).and_then(Value::as_f64);
        models.push(AgentModel {
            id: head.trim().to_owned(),
            name: v["name"].as_str().unwrap_or(head.trim()).to_owned(),
            provider: provider.to_owned(),
            cost_in: num("/cost/input"),
            cost_out: num("/cost/output"),
            context: v.pointer("/limit/context").and_then(Value::as_u64),
            image: v.pointer("/capabilities/input/image").and_then(Value::as_bool),
            variants: v["variants"].as_object().map(|v| v.iter().filter(|(_, value)| value["disabled"] != true).map(|(name, _)| name.clone()).collect()).unwrap_or_default(),
        });
    }
    models
}

/// `pi --list-models`: a header, then `provider model context max-out thinking images` columns,
/// sizes written like `131.1K` or `1M`.
pub fn parse_pi(out: &str) -> Vec<AgentModel> {
    let size = |s: &str| -> Option<u64> {
        let (n, mul) = match s.as_bytes().last()? {
            b'K' => (&s[..s.len() - 1], 1e3),
            b'M' => (&s[..s.len() - 1], 1e6),
            _ => (s, 1.0),
        };
        n.parse::<f64>().ok().map(|v| (v * mul).round() as u64)
    };
    out.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            let [provider, model, context, _, _, images] = c[..] else { return None };
            if provider == "provider" {
                return None;
            }
            Some(AgentModel {
                id: format!("{provider}/{model}"),
                name: model.to_owned(),
                provider: provider.to_owned(),
                context: size(context),
                image: Some(images == "yes"),
                ..Default::default()
            })
        })
        .collect()
}

/// Runs `pi --list-models --offline` with `env` (the lean Pi folder and the provider keys).
#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_pi(program: &std::path::Path, env: &[(String, String)]) -> Option<Vec<AgentModel>> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(["--list-models", "--offline"]).envs(env.iter().cloned()).stdin(std::process::Stdio::null());
    super::path::apply(&mut cmd);
    let out = cmd.output().map_err(|e| log::warn!("models: could not run {}: {e}", program.display())).ok()?;
    let models = parse_pi(&String::from_utf8_lossy(&out.stdout));
    (!models.is_empty()).then_some(models)
}

/// Runs `program models --verbose` with `env` (the lean environment, so the list matches what a
/// lean session offers). None when it fails or lists nothing.
#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_opencode(program: &std::path::Path, env: &[(String, String)]) -> Option<Vec<AgentModel>> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(["models", "--verbose"]).envs(env.iter().cloned()).stdin(std::process::Stdio::null());
    super::path::apply(&mut cmd);
    let out = cmd.output().map_err(|e| log::warn!("models: could not run {}: {e}", program.display())).ok()?;
    let models = parse_opencode(&String::from_utf8_lossy(&out.stdout));
    (!models.is_empty()).then_some(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: &str = "opencode/big-pickle\n{\n  \"id\": \"big-pickle\",\n  \"name\": \"Big Pickle\",\n  \"cost\": {\n    \"input\": 0,\n    \"output\": 0\n  },\n  \"limit\": {\n    \"context\": 200000\n  },\n  \"capabilities\": {\n    \"input\": {\n      \"image\": false\n    }\n  }\n}\nminimax/MiniMax-M3\n{\n  \"name\": \"MiniMax-M3\",\n  \"cost\": {\n    \"input\": 0.3,\n    \"output\": 1.2\n  }\n}\n";

    #[test]
    fn opencode_listing_gives_names_prices_and_limits() {
        let m = parse_opencode(OUT);
        assert_eq!(m.iter().map(|m| (m.id.as_str(), m.provider.as_str(), m.price())).collect::<Vec<_>>(), [("opencode/big-pickle", "opencode", "Free".to_owned()), ("minimax/MiniMax-M3", "minimax", "$0.30 / $1.20".to_owned())]);
        assert_eq!((m[0].context, m[0].image, m[1].context), (Some(200_000), Some(false), None));
    }

    #[test]
    fn opencode_listing_keeps_model_variants() {
        let models = parse_opencode("provider/model\n{\n\"name\":\"Model\",\n\"variants\":{\"high\":{},\"low\":{}}\n}\n");
        assert_eq!(models[0].variants, ["high", "low"]);
    }

    #[test]
    fn pi_listing_gives_ids_context_and_images() {
        let out = "provider    model                         context  max-out  thinking  images\nopenrouter  qwen/qwen3-coder:free         262.1K   32.8K    no        no\ngroq        llama-4-scout                 1M       8.2K     yes       yes\nbroken line\n";
        let m = parse_pi(out);
        assert_eq!(m.iter().map(|m| (m.id.as_str(), m.context, m.image)).collect::<Vec<_>>(), [("openrouter/qwen/qwen3-coder:free", Some(262_100), Some(false)), ("groq/llama-4-scout", Some(1_000_000), Some(true))]);
    }

    #[test]
    fn free_models_go_first_by_context_and_skip_the_failed() {
        let mut list = parse_opencode(OUT);
        list.extend(parse_pi("openrouter  a:free  1M  1K  no  no\nopenrouter  b  1M  1K  no  no\ngroq  c  100K  1K  no  yes\n"));
        assert_eq!(free_order(&list, &[]), ["openrouter/a:free", "opencode/big-pickle", "groq/c"]);
        assert_eq!(free_order(&list, &["openrouter/a:free".into()]), ["opencode/big-pickle", "groq/c"]);
    }

    #[test]
    fn session_options_keep_what_the_listing_knew() {
        let mut list = parse_opencode(OUT);
        let fresh = from_options(&[("opencode/big-pickle".into(), "OpenCode Zen/Big Pickle".into()), ("x/y".into(), "Y".into())]);
        assert!(merge(&mut list, fresh));
        assert_eq!((list[0].provider.as_str(), list[0].name.as_str(), list[0].price(), list[0].context), ("OpenCode Zen", "Big Pickle", "Free".to_owned(), Some(200_000)));
        assert_eq!((list[1].provider.as_str(), list.len()), ("x", 2), "a model the session no longer offers is dropped");
        assert!(!merge(&mut list.clone(), list.clone()));
    }
}
