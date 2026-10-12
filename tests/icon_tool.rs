//! `<x-paper-icon>` and `search_icons` against a throwaway icon library. Its own test binary
//! with a single test: it sets `PAPERCRAFT_ICONS` once, before anything reads the environment.

use papercraft::mcp::{Out, exec};
use papercraft::model::{NodeId, NodeKind, Paint};
use papercraft::project::Store;
use serde_json::{Value, json};

const STROKE_ICON: &str = r#"<!-- @license test -->
<svg class="x" xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"
 fill="none" stroke="currentColor" stroke-width="2"><path d="M20 6 9 17l-5-5" /></svg>"#;
const FILL_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect x="32" y="32" width="192" height="192" fill="currentColor"/></svg>"#;

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Result<Value, String> {
    exec::call(h, name, &args).map(|outs| match outs.into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    })
}

#[test]
fn icons_place_as_vector_layers_that_follow_a_variable() {
    let root = std::env::temp_dir().join(format!("papercraft-icon-lib-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(root.join("test")).unwrap();
    std::fs::write(root.join("test/check.svg"), STROKE_ICON).unwrap();
    std::fs::write(root.join("test/square.svg"), FILL_ICON).unwrap();
    // SAFETY: the only test in this binary, so nothing else is reading the environment.
    unsafe { std::env::set_var("PAPERCRAFT_ICONS", &root) };

    let store =
        Store::open(std::env::temp_dir().join(format!("papercraft-ic-{:016x}", fastrand::u64(..))))
            .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Icons").unwrap();

    let found = call(&mut h, "search_icons", json!({"query": "chec"})).unwrap();
    assert_eq!(found["results"], json!(["test:check"]));
    assert_eq!(found["installed"][0]["icons"], 2);
    let none = call(&mut h, "search_icons", json!({})).unwrap();
    assert!(none["installable"].to_string().contains("lucide"));

    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#cba6f7"}}),
    )
    .unwrap();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "S", "styles": {"width": "100px", "height": "100px", "display": "flex"}}),
    )
    .unwrap();
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": "<x-paper-icon name=\"test:check\" size=\"20\" color=\"#ff0000\" layer-name=\"Icon / Done\"/><x-paper-icon name=\"test:square\" size=\"16\" color=\"var(--accent)\"/><x-paper-icon name=\"test:missing\"/>"}),
    )
    .unwrap();
    let ids = r["createdNodeIds"].as_array().unwrap();
    assert_eq!(ids.len(), 2, "{r}");
    assert!(
        r.to_string().contains("not found"),
        "the missing icon is reported: {r}"
    );
    fn node<'a>(h: &'a exec::SessionHost, v: &Value) -> &'a papercraft::model::Node {
        h.ed.doc
            .get(NodeId(
                v.as_str()
                    .unwrap()
                    .rsplit(':')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap(),
            ))
            .unwrap()
    }
    assert_eq!(node(&h, &ids[0]).name, "Icon / Done");
    assert!((node(&h, &ids[0]).rect.w - 20.0).abs() < 0.5);
    // A 256-unit viewBox is scaled to the requested size, not left at 256.
    let b =
        h.ed.doc
            .world_rect(NodeId(
                ids[1]
                    .as_str()
                    .unwrap()
                    .rsplit(':')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap(),
            ))
            .unwrap();
    assert!(b.w <= 16.5 && b.h <= 16.5, "{b:?}");
    let inner =
        h.ed.doc
            .subtree(node(&h, &ids[1]).id)
            .into_iter()
            .filter_map(|i| h.ed.doc.world_rect(i))
            .fold(0.0f64, |m, r| m.max(r.w));
    assert!(inner <= 16.5, "shapes scaled too: {inner}");
    // The fill icon's shape follows `accent`; changing the variable recolors it.
    let tree: Vec<NodeId> = h.ed.doc.subtree(NodeId(
        ids[1]
            .as_str()
            .unwrap()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap(),
    ));
    let linked = |h: &exec::SessionHost| {
        tree.iter().filter_map(|i| h.ed.doc.get(*i)).any(|n| {
            n.style
                .fills
                .iter()
                .any(|p| matches!(p, Paint::Solid { var: Some(v), .. } if v == "accent"))
        })
    };
    assert!(linked(&h));
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#1e66f5"}}),
    )
    .unwrap();
    assert!(tree.iter().filter_map(|i| h.ed.doc.get(*i)).any(|n| {
        n.style
            .fills
            .iter()
            .any(|p| p.preview_color().to_hex() == "#1e66f5")
    }));
    assert!(matches!(
        node(&h, &ids[0]).kind,
        NodeKind::Group | NodeKind::Frame { .. } | NodeKind::Path { .. }
    ));
    std::fs::remove_dir_all(root).ok();
}
