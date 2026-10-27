//! The Actions menu on the selection (canvas right-click, the composer's Actions chip, the
//! command palette). Deterministic actions run in-process through `runs::Tracker::call`, as one
//! run and one undo step each; intent actions put a scoped prompt and a selection chip in the
//! chat composer and send it.

use super::UiState;
use super::chat_msgs::ACTION_PROMPTS;
use crate::editor::Editor;
use crate::mcp::exec::{Host, find_page, node_key};
use crate::mcp::{ACTIONS_CLIENT, Client, Out};
use crate::model::{Color, Direction, Document, NodeId, Paint};
use crate::runs::Tracker;
use egui::Ui;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Largest RGB distance at which a hard-coded color still binds to a variable.
pub const BIND_COLOR_TOLERANCE: f64 = 12.0;
/// Largest px difference at which a spacing or radius still binds to a token.
pub const BIND_SIZE_TOLERANCE: f64 = 2.0;
/// Longest layer name Rename layers takes from text content.
const NAME_CHARS: usize = 30;
/// Unbound values listed in the status line.
const LEFT_SHOWN: usize = 6;
/// Token names spacing binds to, and radius binds to.
const SPACE_WORDS: [&str; 4] = ["space", "spacing", "gap", "pad"];
const RADIUS_WORDS: [&str; 2] = ["radius", "round"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    FixLint,
    BindTokens,
    UseComponents,
    RenameLayers,
    Annotate,
    AnnotateAi,
    /// An entry of `chat_msgs::ACTION_PROMPTS`.
    Prompt(usize),
}

/// Menu rows in order: the deterministic lane, then the intent lane.
pub fn items() -> Vec<(Act, &'static str)> {
    let mut v = vec![
        (Act::FixLint, "Fix lint here"),
        (Act::BindTokens, "Bind to tokens"),
        (Act::UseComponents, "Use components"),
        (Act::RenameLayers, "Rename layers"),
        (Act::Annotate, "Annotate"),
        (Act::Prompt(0), ACTION_PROMPTS[0].0),
        (Act::Prompt(1), ACTION_PROMPTS[1].0),
        (Act::AnnotateAi, "Annotate with AI"),
    ];
    v.extend((2..ACTION_PROMPTS.len()).map(|i| (Act::Prompt(i), ACTION_PROMPTS[i].0)));
    v
}

fn is_intent(a: Act) -> bool {
    matches!(a, Act::AnnotateAi | Act::Prompt(_))
}

/// The menu rows; returns the one clicked. Off without a selection.
pub fn menu(ui: &mut Ui, ed: &Editor) -> Option<Act> {
    let mut out = None;
    ui.add_enabled_ui(!ed.selection.is_empty(), |ui| {
        let mut lane = false;
        for (act, label) in items() {
            if is_intent(act) != lane {
                lane = true;
                ui.separator();
            }
            if ui.button(label).clicked() {
                out = Some(act);
            }
        }
    });
    out
}

/// The composer's "Actions" chip and its menu.
pub fn chip(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let r = super::kit::chip(ui, "Actions", false).on_hover_text("Actions on the selection");
    let mut act = None;
    egui::Popup::menu(&r).width(200.0).show(|ui| {
        crate::theme::popup_backing(ui);
        act = menu(ui, ed);
        if act.is_some() {
            ui.close();
        }
    });
    if let Some(a) = act {
        dispatch(ed, st, a);
    }
}

/// Starts `act` on the selection: deterministic actions are queued for the host
/// (`UiState::pending_action`), intents fill the composer and send.
pub fn dispatch(ed: &mut Editor, st: &mut UiState, act: Act) {
    let page = ed.active_page;
    let top = ed.top_selection();
    let Some(first) = top.first().copied() else {
        ed.status = "Select a layer first".into();
        return;
    };
    match act {
        Act::Annotate => super::annotations::open_popover(ed, &mut st.ann_ui, first),
        Act::AnnotateAi => {
            super::annotations::annotate_with_ai(ed, st);
            st.chat.send_draft = true;
        }
        Act::Prompt(i) => {
            super::chat::add_selection(ed, &mut st.chat);
            st.chat.draft = ACTION_PROMPTS[i].1.into();
            st.set_chat_mode(true);
            st.chat.send_draft = true;
        }
        _ => st.pending_action = Some((act, top.iter().map(|id| node_key(ed, page, *id)).collect())),
    }
}

// ---------------------------------------------------------------------------- deterministic lane

/// Runs a deterministic action on `keys` as one run of the Actions client and one undo step.
/// The run's title is the summary the toast shows; nothing changed (or an error) goes to the
/// status line.
pub fn run(host: &mut dyn Host, runs: &mut Tracker, act: Act, keys: &[String], now: u64) {
    let client = Client { name: ACTIONS_CLIENT.into(), session: String::new() };
    let undo_from = host.editor().history.undo_len();
    let mut tool = |host: &mut dyn Host, name: &str, args: Value| -> Result<Value, String> {
        let out = runs.call(host, &client, name, &args, now)?;
        Ok(match out.first() {
            Some(Out::Text(t)) => serde_json::from_str(t).unwrap_or(Value::Null),
            _ => Value::Null,
        })
    };
    let (label, r) = match act {
        Act::FixLint => ("Fix lint", fix_lint(host, &mut tool, keys)),
        Act::BindTokens => ("Bind to tokens", bind_tokens(host, &mut tool, keys)),
        Act::UseComponents => ("Use components", use_components(host, &mut tool, keys)),
        Act::RenameLayers => ("Rename layers", rename_layers(host, &mut tool, keys)),
        Act::Annotate | Act::AnnotateAi | Act::Prompt(_) => return,
    };
    host.editor().history.squash_since(undo_from, label);
    let ed = host.editor();
    match r {
        Ok((summary, changed)) => {
            runs.finish_client(now, ed, &client, summary.clone());
            if !changed {
                ed.status = summary;
            }
        }
        Err(e) => {
            runs.finish_client(now, ed, &client, label.into());
            ed.status = format!("{label}: {e}");
        }
    }
}

/// (summary, whether anything changed)
type Outcome = Result<(String, bool), String>;
type Tool<'a> = dyn FnMut(&mut dyn Host, &str, Value) -> Result<Value, String> + 'a;

/// The page and node ids of `keys` (dropped when gone).
fn resolve(ed: &Editor, keys: &[String]) -> (usize, Vec<NodeId>) {
    let mut page = ed.active_page;
    let ids = keys
        .iter()
        .filter_map(|k| {
            let (p, n) = k.split_once(':')?;
            page = find_page(ed, p).ok()?;
            let id = NodeId(n.parse().ok()?);
            ed.page_doc(page).get(id).map(|_| id)
        })
        .collect();
    (page, ids)
}

/// Every layer under `roots` that is not part of an instance, roots first.
fn own_layers(doc: &Document, roots: &[NodeId]) -> Vec<NodeId> {
    roots.iter().flat_map(|r| doc.subtree(*r)).filter(|id| doc.get(*id).is_some_and(|n| n.instance.is_none())).collect()
}

fn fix_lint(host: &mut dyn Host, tool: &mut Tool, keys: &[String]) -> Outcome {
    let mut fixed = 0;
    for k in keys {
        fixed += tool(host, "lint_design", json!({"nodeId": k, "fix": true}))?["fixedNodes"].as_u64().unwrap_or(0);
    }
    Ok(match fixed {
        0 => ("Nothing to fix".into(), false),
        n => (format!("Fixed {}", super::panels::count(n as usize, "layer")), true),
    })
}

fn color_distance(a: Color, b: Color) -> f64 {
    let d = |x: u8, y: u8| f64::from(x) - f64::from(y);
    (d(a.r, b.r).powi(2) + d(a.g, b.g).powi(2) + d(a.b, b.b).powi(2)).sqrt()
}

/// The variable closest to `c` within `BIND_COLOR_TOLERANCE`.
pub fn nearest_color(vars: &BTreeMap<String, Color>, c: Color) -> Option<&str> {
    vars.iter()
        .map(|(n, v)| (n, color_distance(*v, c)))
        .filter(|(_, d)| *d <= BIND_COLOR_TOLERANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, _)| n.as_str())
}

/// The token closest to `v` within `BIND_SIZE_TOLERANCE`.
pub fn nearest_size(tokens: &[(String, f64)], v: f64) -> Option<&str> {
    tokens
        .iter()
        .map(|(n, t)| (n, (t - v).abs()))
        .filter(|(_, d)| *d <= BIND_SIZE_TOLERANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, _)| n.as_str())
}

/// Px tokens whose name has one of `words`.
fn px_tokens(doc: &Document, words: &[&str]) -> Vec<(String, f64)> {
    doc.tokens
        .iter()
        .filter(|(n, _)| words.iter().any(|w| n.contains(w)))
        .filter_map(|(n, v)| Some((n.clone(), v.trim().strip_suffix("px").unwrap_or(v.trim()).trim().parse().ok()?)))
        .collect()
}

fn bind_tokens(host: &mut dyn Host, tool: &mut Tool, keys: &[String]) -> Outcome {
    let ed = host.editor();
    let (page, roots) = resolve(ed, keys);
    let page_id = ed.pages[page].id.clone();
    let doc = ed.page_doc(page);
    let (spaces, radii) = (px_tokens(doc, &SPACE_WORDS), px_tokens(doc, &RADIUS_WORDS));
    // (field, variable) -> layers, for one bind_variable call each.
    let mut binds: BTreeMap<(&str, String), Vec<String>> = BTreeMap::new();
    let mut fills = Vec::new();
    let (mut colors, mut space, mut radius) = (0, 0, 0);
    let mut left = Vec::new();
    for id in own_layers(doc, &roots) {
        let n = doc.get(id).expect("own layer");
        let key = node_key(ed, page, id);
        let text = n.kind.text().is_some();
        if let Some(Paint::Solid { color, var: None }) = n.style.fills.last()
            && color.a == 255
            && !(text && n.bindings.contains_key("text-color"))
        {
            match nearest_color(&doc.variables, *color) {
                Some(v) => {
                    let prop = if text { "color" } else { "backgroundColor" };
                    fills.push(json!({"nodeIds": [key], "styles": {prop: format!("var(--{v})")}}));
                    colors += 1;
                }
                None => left.push(format!("{} on {}", color.to_hex(), n.name)),
            }
        }
        if let Some(s) = n.style.stroke.filter(|s| s.a == 255 && !n.bindings.contains_key("stroke")) {
            match nearest_color(&doc.variables, s) {
                Some(v) => {
                    binds.entry(("stroke", v.to_string())).or_default().push(key.clone());
                    colors += 1;
                }
                None => left.push(format!("stroke {} on {}", s.to_hex(), n.name)),
            }
        }
        let mut field = |field: &'static str, v: f64, tokens: &[(String, f64)], count: &mut usize| {
            if v <= 0.0 || n.bindings.contains_key(field) {
                return;
            }
            match nearest_size(tokens, v) {
                Some(t) => {
                    binds.entry((field, t.to_string())).or_default().push(key.clone());
                    *count += 1;
                }
                None => left.push(format!("{field} {v} on {}", n.name)),
            }
        };
        if let Some(l) = &n.layout {
            field("gap", l.gap, &spaces, &mut space);
            if l.padding.iter().all(|p| *p == l.padding[0]) {
                field("padding", l.padding[0], &spaces, &mut space);
            } else {
                for (side, p) in ["padding-top", "padding-right", "padding-bottom", "padding-left"].into_iter().zip(l.padding) {
                    field(side, p, &spaces, &mut space);
                }
            }
        }
        if n.style.radii.is_none() {
            field("radius", n.style.corner_radius, &radii, &mut radius);
        }
    }
    if !fills.is_empty() {
        tool(host, "update_styles", json!({"updates": fills}))?;
    }
    for ((field, var), ids) in binds {
        tool(host, "bind_variable", json!({"nodeIds": ids, "field": field, "variable": var, "pageId": page_id}))?;
    }
    let mut parts = Vec::new();
    if colors > 0 {
        parts.push(super::panels::count(colors, "color"));
    }
    if space > 0 {
        parts.push(super::panels::count(space, "space"));
    }
    if radius > 0 {
        parts.push(format!("{radius} {}", if radius == 1 { "radius" } else { "radii" }));
    }
    let changed = !parts.is_empty();
    let mut summary = if changed { format!("Bound {}", parts.join(" \u{b7} ")) } else { "Nothing to bind".into() };
    if !left.is_empty() {
        let more = left.len().saturating_sub(LEFT_SHOWN);
        left.truncate(LEFT_SHOWN);
        if more > 0 {
            left.push(format!("{more} more"));
        }
        let note = format!("no variable near: {}", left.join(", "));
        host.editor().status = note.clone();
        if !changed {
            summary = format!("{summary}; {note}");
        }
    }
    Ok((summary, changed))
}

/// How `create_instance` names the main a hand-made copy matched.
enum MainRef {
    Id(String),
    Key(String),
}

fn use_components(host: &mut dyn Host, tool: &mut Tool, keys: &[String]) -> Outcome {
    let ed = host.editor();
    let (page, roots) = resolve(ed, keys);
    let page_id = ed.pages[page].id.clone();
    let doc = ed.page_doc(page);
    // Same order as `lint::component_mains`: pages, then enabled libraries.
    let mut mains: Vec<(MainRef, &Document, NodeId)> = Vec::new();
    for p in 0..ed.pages.len() {
        let d = ed.page_doc(p);
        mains.extend(d.nodes.values().filter(|n| n.component).map(|n| (MainRef::Id(node_key(ed, p, n.id)), d, n.id)));
    }
    for d in ed.library_docs.values() {
        mains.extend(d.nodes.values().filter(|n| n.component).filter_map(|n| Some((MainRef::Key(n.component_key.clone()?), d, n.id))));
    }
    // Each hand-made copy goes to the first main it matches.
    let mut hits: Vec<(NodeId, usize)> = Vec::new();
    for (i, (_, d, m)) in mains.iter().enumerate() {
        for issue in crate::mcp::lint::not_an_instance(doc, &roots, &[(*d, *m)]) {
            if !hits.iter().any(|h| h.0 == issue.node) {
                hits.push((issue.node, i));
            }
        }
    }
    // A copy inside another copy goes with its outer one.
    let outer: Vec<NodeId> = hits.iter().map(|h| h.0).collect();
    hits.retain(|(id, _)| !doc.ancestors(*id).iter().any(|a| outer.contains(a)));
    struct Plan {
        copy: String,
        args: Value,
        /// left/top when the parent does not lay it out.
        place: Option<(f64, f64)>,
        texts: Vec<String>,
    }
    let plans: Vec<Plan> = hits
        .iter()
        .map(|(id, i)| {
            let n = doc.get(*id).expect("hit");
            let mut args = json!({"name": n.name, "pageId": page_id});
            match &mains[*i].0 {
                MainRef::Id(k) => args["componentId"] = json!(k),
                MainRef::Key(k) => args["componentKey"] = json!(k),
            }
            let parent = n.parent.and_then(|p| doc.get(p));
            let place = match parent {
                None => {
                    args["x"] = json!(n.rect.x);
                    args["y"] = json!(n.rect.y);
                    None
                }
                Some(p) => {
                    args["parentId"] = json!(node_key(ed, page, p.id));
                    args["index"] = json!(p.children.iter().position(|c| c == id).unwrap_or(0));
                    (p.layout.is_none() || n.layout_child.absolute).then_some((n.rect.x, n.rect.y))
                }
            };
            let texts = doc.subtree(*id).iter().filter_map(|t| doc.get(*t)?.kind.text().cloned()).collect();
            Plan { copy: node_key(ed, page, *id), args, place, texts }
        })
        .collect();
    for p in &plans {
        let made = tool(host, "create_instance", p.args.clone())?;
        let inst = made["id"].as_str().unwrap_or_default().to_string();
        let updates: Vec<Value> = made["textLayers"]
            .as_array()
            .into_iter()
            .flatten()
            .zip(&p.texts)
            .filter(|(l, t)| l["text"].as_str() != Some(t.as_str()))
            .map(|(l, t)| json!({"nodeId": l["id"], "textContent": t}))
            .collect();
        if !updates.is_empty() {
            tool(host, "set_text_content", json!({"updates": updates}))?;
        }
        if let Some((x, y)) = p.place {
            tool(host, "update_styles", json!({"updates": [{"nodeIds": [inst], "styles": {"left": format!("{x}px"), "top": format!("{y}px")}}]}))?;
        }
        tool(host, "delete_nodes", json!({"nodeIds": [p.copy]}))?;
    }
    Ok(match plans.len() {
        0 => ("No hand-made copies of components".into(), false),
        n => (format!("Replaced {} with instances", super::panels::count(n, "copy").replace("copys", "copies")), true),
    })
}

/// A name the app or an import gave by default: `Frame 12`, `Text`, `Rectangle 3`.
fn default_name(name: &str) -> bool {
    crate::mcp::lint::is_default_name(name) || name.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ') == "Text"
}

/// A name from the layer's text (or its first text inside) and, for frames, its role.
pub fn suggest_name(doc: &Document, id: NodeId) -> Option<String> {
    let n = doc.get(id)?;
    let text = doc.subtree(id).into_iter().find_map(|t| doc.get(t)?.kind.text().cloned())?;
    let line: String = text.lines().map(str::trim).find(|l| !l.is_empty())?.chars().take(NAME_CHARS).collect();
    if n.kind.text().is_some() {
        return Some(line);
    }
    let role = match n.layout.map(|l| l.direction) {
        Some(Direction::Horizontal) => "Row",
        Some(Direction::Vertical) => "Stack",
        None => "Group",
    };
    Some(format!("{line} {role}"))
}

fn rename_layers(host: &mut dyn Host, tool: &mut Tool, keys: &[String]) -> Outcome {
    let ed = host.editor();
    let (page, roots) = resolve(ed, keys);
    let doc = ed.page_doc(page);
    let updates: Vec<Value> = own_layers(doc, &roots)
        .into_iter()
        .filter(|id| doc.get(*id).is_some_and(|n| default_name(&n.name)))
        .filter_map(|id| Some(json!({"nodeId": node_key(ed, page, id), "name": suggest_name(doc, id)?})))
        .collect();
    if updates.is_empty() {
        return Ok(("No default layer names".into(), false));
    }
    let n = tool(host, "rename_nodes", json!({"updates": updates}))?["renamed"].as_u64().unwrap_or(0);
    Ok((format!("Renamed {}", super::panels::count(n as usize, "layer")), true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::{self, SessionHost};

    fn host() -> (SessionHost, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("papercraft-actions-{:016x}", fastrand::u64(..)));
        let mut h = SessionHost::new(crate::project::Store::open(&dir).unwrap());
        h.session.create(&mut h.ed, "Actions").unwrap();
        (h, dir)
    }

    fn call(h: &mut SessionHost, name: &str, args: Value) -> Value {
        match exec::call(h, name, &args).unwrap_or_else(|e| panic!("{name}: {e}")).remove(0) {
            Out::Text(t) => serde_json::from_str(&t).unwrap(),
            _ => Value::Null,
        }
    }

    /// Runs `act` on `key` and checks it made one undo step and one Actions run.
    fn act(h: &mut SessionHost, act: Act, key: &str) -> crate::runs::Run {
        let mut t = Tracker::default();
        let undo = h.ed.history.undo_len();
        run(h, &mut t, act, &[key.to_string()], 1_000);
        assert_eq!(h.ed.history.undo_len(), undo + 1, "one undo step");
        let r = t.take_finished().expect("one run");
        assert!(t.take_finished().is_none() && t.open_runs().next().is_none(), "exactly one run, closed");
        assert_eq!(r.client, ACTIONS_CLIENT);
        r
    }

    fn lint(h: &mut SessionHost, key: &str, rule: &str) -> usize {
        call(h, "lint_design", json!({"nodeId": key}))["issues"].as_array().unwrap().iter().filter(|i| i["rule"] == rule).count()
    }

    #[test]
    fn nearest_matching_respects_the_tolerance() {
        let vars: BTreeMap<String, Color> = [("ink".to_string(), Color::rgb(20, 20, 20)), ("brand".to_string(), Color::rgb(0x3b, 0x82, 0xf6))].into();
        assert_eq!(nearest_color(&vars, Color::rgb(0x3c, 0x80, 0xf0)), Some("brand"));
        assert_eq!(nearest_color(&vars, Color::rgb(200, 0, 0)), None);
        let toks = vec![("space-2".to_string(), 8.0), ("space-4".to_string(), 16.0)];
        assert_eq!(nearest_size(&toks, 15.0), Some("space-4"));
        assert_eq!(nearest_size(&toks, 12.0), None);
    }

    #[test]
    fn bind_to_tokens_binds_near_values_in_one_step_and_run() {
        let (mut h, dir) = host();
        call(&mut h, "set_variables", json!({"variables": {"brand": "#3b82f6", "ink": "#111111"}}));
        call(&mut h, "set_tokens", json!({"tokens": {"space-4": "16px", "radius-md": "8px"}}));
        let ab = call(&mut h, "create_artboard", json!({"name": "Card", "styles": {"width": "300px", "height": "200px", "display": "flex", "gap": "15px", "padding": "16px", "borderRadius": "9px", "backgroundColor": "#3c80f0"}}));
        let key = ab["id"].as_str().unwrap().to_string();
        call(&mut h, "write_html", json!({"targetNodeId": key, "mode": "insert-children", "html": "<div style=\"color:#121212;font-size:16px\">Hi</div><div style=\"color:#ff0000;font-size:16px\">Red</div>"}));
        assert_eq!(lint(&mut h, &key, "hard-coded-color"), 3, "near blue, near ink, red");
        let r = act(&mut h, Act::BindTokens, &key);
        assert_eq!(r.title, "Bound 2 colors \u{b7} 2 spaces \u{b7} 1 radius");
        assert_eq!(lint(&mut h, &key, "hard-coded-color"), 1, "only the red is left");
        let (page, id) = (h.ed.active_page, resolve(&h.ed, std::slice::from_ref(&key)).1[0]);
        let n = h.ed.page_doc(page).get(id).unwrap();
        assert_eq!(n.style.fills.last().and_then(Paint::var), Some("brand"));
        assert_eq!(n.bindings.get("gap").map(String::as_str), Some("space-4"));
        assert_eq!(n.layout.unwrap().gap, 16.0, "the value follows the token");
        assert_eq!(n.bindings.get("radius").map(String::as_str), Some("radius-md"));
        assert!(h.ed.status.contains("#FF0000") || h.ed.status.to_lowercase().contains("#ff0000"), "the red is listed: {}", h.ed.status);
        h.ed.undo();
        let n = h.ed.page_doc(page).get(id).unwrap();
        assert!(n.bindings.is_empty() && n.style.fills.last().and_then(Paint::var).is_none(), "one undo takes it all back");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn use_components_swaps_copies_for_instances_and_keeps_their_text() {
        let (mut h, dir) = host();
        let button = |name: &str, label: &str| format!("<div layer-name=\"{name}\" style=\"display:flex;padding:12px 16px\"><div style=\"font-size:16px\">{label}</div></div>");
        let ab = call(&mut h, "create_artboard", json!({"name": "Screen", "styles": {"width": "400px", "height": "400px", "display": "flex", "gap": "8px"}}));
        let key = ab["id"].as_str().unwrap().to_string();
        let r = call(&mut h, "write_html", json!({"targetNodeId": key, "mode": "insert-children", "html": button("Button", "Save")}));
        call(&mut h, "create_component", json!({"nodeId": r["createdNodeIds"][0], "name": "Button"}));
        call(&mut h, "write_html", json!({"targetNodeId": key, "mode": "insert-children", "html": button("Send copy", "Send")}));
        assert_eq!(lint(&mut h, &key, "not-an-instance"), 1);
        let r = act(&mut h, Act::UseComponents, &key);
        assert_eq!(r.title, "Replaced 1 copy with instances");
        assert_eq!(lint(&mut h, &key, "not-an-instance"), 0);
        let (page, id) = (h.ed.active_page, resolve(&h.ed, std::slice::from_ref(&key)).1[0]);
        let doc = h.ed.page_doc(page);
        let kids = &doc.get(id).unwrap().children;
        assert_eq!(kids.len(), 2);
        let inst = doc.get(kids[1]).unwrap();
        assert!(inst.instance.is_some(), "in the copy's place");
        assert_eq!(inst.name, "Send copy");
        let text = doc.subtree(inst.id).into_iter().find_map(|t| doc.get(t)?.kind.text().cloned());
        assert_eq!(text.as_deref(), Some("Send"), "the copy's text is kept as an override");
        h.ed.undo();
        assert_eq!(lint(&mut h, &key, "not-an-instance"), 1, "one undo brings the copy back");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rename_layers_names_default_layers_from_their_text() {
        let (mut h, dir) = host();
        let ab = call(&mut h, "create_artboard", json!({"name": "Screen", "styles": {"width": "400px", "height": "400px", "display": "flex", "flexDirection": "column"}}));
        let key = ab["id"].as_str().unwrap().to_string();
        call(&mut h, "write_html", json!({"targetNodeId": key, "mode": "insert-children", "html": "<div layer-name=\"Frame 12\" style=\"display:flex\"><div layer-name=\"Text\">Sign in</div></div><div layer-name=\"Kept\">x</div>"}));
        let r = act(&mut h, Act::RenameLayers, &key);
        assert_eq!(r.title, "Renamed 2 layers");
        let (page, id) = (h.ed.active_page, resolve(&h.ed, std::slice::from_ref(&key)).1[0]);
        let doc = h.ed.page_doc(page);
        let names: Vec<&str> = doc.subtree(id).iter().skip(1).map(|i| doc.get(*i).unwrap().name.as_str()).collect();
        assert_eq!(names, ["Sign in Row", "Kept", "Sign in"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn fix_lint_here_is_one_run() {
        let (mut h, dir) = host();
        let ab = call(&mut h, "create_artboard", json!({"name": "Screen", "styles": {"width": "400px", "height": "400px", "display": "flex", "gap": "10px", "padding": "6px"}}));
        let key = ab["id"].as_str().unwrap().to_string();
        let r = act(&mut h, Act::FixLint, &key);
        assert_eq!(r.title, "Fixed 1 layer");
        assert_eq!(lint(&mut h, &key, "spacing-grid"), 0);
        let mut t = Tracker::default();
        run(&mut h, &mut t, Act::FixLint, &[key], 2_000);
        assert!(t.take_finished().is_none(), "nothing left to fix: no run");
        assert_eq!(h.ed.status, "Nothing to fix");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn deterministic_actions_queue_and_intents_fill_the_composer() {
        use crate::geom::Rect;
        use crate::model::{Node, NodeKind};
        let mut e = Editor::default();
        let mut st = UiState::default();
        dispatch(&mut e, &mut st, Act::BindTokens);
        assert!(st.pending_action.is_none() && e.status == "Select a layer first");
        let f = e.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: true }, Rect::new(0.0, 0.0, 100.0, 100.0)), None);
        e.edit("name", |tx| tx.node_mut(f).unwrap().name = "Card".into());
        e.select(vec![f]);
        dispatch(&mut e, &mut st, Act::BindTokens);
        assert_eq!(st.pending_action, Some((Act::BindTokens, vec![node_key(&e, 0, f)])));
        dispatch(&mut e, &mut st, Act::Prompt(0));
        assert_eq!(st.chat.draft, ACTION_PROMPTS[0].1);
        assert_eq!(st.chat.sels.len(), 1);
        assert_eq!(st.chat.sels[0].keys, [node_key(&e, 0, f)]);
        assert!(st.chat.send_draft);
        assert_eq!(items().iter().filter(|(a, _)| is_intent(*a)).count(), ACTION_PROMPTS.len() + 1);
    }

    #[cfg(unix)]
    #[test]
    fn an_intent_is_sent_with_its_prompt_and_chip() {
        use crate::geom::Rect;
        use crate::model::{Node, NodeKind};
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("papercraft-actions-send-{:016x}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("claude");
        std::fs::write(&bin, "#!/bin/sh\ncat >/dev/null\necho '{\"type\":\"result\",\"session_id\":\"s\"}'\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut e = Editor::default();
        let f = e.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: true }, Rect::new(0.0, 0.0, 100.0, 100.0)), None);
        e.edit("name", |tx| tx.node_mut(f).unwrap().name = "Card".into());
        e.select(vec![f]);
        let mut st = UiState::default();
        st.chat.project = Some(dir.clone());
        st.chat.prefs.claude_path = Some(bin.display().to_string());
        st.chat.mcp_url = "http://127.0.0.1:1/mcp".into();
        dispatch(&mut e, &mut st, Act::Prompt(1));
        let ctx = egui::Context::default();
        super::super::chat::send_draft(&ctx, &e, &mut st.chat);
        assert!(st.chat.send_draft, "waits for the agent scan");
        st.chat.detect = Some(Default::default());
        super::super::chat::send_draft(&ctx, &e, &mut st.chat);
        assert!(!st.chat.send_draft && st.chat.error.is_none(), "{:?}", st.chat.error);
        let sent = &st.chat.chat.as_ref().unwrap().messages[0].text;
        let (context, text) = super::super::chat_msgs::split_context(sent);
        assert_eq!(text, ACTION_PROMPTS[1].1);
        assert_eq!(context, [format!("[Selection] Card ({})", node_key(&e, 0, f))]);
        assert!(st.chat.draft.is_empty() && st.chat.sels.is_empty());
        st.chat.stop();
        std::fs::remove_dir_all(dir).ok();
    }
}
