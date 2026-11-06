//! `set_text_style`: define, change, delete and apply the page's named type styles.

use super::ToolResult;
use super::exec::{Args, node_key, page_arg, resolve_all, text};
use crate::editor::Editor;
use crate::model::TypeStyle;
use serde_json::{Value, json};

/// The page's styles as JSON, for `get_variables`.
pub(super) fn styles_json(doc: &crate::model::Document) -> Value {
    serde_json::to_value(&doc.text_styles).unwrap_or(Value::Null)
}

/// `current` with the fields the call names replaced. `lineHeight` is a multiple of the size;
/// null clears it. A new style needs `fontSize`.
fn merged(current: Option<&TypeStyle>, a: &Args) -> Result<TypeStyle, String> {
    let mut s = match (current, a.num("fontSize")) {
        (Some(c), _) => c.clone(),
        (None, Some(size)) => TypeStyle { font_size: size, font: None, bold: false, italic: false, line_height: None, letter_spacing: 0.0 },
        (None, None) => return Err("a new style needs `fontSize`".into()),
    };
    if let Some(v) = a.num("fontSize") {
        s.font_size = v;
    }
    if a.0.contains_key("font") {
        s.font = a.str("font").filter(|f| !f.is_empty()).map(String::from);
    }
    if let Some(v) = a.bool("bold") {
        s.bold = v;
    }
    if let Some(v) = a.bool("italic") {
        s.italic = v;
    }
    if a.0.contains_key("lineHeight") {
        s.line_height = a.num("lineHeight");
    }
    if let Some(v) = a.num("letterSpacing") {
        s.letter_spacing = v;
    }
    if s.font_size <= 0.0 || s.line_height.is_some_and(|l| l <= 0.0) {
        return Err("`fontSize` and `lineHeight` must be above 0".into());
    }
    Ok(s)
}

pub(super) fn set_text_style(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let action = a.req_str("action")?;
    let name = a.str("name").filter(|n| !n.is_empty());
    match action {
        "set" => {
            let name = name.ok_or("`name` is required")?;
            let style = merged(ed.page_doc(page).text_styles.get(name), a)?;
            let updated = ed.with_page(page, |ed| ed.set_text_style(name, style.clone()));
            text(json!({"name": name, "style": style, "layersUpdated": updated}))
        }
        "delete" => {
            let name = name.ok_or("`name` is required")?;
            let detached = ed.with_page(page, |ed| ed.delete_text_style(name)).ok_or_else(|| format!("no text style `{name}`"))?;
            text(json!({"deleted": name, "layersDetached": detached}))
        }
        "apply" => {
            let keys = a.strs("nodeIds")?;
            let (page, ids) = resolve_all(ed, &keys, page)?;
            let n = ed.with_page(page, |ed| ed.apply_text_style(&ids, name))?;
            let changed: Vec<String> = ids.iter().map(|i| node_key(ed, page, *i)).collect();
            text(json!({"style": name, "layersChanged": n, "nodeIds": changed}))
        }
        _ => Err(format!("unknown action `{action}` (set, delete, apply)")),
    }
}
