//! `get_context`: the one compact read an agent makes before editing. It answers where the agent
//! is, what is cheap to read next, which tokens and components are in play and what is already
//! wrong, in about two thousand tokens, so the heavy reads (tree, code, spec) can be aimed at
//! one node.

use super::exec::{artboard_of, design_of, node_key, resolve};
use super::{Out, ToolResult};
use crate::editor::Editor;
use crate::model::{Document, NodeId, Source};
use crate::variables::{DesignSystem, Value as DsVal};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Rough cost of reading one layer through `get_tree_summary` (a line of about 64 bytes).
const TOKENS_PER_NODE: usize = 16;
/// Entries listed per outline level and in the selection; the rest is counted in `more`.
const MAX_LISTED: usize = 24;
const MAX_ISSUES: usize = 5;

pub fn get_context(ed: &mut Editor, id: Option<&str>) -> ToolResult {
    let (page, target) = match id.filter(|s| !s.is_empty() && *s != "root") {
        Some(key) => {
            let (p, n) = resolve(ed, key, ed.active_page)?;
            (p, Some(n))
        }
        None => (ed.active_page, ed.selection.iter().copied().find(|i| ed.doc.get(*i).is_some())),
    };
    let doc = ed.page_doc(page);
    let roots = target.map_or_else(|| doc.root.clone(), |t| vec![t]);
    let parents = target.map_or_else(|| roots.clone(), |t| doc.children(Some(t)).to_vec());
    let scope: Vec<NodeId> = roots.iter().flat_map(|r| doc.subtree(*r)).collect();

    let mut out = json!({"page": {"id": ed.pages[page].id, "name": ed.pages[page].name}});
    if let Some(t) = target {
        out["target"] = brief(ed, page, t);
        if let Some(ab) = artboard_of(doc, t).filter(|ab| *ab != t) {
            out["screen"] = brief(ed, page, ab);
        }
    }
    let selected: Vec<NodeId> = ed.selection.iter().copied().filter(|i| ed.doc.get(*i).is_some()).collect();
    if page == ed.active_page && !selected.is_empty() {
        out["selection"] = json!(selected.iter().take(MAX_LISTED).map(|i| brief(ed, page, *i)).collect::<Vec<_>>());
    }
    out["outline"] = outline(ed, page, &parents, 2);

    let ds = design_of(ed);
    out["variables"] = used_variables(ed, page, &scope, &ds);
    out["collections"] = collections(ed, page, &ds);
    out["components"] = used_components(ed, page, &scope);
    out["lint"] = lint_summary(ed, page, &roots);
    Ok(vec![Out::Text(out.to_string())])
}

fn brief(ed: &Editor, page: usize, id: NodeId) -> Value {
    let n = ed.page_doc(page).get(id);
    json!({"id": node_key(ed, page, id), "name": n.map(|n| n.name.clone()),
           "kind": n.map(|n| n.kind.type_name()),
           "size": n.map(|n| format!("{}x{}", n.rect.w.round(), n.rect.h.round()))})
}

/// `parents` and, `levels` deep, their children, each with a child count and the tokens a full
/// `get_tree_summary` of that layer would cost.
fn outline(ed: &Editor, page: usize, parents: &[NodeId], levels: usize) -> Value {
    let doc = ed.page_doc(page);
    let mut items: Vec<Value> = parents
        .iter()
        .take(MAX_LISTED)
        .filter_map(|id| {
            let n = doc.get(*id)?;
            let mut v = brief(ed, page, *id);
            v["children"] = json!(n.children.len());
            v["estimatedTokens"] = json!(doc.subtree(*id).len() * TOKENS_PER_NODE);
            if levels > 1 && !n.children.is_empty() {
                v["items"] = outline(ed, page, &n.children, levels - 1);
            }
            Some(v)
        })
        .collect();
    if parents.len() > MAX_LISTED {
        items.push(json!({"more": parents.len() - MAX_LISTED}));
    }
    json!(items)
}

fn used_variables(ed: &Editor, page: usize, scope: &[NodeId], ds: &DesignSystem) -> Value {
    let doc = ed.page_doc(page);
    let mut names: BTreeSet<String> = BTreeSet::new();
    for n in scope.iter().filter_map(|i| doc.get(*i)) {
        names.extend(n.style.fills.iter().filter_map(|p| p.var()).map(str::to_string));
        names.extend(n.bindings.values().cloned());
    }
    let modes = &ed.pages[page].modes;
    names
        .into_iter()
        .map(|name| {
            let found = ds.find_by_css(&name);
            let value = match (doc.variables.get(&name), found) {
                (Some(c), _) => json!(c.to_hex()),
                (None, Some((c, v))) => {
                    // Same fallbacks as `DesignSystem::resolve`, which the canvas uses.
                    let mode = modes.get(&c.id).filter(|m| c.modes.iter().any(|x| &x.id == *m)).unwrap_or(&c.default_mode);
                    v.values.get(mode).or_else(|| v.values.get(&c.default_mode)).map_or(Value::Null, |x| ds_value(ds, x))
                }
                (None, None) => Value::Null,
            };
            json!({"name": name, "value": value, "collection": found.map(|(c, _)| c.name.clone())})
        })
        .collect()
}

fn ds_value(ds: &DesignSystem, v: &DsVal) -> Value {
    match v {
        DsVal::Color(c) => json!(c.to_hex()),
        DsVal::Number(n) => json!(n),
        DsVal::String(s) => json!(s),
        DsVal::Boolean(b) => json!(b),
        DsVal::Alias(id) => json!(ds.find(id).map(|(_, t)| t.name.clone())),
    }
}

fn collections(ed: &Editor, page: usize, ds: &DesignSystem) -> Value {
    let modes = &ed.pages[page].modes;
    ds.collections
        .iter()
        .map(|c| {
            let active = modes.get(&c.id).unwrap_or(&c.default_mode);
            let name = c.modes.iter().find(|m| &m.id == active).map(|m| m.name.clone());
            json!({"name": c.name, "modes": c.modes.iter().map(|m| &m.name).collect::<Vec<_>>(), "active": name})
        })
        .collect()
}

/// Components with at least one instance inside `scope`, counting instance roots only.
fn used_components(ed: &Editor, page: usize, scope: &[NodeId]) -> Value {
    let doc = ed.page_doc(page);
    let mut counts: BTreeMap<(usize, NodeId), usize> = BTreeMap::new();
    for n in scope.iter().filter_map(|i| doc.get(*i)) {
        let Some(link) = &n.instance else { continue };
        if n.parent.and_then(|p| doc.get(p)).is_some_and(|p| p.instance.is_some()) {
            continue;
        }
        let (main, source) = link.target();
        let main_page = match source {
            None => page,
            Some(Source::Page(p)) => match ed.pages.iter().position(|s| s.id == *p) {
                Some(i) => i,
                None => continue,
            },
            Some(Source::Library(_)) => continue,
        };
        if main != n.id || main_page != page {
            *counts.entry((main_page, main)).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|((p, main), instances)| {
            json!({"id": node_key(ed, p, main), "name": ed.page_doc(p).get(main).map(|m| m.name.clone()),
                   "instances": instances})
        })
        .collect()
}

/// The lint rules on this scope only: counts per rule and the first few findings.
fn lint_summary(ed: &Editor, page: usize, roots: &[NodeId]) -> Value {
    let doc: &Document = ed.page_doc(page);
    let mut issues = super::lint::lint(doc, roots, &super::lint::DEFAULT_SCALE);
    issues.extend(super::lint::not_an_instance(doc, roots, &super::lint::component_mains(ed)));
    // Layers hidden behind a drawer or modal are not findings: nobody sees them.
    issues.retain(|i| !crate::editor::screens::coverage(doc, i.node).is_some_and(|c| c.is_hidden()));
    let mut by_rule: Map<String, Value> = Map::new();
    for i in &issues {
        let n = by_rule.get(i.rule).and_then(Value::as_u64).unwrap_or(0);
        by_rule.insert(i.rule.to_string(), json!(n + 1));
    }
    let count = |sev: &str| issues.iter().filter(|i| i.severity == sev).count();
    json!({"errors": count("error"), "warnings": count("warning"), "info": count("info"),
           "byRule": by_rule,
           "first": issues.iter().take(MAX_ISSUES).map(|i| json!({
               "rule": i.rule, "severity": i.severity, "node": node_key(ed, page, i.node), "message": i.message,
           })).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Node, NodeKind};
    use crate::variables::Kind;

    #[test]
    fn a_variable_missing_in_the_active_mode_shows_its_default_mode_value() {
        let mut ed = Editor::default();
        let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0., 0., 10., 10.));
        n.bindings.insert("gap".into(), "space".into());
        let id = ed.add_node(n, None);
        let mut ds = DesignSystem::default();
        let c = ds.add_collection("Sizes");
        let dark = ds.add_mode(&c, "Dark").unwrap();
        let v = ds.add_variable(&c, "space", Kind::Number).unwrap();
        let default_mode = ds.collections[0].default_mode.clone();
        ds.set_value(&v, &default_mode, DsVal::Number(8.0)).unwrap();
        ds.collections[0].variables[0].values.remove(&dark);
        ed.pages[0].modes.insert(c, dark);
        let vars = used_variables(&ed, 0, &[id], &ds);
        assert_eq!(vars[0]["value"], json!(8.0));
    }
}
