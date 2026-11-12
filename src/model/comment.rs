//! Threaded comments on layers: a thread is a [`Comment`] on a node with [`Reply`]s under it.
//!
//! A thread follows its layer. With a `point` it sits at that offset inside the layer (relative
//! to the layer's top-left, so it follows moves and resizes), else on the layer's corner.
//! `@name` mentions are parsed once, stored as names, and rendered highlighted from the text.

use super::node::Node;
use crate::geom::V2;
use serde::{Deserialize, Serialize};

/// A note attached to a layer: what to change, from the user to an agent or back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    /// Unique within the layer.
    pub id: u32,
    pub text: String,
    /// "user" or "agent".
    pub author: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolved: bool,
    /// What was done about it, set when resolved by an older build; newer ones add a [`Reply`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// Unix milliseconds; 0 for a note made before threads.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created: u64,
    /// Offset inside the layer where the pin sits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<V2>,
    /// Names `@mentioned` in `text`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<Reply>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    /// Unique within the thread.
    pub id: u32,
    pub author: String,
    pub text: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<String>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Text cut into runs; `true` marks an `@name` mention (the `@` included).
pub fn spans(text: &str) -> Vec<(&str, bool)> {
    let is_name = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-');
    let mut out = Vec::new();
    let (mut plain, mut prev) = (0, None::<char>);
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let starts = c == '@' && prev.is_none_or(|p| !is_name(p)) && it.peek().is_some_and(|(_, n)| is_name(*n));
        if starts {
            let mut end = i + 1;
            while let Some(&(j, n)) = it.peek() {
                if !is_name(n) {
                    break;
                }
                end = j + n.len_utf8();
                it.next();
            }
            if plain < i {
                out.push((&text[plain..i], false));
            }
            out.push((&text[i..end], true));
            plain = end;
        }
        prev = Some(c);
    }
    if plain < text.len() {
        out.push((&text[plain..], false));
    }
    out
}

/// Distinct mentioned names in order, without the `@`.
pub fn mentions(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (s, _) in spans(text).into_iter().filter(|(_, m)| *m) {
        let name = s[1..].to_string();
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    out
}

impl Node {
    pub fn open_comments(&self) -> usize {
        self.comments.iter().filter(|c| !c.resolved).count()
    }
    pub fn resolved_comments(&self) -> usize {
        self.comments.iter().filter(|c| c.resolved).count()
    }

    /// Start a thread on this layer and return its id.
    pub fn post_comment(&mut self, author: &str, text: &str, created: u64, point: Option<V2>) -> u32 {
        let id = self.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        self.comments.push(Comment {
            id,
            text: text.into(),
            author: author.into(),
            resolved: false,
            reply: None,
            created,
            point,
            mentions: mentions(text),
            replies: Vec::new(),
        });
        id
    }

    /// Add a reply to thread `thread`; None when there is no such thread.
    pub fn reply_to(&mut self, thread: u32, author: &str, text: &str, created: u64) -> Option<u32> {
        let c = self.comments.iter_mut().find(|c| c.id == thread)?;
        let id = c.replies.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        c.replies.push(Reply { id, author: author.into(), text: text.into(), created, mentions: mentions(text) });
        Some(id)
    }

    /// Resolve or reopen thread `thread`; false when there is no such thread.
    pub fn set_thread_resolved(&mut self, thread: u32, resolved: bool) -> bool {
        match self.comments.iter_mut().find(|c| c.id == thread) {
            Some(c) => {
                c.resolved = resolved;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{NodeId, NodeKind};

    fn node() -> Node {
        Node::new(NodeId(1), NodeKind::Frame { clip: false }, Rect::new(0.0, 0.0, 10.0, 10.0))
    }

    #[test]
    fn mentions_are_names_after_a_word_boundary() {
        assert_eq!(mentions("ask @ana and @Bob-2, @ana again; mail a@b.com @"), vec!["ana", "Bob-2"]);
        assert_eq!(spans("hi @ana!"), vec![("hi ", false), ("@ana", true), ("!", false)]);
    }

    #[test]
    fn replies_and_resolve_live_in_the_thread() {
        let mut n = node();
        let t = n.post_comment("user", "fix @agent", 5, Some(V2 { x: 3.0, y: 4.0 }));
        assert_eq!(n.reply_to(t, "agent", "done @user", 6), Some(1));
        assert_eq!(n.reply_to(9, "agent", "x", 6), None);
        assert!(n.set_thread_resolved(t, true));
        assert_eq!(n.open_comments(), 0);
        let c = &n.comments[0];
        assert_eq!((c.mentions.clone(), c.replies[0].mentions.clone()), (vec!["agent".to_string()], vec!["user".to_string()]));
    }

    #[test]
    fn an_old_note_loads_and_a_plain_one_saves_without_thread_fields() {
        let c: Comment = serde_json::from_str(r#"{"id":1,"text":"t","author":"user"}"#).unwrap();
        assert_eq!((c.created, c.replies.len(), c.point), (0, 0, None));
        assert_eq!(serde_json::to_string(&c).unwrap(), r#"{"id":1,"text":"t","author":"user"}"#);
    }
}
