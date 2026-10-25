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

#[test]
fn per_state_components_merge_into_one_with_an_enum() {
    let mut h = host();
    let ds = call(&mut h, "get_basic_info", json!({}))["page"]["id"].clone();
    let other = call(&mut h, "create_page", json!({"name": "Screens"}))["pageId"].clone();
    let mut mains = Vec::new();
    for (name, active) in [("Hoje", 0), ("Lojas", 1), ("Pedidos", 2)] {
        let bar = call(&mut h, "create_artboard", json!({"pageId": ds, "name": format!("Tab bar / {name}"), "styles": {"width": "390px", "height": "64px", "display": "flex"}}));
        let items: String = ["Hoje", "Lojas", "Pedidos"].iter().enumerate().map(|(i, t)| {
            let c = if i == active { "#0f5132" } else { "#6b7280" };
            format!("<div layer-name=\"{t}\" style=\"display:flex\"><span layer-name=\"Label\" style=\"font-size:12px;color:{c}\">{t}</span></div>")
        }).collect();
        call(&mut h, "write_html", json!({"pageId": ds, "targetNodeId": bar["id"], "html": items}));
        mains.push(call(&mut h, "create_component", json!({"pageId": ds, "nodeId": bar["id"]}))["id"].clone());
    }
    // Instances of each variant on another page.
    let screen = call(&mut h, "create_artboard", json!({"pageId": other, "name": "S", "styles": {"width": "400px", "height": "300px", "display": "flex", "flexDirection": "column"}}));
    for v in &mains {
        call(&mut h, "create_instance", json!({"pageId": other, "componentId": v, "parentId": screen["id"]}));
    }
    let r = call(&mut h, "merge_variants", json!({"pageId": ds, "componentIds": mains, "name": "Active"}));
    assert_eq!(r["options"], json!(["Hoje", "Lojas", "Pedidos"]), "{r}");
    assert_eq!(r["instancesMoved"], 3, "{r}");
    let comps = call(&mut h, "get_components", json!({}));
    let list = comps["components"].as_array().unwrap();
    assert_eq!(list.len(), 1, "the emptied variants are gone: {comps}");
    assert_eq!(list[0]["name"], "Tab bar");
    assert_eq!(list[0]["uses"], 3);
    // The Pedidos instance shows Pedidos active.
    let doc = h.ed.page_doc(1);
    let s = doc.get(num(&screen["id"])).unwrap();
    let third = s.children[2];
    let label_of = |item: &str| {
        let it = doc.subtree(third).into_iter().find(|i| doc.get(*i).is_some_and(|n| n.name == item)).unwrap();
        let l = doc.subtree(it).into_iter().find(|i| doc.get(*i).is_some_and(|n| n.name == "Label")).unwrap();
        match doc.get(l).unwrap().style.fills.last() { Some(Paint::Solid { color, .. }) => color.to_hex(), _ => String::new() }
    };
    assert_eq!((label_of("Pedidos"), label_of("Hoje")), ("#0f5132".into(), "#6b7280".into()));
    assert_eq!(doc.get(third).unwrap().prop_values["Active"], "Pedidos");
}

#[test]
fn a_variant_with_another_layout_is_left_out_of_a_merge() {
    let mut h = host();
    let mut mains = Vec::new();
    for (name, justify) in [("Mensagem / Recebida", "flex-start"), ("Mensagem / Enviada", "flex-end")] {
        let m = call(&mut h, "create_artboard", json!({"name": name, "styles": {"width": "360px", "height": "60px", "display": "flex", "justifyContent": justify}}));
        call(&mut h, "write_html", json!({"targetNodeId": m["id"], "html": "<span layer-name=\"Text\">Oi</span>"}));
        mains.push(call(&mut h, "create_component", json!({"nodeId": m["id"]}))["id"].clone());
    }
    let e = exec::call(&mut h, "merge_variants", &json!({"componentIds": mains, "name": "Side"})).unwrap_err();
    assert!(e.contains("justifyContent"), "{e}");
    assert_eq!(call(&mut h, "get_components", json!({}))["components"].as_array().unwrap().len(), 2, "nothing deleted");
}

#[test]
fn a_merge_grows_the_base_with_hidden_layers_and_extends_an_existing_enum() {
    let mut h = host();
    let make = |h: &mut exec::SessionHost, name: &str, inner: &str, bg: &str| {
        let m = call(h, "create_artboard", json!({"name": name, "styles": {"width": "16px", "height": "16px", "display": "flex", "background": bg}}));
        if !inner.is_empty() {
            call(h, "write_html", json!({"targetNodeId": m["id"], "html": inner}));
        }
        call(h, "create_component", json!({"nodeId": m["id"]}))["id"].clone()
    };
    let empty = make(&mut h, "Checkbox / Vazio", "", "#ffffff");
    let half = make(&mut h, "Checkbox / Parcial", "", "#dddddd");
    let checked = make(&mut h, "Checkbox / Marcado", "<div layer-name=\"Check\" style=\"width:8px;height:8px;background:#000\"></div>", "#0f5132");
    // Nothing merges when fewer than two can: nothing changes either.
    assert!(exec::call(&mut h, "merge_variants", &json!({"componentIds": [empty, empty]})).is_err());
    assert_eq!(call(&mut h, "get_components", json!({}))["components"].as_array().unwrap().len(), 3, "nothing changed");
    let r = call(&mut h, "merge_variants", json!({"componentIds": [empty, half], "name": "State"}));
    assert_eq!(r["options"], json!(["Vazio", "Parcial"]), "{r}");
    // The merged base already has State: Marcado extends it, and its check joins the base hidden.
    let base = r["component"].clone();
    let screen = call(&mut h, "create_artboard", json!({"name": "S", "styles": {"width": "100px", "height": "40px", "display": "flex"}}));
    let inst = call(&mut h, "create_instance", json!({"componentId": checked, "parentId": screen["id"]}));
    let r = call(&mut h, "merge_variants", json!({"componentIds": [base, checked], "name": "State"}));
    assert_eq!(r["layersAdded"], 1, "{r}");
    let comps = call(&mut h, "get_components", json!({}));
    assert_eq!(comps["components"].as_array().unwrap().len(), 1, "{comps}");
    assert_eq!(comps["components"][0]["props"][0]["options"], json!(["Vazio", "Parcial", "Marcado"]), "{comps}");
    let i = num(&inst["id"]);
    let check = named(&h, i, "Check");
    assert!(h.ed.doc.get(check).unwrap().visible, "Marcado shows its check");
    assert_eq!(fill_hex(&h, i), "#0f5132");
    let main_check = named(&h, num(&base), "Check");
    assert!(!h.ed.doc.get(main_check).unwrap().visible, "the base keeps it hidden");
}

#[test]
fn prototype_links_connect_a_button_to_a_screen() {
    let mut h = host();
    let a = call(&mut h, "create_artboard", json!({"name": "A", "styles": {"width": "100px", "height": "100px", "display": "flex"}}));
    let b = call(&mut h, "create_artboard", json!({"name": "B", "styles": {"width": "100px", "height": "100px"}}));
    let btn = call(&mut h, "write_html", json!({"targetNodeId": a["id"], "html": "<div style=\"width:40px;height:20px\"></div>"}))["createdNodeIds"][0].clone();
    call(&mut h, "set_prototype_links", json!({"links": [{"nodeId": btn, "to": b["id"]}]}));
    assert_eq!(h.ed.doc.get(num(&btn)).unwrap().link, Some(num(&b["id"])));
    call(&mut h, "set_prototype_links", json!({"links": [{"nodeId": btn, "to": null}]}));
    assert_eq!(h.ed.doc.get(num(&btn)).unwrap().link, None);
}
