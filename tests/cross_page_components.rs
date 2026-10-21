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
