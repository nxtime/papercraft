//! Token budget for read tools: every big read reports `estimatedTokens` and is cut at a clean
//! boundary (whole lines, or whole array elements of the JSON) when it passes `maxTokens`, so
//! one call cannot swamp the agent's context.

use super::Out;
use serde_json::{Value, json};

/// Budget when a call gives no `maxTokens`.
pub const DEFAULT_MAX_TOKENS: usize = 20_000;
const BYTES_PER_TOKEN: usize = 4;

pub fn tokens(bytes: usize) -> usize {
    bytes.div_ceil(BYTES_PER_TOKEN)
}

fn budget_bytes(max_tokens: Option<f64>) -> usize {
    max_tokens
        .filter(|t| *t >= 1.0)
        .map_or(DEFAULT_MAX_TOKENS, |t| t as usize)
        .saturating_mul(BYTES_PER_TOKEN)
}

/// A text result (tree, code, markdown). The first block is the text itself, unchanged when it
/// fits; the second is a JSON block with `estimatedTokens` and, once cut, `truncated`,
/// `nextOffset` (pass it as `offset` to read on) and `hint`. Cuts fall on line ends and at least
/// one line is always returned.
pub fn text_out(body: String, max_tokens: Option<f64>, offset: Option<f64>, hint: &str) -> Vec<Out> {
    let total = tokens(body.len());
    let skip = offset.filter(|o| *o >= 1.0).map_or(0, |o| o as usize);
    let budget = budget_bytes(max_tokens);
    if skip == 0 && body.len() <= budget {
        return vec![Out::Text(body), Out::Text(json!({"estimatedTokens": total}).to_string())];
    }
    let mut shown = String::new();
    let mut next = skip;
    let mut cut = false;
    for line in body.split_inclusive('\n').skip(skip) {
        if next > skip && shown.len() + line.len() > budget {
            cut = true;
            break;
        }
        shown.push_str(line);
        next += 1;
    }
    let mut meta = json!({"estimatedTokens": total});
    if cut {
        meta["truncated"] = json!(true);
        meta["nextOffset"] = json!(next);
        meta["hint"] = json!(format!("Pass offset={next} to read on, or {hint}."));
    }
    vec![Out::Text(shown), Out::Text(meta.to_string())]
}

/// A JSON result (pretty-printed, as before). `estimatedTokens` is added to the object; over
/// budget, the longest array loses its tail (whole elements, so the JSON stays valid) until it
/// fits, and `truncated` and `hint` are added. Arrays are not paged: the hint names the
/// narrower read to use.
pub fn json_out(mut v: Value, max_tokens: Option<f64>, hint: &str) -> Vec<Out> {
    let size = |v: &Value| serde_json::to_string_pretty(v).map_or(0, |s| s.len());
    let budget = budget_bytes(max_tokens);
    let total = size(&v);
    v["estimatedTokens"] = json!(tokens(total));
    if total > budget {
        v["truncated"] = json!(true);
        v["hint"] = json!(format!("Output was cut at {} tokens: {hint}.", budget / BYTES_PER_TOKEN));
        while size(&v) > budget {
            let Some(items) = longest_array(&mut v).filter(|a| !a.is_empty()) else { break };
            items.truncate(items.len() / 2);
        }
    }
    vec![Out::Text(serde_json::to_string_pretty(&v).unwrap_or_default())]
}

/// The first array on the heaviest path down the object, wherever it sits.
fn longest_array(v: &mut Value) -> Option<&mut Vec<Value>> {
    fn weight(v: &Value) -> usize {
        match v {
            Value::Array(a) if !a.is_empty() => v.to_string().len(),
            Value::Array(_) => 0,
            Value::Object(m) => m.values().map(weight).max().unwrap_or(0),
            _ => 0,
        }
    }
    match v {
        Value::Array(a) => Some(a),
        Value::Object(m) => {
            let key = m.iter().max_by_key(|(_, e)| weight(e)).filter(|(_, e)| weight(e) > 0)?.0.clone();
            longest_array(m.get_mut(&key)?)
        }
        _ => None,
    }
}
