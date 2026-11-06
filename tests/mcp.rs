//! MCP server: tool behaviour on a headless session, then the real thing end to end — an rmcp
//! client over streamable HTTP, and through the `papercraft-mcp` stdio bridge.

use papercraft::mcp::{self, Out, exec};
use papercraft::model::{Color, NodeKind, Paint, Sizing};
use papercraft::project::Store;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::{Value, json};

fn store() -> Store {
    Store::open(std::env::temp_dir().join(format!("papercraft-mcp-{:016x}", fastrand::u64(..))))
        .unwrap()
}

/// A session with a fresh project open.
fn host() -> exec::SessionHost {
    let s = store();
    let mut h = exec::SessionHost::new(s);
    h.session.create(&mut h.ed, "MCP test").unwrap();
    h
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    match exec::call(h, name, &args) {
        Ok(outs) => match outs.into_iter().next() {
            Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
            Some(Out::Image { .. }) => json!("image"),
            None => Value::Null,
        },
        Err(e) => panic!("{name} failed: {e}"),
    }
}

fn call_err(h: &mut exec::SessionHost, name: &str, args: Value) -> String {
    exec::call(h, name, &args).err().unwrap_or_else(|| panic!("{name} should fail"))
}

fn num(id: &Value) -> papercraft::model::NodeId {
    papercraft::model::NodeId(
        id.as_str()
            .unwrap()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap(),
    )
}

#[test]
fn artboard_and_html_become_auto_layout_frames_and_text() {
    let mut h = host();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Phone", "styles": {"width": "390px", "height": "844px", "padding": "24px", "gap": 16, "backgroundColor": "#f5f5f5"}}),
    );
    let ab_id = ab["id"].clone();
    let r = call(
        &mut h,
        "write_html",
        json!({
            "targetNodeId": ab_id, "mode": "insert-children",
            "html": r#"<div layer-name="Card" style="display:flex; flex-direction:column; gap:8px; padding:16px; background:#ffffff; border-radius:12px; border:1px solid #e5e5e5">
                <h1 style="color:#111">Welcome back</h1>
                <p style="font-size:14px; color:rgb(80, 80, 80); line-height:20px">Sign in to <b>continue</b> where you left off.</p>
                <div style="display:flex; justify-content:center; align-items:center; height:44px; background:#2563eb; border-radius:8px; color:white; font-weight:600">Continue</div>
              </div>"#,
        }),
    );
    assert_eq!(r["createdNodeIds"].as_array().unwrap().len(), 1, "{r}");
    let card = num(&r["createdNodeIds"][0]);
    let doc = &h.ed.doc;
    doc.validate().unwrap();
    let c = doc.get(card).unwrap();
    assert_eq!(c.name, "Card");
    assert!(c.layout.is_some());
    assert_eq!(c.style.corner_radius, 12.0);
    // Stretched across the artboard's inner width (390 - 2 × 24).
    assert_eq!(c.layout_child.sizing[0], Sizing::Fill);
    assert!((c.rect.w - 342.0).abs() < 0.5, "{:?}", c.rect);
    assert_eq!((c.rect.x, c.rect.y), (24.0, 24.0));
    assert_eq!(c.children.len(), 3);

    let title = doc.get(c.children[0]).unwrap();
    let NodeKind::Text {
        text, font_size, ..
    } = &title.kind
    else {
        panic!("{:?}", title.kind)
    };
    assert_eq!((text.as_str(), *font_size), ("Welcome back", 32.0));
    assert!(title.text_style.bold);
    assert_eq!(title.style.fill(), Some(Color::rgb(0x11, 0x11, 0x11)));
    // Text fills the card's inner width and sits inside its padding.
    assert!(
        (title.rect.w - (342.0 - 32.0)).abs() < 0.5,
        "{:?}",
        title.rect
    );
    assert_eq!((title.rect.x, title.rect.y), (16.0, 16.0));

    let body = doc.get(c.children[1]).unwrap();
    let NodeKind::Text { text, .. } = &body.kind else {
        panic!()
    };
    assert_eq!(text, "Sign in to continue where you left off.");
    assert_eq!(body.text_style.line_height, Some(20.0 / 14.0));
    assert!(
        body.text_style
            .spans
            .iter()
            .any(|s| s.bold == Some(true) && s.end - s.start == 8)
    );
    assert!((body.rect.y - (title.rect.y + title.rect.h + 8.0)).abs() < 0.5);

    // The button is a frame (it has a background) holding a centered white bold label.
    let button = doc.get(c.children[2]).unwrap();
    assert!(matches!(button.kind, NodeKind::Frame { .. }));
    assert_eq!(button.rect.h, 44.0);
    assert_eq!(button.style.fill(), Some(Color::rgb(0x25, 0x63, 0xeb)));
    let label = doc.get(button.children[0]).unwrap();
    assert_eq!(label.kind.text().unwrap(), "Continue");
    assert!(label.text_style.bold);
    assert_eq!(label.style.fill(), Some(Color::WHITE));
    let centered = button.rect.w * 0.5 - (label.rect.x + label.rect.w * 0.5);
    assert!(
        centered.abs() < 1.0,
        "label not centered: {:?} in {:?}",
        label.rect,
        button.rect
    );

    // The card hugs its content: padding + three children + two gaps.
    let want = 16.0 + title.rect.h + 8.0 + body.rect.h + 8.0 + 44.0 + 16.0;
    assert!((c.rect.h - want).abs() < 0.5, "{} vs {want}", c.rect.h);

    // One undo step removes the whole write.
    let before = h.ed.doc.nodes.len();
    call(&mut h, "undo", json!({}));
    assert_eq!(h.ed.doc.nodes.len(), 1, "{before} nodes before undo");
}

#[test]
fn write_html_maps_margin_grid_and_translate_and_groups_what_it_drops() {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "A", "styles": {"width": "400px", "height": "400px"}}));
    let html = "<div layer-name=\"Page\" style=\"display:flex;flex-direction:column;width:400px\">        <div layer-name=\"Grid\" style=\"display:grid;grid-template-columns:repeat(2,1fr);gap:10px;width:400px\">          <div layer-name=\"Wide\" style=\"grid-column:span 2;height:20px\"></div>          <div layer-name=\"Left\" style=\"height:20px\"></div><div layer-name=\"Right\" style=\"height:20px;transform:translate(5px,5px) rotate(3deg)\"></div></div>        <div layer-name=\"Tilted\" style=\"height:10px;transform:rotate(2deg) scale(1.1);float:left\"></div></div>";
    let w = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": html}));
    let ignored = w["ignoredStyles"].to_string();
    assert!(ignored.contains("transform: rotate, scale"), "{w}");
    assert_eq!(w["ignoredStyles"].as_array().unwrap().iter().filter(|v| v.as_str().unwrap().starts_with("transform")).count(), 1, "{w}");
    let rect = |name: &str| h.ed.doc.nodes.values().find(|n| n.name == name).unwrap_or_else(|| panic!("no {name}")).rect;
    assert!((rect("Wide").w - 400.0).abs() < 0.5, "a span covers both columns: {:?}", rect("Wide"));
    assert!((rect("Left").w - 195.0).abs() < 0.5, "{:?}", rect("Left"));
}

#[test]
fn import_html_opens_a_zip_with_its_css_and_images() {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 2, image::Rgba([200, 30, 30, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let page = |title: &str| format!(
        "<html><head><style>.card{{background:#eeeeee;padding:12px;margin:0 auto;width:300px}} h1{{font-size:30px}} .x{{transform:rotate(4deg)}}</style>         <link rel=\"stylesheet\" href=\"css/site.css\"></head><body><div class=\"card x\"><h1>{title}</h1><img src=\"img/a.png\" style=\"width:40px\"></div></body></html>");
    let entries: Vec<papercraft::io::zip::Entry> = vec![
        ("index.html".into(), page("Home").into_bytes()),
        ("about/about.html".into(), page("About").into_bytes()),
        ("css/site.css".into(), b"h1{color:#0000ff}".to_vec()),
        ("img/a.png".into(), png.into_inner()),
        ("../evil.html".into(), b"<p>no</p>".to_vec()),
    ];
    let path = std::env::temp_dir().join(format!("papercraft-import-{:016x}.zip", fastrand::u64(..)));
    std::fs::write(&path, papercraft::io::zip::write(&entries)).unwrap();

    let mut h = host();
    let r = call(&mut h, "import_html", json!({"path": path.to_string_lossy()}));
    let frames = r["imported"].as_array().unwrap();
    assert_eq!(frames.len(), 2, "{r}");
    assert_eq!(h.ed.doc.root.len(), 2, "one frame per file");
    let warnings = r["warnings"].to_string();
    assert!(warnings.contains("evil.html"), "unsafe entries are reported: {r}");
    assert!(r["ignoredStyles"].to_string().contains("transform: rotate"), "{r}");
    let nodes = || h.ed.doc.nodes.values();
    let title = nodes().find(|n| n.kind.text().map(String::as_str) == Some("Home")).expect("Home text");
    assert_eq!(title.kind.font_size(), Some(30.0), "<style> rule applied");
    assert_eq!(title.style.fill(), Some(Color::rgb(0, 0, 255)), "linked stylesheet applied");
    assert!(nodes().any(|n| matches!(n.style.fills.first(), Some(Paint::Image { .. }))), "image from the zip");

    call(&mut h, "undo", json!({}));
    assert_eq!(h.ed.doc.root.len(), 1, "one undo step per file");
    std::fs::remove_file(&path).ok();
}

#[test]
fn prototype_links_set_each_kind_and_refuse_bad_ones() {
    use papercraft::model::LinkAction;
    let mut h = host();
    let mut frame = |name: &str| call(&mut h, "create_artboard", json!({"name": name, "styles": {"width": "100px", "height": "100px"}}))["id"].clone();
    let (home, modal, button) = (frame("Home"), frame("Modal"), frame("Button"));
    let other = call(&mut h, "create_page", json!({"name": "Other"}))["pageId"].clone();
    let far = call(&mut h, "create_artboard", json!({"name": "Far", "pageId": other, "styles": {"width": "100px", "height": "100px"}}))["id"].clone();
    let link = |h: &mut exec::SessionHost| {
        let n = h.ed.doc.get(num(&button)).unwrap();
        (n.link, n.link_action.clone())
    };
    let set = |h: &mut exec::SessionHost, l: Value| exec::call(h, "set_prototype_links", &json!({"links": [l]}));

    set(&mut h, json!({"nodeId": button, "to": home})).unwrap();
    assert_eq!(link(&mut h), (Some(num(&home)), None), "the original arguments still work");
    set(&mut h, json!({"nodeId": button, "overlay": modal})).unwrap();
    assert_eq!(link(&mut h), (None, Some(LinkAction::Overlay { target: num(&modal) })), "a new kind replaces the old link");
    set(&mut h, json!({"nodeId": button, "url": " https://a.dev/x "})).unwrap();
    assert_eq!(link(&mut h).1, Some(LinkAction::Url { url: "https://a.dev/x".into() }));
    set(&mut h, json!({"nodeId": button, "action": "back"})).unwrap();
    assert_eq!(link(&mut h).1, Some(LinkAction::Back));
    set(&mut h, json!({"nodeId": button, "action": "close"})).unwrap();
    assert_eq!(link(&mut h).1, Some(LinkAction::Close));

    for bad in [
        json!({"nodeId": button, "url": "javascript:alert(1)"}),
        json!({"nodeId": button, "url": "file:///etc/passwd"}),
        json!({"nodeId": button, "action": "forward"}),
        json!({"nodeId": button, "to": home, "url": "https://a.dev"}),
        json!({"nodeId": button, "to": far}),
        json!({"nodeId": button, "overlay": far}),
        json!({"nodeId": button, "overlay": "999999"}),
    ] {
        assert!(set(&mut h, bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(link(&mut h).1, Some(LinkAction::Close), "refused links change nothing");

    // One bad link refuses the whole call.
    let both = json!({"links": [{"nodeId": button, "action": "back"}, {"nodeId": button, "url": "ftp://x"}]});
    assert!(exec::call(&mut h, "set_prototype_links", &both).is_err());
    assert_eq!(link(&mut h).1, Some(LinkAction::Close));

    set(&mut h, json!({"nodeId": button, "to": null})).unwrap();
    assert_eq!(link(&mut h), (None, None), "no kind clears the link");
}

#[test]
fn an_installed_font_reaches_text_from_html_and_computed_styles() {
    let mut h = host();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts/VarelaRound-Regular.ttf");
    let installed = call(&mut h, "install_font", json!({"source": path}));
    assert_eq!(installed["family"], "Varela Round", "{installed}");

    let info = call(&mut h, "get_font_family_info", json!({"familyNames": ["varela round", "Nope"]}));
    assert_eq!(info["families"][0]["available"], true, "{info}");
    assert_eq!(info["families"][1]["available"], false, "{info}");

    let ab = call(&mut h, "create_artboard", json!({"name": "A", "styles": {"width": "300px", "height": "200px"}}));
    let w = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": "<div style=\"font-family:'Varela Round',serif\"><div style='font-size:20px'>Hi</div></div>"}),
    );
    assert!(w.to_string().contains("createdNodeIds"), "{w}");
    let text = h.ed.doc.nodes.values().find(|n| matches!(n.kind, papercraft::model::NodeKind::Text { .. })).unwrap();
    assert_eq!(text.text_style.font.as_deref(), Some("Varela Round"));
    assert!(papercraft::text::layout_node(text).unwrap().glyphs.iter().all(|g| g.face >= papercraft::text::BUILT_IN_FACES));
}

/// Every text block of a result, for tools that answer with a body and a metadata block.
fn call_blocks(h: &mut exec::SessionHost, name: &str, args: Value) -> Vec<String> {
    match exec::call(h, name, &args) {
        Ok(outs) => outs.into_iter().filter_map(|o| if let Out::Text(t) = o { Some(t) } else { None }).collect(),
        Err(e) => panic!("{name} failed: {e}"),
    }
}

/// A screen with a themed button and a long list of rows, selected as one layer.
fn screen_with_rows(h: &mut exec::SessionHost, rows: usize) -> Value {
    call(h, "set_variables", json!({"variables": {"accent": "#cba6f7"}}));
    let ab = call(h, "create_artboard", json!({"name": "Home", "styles": {"width": 390, "height": 844}}));
    let list: String = (0..rows).map(|i| format!("<p layer-name=\"Row {i}\">Row number {i}</p>")).collect();
    call(
        h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": format!("<div layer-name=\"Button\" style=\"background:var(--accent); padding:8px\"><span>Save</span></div><div layer-name=\"List\">{list}</div>")}),
    );
    ab
}

#[test]
fn budget_get_context_is_a_small_overview_of_the_target() {
    let mut h = host();
    let ab = screen_with_rows(&mut h, 40);
    let raw = call_blocks(&mut h, "get_context", json!({"id": ab["id"]}));
    assert_eq!(raw.len(), 1);
    let ctx: Value = serde_json::from_str(&raw[0]).unwrap();
    assert_eq!(ctx["target"]["name"], "Home");
    assert_eq!(ctx["target"]["size"], "390x844");
    let outline = ctx["outline"].as_array().unwrap();
    assert_eq!(outline.iter().map(|o| o["name"].as_str().unwrap()).collect::<Vec<_>>(), ["Button", "List"]);
    let list = &outline[1];
    assert_eq!(list["children"], 40);
    assert!(list["estimatedTokens"].as_u64().unwrap() >= 40, "{list}");
    assert!(list["items"].as_array().unwrap().len() <= 25, "long levels are capped: {list}");
    let var = &ctx["variables"][0];
    assert_eq!((var["name"].as_str(), var["value"].as_str()), (Some("accent"), Some("#cba6f7")), "{ctx}");
    assert!(ctx["collections"].is_array() && ctx["components"].as_array().unwrap().is_empty());
    assert!(ctx["lint"]["first"].as_array().unwrap().len() <= 5);
    assert!(raw[0].len() / 4 < 2000, "{} tokens", raw[0].len() / 4);

    // Without an id it follows the selection, and without one it covers the page.
    let page = call(&mut h, "get_context", json!({}));
    assert!(page["target"].is_null());
    assert_eq!(page["outline"][0]["name"], "Home");
    call(&mut h, "select_nodes", json!({"nodeIds": [ab["id"]]}));
    let sel = call(&mut h, "get_context", json!({}));
    assert_eq!(sel["target"]["name"], "Home");
    assert_eq!(sel["selection"][0]["name"], "Home");
}

#[test]
fn budget_truncated_reads_stay_valid_and_page_on() {
    let mut h = host();
    let ab = screen_with_rows(&mut h, 120);
    let all = call_blocks(&mut h, "get_tree_summary", json!({"nodeId": ab["id"], "depth": 4}));
    let meta: Value = serde_json::from_str(&all[1]).unwrap();
    assert!(meta["estimatedTokens"].as_u64().unwrap() > 100 && meta.get("truncated").is_none(), "{meta}");

    // Cut small, then follow nextOffset until the whole tree is back, line for line.
    let mut got = String::new();
    let mut offset = 0u64;
    loop {
        let r = call_blocks(&mut h, "get_tree_summary", json!({"nodeId": ab["id"], "depth": 4, "maxTokens": 100, "offset": offset}));
        assert!(r[0].ends_with('\n') && r[0].len() <= 400 + 100, "cut on a line end: {}", r[0].len());
        got += &r[0];
        let meta: Value = serde_json::from_str(&r[1]).unwrap();
        match meta["nextOffset"].as_u64() {
            Some(next) => {
                assert_eq!(meta["truncated"], true);
                assert!(next > offset, "{meta}");
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(got, all[0]);

    // JSON reads drop whole elements and report it.
    let kids = call(&mut h, "get_children", json!({"nodeId": ab["id"], "maxTokens": 10_000}));
    assert!(kids.get("truncated").is_none() && kids["estimatedTokens"].as_u64().unwrap() > 0);
    let list = kids["children"][1]["id"].clone();
    let cut = call(&mut h, "get_node_info", json!({"nodeId": list, "maxTokens": 100}));
    assert_eq!(cut["truncated"], true);
    assert!(cut["childIds"].as_array().unwrap().len() < 120 && cut["estimatedTokens"].as_u64().unwrap() > 100, "{cut}");
    let spec = call(&mut h, "get_design_spec", json!({"nodeId": ab["id"], "format": "json", "maxTokens": 300}));
    assert_eq!(spec["truncated"], true, "{spec}");
    let code = call_blocks(&mut h, "get_jsx", json!({"nodeId": ab["id"], "maxTokens": 100}));
    assert_eq!(serde_json::from_str::<Value>(&code[1]).unwrap()["truncated"], true);
}

#[test]
fn reading_tools_describe_the_page() {
    let mut h = host();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Desktop", "styles": {"width": 1440, "height": 900}}),
    );
    call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": "<nav layer-name=\"Nav\" style=\"display:flex; gap:24px; padding:20px\"><span>Home</span><span>Pricing</span><span>Get started</span></nav>"}),
    );
    let info = call(&mut h, "get_basic_info", json!({}));
    assert_eq!(info["fileName"], "MCP test");
    assert_eq!(info["artboards"][0]["name"], "Desktop");
    assert_eq!(info["artboards"][0]["width"], 1440.0);
    assert_eq!(info["pages"].as_array().unwrap().len(), 1);

    let tree = call(
        &mut h,
        "get_tree_summary",
        json!({"nodeId": "root", "depth": 5}),
    );
    let tree = tree.as_str().unwrap();
    assert!(
        tree.contains("Frame \"Desktop\"") && tree.contains("\"Get started\""),
        "{tree}"
    );

    let found = call(&mut h, "find_nodes", json!({"textValue": "get *"}));
    assert_eq!(found["count"], 1, "{found}");
    let span = found["results"][0]["id"].clone();
    let n = call(&mut h, "get_node_info", json!({"nodeId": span}));
    assert_eq!(n["type"], "Text");
    assert_eq!(n["text"], "Get started");

    let styles = call(
        &mut h,
        "get_computed_styles",
        json!({"nodeIds": [ab["id"]]}),
    );
    let s = &styles[ab["id"].as_str().unwrap()];
    assert_eq!(s["width"], "1440px");
    assert_eq!(s["flexDirection"], "column");
    assert_eq!(s["backgroundColor"], "#ffffff");

    let jsx = call(&mut h, "get_jsx", json!({"nodeId": ab["id"]}));
    let jsx = jsx.as_str().unwrap();
    assert!(
        jsx.contains("<div data-name=\"Nav\"") && jsx.contains(">Pricing</span>"),
        "{jsx}"
    );

    let by_style = call(
        &mut h,
        "find_nodes",
        json!({"filters": [{"styleName": "gap", "styleValue": "24px"}]}),
    );
    assert_eq!(by_style["count"], 1, "{by_style}");
}

#[test]
fn editing_tools_update_move_duplicate_and_delete() {
    let mut h = host();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "A", "styles": {"width": "400px", "height": "fit-content"}}),
    );
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": "<div style=\"padding:8px\"><p>One</p></div><div style=\"padding:8px\"><p>Two</p></div>"}),
    );
    let (a, b) = (
        r["createdNodeIds"][0].clone(),
        r["createdNodeIds"][1].clone(),
    );
    let ab_n = num(&ab["id"]);
    assert_eq!(
        h.ed.doc.get(ab_n).unwrap().layout_child.sizing[1],
        Sizing::Hug
    );

    let up = call(
        &mut h,
        "update_styles",
        json!({"updates": [{"nodeIds": [a.clone()], "styles": {"backgroundColor": "linear-gradient(90deg, red, blue)", "boxShadow": "0 1px 2px black", "textShadow": "1px 1px red", "sketch": true, "roughness": 2}}]}),
    );
    assert_eq!(up["updated"], 1);
    assert_eq!(
        up["ignoredStyles"][a.as_str().unwrap()],
        json!(["textShadow"])
    );
    let an = h.ed.doc.get(num(&a)).unwrap();
    assert!(matches!(an.style.fills[0], Paint::Linear { .. }));
    assert_eq!(an.style.rough.unwrap().roughness, 2.0);

    // Text edits refit the layer.
    let one = h.ed.doc.get(num(&a)).unwrap().children[0];
    let key = exec::node_key(&h.ed, 0, one);
    let w0 = h.ed.doc.get(one).unwrap().rect.w;
    call(
        &mut h,
        "set_text_content",
        json!({"updates": [{"nodeId": key, "textContent": "One, now longer"}]}),
    );
    assert_eq!(
        h.ed.doc.get(one).unwrap().kind.text().unwrap(),
        "One, now longer"
    );
    let _ = w0;

    // Move b before a, then duplicate a, then delete the copy.
    call(
        &mut h,
        "move_nodes",
        json!({"moves": [{"nodeId": b, "before": a}]}),
    );
    let kids = h.ed.doc.get(ab_n).unwrap().children.clone();
    assert_eq!(kids, vec![num(&b), num(&a)]);
    let d = call(&mut h, "duplicate_nodes", json!({"nodes": [{"id": a}]}));
    let copy = d["duplicates"][0]["newId"].clone();
    assert_eq!(
        d["duplicates"][0]["descendantIdMap"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(h.ed.doc.get(ab_n).unwrap().children.len(), 3);
    call(
        &mut h,
        "rename_nodes",
        json!({"updates": [{"nodeId": copy, "name": "Copy"}]}),
    );
    assert_eq!(h.ed.doc.get(num(&copy)).unwrap().name, "Copy");
    let del = call(&mut h, "delete_nodes", json!({"nodeIds": [copy, "nope:1"]}));
    assert_eq!(del["deleted"], 1);
    h.ed.doc.validate().unwrap();

    // Replace swaps a node for new markup in the same slot.
    let rep = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": b, "mode": "replace", "html": "<p>Replaced</p>"}),
    );
    let new = num(&rep["createdNodeIds"][0]);
    assert_eq!(h.ed.doc.get(ab_n).unwrap().children[0], new);
}

#[test]
fn pages_projects_and_whiteboard_shapes() {
    let mut h = host();
    let p = call(&mut h, "create_page", json!({"name": "Ideas"}));
    assert_eq!(h.ed.active_page, 0, "the user's page doesn't change");
    let page = p["pageId"].as_str().unwrap().to_string();
    // Shapes on the other page, bound by an arrow.
    let a = call(
        &mut h,
        "create_shape",
        json!({"type": "rectangle", "x": 0, "y": 0, "width": 120, "height": 80, "pageId": page, "styles": {"sketch": true, "fillStyle": "cross-hatch", "backgroundColor": "#ffec99"}}),
    );
    let b = call(
        &mut h,
        "create_shape",
        json!({"type": "ellipse", "x": 300, "y": 0, "width": 100, "height": 100, "pageId": page}),
    );
    let arrow = call(
        &mut h,
        "create_shape",
        json!({"type": "arrow", "startNodeId": a["id"], "endNodeId": b["id"], "pageId": page}),
    );
    let sticky = call(
        &mut h,
        "create_shape",
        json!({"type": "sticky", "text": "Ship it", "x": 0, "y": 200, "pageId": page}),
    );
    assert!(
        h.ed.doc.nodes.is_empty(),
        "nothing landed on the user's page"
    );
    let doc = h.ed.page_doc(1);
    doc.validate().unwrap();
    assert_eq!(doc.nodes.len(), 4);
    let NodeKind::Connector { start, end, .. } = &doc.get(num(&arrow["id"])).unwrap().kind else {
        panic!()
    };
    assert!(start.is_some() && end.is_some());
    assert_eq!(
        doc.get(num(&sticky["id"])).unwrap().kind.text().unwrap(),
        "Ship it"
    );
    assert!(doc.get(num(&a["id"])).unwrap().style.rough.is_some());

    // Undo on that page only.
    call(&mut h, "undo", json!({"pageId": page}));
    assert_eq!(h.ed.page_doc(1).nodes.len(), 3);

    // Projects: list, create, open.
    call(
        &mut h,
        "rename_pages",
        json!({"updates": [{"pageId": page, "name": "Board"}]}),
    );
    call(&mut h, "finish_working_on_nodes", json!({}));
    let made = call(&mut h, "create_file", json!({"name": "Second"}));
    let list = call(&mut h, "list_resources", json!({}));
    assert_eq!(list["items"].as_array().unwrap().len(), 2);
    let opened = call(&mut h, "open_file", json!({"fileId": made["fileId"]}));
    assert_eq!(opened["fileName"], "Second");
    // Reopen the first: the work was saved, pages and names included.
    let first = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "MCP test")
        .unwrap()["id"]
        .clone();
    let info = call(&mut h, "open_file", json!({"fileId": first}));
    assert_eq!(info["pages"][1]["name"], "Board");
    assert_eq!(info["pages"][1]["nodeCount"], 3);
    // A fileId that isn't open is refused rather than silently editing another project.
    let err = exec::call(&mut h, "get_basic_info", &json!({"fileId": made["fileId"]})).unwrap_err();
    assert!(err.contains("open_file"), "{err}");
}

/// A 120px card with a heading and a paragraph; returns the card's id.
fn card(h: &mut exec::SessionHost) -> Value {
    let ab = call(h, "create_artboard", json!({"name": "Hero", "styles": {"width": "200px", "height": "400px", "padding": "10px", "gap": 8}}));
    let r = call(
        h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": r#"<div layer-name="Card" style="display:flex; flex-direction:column; width:120px; gap:8px; background:#fff"><h1 layer-name="Title" style="font-size:20px">Hello</h1><p layer-name="Body" style="font-size:14px">Short copy</p></div>"#}),
    );
    r["createdNodeIds"][0].clone()
}

fn texts(h: &exec::SessionHost) -> Vec<(papercraft::model::NodeId, String, f64)> {
    let mut v: Vec<_> = h.ed.doc.nodes.values().filter_map(|n| n.kind.text().map(|t| (n.id, t.to_string(), n.rect.h))).collect();
    v.sort_by_key(|t| t.0);
    v
}

fn text_of_layer(h: &exec::SessionHost, name: &str) -> (String, f64) {
    let n = h.ed.doc.nodes.values().find(|n| n.name == name).unwrap();
    (n.kind.text().unwrap().to_string(), n.rect.h)
}

#[test]
fn a_text_bound_to_a_string_variable_follows_the_mode_and_reflows() {
    let mut h = host();
    let card = card(&mut h);
    let title = h.ed.doc.nodes.values().find(|n| n.name == "Title").unwrap().id;
    let (base_h, card_h) = (text_of_layer(&h, "Title").1, h.ed.doc.get(num(&card)).unwrap().rect.h);
    call(&mut h, "set_variables", json!({"variables": {"greeting": {"value": "Hello", "type": "string", "collection": "Copy", "mode": "en"}}}));
    call(&mut h, "set_variables", json!({"variables": {"greeting": {"value": "Bem-vindo de volta a todos os nossos clientes", "type": "string", "collection": "Copy", "mode": "pt"}}}));
    call(&mut h, "set_variable_mode", json!({"mode": "en"}));
    call(&mut h, "bind_variable", json!({"nodeIds": [format!("{}", title.0)], "field": "text", "variable": "greeting"}));
    assert_eq!(text_of_layer(&h, "Title").0, "Hello");

    call(&mut h, "set_variable_mode", json!({"mode": "pt"}));
    let (t, h2) = text_of_layer(&h, "Title");
    assert_eq!(t, "Bem-vindo de volta a todos os nossos clientes");
    assert!(h2 > base_h, "the longer text wraps and grows: {h2} vs {base_h}");
    assert!(h.ed.doc.get(num(&card)).unwrap().rect.h > card_h, "the hugging card grows with it");

    call(&mut h, "set_variable_mode", json!({"mode": "en"}));
    assert_eq!(text_of_layer(&h, "Title"), ("Hello".to_string(), base_h));

    // Typing into a bound text replaces the binding, as a new colour replaces a text-color one.
    call(&mut h, "set_text_content", json!({"updates": [{"nodeId": format!("{}", title.0), "textContent": "Typed"}]}));
    call(&mut h, "set_variable_mode", json!({"mode": "pt"}));
    assert_eq!(text_of_layer(&h, "Title").0, "Typed");
}

#[test]
fn extracted_strings_are_translated_with_one_call_and_listed_when_missing() {
    let mut h = host();
    card(&mut h);
    let before = texts(&h);
    let ex = call(&mut h, "set_strings", json!({"extract": true, "mode": "en"}));
    assert_eq!(ex["strings"]["hero/card/title"], "Hello", "{ex}");
    assert_eq!(ex["strings"]["hero/card/body"], "Short copy");
    assert_eq!(texts(&h), before, "extracting leaves the page as it is");
    let again = call(&mut h, "set_strings", json!({"extract": true}));
    assert_eq!(again["strings"], json!({}), "bound text is not extracted twice");

    let got = call(&mut h, "get_strings", json!({}));
    assert_eq!(got["strings"]["hero/card/title"], json!({"en": "Hello"}));
    assert_eq!(got["unbound"], json!({}));

    call(&mut h, "set_strings", json!({"collection": "Strings", "mode": "pt", "strings": {"hero/card/title": "Ola"}}));
    let got = call(&mut h, "get_strings", json!({}));
    assert_eq!(got["missing"], json!({"pt": ["hero/card/body"]}));
    let todo = call(&mut h, "get_strings", json!({"missing": "pt"}));
    assert_eq!(todo, json!({"mode": "pt", "base": "en", "missing": {"hero/card/body": "Short copy"}}));

    assert_eq!(text_of_layer(&h, "Title").0, "Hello", "set_strings does not switch the mode");
    call(&mut h, "set_variable_mode", json!({"mode": "pt"}));
    assert_eq!(text_of_layer(&h, "Title").0, "Ola");
    assert_eq!(text_of_layer(&h, "Body").0, "Short copy", "an untranslated key shows the base text");
    call(&mut h, "set_variable_mode", json!({"mode": "en"}));
    assert_eq!(text_of_layer(&h, "Title").0, "Hello");
}

#[test]
fn a_pseudo_locale_is_longer_and_bracketed() {
    let mut h = host();
    card(&mut h);
    call(&mut h, "set_strings", json!({"extract": true, "mode": "en"}));
    call(&mut h, "set_strings", json!({"collection": "Strings", "mode": "xx", "pseudo": true}));
    call(&mut h, "set_variable_mode", json!({"mode": "xx"}));
    let (t, _) = text_of_layer(&h, "Body");
    assert!(t.starts_with("[Shórt çópý") && t.ends_with(']'), "{t}");
    assert!(t.chars().count() as f64 >= "Short copy".chars().count() as f64 * 1.35, "{t}");
    assert!(call(&mut h, "get_strings", json!({"missing": "xx"}))["missing"].as_object().unwrap().is_empty());
}

#[test]
fn a_pseudo_locale_cannot_overwrite_the_base_mode_and_an_empty_translation_counts() {
    let mut h = host();
    card(&mut h);
    call(&mut h, "set_strings", json!({"extract": true, "mode": "en"}));
    let e = exec::call(&mut h, "set_strings", &json!({"collection": "Strings", "mode": "en", "pseudo": true})).unwrap_err();
    assert!(e.contains("base mode"), "{e}");
    assert_eq!(call(&mut h, "get_strings", json!({}))["strings"]["hero/card/body"], json!({"en": "Short copy"}));

    call(&mut h, "set_strings", json!({"collection": "Strings", "mode": "pt", "strings": {"hero/card/body": ""}}));
    let got = call(&mut h, "get_strings", json!({}));
    assert_eq!(got["missing"], json!({"pt": ["hero/card/title"]}), "an explicit empty string is a value");
}

#[test]
fn bad_string_calls_are_errors_and_apply_nothing() {
    let mut h = host();
    card(&mut h);
    call(&mut h, "set_strings", json!({"extract": true, "mode": "en"}));
    call(&mut h, "set_variables", json!({"variables": {"gap": {"value": 8, "type": "number", "collection": "Strings"}}}));
    let before = h.ed.design.clone();
    let texts_before = texts(&h);

    let err = |h: &mut exec::SessionHost, args: Value| exec::call(h, "set_strings", &args).unwrap_err();
    let e = err(&mut h, json!({"collection": "Strings", "mode": "pt", "strings": {"hero/card/title": "Ola", "nope": "x", "nada": "y"}}));
    assert!(e.contains("nope") && e.contains("nada") && !e.contains("hero/card/title"), "{e}");
    let e = err(&mut h, json!({"collection": "Strings", "mode": "pt", "strings": {"hero/card/title": "Ola", "gap": "x"}}));
    assert!(e.contains("not string variables: gap"), "{e}");
    let e = err(&mut h, json!({"collection": "Missing", "mode": "pt", "strings": {"hero/card/title": "Ola"}}));
    assert!(e.contains("no collection `Missing`"), "{e}");
    let body = format!("{}", h.ed.doc.nodes.values().find(|n| n.name == "Body").unwrap().id.0);
    let e = exec::call(&mut h, "bind_variable", &json!({"nodeIds": [body], "field": "text", "variable": "gap"})).unwrap_err();
    assert!(e.contains("needs a String variable"), "{e}");
    assert_eq!(h.ed.design, before, "no mode was created and no value set");
    assert_eq!(texts(&h), texts_before);
}

#[test]
fn extract_is_one_undo_step_and_an_old_page_is_untouched() {
    let mut h = host();
    card(&mut h);
    let doc_before = h.ed.doc.clone();
    let design_before = h.ed.design.clone();
    call(&mut h, "set_strings", json!({"extract": true, "mode": "en"}));
    assert!(h.ed.doc.nodes.values().any(|n| n.bindings.contains_key("text")));
    call(&mut h, "undo", json!({}));
    assert_eq!(h.ed.doc, doc_before);
    assert_eq!(h.ed.design, design_before);

    let json = serde_json::to_string(&doc_before).unwrap();
    let back: papercraft::model::Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back, doc_before, "a page with no text binding round-trips");
}

#[test]
fn errors_are_reported_not_panics() {
    let mut h = host();
    for (name, args) in [
        ("get_node_info", json!({"nodeId": "999"})),
        (
            "write_html",
            json!({"targetNodeId": "1", "mode": "insert-children", "html": "<p>x</p>"}),
        ),
        (
            "create_artboard",
            json!({"name": "x", "styles": {"width": "10px"}}),
        ),
        ("move_nodes", json!({"moves": [{"nodeId": "1"}]})),
        ("nonexistent_tool", json!({})),
        ("get_guide", json!({"topic": "nope"})),
    ] {
        assert!(
            exec::call(&mut h, name, &args).is_err(),
            "{name} should fail"
        );
    }
}

#[test]
fn screenshot_returns_png() {
    if papercraft::render::gpu::Headless::new().is_err() {
        eprintln!("no GPU adapter; skipping");
        return;
    }
    let mut h = host();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "S", "styles": {"width": "200px", "height": "100px", "backgroundColor": "#ff0000"}}),
    );
    let outs = exec::call(
        &mut h,
        "get_screenshot",
        &json!({"nodeId": ab["id"], "scale": 2}),
    )
    .unwrap();
    let Out::Image { data, mime } = &outs[0] else {
        panic!()
    };
    assert_eq!(mime, "image/png");
    let img = image::load_from_memory(data).unwrap().to_rgba8();
    assert_eq!(img.dimensions(), (400, 200));
    let p = img.get_pixel(200, 100).0;
    assert!(p[0] > 240 && p[1] < 20 && p[2] < 20, "{p:?}");
}

// ------------------------------------------------------------------------------ end to end

/// Held while a test relies on a port staying unused, and while `spawn_app` binds one: the OS
/// may hand a just-freed ephemeral port to the next `bind(0)` in this process.
static PORT_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Serve MCP on a free port from a thread that owns a headless session, like the app does.
/// The worker hands back the final document and undo depth when the server stops.
fn spawn_app() -> (
    mcp::server::Running,
    std::thread::JoinHandle<(papercraft::model::Document, usize)>,
) {
    let (tx, rx) = std::sync::mpsc::channel::<mcp::Call>();
    let worker = std::thread::spawn(move || {
        let mut h = host();
        mcp::serve_host(&mut h, rx);
        (h.ed.doc.clone(), h.ed.history.undo_len())
    });
    let tx = std::sync::Mutex::new(tx);
    let _bind = PORT_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let running = mcp::server::spawn(
        0,
        std::sync::Arc::new(move |c| tx.lock().unwrap().send(c).is_ok()),
    )
    .unwrap();
    (running, worker)
}

fn args(v: Value) -> CallToolRequestParams {
    let mut p = CallToolRequestParams::new("");
    if let Value::Object(m) = v {
        p.arguments = Some(m);
    }
    p
}

fn tool(name: &str, v: Value) -> CallToolRequestParams {
    let mut p = args(v);
    p.name = name.to_string().into();
    p
}

fn text_of(r: &CallToolResult) -> Value {
    assert_ne!(r.is_error, Some(true), "{r:?}");
    let t = r
        .content
        .iter()
        .find_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .unwrap();
    serde_json::from_str(&t).unwrap_or(Value::String(t))
}

/// What an agent does: draw a frame with text and look at it.
async fn draw_frame_with_text<S: rmcp::service::Service<rmcp::RoleClient>>(
    client: &rmcp::service::RunningService<rmcp::RoleClient, S>,
) -> String {
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "write_html"));
    let ab = text_of(&client.call_tool(tool("create_artboard", json!({"name": "Hello", "styles": {"width": "320px", "height": "200px", "padding": "24px"}}))).await.unwrap());
    let id = ab["id"].as_str().unwrap().to_string();
    let w = text_of(
        &client
            .call_tool(tool("write_html", json!({"targetNodeId": id, "mode": "insert-children", "html": "<h1 style=\"color:#2563eb\">Hello from MCP</h1>"})))
            .await
            .unwrap(),
    );
    assert_eq!(w["createdNodeIds"].as_array().unwrap().len(), 1);
    let tree = text_of(
        &client
            .call_tool(tool("get_tree_summary", json!({"nodeId": id})))
            .await
            .unwrap(),
    );
    assert!(tree.as_str().unwrap().contains("Hello from MCP"), "{tree}");
    // Errors come back as tool errors, not protocol failures.
    let bad = client
        .call_tool(tool("get_node_info", json!({"nodeId": "nope"})))
        .await
        .unwrap();
    assert_eq!(bad.is_error, Some(true));
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_client_draws_a_frame_with_text() {
    let (running, worker) = spawn_app();
    let transport = rmcp::transport::StreamableHttpClientTransport::from_uri(running.url());
    let client = ().serve(transport).await.unwrap();
    let info = client.peer_info().unwrap();
    assert_eq!(info.server_info.as_ref().unwrap().name, "PaperCraft");
    assert!(
        info.instructions
            .as_deref()
            .unwrap_or("")
            .contains("write_html")
    );
    let id = draw_frame_with_text(&client).await;
    client.cancel().await.unwrap();
    running.stop();
    // The edit landed in the editor's document, through its undo history.
    let (doc, undo_len) = worker.join().unwrap();
    let frame = doc.get(num(&json!(id))).unwrap();
    assert!(frame.is_frame());
    let text = doc.get(frame.children[0]).unwrap();
    assert_eq!(text.kind.text().unwrap(), "Hello from MCP");
    assert_eq!(undo_len, 2);
}

/// `?tools=core` lists the core tools plus search_tools and call_tool; the others still run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_core_session_reaches_every_tool_through_search_and_call() {
    let (running, worker) = spawn_app();
    let transport = rmcp::transport::StreamableHttpClientTransport::from_uri(format!("{}?tools=core", running.url()));
    let client = ().serve(transport).await.unwrap();
    let names: Vec<String> = client.list_all_tools().await.unwrap().iter().map(|t| t.name.to_string()).collect();
    assert_eq!(names.len(), mcp::core_tools::CORE.len() + 2);
    assert!(names.iter().any(|n| n == "call_tool") && !names.iter().any(|n| n == "create_shape"));
    let found = text_of(&client.call_tool(tool("search_tools", json!({"query": "create_shape"}))).await.unwrap());
    assert!(found["tools"][0]["inputSchema"]["properties"].is_object(), "{found}");
    let page = text_of(&client.call_tool(tool("call_tool", json!({"name": "create_page", "arguments": {"name": "Moodboard"}}))).await.unwrap());
    assert!(page.is_object(), "{page}");
    let wrapped = client.call_tool(tool("call_tool", json!({"name": "rename_pages", "arguments": {"renames": []}}))).await.unwrap();
    assert!(wrapped.content.iter().any(|c| matches!(c, ContentBlock::Text(_))), "the tool's own result or error comes back");
    let full = ().serve(rmcp::transport::StreamableHttpClientTransport::from_uri(running.url())).await.unwrap();
    assert!(full.list_all_tools().await.unwrap().len() > names.len() * 2, "without the query every tool is listed");
    for c in [client, full] {
        c.cancel().await.unwrap();
    }
    running.stop();
    worker.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_rejects_browsers_and_unknown_routes() {
    let (running, _worker) = spawn_app();
    let base = format!("http://127.0.0.1:{}", running.port);
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;
    let http = reqwest::Client::new();
    let r = http
        .post(format!("{base}/mcp"))
        .header("content-type", "application/json")
        .header("origin", "https://evil.example")
        .body(init)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    // Missing Accept header is repaired.
    let r = http
        .post(format!("{base}/mcp"))
        .header("content-type", "application/json")
        .body(init)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{:?}", r.text().await);
    let r = http
        .get(format!("{base}/.well-known/oauth-authorization-server"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    assert!(r.text().await.unwrap().contains("/mcp"));
    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_bridge_relays_to_the_app() {
    let (running, _worker) = spawn_app();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_papercraft-mcp"));
    cmd.env("PAPERCRAFT_MCP_PORT", running.port.to_string());
    let child = rmcp::transport::TokioChildProcess::new(cmd).unwrap();
    let client = ().serve(child).await.unwrap();
    draw_frame_with_text(&client).await;
    client.cancel().await.unwrap();
    running.stop();
}

/// Protocol 2026-07-28 clients (e.g. Claude Code) skip `initialize`, send the revision in each
/// request's `_meta`, and reject `tools/call` results without `resultType`. The app's HTTP leg
/// answers the bridge in an older revision, so the bridge must add it back.
#[test]
fn stdio_bridge_results_carry_result_type_for_2026_clients() {
    use std::io::{BufRead, Write};
    let (running, _worker) = spawn_app();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_papercraft-mcp"))
        .env("PAPERCRAFT_MCP_PORT", running.port.to_string())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let meta = r#""_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"raw","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}"#;
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{{"name":"create_artboard","arguments":{{"name":"Raw","styles":{{"width":"100px","height":"100px"}}}},{meta}}}}}"#
    )
    .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    running.stop();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["id"], 7, "{line}");
    assert_eq!(v["result"]["resultType"], "complete", "{line}");
    assert!(
        v["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("\"Raw\""),
        "{line}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_bridge_lists_tools_without_the_app() {
    // A port nothing listens on: no other test may bind one until this test is done.
    let _guard = PORT_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_papercraft-mcp"));
    cmd.env("PAPERCRAFT_MCP_PORT", port.to_string());
    let client = ().serve(rmcp::transport::TokioChildProcess::new(cmd).unwrap()).await.unwrap();
    assert!(client.list_all_tools().await.unwrap().len() >= 30);
    let r = client
        .call_tool(tool("get_basic_info", json!({})))
        .await
        .unwrap();
    assert_eq!(r.is_error, Some(true));
    let ContentBlock::Text(t) = &r.content[0] else {
        panic!()
    };
    assert!(t.text.contains("not running"), "{}", t.text);
    client.cancel().await.unwrap();
}

/// A design written the way an agent would, rendered for the docs (`milestone7_mcp.png`).
#[test]
fn agent_design_screenshot() {
    if papercraft::render::gpu::Headless::new().is_err() {
        eprintln!("no GPU adapter; skipping");
        return;
    }
    let mut h = host();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Sign in", "styles": {"width": "390px", "height": "fit-content", "padding": "32px 24px", "gap": "24px", "backgroundColor": "#f8fafc"}}),
    );
    let id = ab["id"].clone();
    for html in [
        r#"<div layer-name="Header" style="display:flex; align-items:center; gap:12px">
             <div style="width:40px; height:40px; border-radius:12px; background:linear-gradient(135deg, #6366f1, #ec4899)"></div>
             <span style="font-size:20px; font-weight:700; color:#0f172a">Paper RS</span></div>"#,
        r#"<div layer-name="Intro" style="display:flex; flex-direction:column; gap:8px">
             <h1 style="font-size:28px; color:#0f172a; line-height:34px">Welcome back</h1>
             <p style="font-size:15px; color:#475569; line-height:22px">Sign in to pick up <b>where you left off</b>. Your boards sync locally.</p></div>"#,
        r#"<div layer-name="Form" style="display:flex; flex-direction:column; gap:12px; padding:20px; background:#ffffff; border:1px solid #e2e8f0; border-radius:16px">
             <label style="font-size:13px; font-weight:600; color:#334155">Email</label>
             <div style="padding:12px 14px; border:1px solid #cbd5e1; border-radius:10px; color:#94a3b8; font-size:15px">you@example.com</div>
             <label style="font-size:13px; font-weight:600; color:#334155">Password</label>
             <div style="padding:12px 14px; border:1px solid #cbd5e1; border-radius:10px; color:#94a3b8; font-size:15px">••••••••</div>
             <div style="display:flex; justify-content:center; align-items:center; height:48px; border-radius:10px; background:#4f46e5; color:#ffffff; font-size:16px; font-weight:600">Continue</div></div>"#,
        r#"<div style="display:flex; justify-content:space-between"><span style="font-size:14px; color:#4f46e5">Forgot password?</span><span style="font-size:14px; color:#64748b">Create account</span></div>"#,
    ] {
        call(
            &mut h,
            "write_html",
            json!({"targetNodeId": id, "mode": "insert-children", "html": html}),
        );
    }
    h.ed.doc.validate().unwrap();
    let outs = exec::call(&mut h, "get_screenshot", &json!({"nodeId": id, "scale": 2})).unwrap();
    let Out::Image { data, .. } = &outs[0] else {
        panic!()
    };
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(dir.join("milestone7_mcp.png"), data).unwrap();
    let ab_node = h.ed.doc.get(num(&id)).unwrap();
    assert!(
        ab_node.rect.h > 400.0,
        "artboard hugs its content: {:?}",
        ab_node.rect
    );
}

/// What a shell-driven agent does with `papercraft-cli`: list tools, draw, read back, screenshot.
#[test]
fn cli_drives_the_running_app() {
    let (running, _worker) = spawn_app();
    let dir = std::env::temp_dir().join(format!("papercraft-cli-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    let cli = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_papercraft-cli"))
            .env("PAPERCRAFT_MCP_PORT", running.port.to_string())
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap()
    };
    let out = |o: &std::process::Output| String::from_utf8_lossy(&o.stdout).to_string();

    let list = cli(&["list"]);
    assert!(list.status.success() && out(&list).contains("write_html\t"));
    let schema = cli(&["schema", "create_artboard"]);
    assert!(out(&schema).contains("inputSchema"), "{}", out(&schema));
    assert!(!cli(&["schema", "nope"]).status.success());

    let ab = cli(&[
        "call",
        "create_artboard",
        r##"{"name":"CLI","styles":{"width":"200px","height":"100px","backgroundColor":"#ff0000"}}"##,
    ]);
    assert!(
        ab.status.success(),
        "{}",
        String::from_utf8_lossy(&ab.stderr)
    );
    let id = serde_json::from_str::<Value>(&out(&ab)).unwrap()["id"].clone();
    let shot = cli(&[
        "call",
        "get_screenshot",
        &json!({"nodeId": id}).to_string(),
        "--out",
        "a.png",
    ]);
    assert!(
        shot.status.success(),
        "{}",
        String::from_utf8_lossy(&shot.stderr)
    );
    assert!(out(&shot).lines().any(|l| l == "a.png"), "{}", out(&shot));
    let img = image::open(dir.join("a.png")).unwrap().to_rgba8();
    assert_eq!(img.dimensions(), (200, 100));

    let bad = cli(&["call", "get_node_info", r#"{"nodeId":"nope"}"#]);
    assert!(!bad.status.success(), "a tool error exits non-zero");
    let junk = cli(&["call", "get_node_info", "{not json"]);
    assert!(!junk.status.success());
    running.stop();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn delete_pages_guards_layers_and_the_last_page() {
    let mut h = host();
    let empty = call(&mut h, "create_page", json!({"name": "Scratch"}))["pageId"].clone();
    let full = call(&mut h, "create_page", json!({"name": "Work"}))["pageId"].clone();
    call(
        &mut h,
        "create_artboard",
        json!({"pageId": full, "name": "A", "styles": {"width": "100px", "height": "100px"}}),
    );
    // A page with layers needs `confirm`.
    let err = exec::call(&mut h, "delete_pages", &json!({"pageIds": [full]})).unwrap_err();
    assert!(err.contains("confirm"), "{err}");
    assert_eq!(h.ed.pages.len(), 3);
    // An empty page goes without it; with confirm the other follows.
    let done = call(&mut h, "delete_pages", json!({"pageIds": [empty]}));
    assert_eq!(done["deleted"], json!(["Scratch"]));
    call(
        &mut h,
        "delete_pages",
        json!({"pageIds": [full], "confirm": true}),
    );
    assert_eq!(h.ed.pages.len(), 1);
    // The last page stays.
    let last = h.ed.pages[0].id.clone();
    let err = exec::call(
        &mut h,
        "delete_pages",
        &json!({"pageIds": [last], "confirm": true}),
    )
    .unwrap_err();
    assert!(err.contains("at least one"), "{err}");
}

/// Two MCP sessions editing at once, plus a chat's CLI backend (`?chat=<id>` on the URL): each
/// gets its own run with its own counts, and reverting one leaves the others' nodes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parallel_sessions_get_their_own_runs() {
    use papercraft::runs::{self, RUN_IDLE_SECS};
    let (tx, rx) = std::sync::mpsc::channel::<mcp::Call>();
    let worker = std::thread::spawn(move || {
        let mut h = host();
        let mut t = runs::Tracker::default();
        while let Ok(c) = rx.recv() {
            let now = papercraft::project::format::now_ms();
            let _ = c.reply.send(t.call(&mut h, &c.client, &c.name, &c.args, now));
        }
        let dir = h.session.project.as_ref().unwrap().dir.clone();
        let later = papercraft::project::format::now_ms() + RUN_IDLE_SECS * 1000 + 1;
        t.tick(later, Some(&dir), &h.ed);
        let all = runs::list(&dir);
        // Revert B's run alone.
        if let Some(mut rb) = all.iter().find(|r| r.client == "Agent B").cloned() {
            runs::revert(&dir, &mut h.ed, &mut rb).unwrap();
        }
        let names: Vec<String> = h.ed.doc.root.iter().map(|id| h.ed.doc.get(*id).unwrap().name.clone()).collect();
        (all, names)
    });
    let tx = std::sync::Mutex::new(tx);
    let running = {
        let _bind = PORT_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        mcp::server::spawn(0, std::sync::Arc::new(move |c| tx.lock().unwrap().send(c).is_ok())).unwrap()
    };
    let connect = |name: &'static str, url: String| async move {
        let info = rmcp::model::ClientConfig::new(
            Default::default(),
            rmcp::model::Implementation::new(name, "1"),
        );
        info.serve(rmcp::transport::StreamableHttpClientTransport::from_uri(url))
            .await
            .unwrap()
    };
    let a = connect("Agent A", running.url()).await;
    let b = connect("Agent B", running.url()).await;
    let chat = connect("Claude Code", format!("{}?chat=c1", running.url())).await;
    let frame = |name: &str| tool("create_artboard", json!({"name": name, "styles": {"width": "100px", "height": "100px"}}));
    let start = text_of(&a.call_tool(tool("get_basic_info", json!({}))).await.unwrap())["rev"].clone();
    // Interleaved: A, B, A, chat.
    for (client, name) in [(&a, "A1"), (&b, "B1"), (&a, "A2"), (&chat, "C1")] {
        text_of(&client.call_tool(frame(name)).await.unwrap());
    }
    // The change log tells the sessions apart: A sees only what B and the chat changed.
    let others = text_of(&a.call_tool(tool("get_changes", json!({"since": start, "exclude": "self"}))).await.unwrap());
    let mut by: Vec<&str> = others["changes"].as_array().unwrap().iter().map(|g| g["by"].as_str().unwrap()).collect();
    by.sort();
    assert_eq!(by, ["Agent B", "Chat"], "{others}");
    for c in [a, b, chat] {
        c.cancel().await.unwrap();
    }
    running.stop();
    let (all, names) = worker.join().unwrap();
    let by = |client: &str| all.iter().find(|r| r.client == client).unwrap_or_else(|| panic!("no run of {client}: {all:?}")).clone();
    let (ra, rb, rc) = (by("Agent A"), by("Agent B"), by("Chat"));
    assert_eq!(all.len(), 3);
    assert_eq!((ra.calls, ra.added, ra.changed, ra.removed), (2, 2, 0, 0));
    assert_eq!((rb.calls, rb.added), (1, 1));
    assert_eq!((rc.calls, rc.added), (1, 1));
    assert_eq!(names, ["A1", "A2", "C1"], "only B's frame is gone");
}

#[test]
fn export_presentation_writes_a_deck_of_the_screens() {
    let mut h = host();
    let err = exec::call(&mut h, "export_presentation", &json!({})).unwrap_err();
    assert!(err.to_string().contains("no screens"), "{err}");
    call(&mut h, "create_artboard", json!({"name": "Sign in", "styles": {"width": 1440, "height": 900}}));
    let path = std::env::temp_dir().join(format!("papercraft-deck-{:016x}.pdf", fastrand::u64(..)));
    let out = call(&mut h, "export_presentation", json!({"title": "Deck", "author": "A", "path": path}));
    assert_eq!((out["screens"].as_u64(), out["pages"].as_u64()), (Some(1), Some(1)));
    let pdf = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    // Cover, contents, divider, the screen, next steps, end.
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 6"));
    let err = exec::call(&mut h, "export_presentation", &json!({"language": "xx", "path": path})).unwrap_err();
    assert!(err.to_string().contains("en, pt"), "{err}");
    call(&mut h, "export_presentation", json!({"language": "pt-BR", "path": path}));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn write_html_sizes_flex_items_as_a_browser_does() {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Rows", "styles": {"width": 800, "height": "fit-content", "display": "flex", "flexDirection": "column", "alignItems": "flex-start"}}));
    let out = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": "<div style=\"display:flex;width:fit-content\"><div style=\"flex:1;padding:8px\"><span>Two Sum</span></div></div>\
                        <div style=\"display:flex;width:400px\"><div style=\"flex:0 0 120px\">Fixed</div><div style=\"flex-basis:90px\">Basis</div></div>"}),
    );
    let width = |h: &mut exec::SessionHost, text: &str| {
        let found = call(h, "find_nodes", json!({"textValue": text}));
        let id = found["results"][0]["id"].clone();
        call(h, "get_node_info", json!({"nodeId": id}))["width"].as_f64().unwrap()
    };
    assert!(width(&mut h, "Two Sum") > 40.0, "a growing cell in a hugging row keeps its text on one line: {out}");
    assert_eq!(width(&mut h, "Fixed"), 120.0);
    assert_eq!(width(&mut h, "Basis"), 90.0);
}

#[test]
fn the_base_kits_build_their_components_and_layouts_clean() {
    // shadcn and the kits drawn with its markup in another library's palette and code.
    for (id, package) in [("mui", "@mui/material"), ("mantine", "@mantine/core"), ("bootstrap", "react-bootstrap"), ("chakra", "@chakra-ui/react")] {
        let mut h = host();
        let kit = call(&mut h, "apply_ui_kit", json!({"kit": id}));
        let layouts = kit["layouts"].as_array().unwrap();
        assert_eq!(layouts.len(), 5, "{id}");
        for root in std::iter::once(&kit["frame"]).chain(layouts.iter().map(|l| &l["id"])) {
            let lint = call(&mut h, "lint_design", json!({"pageId": kit["pageId"], "nodeId": root}));
            assert_eq!((lint["errors"].as_u64(), lint["warnings"].as_u64()), (Some(0), Some(0)), "{id} {root}: {}", lint["issues"]);
        }
        let button = &kit["components"].as_array().unwrap().iter().find(|c| c["name"] == "Button / Outline").unwrap()["id"];
        let jsx = call(&mut h, "get_jsx", json!({"nodeId": button}));
        assert!(jsx.to_string().contains(package), "{id} links its own library: {jsx}");
    }
    let mut h = host();
    let kit = call(&mut h, "apply_ui_kit", json!({"kit": "shadcn"}));
    let pid = kit["pageId"].clone();
    assert!(kit["components"].as_array().unwrap().len() >= 40, "{}", kit["components"]);
    let layouts = kit["layouts"].as_array().unwrap();
    assert_eq!(layouts.len(), 5);
    for root in std::iter::once(&kit["frame"]).chain(layouts.iter().map(|l| &l["id"])) {
        let lint = call(&mut h, "lint_design", json!({"pageId": pid, "nodeId": root}));
        assert_eq!((lint["errors"].as_u64(), lint["warnings"].as_u64()), (Some(0), Some(0)), "{root}: {}", lint["issues"]);
    }
    // An icon takes its <svg>'s `color: var(--…)`, not black.
    let found = call(&mut h, "find_nodes", json!({"pageId": pid, "textValue": "New item"}));
    let label = found["results"][0]["id"].clone();
    let button = call(&mut h, "get_node_info", json!({"nodeId": label}))["parentId"].clone();
    let icon = call(&mut h, "get_children", json!({"nodeId": button}))["children"][0]["id"].clone();
    let jsx = call(&mut h, "get_jsx", json!({"nodeId": icon}));
    assert!(!jsx.to_string().to_lowercase().contains("#000000"), "{jsx}");
}

#[test]
fn a_screen_keeps_its_height_and_scrolls_its_main_column() {
    let mut h = host();
    let rows: String = (0..40).map(|i| format!("<div style=\"display:flex;padding:16px\"><span>Order {i}</span></div>")).collect();
    let shell = |main_style: &str| {
        format!("<div layer-name=\"Shell\" style=\"display:flex\"><div layer-name=\"Sidebar\" style=\"display:flex;flex-direction:column;width:240px\"><span>Home</span></div><div layer-name=\"Main\" style=\"{main_style}\">{rows}</div></div>")
    };
    // A screen made to hug its content takes its device's height; a hugging shell fills it.
    let ab = call(&mut h, "create_artboard", json!({"name": "Orders", "styles": {"width": "1440px", "height": "fit-content"}}));
    assert!(ab["note"].as_str().is_some_and(|n| n.contains("900")), "{ab}");
    let out = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": shell("display:flex;flex-direction:column;flex-grow:1")}));
    assert!(out["warnings"].to_string().contains("Main"), "{out}");
    let size = |h: &mut exec::SessionHost, id: &Value| {
        let info = call(h, "get_node_info", json!({"nodeId": id}));
        let styles = call(h, "get_computed_styles", json!({"nodeIds": [id]}));
        (info["height"].as_f64().unwrap(), styles.to_string().contains("\"overflow\":\"auto\""))
    };
    let kids = call(&mut h, "get_children", json!({"nodeId": out["createdNodeIds"][0]}))["children"].clone();
    assert_eq!(size(&mut h, &ab["id"]).0, 900.0);
    assert_eq!(size(&mut h, &kids[0]["id"]), (900.0, false), "the sidebar stays");
    assert_eq!(size(&mut h, &kids[1]["id"]), (900.0, true), "the main column scrolls");
    // `min-height: 100vh` fills the screen instead of being ignored.
    let ab = call(&mut h, "create_artboard", json!({"name": "Vh", "styles": {"width": "1440px", "height": "900px", "left": "2000px"}}));
    let html = shell("display:flex;flex-direction:column;flex:1").replace("display:flex\"><div layer-name=\"Sidebar\"", "display:flex;min-height:100vh\"><div layer-name=\"Sidebar\"");
    let out = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": html}));
    assert_eq!(size(&mut h, &out["createdNodeIds"][0]).0, 900.0);
    // A board asked to grow still grows.
    let ab = call(&mut h, "create_artboard", json!({"name": "Landing", "styles": {"width": "1440px", "height": "fit-content", "overflow": "visible", "left": "4000px"}}));
    call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": rows}));
    assert!(size(&mut h, &ab["id"]).0 > 900.0);
}

#[test]
fn a_finished_design_is_offered_a_critique_and_keeps_approved_lessons() {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Home", "styles": {"width": "390px", "height": "844px"}}));
    call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<div style=\"padding:16px\">Hi</div>"}));
    let done = call(&mut h, "finish_working_on_nodes", json!({}));
    assert!(done["next"].as_str().is_some_and(|n| n.contains("critique_design")), "{done}");
    assert!(call(&mut h, "finish_working_on_nodes", json!({}))["next"].is_null(), "no edits, no second offer");

    let review = call(&mut h, "critique_design", json!({"nodeId": ab["id"]}));
    assert!(review["lint"]["errors"].is_u64() && review["steps"].as_array().unwrap().len() >= 5, "{review}");
    assert!(review["lessons"].as_array().unwrap().is_empty());
    assert_eq!(review["personas"].as_array().unwrap().len(), 8, "every reviewer by default");
    assert!(review["objective"].as_str().unwrap().starts_with("missing"), "no objective: ask for one");
    // The flow follows arrows between screens: Home -> Cart is fine, Cart leads nowhere, Help is never reached.
    let screen = |h: &mut exec::SessionHost, name: &str, x: u32| call(h, "create_artboard", json!({"name": name, "styles": {"width": "390px", "height": "844px", "left": format!("{x}px"), "top": "0px"}}))["id"].clone();
    let cart = screen(&mut h, "Cart", 600);
    screen(&mut h, "Help", 1200);
    call(&mut h, "create_shape", json!({"type": "arrow", "startNodeId": ab["id"], "endNodeId": cart}));
    let review = call(&mut h, "critique_design", json!({"personas": ["flow", "accessibility"], "objective": "Buy in two taps"}));
    assert_eq!(review["personas"].as_array().unwrap().len(), 2);
    assert_eq!(review["flow"]["links"], json!([["Home", "Cart"]]), "{}", review["flow"]);
    assert_eq!(review["flow"]["deadEnds"], json!(["Cart"]));
    assert_eq!(review["flow"]["unreached"], json!(["Help"]));
    assert!(exec::call(&mut h, "critique_design", &json!({"personas": ["boss"]})).unwrap_err().contains("no persona"));

    let added = call(&mut h, "design_lessons", json!({"add": {"rule": "Screens keep their device height", "why": "a page grew to 2000px"}}));
    let id = added["added"]["id"].clone();
    call(&mut h, "design_lessons", json!({"add": {"rule": "Headlines use the accent colour", "scope": "project"}}));
    let info = call(&mut h, "get_basic_info", json!({}));
    assert_eq!(info["lessons"].as_array().unwrap().len(), 2, "{info}");
    assert_eq!(call(&mut h, "critique_design", json!({}))["lessons"].as_array().unwrap().len(), 2);
    let left = call(&mut h, "design_lessons", json!({"remove": id}));
    assert_eq!(left["lessons"].as_array().unwrap().len(), 1, "{left}");
}

#[test]
fn claims_need_the_running_app() {
    // Claims fence agents off from each other, so they live in the app every agent talks to;
    // the full behaviour is in tests/swarm.rs.
    let mut h = host();
    for tool in ["claim_nodes", "release_claims", "list_claims"] {
        let err = exec::call(&mut h, tool, &json!({"ids": []})).unwrap_err();
        assert!(err.to_string().contains("running PaperCraft"), "{tool}: {err}");
    }
}

#[test]
fn a_handoff_point_reports_what_changed_and_the_bundle_rewrites_only_that() {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Sign in", "styles": {"width": 390, "height": 844, "display": "flex", "flexDirection": "column"}}));
    let r = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "html": "<p layer-name=\"Title\" style=\"font-size:24px\">Entrar</p><div layer-name=\"Box\" style=\"width:100px;height:40px;background:#ff0000\"></div>"}));
    let (title, bx) = (r["createdNodeIds"][0].clone(), r["createdNodeIds"][1].clone());
    assert!(exec::call(&mut h, "get_handoff_changes", &json!({"since": "v1"})).unwrap_err().contains("none"), "nothing marked yet");
    assert_eq!(call(&mut h, "mark_handoff", json!({"name": "v1"}))["screens"], 1);
    assert!(call(&mut h, "get_handoff_changes", json!({"since": "v1"})).as_str().unwrap().contains("No changes."));
    call(&mut h, "set_text_content", json!({"updates": [{"nodeId": title, "textContent": "Acessar"}]}));
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [bx], "styles": {"width": "120px"}}]}));
    let md = call(&mut h, "get_handoff_changes", json!({"since": "v1"})).as_str().unwrap().to_owned();
    assert!(md.contains("copy: text: Entrar → Acessar"), "{md}");
    assert!(md.contains("layout: size: 100 × 40 → 120 × 40"), "{md}");
    assert!(!md.contains("position"), "auto layout placed them, code does not: {md}");

    let dir = std::env::temp_dir().join(format!("papercraft-bundle-{:016x}", fastrand::u64(..)));
    let args = json!({"dir": dir, "since": "v1", "images": false});
    let first = call(&mut h, "export_handoff", args.clone());
    assert_eq!(first["screens"], 1, "{first}");
    let index = std::fs::read_to_string(dir.join("index.md")).unwrap();
    let page_dir = index.lines().filter(|l| l.starts_with("## ")).find_map(|l| l.split_once(" (")).unwrap().1.trim_end_matches(')').to_owned();
    let spec = format!("{page_dir}{}", index.lines().find(|l| l.starts_with("- ")).unwrap().rsplit(" · ").next().unwrap().split(" ~").next().unwrap());
    assert!(std::fs::read_to_string(dir.join(&spec)).unwrap().contains("Acessar"), "{spec}");
    assert!(std::fs::read_to_string(dir.join("changes.md")).unwrap().contains("Entrar → Acessar"));
    assert_eq!(call(&mut h, "export_handoff", args.clone())["written"], 0, "same design, same bytes");
    call(&mut h, "rename_nodes", json!({"updates": [{"nodeId": ab["id"], "name": "Login"}]}));
    let again = call(&mut h, "export_handoff", args);
    assert_eq!(again["removed"], 1, "the old spec goes: {again}");
    assert!(!dir.join(&spec).exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_changed_main_or_token_is_reported_once_with_the_screens_using_it() {
    let mut h = host();
    call(&mut h, "set_variables", json!({"variables": {"accent": "#cba6f7"}}));
    let ds = call(&mut h, "create_artboard", json!({"name": "Button", "styles": {"width": 120, "height": 40, "display": "flex", "background": "var(--accent)"}}));
    let label = call(&mut h, "write_html", json!({"targetNodeId": ds["id"], "html": "<p layer-name=\"Label\">Save</p>"}))["createdNodeIds"][0].clone();
    call(&mut h, "create_component", json!({"nodeId": ds["id"]}));
    for (name, x) in [("Form", 400), ("Settings", 900)] {
        let s = call(&mut h, "create_artboard", json!({"name": name, "styles": {"width": 390, "height": 844, "left": format!("{x}px"), "display": "flex"}}));
        call(&mut h, "create_instance", json!({"componentId": ds["id"], "parentId": s["id"]}));
    }
    call(&mut h, "mark_handoff", json!({"name": "v1"}));
    call(&mut h, "set_text_content", json!({"updates": [{"nodeId": label, "textContent": "Save all"}]}));
    call(&mut h, "set_variables", json!({"variables": {"accent": "#89b4fa"}}));
    let md = call(&mut h, "get_handoff_changes", json!({"since": "v1"})).as_str().unwrap().to_owned();
    assert!(md.contains("- --accent: #cba6f7 → #89b4fa · 2 screens: Form, Settings"), "{md}");
    assert!(md.contains("used on 2 screens: Form, Settings"), "{md}");
    assert_eq!(md.matches("Save → Save all").count(), 1, "the main says it once, its instances follow: {md}");
    assert!(!md.contains("## Screens"), "no screen needs its own work: {md}");
}

#[test]
fn a_non_finite_number_is_a_missing_one_and_the_page_still_round_trips() {
    let mut h = host();
    let r = call(&mut h, "create_shape", json!({"type": "rectangle", "x": "NaN", "y": "1e999", "width": "infpx", "height": 10}));
    let info = call(&mut h, "get_node_info", json!({"nodeId": r["id"]}));
    for k in ["x", "y", "width"] {
        assert!(info[k].as_f64().is_some_and(f64::is_finite), "{k}: {info}");
    }
    let saved = serde_json::to_string(h.ed.page_doc(0)).unwrap();
    serde_json::from_str::<papercraft::model::Document>(&saved).unwrap();
}

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("papercraft-out-{:016x}", fastrand::u64(..))).join(name)
}

#[test]
fn file_outputs_must_have_the_extension_the_tool_writes() {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Sign in", "styles": {"width": 1440, "height": 900}}));
    let docx = temp_path("thesis.docx");
    for (tool, args) in [
        ("export_presentation", json!({"path": docx})),
        ("export_combined_pdf", json!({"nodeIds": [ab["id"]], "path": docx})),
        ("get_screenshot", json!({"out": docx})),
    ] {
        if tool == "get_screenshot" && papercraft::render::gpu::Headless::new().is_err() {
            continue;
        }
        let err = exec::call(&mut h, tool, &args).unwrap_err();
        assert!(err.contains("must end in"), "{tool}: {err}");
    }
    assert!(!docx.exists());
    let pdf = temp_path("Deck.PDF");
    std::fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    call(&mut h, "export_presentation", json!({"path": pdf}));
    assert!(pdf.exists(), "the extension check ignores case");
    std::fs::remove_dir_all(pdf.parent().unwrap()).unwrap();
}

#[test]
fn folder_outputs_need_a_new_empty_or_earlier_export_folder() {
    let mut h = host();
    call(&mut h, "create_artboard", json!({"name": "Sign in", "styles": {"width": 390, "height": 844}}));
    let base = temp_path("x");
    let base = base.parent().unwrap();
    let theirs = base.join("theirs");
    std::fs::create_dir_all(&theirs).unwrap();
    std::fs::write(theirs.join("notes.txt"), "mine").unwrap();
    for (tool, key) in [("export_design_system", "path"), ("export_handoff", "dir")] {
        let err = exec::call(&mut h, tool, &json!({key: theirs, "images": false})).unwrap_err();
        assert!(err.contains("not empty"), "{tool}: {err}");
        let fresh = base.join(format!("{tool}-new"));
        call(&mut h, tool, json!({key: fresh, "images": false}));
        call(&mut h, tool, json!({key: fresh, "images": false}));
        let empty = base.join(format!("{tool}-empty"));
        std::fs::create_dir_all(&empty).unwrap();
        call(&mut h, tool, json!({key: empty, "images": false}));
    }
    // One tool's folder is not the other's.
    let err = exec::call(&mut h, "export_design_system", &json!({"path": base.join("export_handoff-new")})).unwrap_err();
    assert!(err.contains("not empty"), "{err}");
    assert_eq!(std::fs::read_to_string(theirs.join("notes.txt")).unwrap(), "mine");
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_handoff_manifest_cannot_make_the_export_delete_outside_the_folder() {
    let mut h = host();
    call(&mut h, "create_artboard", json!({"name": "Sign in", "styles": {"width": 390, "height": 844}}));
    let base = temp_path("x");
    let base = base.parent().unwrap().to_path_buf();
    let dir = base.join("bundle");
    let (outside, absolute, inside) = (base.join("outside.txt"), base.join("absolute.txt"), dir.join("stale.txt"));
    call(&mut h, "export_handoff", json!({"dir": dir, "images": false}));
    for f in [&outside, &absolute, &inside] {
        std::fs::write(f, "keep").unwrap();
    }
    let manifest = dir.join("manifest.json");
    let mut m: Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    for rel in ["../outside.txt", "..\\outside.txt", "stale.txt"] {
        m["files"][rel] = json!({"hash": "0"});
    }
    m["files"][absolute.to_string_lossy().as_ref()] = json!({"hash": "0"});
    std::fs::write(&manifest, serde_json::to_vec(&m).unwrap()).unwrap();
    let out = call(&mut h, "export_handoff", json!({"dir": dir, "images": false}));
    assert_eq!(out["removed"], 1, "only the entry inside the folder: {out}");
    assert!(outside.exists() && absolute.exists() && !inside.exists());
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn an_instance_cannot_be_placed_inside_its_own_component() {
    let mut h = host();
    let frame = |h: &mut exec::SessionHost, name: &str| call(h, "create_artboard", json!({"name": name, "styles": {"width": 200, "height": 100}}));
    let (a, b, c) = (frame(&mut h, "A"), frame(&mut h, "B"), frame(&mut h, "C"));
    let kid = call(&mut h, "write_html", json!({"targetNodeId": a["id"], "html": "<div layer-name=\"Kid\" style=\"width:20px;height:20px\"></div>"}));
    for x in [&a, &b, &c] {
        call(&mut h, "create_component", json!({"nodeId": x["id"]}));
    }
    let inside = json!({"componentId": a["id"], "parentId": kid["createdNodeIds"][0]});
    let err = exec::call(&mut h, "create_instance", &inside).unwrap_err();
    assert!(err.contains("can't contain an instance of itself"), "{err}");

    // B holds an instance of A, so A can't hold one of B, directly or by a swap.
    call(&mut h, "create_instance", json!({"componentId": a["id"], "parentId": b["id"]}));
    let err = exec::call(&mut h, "create_instance", &json!({"componentId": b["id"], "parentId": a["id"]})).unwrap_err();
    assert!(err.contains("can't contain an instance of itself"), "{err}");
    let x = call(&mut h, "create_instance", json!({"componentId": c["id"], "parentId": a["id"]}));
    let err = exec::call(&mut h, "swap_instance", &json!({"nodeId": x["id"], "componentId": b["id"]})).unwrap_err();
    assert!(err.contains("can't contain an instance of itself"), "{err}");
    let err = exec::call(&mut h, "move_nodes", &json!({"moves": [{"nodeId": b["id"], "parentId": a["id"]}]})).unwrap_err();
    assert!(err.contains("can't contain an instance of itself"), "{err}");
}

#[test]
fn a_copy_of_an_instance_cannot_be_dropped_into_its_own_component() {
    let mut h = host();
    let frame = |h: &mut exec::SessionHost, name: &str| call(h, "create_artboard", json!({"name": name, "styles": {"width": 200, "height": 100}}));
    let (a, c) = (frame(&mut h, "A"), frame(&mut h, "C"));
    let kid = call(&mut h, "write_html", json!({"targetNodeId": a["id"], "html": "<div layer-name=\"Kid\" style=\"width:20px;height:20px\"></div>"}));
    call(&mut h, "create_component", json!({"nodeId": a["id"]}));
    let inst = call(&mut h, "create_instance", json!({"componentId": a["id"], "parentId": c["id"]}));
    let into = json!({"nodes": [{"id": inst["id"], "parentId": kid["createdNodeIds"][0]}]});
    let err = exec::call(&mut h, "duplicate_nodes", &into).unwrap_err();
    assert!(err.contains("can't contain an instance of itself"), "{err}");
    let html = format!("<x-paper-clone node-id=\"{}\"/>", inst["id"].as_str().unwrap());
    let err = exec::call(&mut h, "write_html", &json!({"targetNodeId": kid["createdNodeIds"][0], "html": html})).unwrap_err();
    assert!(err.contains("can't contain an instance of itself"), "{err}");
}

// ------------------------------------------------------------------------------ agent input

/// An artboard id and the folder the open project lives in.
fn board(h: &mut exec::SessionHost) -> (Value, std::path::PathBuf) {
    let ab = call(h, "create_artboard", json!({"name": "A", "styles": {"width": "300px", "height": "200px"}}));
    (ab["id"].clone(), h.session.project.as_ref().unwrap().dir.clone())
}

/// A page that saved a NaN or infinity would not open again: JSON writes `null` for them.
fn assert_saves_and_reopens(h: &mut exec::SessionHost, dir: &std::path::Path) {
    h.session.save(&mut h.ed).unwrap();
    h.session.open(&mut h.ed, dir).unwrap();
}

#[test]
fn non_finite_css_values_leave_the_page_saveable() {
    let mut h = host();
    let (ab, dir) = board(&mut h);
    let html = r#"<div style="opacity:NaN;roughness:NaN;bowing:NaN;width:1e308rem;height:NaNpx;gap:1e308em;
        background:linear-gradient(NaNdeg,red,blue);color:hsl(NaNdeg,50%,50%)"><p style="line-height:inf;font-size:1e308em">hi</p>
        <p style="line-height:NaN%;opacity:NaN%">there</p></div>"#;
    call(&mut h, "write_html", json!({"targetNodeId": ab, "mode": "insert-children", "html": html}));
    assert_saves_and_reopens(&mut h, &dir);
}

#[test]
fn non_finite_svg_numbers_leave_the_page_saveable() {
    let mut h = host();
    let (ab, dir) = board(&mut h);
    let html = r#"<svg viewBox="NaN 0 inf 10" width="1e999" height="NaN"><rect width="5" height="5"/></svg>
        <svg viewBox="0 0 1e308 1e308" width="100" height="100"><rect width="5" height="5"/></svg>
        <svg width="inf" height="20"><rect width="5" height="5"/></svg>"#;
    call(&mut h, "write_html", json!({"targetNodeId": ab, "mode": "insert-children", "html": html}));
    assert_saves_and_reopens(&mut h, &dir);
}

#[test]
fn a_bad_export_scale_is_an_error_not_a_panic() {
    let mut h = host();
    let (ab, dir) = board(&mut h);
    for scale in ["2é", "NaNx", "infx", "1e999w", "é"] {
        let err = exec::call(&mut h, "export", &json!({"nodes": {ab.as_str().unwrap(): [{"format": "png", "scale": scale}]}, "directory": dir.join("out")}))
            .unwrap_err();
        assert!(err.contains("bad scale"), "{scale}: {err}");
    }
}

#[test]
fn paths_naming_shares_devices_or_streams_are_refused() {
    for bad in [r"\\host\share\x.png", "//host/share/x.png", r"\\?\UNC\host\share\x", r"\\?\GLOBALROOT\Device\Mup\host\share\x.png", r"\\.\pipe\x", r"\??\C:\x", "a.exe:x.png", r"C:\dir\a.exe:x.png", r"\\?\C:\a:b"] {
        assert!(exec::check_path(bad).is_err(), "{bad} should be refused");
    }
    for ok in [r"C:\dir\x.png", r"\\?\C:\dir\x.png", "/tmp/x.png", "rel/x.png", "C:x.png"] {
        assert!(exec::check_path(ok).is_ok(), "{ok} should pass");
    }

    let mut h = host();
    let (ab, _) = board(&mut h);
    for (tool, args) in [
        ("install_font", json!({"source": r"\\host\share\f.ttf"})),
        ("import_html", json!({"path": r"\\host\share\page.zip"})),
        ("import_design_system", json!({"path": "//host/share/pkg"})),
        ("export_design_system", json!({"path": r"\\host\share\pkg"})),
        ("export_handoff", json!({"dir": r"\\host\share\out"})),
        ("export", json!({"nodes": {}, "directory": r"\\host\share\out"})),
        ("get_screenshot", json!({"out": r"\\host\share\x.png"})),
        ("export_combined_pdf", json!({"nodeIds": [ab], "path": "a.pdf:s.pdf"})),
        ("open_file", json!({"fileId": r"\\host\share\proj"})),
    ] {
        let err = exec::call(&mut h, tool, &args).unwrap_err();
        assert!(err.contains("network or device path") || err.contains("`:`"), "{tool}: {err}");
    }
}

#[test]
fn an_export_never_fills_a_folder_with_someone_elses_manifest() {
    let mut h = host();
    call(&mut h, "create_artboard", json!({"name": "A", "styles": {"width": "300px", "height": "200px"}}));
    let dir = std::env::temp_dir().join(format!("papercraft-foreign-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    let keep = dir.join("keep.txt");
    std::fs::write(&keep, "mine").unwrap();
    std::fs::write(dir.join("manifest.json"), r#"{"format": 1, "files": {"keep.txt": "x"}, "schemaVersion": "other"}"#).unwrap();

    for (tool, args) in [
        ("export_handoff", json!({"dir": dir, "images": false})),
        ("export_design_system", json!({"path": dir})),
    ] {
        let err = exec::call(&mut h, tool, &args).unwrap_err();
        assert!(err.contains("not empty"), "{tool}: {err}");
    }
    assert_eq!(std::fs::read_to_string(&keep).unwrap(), "mine");

    let ours = std::env::temp_dir().join(format!("papercraft-ours-{:016x}", fastrand::u64(..)));
    for _ in 0..2 {
        call(&mut h, "export_handoff", json!({"dir": ours, "images": false}));
    }
}

const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn site_out() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("papercraft-site-{:016x}", fastrand::u64(..)))
}

/// Home (copy, an image, a button) and Pricing; the button links to Pricing.
fn two_frame_site() -> (exec::SessionHost, Value) {
    let mut h = host();
    let home = call(&mut h, "create_artboard", json!({"name": "Home", "styles": {"width": "390px", "height": "600px", "backgroundColor": "var(--bg)"}}));
    let pricing = call(&mut h, "create_artboard", json!({"name": "Pricing", "styles": {"width": "390px", "height": "600px", "left": "500px"}}));
    let markup = format!("<p layer-name=\"Headline\" style=\"font-size:20px\">Fish &amp; Chips</p><img layer-name=\"Hero\" src=\"{PIXEL}\" style=\"width:40px;height:40px\"><div layer-name=\"Go\" style=\"width:80px;height:30px;background:#ff0000\"></div>");
    call(&mut h, "write_html", json!({"targetNodeId": home["id"], "mode": "insert-children", "html": markup}));
    call(&mut h, "write_html", json!({"targetNodeId": pricing["id"], "mode": "insert-children", "html": "<p style=\"font-size:20px\">Plans</p>"}));
    let go = h.ed.doc.nodes.values().find(|n| n.name == "Go").unwrap().id;
    let go_key = exec::node_key(&h.ed, 0, go);
    call(&mut h, "set_prototype_links", json!({"links": [{"nodeId": go_key, "to": pricing["id"]}]}));
    (h, pricing)
}

fn read_site(dir: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                out.insert(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = Default::default();
    walk(dir, dir, &mut out);
    out
}

fn site_text(files: &std::collections::BTreeMap<String, Vec<u8>>, key: &str) -> String {
    String::from_utf8(files.get(key).unwrap_or_else(|| panic!("{key} not in {:?}", files.keys())).clone()).unwrap()
}

fn elements<'a>(nodes: &'a [mcp::html::Html], tag: &str, out: &mut Vec<&'a mcp::html::Element>) {
    for n in nodes {
        if let mcp::html::Html::Element(e) = n {
            if e.tag == tag {
                out.push(e);
            }
            elements(&e.children, tag, out);
        }
    }
}

fn stylesheet(files: &std::collections::BTreeMap<String, Vec<u8>>) -> String {
    files.keys().find(|k| k.starts_with("assets/site-") && k.ends_with(".css")).unwrap_or_else(|| panic!("no stylesheet in {:?}", files.keys())).clone()
}

#[test]
fn site_two_frames_have_real_links_assets_and_a_marker() {
    let (mut h, _) = two_frame_site();
    let dir = site_out();
    let r = call(&mut h, "export_site", json!({"path": dir, "title": "Fish"}));
    assert_eq!(r["pages"], 2, "{r}");
    assert_eq!(r["links"]["resolved"], 1, "{r}");
    assert_eq!(r["links"]["unresolved"], json!([]), "{r}");
    let files = read_site(&dir);
    assert!(files.contains_key(".papercraft-site.json") && files.contains_key("pricing.html"), "{:?}", files.keys());
    let home = site_text(&files, "index.html");
    assert!(home.contains("<a class=\"pc-link\" href=\"pricing.html\" aria-label=\"Go\">"), "{home}");
    let css = site_text(&files, &stylesheet(&files));
    assert!(css.contains("max-width: 390px") && home.contains(&stylesheet(&files)), "{css}");
    assert!(home.contains("<meta name=\"viewport\"") && home.contains("<meta charset=\"utf-8\">") && home.contains("<title>Fish</title>"), "{home}");
    assert!(home.contains("Fish &amp; Chips"), "{home}");
    let asset = files.keys().find(|k| k.starts_with("assets/")).unwrap_or_else(|| panic!("no asset in {:?}", files.keys()));
    assert!(home.contains(asset.as_str()) && !home.contains("data:image") && !home.contains("paper-asset"), "{home}");

    // Parses back: one <html>, and the anchor survives with its href.
    let tree = mcp::html::parse(&home);
    let (mut htmls, mut links) = (Vec::new(), Vec::new());
    elements(&tree, "html", &mut htmls);
    elements(&tree, "a", &mut links);
    assert_eq!((htmls.len(), links.len()), (1, 1), "{home}");
    assert_eq!(links[0].attr("href"), Some("pricing.html"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_foreign_folder_is_refused_and_a_reexport_drops_stale_pages() {
    let (mut h, pricing) = two_frame_site();
    let dir = site_out();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mine.txt"), "mine").unwrap();
    let err = exec::call(&mut h, "export_site", &json!({"path": dir})).unwrap_err();
    assert!(err.contains("not empty"), "{err}");
    assert_eq!(std::fs::read_to_string(dir.join("mine.txt")).unwrap(), "mine");
    std::fs::remove_dir_all(&dir).unwrap();

    call(&mut h, "export_site", json!({"path": dir}));
    std::fs::write(dir.join("notes.txt"), "keep").unwrap();
    call(&mut h, "rename_nodes", json!({"updates": [{"nodeId": pricing["id"], "name": "Plans"}]}));
    let again = call(&mut h, "export_site", json!({"path": dir}));
    assert!(again["removed"].as_u64().unwrap() >= 1, "{again}");
    assert!(dir.join("plans.html").exists() && !dir.join("pricing.html").exists());
    assert!(dir.join("notes.txt").exists(), "files the exporter did not write stay");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_unexported_link_targets_are_listed_and_the_same_input_gives_the_same_bytes() {
    let (mut h, _) = two_frame_site();
    let home = h.ed.doc.nodes.values().find(|n| n.name == "Home").unwrap().id;
    let only = json!([exec::node_key(&h.ed, 0, home)]);
    let (a, b) = (site_out(), site_out());
    let r = call(&mut h, "export_site", json!({"path": a, "frames": only}));
    assert_eq!(r["links"]["resolved"], 0, "{r}");
    assert!(r["links"]["unresolved"].to_string().contains("Go"), "{r}");
    assert!(!site_text(&read_site(&a), "index.html").contains("<a "));
    call(&mut h, "export_site", json!({"path": b, "frames": only}));
    assert_eq!(read_site(&a), read_site(&b));
    for d in [a, b] {
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[test]
fn site_mode_export_links_languages_and_leaves_the_document_and_history_alone() {
    let (mut h, _) = two_frame_site();
    call(&mut h, "set_variables", json!({"mode": "en", "variables": {"bg": "#ffffff"}}));
    call(&mut h, "set_variables", json!({"mode": "pt-BR", "variables": {"bg": "#00ff00"}}));
    // A token nothing reads is not written, so bind the page background to it now that it exists.
    let home = h.ed.doc.nodes.values().find(|n| n.name == "Home").unwrap().id;
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [format!("{}", home.0)], "styles": {"backgroundColor": "var(--bg)"}}]}));
    let collection = h.ed.design.collections[0].name.clone();
    let snapshot = |h: &exec::SessionHost| {
        (serde_json::to_string(&h.ed.doc).unwrap(), h.ed.history.undo_len(), format!("{:?}", h.ed.pages[h.ed.active_page].modes))
    };
    let before = snapshot(&h);

    let dir = site_out();
    let modes = json!({"collection": collection, "modes": ["en", "pt-BR"]});
    let r = call(&mut h, "export_site", json!({"path": dir, "modes": modes, "base_url": "https://example.com/"}));
    assert_eq!(r["modes"], json!(["en", "pt-BR"]), "{r}");
    assert_eq!(snapshot(&h), before);

    let files = read_site(&dir);
    let pt = site_text(&files, "pt-BR/index.html");
    assert!(pt.contains("<html lang=\"pt-BR\">") && site_text(&files, "en/index.html").contains("<html lang=\"en\">"), "{pt}");
    // Colours live in the one shared stylesheet: the first mode is the base, the others are scoped by language.
    let css = site_text(&files, &stylesheet(&files));
    assert!(css.contains("#ffffff") && css.contains(":lang(pt-BR)") && css.contains("#00ff00"), "{css}");
    for want in [
        "hreflang=\"en\" href=\"https://example.com/en/index.html\"",
        "hreflang=\"pt-BR\" href=\"https://example.com/pt-BR/index.html\"",
        "hreflang=\"x-default\"",
        "rel=\"canonical\" href=\"https://example.com/pt-BR/index.html\"",
        "href=\"pricing.html\"",
    ] {
        assert!(pt.contains(want), "{want} in {pt}");
    }
    let root = site_text(&files, "index.html");
    assert!(root.contains("href=\"en/index.html\"") && root.contains("href=\"pt-BR/index.html\""), "{root}");
    assert!(site_text(&files, "sitemap.xml").contains("<loc>https://example.com/pt-BR/pricing.html</loc>"));

    // Relative without a base URL, and the summary says so.
    let rel = site_out();
    let r = call(&mut h, "export_site", json!({"path": rel, "modes": modes}));
    assert!(r["urls"].as_str().unwrap().starts_with("relative"), "{r}");
    let rel_files = read_site(&rel);
    assert!(site_text(&rel_files, "en/index.html").contains("hreflang=\"pt-BR\" href=\"../pt-BR/index.html\""));
    assert!(!rel_files.contains_key("sitemap.xml"));
    for d in [dir, rel] {
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[test]
fn site_a_link_inside_a_link_keeps_only_the_outer_anchor_and_warns() {
    let (mut h, pricing) = two_frame_site();
    let home = h.ed.doc.nodes.values().find(|n| n.name == "Home").unwrap().id;
    let home_key = exec::node_key(&h.ed, 0, home);
    let go = h.ed.doc.nodes.values().find(|n| n.name == "Go").unwrap().id;
    let go_key = exec::node_key(&h.ed, 0, go);
    call(&mut h, "set_prototype_links", json!({"links": [{"nodeId": home_key, "to": pricing["id"]}, {"nodeId": go_key, "to": pricing["id"]}]}));
    let dir = site_out();
    let r = call(&mut h, "export_site", json!({"path": dir}));
    assert!(r["warnings"].to_string().contains("link inside a link"), "{r}");
    let page = site_text(&read_site(&dir), "index.html");
    assert_eq!(page.matches("<a ").count(), 1, "{page}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_links_and_buttons_without_text_are_named_by_their_layer() {
    let (mut h, pricing) = two_frame_site();
    let home = h.ed.doc.nodes.values().find(|n| n.name == "Home").unwrap().id;
    let home = exec::node_key(&h.ed, 0, home);
    let markup = "<div layer-name=\"Menu button\" style=\"display:flex;width:40px;height:40px\"><div style=\"width:20px;height:2px;background:#000000\"></div></div><div layer-name=\"Buy button\" style=\"display:flex\"><p>Buy</p></div><div layer-name=\"Rectangle 3\" style=\"width:20px;height:20px;background:#0000ff\"></div>";
    call(&mut h, "write_html", json!({"targetNodeId": home, "mode": "insert-children", "html": markup}));
    let rect = h.ed.doc.nodes.values().find(|n| n.name == "Rectangle 3").unwrap().id;
    let rect = exec::node_key(&h.ed, 0, rect);
    call(&mut h, "set_prototype_links", json!({"links": [{"nodeId": rect, "to": pricing["id"]}]}));
    let dir = site_out();
    let r = call(&mut h, "export_site", json!({"path": dir}));
    let page = site_text(&read_site(&dir), "index.html");
    assert!(page.contains("type=\"button\" aria-label=\"Menu button\">") && page.contains("href=\"pricing.html\" aria-label=\"Go\">"), "{page}");
    assert_eq!(page.matches("aria-label").count(), 2, "the button with text and the generic link get none: {page}");
    assert!(r["warnings"].to_string().contains("Rectangle 3: a link with no text"), "{r}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_html_has_semantic_tags_images_social_tags_and_one_shared_stylesheet() {
    let (mut h, _) = two_frame_site();
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir, "title": "Fish", "base_url": "https://example.com", "lang": "pt-BR", "description": "Fresh"}));
    let files = read_site(&dir);
    let home = site_text(&files, "index.html");
    assert!(home.matches("<h1").count() == 1 && home.contains("<main") && home.contains("Fish &amp; Chips") && home.contains("<html lang=\"pt-BR\">"), "{home}");
    let img = home.lines().find(|l| l.contains("<img")).unwrap_or_else(|| panic!("{home}"));
    for want in ["alt=\"Hero\"", "width=\"40\"", "height=\"40\"", "fetchpriority=\"high\"", "src=\"assets/"] {
        assert!(img.contains(want), "{want} in {img}");
    }
    for want in ["property=\"og:title\"", "name=\"twitter:card\"", "name=\"description\" content=\"Fresh\"", "rel=\"canonical\" href=\"https://example.com/index.html\""] {
        assert!(home.contains(want), "{want} in {home}");
    }
    let css = stylesheet(&files);
    assert_eq!(home.matches("rel=\"stylesheet\"").count(), 1);
    assert_eq!(site_text(&files, "pricing.html").matches("<h1").count(), 1, "a small lone text is still the h1");
    assert!(site_text(&files, "pricing.html").contains(&css) && !home.contains("<style"), "{home}");
    assert!(site_text(&files, "robots.txt").contains("Sitemap: https://example.com/sitemap.xml"));
    assert!(!files.keys().any(|k| k.ends_with(".js")) && !home.contains("<script"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_bad_options_are_refused_with_the_allowed_values() {
    let (mut h, _) = two_frame_site();
    let dir = site_out();
    for (args, want) in [
        (json!({"framework": "svelte"}), "one of"),
        (json!({"styling": "sass"}), "one of"),
        (json!({"styling": "emotion"}), "framework: react"),
        (json!({"render": "spa"}), "react, vue and angular"),
        (json!({"lang": "not a tag!"}), "lang"),
    ] {
        let mut a = args.clone();
        a["path"] = json!(dir);
        let err = exec::call(&mut h, "export_site", &a).unwrap_err();
        assert!(err.contains(want), "{args}: {err}");
        assert!(!dir.exists(), "nothing is written for {args}");
    }
}

#[test]
fn site_every_framework_writes_its_project_from_a_real_design() {
    let (mut h, _) = two_frame_site();
    for (framework, render, config) in [
        ("react", "static", "next.config.ts"),
        ("react", "spa", "vite.config.ts"),
        ("vue", "static", "nuxt.config.ts"),
        ("vue", "spa", "vite.config.ts"),
        ("angular", "static", "angular.json"),
        ("astro", "static", "astro.config.mjs"),
    ] {
        let dir = site_out();
        let r = call(&mut h, "export_site", json!({"path": dir, "framework": framework, "render": render, "base_url": "https://example.com"}));
        let files = read_site(&dir);
        assert!(files.contains_key("package.json") && files.contains_key(config), "{framework} {render}: {:?}", files.keys());
        assert!(files.keys().any(|k| k.ends_with("robots.txt")) && r["next_steps"].to_string().contains("npm"), "{framework} {render}: {r}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn site_scss_and_tailwind_write_sources_a_package_json_and_next_steps() {
    for (styling, source) in [("scss", "src/styles/main.scss"), ("tailwind", "src/input.css")] {
        let (mut h, _) = two_frame_site();
        let dir = site_out();
        let r = call(&mut h, "export_site", json!({"path": dir, "styling": styling}));
        let files = read_site(&dir);
        assert!(files.contains_key(source) && files.contains_key("package.json"), "{styling}: {:?}", files.keys());
        assert!(r["next_steps"].to_string().contains("npm run build"), "{r}");
        assert!(site_text(&files, "index.html").contains("assets/site.css"));
        assert_eq!(r["styling"], styling);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// Exports one artboard filled with `markup` as html and returns its page.
fn site_page_of(markup: &str) -> String {
    let mut h = host();
    let board = call(&mut h, "create_artboard", json!({"name": "Home", "styles": {"width": "1200px", "height": "900px"}}));
    call(&mut h, "write_html", json!({"targetNodeId": board["id"], "mode": "insert-children", "html": markup}));
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir}));
    let files = read_site(&dir);
    let page = format!("{}
{}", site_text(&files, "index.html"), site_text(&files, &stylesheet(&files)));
    std::fs::remove_dir_all(&dir).unwrap();
    page
}

#[test]
fn site_a_frame_with_free_layout_is_positioned_for_its_absolute_children() {
    let page = site_page_of("<div layer-name=\"Floating badges\" style=\"position:relative;width:400px;height:80px\"><p layer-name=\"Badge one\" style=\"position:absolute;left:0;top:0;font-size:14px\">New</p><p layer-name=\"Badge two\" style=\"position:absolute;left:120px;top:30px;font-size:14px\">Beta</p></div>");
    let rule = page.split('}').find(|r| r.contains(".floating-badges")).unwrap_or_else(|| panic!("{page}"));
    assert!(rule.contains("position: relative"), "{page}");
}

#[test]
fn site_headings_of_identical_siblings_share_a_level_and_prices_are_not_headings() {
    let q = |t: &str| format!("<div style=\"display:flex;flex-direction:column\"><p style=\"font-size:20px;font-weight:700\">{t}</p><p style=\"font-size:16px\">An answer that reads like a sentence of copy.</p></div>");
    let page = site_page_of(&format!(
        "<div><p style=\"font-size:48px;font-weight:700\">Title</p></div><div><p style=\"font-size:40px\">Questions</p></div>{}{}{}{}<p style=\"font-size:32px\">$29</p><p style=\"font-size:32px\">R$ 49/mês</p>",
        q("One?"), q("Two?"), q("Three?"), q("Four?")
    ));
    for t in ["One?", "Two?", "Three?", "Four?"] {
        assert!(page.contains(&format!(">{t}</h3>")), "{t} in {page}");
    }
    assert!(!page.contains(">$29</h") && !page.contains(">R$ 49/mês</h"), "{page}");
}

#[test]
fn site_a_landmark_is_not_inferred_inside_the_same_landmark() {
    let page = site_page_of("<div layer-name=\"Nav\" style=\"display:flex\"><p layer-name=\"Nav home\" style=\"font-size:14px\">Home</p></div><div layer-name=\"Footer\" style=\"display:flex\"><div layer-name=\"Footer links\" style=\"display:flex\"><p style=\"font-size:14px\">Docs</p></div></div>");
    assert_eq!(page.matches("<nav").count(), 1, "{page}");
    assert_eq!(page.matches("<footer").count(), 1, "{page}");
}

#[test]
fn site_tailwind_packages_are_pinned_to_an_exact_version_in_every_framework() {
    let (mut h, _) = two_frame_site();
    for (framework, render) in [("html", "static"), ("react", "static"), ("react", "spa"), ("vue", "static"), ("vue", "spa"), ("angular", "static"), ("astro", "static")] {
        let dir = site_out();
        call(&mut h, "export_site", json!({"path": dir, "framework": framework, "render": render, "styling": "tailwind"}));
        let pkg: Value = serde_json::from_str(&site_text(&read_site(&dir), "package.json")).unwrap();
        let mut seen = 0;
        for section in ["dependencies", "devDependencies"] {
            for (name, ver) in pkg[section].as_object().into_iter().flatten().filter(|(n, _)| n.contains("tailwindcss")) {
                seen += 1;
                assert!(ver.as_str().is_some_and(|v| v.chars().all(|c| c.is_ascii_digit() || c == '.')), "{framework} {render}: {name} {ver}");
            }
        }
        assert!(seen >= 2, "{framework} {render}: {pkg}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// Exports "Headline" bound to a string with `pt` as its pt-BR text; returns the stylesheet and
/// the font files `(path, bytes)` of the export.
fn site_with_pt_text(pt: &str) -> (String, Vec<(String, usize)>) {
    let (mut h, _) = two_frame_site();
    call(&mut h, "set_variables", json!({"variables": {"greeting": {"value": "Hello", "type": "string", "collection": "Copy", "mode": "en"}}}));
    call(&mut h, "set_variables", json!({"variables": {"greeting": {"value": pt, "type": "string", "collection": "Copy", "mode": "pt-BR"}}}));
    call(&mut h, "set_variable_mode", json!({"mode": "en"}));
    let headline = h.ed.doc.nodes.values().find(|n| n.name == "Headline").unwrap().id;
    call(&mut h, "bind_variable", json!({"nodeIds": [format!("{}", headline.0)], "field": "text", "variable": "greeting"}));
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir, "modes": {"collection": "Copy", "modes": ["en", "pt-BR"]}}));
    let files = read_site(&dir);
    let css = site_text(&files, &stylesheet(&files));
    let fonts = files
        .iter()
        .filter(|(k, _)| k.starts_with("assets/fonts/"))
        .map(|(k, v)| (k.clone(), v.len()))
        .collect();
    std::fs::remove_dir_all(&dir).unwrap();
    (css, fonts)
}

#[test]
fn site_fonts_are_subset_to_the_characters_of_every_language_and_get_a_fallback_face() {
    let (css, fonts) = site_with_pt_text("Olá");
    let (_, plain) = site_with_pt_text("Ola");
    assert!(!fonts.is_empty(), "{css}");
    for (path, len) in &fonts {
        assert!(path.ends_with(".woff2") && *len < 40_000, "{path} is {len} bytes");
        assert!(css.contains(&format!("{path}) format(\"woff2\")")), "{css}");
    }
    assert_ne!(fonts, plain, "a character only the pt-BR catalog uses must change the subset");
    assert!(css.contains("font-family: \"Inter Fallback\"") && css.contains("src: local(\"Arial\")"), "{css}");
    assert!(css.contains("size-adjust:") && css.contains("ascent-override:"), "{css}");
    assert!(css.contains("\"Inter\", \"Inter Fallback\", system-ui"), "{css}");
}

#[test]
fn site_a_desktop_page_fills_the_window_and_a_phone_page_stays_a_column() {
    let mut h = host();
    for (name, width, left) in [("Desk", "1280px", "0px"), ("Phone", "390px", "1400px")] {
        let ab = call(&mut h, "create_artboard", json!({"name": name, "styles": {"width": width, "height": "600px", "left": left}}));
        call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": "<p style=\"font-size:20px\">Hi</p>"}));
    }
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir}));
    let files = read_site(&dir);
    let css = site_text(&files, &stylesheet(&files));
    assert_eq!(css.matches("min-height: max(100vh, 600px)").count(), 2, "{css}");
    assert!(!css.contains("max-width: 1280px") && css.matches("margin: 0 auto").count() == 1, "{css}");
    assert!(css.contains("max-width: 390px"), "{css}");
    std::fs::remove_dir_all(&dir).unwrap();

    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir, "styling": "tailwind"}));
    let files = read_site(&dir);
    let (desk, phone) = (site_text(&files, "index.html"), site_text(&files, "phone.html"));
    assert!(desk.contains("w-full h-auto min-h-[max(100vh,600px)]") && !desk.contains("max-w-[1280px]"), "{desk}");
    assert!(phone.contains("w-full max-w-[390px] mx-auto h-auto min-h-[max(100vh,600px)]"), "{phone}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_a_page_narrower_than_its_design_reflows_instead_of_clipping() {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Desk", "styles": {"width": "1280px", "height": "800px"}}));
    let html = concat!(
        "<div style=\"display:flex;flex-direction:column;gap:16px;padding:48px;width:100%\">",
        "<h1 style=\"font-size:56px;margin:0\">Big headline</h1>",
        "<p style=\"width:640px;font-size:18px\">A subtitle that is fixed at 640 wide</p>",
        "<div style=\"display:flex;flex-direction:row;gap:24px\">",
        "<div style=\"width:300px;height:120px;flex-shrink:0\"></div>",
        "<div style=\"width:300px;height:120px;flex-shrink:0\"></div>",
        "<div style=\"width:300px;height:120px;flex-shrink:0\"></div></div>",
        "<div style=\"display:flex;flex-direction:row;gap:8px\">",
        "<div style=\"width:24px;height:24px\"></div><span style=\"font-size:14px\">Label</span></div></div>"
    );
    call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": html}));
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir}));
    let files = read_site(&dir);
    let css = site_text(&files, &stylesheet(&files));
    assert!(css.contains("font-size: clamp(28px, 5.47vw, 56px)"), "{css}");
    assert!(css.contains("width: 640px;") && css.contains("width: 300px;"), "{css}");
    assert!(css.matches("max-width: 100%").count() >= 4, "{css}");
    assert_eq!(css.matches("flex-wrap: wrap").count(), 1, "only the row of cards wraps: {css}");
    assert!(!css.contains("font-size: clamp(7px"), "body text keeps its size: {css}");
    std::fs::remove_dir_all(&dir).unwrap();

    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir, "styling": "tailwind"}));
    let files = read_site(&dir);
    let page = site_text(&files, "index.html");
    assert!(page.contains("text-[length:clamp(28px,5.47vw,56px)]") && !page.contains("text-[56px]"), "{page}");
    assert!(page.contains("w-[300px]") && page.contains("max-w-full"), "{page}");
    assert_eq!(page.matches("flex-wrap").count(), 1, "{page}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_tailwind_text_gets_the_font_stack_of_the_css_dialects() {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Page", "styles": {"width": "1280px", "height": "800px"}}));
    let html = "<h1 style=\"font-size:40px\">Default font</h1><p style=\"font-family:Inter\">Named font</p>";
    call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": html}));
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir, "styling": "tailwind"}));
    let page = site_text(&read_site(&dir), "index.html");
    let stack = "font-[family-name:'Inter','Inter_Fallback',system-ui,sans-serif]";
    assert_eq!(page.matches(stack).count(), 2, "{page}");
    assert!(!page.contains("font-['"), "one font class per text: {page}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The stylesheet of a site exported from one frame of `width` holding `body`.
fn site_css_of(width: &str, body: &str) -> String {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Page", "styles": {"width": width, "height": "800px"}}));
    call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": body}));
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir}));
    let files = read_site(&dir);
    let css = site_text(&files, &stylesheet(&files));
    std::fs::remove_dir_all(&dir).unwrap();
    css
}

#[test]
fn site_big_type_on_a_phone_frame_keeps_its_size_on_the_phone() {
    let css = site_css_of(
        "390px",
        "<div style=\"display:flex;flex-direction:column;width:100%\"><h1 style=\"font-size:48px;margin:0\">Hello</h1></div>",
    );
    assert!(css.contains("font-size: clamp(24px, 12.31vw, 48px)"), "{css}");
}

#[test]
fn site_a_layer_wider_than_its_parent_keeps_its_designed_width() {
    let css = site_css_of(
        "390px",
        "<div style=\"display:flex;flex-direction:column;width:100%\"><div style=\"width:600px;height:100px\"></div></div>",
    );
    assert!(css.contains("width: 600px") && css.matches("max-width: 100%").count() == 1, "only the base img rule: {css}");
}

#[test]
fn site_a_row_wider_than_its_frame_is_a_carousel_and_does_not_wrap() {
    let card = "<div style=\"width:300px;height:100px;flex-shrink:0\"></div>";
    let css = site_css_of(
        "390px",
        &format!("<div style=\"display:flex;flex-direction:row;gap:24px;width:100%\">{}</div>", card.repeat(5)),
    );
    assert!(css.contains("width: 300px") && !css.contains("flex-wrap"), "{css}");
}

/// Exports one frame made from `body` as html and returns the home page and the export result.
fn site_home_of(width: &str, bg: &str, body: &str) -> (String, std::collections::BTreeMap<String, Vec<u8>>, Value) {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Page", "styles": {"width": width, "height": "800px", "backgroundColor": bg}}));
    call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": body}));
    let dir = site_out();
    let r = call(&mut h, "export_site", json!({"path": dir}));
    let files = read_site(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    (site_text(&files, "index.html"), files, r)
}

#[test]
fn site_headings_follow_the_outline_and_brand_text_and_subtitles_are_not_headings() {
    let body = "<div layer-name=\"Header\" style=\"display:flex\"><p style=\"font-size:22px\">Lumen</p></div>\
        <div style=\"display:flex;flex-direction:column\"><p style=\"font-size:48px\">Make light</p><p style=\"font-size:24px\">Calm tools for tired eyes</p></div>\
        <p style=\"font-size:16px\">A long paragraph of body copy that is well over one hundred and twenty characters, so that sixteen pixels is the size most text uses on this page.</p>\
        <div style=\"display:flex;flex-direction:column\"><p style=\"font-size:32px\">Features</p></div>\
        <div style=\"display:flex;flex-direction:column\"><p layer-name=\"H5 notes\" style=\"font-size:20px\">Notes</p></div>";
    let (html, _, _) = site_home_of("1200px", "#ffffff", body);
    assert!(html.contains("<h1") && html.contains("Make light"), "{html}");
    assert!(!html.contains(">Lumen</h") && html.contains("Lumen"), "brand text in the header is not a heading: {html}");
    assert!(html.contains("<p class=\"") && !html.contains(">Calm tools for tired eyes</h"), "{html}");
    let mut prev = 0;
    for level in html.split("<h").skip(1).filter_map(|s| s.chars().next()?.to_digit(10)) {
        assert!(level <= prev + 1, "h{level} after h{prev}: {html}");
        prev = level;
    }
}

#[test]
fn site_icon_is_the_favicon_layer_else_data_colon_so_the_browser_asks_for_nothing() {
    let (plain, _, _) = site_home_of("390px", "#ffffff", "<p style=\"font-size:20px\">Hi</p>");
    assert!(plain.contains("<link rel=\"icon\" href=\"data:,\">"), "{plain}");
    let body = "<p style=\"font-size:20px\">Hi</p><div layer-name=\"FAVICON\" style=\"width:32px;height:32px;background:#ff0000\"></div>";
    let (home, files, _) = site_home_of("390px", "#ffffff", body);
    let icon = files.keys().find(|k| k.ends_with(".svg")).unwrap_or_else(|| panic!("{:?}", files.keys()));
    assert!(home.contains(&format!("<link rel=\"icon\" type=\"image/svg+xml\" href=\"{icon}\">")), "{home}");
}

#[test]
fn site_a_string_variable_nothing_reads_is_not_a_custom_property() {
    let mut h = two_language_site();
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir, "modes": {"collection": "Copy", "modes": ["en", "pt-BR"]}}));
    let files = read_site(&dir);
    let css = site_text(&files, &stylesheet(&files));
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(!css.contains("--greeting"), "{css}");
}

#[test]
fn site_preloads_the_h1_face_and_the_body_face_and_warns_about_low_contrast() {
    let body = "<p style=\"font-size:48px;font-weight:700;color:#ffffff\">Bold title</p>\
        <p style=\"font-size:16px;color:#ffffff\">Plenty of regular body copy sits here to be the face most characters use.</p>";
    let (home, _, r) = site_home_of("1200px", "#7c5cff", body);
    assert_eq!(home.matches("rel=\"preload\"").count(), 2, "{home}");
    assert!(home.contains("as=\"font\"") && home.contains("crossorigin"), "{home}");
    let warnings = r["warnings"].to_string();
    assert!(warnings.contains("WCAG AA"), "{warnings}");
    assert!(!warnings.contains("Bold title"), "large text needs only 3:1: {warnings}");
}

#[test]
fn site_text_bound_to_a_string_variable_becomes_a_message_per_language() {
    let (mut h, _) = two_frame_site();
    call(&mut h, "set_variables", json!({"variables": {"greeting": {"value": "Hello", "type": "string", "collection": "Copy", "mode": "en"}}}));
    call(&mut h, "set_variables", json!({"variables": {"greeting": {"value": "Ola <b>", "type": "string", "collection": "Copy", "mode": "pt-BR"}}}));
    call(&mut h, "set_variable_mode", json!({"mode": "en"}));
    let headline = h.ed.doc.nodes.values().find(|n| n.name == "Headline").unwrap().id;
    call(&mut h, "bind_variable", json!({"nodeIds": [format!("{}", headline.0)], "field": "text", "variable": "greeting"}));
    let dir = site_out();
    let r = call(&mut h, "export_site", json!({"path": dir, "modes": {"collection": "Copy", "modes": ["en", "pt-BR"]}}));
    let files = read_site(&dir);
    assert!(site_text(&files, "en/index.html").contains(">Hello<"), "{r}");
    assert!(site_text(&files, "pt-BR/index.html").contains("Ola &lt;b&gt;"), "{r}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A two-language site whose copy is bound, so the languages are real modes of a collection.
fn two_language_site() -> exec::SessionHost {
    let (mut h, _) = two_frame_site();
    call(&mut h, "set_variables", json!({"variables": {"greeting": {"value": "Hello", "type": "string", "collection": "Copy", "mode": "en"}}}));
    call(&mut h, "set_variables", json!({"variables": {"greeting": {"value": "Ola", "type": "string", "collection": "Copy", "mode": "pt-BR"}}}));
    h
}

#[test]
fn site_title_and_description_can_differ_per_language_in_every_framework() {
    for framework in ["html", "astro", "vue", "angular", "react"] {
        let mut h = two_language_site();
        let dir = site_out();
        call(&mut h, "export_site", json!({"path": dir, "framework": framework, "modes": {"collection": "Copy", "modes": ["en", "pt-BR"]},
            "title": {"en": "Design once", "pt-BR": "Projete uma vez"}, "description": {"en": "Ship everywhere", "pt-BR": "Entregue em todo lugar"}}));
        let all: String = read_site(&dir).iter().filter(|(k, _)| !k.contains("node_modules")).map(|(_, v)| String::from_utf8_lossy(v).into_owned()).collect::<Vec<_>>().join("\n");
        for needle in ["Design once", "Projete uma vez", "Ship everywhere", "Entregue em todo lugar"] {
            assert!(all.contains(needle), "{framework}: `{needle}` missing");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn site_html_pages_carry_their_own_language_title_and_description() {
    let mut h = two_language_site();
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir, "modes": {"collection": "Copy", "modes": ["en", "pt-BR"]},
        "title": {"en": "Design once", "pt-BR": "Projete uma vez"}, "description": {"pt-BR": "Entregue", "en": "Ship"}}));
    let files = read_site(&dir);
    let (en, pt) = (site_text(&files, "en/index.html"), site_text(&files, "pt-BR/index.html"));
    assert!(en.contains("<title>Design once</title>") && en.contains("content=\"Ship\""), "{en}");
    assert!(pt.contains("<title>Projete uma vez</title>") && pt.contains("content=\"Entregue\"") && !pt.contains("Design once"), "{pt}");
    // The second page's title is the language's title as a suffix.
    assert!(site_text(&files, "pt-BR/pricing.html").contains("<title>Pricing | Projete uma vez</title>"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_language_without_an_entry_takes_the_base_language_text() {
    let mut h = two_language_site();
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir, "modes": {"collection": "Copy", "modes": ["en", "pt-BR"]}, "title": {"en": "Design once"}}));
    let files = read_site(&dir);
    assert!(site_text(&files, "pt-BR/index.html").contains("<title>Design once</title>"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn site_title_of_a_wrong_type_is_refused() {
    for (key, value) in [("title", json!(3)), ("description", json!(["a"])), ("title", json!({"en": 1}))] {
        let mut h = two_language_site();
        let dir = site_out();
        let err = exec::call(&mut h, "export_site", &json!({"path": dir, key: value})).unwrap_err();
        assert!(err.contains(&format!("`{key}` must be a string")), "{err}");
    }
}

#[test]
fn site_title_for_a_language_the_export_does_not_have_is_refused() {
    let mut h = two_language_site();
    let dir = site_out();
    let args = json!({"path": dir, "modes": {"collection": "Copy", "modes": ["en", "pt-BR"]}, "title": {"en": "A", "pt_BR": "B"}});
    let err = exec::call(&mut h, "export_site", &args).unwrap_err();
    assert!(err.contains("`pt_BR`") && err.contains("en, pt-BR"), "{err}");
}

#[test]
fn site_language_picker_page_has_the_first_languages_tag() {
    for framework in ["html", "astro", "vue", "angular"] {
        let mut h = two_language_site();
        let dir = site_out();
        call(&mut h, "export_site", json!({"path": dir, "framework": framework, "modes": {"collection": "Copy", "modes": ["pt-BR", "en"]}}));
        let files = read_site(&dir);
        let has_lang = match framework {
            "html" => site_text(&files, "index.html").contains("<html lang=\"pt-BR\""),
            "astro" => site_text(&files, "src/pages/index.astro").contains("<html lang=\"pt-BR\""),
            "vue" => site_text(&files, "app/head.ts").split("INDEX_HEAD").nth(1).is_some_and(|t| t.contains("\"lang\": \"pt-BR\"")),
            _ => serde_json::from_str::<Value>(&site_text(&files, "src/app/site.json")).unwrap()["index"]["lang"] == "pt-BR",
        };
        assert!(has_lang, "{framework}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn site_host_files_cache_hashed_assets_and_fall_back_for_single_page_apps() {
    // (framework, render, root of the copied files, a hashed glob, has fallback, has .nojekyll)
    let cases = [
        ("html", "static", "", "/assets/*", false, false),
        ("astro", "static", "public/", "/_astro/*", false, true),
        ("vue", "static", "public/", "/_nuxt/*", false, true),
        ("vue", "spa", "public/", "/assets/*", true, false),
        ("react", "static", "public/", "/_next/static/*", false, true),
        ("react", "spa", "public/", "/assets/*", true, false),
        ("angular", "static", "public/", "/*.js", false, false),
        ("angular", "spa", "public/", "/*.css", true, false),
    ];
    for (framework, render, root, glob, spa, nojekyll) in cases {
        let mut h = two_language_site();
        let dir = site_out();
        let args = json!({"path": dir, "framework": framework, "render": render});
        call(&mut h, "export_site", args);
        let files = read_site(&dir);
        let at = |name: &str| format!("{root}{name}");
        let what = format!("{framework} {render}");
        let headers = site_text(&files, &at("_headers"));
        let rule = format!("{glob}\n  Cache-Control: public, max-age=31536000, immutable");
        assert!(headers.contains(&rule), "{what}: {headers}");
        assert!(
            !headers.contains("html") && !headers.lines().any(|l| l == "/*"),
            "{what}: html must not be immutable: {headers}"
        );
        // Vercel reads its file from the project root, not from the built folder.
        let vercel: Value = serde_json::from_str(&site_text(&files, "vercel.json")).unwrap();
        let value = &vercel["headers"][0]["headers"][0]["value"];
        assert!(value.as_str().unwrap().contains("immutable"), "{what}");
        assert_eq!(vercel.get("rewrites").is_some(), spa, "{what}");
        assert_eq!(files.contains_key(&at("_redirects")), spa, "{what}");
        assert_eq!(files.contains_key(&at(".nojekyll")), nojekyll, "{what}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// "Home" at 1440 and "Home @mobile" at 390, plus a lone "Orphan @x"; the phone has no Aside.
fn breakpoint_site(extra: Value) -> (std::collections::BTreeMap<String, Vec<u8>>, Value) {
    breakpoint_site_of(&[
        ("Home", "1440px", "0px", "<p layer-name=\"Title\" style=\"font-size:20px\">Hi</p><p layer-name=\"Aside\" style=\"font-size:12px\">Side</p>"),
        ("Home @mobile", "390px", "1600px", "<p layer-name=\"Title\" style=\"font-size:14px\">Hi</p>"),
        ("Orphan @x", "390px", "2200px", "<p>Lost</p>"),
    ], extra)
}

fn breakpoint_site_of(frames: &[(&str, &str, &str, &str)], extra: Value) -> (std::collections::BTreeMap<String, Vec<u8>>, Value) {
    let mut h = host();
    for (name, width, left, html) in frames.iter().copied() {
        let ab = call(&mut h, "create_artboard", json!({"name": name, "styles": {"width": width, "height": "600px", "left": left}}));
        call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": html}));
    }
    let dir = site_out();
    let mut args = json!({"path": dir});
    args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    let r = call(&mut h, "export_site", args);
    let files = read_site(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    (files, r)
}

#[test]
fn site_frames_named_at_something_join_their_page_as_media_queries() {
    let (files, r) = breakpoint_site(json!({}));
    let pages: Vec<_> = files.keys().filter(|k| k.ends_with(".html")).collect();
    assert_eq!(pages, ["index.html", "orphan-x.html"], "{pages:?}");
    let css = site_text(&files, &stylesheet(&files));
    assert!(css.contains("@media (max-width: 390px) {\n  :root .title {\n    font-size: 14px;"), "{css}");
    assert!(css.contains(":root .aside {\n    display: none;"), "{css}");
    let warnings = r["warnings"].to_string();
    assert!(warnings.contains("`Orphan @x`") && warnings.contains("exported as its own page"), "{warnings}");
    assert!(warnings.contains("hidden at 390px and below"), "{warnings}");
}

#[test]
fn a_phone_frame_matches_layers_by_name_when_the_desktop_wraps_them_in_rows() {
    let card = |n: u8, size: u8| format!("<p layer-name=\"Feature {n}\" style=\"font-size:{size}px\">F{n}</p>");
    let desktop = format!("<div layer-name=\"Row 1\">{}{}</div><div layer-name=\"Row 2\">{}</div>", card(1, 20), card(2, 20), card(3, 20));
    let phone = format!("{}{}{}", card(1, 14), card(2, 14), card(3, 14));
    let (files, r) = breakpoint_site_of(&[("Home", "1440px", "0px", &desktop), ("Home @mobile", "390px", "1600px", &phone)], json!({}));
    let css = site_text(&files, &stylesheet(&files));
    assert!(css.contains(":root .feature-1 {
    font-size: 14px;"), "cards take their phone style: {css}");
    assert!(css.contains("display: contents;") && !css.contains("display: none;"), "rows dissolve instead of hiding the cards: {css}");
    let warnings = r["warnings"].to_string();
    assert!(!warnings.contains("only in this frame") && !warnings.contains("hidden at"), "{warnings}");
}

#[test]
fn breakpoint_warnings_name_the_layers() {
    let (_, r) = breakpoint_site(json!({}));
    assert!(r["warnings"].to_string().contains("(`Aside`)"), "{r}");
}

#[test]
fn site_breakpoints_work_in_tailwind_and_are_skipped_with_a_warning_in_css_in_js() {
    let (files, _) = breakpoint_site(json!({"styling": "tailwind"}));
    let all: String = files.iter().filter(|(k, _)| k.ends_with(".css") || k.ends_with(".html")).map(|(_, v)| String::from_utf8_lossy(v).into_owned()).collect();
    assert!(all.contains("@media (max-width: 390px)") && all.contains("font-size: 14px") && all.contains("text-xl"), "the phone size wins under 390, the desktop utility stays: {all}");
    let (_, r) = breakpoint_site(json!({"framework": "react", "styling": "emotion"}));
    assert!(r["warnings"].to_string().contains("CSS-in-JS"), "{r}");
}

#[test]
fn a_gradient_stop_bound_to_a_colour_variable_follows_the_mode_and_exports_as_var() {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Hero", "styles": {"width": "200px", "height": "100px", "background": "linear-gradient(180deg, #ff0000, #0000ff)"}}));
    call(&mut h, "set_variables", json!({"mode": "Light", "variables": {"brand": "#112233"}}));
    call(&mut h, "set_variables", json!({"mode": "Dark", "variables": {"brand": "#aabbcc"}}));
    call(&mut h, "set_variable_mode", json!({"mode": "Light"}));
    let id = num(&ab["id"]);
    let e = exec::call(&mut h, "bind_variable", &json!({"nodeIds": [ab["id"]], "field": "gradient-stop-5", "variable": "brand"})).unwrap_err();
    assert!(e.contains("does not apply"), "{e}");
    call(&mut h, "bind_variable", json!({"nodeIds": [ab["id"]], "field": "gradient-stop-1", "variable": "brand"}));
    let stop = |h: &exec::SessionHost| h.ed.doc.get(id).unwrap().style.fills[0].stops().unwrap()[1].color.to_hex();
    assert_eq!(stop(&h), "#112233");
    call(&mut h, "set_variable_mode", json!({"mode": "Dark"}));
    assert_eq!(stop(&h), "#aabbcc");
    let styles = call(&mut h, "get_computed_styles", json!({"nodeIds": [ab["id"]]}));
    let bg = styles[ab["id"].as_str().unwrap()]["background"].as_str().unwrap().to_owned();
    assert!(bg.contains("var(--brand"), "{bg}");
    // Detaching keeps the current colour and exports it fixed again.
    call(&mut h, "bind_variable", json!({"nodeIds": [ab["id"]], "field": "gradient-stop-1"}));
    call(&mut h, "set_variable_mode", json!({"mode": "Light"}));
    assert_eq!(stop(&h), "#aabbcc");
}

#[test]
fn a11y_annotations_are_set_read_linted_and_exported() {
    let (mut h, _) = two_frame_site();
    let key = |h: &exec::SessionHost, name: &str| {
        let id = h.ed.doc.nodes.values().find(|n| n.name == name).unwrap().id;
        exec::node_key(&h.ed, 0, id)
    };
    let (hero, head, go) = (key(&h, "Hero"), key(&h, "Headline"), key(&h, "Go"));
    let lint = |h: &mut exec::SessionHost| call(h, "lint_design", json!({}))["issues"].to_string();
    assert!(lint(&mut h).contains("image-alt"), "a picture without alt text is flagged");

    call(&mut h, "set_accessibility", json!({"items": [
        {"nodeId": hero, "alt": "A bowl of fish and chips"},
        {"nodeId": head, "role": "heading", "level": 3},
        {"nodeId": go, "order": 0},
    ]}));
    assert!(!lint(&mut h).contains("image-alt"));
    let info = call(&mut h, "get_node_info", json!({"nodeId": head}));
    assert_eq!(info["accessibility"], json!({"role": "heading", "level": 3}), "{info}");

    // Only a frame placed by position (no auto layout) may reorder its children.
    let board = h.ed.doc.nodes.values().find(|n| n.name == "Home").unwrap().id;
    h.ed.doc.nodes.get_mut(&board).unwrap().layout = None;
    let dir = site_out();
    call(&mut h, "export_site", json!({"path": dir}));
    let home = site_text(&read_site(&dir), "index.html");
    assert!(home.contains("alt=\"A bowl of fish and chips\""), "{home}");
    assert!(home.contains("<h3") && home.contains("Fish &amp; Chips"), "{home}");
    assert!(home.find("href=\"pricing.html\"") < home.find("<h3"), "explicit order puts the link first: {home}");
    std::fs::remove_dir_all(&dir).unwrap();

    // Decorative replaces alt text; clearing removes the annotation (and the key from the node).
    call(&mut h, "set_accessibility", json!({"items": [{"nodeId": hero, "alt": null, "decorative": true}]}));
    assert!(!lint(&mut h).contains("image-alt"));
    call(&mut h, "set_accessibility", json!({"items": [{"nodeId": hero, "clear": true}]}));
    assert!(call(&mut h, "get_node_info", json!({"nodeId": hero})).get("accessibility").is_none());

    for bad in [
        json!({"nodeId": hero, "role": "banner"}),
        json!({"nodeId": hero, "level": 7, "role": "heading"}),
        json!({"nodeId": hero, "level": 2}),
        json!({"nodeId": hero, "alt": "x", "decorative": true}),
    ] {
        assert!(exec::call(&mut h, "set_accessibility", &json!({"items": [bad.clone()]})).is_err(), "{bad}");
    }
}

#[test]
fn a_text_style_updates_every_layer_that_follows_it_in_one_undo_step() {
    let mut h = host();
    card(&mut h);
    let id = |h: &exec::SessionHost, name: &str| h.ed.doc.nodes.values().find(|n| n.name == name).unwrap().id.0.to_string();
    let size = |h: &exec::SessionHost, name: &str| h.ed.doc.nodes.values().find(|n| n.name == name).unwrap().kind.font_size().unwrap();
    let title = id(&h, "Title");
    let body_size = size(&h, "Body");

    let r = call(&mut h, "set_text_style", json!({"action": "set", "name": "Heading / L", "fontSize": 40, "bold": true, "lineHeight": 1.2}));
    assert_eq!(r["layersUpdated"], 0);
    assert!(call_err(&mut h, "set_text_style", json!({"action": "set", "name": "Nope"})).contains("fontSize"));
    assert!(call_err(&mut h, "set_text_style", json!({"action": "apply", "name": "Missing", "nodeIds": [title.clone()]})).contains("no text style"));

    let r = call(&mut h, "set_text_style", json!({"action": "apply", "name": "Heading / L", "nodeIds": [title.clone()]}));
    assert_eq!(r["layersChanged"], 1);
    assert_eq!(size(&h, "Title"), 40.0);
    assert_eq!(size(&h, "Body"), body_size, "only the applied layer follows");
    assert!(h.ed.doc.nodes.values().find(|n| n.name == "Title").unwrap().text_style.bold);
    assert_eq!(call(&mut h, "get_variables", json!({}))["textStyles"]["Heading / L"]["font_size"], 40.0);

    let r = call(&mut h, "set_text_style", json!({"action": "set", "name": "Heading / L", "fontSize": 56}));
    assert_eq!(r["layersUpdated"], 1);
    assert_eq!(r["style"]["bold"], true, "fields not given are kept");
    assert_eq!(size(&h, "Title"), 56.0);
    call(&mut h, "undo", json!({}));
    assert_eq!(size(&h, "Title"), 40.0);
    call(&mut h, "redo", json!({}));

    let r = call(&mut h, "set_text_style", json!({"action": "delete", "name": "Heading / L"}));
    assert_eq!(r["layersDetached"], 1);
    assert_eq!(size(&h, "Title"), 56.0, "a detached layer keeps its look");
    assert!(h.ed.doc.text_styles.is_empty());
    assert!(h.ed.doc.nodes.values().all(|n| n.text_style.style_ref.is_none()));
}
