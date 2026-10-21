//! An icon whose color is a `var(--x)` that does not exist: the tool reports it and the icon
//! keeps the inherited color instead of turning black. Its own test binary with a single test:
//! it sets `PAPERCRAFT_ICONS` once, before anything reads the environment.

use papercraft::mcp::{Out, exec};
use papercraft::project::Store;
use serde_json::{Value, json};

const STROKE_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 6 9 17l-5-5" /></svg>"#;

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Result<Value, String> {
    exec::call(h, name, &args).map(|outs| match outs.into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    })
}

#[test]
fn an_icon_with_an_unresolvable_variable_warns_and_keeps_the_inherited_color() {
    let root = std::env::temp_dir().join(format!("papercraft-icon-fb-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(root.join("test")).unwrap();
    std::fs::write(root.join("test/check.svg"), STROKE_ICON).unwrap();
    // SAFETY: the only test in this binary, so nothing else is reading the environment.
    unsafe { std::env::set_var("PAPERCRAFT_ICONS", &root) };

    let store = Store::open(
        std::env::temp_dir().join(format!("papercraft-icfb-{:016x}", fastrand::u64(..))),
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
               "html": "<div style=\"color: #ff8800\"><x-paper-icon name=\"test:check\" size=\"20\" color=\"var(--nope)\"/></div>"}),
    )
    .unwrap();

    // The variable that does not exist is reported.
    let warnings = r["warnings"]
        .as_array()
        .map(|w| {
            w.iter()
                .filter_map(|w| w.as_str())
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default();
    assert!(warnings.contains("x-paper-icon"), "{r}");
    assert!(warnings.contains("--nope"), "{r}");

    // The icon is not black: it keeps the color it inherits.
    let icon =
        h.ed.doc
            .paint_order()
            .into_iter()
            .find(|i| {
                h.ed.doc
                    .get(*i)
                    .is_some_and(|n| n.name == "Icon / test:check")
            })
            .expect("the icon was placed");
    let strokes: Vec<String> =
        h.ed.doc
            .subtree(icon)
            .into_iter()
            .filter_map(|i| h.ed.doc.get(i))
            .filter_map(|n| n.style.stroke)
            .map(|c| c.to_hex())
            .collect();
    assert!(!strokes.is_empty(), "the icon has a stroke");
    assert!(
        strokes.iter().all(|c| c != "#000000"),
        "the icon is not black: {strokes:?}"
    );
    assert!(
        strokes.iter().any(|c| c == "#ff8800"),
        "the icon keeps the inherited color: {strokes:?}"
    );
    std::fs::remove_dir_all(root).ok();
}
