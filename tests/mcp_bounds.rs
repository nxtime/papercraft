//! `lint_design`'s `overflow` rule and the `get_bounds` read, on a headless session.

use papercraft::mcp::{Out, exec};
use papercraft::model::{NodeId, NodeKind};
use papercraft::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let s = Store::open(std::env::temp_dir().join(format!("papercraft-mcp-{:016x}", fastrand::u64(..)))).unwrap();
    let mut h = exec::SessionHost::new(s);
    h.session.create(&mut h.ed, "Bounds test").unwrap();
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

fn num(id: &Value) -> NodeId {
    NodeId(id.as_str().unwrap().rsplit(':').next().unwrap().parse().unwrap())
}

fn key(h: &exec::SessionHost, id: NodeId) -> String {
    exec::node_key(&h.ed, h.ed.active_page, id)
}

/// An artboard with a 100x20 "Chip" child; tests then move its rect so layout doesn't interfere.
fn board_with_chip(h: &mut exec::SessionHost, extra: Value) -> (NodeId, NodeId) {
    let mut styles = json!({"width": "390px", "height": "400px"});
    styles.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    let ab = call(h, "create_artboard", json!({"name": "Phone", "styles": styles}));
    let r = call(
        h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": r#"<div layer-name="Chip" style="width:100px; height:20px; background:#2563eb"></div>"#}),
    );
    (num(&ab["id"]), num(&r["createdNodeIds"][0]))
}

fn overflow_issues(h: &mut exec::SessionHost) -> Vec<Value> {
    let r = call(h, "lint_design", json!({}));
    r["issues"].as_array().unwrap().iter().filter(|i| i["rule"] == "overflow").cloned().collect()
}

fn message(hit: &Value) -> &str {
    hit["message"].as_str().unwrap()
}

#[test]
fn a_child_past_the_right_edge_is_flagged() {
    let mut h = host();
    let (_, chip) = board_with_chip(&mut h, json!({}));
    h.ed.doc.nodes.get_mut(&chip).unwrap().rect.x = 330.0;
    let hits = overflow_issues(&mut h);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0]["name"], "Chip");
    assert!(message(&hits[0]).contains("40px past the right edge"), "{hits:?}");
    assert!(!hits[0]["fix"].as_str().unwrap().is_empty());
}

#[test]
fn a_clipping_frame_is_checked_inside_the_artboard() {
    let mut h = host();
    let (board, _) = board_with_chip(&mut h, json!({}));
    let board_key = key(&h, board);
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": board_key, "mode": "insert-children",
               "html": r#"<div layer-name="Clip" style="width:120px; height:60px; overflow:hidden"><div layer-name="Wide" style="width:90px; height:20px; background:#111"></div></div>"#}),
    );
    let clip = num(&r["createdNodeIds"][0]);
    let wide = h.ed.doc.get(clip).unwrap().children[0];
    h.ed.doc.nodes.get_mut(&wide).unwrap().rect.w = 150.0;
    let hits = overflow_issues(&mut h);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(message(&hits[0]).contains("`Clip`"), "{hits:?}");
}

#[test]
fn a_scrolling_screen_and_a_clean_design_are_not_flagged() {
    let mut h = host();
    let (board, chip) = board_with_chip(&mut h, json!({"overflow": "auto"}));
    assert!(h.ed.doc.get(board).unwrap().scroll);
    h.ed.doc.nodes.get_mut(&chip).unwrap().rect.h = 3000.0;
    assert!(overflow_issues(&mut h).is_empty());

    let mut clean = host();
    board_with_chip(&mut clean, json!({}));
    assert!(overflow_issues(&mut clean).is_empty());
}

#[test]
fn long_text_in_a_fixed_box_is_flagged() {
    let mut h = host();
    let (board, _) = board_with_chip(&mut h, json!({}));
    let board_key = key(&h, board);
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": board_key, "mode": "insert-children",
               "html": "<p style=\"font-size:16px\">A very long title that has to wrap onto several lines in a narrow box</p>"}),
    );
    let t = num(&r["createdNodeIds"][0]);
    let n = h.ed.doc.nodes.get_mut(&t).unwrap();
    if let NodeKind::Text { auto_width, .. } = &mut n.kind {
        *auto_width = false;
    }
    n.rect.w = 80.0;
    n.rect.h = 20.0;
    let hits = overflow_issues(&mut h);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(message(&hits[0]).contains("taller than its 20px box"), "{hits:?}");
}

#[test]
fn get_bounds_lists_every_layer_in_one_read_without_touching_the_document() {
    let mut h = host();
    let (board, chip) = board_with_chip(&mut h, json!({}));
    let before = (h.ed.revision, h.ed.doc_stamp);
    let r = call(&mut h, "get_bounds", json!({}));
    assert_eq!((h.ed.revision, h.ed.doc_stamp), before, "a read must not change the document");
    assert_eq!(r["columns"], json!(["id", "parent", "x", "y", "w", "h", "absX", "absY", "clip"]));
    let rows = r["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{r}");
    assert_eq!(rows[0][1], Value::Null);
    assert_eq!((&rows[0][4], &rows[0][5], &rows[0][8]), (&json!(390.0), &json!(400.0), &json!(true)));
    assert_eq!(rows[1][1], rows[0][0]);
    assert_eq!(rows[1][0], json!(key(&h, chip)));
    assert!(r.get("truncated").is_none());

    let (board_key, chip_key) = (key(&h, board), key(&h, chip));
    let one = call(&mut h, "get_bounds", json!({"nodeIds": [board_key], "depth": 1}));
    assert_eq!(one["rows"].as_array().unwrap().len(), 1);
    let sub = call(&mut h, "get_bounds", json!({"nodeIds": [chip_key]}));
    assert_eq!(sub["rows"].as_array().unwrap().len(), 1);
}

#[test]
fn get_bounds_rejects_an_unknown_node() {
    let mut h = host();
    assert!(exec::call(&mut h, "get_bounds", &json!({"nodeIds": ["999999"]})).is_err());
}
