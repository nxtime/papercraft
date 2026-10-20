//! OpenDesign design-system packages: parsing `tokens.css`, importing a package folder into
//! the project and exporting one back.

use papercraft::design_system as ds;
use papercraft::mcp::{Out, exec};
use papercraft::model::{Color, Document, NodeId, Paint};
use papercraft::project::Store;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("papercraft-od-{tag}-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The id of a package folder: its slug.
fn folder_id(dir: &Path) -> String {
    ds::slug(&dir.file_name().unwrap().to_string_lossy())
}

/// Write a package folder; the id is the slug of the folder.
fn write_package(dir: &Path) -> String {
    let id = folder_id(dir);
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": "od-design-system-project/v1",
            "id": id,
            "name": "My System",
            "category": "product",
            "description": "A small system",
            "source": {"type": "github", "url": "https://example.com/system"},
            "files": {"design": "DESIGN.md", "tokens": "tokens.css"},
            "importMode": "hybrid",
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("tokens.css"),
        ":root {\n  --bg: #ffffff;\n  --accent: #cba6f7;\n  --spacing-md: 16px;\n}\n\
         [data-theme=\"Dark\"] {\n  --bg: #1e1e2e;\n  --accent: #89b4fa;\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("DESIGN.md"), "# My System\n\nWarm and calm.\n").unwrap();
    id
}

fn host() -> exec::SessionHost {
    let store =
        Store::open(std::env::temp_dir().join(format!("papercraft-od-{:016x}", fastrand::u64(..))))
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

/// The color of the last fill of a node.
fn fill(h: &exec::SessionHost, id: &Value) -> Color {
    let id = NodeId(
        id.as_str()
            .unwrap()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap(),
    );
    match h.ed.doc.get(id).unwrap().style.fills.last() {
        Some(Paint::Solid { color, .. }) => *color,
        other => panic!("not a solid fill: {other:?}"),
    }
}

#[test]
fn tokens_css_splits_colors_tokens_and_modes() {
    let css = "\
:root {
  --bg: #ffffff;
  --accent: #cba6f7;
  --spacing-md: 16px;
  --shadow-sm: 0 1px 2px rgba(0,0,0,.06);
  --brand: oklch(0.7 0.1 200);
}
[data-theme=\"Dark\"] {
  --bg: #1e1e2e;
  --accent: #89b4fa;
}
";
    let t = ds::parse_tokens_css(css);
    assert_eq!(t.variables["bg"], Color::rgb(255, 255, 255));
    assert_eq!(t.variables["accent"], Color::rgb(0xcb, 0xa6, 0xf7));
    assert_eq!(t.tokens["spacing-md"], "16px");
    assert_eq!(t.tokens["shadow-sm"], "0 1px 2px rgba(0,0,0,.06)");
    // A color PaperCraft cannot hold stays a token.
    assert_eq!(t.tokens["brand"], "oklch(0.7 0.1 200)");
    assert!(!t.variables.contains_key("spacing-md"));
    assert_eq!(t.modes["Dark"]["bg"], Color::rgb(0x1e, 0x1e, 0x2e));
    assert_eq!(t.modes["Dark"]["accent"], Color::rgb(0x89, 0xb4, 0xfa));
    assert!(!t.modes["Dark"].contains_key("spacing-md"));
}

#[test]
fn a_package_survives_an_import_and_an_export() {
    let dir = tmp_dir("round-trip");
    let id = write_package(&dir);
    let pkg = ds::import_package(&dir).unwrap();
    assert_eq!(pkg.manifest.id, id);
    assert_eq!(pkg.manifest.name, "My System");
    assert_eq!(pkg.manifest.category, "product");
    assert_eq!(pkg.manifest.description.as_deref(), Some("A small system"));
    assert_eq!(pkg.manifest.source.kind, "github");
    assert_eq!(
        pkg.manifest.source.extra["url"],
        "https://example.com/system"
    );
    assert_eq!(pkg.manifest.import_mode.as_deref(), Some("hybrid"));
    assert!(pkg.design.as_deref().unwrap().contains("Warm and calm"));

    // Export the imported values again: the tokens file comes back the same.
    let doc = Document {
        variables: pkg.tokens.variables.clone(),
        modes: pkg.tokens.modes.clone(),
        tokens: pkg.tokens.tokens.clone(),
        ..Default::default()
    };
    let out = tmp_dir("round-trip-out");
    let written = ds::export_package(&out, &doc).unwrap();
    assert_eq!(written.len(), 3);
    assert!(written.iter().all(|p| p.exists()));
    let again = ds::import_package(&out).unwrap();
    assert_eq!(again.tokens.variables, pkg.tokens.variables);
    assert_eq!(again.tokens.modes, pkg.tokens.modes);
    assert_eq!(again.tokens.tokens, pkg.tokens.tokens);

    // The manifest carries the keys the format asks for.
    let raw = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    let m: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(m["schemaVersion"], "od-design-system-project/v1");
    assert_eq!(m["id"], folder_id(&out));
    assert_eq!(m["name"], "Untitled");
    assert_eq!(m["category"], "design-system");
    assert_eq!(m["source"]["type"], "local");
    assert_eq!(m["files"]["design"], "DESIGN.md");
    assert_eq!(m["files"]["tokens"], "tokens.css");
}

#[test]
fn names_are_made_css_safe() {
    assert_eq!(ds::sanitize_name("--Brand Color"), "Brand-Color");
    assert_eq!(ds::sanitize_name("spacing.md"), "spacing-md");
    assert_eq!(ds::sanitize_name("radius/md"), "radius-md");
    assert_eq!(ds::slug("My System"), "my-system");
    assert_eq!(ds::slug("Papercraft"), "papercraft");

    // A package with odd names still imports, with safe names.
    let dir = tmp_dir("names");
    let id = folder_id(&dir);
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": "od-design-system-project/v1",
            "id": id,
            "name": "Odd names",
            "category": "product",
            "source": {"type": "local"},
            "files": {"design": "DESIGN.md", "tokens": "tokens.css"},
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("tokens.css"),
        ":root {\n  --Brand Color: #ff0000;\n  --spacing.md: 8px;\n}\n",
    )
    .unwrap();
    let pkg = ds::import_package(&dir).unwrap();
    assert_eq!(pkg.tokens.variables["Brand-Color"], Color::rgb(255, 0, 0));
    assert_eq!(pkg.tokens.tokens["spacing-md"], "8px");
}

#[test]
fn a_malformed_manifest_is_an_error_not_a_panic() {
    let dir = tmp_dir("bad");
    let id = folder_id(&dir);

    // Not JSON at all.
    std::fs::write(dir.join("manifest.json"), "{ not json").unwrap();
    let err = ds::import_package(&dir).unwrap_err();
    assert!(err.contains("malformed"), "{err}");

    // No `files`.
    std::fs::write(
        dir.join("manifest.json"),
        json!({"schemaVersion": "od-design-system-project/v1", "id": id, "name": "Bad",
               "category": "product", "source": {"type": "local"}})
        .to_string(),
    )
    .unwrap();
    let err = ds::import_package(&dir).unwrap_err();
    assert!(err.contains("files"), "{err}");

    // An id that is not a slug.
    std::fs::write(
        dir.join("manifest.json"),
        json!({"schemaVersion": "od-design-system-project/v1", "id": "Not A Slug",
               "name": "Bad", "category": "product", "source": {"type": "local"},
               "files": {"design": "DESIGN.md", "tokens": "tokens.css"}})
        .to_string(),
    )
    .unwrap();
    let err = ds::import_package(&dir).unwrap_err();
    assert!(err.contains("slug"), "{err}");

    // Another schema version.
    std::fs::write(
        dir.join("manifest.json"),
        json!({"schemaVersion": "od-design-system-project/v2", "id": id, "name": "Bad",
               "category": "product", "source": {"type": "local"},
               "files": {"design": "DESIGN.md", "tokens": "tokens.css"}})
        .to_string(),
    )
    .unwrap();
    let err = ds::import_package(&dir).unwrap_err();
    assert!(err.contains("schemaVersion"), "{err}");

    // A tokens file that is not there.
    std::fs::write(
        dir.join("manifest.json"),
        json!({"schemaVersion": "od-design-system-project/v1", "id": id, "name": "Bad",
               "category": "product", "source": {"type": "local"},
               "files": {"design": "DESIGN.md", "tokens": "tokens.css"}})
        .to_string(),
    )
    .unwrap();
    let err = ds::import_package(&dir).unwrap_err();
    assert!(err.contains("tokens.css"), "{err}");
}

#[test]
fn the_tools_import_into_every_page_and_export_again() {
    let mut h = host();
    // A fill that follows a variable moves when the package brings a new value.
    call(
        &mut h,
        "set_variables",
        json!({"variables": {"accent": "#ff0000"}}),
    );
    let ab = call(
        &mut h,
        "create_artboard",
        json!({"name": "Screen", "styles": {"width": "100px", "height": "100px",
               "backgroundColor": "var(--accent)"}}),
    );
    assert_eq!(fill(&h, &ab["id"]), Color::rgb(255, 0, 0));

    let dir = tmp_dir("tools");
    write_package(&dir);
    let r = call(
        &mut h,
        "import_design_system",
        json!({"path": dir.to_string_lossy()}),
    );
    assert_eq!(r["variables"], 2);
    assert_eq!(r["modes"], 1);
    assert_eq!(r["tokens"], 1);
    assert_eq!(r["package"]["name"], "My System");
    assert_eq!(fill(&h, &ab["id"]), Color::rgb(0xcb, 0xa6, 0xf7));

    let vars = call(&mut h, "get_variables", json!({}));
    assert_eq!(vars["variables"]["accent"], "#cba6f7");
    assert_eq!(vars["variables"]["bg"], "#ffffff");
    assert_eq!(vars["tokens"]["spacing-md"], "16px");
    assert_eq!(vars["modes"][0], "Dark");

    // Every page has the design system, not only the one the user looks at.
    let p2 = call(&mut h, "create_page", json!({"name": "Second"}));
    let vars = call(&mut h, "get_variables", json!({"pageId": p2["pageId"]}));
    assert_eq!(vars["variables"]["accent"], "#cba6f7");

    // One undo step takes it back.
    call(&mut h, "undo", json!({}));
    let vars = call(&mut h, "get_variables", json!({}));
    assert_eq!(vars["variables"]["accent"], "#ff0000");
    assert_eq!(fill(&h, &ab["id"]), Color::rgb(255, 0, 0));

    // Export the design system again.
    call(
        &mut h,
        "import_design_system",
        json!({"path": dir.to_string_lossy()}),
    );
    let out = tmp_dir("tools-out");
    let r = call(
        &mut h,
        "export_design_system",
        json!({"path": out.to_string_lossy()}),
    );
    let files = r["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert!(out.join("manifest.json").exists());
    let css = std::fs::read_to_string(out.join("tokens.css")).unwrap();
    assert!(css.contains("--accent: #cba6f7;"), "{css}");
    assert!(css.contains("[data-theme=\"Dark\"]"), "{css}");
    let pkg = ds::import_package(&out).unwrap();
    assert_eq!(pkg.tokens.variables["accent"], Color::rgb(0xcb, 0xa6, 0xf7));
}
