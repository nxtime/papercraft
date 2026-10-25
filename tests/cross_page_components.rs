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
