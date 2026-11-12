//! An icon that cannot be placed leaves a visible dashed frame and the closest names in the
//! warnings; an icon of a loaded bundle is placed. Its own test binary with a single test: it
//! sets `PAPERCRAFT_ICONS` once and the bundle store is process-wide.

use papercraft::mcp::{Out, exec};
use papercraft::project::Store;
use serde_json::{Value, json};

const BUNDLE: &str = r#"{"license":"ISC","icons":{"check":"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\"><path d=\"M20 6 9 17l-5-5\"/></svg>"}}"#;

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Result<Value, String> {
    exec::call(h, name, &args).map(|outs| match outs.into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    })
}

#[test]
fn a_missing_icon_leaves_a_dashed_frame_and_names_the_closest_icons() {
    let root = std::env::temp_dir().join(format!("papercraft-icph-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the only test in this binary, so nothing else is reading the environment.
    unsafe { std::env::set_var("PAPERCRAFT_ICONS", &root) };
    papercraft::icons::add_bundle("lucide", BUNDLE.as_bytes()).unwrap();

    let store = Store::open(
        std::env::temp_dir().join(format!("papercraft-icph-s-{:016x}", fastrand::u64(..))),
    )
    .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Icons").unwrap();
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "S", "styles": {"width": "100px", "height": "100px"}}),
    )
    .unwrap();
    let r = call(
        &mut h,
        "write_html",
        json!({"targetNodeId": ab["id"], "mode": "insert-children",
               "html": "<x-paper-icon name=\"lucide:check\" size=\"20\"/><x-paper-icon name=\"lucide:check-mark\" size=\"20\"/>"}),
    )
    .unwrap();

    let warnings = r["warnings"].to_string();
    assert!(warnings.contains("lucide:check"), "closest names: {r}");
    assert!(warnings.contains("missing icon: lucide:check-mark"), "{r}");

    let named = |name: &str| {
        h.ed.doc
            .paint_order()
            .into_iter()
            .filter_map(|i| h.ed.doc.get(i))
            .find(|n| n.name == name)
            .cloned()
    };
    assert!(
        named("Icon / lucide:check").is_some(),
        "the bundled icon is placed"
    );
    let hole =
        named("missing icon: lucide:check-mark").expect("the placeholder is in the document");
    assert_eq!((hole.rect.w, hole.rect.h), (20.0, 20.0));
    assert!(!hole.style.dash.is_empty() && hole.style.stroke.is_some());
    std::fs::remove_dir_all(root).ok();
}
