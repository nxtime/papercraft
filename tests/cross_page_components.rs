//! Components used across pages through the MCP tools, and variable binding.

use papercraft::mcp::{Out, exec};
use papercraft::model::NodeId;
use papercraft::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store =
        Store::open(std::env::temp_dir().join(format!("papercraft-xp-{:016x}", fastrand::u64(..))))
            .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Cross").unwrap();
    h
}

fn try_call(h: &mut exec::SessionHost, name: &str, args: Value) -> Result<Value, String> {
    exec::call(h, name, &args).map(|o| match o.into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    })
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    try_call(h, name, args).unwrap_or_else(|e| panic!("{name} failed: {e}"))
}

fn num(id: &Value) -> NodeId {
    NodeId(id.as_str().unwrap().rsplit(':').next().unwrap().parse().unwrap())
}

/// Page A with component "Pill", page B empty. Returns (A id, B id, component id).
fn setup(h: &mut exec::SessionHost) -> (Value, Value, Value) {
    let a = call(h, "get_basic_info", json!({}))["pages"][0]["id"].clone();
    let b = call(h, "create_page", json!({"name": "B"}))["pageId"].clone();
    let ab = call(h, "create_artboard", json!({"pageId": a, "name": "Pill",
        "styles": {"width": "100px", "height": "40px", "background": "#ff0000"}}));
    call(h, "create_component", json!({"pageId": a, "nodeId": ab["id"]}));
    (a, b, ab["id"].clone())
}

fn bg_red(h: &exec::SessionHost, page: usize, id: &Value) -> bool {
    use papercraft::model::Paint;
    matches!(h.ed.page_doc(page).get(num(id)).unwrap().style.fills.first(),
        Some(Paint::Solid { color, .. }) if color.r == 255 && color.b == 0)
}

#[test]
fn instance_on_another_page_follows_main_and_survives_deletion() {
    let mut h = host();
    let (a, b, comp) = setup(&mut h);
    let inst = call(&mut h, "create_instance", json!({"pageId": b, "componentId": comp}));
    assert!(bg_red(&h, 1, &inst["id"]));
    call(&mut h, "update_styles", json!({"pageId": a, "updates": [{"nodeIds": [comp], "styles": {"background": "#0000ff"}}]}));
    assert!(!bg_red(&h, 1, &inst["id"]), "B follows A");
    let comps = call(&mut h, "get_components", json!({}));
    let c = &comps["components"][0];
    assert_eq!(c["uses"], 1, "{comps}");
    assert_eq!(c["pageName"], "Pill".replace("Pill", &h.ed.pages[0].name));
    assert!(c["componentKey"].is_string());
    call(&mut h, "delete_nodes", json!({"pageId": a, "nodeIds": [comp]}));
    assert!(h.ed.page_doc(1).get(num(&inst["id"])).is_some());
    assert_eq!(h.ed.missing_mains(1), vec![num(&inst["id"])]);
}

#[test]
fn create_and_swap_by_component_key_across_pages() {
    let mut h = host();
    let (a, b, comp) = setup(&mut h);
    let key = call(&mut h, "get_components", json!({}))["components"][0]["componentKey"].clone();
    let i1 = call(&mut h, "create_instance", json!({"pageId": b, "componentKey": key}));
    // A second component on A; swap B's instance to it by key.
    let ab = call(&mut h, "create_artboard", json!({"pageId": a, "name": "Other",
        "styles": {"width": "10px", "height": "10px", "background": "#00ff00"}}));
    call(&mut h, "create_component", json!({"pageId": a, "nodeId": ab["id"]}));
    let comps = call(&mut h, "get_components", json!({"pageId": a}));
    let other = comps["components"].as_array().unwrap().iter()
        .find(|c| c["id"] == ab["id"]).unwrap()["componentKey"].clone();
    assert_ne!(other, key);
    call(&mut h, "swap_instance", json!({"pageId": b, "nodeId": i1["id"], "componentKey": other}));
    assert!(!bg_red(&h, 1, &i1["id"]));
    assert!(try_call(&mut h, "create_instance", json!({"pageId": b, "componentKey": "nope"})).is_err());
    let _ = comp;
}

#[test]
fn bind_variable_binds_and_detaches() {
    let mut h = host();
    let (_a, _b, comp) = setup(&mut h);
    call(&mut h, "set_tokens", json!({"tokens": {"space-md": "24px"}}));
    let child = call(&mut h, "create_artboard", json!({"name": "F",
        "styles": {"width": "10px", "height": "10px", "display": "flex"}}));
    call(&mut h, "bind_variable", json!({"nodeIds": [child["id"]], "field": "gap", "variable": "space-md"}));
    let n = h.ed.doc.get(num(&child["id"])).unwrap();
    assert_eq!(n.bindings["gap"], "space-md");
    call(&mut h, "bind_variable", json!({"nodeIds": [child["id"]], "field": "gap", "variable": null}));
    assert!(h.ed.doc.get(num(&child["id"])).unwrap().bindings.is_empty());
    assert!(try_call(&mut h, "bind_variable", json!({"nodeIds": [child["id"]], "field": "gap", "variable": "zzz"})).is_err());
    let _ = comp;
}

#[test]
fn duplicating_keeps_cross_page_links_and_gives_copied_mains_fresh_keys() {
    let mut h = host();
    let (_a, b, comp) = setup(&mut h);
    let inst = call(&mut h, "create_instance", json!({"pageId": b, "componentId": comp}));
    let d = call(&mut h, "duplicate_nodes", json!({"pageId": b, "nodes": [{"id": inst["id"]}]}));
    let copy = num(&d["duplicates"][0]["newId"]);
    let src = h.ed.page_doc(1).get(num(&inst["id"])).unwrap().instance.clone().unwrap();
    let l = h.ed.page_doc(1).get(copy).unwrap().instance.clone().expect("link kept");
    assert_eq!((l.main, l.source.clone()), (src.main, src.source), "cross-page link untouched");

    let d = call(&mut h, "duplicate_nodes", json!({"nodes": [{"id": comp}]}));
    let _ = d;
    let keys: Vec<String> = h.ed.page_doc(0).nodes.values().filter_map(|n| n.component_key.clone()).collect();
    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0], keys[1], "copied main gets its own key");
}

#[test]
fn write_html_instance_of_another_pages_main_links_instead_of_copying() {
    use papercraft::model::Source;
    let mut h = host();
    let (a, b, comp) = setup(&mut h);
    let screen = call(&mut h, "create_artboard", json!({"pageId": b, "name": "Screen",
        "styles": {"width": "300px", "height": "200px", "display": "flex"}}));
    let before = h.ed.page_doc(1).nodes.len();
    call(&mut h, "write_html", json!({"pageId": b, "targetNodeId": screen["id"], "mode": "insert-children",
        "html": "<x-paper-instance component=\"Pill\" layer-name=\"Pill / Here\"/>"}));
    let doc = h.ed.page_doc(1);
    assert!(doc.nodes.values().all(|n| !n.component), "no main copied onto B");
    assert_eq!(doc.nodes.len(), before + 1, "only the instance root was added");
    let inst = doc.nodes.values().find(|n| n.name == "Pill / Here").unwrap();
    let link = inst.instance.as_ref().unwrap();
    assert_eq!(link.main, num(&comp));
    assert_eq!(link.source, Some(Source::Page(a.as_str().unwrap().to_string())));
    let key = Value::String(format!("{}:{}", &b.as_str().unwrap()[..6], inst.id.0));
    assert!(bg_red(&h, 1, &key));
    call(&mut h, "update_styles", json!({"pageId": a, "updates": [{"nodeIds": [comp], "styles": {"background": "#0000ff"}}]}));
    assert!(!bg_red(&h, 1, &key), "the instance follows the main on A");
}

#[test]
fn moving_mains_to_another_page_keeps_every_instance_linked() {
    // Issue #10: a frame of mains moved to a Design System page kept no links.
    let mut h = host();
    let (a, b, comp) = setup(&mut h);
    let c = call(&mut h, "create_page", json!({"name": "C"}))["pageId"].clone();
    let key = call(&mut h, "get_components", json!({}))["components"][0]["componentKey"].clone();
    let same = call(&mut h, "create_instance", json!({"pageId": a, "componentId": comp}));
    let other = call(&mut h, "create_instance", json!({"pageId": c, "componentId": comp}));
    call(&mut h, "move_nodes", json!({"pageId": a, "moves": [{"nodeId": comp, "parentId": "root", "toPageId": b}]}));
    let comps = call(&mut h, "get_components", json!({"pageId": b}));
    let moved = &comps["components"][0];
    assert_eq!(moved["componentKey"], key, "the key survives the move: {comps}");
    assert_eq!(moved["uses"], 2, "{comps}");
    call(&mut h, "update_styles", json!({"pageId": b, "updates": [{"nodeIds": [moved["id"]], "styles": {"background": "#0000ff"}}]}));
    assert!(!bg_red(&h, 0, &same["id"]), "the instance left on A follows the moved main");
    assert!(!bg_red(&h, 2, &other["id"]), "the instance on C follows it too");
}

#[test]
fn rename_file_renames_the_open_project_and_keeps_it_open() {
    let mut h = host();
    let r = call(&mut h, "rename_file", json!({"name": "Renamed"}));
    let dir = std::path::PathBuf::from(r["path"].as_str().unwrap());
    assert!(dir.ends_with("Renamed") && dir.is_dir(), "{r}");
    assert_eq!(h.session.project_name(), "Renamed");
    call(&mut h, "create_artboard", json!({"name": "After", "styles": {"width": "10px", "height": "10px"}}));
    call(&mut h, "finish_working_on_nodes", json!({}));
    assert!(dir.is_dir(), "saves go to the renamed folder");
    assert!(try_call(&mut h, "rename_file", json!({"name": "  "})).is_err());
}

/// Badge / Green and Badge / Amber mains, a Row main holding a Green badge, and a Screen with
/// one Row instance. Returns (row main id, screen id, the nested badge inside the row instance).
fn rows(h: &mut exec::SessionHost) -> (Value, Value, NodeId) {
    for (name, bg, label) in [("Badge / Green", "#00ff00", "OK"), ("Badge / Amber", "#ffbf00", "-")] {
        let ab = call(h, "create_artboard", json!({"name": name, "styles": {"width": "60px", "height": "20px", "background": bg, "display": "flex"}}));
        call(h, "write_html", json!({"targetNodeId": ab["id"], "html": format!("<span layer-name=\"Label\">{label}</span>")}));
        call(h, "create_component", json!({"nodeId": ab["id"]}));
    }
    let row = call(h, "create_artboard", json!({"name": "Row", "styles": {"width": "200px", "height": "40px", "display": "flex", "gap": "8px"}}));
    call(h, "write_html", json!({"targetNodeId": row["id"], "html": "<x-paper-instance component=\"Badge / Green\"/>"}));
    call(h, "create_component", json!({"nodeId": row["id"]}));
    let screen = call(h, "create_artboard", json!({"name": "Screen", "styles": {"width": "300px", "height": "200px", "display": "flex"}}));
    call(h, "write_html", json!({"targetNodeId": screen["id"], "html": "<x-paper-instance component=\"Row\"/>"}));
    let row_inst = h.ed.doc.get(num(&screen["id"])).unwrap().children[0];
    let badge = h.ed.doc.get(row_inst).unwrap().children[0];
    (row["id"].clone(), screen["id"].clone(), badge)
}

fn badge_state(h: &exec::SessionHost, screen: &Value) -> (String, String) {
    use papercraft::model::Paint;
    let doc = &h.ed.doc;
    let row_inst = doc.get(num(screen)).unwrap().children[0];
    let badge = doc.get(doc.get(row_inst).unwrap().children[0]).unwrap();
    let label = doc.get(badge.children[0]).unwrap().kind.text().map(|t| t.to_string()).unwrap_or_default();
    let fill = match badge.style.fills.first() {
        Some(Paint::Solid { color, .. }) => color.to_hex(),
        _ => String::new(),
    };
    (label, fill)
}

#[test]
fn a_nested_instance_replaced_inside_an_instance_survives_edits_to_the_outer_main() {
    // Issue #11.
    let mut h = host();
    let (row, screen, badge) = rows(&mut h);
    let key = format!("{}:{}", &h.ed.pages[0].id[..6], badge.0);
    call(&mut h, "write_html", json!({"mode": "replace", "targetNodeId": key,
        "html": "<x-paper-instance component=\"Badge / Amber\" overrides='{\"Label\":\"Late\"}'/>"}));
    assert_eq!(badge_state(&h, &screen), ("Late".into(), "#ffbf00".into()));
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [row], "styles": {"gap": "0px"}}]}));
    assert_eq!(badge_state(&h, &screen), ("Late".into(), "#ffbf00".into()), "the swap survives the outer main's edit");
    // Copies of the screen keep it too.
    let copy = call(&mut h, "create_artboard", json!({"name": "Copy", "styles": {"width": "300px", "height": "200px", "display": "flex"}}));
    let row_inst = h.ed.doc.get(num(&screen)).unwrap().children[0];
    let clone = format!("<x-paper-clone node-id=\"{}:{}\"/>", &h.ed.pages[0].id[..6], row_inst.0);
    call(&mut h, "write_html", json!({"targetNodeId": copy["id"], "html": clone}));
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [row], "styles": {"gap": "4px"}}]}));
    assert_eq!(badge_state(&h, &copy["id"]), ("Late".into(), "#ffbf00".into()));
}

#[test]
fn swap_instance_swaps_a_nested_instance_and_keeps_it() {
    let mut h = host();
    let (row, screen, badge) = rows(&mut h);
    let key = format!("{}:{}", &h.ed.pages[0].id[..6], badge.0);
    let comps = call(&mut h, "get_components", json!({}));
    let amber = comps["components"].as_array().unwrap().iter().find(|c| c["name"] == "Badge / Amber").unwrap()["id"].clone();
    call(&mut h, "swap_instance", json!({"nodeId": key, "componentId": amber}));
    assert_eq!(badge_state(&h, &screen).1, "#ffbf00");
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [row], "styles": {"gap": "0px"}}]}));
    assert_eq!(badge_state(&h, &screen).1, "#ffbf00", "kept after the outer main changed");
    let comps = call(&mut h, "get_components", json!({}));
    let uses = |name: &str| comps["components"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()["uses"].clone();
    assert_eq!(uses("Badge / Amber"), 1, "the swapped badge counts for Amber: {comps}");
}

#[test]
fn computed_styles_name_the_variable_a_text_colour_is_bound_to() {
    let mut h = host();
    call(&mut h, "set_variables", json!({"variables": {"text": "#cdd6f4"}}));
    let ab = call(&mut h, "create_artboard", json!({"name": "T", "styles": {"width": "100px", "height": "40px", "display": "flex"}}));
    let t = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<span style=\"color: var(--text)\">Hi</span>"}))["createdNodeIds"][0].clone();
    let s = call(&mut h, "get_computed_styles", json!({"nodeIds": [t]}));
    assert_eq!(s[t.as_str().unwrap()]["color"], "var(--text)", "{s}");
}

#[test]
fn an_instance_with_its_own_text_keeps_its_own_width_when_the_main_text_changes() {
    // Issue #17.
    let mut h = host();
    let btn = call(&mut h, "create_artboard", json!({"name": "Btn", "styles": {"width": "fit-content", "height": "fit-content", "display": "flex", "padding": "0px 16px"}}));
    call(&mut h, "write_html", json!({"targetNodeId": btn["id"], "html": "<span layer-name=\"Label\" style=\"font-size:14px;font-weight:400\">Button</span>"}));
    call(&mut h, "create_component", json!({"nodeId": btn["id"]}));
    let screen = call(&mut h, "create_artboard", json!({"name": "S", "styles": {"width": "400px", "height": "100px", "display": "flex"}}));
    call(&mut h, "write_html", json!({"targetNodeId": screen["id"], "html": "<x-paper-instance component=\"Btn\" overrides='{\"Label\":\"Confirm order\"}'/>"}));
    let inst = h.ed.doc.get(num(&screen["id"])).unwrap().children[0];
    let label_main = h.ed.doc.get(num(&btn["id"])).unwrap().children[0];
    let label_key = format!("{}:{}", &h.ed.pages[0].id[..6], label_main.0);
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [label_key], "styles": {"fontWeight": "700"}}]}));
    let doc = &h.ed.doc;
    let root = doc.get(inst).unwrap();
    let text = doc.get(root.children[0]).unwrap();
    let main_w = doc.get(num(&btn["id"])).unwrap().rect.w;
    assert!(text.rect.w + 32.0 <= root.rect.w + 0.5, "the label fits its button: {} in {}", text.rect.w, root.rect.w);
    assert!(root.rect.w > main_w + 10.0, "\"Confirm order\" is wider than \"Button\": {} vs {main_w}", root.rect.w);
}

#[test]
fn an_svg_viewbox_is_fitted_to_its_width_and_height() {
    // Issue #14.
    let mut h = host();
    let row = call(&mut h, "create_artboard", json!({"name": "Row", "styles": {"width": "300px", "height": "40px", "display": "flex"}}));
    let made = call(&mut h, "write_html", json!({"targetNodeId": row["id"],
        "html": "<svg width=\"100\" height=\"20\" viewBox=\"0 0 200 40\"><path d=\"M0 20H200\" stroke=\"#000\" stroke-width=\"2\"/></svg><div>Text</div>"}));
    let svg = num(&made["createdNodeIds"][0]);
    let doc = &h.ed.doc;
    let frame = doc.world_rect(svg).unwrap();
    assert_eq!((frame.w, frame.h), (100.0, 20.0));
    for id in doc.subtree(svg).into_iter().skip(1) {
        let r = doc.world_rect(id).unwrap();
        assert!(r.x >= frame.x - 1.5 && r.x + r.w <= frame.x + frame.w + 1.5, "artwork {r:?} stays inside {frame:?}");
    }
}

#[test]
fn lint_binds_hard_coded_borders_so_dark_mode_reaches_them() {
    // Issue #18.
    let mut h = host();
    call(&mut h, "set_variables", json!({"mode": "Light", "variables": {"border": "#e2e6df"}}));
    call(&mut h, "set_variables", json!({"mode": "Dark", "variables": {"border": "#26302b"}}));
    let ab = call(&mut h, "create_artboard", json!({"name": "A", "styles": {"width": "200px", "height": "100px", "display": "flex"}}));
    let d = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<div style=\"border:1px solid #e2e6df;width:100px;height:40px\"></div>"}))["createdNodeIds"][0].clone();
    let r = call(&mut h, "lint_design", json!({}));
    assert!(r["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "hard-coded-color" && i["node"] == d), "{r}");
    call(&mut h, "lint_design", json!({"fix": true}));
    call(&mut h, "set_variable_mode", json!({"mode": "Dark"}));
    let s = h.ed.doc.get(num(&d)).unwrap().style.stroke.unwrap();
    assert_eq!(s.to_hex(), "#26302b");
}

#[test]
fn a_border_written_with_a_variable_stays_bound_to_it() {
    // Issue #20.
    let mut h = host();
    call(&mut h, "set_variables", json!({"mode": "Light", "variables": {"border": "#cccccc"}}));
    call(&mut h, "set_variables", json!({"mode": "Dark", "variables": {"border": "#333333"}}));
    let ab = call(&mut h, "create_artboard", json!({"name": "A", "styles": {"width": "200px", "height": "100px", "display": "flex"}}));
    let d = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<div style=\"width:100px;height:40px;border:1px solid #ccc\"></div>"}))["createdNodeIds"][0].clone();
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [d], "styles": {"border": "1px solid var(--border)"}}]}));
    let s = call(&mut h, "get_computed_styles", json!({"nodeIds": [d]}));
    assert_eq!(s[d.as_str().unwrap()]["border"], "1px solid var(--border)", "{s}");
    call(&mut h, "set_variable_mode", json!({"mode": "Dark"}));
    assert_eq!(h.ed.doc.get(num(&d)).unwrap().style.stroke.unwrap().to_hex(), "#333333");
    // A literal border unbinds it again.
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [d], "styles": {"border": "1px solid #ff0000"}}]}));
    assert!(!h.ed.doc.get(num(&d)).unwrap().bindings.contains_key("stroke"));
}

#[test]
fn a_fit_content_artboard_grows_around_a_growing_child() {
    // Issue #21.
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Phone", "styles": {"width": "390px", "height": "fit-content", "minHeight": "844px", "display": "flex", "flexDirection": "column"}}));
    call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<div style=\"display:flex;flex-direction:column;flex-grow:1\"><div style=\"height:1200px;width:100px\"></div></div>"}));
    let doc = &h.ed.doc;
    let a = doc.get(num(&ab["id"])).unwrap();
    let k = doc.get(a.children[0]).unwrap();
    assert_eq!((a.rect.h, k.rect.h), (1200.0, 1200.0));
    // Short content: the screen keeps its min height and the growing area pins the footer down.
    let ab = call(&mut h, "create_artboard", json!({"name": "Short", "styles": {"width": "390px", "height": "fit-content", "minHeight": "844px", "display": "flex", "flexDirection": "column"}}));
    call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<div style=\"display:flex;flex-direction:column;flex-grow:1\"><div style=\"height:100px;width:100px\"></div></div><div style=\"height:60px;width:390px\"></div>"}));
    let doc = &h.ed.doc;
    let a = doc.get(num(&ab["id"])).unwrap();
    let (body, footer) = (doc.get(a.children[0]).unwrap(), doc.get(a.children[1]).unwrap());
    assert_eq!((a.rect.h, body.rect.h, footer.rect.y), (844.0, 784.0, 784.0));
}

#[test]
fn a_clipping_frame_draws_its_border_over_its_filled_children() {
    // Issue #19.
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "A", "styles": {"width": "400px", "height": "200px", "display": "flex"}}));
    let card = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<div style=\"display:flex;flex-direction:column;border:1px solid #ccc;border-radius:14px;overflow:hidden;width:300px\"><div style=\"align-self:stretch;height:40px;background:#eee\"></div><div style=\"align-self:stretch;height:40px;background:#fff\"></div></div>"}))["createdNodeIds"][0].clone();
    let card = num(&card);
    let items = papercraft::render::scene::SceneCache::default().items(&h.ed.doc, None, 1.0, None);
    let order: Vec<NodeId> = items.iter().map(|i| i.id).collect();
    let kids = h.ed.doc.get(card).unwrap().children.clone();
    let last_kid = order.iter().rposition(|i| kids.contains(i)).unwrap();
    assert_eq!(order.iter().rposition(|i| *i == card), Some(last_kid + 1), "the border comes after the rows: {order:?}");
}

#[test]
fn a_scrolling_screen_keeps_its_size_and_the_wheel_scrolls_its_overflow() {
    use papercraft::editor::InputEvent;
    use papercraft::geom::v2;
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Phone", "styles": {"width": "390px", "height": "fit-content", "minHeight": "844px", "display": "flex", "flexDirection": "column"}}));
    let kid = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<div style=\"height:1200px;width:390px;background:#eee\"></div>"}))["createdNodeIds"][0].clone();
    let (ab, kid) = (num(&ab["id"]), num(&kid));
    assert_eq!(h.ed.doc.get(ab).unwrap().rect.h, 1200.0, "fit-content grew");
    h.ed.set_scroll(&[ab], true);
    let n = h.ed.doc.get(ab).unwrap();
    assert!(n.scroll && n.rect.h == 844.0, "back to its screen height: {:?}", n.rect);
    let key = format!("{}:{}", &h.ed.pages[0].id[..6], ab.0);
    let s = call(&mut h, "get_computed_styles", json!({"nodeIds": [key]}));
    assert!(s.to_string().contains("\"overflow\":\"auto\""), "{s}");
    // The wheel over the screen scrolls it, up to the end of its content, then pans the canvas.
    h.ed.viewport = v2(1000.0, 800.0);
    h.ed.camera = Default::default();
    let over = v2(100.0, 100.0);
    let pan = h.ed.camera.pan;
    let bar = call(&mut h, "write_html", json!({"targetNodeId": key, "html": "<div style=\"position:absolute;left:0px;bottom:0px;width:390px;height:64px;background:#fff\"></div>"}))["createdNodeIds"][0].clone();
    let bar = num(&bar);
    let bar_y = h.ed.doc.world_rect(bar).unwrap().y;
    h.ed.handle(InputEvent::Scroll { delta: v2(0.0, 200.0), pos: over });
    assert_eq!(h.ed.doc.get(ab).unwrap().scroll_y, 200.0);
    assert_eq!(h.ed.doc.world_rect(kid).unwrap().y, -200.0, "the content moved up");
    assert_eq!(h.ed.doc.world_rect(bar).unwrap().y, bar_y, "an absolute bar stays pinned");
    assert_eq!(h.ed.camera.pan, pan, "the canvas stayed");
    h.ed.handle(InputEvent::Scroll { delta: v2(0.0, 1000.0), pos: over });
    assert_eq!(h.ed.doc.get(ab).unwrap().scroll_y, 356.0, "stops at the end of the content");
    h.ed.handle(InputEvent::Scroll { delta: v2(0.0, 50.0), pos: over });
    assert_ne!(h.ed.camera.pan, pan, "past the end the canvas pans");
    // overflow: auto from markup does the same; hidden turns it off.
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [key], "styles": {"overflow": "hidden"}}]}));
    assert!(!h.ed.doc.get(ab).unwrap().scroll);
}

#[test]
fn a_hug_column_with_full_width_text_wraps_its_text_not_more() {
    // Issue #22.
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Sheet", "styles": {"width": "390px", "height": "844px"}}));
    let made = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<div style=\"position:absolute;left:0px;top:400px;width:390px;height:fit-content;display:flex;flex-direction:column;align-items:center;padding:24px\"><div style=\"display:flex;flex-direction:column;align-items:center;gap:8px;width:100%\"><div style=\"font-size:20px;line-height:28px;text-align:center\">Short title</div><div style=\"width:100%;font-size:16px;line-height:24px;text-align:center\">A sentence long enough to wrap onto two or three lines in this column.</div></div></div>"}));
    let outer = num(&made["createdNodeIds"][0]);
    let doc = &h.ed.doc;
    let inner = doc.get(outer).unwrap().children[0];
    let n = doc.get(inner).unwrap();
    let kids: Vec<_> = n.children.iter().map(|c| doc.get(*c).unwrap().rect).collect();
    assert!(n.rect.h < 140.0, "inner column {:?} kids {kids:?}", n.rect);
    assert!(doc.get(outer).unwrap().rect.h < 200.0, "the sheet wraps it: {:?}", doc.get(outer).unwrap().rect);
}

#[test]
fn a_legacy_nested_replacement_becomes_a_swap_instead_of_being_lost() {
    use papercraft::model::InstanceLink;
    let mut h = host();
    let (row, screen, badge) = rows(&mut h);
    let key = format!("{}:{}", &h.ed.pages[0].id[..6], badge.0);
    call(&mut h, "write_html", json!({"mode": "replace", "targetNodeId": key,
        "html": "<x-paper-instance component=\"Badge / Amber\" overrides='{\"Label\":\"Late\"}'/>"}));
    // Rewrite it the way older versions stored it: its own instance root, no swap.
    let row_inst = h.ed.doc.get(num(&screen)).unwrap().children[0];
    let nested = h.ed.doc.get(row_inst).unwrap().children[0];
    let amber = h.ed.doc.get(nested).unwrap().instance.clone().unwrap().swap.unwrap().main;
    h.ed.edit("legacy", |tx| {
        let n = tx.node_mut(nested).unwrap();
        let l = n.instance.as_mut().unwrap();
        *l = InstanceLink { main: amber, overrides: l.overrides.clone(), source: None, swap: None };
    });
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [row], "styles": {"gap": "2px"}}]}));
    assert_eq!(badge_state(&h, &screen), ("Late".into(), "#ffbf00".into()), "kept, now as a swap");
    let l = h.ed.doc.get(h.ed.doc.get(row_inst).unwrap().children[0]).unwrap().instance.clone().unwrap();
    assert_eq!(l.swap.map(|s| s.main), Some(amber));
}
