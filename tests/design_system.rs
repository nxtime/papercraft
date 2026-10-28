//! Design-system tools: color variables, components with instance overrides, and how both
//! survive undo and a save.

use papercraft::mcp::{Out, exec};
use papercraft::model::{Color, NodeId, Paint};
use papercraft::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store =
        Store::open(std::env::temp_dir().join(format!("papercraft-ds-{:016x}", fastrand::u64(..))))
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
    use papercraft::project::format::{PageData, page_from_json, page_to_json};
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
        page_to_json(&page).contains("\"var\":\"accent\""),
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

#[test]
fn lint_finds_the_usual_mistakes_and_clears_a_clean_design() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#cba6f7", "text": "#111111", "bg": "#ffffff"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "300px", "height": "300px", "backgroundColor": "var(--bg)"}}),
    );
    call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": concat!(
            "<div style=\"display:flex;flex-direction:column;gap:13px;padding:10px\">",
            "<div layer-name=\"Faint\" style=\"color:#cccccc;font-size:14px\">low contrast</div>",
            "<div layer-name=\"Tiny\" style=\"color:#111111;font-size:9px\">tiny</div>",
            "<div layer-name=\"Odd\" style=\"color:#111111;font-size:15px\">off scale</div>",
            "<div layer-name=\"Button / Save\" style=\"display:flex;padding:8px;background:#cba6f7\"><div layer-name=\"Label\" style=\"color:#111111;font-size:14px\">Save</div></div>",
            "</div>")}),
    );
    let r = call(&mut h, "lint_design", json!({}));
    let rules: Vec<&str> = r["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["rule"].as_str().unwrap())
        .collect();
    for want in [
        "contrast",
        "text-size",
        "type-scale",
        "spacing-grid",
        "touch-target",
        "layer-name",
        "hard-coded-color",
    ] {
        assert!(rules.contains(&want), "missing {want}: {r}");
    }
    assert_eq!(r["errors"], 1, "{r}");
    // A tidy design: variables, scale sizes, grid spacing, named layers, tall button.
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#cba6f7", "text": "#111111", "bg": "#ffffff"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "300px", "height": "300px", "backgroundColor": "var(--bg)"}}),
    );
    call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": concat!(
            "<div layer-name=\"Content\" style=\"display:flex;flex-direction:column;gap:12px;padding:16px\">",
            "<div layer-name=\"Title\" style=\"color:var(--text);font-size:24px;font-weight:700\">Hello</div>",
            "<div layer-name=\"Button / Save\" style=\"display:flex;padding:16px;background:var(--accent)\"><div layer-name=\"Label\" style=\"color:var(--text);font-size:16px\">Save</div></div>",
            "</div>")}),
    );
    let r = call(&mut h, "lint_design", json!({}));
    assert_eq!(
        (r["errors"].clone(), r["warnings"].clone()),
        (json!(0), json!(0)),
        "{r}"
    );
    let css = exec::call(&mut h, "get_variables", &json!({"format": "css"})).unwrap();
    let Out::Text(css) = &css[0] else { panic!() };
    assert!(
        css.starts_with(":root {") && css.contains("--accent: #cba6f7;"),
        "{css}"
    );
}

#[test]
fn modes_turn_a_design_from_light_to_dark_and_back_with_undo() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"mode": "Light", "variables": {"bg": "#eff1f5", "text": "#4c4f69"}}),
    );
    call(
        &mut h,
        "set_variables",
        json!({"mode": "Dark", "variables": {"bg": "#1e1e2e", "text": "#cdd6f4"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "100px", "height": "100px", "backgroundColor": "var(--bg)"}}),
    );
    assert_eq!(
        fill(&h, &ab["id"]).0,
        Color::rgb(0xef, 0xf1, 0xf5),
        "the first mode defined is active"
    );
    let r = call(&mut h, "set_variable_mode", json!({"mode": "Dark"}));
    assert_eq!(r["fillsUpdated"], 1);
    assert_eq!(fill(&h, &ab["id"]).0, Color::rgb(0x1e, 0x1e, 0x2e));
    assert_eq!(call(&mut h, "get_variables", json!({}))["mode"], "Dark");
    call(&mut h, "undo", json!({}));
    assert_eq!(fill(&h, &ab["id"]).0, Color::rgb(0xef, 0xf1, 0xf5));
    assert_eq!(h.ed.doc.mode.as_deref(), Some("Light"));
    assert!(exec::call(&mut h, "set_variable_mode", &json!({"mode": "Nope"})).is_err());
    // Editing the active mode's value later moves its fills and is stored for that mode only.
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"bg": "#ffffff"}}),
    );
    assert_eq!(h.ed.doc.modes["Light"]["bg"], Color::rgb(255, 255, 255));
    assert_eq!(h.ed.doc.modes["Dark"]["bg"], Color::rgb(0x1e, 0x1e, 0x2e));
    // Modes survive a save.
    let page = papercraft::project::format::PageData {
        id: "p".into(),
        name: h.ed.doc.name.clone(),
        doc: h.ed.doc.clone(),
        camera: None,
    };
    let back = papercraft::project::format::page_from_json(
        &papercraft::project::format::page_to_json(&page),
    )
    .unwrap();
    assert_eq!(back.doc, h.ed.doc);
}

#[test]
fn feedback_flows_from_the_user_to_the_agent_and_back() {
    let mut h = host();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "200px", "height": "100px"}}),
    );
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
        "html": "<div layer-name=\"Title\" style=\"font-size:20px\">Hello</div>"}),
    );
    let title = r["createdNodeIds"][0].clone();
    assert_eq!(call(&mut h, "get_basic_info", json!({}))["openFeedback"], 0);
    // The user types a note in the inspector (same data the UI writes).
    let id = node(&title);
    h.ed.edit("Add feedback", |tx| {
        tx.node_mut(id)
            .unwrap()
            .comments
            .push(papercraft::model::Comment {
                id: 1,
                text: "make this bigger and bolder".into(),
                author: "user".into(),
                resolved: false,
                reply: None,
            });
    });
    assert_eq!(call(&mut h, "get_basic_info", json!({}))["openFeedback"], 1);
    let fb = call(&mut h, "get_feedback", json!({}));
    assert_eq!(fb["count"], 1);
    assert_eq!(fb["items"][0]["path"], "Screen > Title");
    assert_eq!(
        fb["items"][0]["feedback"][0]["text"],
        "make this bigger and bolder"
    );
    assert_eq!(fb["items"][0]["text"], "Hello");
    h.ed.selection = vec![id];
    assert_eq!(
        call(&mut h, "get_selection", json!({}))["selection"][0]["feedback"][0]["author"],
        "user"
    );
    // The agent answers and resolves; undo brings the note back.
    call(
        &mut h,
        "add_feedback",
        json!({"nodeId": title, "text": "should the subtitle change too?"}),
    );
    assert_eq!(
        call(&mut h, "get_feedback", json!({}))["items"][0]["feedback"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let r = call(
        &mut h,
        "resolve_feedback",
        json!({"nodeId": title, "commentId": 1, "reply": "now 32px bold"}),
    );
    assert_eq!(r["resolved"], 1);
    let fb = call(&mut h, "get_feedback", json!({"includeResolved": true}));
    let notes = fb["items"][0]["feedback"].as_array().unwrap();
    assert!(
        notes
            .iter()
            .any(|c| c["resolved"] == true && c["reply"] == "now 32px bold")
    );
    assert_eq!(call(&mut h, "get_basic_info", json!({}))["openFeedback"], 1);
    call(&mut h, "undo", json!({}));
    assert_eq!(call(&mut h, "get_basic_info", json!({}))["openFeedback"], 2);
}

#[test]
fn tokens_shadows_tables_and_named_instances_with_inline_overrides() {
    let mut h = host();
    call(
        &mut h,
        "set_tokens",
        json!({"tokens": {"spacing-md": "16px", "spacing-sm": 8, "radius-md": "8px", "shadow-md": "0 4px 12px rgba(0,0,0,0.5)"}}),
    );
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"surface": "#313244"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "400px", "height": "300px", "display": "flex", "flexDirection": "column", "padding": "var(--spacing-md)", "gap": "var(--spacing-sm)"}}),
    );
    let ab_id = node(&ab["id"]);
    let l = h.ed.doc.get(ab_id).unwrap().layout.unwrap();
    assert_eq!(
        (l.padding, l.gap),
        ([16.0; 4], 8.0),
        "tokens substitute into padding and gap"
    );
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": concat!(
        "<div layer-name=\"Card\" style=\"padding:var(--spacing-sm) var(--spacing-md);border-radius:var(--radius-md);box-shadow:var(--shadow-md);background:var(--surface)\">",
        "<table layer-name=\"Table\"><tr layer-name=\"Head\"><th style=\"width:100px\">Name</th><th>Role</th></tr>",
        "<tr layer-name=\"Row\"><td style=\"width:100px\">Ada</td><td>Admin</td></tr></table></div>")}),
    );
    let card = h.ed.doc.get(node(&r["createdNodeIds"][0])).unwrap();
    assert_eq!(card.style.corner_radius, 8.0);
    let s = card.style.shadow.expect("box-shadow parsed");
    assert_eq!((s.x, s.y, s.blur), (0.0, 4.0, 12.0));
    assert!(
        papercraft::render::draw::node_draw(card, h.ed.doc.world_rect(card.id).unwrap(), 1.0)
            .ops
            .len()
            > 5,
        "shadow layers are drawn"
    );
    let css = call(
        &mut h,
        "get_computed_styles",
        json!({"nodeIds": [r["createdNodeIds"][0]]}),
    );
    let shadow = css[r["createdNodeIds"][0].as_str().unwrap()]["boxShadow"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        shadow.starts_with("0px 4px 12px 0px rgba(0, 0, 0, 0.50"),
        "{shadow}"
    );
    let rows: Vec<_> =
        h.ed.doc
            .nodes
            .values()
            .filter(|n| n.name == "Row" || n.name == "Head")
            .collect();
    for row in &rows {
        assert_eq!(
            row.layout.unwrap().direction,
            papercraft::model::Direction::Horizontal,
            "{} is a row",
            row.name
        );
        assert_eq!(row.children.len(), 2);
    }
    let bad = call(
        &mut h,
        "update_styles",
        json!({"updates": [{"nodeIds": [ab["id"]], "styles": {"padding": "var(--nope)"}}]}),
    );
    assert!(bad["ignoredStyles"].to_string().contains("--nope"), "{bad}");

    // A component placed by name with its text set inline.
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
        "html": "<div layer-name=\"Pill\" style=\"display:flex;padding:4px 8px;background:var(--surface)\"><div layer-name=\"Label\" style=\"font-size:12px\">Tag</div></div>"}),
    );
    call(
        &mut h,
        "create_component",
        json!({"nodeId": r["createdNodeIds"][0], "name": "Pill"}),
    );
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
        "html": "<x-paper-instance component=\"pill\" layer-name=\"Pill / Admin\" overrides='{\"Label\":\"Admin\"}'/><x-paper-instance component=\"Nope\"/>"}),
    );
    assert!(r.to_string().contains("not a component"), "{r}");
    let inst = r["createdNodeIds"][0].clone();
    let label =
        h.ed.doc
            .subtree(node(&inst))
            .into_iter()
            .filter_map(|i| h.ed.doc.get(i))
            .find(|n| n.name == "Label")
            .unwrap();
    assert_eq!(label.kind.text().map(String::as_str), Some("Admin"));
    assert!(
        label
            .instance
            .as_ref()
            .unwrap()
            .overrides
            .contains(&"text".to_string())
    );
}

#[test]
fn components_import_to_other_pages_and_instances_work_there() {
    let mut h = host();
    let p1 = call(&mut h, "get_basic_info", json!({}))["pages"][0]["id"].clone();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"pageId": p1, "name": "Screen", "styles": {"width": "100px", "height": "100px"}}),
    );
    let r = call(
        &mut h,
        "write_html",
        json!({"pageId": p1, "targetNodeId": ab["id"], "mode": "insert-children",
        "html": "<div layer-name=\"Pill\" style=\"display:flex;padding:4px 8px;background:#cba6f7\"><div layer-name=\"Label\" style=\"font-size:12px\">Tag</div></div>"}),
    );
    call(
        &mut h,
        "create_component",
        json!({"pageId": p1, "nodeId": r["createdNodeIds"][0], "name": "Pill"}),
    );
    // Components used on screens must be top level to be imported: move it out of the artboard.
    call(
        &mut h,
        "move_nodes",
        json!({"pageId": p1, "moves": [{"nodeId": r["createdNodeIds"][0], "parentId": "root"}]}),
    );
    let p2 = call(&mut h, "create_page", json!({"name": "Second"}))["pageId"].clone();
    let imp = call(
        &mut h,
        "import_components",
        json!({"fromPageId": p1, "pageId": p2}),
    );
    assert_eq!(imp["imported"][0]["name"], "Pill", "{imp}");
    let again = call(
        &mut h,
        "import_components",
        json!({"fromPageId": p1, "pageId": p2}),
    );
    assert_eq!(again["skipped"][0], "Pill");
    let ab2 = call(
        &mut h,
        "create_artboard",
        json!({"pageId": p2, "name": "Screen 2", "styles": {"width": "200px", "height": "100px", "display": "flex"}}),
    );
    let inst = call(
        &mut h,
        "create_instance",
        json!({"pageId": p2, "componentId": imp["imported"][0]["id"], "parentId": ab2["id"]}),
    );
    let label = inst["textLayers"][0]["id"].clone();
    call(
        &mut h,
        "set_text_content",
        json!({"pageId": p2, "updates": [{"nodeId": label, "textContent": "Hi"}]}),
    );
    let comps = call(&mut h, "get_components", json!({"pageId": p2}));
    assert_eq!(comps["components"][0]["instances"], 1);
}

#[test]
fn shorthand_vars_text_transform_svg_vars_and_icon_recolor() {
    let mut h = host();
    call(
        &mut h,
        "set_tokens",
        json!({"tokens": {"a": "8px", "b": "24px"}}),
    );
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#cba6f7", "danger": "#f38ba8"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "S", "styles": {"width": "200px", "height": "100px", "display": "flex", "padding": "var(--a) var(--b)"}}),
    );
    let l = h.ed.doc.get(node(&ab["id"])).unwrap().layout.unwrap();
    assert_eq!(l.padding, [8.0, 24.0, 8.0, 24.0]);
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": concat!(
        "<div layer-name=\"Label\" style=\"text-transform:uppercase;font-size:14px\">hello there</div>",
        "<svg layer-name=\"Icon\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\"><path d=\"M0 0h16v16z\" fill=\"var(--accent)\"/></svg>")}),
    );
    let label = h.ed.doc.get(node(&r["createdNodeIds"][0])).unwrap();
    assert_eq!(label.kind.text().map(String::as_str), Some("HELLO THERE"));
    let icon = r["createdNodeIds"][1].clone();
    let shape =
        h.ed.doc
            .subtree(node(&icon))
            .into_iter()
            .filter_map(|i| h.ed.doc.get(i))
            .find(|n| matches!(n.kind, papercraft::model::NodeKind::Path { .. }))
            .unwrap()
            .id;
    assert_eq!(
        h.ed.doc.get(shape).unwrap().style.fills[0].preview_color(),
        Color::rgb(0xcb, 0xa6, 0xf7)
    );
    call(
        &mut h,
        "update_styles",
        json!({"updates": [{"nodeIds": [icon], "styles": {"color": "var(--danger)"}}]}),
    );
    let p = &h.ed.doc.get(shape).unwrap().style.fills[0];
    assert_eq!(
        (p.preview_color(), p.var()),
        (Color::rgb(0xf3, 0x8b, 0xa8), Some("danger"))
    );
}

#[test]
fn changing_variable_modes_preserves_existing_frame_geometry() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"mode":"Mocha", "variables":{"accent":"#cba6f7"}}),
    );
    call(
        &mut h,
        "set_variables",
        json!({"mode":"Latte", "variables":{"accent":"#8839ef"}}),
    );
    let frame = call(
        &mut h,
        "create_artboard",
        json!({"name":"Existing layout", "styles":{"width":"200px", "height":"100px", "display":"flex", "flexDirection":"row"}}),
    );
    let added = call(
        &mut h,
        "write_html",
        json!({"targetNodeId":frame["id"], "mode":"insert-children", "html":"<div style=\"width:30px;height:40px;background:var(--accent)\"></div>"}),
    );
    let id = node(&added["createdNodeIds"][0]);
    // A saved/imported layout may have positions established outside today's layout engine.
    h.ed.doc.nodes.get_mut(&id).unwrap().rect.x = 70.0;
    h.ed.doc.nodes.get_mut(&id).unwrap().rect.y = 25.0;
    let before = h.ed.doc.get(id).unwrap().rect;
    call(&mut h, "set_variable_mode", json!({"mode":"Latte"}));
    assert_eq!(h.ed.doc.get(id).unwrap().rect, before);
    assert_eq!(
        fill(&h, &added["createdNodeIds"][0]).0,
        Color::rgb(0x88, 0x39, 0xef)
    );
    call(&mut h, "undo", json!({}));
    assert_eq!(h.ed.doc.get(id).unwrap().rect, before);
    assert_eq!(
        fill(&h, &added["createdNodeIds"][0]).0,
        Color::rgb(0xcb, 0xa6, 0xf7)
    );
}

#[test]
fn old_variable_and_token_calls_land_in_collections() {
    let mut h = host();
    call(
        &mut h,
        "set_tokens",
        json!({"tokens": {"spacing-md": "16px", "font": "Inter", "gap": 4}}),
    );
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#cba6f7"}}),
    );
    let ds = &h.ed.design;
    let col = |n: &str| ds.collections.iter().find(|c| c.name == n).unwrap();
    assert!(col("Colors").variables.iter().any(|v| v.name == "accent"));
    let kind = |n: &str| {
        col("Tokens")
            .variables
            .iter()
            .find(|v| v.name == n)
            .unwrap()
            .kind
    };
    assert_eq!(kind("spacing-md"), papercraft::variables::Kind::Number);
    assert_eq!(kind("gap"), papercraft::variables::Kind::Number);
    assert_eq!(kind("font"), papercraft::variables::Kind::String);
    assert_eq!(h.ed.doc.tokens["spacing-md"], "16px");
    assert_eq!(h.ed.doc.tokens["font"], "Inter");

    // A second page follows, and a new page copies the active page's modes.
    for (mode, hex) in [("Light", "#eeeeee"), ("Dark", "#111111")] {
        call(
            &mut h,
            "set_variables",
            json!({"mode": mode, "variables": {"accent": hex}}),
        );
    }
    let (cid, dark) = papercraft::editor::design::find_mode(&h.ed.design, "Dark").unwrap();
    h.ed.set_page_mode(&cid, &dark).unwrap();
    let p2 = call(&mut h, "create_page", json!({"name": "Second"}));
    let i = h.ed.pages.iter().position(|p| p.id == p2["pageId"]).unwrap();
    assert_eq!(h.ed.pages[i].modes, h.ed.pages[h.ed.active_page].modes);
    assert_eq!(h.ed.page_doc(i).mode.as_deref(), Some("Dark"));
    assert_eq!(h.ed.page_doc(i).variables["accent"], Color::rgb(0x11, 0x11, 0x11));
}

fn try_call(h: &mut exec::SessionHost, name: &str, args: Value) -> Result<Value, String> {
    exec::call(h, name, &args).map(|outs| match outs.into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    })
}

#[test]
fn set_variables_new_shapes_and_get_variables_collections() {
    let mut h = host();
    call(&mut h, "create_collection", json!({"name": "Brand", "modes": ["Light", "Dark"]}));
    assert!(try_call(&mut h, "create_collection", json!({"name": "Brand"})).is_err());
    call(
        &mut h,
        "set_variables",
        json!({"collection": "Brand", "variables": {
            "color/brand/primary": {"value": "#ff0000", "description": "Main"},
            "color/brand/hover": {"alias": "color/brand/primary", "hidden": true},
            "space/md": {"value": 16, "type": "number", "collection": "Sizes"},
            "flag": {"value": true},
            "font": {"value": "Inter", "type": "string"},
        }}),
    );
    // Old shape still lands in the first color collection.
    call(&mut h, "set_variables", json!({"variables": {"accent": "#00ff00"}}));
    let v = call(&mut h, "get_variables", json!({}));
    let cols = v["collections"].as_array().unwrap();
    let brand = cols.iter().find(|c| c["name"] == "Brand").unwrap();
    assert_eq!(brand["modes"], json!(["Light", "Dark"]));
    assert_eq!(brand["defaultMode"], "Light");
    let var = |n: &str| {
        cols.iter()
            .flat_map(|c| c["variables"].as_array().unwrap())
            .find(|x| x["name"] == n)
            .unwrap_or_else(|| panic!("no {n}"))
            .clone()
    };
    let p = var("color/brand/primary");
    assert_eq!(p["cssName"], "color-brand-primary");
    assert_eq!(p["group"], "color/brand");
    assert_eq!(p["type"], "color");
    assert_eq!(p["values"]["Dark"], "#ff0000");
    assert_eq!(p["description"], "Main");
    let hover = var("color/brand/hover");
    assert_eq!(hover["values"]["Light"], json!({"alias": "color/brand/primary"}));
    assert_eq!(hover["hidden"], true);
    assert_eq!(var("space/md")["type"], "number");
    assert_eq!(var("space/md")["values"]["Default"], 16.0);
    assert!(cols.iter().any(|c| c["name"] == "Sizes"));
    assert_eq!(var("flag")["type"], "boolean");
    assert_eq!(var("font")["type"], "string");
    assert!(v["variables"]["accent"].is_string());
    assert_eq!(v["pageModes"]["Brand"], "Light");
    // Existing keys stay; the alias resolves into the cache.
    assert_eq!(v["variables"]["color-brand-hover"], "#ff0000");
    assert_eq!(h.ed.doc.tokens["space-md"], "16px");
    // A wrong kind or an alias cycle is refused and changes nothing.
    assert!(try_call(&mut h, "set_variables", json!({"variables": {"space/md": "#fff"}})).is_err());
    assert!(
        try_call(
            &mut h,
            "set_variables",
            json!({"variables": {"color/brand/primary": {"alias": "color/brand/hover"}}})
        )
        .is_err()
    );
    // Old color-only error text stays.
    let e = try_call(&mut h, "set_variables", json!({"variables": {"x": "nope"}})).unwrap_err();
    assert!(e.contains("is not a color"), "{e}");
}

#[test]
fn dtcg_nests_groups_and_references_aliases() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {
            "color/brand/primary": "#ff0000",
            "color/brand/hover": {"alias": "color/brand/primary"},
            "space/md": {"value": 16, "type": "number"},
            "enabled": {"value": true},
            "plain": "#000000",
        }}),
    );
    let t = call(&mut h, "get_variables", json!({"format": "json"}));
    assert_eq!(t["color"]["brand"]["primary"]["$type"], "color");
    assert_eq!(t["color"]["brand"]["primary"]["$value"], "#ff0000");
    assert_eq!(t["color"]["brand"]["hover"]["$value"], "{color.brand.primary}");
    assert_eq!(t["space"]["md"]["$type"], "dimension");
    assert_eq!(t["space"]["md"]["$value"], "16px");
    assert_eq!(t["enabled"]["$type"], "boolean");
    assert_eq!(t["enabled"]["$value"], true);
    assert_eq!(t["plain"]["$value"], "#000000");
    // css keeps today's flat shape: colors, numbers as px.
    let css = call(&mut h, "get_variables", json!({"format": "css"}));
    let css = css.as_str().unwrap();
    assert!(css.contains("--color-brand-primary: #ff0000;"), "{css}");
    assert!(css.contains("--space-md: 16px;"), "{css}");
}

#[test]
fn delete_variables_needs_force_when_aliased() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"base": "#112233", "alt": {"alias": "base"}}}),
    );
    let e = try_call(&mut h, "delete_variables", json!({"names": ["base"]})).unwrap_err();
    assert!(e.contains("alt"), "{e}");
    assert!(h.ed.design.find_by_css("base").is_some());
    // Deleting both in one call is fine without force.
    call(&mut h, "delete_variables", json!({"names": ["base", "alt"]}));
    assert!(h.ed.design.find_by_css("base").is_none());
    assert!(h.ed.design.find_by_css("alt").is_none());
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"base": "#112233", "alt": {"alias": "base"}}}),
    );
    call(&mut h, "delete_variables", json!({"names": ["base"], "force": true}));
    let (_, alt) = h.ed.design.find_by_css("alt").unwrap();
    assert!(matches!(
        alt.values.values().next(),
        Some(papercraft::variables::Value::Color(_))
    ));
    assert!(try_call(&mut h, "delete_variables", json!({"names": ["ghost"]})).is_err());
}

#[test]
fn export_design_system_writes_groups_and_round_trips() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {
            "color/brand/primary": "#ff0000",
            "space/md": {"value": 16, "type": "number"},
            "font": {"value": "Inter", "type": "string"},
            "on": {"value": true},
        }}),
    );
    let dir = std::env::temp_dir().join(format!("papercraft-ds-pkg-{:016x}", fastrand::u64(..)));
    call(&mut h, "export_design_system", json!({"path": dir}));
    let css = std::fs::read_to_string(dir.join("tokens.css")).unwrap();
    assert!(css.contains("/* color/brand */"), "{css}");
    assert!(css.contains("--color-brand-primary: #ff0000;"));
    assert!(css.contains("--space-md: 16px;"));
    assert!(css.contains("--font: Inter;"));
    assert!(!css.contains("--on:"), "booleans are not CSS tokens");
    let mut h2 = host();
    call(&mut h2, "import_design_system", json!({"path": dir}));
    let doc = h2.ed.doc.clone();
    assert_eq!(doc.variables["color-brand-primary"], Color::rgb(255, 0, 0));
    assert_eq!(doc.tokens["space-md"], "16px");
    assert_eq!(doc.tokens["font"], "Inter");
}

/// A text colour bound by `update_styles {color: var(--x)}` or `bind_variable text-color` keeps
/// following the variable through `set_variables`, on every page, in both entry shapes.
#[test]
fn text_colour_bindings_survive_set_variables() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"mode": "Light", "variables": {"color/tag/gold/text": "#8a6d00"}}),
    );
    call(
        &mut h,
        "set_variables",
        json!({"mode": "Dark", "variables": {"color/tag/gold/text": "#e0c060"}}),
    );
    let p2 = call(&mut h, "create_page", json!({"name": "Second"}));
    let mut texts: Vec<(Value, usize)> = Vec::new(); // (node id, page index)
    for (pi, page) in [Value::Null, p2["pageId"].clone()].into_iter().enumerate() {
        let mut args = json!({"name": "Screen", "styles": {"width": "200px", "height": "100px"}});
        if !page.is_null() {
            args["pageId"] = page.clone();
        }
        let ab = call(&mut h, "create_artboard", args);
        let r = call(
            &mut h,
            "write_html",
            json!({"targetNodeId": ab["id"], "mode": "insert-children",
                   "html": "<div layer-name=\"A\" style=\"font-size:14px\">A</div><div layer-name=\"B\" style=\"font-size:14px\">B</div>"}),
        );
        let ids = r["createdNodeIds"].as_array().unwrap().clone();
        call(
            &mut h,
            "update_styles",
            json!({"updates": [{"nodeIds": [ids[0]], "styles": {"color": "var(--color-tag-gold-text)"}}]}),
        );
        call(
            &mut h,
            "bind_variable",
            json!({"nodeIds": [ids[1]], "field": "text-color", "variable": "color-tag-gold-text"}),
        );
        texts.push((ids[0].clone(), pi));
        texts.push((ids[1].clone(), pi));
    }
    let check = |h: &exec::SessionHost, want: Color| {
        for (i, (id, pi)) in texts.iter().enumerate() {
            let n = h.ed.page_doc(*pi).get(node(id)).unwrap();
            let (c, var) = match n.style.fills.last() {
                Some(Paint::Solid { color, var }) => (*color, var.clone()),
                o => panic!("not solid: {o:?}"),
            };
            let bound = var.as_deref() == Some("color-tag-gold-text")
                || n.bindings.get("text-color").map(String::as_str) == Some("color-tag-gold-text");
            assert!(bound, "text {i} lost its binding (var {var:?}, bindings {:?})", n.bindings);
            assert_eq!(c, want, "text {i}");
        }
    };
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"color/tag/gold/text": "#4a3a00"}}),
    );
    check(&h, Color::rgb(0x4a, 0x3a, 0));
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"color/tag/gold/text": {"value": "#2a2000"}}}),
    );
    check(&h, Color::rgb(0x2a, 0x20, 0));
}

/// Re-colouring a text layer that was bound with `bind_variable text-color` replaces that
/// binding, so a later change of the old variable cannot overwrite the new colour or unlink it.
#[test]
fn update_styles_colour_replaces_a_text_colour_binding() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"old": "#111111", "new": "#222222"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "200px", "height": "100px"}}),
    );
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": "<div layer-name=\"A\" style=\"font-size:14px\">A</div>"}),
    );
    let t = r["createdNodeIds"][0].clone();
    call(&mut h, "bind_variable", json!({"nodeIds": [t], "field": "text-color", "variable": "old"}));
    call(
        &mut h,
        "update_styles",
        json!({"updates": [{"nodeIds": [t], "styles": {"color": "var(--new)"}}]}),
    );
    call(&mut h, "set_variables", json!({"variables": {"old": "#333333"}}));
    assert_eq!(fill(&h, &t), (Color::rgb(0x22, 0x22, 0x22), Some("new".into())));
}
