//! `set_prototype_links`: what clicking a layer does in Present mode.

use super::exec::{page_arg, resolve, text, Args};
use super::ToolResult;
use crate::editor::Editor;
use crate::model::link::valid_url;
use crate::model::{LinkAction, NodeId};
use serde_json::json;

/// One layer's new click behaviour: a frame to go to, or another action, or neither (cleared).
/// Layer's page, id, frame link, other action, fires on hover, tweens matching layers.
type Change = (usize, NodeId, Option<NodeId>, Option<LinkAction>, bool, bool);

/// A frame on the page of the link's layer; links never cross pages.
fn target_frame(ed: &Editor, key: &str, page: usize) -> Result<NodeId, String> {
    let (tp, id) = resolve(ed, key, page)?;
    if tp != page || !ed.page_doc(page).get(id).is_some_and(|n| n.is_frame()) {
        return Err(format!("`{key}` must be a frame on the same page"));
    }
    Ok(id)
}

fn parse_link(ed: &Editor, l: &Args, default_page: usize) -> Result<Change, String> {
    let (p, from) = resolve(ed, l.req_str("nodeId")?, default_page)?;
    let to = l.str("to").filter(|t| !t.is_empty());
    let overlay = l.str("overlay").filter(|t| !t.is_empty());
    let url = l.str("url");
    let action = l.str("action");
    let given = [to.is_some(), overlay.is_some(), url.is_some(), action.is_some()];
    if given.iter().filter(|g| **g).count() > 1 {
        return Err("give one of `to`, `overlay`, `url` or `action` per link".into());
    }
    let (link, act) = match (to, overlay, url, action) {
        (Some(t), ..) => (Some(target_frame(ed, t, p)?), None),
        (_, Some(t), ..) => (None, Some(LinkAction::Overlay { target: target_frame(ed, t, p)? })),
        (.., Some(u), _) => (None, Some(LinkAction::Url { url: valid_url(u)? })),
        (.., Some("back")) => (None, Some(LinkAction::Back)),
        (.., Some("close")) => (None, Some(LinkAction::Close)),
        (.., Some(other)) => return Err(format!("`action` must be \"back\" or \"close\", not `{other}`")),
        _ => (None, None),
    };
    let hover = match l.str("trigger") {
        None | Some("click") => false,
        Some("hover") => true,
        Some(other) => return Err(format!("`trigger` must be \"click\" or \"hover\", not `{other}`")),
    };
    let smart = match l.str("transition") {
        None | Some("glide") => false,
        Some("smart") => true,
        Some(other) => return Err(format!("`transition` must be \"glide\" or \"smart\", not `{other}`")),
    };
    // Hover links open a frame or an overlay; an address or back/close on hover would fire by accident.
    if hover && !(link.is_some() || matches!(act, Some(LinkAction::Overlay { .. }))) {
        return Err("`trigger: hover` needs `to` or `overlay`".into());
    }
    if smart && link.is_none() {
        return Err("`transition: smart` needs `to`".into());
    }
    Ok((p, from, link, act, hover, smart))
}

/// Sets, per layer, one of: `to` (go to a frame), `overlay` (open a frame as an overlay), `url`
/// (https, http, mailto or tel), `action` ("back" or "close"). A layer with none of them loses
/// its link. With `to` or `overlay`, `trigger: "hover"` fires it while the pointer is over the
/// layer; with `to`, `transition: "smart"` tweens same-named layers. Every link is checked before any is applied.
pub(super) fn set_prototype_links(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let mut set = Vec::new();
    for l in a.arr("links")? {
        let l = Args(l.as_object().ok_or("each link must be an object")?);
        set.push(parse_link(ed, &l, page)?);
    }
    let n = set.len();
    for (p, from, link, act, hover, smart) in set {
        ed.with_page(p, |ed| {
            ed.edit("Prototype link", |tx| {
                if let Some(node) = tx.node_mut(from) {
                    node.link = link;
                    node.link_action = act;
                    node.link_hover = hover;
                    node.link_smart = smart;
                }
            })
        });
    }
    text(json!({"linked": n}))
}
