//! Design handoff (`specs/design-handoff/spec.md`): handoff points, the change report against
//! one, and the bundle a code base builds from.

#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;
pub mod changes;

use super::exec::Host;
use super::{Out, ToolResult};
use crate::handoffs::{self, Handoff};
use changes::{Group, Kind, PagePair, Report};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Change lines shown per screen in the index; the rest are one `get_changes {nodeId}` away.
const LINES_PER_GROUP: usize = 8;
/// Screen names listed under a changed token or component.
const NAMES_SHOWN: usize = 6;
/// Longest value printed, except copy, which code needs whole.
const VALUE_CHARS: usize = 80;

fn project_dir(host: &dyn Host) -> Result<PathBuf, String> {
    host.project().map(|p| p.dir.clone()).ok_or_else(|| "handoff points live in a project: open one first".into())
}

fn str_arg<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

/// `mark_handoff {name}`: snapshot every page as handoff point `name`.
pub fn mark(host: &mut dyn Host, a: &Value) -> ToolResult {
    let name = str_arg(a, "name").ok_or("`name` is required (e.g. \"design v1\")")?.trim().to_owned();
    let dir = project_dir(host)?;
    let ed = host.editor();
    ed.finish_loading();
    let pages: Vec<handoffs::Page> =
        (0..ed.pages.len()).map(|i| handoffs::Page { id: ed.pages[i].id.clone(), name: ed.pages[i].name.clone(), doc: ed.page_doc(i).clone() }).collect();
    let screens: usize = pages.iter().map(|p| p.doc.root.iter().filter(|id| p.doc.get(**id).is_some_and(|n| n.is_frame() && !n.component)).count()).sum();
    let h = Handoff { name: name.clone(), saved: crate::project::format::now_ms(), pages };
    handoffs::save(&dir, &h).map_err(|e| e.to_string())?;
    text(json!({"name": name, "pages": h.pages.len(), "screens": screens}))
}

/// `list_handoffs`: the project's handoff points, newest first.
pub fn list(host: &mut dyn Host) -> ToolResult {
    let dir = project_dir(host)?;
    let all: Vec<Value> = handoffs::list(&dir).into_iter().map(|h| json!({"name": h.name, "saved": h.saved})).collect();
    text(json!({"handoffs": all}))
}

/// The report of what changed since handoff point `since`.
pub fn report_since(host: &mut dyn Host, since: &str) -> Result<Report, String> {
    let dir = project_dir(host)?;
    let h = handoffs::load(&dir, since)?;
    let ed = host.editor();
    ed.finish_loading();
    let ed: &crate::editor::Editor = ed;
    let mut pairs: Vec<PagePair> = (0..ed.pages.len())
        .map(|i| {
            let p = &ed.pages[i];
            PagePair { id: &p.id, name: &p.name, prefix: prefix(&p.id), before: h.pages.iter().find(|b| b.id == p.id).map(|b| &b.doc), after: Some(ed.page_doc(i)) }
        })
        .collect();
    for b in h.pages.iter().filter(|b| !ed.pages.iter().any(|p| p.id == b.id)) {
        pairs.push(PagePair { id: &b.id, name: &b.name, prefix: prefix(&b.id), before: Some(&b.doc), after: None });
    }
    Ok(changes::report(&h.name, &pairs))
}

/// Node keys use a page's first six characters, as everywhere else (`exec::node_key`).
fn prefix(page_id: &str) -> String {
    page_id[..page_id.len().min(6)].to_owned()
}

/// `get_handoff_changes {since, nodeId?, format?}`.
pub fn get_changes(host: &mut dyn Host, a: &Value) -> ToolResult {
    let since = str_arg(a, "since").ok_or("`since` is required: a handoff point name (list_handoffs)")?;
    let r = report_since(host, since)?;
    let only = str_arg(a, "nodeId");
    match str_arg(a, "format").unwrap_or("markdown") {
        "markdown" | "md" => Ok(vec![Out::Text(markdown(&r, only))]),
        "json" => text(serde_json::to_value(&r).map_err(|e| e.to_string())?),
        f => Err(format!("unknown format `{f}` (markdown or json)")),
    }
}

fn text(v: Value) -> ToolResult {
    Ok(vec![Out::Text(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?)])
}

fn clip(s: &str) -> String {
    if s.chars().count() <= VALUE_CHARS { s.to_owned() } else { format!("{}…", s.chars().take(VALUE_CHARS).collect::<String>()) }
}

fn names(all: &[String]) -> String {
    let shown = all.iter().take(NAMES_SHOWN).map(String::as_str).collect::<Vec<_>>().join(", ");
    match all.len().saturating_sub(NAMES_SHOWN) {
        0 => shown,
        more => format!("{shown} +{more}"),
    }
}

/// The report as an index a reader acts from: one heading per screen or component and its
/// first few changes; `only` (a node key) prints that one group whole.
pub fn markdown(r: &Report, only: Option<&str>) -> String {
    let mut out = format!("# Changes since `{}`\n", r.since);
    if r.is_empty() {
        out.push_str("No changes.\n");
        return out;
    }
    let groups = |gs: &[Group]| gs.iter().filter(|g| only.is_none_or(|k| k == g.key)).count();
    if only.is_none() {
        let new = r.screens.iter().filter(|g| g.kinds.contains(&Kind::Added)).count();
        let gone = r.screens.iter().filter(|g| g.kinds.contains(&Kind::Removed)).count();
        let counts = [(r.tokens.len(), "tokens"), (r.components.len(), "components"), (r.screens.len(), "screens"), (new, "added"), (gone, "removed")];
        let counts: Vec<String> = counts.iter().filter(|(n, _)| *n > 0).map(|(n, w)| if *n == 1 { format!("1 {}", w.trim_end_matches('s')) } else { format!("{n} {w}") }).collect();
        out += &format!("{}\n{}\n", counts.join(" · "), changes::ACTIONS);
        if !r.tokens.is_empty() {
            out.push_str("\n## Tokens: regenerate the theme, no screen work\n");
            for t in &r.tokens {
                out += &format!("- {}: {} → {} · {} screens{}\n", t.name, t.before, t.after, t.screens.len(), if t.screens.is_empty() { String::new() } else { format!(": {}", names(&t.screens)) });
            }
        }
    }
    let cap = if only.is_some() { usize::MAX } else { LINES_PER_GROUP };
    for (title, gs) in [("Components: rebuild the component, its instances follow", &r.components), ("Screens", &r.screens)] {
        if groups(gs) == 0 {
            continue;
        }
        out += &format!("\n## {title}\n");
        for g in gs.iter().filter(|g| only.is_none_or(|k| k == g.key)) {
            let kinds: Vec<&str> = g.kinds.iter().map(|k| k.name()).collect();
            out += &format!("### {} / {} `{}` · {}", g.page, g.name, g.key, kinds.join(", "));
            if !g.used_by.is_empty() {
                out += &format!(" · used on {} screens: {}", g.used_by.len(), names(&g.used_by));
            }
            out.push('\n');
            for l in g.lines.iter().take(cap) {
                let at = if l.path.is_empty() { String::new() } else { format!("{} ", l.path) };
                let key = if l.key == g.key { String::new() } else { format!("`{}` ", l.key) };
                let (b, a) = if l.kind == Kind::Copy { (l.before.clone(), l.after.clone()) } else { (clip(&l.before), clip(&l.after)) };
                let change = match (b.is_empty(), a.is_empty()) {
                    (true, false) => format!("{} {a}", l.field),
                    (false, true) => format!("{} {b}", l.field),
                    _ => format!("{}: {b} → {a}", l.field),
                };
                out += &format!("- {at}{key}{}: {change}\n", l.kind.name());
            }
            if g.lines.len() > cap {
                out += &format!("- … {} more: get_handoff_changes {{since: \"{}\", nodeId: \"{}\"}}\n", g.lines.len() - cap, r.since, g.key);
            }
        }
    }
    out
}
