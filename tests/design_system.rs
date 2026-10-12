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
