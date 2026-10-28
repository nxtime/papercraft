//! Screens over MCP: `set_screens`, edits refused on layers hidden behind a drawer (unless
//! `allowHidden`), partly covered ones reported, and screens listed by get_basic_info.

use papercraft::mcp::{Out, exec};
use papercraft::project::Store;
use serde_json::{Value, json};

fn call_raw(h: &mut exec::SessionHost, name: &str, args: Value) -> Result<Vec<Value>, String> {
    exec::call(h, name, &args).map(|o| {
        o.into_iter()
            .filter_map(|o| match o {
                Out::Text(t) => serde_json::from_str(&t).ok(),
                _ => None,
            })
            .collect()
    })
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    call_raw(h, name, args).unwrap_or_else(|e| panic!("{name} failed: {e}")).remove(0)
}

#[test]
fn drawer_blocks_agent_edits_underneath() {
    let path = std::env::temp_dir().join(format!("papercraft-screens-{:016x}", fastrand::u64(..)));
    let mut h = exec::SessionHost::new(Store::open(&path).unwrap());
    h.session.create(&mut h.ed, "Screens").unwrap();
    let h = &mut h;
    let screen = call(h, "create_artboard", json!({"name": "Home", "styles": {"left": "0px", "top": "0px", "width": "390px", "height": "844px"}}))["id"].clone();
    call(h, "write_html", json!({"targetNodeId": screen, "mode": "insert-children", "html": concat!(
        r#"<div layer-name="Under" style="position:absolute;left:0px;top:0px;width:200px;height:100px;background:#ff0000"></div>"#,
        r#"<div layer-name="Half" style="position:absolute;left:100px;top:200px;width:200px;height:100px;background:#00ff00"></div>"#,
        r#"<div layer-name="Drawer" style="position:absolute;left:0px;top:0px;width:200px;height:844px;background:#ffffff"></div>"#)}));
    let kids = call(h, "get_children", json!({"nodeId": screen}))["children"].clone();
    let id = |name: &str| kids.as_array().unwrap().iter().find(|k| k["name"] == name).unwrap()["id"].clone();
    let (under, half) = (id("Under"), id("Half"));

    // Not a screen yet: nothing is guarded.
    call(h, "update_styles", json!({"updates": [{"nodeIds": [under], "styles": {"opacity": 0.9}}]}));
    call(h, "set_screens", json!({"nodeIds": [screen]}));
    let info = call(h, "get_basic_info", json!({}));
    assert_eq!(info["screens"][0]["device"], "Mobile", "{info}");

    let err = call_raw(h, "update_styles", json!({"updates": [{"nodeIds": [under], "styles": {"opacity": 1}}]})).unwrap_err();
    assert!(err.contains("refused") && err.contains("Drawer"), "{err}");
    assert_eq!(call(h, "get_node_info", json!({"nodeId": under}))["screen"]["state"], "hidden");

    let out = call_raw(h, "update_styles", json!({"updates": [{"nodeIds": [half], "styles": {"opacity": 1}}]})).unwrap();
    assert_eq!(out[1]["partlyCovered"][0]["visible"], "50%", "{out:?}");

    let out = call_raw(h, "update_styles", json!({"allowHidden": true, "updates": [{"nodeIds": [under], "styles": {"opacity": 1}}]})).unwrap();
    assert!(out[1]["editedWhileHidden"].is_array(), "{out:?}");

    // A section can't be a screen once it holds one.
    let err = call_raw(h, "set_screens", json!({"nodeIds": [under], "role": "screen"})).unwrap_err();
    assert!(err.contains("inside a screen"), "{err}");
    let _ = std::fs::remove_dir_all(path);
}
