//! Component properties: enums (status, active item), text and booleans, set per instance.

use papercraft::mcp::{Out, exec};
use papercraft::model::{NodeId, Paint};
use papercraft::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store = Store::open(std::env::temp_dir().join(format!("papercraft-props-{:016x}", fastrand::u64(..)))).unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Props").unwrap();
    h
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    match exec::call(h, name, &args).unwrap_or_else(|e| panic!("{name}: {e}")).into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    }
}

fn num(id: &Value) -> NodeId {
    NodeId(id.as_str().unwrap().rsplit(':').next().unwrap().parse().unwrap())
}

fn fill_hex(h: &exec::SessionHost, id: NodeId) -> String {
    match h.ed.doc.get(id).unwrap().style.fills.last() {
        Some(Paint::Solid { color, .. }) => color.to_hex(),
        _ => String::new(),
    }
}

/// The node named `name` under `root`.
fn named(h: &exec::SessionHost, root: NodeId, name: &str) -> NodeId {
    let doc = &h.ed.doc;
    doc.subtree(root).into_iter().find(|i| doc.get(*i).is_some_and(|n| n.name == name)).unwrap_or_else(|| panic!("no `{name}`"))
}

#[test]
fn a_status_enum_styles_each_instance_and_survives_main_edits() {
    let mut h = host();
    let badge = call(&mut h, "create_artboard", json!({"name": "Badge", "styles": {"width": "fit-content", "height": "fit-content", "display": "flex", "gap": "4px", "padding": "2px 8px"}}));
    call(&mut h, "write_html", json!({"targetNodeId": badge["id"], "html": "<span layer-name=\"Label\" style=\"font-size:12px\">Autorizada</span>"}));
    let main = call(&mut h, "create_component", json!({"nodeId": badge["id"]}))["id"].clone();
    let r = call(&mut h, "set_component_props", json!({"componentId": main, "template": "status",
        "options": [{"value": "Autorizada", "color": "#16a34a"}, {"value": "Rejeitada", "color": "#dc2626"}]}));
    assert_eq!(r["props"][0]["options"], json!(["Autorizada", "Rejeitada"]), "{r}");
    let list = call(&mut h, "create_artboard", json!({"name": "List", "styles": {"width": "400px", "height": "200px", "display": "flex", "flexDirection": "column", "gap": "8px"}}));
    let made = call(&mut h, "write_html", json!({"targetNodeId": list["id"], "html":
        "<x-paper-instance component=\"Badge\" props='{\"Status\":\"Autorizada\"}'/><x-paper-instance component=\"Badge\" props='{\"Status\":\"Rejeitada\"}'/>"}));
    let (a, b) = (num(&made["createdNodeIds"][0]), num(&made["createdNodeIds"][1]));
    let label = |h: &exec::SessionHost, r: NodeId| h.ed.doc.get(named(h, r, "Label")).unwrap().kind.text().unwrap().to_string();
    assert_eq!((label(&h, a), label(&h, b)), ("Autorizada".into(), "Rejeitada".into()));
    assert_eq!(fill_hex(&h, named(&h, b, "Label")), "#dc2626");
    // Editing the main keeps the values.
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [main], "styles": {"gap": "6px"}}]}));
    assert_eq!(label(&h, b), "Rejeitada");
    assert_eq!(fill_hex(&h, named(&h, b, "Label")), "#dc2626");
    // Changing one value changes only that instance.
    call(&mut h, "set_instance_props", json!({"nodeIds": [made["createdNodeIds"][0]], "values": {"Status": "Rejeitada"}}));
    assert_eq!((label(&h, a), label(&h, b)), ("Rejeitada".into(), "Rejeitada".into()));
    // Editing an option updates every instance using it.
    call(&mut h, "set_component_props", json!({"componentId": main, "template": "status",
        "options": [{"value": "Autorizada", "color": "#16a34a"}, {"value": "Rejeitada", "color": "#b91c1c"}]}));
    assert_eq!((fill_hex(&h, named(&h, a, "Label")), fill_hex(&h, named(&h, b, "Label"))), ("#b91c1c".into(), "#b91c1c".into()));
    let comps = call(&mut h, "get_components", json!({}));
    assert_eq!(comps["components"][0]["props"][0]["name"], "Status", "{comps}");
}

#[test]
fn the_active_template_highlights_one_item_of_a_tab_bar() {
    let mut h = host();
    call(&mut h, "set_variables", json!({"variables": {"primary": "#0f5132", "text-muted": "#6b7280"}}));
    let bar = call(&mut h, "create_artboard", json!({"name": "Tab bar", "styles": {"width": "390px", "height": "64px", "display": "flex", "justifyContent": "space-between"}}));
    let items: String = ["Hoje", "Roteiro", "Lojas", "Pedidos", "Mais"].iter().map(|t| format!("<div layer-name=\"Tab\" style=\"display:flex;flex-direction:column\"><span layer-name=\"Label\" style=\"font-size:12px\">{t}</span></div>")).collect();
    call(&mut h, "write_html", json!({"targetNodeId": bar["id"], "html": items}));
    let main = call(&mut h, "create_component", json!({"nodeId": bar["id"]}))["id"].clone();
    let r = call(&mut h, "set_component_props", json!({"componentId": main, "template": "active"}));
    assert_eq!(r["props"][0]["options"], json!(["Hoje", "Roteiro", "Lojas", "Pedidos", "Mais"]), "{r}");
    let screen = call(&mut h, "create_artboard", json!({"name": "S", "styles": {"width": "400px", "height": "100px", "display": "flex"}}));
    let made = call(&mut h, "write_html", json!({"targetNodeId": screen["id"], "html": "<x-paper-instance component=\"Tab bar\" props='{\"Active\":\"Pedidos\"}'/>"}));
    let inst = num(&made["createdNodeIds"][0]);
    let item_label = |h: &exec::SessionHost, item: &str| {
        let it = named(h, inst, item);
        h.ed.doc.get(named(h, it, "Label")).unwrap().clone()
    };
    let (on, off) = (item_label(&h, "Pedidos"), item_label(&h, "Lojas"));
    assert_eq!(fill_hex(&h, on.id), "#0f5132");
    assert_eq!(fill_hex(&h, off.id), "#6b7280");
    assert!(on.text_style.bold && !off.text_style.bold, "{:?}", on.text_style);
}
