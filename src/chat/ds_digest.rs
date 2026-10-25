//! A short summary of the open project's design system (variables, tokens, components,
//! libraries, annotation types) that goes to the chat agent with every turn, so it reuses what is
//! there instead of drawing it again. Built from the editor in-process; the tools that return the
//! full data are `get_variables`, `get_components` and `get_annotation_template`.

use crate::editor::Editor;
use crate::libraries::EnabledStatus;
use crate::model::{Document, NodeId, Source};
use crate::variables::{self, Collection, DesignSystem, Kind, PageModes, Value};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// About 1,500 tokens (4 bytes a token).
pub const DIGEST_MAX_BYTES: usize = 6_000;
/// Bytes per token for the context meter's estimate.
pub const BYTES_PER_TOKEN: usize = 4;
pub const REUSE_RULE: &str = "Reuse: use `create_instance`/`swap_instance` for anything matching a component; colors must be `var(--…)`; spacing on tokens.";
const HEAD: &str = "Design system of the project (summary). A variable `group: leaf=value` is var(--group-leaf), with / written as -.";
const MORE: &str = "Truncated: call get_components / get_variables for the rest.";
/// Of the budget left after the fixed lines, the share collapsed variable groups may take.
const VARS_SHARE: usize = 2;

/// The project design system; a project that never went through the session reads as its first
/// page does (as `get_variables` does).
fn design_of(ed: &Editor) -> DesignSystem {
    if ed.design.collections.is_empty() && !ed.pages.is_empty() {
        return variables::migrate_from_document(ed.page_doc(0));
    }
    ed.design.clone()
}

/// One main component: where it lives, what it is called and how often it is used.
struct Comp {
    key: String,
    name: String,
    w: f64,
    h: f64,
    uses: usize,
}

/// Page mains and enabled-library mains, most used first, then by name.
fn components(ed: &Editor) -> Vec<Comp> {
    // Instance roots anywhere, by where their main lives (page index or library id) and its node.
    let mut page_uses: HashMap<(usize, NodeId), usize> = HashMap::new();
    let mut lib_uses: HashMap<(&str, NodeId), usize> = HashMap::new();
    for page in 0..ed.pages.len() {
        for n in ed.page_doc(page).nodes.values() {
            let Some(l) = &n.instance else { continue };
            let (main, source) = l.target();
            match source {
                None if n.id != main => *page_uses.entry((page, main)).or_default() += 1,
                None => {}
                Some(Source::Page(p)) => {
                    if let Some(i) = ed.pages.iter().position(|s| s.id == *p) {
                        *page_uses.entry((i, main)).or_default() += 1;
                    }
                }
                Some(Source::Library(lib)) => *lib_uses.entry((lib.as_str(), main)).or_default() += 1,
            }
        }
    }
    let mut out = Vec::new();
    let mut add = |doc: &Document, uses: &dyn Fn(NodeId) -> usize, key: &dyn Fn(NodeId) -> String| {
        for c in doc.nodes.values().filter(|n| n.component) {
            let key = c.component_key.clone().unwrap_or_else(|| key(c.id));
            out.push(Comp { key, name: c.name.clone(), w: c.rect.w, h: c.rect.h, uses: uses(c.id) });
        }
    };
    for page in 0..ed.pages.len() {
        let uses = |id| page_uses.get(&(page, id)).copied().unwrap_or(0);
        add(ed.page_doc(page), &uses, &|id| crate::mcp::exec::node_key(ed, page, id));
    }
    for (lib, doc) in &ed.library_docs {
        let uses = |id| lib_uses.get(&(lib.as_str(), id)).copied().unwrap_or(0);
        add(doc, &uses, &|id| format!("{lib}:{}", id.0));
    }
    out.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.name.cmp(&b.name)).then_with(|| a.key.cmp(&b.key)));
    // Copies imported to other pages share the name: one line, the most used main's key, all uses.
    let mut merged: Vec<Comp> = Vec::with_capacity(out.len());
    for c in out {
        match merged.iter_mut().find(|m| m.name == c.name) {
            Some(m) => m.uses += c.uses,
            None => merged.push(c),
        }
    }
    merged.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.name.cmp(&b.name)));
    merged
}

/// Hash of what the digest is made of: variables and tokens, component keys and names, enabled
/// library versions and the annotation template. Moving or editing other nodes leaves it as is.
pub fn key(ed: &Editor) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&design_of(ed).collections).unwrap_or_default().hash(&mut h);
    for l in &ed.design.libraries {
        (&l.library_id, l.version).hash(&mut h);
    }
    for c in components(ed) {
        (c.key, c.name, c.w.to_bits(), c.h.to_bits()).hash(&mut h);
    }
    if !ed.pages.is_empty() {
        ed.active_template(ed.active_page).id.hash(&mut h);
    }
    h.finish()
}

/// `color/brand/primary` -> (`color/brand`, `primary`); `spacing-md` -> (`spacing`, `md`): both
/// print as `group: leaf=…`, and `var(--group-leaf)` is the CSS name either way.
fn split(name: &str) -> (&str, &str) {
    name.rsplit_once('/').or_else(|| name.split_once('-')).unwrap_or(("", name))
}

/// Values in each mode, `#a/#b` (one value when every mode agrees). None for strings and booleans.
fn values(ds: &DesignSystem, c: &Collection, var_id: &str) -> Option<String> {
    let per_mode: Vec<String> = c
        .modes
        .iter()
        .filter_map(|m| {
            let modes: PageModes = [(c.id.clone(), m.id.clone())].into();
            match ds.resolve(var_id, &modes).ok()? {
                Value::Color(col) => Some(col.to_hex()),
                Value::Number(n) => Some(format!("{}", (n * 100.0).round() / 100.0)),
                _ => None,
            }
        })
        .collect();
    if per_mode.is_empty() {
        return None;
    }
    Some(if per_mode.iter().all(|v| *v == per_mode[0]) { per_mode[0].clone() } else { per_mode.join("/") })
}

/// One line per variable group of each collection; `collapsed` gives counts instead of names.
fn variable_lines(ds: &DesignSystem, collapsed: bool) -> Vec<String> {
    let mut out = Vec::new();
    for c in &ds.collections {
        let shown: Vec<_> = c.variables.iter().filter(|v| !v.hidden).collect();
        if shown.is_empty() {
            continue;
        }
        let modes = c.modes.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join("/");
        let lib = c.library.as_deref().map(|l| format!(", library {l}")).unwrap_or_default();
        out.push(format!("{} (modes {modes}{lib}):", c.name));
        let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
        for v in shown {
            let (group, leaf) = split(&v.name);
            let item = match (v.kind, values(ds, c, &v.id)) {
                (Kind::Color | Kind::Number, Some(val)) => format!("{leaf}={val}"),
                _ => leaf.to_owned(),
            };
            match groups.iter_mut().find(|g| g.0 == group) {
                Some(g) => g.1.push(item),
                None => groups.push((group, vec![item])),
            }
        }
        for (group, items) in groups {
            let n = items.len();
            let label = if group.is_empty() { String::new() } else { format!("{group}: ") };
            out.push(if collapsed { format!("  {label}{n} variables") } else { format!("  {label}{}", items.join(" ")) });
        }
    }
    out
}

fn component_line(c: &Comp) -> String {
    format!("  {} \u{b7} {} \u{b7} {}\u{d7}{} \u{b7} {}", c.key, c.name, c.w.round(), c.h.round(), c.uses)
}

fn library_line(s: &EnabledStatus) -> String {
    let state = match s.source_version {
        None => "source missing".to_owned(),
        Some(v) if s.update_available => format!("update to v{v} available"),
        Some(_) => "up to date".to_owned(),
    };
    format!("  {} v{} ({state})", s.name, s.local_version)
}

/// The digest, at most [`DIGEST_MAX_BYTES`]. Over the cap, variable groups collapse to counts and
/// only the most used components are kept, with a line saying where the rest is.
pub fn build(ed: &Editor, libs: &[EnabledStatus]) -> String {
    let ds = design_of(ed);
    let comps = components(ed);
    let head = HEAD.to_owned();
    let mut tail = Vec::new();
    if !libs.is_empty() {
        tail.push("Libraries:".to_owned());
        tail.extend(libs.iter().map(library_line));
    }
    if !ed.pages.is_empty() {
        let t = ed.active_template(ed.active_page);
        tail.push(format!("Annotation types ({}): {}", t.name, t.enabled_keys().join(", ")));
    }
    tail.push(REUSE_RULE.to_owned());
    let comp_head = "Components (componentKey \u{b7} name \u{b7} w\u{d7}h \u{b7} uses):".to_owned();
    let section = |lines: &[String]| lines.iter().map(|l| l.len() + 1).sum::<usize>();

    let vars = variable_lines(&ds, false);
    let all_comps: Vec<String> = comps.iter().map(component_line).collect();
    let mut lines = vec![head.clone()];
    lines.extend(vars);
    if !all_comps.is_empty() {
        lines.push(comp_head.clone());
        lines.extend(all_comps.iter().cloned());
    }
    lines.extend(tail.iter().cloned());
    if section(&lines) <= DIGEST_MAX_BYTES {
        return lines.join("\n");
    }

    // ponytail: greedy fill, variables first up to a share of the room, then components by use.
    let fixed = section(&[head.clone(), comp_head.clone(), MORE.to_owned()]) + section(&tail);
    let room = DIGEST_MAX_BYTES.saturating_sub(fixed);
    let mut out = vec![head];
    let mut used = 0;
    for l in variable_lines(&ds, true) {
        if used + l.len() + 1 > room / VARS_SHARE {
            break;
        }
        used += l.len() + 1;
        out.push(l);
    }
    out.push(comp_head);
    for l in all_comps {
        if used + l.len() + 1 > room {
            break;
        }
        used += l.len() + 1;
        out.push(l);
    }
    out.extend(tail);
    out.push(MORE.to_owned());
    out.join("\n")
}

/// The digest with its key, rebuilt only when the key changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cached {
    pub key: u64,
    pub text: String,
}

impl Cached {
    /// Rebuilds when the design system changed; `libs` is only asked for then (it reads the
    /// library store). Returns whether it rebuilt.
    pub fn refresh(&mut self, ed: &Editor, libs: impl FnOnce() -> Vec<EnabledStatus>) -> bool {
        let k = key(ed);
        if k == self.key && !self.text.is_empty() {
            return false;
        }
        *self = Cached { key: k, text: build(ed, &libs()) };
        true
    }

    /// Estimated tokens, for the context meter.
    pub fn tokens(&self) -> u64 {
        (self.text.len() / BYTES_PER_TOKEN) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{Out, exec};
    use serde_json::{Value as J, json};

    fn host() -> exec::SessionHost {
        let dir = std::env::temp_dir().join(format!("papercraft-digest-{:016x}", fastrand::u64(..)));
        let mut h = exec::SessionHost::new(crate::project::Store::open(dir).unwrap());
        h.session.create(&mut h.ed, "Digest").unwrap();
        h
    }

    fn call(h: &mut exec::SessionHost, name: &str, args: J) -> J {
        match exec::call(h, name, &args).unwrap_or_else(|e| panic!("{name}: {e}")).into_iter().next() {
            Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(J::String(t)),
            _ => J::Null,
        }
    }

    /// A project with two-mode colors, spacing tokens and `n` components; the first is used 3 times.
    fn fixture(n: usize) -> exec::SessionHost {
        let mut h = host();
        call(&mut h, "set_variables", json!({"variables": {"accent": "#cba6f7", "text": "#111111"}}));
        call(&mut h, "set_tokens", json!({"tokens": {"spacing-sm": "8px", "spacing-md": "16px", "radius-md": "8px"}}));
        let ab = call(&mut h, "create_artboard", json!({"name": "Screen", "styles": {"width": "400px", "height": "400px", "display": "flex"}}));
        let mut first = None;
        for i in 0..n {
            let r = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children",
                "html": format!("<div layer-name=\"Part {i:03} with a fairly long descriptive name\" style=\"display:flex;padding:8px\"><div style=\"font-size:14px\">x</div></div>")}));
            let c = call(&mut h, "create_component", json!({"nodeId": r["createdNodeIds"][0], "name": format!("Part {i:03} with a fairly long descriptive name")}));
            first.get_or_insert(c["id"].clone());
        }
        for _ in 0..3 {
            call(&mut h, "create_instance", json!({"componentId": first.clone().unwrap(), "parentId": ab["id"]}));
        }
        h
    }

    #[test]
    fn digest_lists_the_design_system_and_stays_under_the_cap() {
        let h = fixture(3);
        let d = build(&h.ed, &[]);
        assert!(d.len() <= DIGEST_MAX_BYTES, "{}", d.len());
        assert!(d.contains("accent=#cba6f7") && d.contains("  accent=#cba6f7 text=#111111\n") && d.contains("spacing: md=16 sm=8"), "{d}");
        assert!(d.contains("Part 000 with a fairly long descriptive name \u{b7} ") && d.contains(" \u{b7} 3\n"), "{d}");
        assert!(d.contains("Annotation types (") && d.ends_with(REUSE_RULE) && !d.contains(MORE), "{d}");

        let h = fixture(120);
        let d = build(&h.ed, &[]);
        assert!(d.len() <= DIGEST_MAX_BYTES, "{}", d.len());
        assert!(d.contains(MORE) && d.contains("variables") && !d.contains("accent=#"), "{d}");
        // The most used component is kept first; the least used tail is dropped.
        let comps: Vec<&str> = d.lines().skip_while(|l| !l.starts_with("Components")).skip(1).take_while(|l| l.starts_with("  ")).collect();
        assert!(comps[0].contains("Part 000") && comps[0].ends_with(" 3"), "{comps:?}");
        assert!(comps.len() < 120 && !d.contains("Part 119"), "{}", comps.len());
    }

    #[test]
    fn key_ignores_unrelated_moves_and_follows_the_design_system() {
        let mut h = fixture(2);
        let k = key(&h.ed);
        let ab = call(&mut h, "create_artboard", json!({"name": "Other", "styles": {"width": "50px", "height": "50px"}}));
        let k2 = key(&h.ed);
        call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [ab["id"]], "styles": {"left": "900px", "top": "40px"}}]}));
        assert_eq!(key(&h.ed), k2, "moving a node keeps the key");
        assert_eq!(k, k2, "a new plain frame keeps the key");
        call(&mut h, "set_variables", json!({"variables": {"accent": "#000000"}}));
        assert_ne!(key(&h.ed), k2, "a variable change changes it");
        let mut c = Cached::default();
        assert!(c.refresh(&h.ed, Vec::new));
        assert!(!c.refresh(&h.ed, || panic!("not rebuilt without a change")));
        assert!(c.tokens() > 0);
    }
}
