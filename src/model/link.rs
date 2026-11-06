//! What a layer does when clicked in Present mode besides jumping to a frame (`Node::link`).

use super::NodeId;
use serde::{Deserialize, Serialize};

/// Longest URL a link may hold; browsers and mail clients cope with far less than this anyway.
const MAX_URL_LEN: usize = 2048;

/// A click action other than "go to frame". Each maps to plain HTML: `Url` to `<a href>`, `Back`
/// to `history.back()`, `Overlay` and `Close` to opening and closing a `<dialog>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LinkAction {
    /// Open an external address (`https`, `http`, `mailto` or `tel`; see [`valid_url`]).
    Url { url: String },
    /// Return to the frame shown before this one.
    Back,
    /// Open `target` as an overlay centred over the current frame.
    Overlay { target: NodeId },
    /// Close the open overlay.
    Close,
}

/// Checks a link address and returns it trimmed. Only `https`, `http`, `mailto` and `tel` pass:
/// anything else (`file:`, `javascript:`, custom schemes) would let a shared design run
/// something on the viewer's machine.
pub fn valid_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() || url.len() > MAX_URL_LEN {
        return Err(format!("a link URL must be 1 to {MAX_URL_LEN} characters"));
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("a link URL cannot contain spaces or control characters".into());
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        return Err(format!("`{url}` has no scheme: use https, http, mailto or tel"));
    };
    let scheme = scheme.to_ascii_lowercase();
    let ok = match scheme.as_str() {
        "https" | "http" => rest.strip_prefix("//").is_some_and(|r| !r.is_empty() && !r.starts_with(['/', '?', '#'])),
        "mailto" | "tel" => !rest.is_empty(),
        _ => return Err(format!("`{scheme}:` links are not allowed: use https, http, mailto or tel")),
    };
    if ok { Ok(url.to_string()) } else { Err(format!("`{url}` is not a complete {scheme} address")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_four_schemes_with_a_target_pass() {
        for ok in ["https://a.dev/x?y=1#z", "HTTP://a.dev", " mailto:a@b.dev ", "tel:+15551234"] {
            assert!(valid_url(ok).is_ok(), "{ok}");
        }
        for bad in ["", "a.dev", "javascript:alert(1)", "file:///etc/passwd", "ftp://a", "https://", "https:///x", "mailto:", "https://a b", "tel:1\n2"] {
            assert!(valid_url(bad).is_err(), "{bad}");
        }
        assert!(valid_url(&format!("https://a/{}", "x".repeat(MAX_URL_LEN))).is_err());
    }

    #[test]
    fn actions_round_trip_as_tagged_json() {
        for a in [LinkAction::Back, LinkAction::Close, LinkAction::Overlay { target: NodeId(7) }, LinkAction::Url { url: "tel:1".into() }] {
            let s = serde_json::to_string(&a).unwrap();
            assert_eq!(serde_json::from_str::<LinkAction>(&s).unwrap(), a);
        }
        assert_eq!(serde_json::to_string(&LinkAction::Back).unwrap(), r#"{"kind":"back"}"#);
    }
}
