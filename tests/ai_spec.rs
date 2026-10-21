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
