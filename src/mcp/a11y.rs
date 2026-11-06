//! `set_accessibility`: alt text, role, heading level and reading order on layers.

use super::exec::{page_arg, resolve, text, Args};
use super::ToolResult;
use crate::editor::Editor;
use crate::model::a11y::ROLES;
use crate::model::{A11y, NodeId};
use serde_json::{json, Value};

/// The layer's annotations after applying one item; `None` when nothing is left.
fn apply(old: Option<&A11y>, u: &Args) -> Result<Option<A11y>, String> {
    let mut a = if u.bool("clear") == Some(true) { A11y::default() } else { old.cloned().unwrap_or_default() };
    // An explicit null clears a field; a missing key leaves it alone.
    let given = |k: &str| u.0.contains_key(k);
    if given("alt") {
        a.alt = u.str("alt").filter(|t| !t.is_empty()).map(str::to_owned);
    }
    if let Some(d) = u.bool("decorative") {
        a.decorative = d;
    }
    if given("role") {
        a.role = match u.str("role") {
            None => None,
            Some(r) if ROLES.contains(&r) => Some(r.to_owned()),
            Some(r) => return Err(format!("`role` must be one of {}, not `{r}`", ROLES.join(", "))),
        };
    }
    if given("level") {
        a.level = match u.0.get("level") {
            Some(Value::Number(n)) => Some(n.as_u64().filter(|l| (1..=6).contains(l)).ok_or("`level` must be 1 to 6")? as u8),
            Some(Value::Null) | None => None,
            Some(_) => return Err("`level` must be a number from 1 to 6".into()),
        };
    }
    if given("order") {
        a.order = match u.0.get("order") {
            Some(Value::Number(n)) => Some(n.as_u64().and_then(|o| u32::try_from(o).ok()).ok_or("`order` must be a whole number, 0 or more")?),
            Some(Value::Null) | None => None,
            Some(_) => return Err("`order` must be a whole number, 0 or more".into()),
        };
    }
    if a.role.as_deref() != Some("heading") {
        if a.level.is_some() && given("level") {
            return Err("`level` needs role `heading`".into());
        }
        a.level = None;
    }
    if a.decorative && a.alt.is_some() {
        return Err("a decorative layer has no alt text: give `alt` or `decorative`, not both".into());
    }
    Ok((!a.is_empty()).then_some(a))
}

/// Applies every item or none: a refused item changes nothing.
pub(super) fn set_accessibility(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let mut set: Vec<(usize, NodeId, Option<A11y>)> = Vec::new();
    for u in a.arr("items")? {
        let u = Args(u.as_object().ok_or("each item must be an object")?);
        let (p, id) = resolve(ed, u.req_str("nodeId")?, page)?;
        let new = apply(ed.page_doc(p).get(id).and_then(|n| n.a11y.as_ref()), &u)?;
        set.push((p, id, new));
    }
    let n = set.len();
    for (p, id, new) in set {
        ed.with_page(p, |ed| {
            ed.edit("Accessibility", |tx| {
                if let Some(node) = tx.node_mut(id) {
                    node.a11y = new;
                }
            })
        });
    }
    text(json!({"updated": n}))
}
