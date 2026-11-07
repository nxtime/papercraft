//! `write_html` honours CSS margin as a browser lays it out: offsets, shorthand, `auto`
//! centring, and sibling collapse in block flow only.

use papercraft::mcp::{Out, exec};
use papercraft::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let s = Store::open(std::env::temp_dir().join(format!("papercraft-margin-{:016x}", fastrand::u64(..)))).unwrap();
    let mut h = exec::SessionHost::new(s);
    h.session.create(&mut h.ed, "Margin test").unwrap();
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

/// Builds `inner` inside a 400px-wide `Box` and returns the host and the write result.
fn build(box_style: &str, inner: &str) -> (exec::SessionHost, Value) {
    let mut h = host();
    let ab = call(&mut h, "create_artboard", json!({"name": "Art", "styles": {"width": "600px", "height": "600px"}}));
    let html = format!("<div layer-name=\"Box\" style=\"width:400px;{box_style}\">{inner}</div>");
    let w = call(&mut h, "write_html", json!({"targetNodeId": ab["id"], "mode": "insert-children", "html": html}));
    (h, w)
}

/// `(x, y, w, h)` of the layer called `name`, relative to the `Box`.
fn rect(h: &exec::SessionHost, name: &str) -> (f64, f64, f64, f64) {
    let doc = &h.ed.doc;
    let find = |n: &str| doc.nodes.values().find(|x| x.name == n).unwrap_or_else(|| panic!("no layer {n}"));
    let target = find(name);
    let (mut x, mut y) = (0.0, 0.0);
    let mut cur = target;
    while cur.name != "Box" {
        x += cur.rect.x;
        y += cur.rect.y;
        cur = doc.get(cur.parent.expect("inside the Box")).unwrap();
    }
    (x, y, target.rect.w, target.rect.h)
}

fn near(got: (f64, f64, f64, f64), want: (f64, f64, f64, f64)) {
    let ok = |a: f64, b: f64| (a - b).abs() < 0.6;
    assert!(ok(got.0, want.0) && ok(got.1, want.1) && ok(got.2, want.2) && ok(got.3, want.3), "got {got:?}, a browser gives {want:?}");
}

#[test]
fn a_block_child_with_margin_is_inset_and_narrowed() {
    let (h, w) = build("", "<div layer-name=\"A\" style=\"height:20px;margin:10px 20px\"></div>");
    near(rect(&h, "A"), (20.0, 10.0, 360.0, 20.0));
    assert!(!w["ignoredStyles"].to_string().contains("margin"), "{w}");
}

#[test]
fn shorthand_and_per_side_margins_agree_with_the_browser() {
    let (h, _) = build("", "<div layer-name=\"A\" style=\"height:20px;margin:1px 2px 3px 4px\"></div><div layer-name=\"B\" style=\"height:20px;margin-top:5px;margin-left:7px;margin-right:3px\"></div>");
    near(rect(&h, "A"), (4.0, 1.0, 394.0, 20.0));
    // A's 3px bottom and B's 5px top collapse to 5px.
    near(rect(&h, "B"), (7.0, 1.0 + 20.0 + 5.0, 390.0, 20.0));
}

#[test]
fn auto_side_margins_centre_a_fixed_width_block() {
    let (h, _) = build("", "<div layer-name=\"A\" style=\"width:200px;height:20px;margin:0 auto\"></div><div layer-name=\"B\" style=\"width:100px;height:20px;margin-left:auto\"></div>");
    near(rect(&h, "A"), (100.0, 0.0, 200.0, 20.0));
    near(rect(&h, "B"), (300.0, 20.0, 100.0, 20.0));
}

#[test]
fn block_siblings_collapse_their_vertical_margins_and_flex_items_do_not() {
    let pair = "<div layer-name=\"A\" style=\"height:20px;margin-bottom:30px\"></div><div layer-name=\"B\" style=\"height:20px;margin-top:10px\"></div>";
    let (h, _) = build("", pair);
    near(rect(&h, "B"), (0.0, 50.0, 400.0, 20.0));
    let (h, _) = build("display:flex;flex-direction:column", pair);
    near(rect(&h, "B"), (0.0, 60.0, 400.0, 20.0));
}

#[test]
fn flex_row_margins_add_and_auto_pushes_to_the_end() {
    let (h, _) = build(
        "display:flex",
        "<div layer-name=\"A\" style=\"width:50px;height:20px\"></div><div layer-name=\"B\" style=\"width:50px;height:20px;margin-left:10px;margin-right:5px;margin-top:4px\"></div>",
    );
    near(rect(&h, "B"), (60.0, 4.0, 50.0, 20.0));
    let (h, _) = build(
        "display:flex",
        "<div layer-name=\"A\" style=\"width:50px;height:20px\"></div><div layer-name=\"B\" style=\"width:50px;height:20px;margin-left:auto\"></div>",
    );
    near(rect(&h, "B"), (350.0, 0.0, 50.0, 20.0));
    let (h, _) = build(
        "display:flex;height:100px",
        "<div layer-name=\"A\" style=\"width:50px;height:20px;margin:auto\"></div>",
    );
    near(rect(&h, "A"), (175.0, 40.0, 50.0, 20.0));
}

#[test]
fn absolute_boxes_shift_by_their_margin() {
    let (h, _) = build(
        "position:relative;height:100px",
        "<div layer-name=\"A\" style=\"position:absolute;left:10px;top:20px;width:30px;height:30px;margin:5px 6px\"></div>\
         <div layer-name=\"B\" style=\"position:absolute;right:10px;bottom:10px;width:30px;height:30px;margin:5px 6px\"></div>",
    );
    near(rect(&h, "A"), (16.0, 25.0, 30.0, 30.0));
    near(rect(&h, "B"), (400.0 - 30.0 - 10.0 - 6.0, 100.0 - 30.0 - 10.0 - 5.0, 30.0, 30.0));
}

#[test]
fn undoing_the_write_removes_the_margin_wrappers_too() {
    let (mut h, _) = build("", "<div style=\"height:20px;margin:8px\"></div>");
    call(&mut h, "undo", json!({}));
    assert!(h.ed.doc.nodes.values().all(|n| n.name != "Box"));
}

