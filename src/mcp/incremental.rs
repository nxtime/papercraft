//! Incremental lint: `get_changes`, the `rev` every write tool reports, and the scope
//! `lint_design {since, changedOnly}` checks. The log itself is `editor::changes`.

use super::exec::{find_page, node_key};
use super::{Out, ToolResult, tools};
use crate::design_system::sanitize_name;
use crate::editor::Editor;
use crate::editor::changes::Filter;
use crate::model::{Document, NodeId};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashSet};

/// What a lint pass covers.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    /// The layers to check, or None for all of them.
    pub ids: Option<HashSet<NodeId>>,
    /// The revision the scope was computed from.
    pub since: Option<u64>,
    /// Why a `since` or `changedOnly` request was answered with a full pass.
    pub fallback: Option<&'static str>,
}

impl Scope {
    fn full(fallback: Option<&'static str>) -> Scope {
        Scope { ids: None, since: None, fallback }
    }
}

/// The layers a change to `changed` can affect: themselves, their ancestors (overflow, layout),
/// descendants, and siblings (spacing, reading order). Layers that no longer exist are skipped;
/// their parent is in the log too, because deleting a child edits its parent's list.
pub fn expand(doc: &Document, changed: &[NodeId]) -> HashSet<NodeId> {
    let mut out = HashSet::new();
    for id in changed.iter().filter(|id| doc.get(**id).is_some()) {
        out.insert(*id);
        out.extend(doc.ancestors(*id));
        out.extend(doc.subtree(*id));
        let parent = doc.get(*id).and_then(|n| n.parent);
        out.extend(doc.children(parent).iter().copied());
    }
    out
}

/// The scope for linting `roots` of page `page`: everything, or what changed since `since`
/// (or since the running client's last whole-page lint when `changed_only`).
pub fn lint_scope(ed: &Editor, page: usize, roots: &[NodeId], since: Option<u64>, changed_only: bool) -> Scope {
    let page_id = &ed.pages[page].id;
    let since = match (since, changed_only) {
        (Some(s), _) => s,
        (None, true) => match ed.changes.last_lint(page_id) {
            Some(s) => s,
            None => return Scope::full(Some("no earlier lint by this client")),
        },
        (None, false) => return Scope::full(None),
    };
    let log = ed.changes.since(since, Some(page_id), Filter::All);
    if log.truncated {
        return Scope::full(Some("change log no longer reaches that revision"));
    }
    let doc = ed.page_doc(page);
    let changed: Vec<NodeId> = log.changes.iter().flat_map(|g| g.ids.iter().copied()).collect();
    let inside: HashSet<NodeId> = roots.iter().flat_map(|r| doc.subtree(*r)).collect();
    let ids = expand(doc, &changed).into_iter().filter(|id| inside.contains(id)).collect();
    Scope { ids: Some(ids), since: Some(since), fallback: None }
}

/// Which variables a variable tool edits: Some(names) in CSS form, None for "any".
fn variable_names(name: &str, args: &Value) -> Option<Option<BTreeSet<String>>> {
    let keys = |field: &str| -> Option<BTreeSet<String>> {
        let o = args.get(field)?;
        let names: BTreeSet<String> = match o {
            Value::Object(m) => m.keys().map(|k| sanitize_name(k)).collect(),
            Value::Array(a) => a.iter().filter_map(Value::as_str).map(sanitize_name).collect(),
            _ => return None,
        };
        (!names.is_empty()).then_some(names)
    };
    match name {
        "set_variables" => Some(keys("variables")),
        "set_tokens" => Some(keys("tokens")),
        "delete_variables" => Some(keys("names")),
        "set_variable_mode" | "import_design_system" => Some(None),
        _ => None,
    }
}

/// Post-processing of every tool call: variable edits count as touching the layers bound to
/// them, a new project starts a new log, and write tools report the revision they left.
pub fn after_call(ed: &mut Editor, name: &str, args: &Value, r: ToolResult) -> ToolResult {
    let mut out = r?;
    if matches!(name, "open_file" | "create_file") {
        ed.changes.reset();
    }
    if let Some(names) = variable_names(name, args) {
        ed.log_variable_users(names.as_ref());
    }
    if tools::edits_doc(name, args) {
        add_rev(&mut out, ed.changes.rev());
    }
    Ok(out)
}

/// Puts `rev` into the result's JSON object, or appends a `{"rev"}` result when the tool
/// answered with something else.
fn add_rev(out: &mut Vec<Out>, rev: u64) {
    if let Some(Out::Text(t)) = out.first_mut()
        && let Ok(Value::Object(mut m)) = serde_json::from_str::<Value>(t)
    {
        m.insert("rev".into(), json!(rev));
        *t = serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default();
        return;
    }
    out.push(Out::Text(json!({"rev": rev}).to_string()));
}

/// `get_changes`.
pub fn get_changes(ed: &mut Editor, args: &Map<String, Value>) -> ToolResult {
    let since = args
        .get("since")
        .and_then(Value::as_f64)
        .filter(|s| *s >= 0.0)
        .ok_or("`since` must be a revision number (the `rev` of an earlier result)")? as u64;
    let page = match args.get("page").and_then(Value::as_str).filter(|p| !p.is_empty()) {
        Some(p) => Some(find_page(ed, p)?),
        None => None,
    };
    let me = ed.changes.client.clone();
    let by = args.get("by").and_then(Value::as_str).filter(|b| !b.is_empty());
    let filter = match (args.get("exclude").and_then(Value::as_str), by) {
        (Some("self"), _) => match &me {
            Some(w) => Filter::Exclude(&w.key),
            None => return Err("`exclude: \"self\"` needs to know which client is asking; this connection is anonymous".into()),
        },
        (Some(other), _) => return Err(format!("`exclude` must be \"self\", not `{other}`")),
        (None, Some(b)) => Filter::Only(b),
        (None, None) => Filter::All,
    };
    let page_id = page.map(|p| ed.pages[p].id.clone());
    let log = ed.changes.since(since, page_id.as_deref(), filter);
    let changes: Vec<Value> = log
        .changes
        .iter()
        .filter_map(|g| {
            let p = ed.pages.iter().position(|p| p.id == g.page)?;
            let ids: Vec<String> = g.ids.iter().map(|id| node_key(ed, p, *id)).collect();
            Some(json!({"page": g.page, "ids": ids, "by": g.by, "rev": g.rev}))
        })
        .collect();
    Ok(vec![Out::Text(
        serde_json::to_string_pretty(&json!({
            "rev": ed.changes.rev(), "since": since, "truncated": log.truncated, "changes": changes,
        }))
        .unwrap_or_default(),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Node, NodeKind};

    /// root frame 1 > [frame 2 > [3, 4], frame 5 > [6]]
    fn tree() -> Document {
        let mut doc = Document::default();
        let mut add = |parent: Option<NodeId>| {
            let id = doc.alloc_id();
            let mut n = Node::new(id, NodeKind::Frame { clip: false }, Rect::new(0.0, 0.0, 10.0, 10.0));
            n.parent = parent;
            doc.nodes.insert(id, n);
            match parent {
                Some(p) => doc.nodes.get_mut(&p).unwrap().children.push(id),
                None => doc.root.push(id),
            }
            id
        };
        let r = add(None);
        let a = add(Some(r));
        add(Some(a));
        add(Some(a));
        let b = add(Some(r));
        add(Some(b));
        doc
    }

    fn set(v: &[u64]) -> HashSet<NodeId> {
        v.iter().map(|n| NodeId(*n)).collect()
    }

    #[test]
    fn a_change_reaches_ancestors_descendants_and_siblings() {
        let doc = tree();
        // Layer 2: itself, root 1, children 3 and 4, sibling 5. Not 5's child 6.
        assert_eq!(expand(&doc, &[NodeId(2)]), set(&[1, 2, 3, 4, 5]));
        // Layer 3: its parent and root, its sibling 4; not the uncle's subtree.
        assert_eq!(expand(&doc, &[NodeId(3)]), set(&[1, 2, 3, 4]));
    }

    #[test]
    fn a_top_level_layer_reaches_the_other_top_level_layers() {
        let mut doc = tree();
        let id = doc.alloc_id();
        doc.nodes.insert(id, Node::new(id, NodeKind::Frame { clip: false }, Rect::new(0.0, 0.0, 1.0, 1.0)));
        doc.root.push(id);
        assert_eq!(expand(&doc, &[id]), set(&[1, 7]));
    }

    #[test]
    fn deleted_layers_are_skipped() {
        assert!(expand(&tree(), &[NodeId(99)]).is_empty());
    }

    #[test]
    fn variable_tools_name_the_variables_they_edit() {
        let some = |v: &[&str]| Some(Some(v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>()));
        assert_eq!(variable_names("set_variables", &json!({"variables": {"accent": "#fff", "--bg": "#000"}})), some(&["accent", "bg"]));
        assert_eq!(variable_names("delete_variables", &json!({"names": ["accent"]})), some(&["accent"]));
        assert_eq!(variable_names("set_variable_mode", &json!({"mode": "Dark"})), Some(None));
        assert_eq!(variable_names("set_variables", &json!({})), Some(None), "unreadable arguments mean any variable");
        assert_eq!(variable_names("update_styles", &json!({})), None);
    }

    #[test]
    fn rev_goes_into_a_json_object_or_a_result_of_its_own() {
        let mut out = vec![Out::Text("{\"ok\": true}".into())];
        add_rev(&mut out, 7);
        let Out::Text(t) = &out[0] else { panic!() };
        assert_eq!(serde_json::from_str::<Value>(t).unwrap(), json!({"ok": true, "rev": 7}));
        let mut out = vec![Out::Text("done".into())];
        add_rev(&mut out, 8);
        assert_eq!(out.len(), 2);
    }
}
