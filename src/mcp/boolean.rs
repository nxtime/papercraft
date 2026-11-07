//! `boolean_nodes`: live boolean groups (union, subtract, intersect, exclude) and their upkeep.

use super::ToolResult;
use super::exec::{Args, node_key, page_arg, resolve_all, text};
use crate::editor::Editor;
use crate::model::{BoolOp, NodeKind};
use serde_json::json;

pub(super) fn boolean_nodes(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let (page, ids) = resolve_all(ed, &a.strs("nodeIds")?, page)?;
    let op = match a.str("op") {
        None => None,
        Some(s) => Some(BoolOp::from_name(s).ok_or_else(|| {
            let names: Vec<&str> = BoolOp::ALL.iter().map(|o| o.name()).collect();
            format!("unknown op `{s}`; use one of {}", names.join(", "))
        })?),
    };
    let flatten = a.bool("flatten").unwrap_or(false);
    ed.with_page(page, |ed| {
        let existing = ids.len() == 1 && ed.doc.get(ids[0]).is_some_and(|n| n.is_boolean());
        if existing {
            if op.is_none() && !flatten {
                return Err(
                    "pass `op` to change the operation or `flatten: true` to bake it into a path"
                        .into(),
                );
            }
            ed.select(ids.clone());
            if let Some(op) = op {
                ed.set_bool_op(op);
            }
            if flatten {
                ed.flatten_selection();
            }
            return text(json!({"id": node_key(ed, page, ids[0]), "flattened": flatten}));
        }
        if flatten {
            return Err("`flatten` applies to one existing boolean group".into());
        }
        let op =
            op.ok_or("missing required argument `op` (union, subtract, intersect or exclude)")?;
        if ids.len() < 2 {
            return Err("a boolean needs at least two nodes".into());
        }
        for id in &ids {
            let n = ed.doc.get(*id).ok_or("node not found")?;
            if matches!(n.kind, NodeKind::Line { .. } | NodeKind::Connector { .. }) {
                return Err(format!(
                    "`{}` is a {} and has no area to combine",
                    n.name,
                    n.kind.type_name()
                ));
            }
        }
        ed.select(ids);
        ed.boolean(op);
        match ed.selection.as_slice() {
            [g] => text(json!({"id": node_key(ed, page, *g), "op": op.name()})),
            _ => Err(ed.status.clone()),
        }
    })
}
