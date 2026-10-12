//! Design-system tools: color variables, components with instance overrides, and how both
//! survive undo and a save.

use paper_rs::mcp::{Out, exec};
use paper_rs::model::{Color, NodeId, Paint};
use paper_rs::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store =
        Store::open(std::env::temp_dir().join(format!("paper-rs-ds-{:016x}", fastrand::u64(..))))
            .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Design system").unwrap();
    h
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    match exec::call(h, name, &args) {
        Ok(outs) => match outs.into_iter().next() {
            Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
            _ => Value::Null,
        },
        Err(e) => panic!("{name} failed: {e}"),
    }
}

fn node(id: &Value) -> NodeId {
    NodeId(
        id.as_str()
            .unwrap()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap(),
    )
}

fn fill(h: &exec::SessionHost, id: &Value) -> (Color, Option<String>) {
    match h.ed.doc.get(node(id)).unwrap().style.fills.last() {
        Some(Paint::Solid { color, var }) => (*color, var.clone()),
        other => panic!("not a solid fill: {other:?}"),
    }
}

#[test]
fn variables_link_fills_and_move_them_with_undo() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#cba6f7", "text": "#cdd6f4"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "200px", "height": "100px", "backgroundColor": "var(--accent)"}}),
    );
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": "<div layer-name=\"Label\" style=\"color:var(--text);font-size:14px\">Hi</div><div layer-name=\"Fixed\" style=\"width:10px;height:10px;background:#ff0000\"></div>"}),
    );
    let ids = r["createdNodeIds"].as_array().unwrap().clone();
    let label = ids[0].clone();
    let fixed = ids[1].clone();
    assert_eq!(
        fill(&h, &ab["id"]),
        (Color::rgb(0xcb, 0xa6, 0xf7), Some("accent".into()))
    );
    assert_eq!(fill(&h, &label).1.as_deref(), Some("text"));
    assert_eq!(fill(&h, &fixed).1, None, "a literal color stays unlinked");

    let set = call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#1e66f5"}}),
    );
    assert_eq!(set["fillsUpdated"]["accent"], 1);
    assert_eq!(fill(&h, &ab["id"]).0, Color::rgb(0x1e, 0x66, 0xf5));
    assert_eq!(fill(&h, &fixed).0, Color::rgb(0xff, 0, 0));
    // The variable and the fills it moved undo together.
    call(&mut h, "undo", json!({}));
    assert_eq!(fill(&h, &ab["id"]).0, Color::rgb(0xcb, 0xa6, 0xf7));
    assert_eq!(h.ed.doc.variables["accent"], Color::rgb(0xcb, 0xa6, 0xf7));
    // CSS read-back keeps the link, and an unknown variable is reported, not applied.
    let css = call(
        &mut h,
        "get_computed_styles",
        json!({"nodeIds": [ab["id"]]}),
    );
    assert_eq!(
        css[ab["id"].as_str().unwrap()]["backgroundColor"],
        "var(--accent)"
    );
    let bad = call(
        &mut h,
        "update_styles",
        json!({"updates": [{"nodeIds": [fixed], "styles": {"backgroundColor": "var(--nope)"}}]}),
    );
    assert!(bad["ignoredStyles"].to_string().contains("--nope"), "{bad}");
    // A new page starts with the project's variables.
    let p = call(&mut h, "create_page", json!({"name": "Second"}));
    let vars = call(&mut h, "get_variables", json!({"pageId": p["pageId"]}));
    assert_eq!(vars["variables"]["text"], "#cdd6f4");
}

#[test]
fn variables_survive_a_save() {
    use paper_rs::project::format::{PageData, page_from_json, page_to_json};
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#cba6f7"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "A", "styles": {"width": "50px", "height": "50px", "background": "var(--accent)"}}),
    );
    let page = PageData {
        id: "p".into(),
        name: h.ed.doc.name.clone(),
        doc: h.ed.doc.clone(),
        camera: None,
    };
    let back = page_from_json(&page_to_json(&page)).unwrap();
    assert_eq!(back.doc.variables, h.ed.doc.variables);
    assert_eq!(back.doc, h.ed.doc);
    assert!(
        page_to_json(&page).contains("\"var\": \"accent\""),
        "{}",
        ab["id"]
    );
}

#[test]
fn instances_follow_the_component_except_what_was_overridden() {
    let mut h = host();
    let page = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "300px", "height": "300px", "display": "flex", "flexDirection": "column", "gap": "8px"}}),
    );
    // A button frame with a label, made into a component.
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": page["id"], "mode": "insert-children",
               "html": "<div layer-name=\"Button\" style=\"display:flex;padding:12px;background:#cba6f7;border-radius:8px\"><div layer-name=\"Label\" style=\"color:#111;font-size:14px\">Button</div></div>"}),
    );
    let button = r["createdNodeIds"][0].clone();
    let comp = call(
        &mut h,
        "create_component",
        json!({"nodeId": button, "name": "Button"}),
    );
    assert_eq!(comp["component"], true);
    assert!(
        exec::call(
            &mut h,
            "create_component",
            &json!({"nodeId": page["id"], "name": "x"})
        )
        .is_ok(),
        "an artboard frame can be a component"
    );

    let a = call(
        &mut h,
        "create_instance",
        json!({"componentId": comp["id"], "parentId": page["id"], "name": "Button / Save"}),
    );
    let b = call(
        &mut h,
        "create_instance",
        json!({"componentId": comp["id"], "parentId": page["id"], "name": "Button / Cancel"}),
    );
    let la = a["textLayers"][0]["id"].clone();
    let lb = b["textLayers"][0]["id"].clone();
    call(
        &mut h,
        "set_text_content",
        json!({"updates": [{"nodeId": la, "textContent": "Save"}, {"nodeId": lb, "textContent": "Cancel"}]}),
    );
    // Restyle the main component: the label font and the box color flow to both instances.
    let main_label = {
        let tree = call(&mut h, "get_children", json!({"nodeId": comp["id"]}));
        tree["children"][0]["id"].clone()
    };
    call(
        &mut h,
        "update_styles",
        json!({"updates": [{"nodeIds": [comp["id"]], "styles": {"backgroundColor": "#89b4fa"}}, {"nodeIds": [main_label], "styles": {"fontSize": "20px"}}]}),
    );
    let text_of = |h: &exec::SessionHost, id: &Value| {
        h.ed.doc
            .get(node(id))
            .unwrap()
            .kind
            .text()
            .unwrap()
            .to_string()
    };
    assert_eq!(
        text_of(&h, &la),
        "Save",
        "text override survives main edits"
    );
    assert_eq!(text_of(&h, &lb), "Cancel");
    assert_eq!(
        h.ed.doc
            .get(node(&a["id"]))
            .unwrap()
            .style
            .fills
            .last()
            .unwrap()
            .preview_color(),
        Color::rgb(0x89, 0xb4, 0xfa)
    );
    assert_eq!(
        h.ed.doc.get(node(&la)).unwrap().kind.font_size(),
        Some(20.0)
    );
    let comps = call(&mut h, "get_components", json!({}));
    assert!(
        comps["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "Button" && c["instances"] == 2),
        "{comps}"
    );

    // <x-paper-instance> places one from HTML.
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": page["id"], "mode": "insert-children",
        "html": format!("<x-paper-instance component-id=\"{}\" layer-name=\"Button / OK\"/>", comp["id"].as_str().unwrap())}),
    );
    let ok = r["createdNodeIds"][0].clone();
    assert_eq!(h.ed.doc.get(node(&ok)).unwrap().name, "Button / OK");
    assert!(h.ed.doc.get(node(&ok)).unwrap().instance.is_some());
}
