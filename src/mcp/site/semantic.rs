//! Which HTML tag a layer deserves: landmarks and buttons from layer names, headings and
//! paragraphs from the size and place of the text. Only tags change; the layout never does.

use crate::model::{Document, LinkAction, Node, NodeId, NodeKind};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Text starting above this many px from the top of the page is in the first screen.
const FOLD_PX: f64 = 900.0;
/// Longer copy is a paragraph, not a heading, whatever its size.
const HEADING_MAX_CHARS: usize = 120;
const HEADING_MIN_PX: f64 = 20.0;
/// A heading is at least this much larger than the body text.
const HEADING_RATIO: f64 = 1.2;
const PARAGRAPH_CHARS: usize = 40;
const WRAPPED_PARAGRAPH_CHARS: usize = 25;

#[derive(Default)]
pub struct Sem {
    /// Heading level per text layer.
    pub heads: HashMap<NodeId, u8>,
    /// Short lines straight under a larger heading that only restate it: paragraphs.
    pub subtitles: HashSet<NodeId>,
    /// The layer written as `<main>`: a layer named so, else the root or its tallest content child.
    pub main: Option<NodeId>,
}

fn words(name: &str) -> impl Iterator<Item = String> + '_ {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// The landmark or button a frame or group's name asks for.
pub fn landmark(name: &str) -> Option<&'static str> {
    words(name).find_map(|w| match w.as_str() {
        "header" => Some("header"),
        "nav" | "navbar" | "navigation" => Some("nav"),
        "footer" => Some("footer"),
        "main" => Some("main"),
        "section" => Some("section"),
        "aside" | "sidebar" => Some("aside"),
        "button" | "btn" | "cta" => Some("button"),
        _ => None,
    })
}

/// `h1`..`h6` or `p` when a text layer is named so.
pub fn text_name(name: &str) -> Option<&'static str> {
    words(name).find_map(|w| match w.as_str() {
        "h1" => Some("h1"),
        "h2" => Some("h2"),
        "h3" => Some("h3"),
        "h4" => Some("h4"),
        "h5" => Some("h5"),
        "h6" => Some("h6"),
        "p" | "paragraph" => Some("p"),
        _ => None,
    })
}

pub fn is_container(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Frame { .. } | NodeKind::Group)
}

/// The landmark or button of a frame or group: the author's role when set, else its name's.
pub fn landmark_of(n: &Node) -> Option<&'static str> {
    match n.a11y.as_ref().and_then(|a| a.role.as_deref()) {
        Some(r @ ("button" | "nav" | "main" | "header" | "footer" | "aside" | "section")) => {
            landmark(r)
        }
        _ => landmark(&n.name),
    }
}

pub fn is_button(n: &Node) -> bool {
    is_container(n) && landmark_of(n) == Some("button")
}

pub fn has_link(n: &Node) -> bool {
    n.link.is_some() || matches!(n.link_action, Some(LinkAction::Url { .. }))
}

/// Whether a layer below `root` sits inside a button or link (headings there stay inline).
fn in_action(doc: &Document, root: NodeId, id: NodeId) -> bool {
    let mut cur = doc.get(id).and_then(|n| n.parent);
    while let Some(n) = cur.and_then(|p| doc.get(p)) {
        if is_button(n) || has_link(n) {
            return true;
        }
        cur = if n.id == root { None } else { n.parent };
    }
    false
}

/// A short amount or number ("$29", "R$ 49/mês"): a figure, never a heading.
fn price_like(text: &str) -> bool {
    let has_digit = text.chars().any(|c| c.is_ascii_digit());
    let money = text.chars().any(|c| matches!(c, '$' | '€' | '£' | '¥'));
    text.chars().count() <= 20 && has_digit && (money || !text.chars().any(char::is_alphabetic))
}

/// Whether a layer below `root` sits in a header or nav: its text is the brand, not a heading.
fn in_chrome(doc: &Document, root: NodeId, id: NodeId) -> bool {
    let mut cur = doc.get(id).and_then(|n| n.parent);
    while let Some(n) = cur.and_then(|p| doc.get(p)) {
        if is_container(n) && matches!(landmark_of(n), Some("header" | "nav")) {
            return true;
        }
        cur = if n.id == root { None } else { n.parent };
    }
    false
}

/// The tag of text that is not a heading: a `p` when it reads as a sentence, else a `span`.
pub fn text_tag(chars: usize, wraps: bool) -> &'static str {
    if chars > PARAGRAPH_CHARS || (wraps && chars > WRAPPED_PARAGRAPH_CHARS) {
        "p"
    } else {
        "span"
    }
}

pub fn semantics(doc: &Document, root: NodeId) -> Sem {
    let layers = doc.subtree(root);
    let top = doc.world_rect(root).map_or(0.0, |r| r.y);
    let mut sem = Sem {
        main: main_of(doc, root, &layers),
        ..Sem::default()
    };

    struct T {
        id: NodeId,
        size: f64,
        bold: bool,
        y: f64,
        short: bool,
        brand: bool,
    }
    let mut candidates = Vec::new();
    let mut by_size: BTreeMap<i64, usize> = BTreeMap::new();
    let mut explicit_h1 = None;
    // Every text layer in reading order, with its size and parent.
    let mut order: Vec<(NodeId, f64, Option<NodeId>)> = Vec::new();
    for id in &layers {
        let Some(n) = doc.get(*id).filter(|n| n.visible) else {
            continue;
        };
        let NodeKind::Text {
            text, font_size, ..
        } = &n.kind
        else {
            continue;
        };
        let chars = text.chars().count();
        if chars == 0 {
            continue;
        }
        order.push((*id, *font_size, n.parent));
        *by_size
            .entry((font_size * 100.0).round() as i64)
            .or_default() += chars;
        let level = text_name(&n.name)
            .and_then(|t| t.strip_prefix('h'))
            .and_then(|l| l.parse::<u8>().ok());
        if let Some(l) = level {
            let l = if l == 1 && explicit_h1.replace(*id).is_some() {
                2
            } else {
                l
            };
            sem.heads.insert(*id, l);
        } else if words(&n.name).any(|w| w == "heading") && explicit_h1.is_none() {
            explicit_h1 = Some(*id);
            sem.heads.insert(*id, 1);
        } else if text_name(&n.name).is_none()
            && !in_action(doc, root, *id)
            && !price_like(text)
        {
            let y = doc.world_rect(*id).map_or(0.0, |r| r.y) - top;
            candidates.push(T {
                id: *id,
                size: *font_size,
                bold: n.text_style.bold,
                y,
                short: chars <= HEADING_MAX_CHARS,
                brand: in_chrome(doc, root, *id),
            });
        }
    }
    // Every page has one h1: the most prominent text, whatever its size. Size only decides
    // between the lower headings and paragraphs below.
    if explicit_h1.is_none() {
        let prominence = |a: &&T, b: &&T| {
            a.size
                .total_cmp(&b.size)
                .then(a.bold.cmp(&b.bold))
                .then(b.y.total_cmp(&a.y))
        };
        let pick = |pool: Vec<&T>| pool.into_iter().max_by(prominence).map(|t| t.id);
        let first_screen = |t: &&T| t.short && t.y < FOLD_PX;
        let named: Vec<&T> = candidates.iter().filter(|t| !t.brand).collect();
        let h1 = pick(named.iter().copied().filter(first_screen).collect())
            .or_else(|| pick(named.iter().copied().filter(|t| t.short).collect()))
            .or_else(|| pick(named));
        if let Some(h1) = h1 {
            sem.heads.insert(h1, 1);
            candidates.retain(|t| t.id != h1);
        }
    }
    // The size most characters use is the body; a heading must clearly stand out of it.
    let body = by_size
        .iter()
        .max_by_key(|(size, chars)| (**chars, std::cmp::Reverse(**size)))
        .map_or(16.0, |(s, _)| *s as f64 / 100.0);
    candidates
        .retain(|t| !t.brand && t.short && t.size >= HEADING_MIN_PX.max(body * HEADING_RATIO));
    let mut sizes: Vec<i64> = candidates
        .iter()
        .map(|t| (t.size * 100.0).round() as i64)
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes.dedup();
    for t in &candidates {
        let rank = sizes
            .iter()
            .position(|s| *s == (t.size * 100.0).round() as i64)
            .unwrap_or(0);
        sem.heads.insert(t.id, (2 + rank).min(6) as u8);
    }
    // A line right under a larger heading, in the same parent, is its subtitle.
    let guessed: HashSet<NodeId> = candidates.iter().map(|t| t.id).collect();
    for w in order.windows(2) {
        let ((prev, prev_size, prev_parent), (id, size, parent)) = (w[0], w[1]);
        if guessed.contains(&id)
            && sem.heads.contains_key(&prev)
            && prev_size > size
            && prev_parent == parent
        {
            sem.heads.remove(&id);
            sem.subtitles.insert(id);
        }
    }
    // The outline never skips a level downward: h1 then h3 becomes h1 then h2. Guessed headings
    // of one size share one level, so identical siblings do not drift down one by one.
    let sizes_of: HashMap<NodeId, i64> = order
        .iter()
        .map(|(id, size, _)| (*id, (size * 100.0).round() as i64))
        .collect();
    let mut settled: HashMap<i64, u8> = HashMap::new();
    let mut above = None;
    for (id, ..) in &order {
        let Some(level) = sem.heads.get_mut(id) else {
            continue;
        };
        let cap = above.map_or(6, |a: u8| a + 1);
        if guessed.contains(id) {
            let size = sizes_of[id];
            let settled_level = *settled.entry(size).or_insert((*level).min(cap));
            *level = settled_level.min(cap);
        } else {
            *level = (*level).min(cap);
        }
        above = Some(*level);
    }
    explicit(doc, &layers, &mut sem);
    sem
}

/// The author's role and heading level win over every guess above.
fn explicit(doc: &Document, layers: &[NodeId], sem: &mut Sem) {
    for id in layers {
        let Some(a) = doc.get(*id).and_then(|n| n.a11y.as_ref()) else {
            continue;
        };
        match a.role.as_deref() {
            Some("heading") => {
                sem.subtitles.remove(id);
                sem.heads.insert(*id, a.level.unwrap_or(2).clamp(1, 6));
            }
            Some(_) => {
                sem.subtitles.remove(id);
                sem.heads.remove(id);
            }
            None => {}
        }
    }
}

/// A layer named `main`, else the root when no header or footer is split off, else the tallest
/// content child.
fn main_of(doc: &Document, root: NodeId, layers: &[NodeId]) -> Option<NodeId> {
    let named = |id: &&NodeId| {
        doc.get(**id)
            .is_some_and(|n| is_container(n) && landmark_of(n) == Some("main"))
    };
    if let Some(id) = layers.iter().skip(1).find(named) {
        return Some(*id);
    }
    let kids: Vec<&Node> = doc
        .get(root)?
        .children
        .iter()
        .filter_map(|c| doc.get(*c))
        .filter(|n| is_container(n))
        .collect();
    let chrome = |n: &&Node| {
        matches!(
            landmark_of(n),
            Some("header" | "footer" | "nav" | "aside")
        )
    };
    if !kids
        .iter()
        .any(|n| matches!(landmark_of(n), Some("header" | "footer")))
    {
        return Some(root);
    }
    kids.iter()
        .filter(|n| !chrome(n))
        .max_by(|a, b| a.rect.h.total_cmp(&b.rect.h))
        .map(|n| n.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_pick_landmarks_and_text_names_pick_headings() {
        assert_eq!(landmark("Site Header"), Some("header"));
        assert_eq!(landmark("Footer / links"), Some("footer"));
        assert_eq!(landmark("Get started CTA"), Some("button"));
        assert_eq!(landmark("Card"), None);
        assert_eq!(landmark("Headers"), None);
        assert_eq!(text_name("H2 title"), Some("h2"));
        assert_eq!(text_name("Paragraph"), Some("p"));
        assert_eq!(text_name("Hero"), None);
        assert_eq!(text_tag(10, false), "span");
        assert_eq!(text_tag(30, true), "p");
        assert_eq!(text_tag(41, false), "p");
    }
}
