//! What changed since a handoff point, sorted the way the code side acts on it: token values
//! once (regenerate the theme), main components once (their instances follow), then screens.
//! Built on [`diff_pages`]; what this adds is dropping what code never sees (sizes that follow
//! content, positions set by auto layout, instance fields that follow their main, a token's
//! new value repeated on every node using it) and naming the action for each kind.

use crate::mcp::diff::{NodeDiff, PropChange, diff_pages};
use crate::model::{Document, Node, NodeId, NodeKind, Paint, Sizing, Source};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Copy,
    Binding,
    Style,
    Layout,
    Structure,
    Instance,
    Note,
    Added,
    Removed,
    Renamed,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Copy => "copy",
            Kind::Binding => "binding",
            Kind::Style => "style",
            Kind::Layout => "layout",
            Kind::Structure => "structure",
            Kind::Instance => "instance",
            Kind::Note => "note",
            Kind::Added => "added",
            Kind::Removed => "removed",
            Kind::Renamed => "renamed",
        }
    }
}

/// What each kind asks of the code; printed once at the top of a report.
pub const ACTIONS: &str = "Actions: copy = update the string · binding/style = restyle that node · layout/structure = rebuild that subtree (get_design_spec {nodeId, format: compact}) · instance = swap the component or its props · note = review the rule · added = build from its spec · removed = delete it · renamed = rename the symbol or layer id.";

/// A move or resize smaller than this is noise.
const MIN_PX: f64 = 1.0;

#[derive(Debug, Serialize)]
pub struct Line {
    pub kind: Kind,
    pub key: String,
    /// Layer names from the screen (or main) down, without it.
    pub path: String,
    pub field: String,
    pub before: String,
    pub after: String,
}

/// The changes of one screen or one main component.
#[derive(Debug, Serialize)]
pub struct Group {
    pub page: String,
    pub key: String,
    pub name: String,
    pub kinds: BTreeSet<Kind>,
    pub lines: Vec<Line>,
    /// Screens using a main component.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub used_by: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Token {
    pub name: String,
    pub before: String,
    pub after: String,
    /// Screens drawing with it.
    pub screens: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub since: String,
    pub tokens: Vec<Token>,
    pub components: Vec<Group>,
    pub screens: Vec<Group>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty() && self.components.is_empty() && self.screens.is_empty()
    }
}

/// One page at the handoff point and now (None where it did not exist).
pub struct PagePair<'a> {
    pub id: &'a str,
    pub name: &'a str,
    /// Prefix of its node keys (`<prefix>:<id>`).
    pub prefix: String,
    pub before: Option<&'a Document>,
    pub after: Option<&'a Document>,
}

/// The pages of one side, by page id, to find main components across pages.
type Side<'a> = BTreeMap<&'a str, &'a Document>;

pub fn report(since: &str, pairs: &[PagePair]) -> Report {
    let empty = Document::default();
    let before: Side = pairs.iter().filter_map(|p| Some((p.id, p.before?))).collect();
    let after: Side = pairs.iter().filter_map(|p| Some((p.id, p.after?))).collect();
    let mut groups: BTreeMap<(bool, usize, u64), Group> = BTreeMap::new();
    let mut tokens: BTreeMap<String, Token> = BTreeMap::new();
    for (pi, p) in pairs.iter().enumerate() {
        let (b, a) = (p.before.unwrap_or(&empty), p.after.unwrap_or(&empty));
        let key = |id: NodeId| format!("{}:{}", p.prefix, id.0);
        let mut push = |doc: &Document, id: NodeId, line: Option<Line>, kind: Kind| {
            let Some((main, owner)) = owner_of(doc, id) else { return };
            let g = groups.entry((main, pi, owner.0)).or_insert_with(|| Group {
                page: p.name.to_owned(),
                key: key(owner),
                name: doc.get(owner).map(|n| n.name.clone()).unwrap_or_default(),
                kinds: BTreeSet::new(),
                lines: Vec::new(),
                used_by: Vec::new(),
            });
            g.kinds.insert(kind);
            g.lines.extend(line);
        };
        let d = diff_pages(b, a);
        for nd in d.groups.iter().flat_map(|g| &g.nodes) {
            let removed = nd.status == "removed";
            let (doc, id) = if removed { (b, nd.id_a) } else { (a, nd.id_b) };
            let Some(id) = id else { continue };
            let owner = owner_of(doc, id).map(|(_, o)| o);
            let path = path_of(doc, id, owner);
            if owner == Some(id) && nd.status != "modified" {
                push(doc, id, None, if removed { Kind::Removed } else { Kind::Added });
                continue;
            }
            if nd.status != "modified" {
                let what = format!("{} \"{}\"", nd.kind, nd.name);
                let (before, after) = if removed { (what, String::new()) } else { (String::new(), what) };
                let line = Line { kind: Kind::Structure, key: key(id), path, field: nd.status.into(), before, after };
                push(doc, id, Some(line), Kind::Structure);
                continue;
            }
            let (Some(na), Some(nb)) = (b.get(id), a.get(id)) else { continue };
            for (kind, c) in classify(nd, na, nb, a, &after) {
                let kind = if owner == Some(id) && c.field == "name" { Kind::Renamed } else { kind };
                let line = Line { kind, key: key(id), path: path.clone(), field: c.field.into(), before: c.before, after: c.after };
                push(a, id, Some(line), kind);
            }
        }
        // Instance swaps and property values: not part of a node's own properties.
        for (id, nb) in &a.nodes {
            let Some(na) = b.get(*id) else { continue };
            let (Some(la), Some(lb)) = (&na.instance, &nb.instance) else { continue };
            if !is_instance_root(nb, a, &after) {
                continue;
            }
            let path = path_of(a, *id, owner_of(a, *id).map(|(_, o)| o));
            let line = |field: &str, before: String, after: String| Line { kind: Kind::Instance, key: key(*id), path: path.clone(), field: field.into(), before, after };
            if la.target() != lb.target() {
                let name = |side: &Side, l: &crate::model::InstanceLink, doc: &Document| main_of(side, l, doc).map_or("?".into(), |m| m.name.clone());
                push(a, *id, Some(line("component", name(&before, la, b), name(&after, lb, a))), Kind::Instance);
            }
            if na.prop_values != nb.prop_values {
                let show = |v: &BTreeMap<String, String>| v.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ");
                push(a, *id, Some(line("props", show(&na.prop_values), show(&nb.prop_values))), Kind::Instance);
            }
        }
        for ad in &d.annotations {
            let (old, new) = (b.annotations.iter().find(|n| n.id == ad.id), a.annotations.iter().find(|n| n.id == ad.id));
            let Some((doc, target)) = new.and_then(|n| Some((a, n.target?))).or_else(|| old.and_then(|n| Some((b, n.target?)))) else { continue };
            let show = |n: Option<&crate::model::annotation::Annotation>| n.map_or(String::new(), |n| format!("{} — {}", n.label, n.body.lines().next().unwrap_or("")).trim_end_matches(" — ").to_owned());
            let line = Line { kind: Kind::Note, key: key(target), path: path_of(doc, target, owner_of(doc, target).map(|(_, o)| o)), field: "note".into(), before: show(old), after: show(new) };
            push(doc, target, Some(line), Kind::Note);
        }
        token_changes(b, a, &mut tokens);
    }
    // Who uses each changed main, and each changed token.
    let screens_using = |hit: &dyn Fn(&str, &Document, &Node) -> bool| -> Vec<String> {
        let mut names = BTreeSet::new();
        for (pid, doc) in &after {
            for n in doc.nodes.values().filter(|n| hit(pid, doc, n)) {
                if let Some((false, s)) = owner_of(doc, n.id) {
                    names.insert(doc.get(s).map(|s| s.name.clone()).unwrap_or_default());
                }
            }
        }
        names.into_iter().collect()
    };
    for ((main, pi, owner), g) in groups.iter_mut() {
        if !*main {
            continue;
        }
        let (page, owner) = (pairs[*pi].id, NodeId(*owner));
        g.used_by = screens_using(&|pid, doc, n| {
            n.instance.as_ref().is_some_and(|l| {
                let (m, src) = l.target();
                m == owner && match src {
                    None => pid == page,
                    Some(Source::Page(p)) => p == page,
                    Some(Source::Library(_)) => false,
                }
            }) && is_instance_root(n, doc, &after)
        });
    }
    for t in tokens.values_mut() {
        t.screens = screens_using(&|_, _, n| uses_var(n, &t.name));
    }
    let (components, screens): (Vec<_>, Vec<_>) = groups.into_iter().partition(|((main, ..), _)| *main);
    Report {
        since: since.into(),
        tokens: tokens.into_values().collect(),
        components: components.into_iter().map(|(_, g)| g).collect(),
        screens: screens.into_iter().map(|(_, g)| g).collect(),
    }
}

/// Who a node's change belongs to: `(true, main)` inside a main component, else `(false,
/// screen)`, the nearest declared screen above it or its top-level frame.
fn owner_of(doc: &Document, id: NodeId) -> Option<(bool, NodeId)> {
    let chain: Vec<NodeId> = std::iter::once(id).chain(doc.ancestors(id)).collect();
    if let Some(m) = chain.iter().find(|i| doc.get(**i).is_some_and(|n| n.component)) {
        return Some((true, *m));
    }
    let top = *chain.last()?;
    let screen = chain.iter().find(|i| doc.get(**i).is_some_and(|n| n.screen)).copied();
    doc.get(top).filter(|n| n.is_frame() || screen.is_some()).map(|_| (false, screen.unwrap_or(top)))
}

fn path_of(doc: &Document, id: NodeId, owner: Option<NodeId>) -> String {
    let mut names = Vec::new();
    let mut cur = Some(id);
    while let Some(n) = cur.filter(|c| Some(*c) != owner).and_then(|c| doc.get(c)) {
        names.push(n.name.as_str());
        cur = n.parent;
    }
    names.reverse();
    names.join(" > ")
}

fn main_of<'a>(side: &Side<'a>, l: &crate::model::InstanceLink, own: &'a Document) -> Option<&'a Node> {
    let (m, src) = l.target();
    match src {
        None => own.get(m),
        Some(Source::Page(p)) => side.get(p.as_str()).and_then(|d| d.get(m)),
        Some(Source::Library(_)) => None,
    }
}

/// The top of an instance: its link points at a main component itself (a library main counts).
fn is_instance_root(n: &Node, doc: &Document, side: &Side) -> bool {
    n.instance.as_ref().is_some_and(|l| matches!(l.target().1, Some(Source::Library(_))) || main_of(side, l, doc).is_some_and(|m| m.component))
}

/// The changes of a node present on both sides that code sees, by kind.
fn classify(nd: &NodeDiff, na: &Node, nb: &Node, doc: &Document, side: &Side) -> Vec<(Kind, PropChange)> {
    // Inside an instance a field follows its main unless overridden; the main reports it.
    let inner = nb.instance.is_some() && !is_instance_root(nb, doc, side);
    let follows = |key: &str| inner && !nb.instance.as_ref().is_some_and(|l| l.overrides.iter().any(|o| o == key));
    let mut out = Vec::new();
    for c in &nd.changes {
        let (kind, key) = match c.field {
            "text" => (Kind::Copy, "text"),
            "fills" => {
                let (x, y) = (fills(na), fills(nb));
                if x == y || follows("fills") {
                    continue;
                }
                let kind = if x.contains("--") || y.contains("--") { Kind::Binding } else { Kind::Style };
                out.push((kind, PropChange { field: "fills", before: x, after: y }));
                continue;
            }
            "stroke" => {
                let (x, y) = (stroke(na), stroke(nb));
                if x == y || follows("stroke") {
                    continue;
                }
                let kind = if x.contains("--") || y.contains("--") { Kind::Binding } else { Kind::Style };
                out.push((kind, PropChange { field: "stroke", before: x, after: y }));
                continue;
            }
            "corner_radius" => (Kind::Style, "radius"),
            "opacity" => (Kind::Style, "opacity"),
            "font_size" => (Kind::Style, "font"),
            "size" if !sized_by_hand(na, nb) => continue,
            "size" => (Kind::Layout, "size"),
            "position" if !placed_by_hand(nb, doc) || moved_less(na, nb) => continue,
            "position" => (Kind::Layout, "position"),
            "layout" => (Kind::Layout, "layout"),
            "visible" => (Kind::Structure, "visible"),
            "name" => (Kind::Structure, "name"),
            "parent" | "order" => (Kind::Structure, ""),
            "comments" => continue,
            _ => (Kind::Style, ""),
        };
        if !key.is_empty() && follows(key) {
            continue;
        }
        out.push((kind, c.clone()));
    }
    out
}

/// A size code sets: an axis that is fixed and changed by a pixel or more (hug and fill sizes,
/// and auto-width text, follow their content or parent).
fn sized_by_hand(na: &Node, nb: &Node) -> bool {
    let auto_text = matches!(nb.kind, NodeKind::Text { auto_width: true, .. });
    let s = nb.layout_child.sizing;
    let w = s[0] == Sizing::Fixed && !auto_text && (na.rect.w - nb.rect.w).abs() >= MIN_PX;
    let h = s[1] == Sizing::Fixed && !matches!(nb.kind, NodeKind::Text { .. }) && (na.rect.h - nb.rect.h).abs() >= MIN_PX;
    w || h
}

/// A position code sets: not a top-level frame on the canvas, not placed by auto layout.
fn placed_by_hand(n: &Node, doc: &Document) -> bool {
    n.parent.and_then(|p| doc.get(p)).is_some_and(|p| p.layout.is_none() || n.layout_child.absolute)
}

fn moved_less(na: &Node, nb: &Node) -> bool {
    (na.rect.x - nb.rect.x).abs() < MIN_PX && (na.rect.y - nb.rect.y).abs() < MIN_PX
}

/// Fills as code writes them: a token name, or a value when there is none.
fn fills(n: &Node) -> String {
    let all: Vec<String> = n
        .style
        .fills
        .iter()
        .map(|p| match p {
            Paint::Solid { var: Some(v), .. } => format!("--{v}"),
            Paint::Solid { color, var: None } => color.to_hex(),
            Paint::Image { .. } => "image".into(),
            Paint::Linear { .. } => "linear-gradient".into(),
            Paint::Radial { .. } => "radial-gradient".into(),
        })
        .collect();
    if all.is_empty() { "none".into() } else { all.join(", ") }
}

fn stroke(n: &Node) -> String {
    let Some(c) = n.style.stroke.filter(|_| n.style.stroke_width > 0.0) else { return "none".into() };
    let color = n.bindings.get("stroke").map_or_else(|| c.to_hex(), |v| format!("--{}", v.trim_start_matches("--")));
    format!("{color} {}px", n.style.stroke_width)
}

fn uses_var(n: &Node, name: &str) -> bool {
    let bare = name.trim_start_matches("--");
    n.style.fills.iter().any(|p| matches!(p, Paint::Solid { var: Some(v), .. } if v == bare)) || n.bindings.values().any(|v| v.trim_start_matches("--") == bare)
}

/// Variable values (each mode) and dimension tokens that changed, once per name across pages.
fn token_changes(b: &Document, a: &Document, out: &mut BTreeMap<String, Token>) {
    let mut values = |name: &str, mode: Option<&str>, x: Option<String>, y: Option<String>| {
        if x == y {
            return;
        }
        let show = |v: Option<String>| v.unwrap_or_else(|| "none".into());
        let label = |v: String| mode.map_or(v.clone(), |m| format!("{m} {v}"));
        let t = out.entry(format!("--{name}")).or_insert_with(|| Token { name: format!("--{name}"), before: String::new(), after: String::new(), screens: Vec::new() });
        let (bx, ay) = (label(show(x)), label(show(y)));
        if !t.before.split("; ").any(|s| s == bx) {
            t.before = [t.before.as_str(), bx.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("; ");
            t.after = [t.after.as_str(), ay.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("; ");
        }
    };
    // With modes, the base values are only the active mode's: switching modes is no change.
    if b.modes.is_empty() && a.modes.is_empty() {
        let names: BTreeSet<&String> = b.variables.keys().chain(a.variables.keys()).collect();
        for n in names {
            values(n, None, b.variables.get(n).map(|c| c.to_hex()), a.variables.get(n).map(|c| c.to_hex()));
        }
    }
    let modes: BTreeSet<&String> = b.modes.keys().chain(a.modes.keys()).collect();
    for m in modes {
        let (x, y) = (b.modes.get(m), a.modes.get(m));
        let names: BTreeSet<&String> = x.into_iter().chain(y).flat_map(|v| v.keys()).collect();
        for n in names {
            values(n, Some(m), x.and_then(|v| v.get(n)).map(|c| c.to_hex()), y.and_then(|v| v.get(n)).map(|c| c.to_hex()));
        }
    }
    let names: BTreeSet<&String> = b.tokens.keys().chain(a.tokens.keys()).collect();
    for n in names {
        values(n, None, b.tokens.get(n).cloned(), a.tokens.get(n).cloned());
    }
}
