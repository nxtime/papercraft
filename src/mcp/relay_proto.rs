//! Wire protocol between the relay and a PaperCraft tab running in a browser.
//!
//! JSON text frames, one tool call per exchange: the relay sends a [`Call`], the tab answers
//! with a [`Reply`] carrying the same `id`. Compiled for native and wasm alike: the tab side
//! speaks it too.

use crate::mcp::{Out, ToolResult};
use serde::{Deserialize, Serialize};

/// Shortest link token a tab may generate.
pub const MIN_TOKEN_LEN: usize = 32;
/// Longest link token a tab may generate.
pub const MAX_TOKEN_LEN: usize = 64;

/// relay -> tab: run `name` with `args`, answer frame `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub id: u64,
    pub name: String,
    pub args: serde_json::Value,
}

/// One output block, tagged by `type` so the tab can switch on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OutFrame {
    Text {
        text: String,
    },
    /// `data` is base64 (standard alphabet, padded).
    Image {
        mime: String,
        data: String,
    },
}

/// tab -> relay: the answer to [`Call::id`]. `out` and `error` are mutually exclusive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out: Option<Vec<OutFrame>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Reply {
    pub fn ok(id: u64, out: Vec<Out>) -> Reply {
        Reply {
            id,
            ok: true,
            out: Some(out.into_iter().map(OutFrame::from).collect()),
            error: None,
        }
    }

    pub fn err(id: u64, error: impl Into<String>) -> Reply {
        Reply {
            id,
            ok: false,
            out: None,
            error: Some(error.into()),
        }
    }

    /// The tool result this reply carries; a malformed image block is an error, not a panic.
    pub fn into_result(self) -> ToolResult {
        if !self.ok {
            return Err(self
                .error
                .unwrap_or_else(|| "the PaperCraft tab reported an error".into()));
        }
        let blocks = match self.out {
            Some(b) => b,
            None => return Err("the PaperCraft tab sent an empty reply".into()),
        };
        let mut out = Vec::with_capacity(blocks.len());
        for b in blocks {
            match b.into() {
                Some(o) => out.push(o),
                None => return Err("the PaperCraft tab sent an image that is not base64".into()),
            }
        }
        Ok(out)
    }
}

impl From<Out> for OutFrame {
    fn from(o: Out) -> OutFrame {
        match o {
            Out::Text(text) => OutFrame::Text { text },
            Out::Image { data, mime } => OutFrame::Image {
                mime,
                data: crate::assets::base64(&data),
            },
        }
    }
}

impl From<OutFrame> for Option<Out> {
    fn from(f: OutFrame) -> Option<Out> {
        match f {
            OutFrame::Text { text } => Some(Out::Text(text)),
            OutFrame::Image { mime, data } => {
                crate::assets::base64_decode(&data).map(|d| Out::Image { data: d, mime })
            }
        }
    }
}

/// A link token is a bearer secret the tab generates: 32..=64 lowercase hex chars, nothing else.
pub fn valid_token(token: &str) -> bool {
    (MIN_TOKEN_LEN..=MAX_TOKEN_LEN).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Bytes in a freshly generated link token (hex-encoded: [`MIN_TOKEN_LEN`] characters).
pub const TOKEN_BYTES: usize = MIN_TOKEN_LEN / 2;

/// Lowercase hex of `bytes`, the token alphabet.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where the tab's WebSocket connects: `base` is a `ws(s)://host` origin.
pub fn tab_url(base: &str, token: &str) -> String {
    format!("{}/tab/{token}", base.trim_end_matches('/'))
}

/// The URL an agent is given: the relay's HTTP origin plus the token's MCP endpoint.
pub fn agent_url(origin: &str, token: &str) -> String {
    format!("{}/s/{token}/mcp", origin.trim_end_matches('/'))
}

/// The relay's WebSocket base: `?relay=` in the page URL, else `window.PAPERCRAFT_RELAY`, else the
/// page's own origin. `search` is `location.search`; `page_origin` is `location.origin`.
pub fn relay_base(search: &str, global: Option<&str>, page_origin: &str) -> String {
    let from_query = search
        .trim_start_matches('?')
        .split('&')
        .find_map(|kv| kv.strip_prefix("relay="))
        .filter(|v| !v.is_empty());
    match from_query.or(global.filter(|g| !g.is_empty())) {
        Some(url) => url.to_string(),
        None => page_origin
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1),
    }
}

/// The HTTP origin matching a relay WebSocket base (what the agent connects to).
pub fn http_origin(ws_base: &str) -> String {
    ws_base
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn png() -> Vec<u8> {
        // 1x1 transparent PNG, encoded in the test so the payload stays tiny.
        crate::assets::base64_decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .expect("valid base64 fixture")
    }

    #[test]
    fn success_reply_round_trips_through_json() {
        let reply = Reply::ok(
            7,
            vec![
                Out::Text("{\"nodes\":1}".into()),
                Out::Image {
                    data: png(),
                    mime: "image/png".into(),
                },
            ],
        );
        let json = serde_json::to_string(&reply).unwrap();
        assert_eq!(
            json,
            format!(
                r#"{{"id":7,"ok":true,"out":[{{"type":"text","text":"{{\"nodes\":1}}"}},{{"type":"image","mime":"image/png","data":"{}"}}]}}"#,
                crate::assets::base64(&png())
            )
        );
        let back: Reply = serde_json::from_str(&json).unwrap();
        assert_eq!(back, reply);
        let outs = back.into_result().unwrap();
        assert_eq!(outs[0], Out::Text("{\"nodes\":1}".into()));
        assert_eq!(
            outs[1],
            Out::Image {
                data: png(),
                mime: "image/png".into()
            }
        );
    }

    #[test]
    fn failure_reply_is_exactly_id_ok_error() {
        let reply = Reply::err(42, "no artboard");
        let json = serde_json::to_string(&reply).unwrap();
        assert_eq!(json, r#"{"id":42,"ok":false,"error":"no artboard"}"#);
        assert_eq!(serde_json::from_str::<Reply>(&json).unwrap(), reply);
        assert_eq!(reply.into_result().unwrap_err(), "no artboard");
    }

    #[test]
    fn a_reply_that_is_not_a_valid_image_is_an_error() {
        let bad: Reply = serde_json::from_value(json!({
            "id": 1, "ok": true,
            "out": [{"type": "image", "mime": "image/png", "data": "not base64!"}]
        }))
        .unwrap();
        assert!(bad.into_result().is_err());
    }

    #[test]
    fn call_round_trips_through_json() {
        let call = Call {
            id: 7,
            name: "get_screenshot".into(),
            args: json!({"nodeId": "n1:3"}),
        };
        let json = serde_json::to_string(&call).unwrap();
        assert_eq!(
            json,
            r#"{"id":7,"name":"get_screenshot","args":{"nodeId":"n1:3"}}"#
        );
        assert_eq!(serde_json::from_str::<Call>(&json).unwrap(), call);
    }

    #[test]
    fn tokens_are_lowercase_hex_of_the_documented_length() {
        let n32 = "a".repeat(MIN_TOKEN_LEN);
        let n64 = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";
        assert_eq!(n64.len(), MAX_TOKEN_LEN);
        assert!(valid_token(&n32));
        assert!(valid_token(&n64));
        assert!(valid_token(&"0123456789abcdef".repeat(2)));
        assert!(!valid_token(""));
        assert!(!valid_token(&"a".repeat(MIN_TOKEN_LEN - 1)));
        assert!(!valid_token(&"a".repeat(MAX_TOKEN_LEN + 1)));
        assert!(!valid_token(&"A".repeat(MIN_TOKEN_LEN)), "uppercase");
        assert!(!valid_token(&"g".repeat(MIN_TOKEN_LEN)), "non-hex");
        assert!(!valid_token(&format!(
            "{}-{}",
            "a".repeat(MIN_TOKEN_LEN - 1),
            "b"
        )));
    }

    #[test]
    fn urls_and_hex() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
        assert!(valid_token(&hex(&[0xab; TOKEN_BYTES])));
        assert_eq!(tab_url("wss://h.example/", "t"), "wss://h.example/tab/t");
        assert_eq!(
            agent_url("https://h.example", "t"),
            "https://h.example/s/t/mcp"
        );
    }

    #[test]
    fn relay_base_prefers_query_then_global_then_origin() {
        let o = "https://app.example";
        assert_eq!(
            relay_base("?relay=wss://r.example&x=1", Some("wss://g"), o),
            "wss://r.example"
        );
        assert_eq!(
            relay_base("?x=1", Some("wss://g.example"), o),
            "wss://g.example"
        );
        assert_eq!(relay_base("", None, o), "wss://app.example");
        assert_eq!(
            relay_base("", Some(""), "http://localhost:8787"),
            "ws://localhost:8787"
        );
        assert_eq!(http_origin("wss://r.example"), "https://r.example");
        assert_eq!(http_origin("ws://localhost:1"), "http://localhost:1");
    }
}
