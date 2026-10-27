//! The AI Spec must carry everything needed to rebuild a design: icons, vectors, text metrics,
//! token matches and component definitions.

use papercraft::mcp::{Out, exec};
use papercraft::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store = Store::open(
        std::env::temp_dir().join(format!("papercraft-spec-{:016x}", fastrand::u64(..))),
    )
    .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Spec test").unwrap();
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

fn find<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    if node["name"] == name {
        return Some(node);
    }
    node["children"]
        .as_array()?
        .iter()
        .find_map(|c| find(c, name))
}

#[test]
fn spec_describes_icons_vectors_text_tokens_and_components() {
    let mut h = host();
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#cba6f7", "surface": "#181825"}}),
    );
    call(
        &mut h,
        "set_tokens",
        json!({"tokens": {"space-3": "12px", "radius-lg": "16px", "text-sm": "14px"}}),
    );
    let board = call(
        &mut h,
        "create_artboard",
        json!({"name": "Card", "styles": {"width": "320px", "height": "fit-content", "padding": "12px", "gap": "12px", "backgroundColor": "var(--surface)", "borderRadius": "16px", "display": "flex", "flexDirection": "column"}}),
    );
    let icon = r##"<div layer-name="Icon / lucide:check" style="width:24px;height:24px"><svg width="24" height="24" viewBox="0 0 24 24"><path d="M5 12l5 5L20 7" fill="none" stroke="#cba6f7" stroke-width="2"/></svg></div>"##;
    let text = r#"<div layer-name="Title" style="font-size:14px;font-weight:700;line-height:20px;letter-spacing:0.5px;color:var(--accent)">Hello</div>"#;
    let button = r#"<div layer-name="Button / Primary" style="display:flex;padding:12px;background:var(--accent)"><div layer-name="Label" style="font-size:14px">Go</div></div>"#;
    call(
        &mut h,
        "write_html",
        json!({"targetNodeId": board["id"], "mode": "insert-children", "html": format!("{icon}{text}{button}")}),
    );
    let kids = call(&mut h, "get_children", json!({"nodeId": board["id"]}));
    let button_id = kids["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Button / Primary")
        .unwrap()["id"]
        .clone();
    call(&mut h, "create_component", json!({"nodeId": button_id}));
    call(
        &mut h,
        "create_instance",
        json!({"componentId": button_id, "parentId": board["id"], "name": "Button / Go"}),
    );

    let spec = call(
        &mut h,
        "get_design_spec",
        json!({"nodeId": board["id"], "format": "json", "target": "html"}),
    );
    let tree = &spec["layoutTree"];
    let icon = find(tree, "Icon / lucide:check").expect("icon node");
    assert_eq!(icon["kind"], "icon");
    assert_eq!(icon["icon"]["set"], "lucide");
    assert_eq!(icon["icon"]["name"], "check");
    assert_eq!(icon["icon"]["colorVar"], "--accent");
    assert!(icon["children"].as_array().unwrap().is_empty());

    let title = find(tree, "Title").unwrap();
    assert_eq!(title["text"]["fontWeight"], 700);
    assert_eq!(title["text"]["fontFamily"], "Inter");
    assert_eq!(title["text"]["fontSizeToken"], "--text-sm");
    assert!((title["text"]["letterSpacing"].as_f64().unwrap() - 0.5).abs() < 1e-6);

    assert_eq!(tree["style"]["cornerRadiusToken"], "--radius-lg");
    assert_eq!(tree["autoLayout"]["gapToken"], "--space-3");
    assert_eq!(tree["style"]["fills"][0]["var"], "--surface");

    assert_eq!(
        find(tree, "Button / Go").unwrap()["instanceOf"]["component"],
        "Button / Primary"
    );
    // Text boxes size themselves vertically.
    assert_eq!(title["sizing"]["vertical"], "hug");
    let defs = spec["componentDefinitions"].as_array().unwrap();
    assert_eq!(defs[0]["name"], "Button / Primary");
    assert!(find(&defs[0]["layoutTree"], "Label").is_some());

    let reference = spec["referenceCode"].as_str().unwrap();
    assert!(
        reference.contains("data-icon=\"lucide:check\""),
        "{reference}"
    );
    assert!(
        reference.contains("<svg"),
        "icons render as inline SVG: {reference}"
    );
    let rules = spec["enforcementRules"].as_array().unwrap();
    assert!(
        rules
            .iter()
            .any(|r| r.as_str().unwrap_or("").contains("icon `lucide:check`")),
        "{rules:?}"
    );
}

#[test]
fn compact_spec_is_one_line_per_node_with_tokens_and_no_reference_code() {
    let mut h = host();
    call(&mut h, "set_variables", json!({"variables": {"accent": "#cba6f7", "surface": "#181825"}}));
    call(&mut h, "set_tokens", json!({"tokens": {"space-3": "12px", "text-sm": "14px"}}));
    let board = call(
        &mut h,
        "create_artboard",
        json!({"name": "Card", "styles": {"width": "320px", "height": "fit-content", "padding": "12px", "gap": "12px", "backgroundColor": "var(--surface)", "display": "flex", "flexDirection": "column"}}),
    );
    let html = r##"<div layer-name="Icon / lucide:check" style="width:24px;height:24px"><svg width="24" height="24" viewBox="0 0 24 24"><path d="M5 12l5 5L20 7" fill="none" stroke="#cba6f7" stroke-width="2"/></svg></div><div layer-name="Title" style="font-size:14px;font-weight:700;line-height:20px;color:var(--accent)">Hello</div>"##;
    call(&mut h, "write_html", json!({"targetNodeId": board["id"], "mode": "insert-children", "html": html}));
    let json_len = call(&mut h, "get_design_spec", json!({"nodeId": board["id"], "format": "json"})).to_string().len();
    let compact = call(&mut h, "get_design_spec", json!({"nodeId": board["id"], "format": "compact"}));
    let text = compact.as_str().expect("compact is text");
    assert!(text.contains("Frame \"Card\" 320×hug col gap=--space-3 pad=[--space-3,--space-3,--space-3,--space-3] bg=--surface"), "{text}");
    assert!(text.contains("Icon lucide:check 24 color=--accent"), "{text}");
    assert!(text.contains("Text \"Title\" \"Hello\" --text-sm/20 700 color=--accent"), "{text}");
    assert!(text.contains("--surface: #181825") && text.contains("--space-3: 12px"), "{text}");
    assert!(!text.contains("MUST") && !text.contains("<div"), "no rules or reference code: {text}");
    assert!(text.len() * 4 < json_len, "compact {} vs json {json_len}", text.len());
}

#[test]
fn compact_spec_writes_a_bound_stroke_as_its_variable() {
    let mut h = host();
    call(&mut h, "set_variables", json!({"variables": {"border": "#e2e6df"}}));
    let board = call(
        &mut h,
        "create_artboard",
        json!({"name": "Card", "styles": {"width": "200px", "height": "100px", "border": "1px solid #e2e6df"}}),
    );
    call(&mut h, "bind_variable", json!({"nodeIds": [board["id"]], "field": "stroke", "variable": "border"}));
    let compact = call(&mut h, "get_design_spec", json!({"nodeId": board["id"], "format": "compact"}));
    let text = compact.as_str().expect("compact is text");
    assert!(text.contains("border=--border/1"), "{text}");
}

#[test]
fn inventory_lists_screens_by_size_class_and_open_questions() {
    let mut h = host();
    let phone = call(&mut h, "create_artboard", json!({"name": "Home", "styles": {"width": "390px", "height": "844px"}}));
    call(&mut h, "create_artboard", json!({"name": "Dashboard", "styles": {"width": "1440px", "height": "900px"}}));
    call(&mut h, "create_artboard", json!({"name": "Weekly e-mail", "styles": {"width": "640px", "height": "900px"}}));
    call(&mut h, "annotate", json!({"notes": [
        {"node": phone["id"], "text": "Check-in radius\n200 m ? (team)"},
        {"node": phone["id"], "text": "Camera only\nNo gallery"}
    ]}));
    let inv = call(&mut h, "get_inventory", json!({}));
    assert_eq!(inv["screens"], 3);
    let page = &inv["pages"][0];
    let classes: Vec<&str> = page["screens"].as_array().unwrap().iter().map(|s| s["class"].as_str().unwrap()).collect();
    assert_eq!(classes, ["mobile", "desktop", "email"]);
    assert_eq!(page["annotations"], 2);
    let open = page["openQuestions"].as_array().unwrap();
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0]["layer"], "Home");
    assert!(open[0]["text"].as_str().unwrap().contains("200 m"));
}

#[test]
fn vector_svg_in_the_spec_starts_at_the_origin() {
    let mut h = host();
    let board = call(
        &mut h,
        "create_artboard",
        json!({"name": "Dots", "left": "500px", "top": "300px", "styles": {"width": "200px", "height": "100px", "left": "500px", "top": "300px"}}),
    );
    let svg = r##"<svg width="40" height="20" viewBox="0 0 40 20"><path d="M5 5h30" fill="none" stroke="#ffffff" stroke-width="2"/></svg>"##;
    call(
        &mut h,
        "write_html",
        json!({"targetNodeId": board["id"], "mode": "insert-children", "html": svg}),
    );
    let spec = call(
        &mut h,
        "get_design_spec",
        json!({"nodeId": board["id"], "format": "json", "target": "html"}),
    );
    fn first_svg(n: &Value) -> Option<String> {
        n["svg"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| n["children"].as_array()?.iter().find_map(first_svg))
    }
    let svg = first_svg(&spec["layoutTree"]).expect("a vector with svg");
    assert!(svg.contains("viewBox=\"0 0 "), "{svg}");
    fn find_vector(n: &Value) -> Option<&Value> {
        if n["svg"].is_string() {
            return Some(n);
        }
        n["children"].as_array()?.iter().find_map(find_vector)
    }
    let vector = find_vector(&spec["layoutTree"]).unwrap();
    assert!(
        vector["svgBounds"]["width"].as_f64().unwrap()
            >= vector["bounds"]["width"].as_f64().unwrap()
    );
}

#[test]
fn theme_copies_are_flagged_and_a_mode_previews_without_switching() {
    let mut h = host();
    call(&mut h, "set_variables", json!({"mode": "Light", "variables": {"bg": "#ffffff"}}));
    call(&mut h, "set_variables", json!({"mode": "Dark", "variables": {"bg": "#111111"}}));
    let board = |h: &mut exec::SessionHost, name: &str, x: u32, bg: &str| {
        let b = call(h, "create_artboard", json!({"name": name, "styles": {"width": "200px", "height": "100px", "left": format!("{x}px"), "backgroundColor": bg}}));
        call(h, "write_html", json!({"targetNodeId": b["id"], "mode": "insert-children", "html": "<div layer-name=\"Title\" style=\"font-size:16px\">Hi</div>"}));
        b
    };
    let a = board(&mut h, "Home", 0, "var(--bg)");
    let b = board(&mut h, "Home copy", 400, "#222222");
    let issues = |h: &mut exec::SessionHost, b: &Value| call(h, "lint_design", json!({"nodeId": b["id"]}));
    let lint = issues(&mut h, &b).to_string();
    assert!(lint.contains("theme-copy"), "same layers, other colours: {lint}");
    let named = board(&mut h, "Home / Dark", 800, "var(--bg)");
    assert!(issues(&mut h, &named).to_string().contains("theme-copy"), "name differs by a theme word");
    let other = call(&mut h, "create_artboard", json!({"name": "Settings", "styles": {"width": "200px", "height": "100px", "left": "1200px", "backgroundColor": "#222222"}}));
    call(&mut h, "write_html", json!({"targetNodeId": other["id"], "mode": "insert-children", "html": "<div layer-name=\"Title\" style=\"font-size:16px\">Account</div>"}));
    let lo = issues(&mut h, &other).to_string(); assert!(!lo.contains("theme-copy"), "a different screen is not a copy: {lo}");

    // Preview Dark while the document stays Light.
    let page = h.ed.active_page;
    let before = h.ed.page_doc(page).clone();
    let dark = h.ed.page_doc_in_mode(page, "Dark").unwrap();
    let id = a["id"].as_str().unwrap();
    let fill = |d: &papercraft::model::Document| {
        let n = d.nodes.values().find(|n| n.name == "Home").unwrap();
        n.style.fills.last().unwrap().preview_color().to_hex()
    };
    assert_eq!(fill(&dark).to_lowercase(), "#111111", "{id}");
    assert_eq!(fill(h.ed.page_doc(page)), fill(&before), "document untouched");
}
