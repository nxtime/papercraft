//! Publishing a project as a library and using it from another project.

use papercraft::libraries::{self, LibrarySummary};
use papercraft::mcp::{Out, exec};
use papercraft::model::{NodeId, Paint};
use papercraft::project::Store;
use serde_json::{Value, json};
use std::path::PathBuf;

fn host() -> exec::SessionHost {
    let root = std::env::temp_dir().join(format!("papercraft-lib-{:016x}", fastrand::u64(..)));
    exec::SessionHost::new(Store::open(root).unwrap())
}

fn try_call(h: &mut exec::SessionHost, name: &str, args: Value) -> Result<Value, String> {
    exec::call(h, name, &args).map(|o| match o.into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    })
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    try_call(h, name, args).unwrap_or_else(|e| panic!("{name} failed: {e}"))
}

fn num(id: &Value) -> NodeId {
    NodeId(id.as_str().unwrap().rsplit(':').next().unwrap().parse().unwrap())
}

fn dir(h: &exec::SessionHost) -> PathBuf {
    h.session.project.as_ref().unwrap().dir.clone()
}

/// Project "Kit" with variable `brand` and component "Pill" (red), saved. Returns its key.
fn make_kit(h: &mut exec::SessionHost, brand: &str) -> String {
    h.session.create(&mut h.ed, "Kit").unwrap();
    call(h, "set_variables", json!({"variables": {"brand": brand}}));
    let ab = call(h, "create_artboard", json!({"name": "Pill",
        "styles": {"width": "100px", "height": "40px", "backgroundColor": "var(--brand)"}}));
    call(h, "create_component", json!({"nodeId": ab["id"]}));
    h.session.save(&mut h.ed).unwrap();
    let c = call(h, "get_components", json!({}));
    c["components"][0]["componentKey"].as_str().unwrap().to_string()
}

fn kit_summary(h: &exec::SessionHost) -> LibrarySummary {
    libraries::available(&h.session.store, None)
        .into_iter()
        .find(|l| l.name == "Kit")
        .unwrap()
}

#[test]
fn publish_twice_increments_version_and_diffs() {
    let mut h = host();
    make_kit(&mut h, "#ff0000");
    let d = dir(&h);
    assert!(!libraries::publish_diff(&d, &h.ed).unwrap().is_empty());
    let p1 = libraries::publish(&d, &h.ed, "Kit v1").unwrap();
    assert_eq!((p1.version, p1.components.len()), (1, 1));
    assert!(libraries::publish_diff(&d, &h.ed).unwrap().is_empty());
    call(&mut h, "set_variables", json!({"variables": {"brand": "#00ff00", "extra": "#0000ff"}}));
    let diff = libraries::publish_diff(&d, &h.ed).unwrap();
    assert_eq!(diff.variables_changed, ["brand"]);
    assert_eq!(diff.variables_added, ["extra"]);
    // The component's bound fill color follows the variable, so it changed too.
    assert_eq!(diff.to_string(), "1 variable added, 1 variable changed, 1 component changed");
    let p2 = libraries::publish(&d, &h.ed, "").unwrap();
    assert_eq!((p2.version, p2.description.as_str()), (2, "Kit v1"));
    assert_eq!(p1.components[0].root, p2.components[0].root, "ids are stable");
    // The library itself is not offered to itself.
    assert!(libraries::available(&h.session.store, Some(&d)).is_empty());
}

#[test]
fn enable_in_second_project_binds_and_places_components() {
    let mut h = host();
    let key = make_kit(&mut h, "#ff0000");
    libraries::publish(&dir(&h), &h.ed, "").unwrap();
    h.session.create(&mut h.ed, "App").unwrap();
    let app = dir(&h);
    let lib = kit_summary(&h);
    libraries::enable(&app, &mut h.ed, &lib).unwrap();
    let col = h.ed.design.collections.iter().find(|c| c.library.is_some()).unwrap();
    assert_eq!(col.library.as_deref(), Some(lib.library_id.as_str()));
    assert!(app.join("libraries").join(format!("{}.json", lib.library_id)).is_file());
    assert_eq!(h.ed.design.libraries[0].version, 1);
    // Library collections are read-only.
    let id = h.ed.design.collections.iter().find(|c| c.library.is_some()).unwrap().id.clone();
    assert!(h.ed.edit_design("x", |ds| ds.add_mode(&id, "Dark").map(|_| ())).is_err());
    // A fill bound to the library variable resolves.
    let ab = call(&mut h, "create_artboard", json!({"name": "Card",
        "styles": {"width": "50px", "height": "50px", "backgroundColor": "var(--brand)"}}));
    match h.ed.doc.get(num(&ab["id"])).unwrap().style.fills.last() {
        Some(Paint::Solid { color, var }) => {
            assert_eq!((color.r, color.g), (255, 0));
            assert_eq!(var.as_deref(), Some("brand"));
        }
        o => panic!("{o:?}"),
    }
    // A library component instance places and renders its nodes.
    let inst = call(&mut h, "create_instance", json!({"componentKey": key}));
    let n = h.ed.doc.get(num(&inst["id"])).unwrap();
    assert!(matches!(n.instance.as_ref().unwrap().source, Some(papercraft::model::Source::Library(_))));
    assert_eq!((n.rect.w, n.rect.h), (100.0, 40.0));
    assert!(h.ed.is_instance_root(n.id));
    let svg = papercraft::io::svg::export(&h.ed.doc, Some(&[n.id]), None).unwrap();
    assert!(svg.contains("<rect") || svg.contains("<path"), "{svg}");
    // Reset overrides resyncs from the snapshot without losing the instance.
    let nid = n.id;
    h.ed.selection = vec![nid];
    h.ed.reset_overrides();
    assert!(h.ed.doc.get(nid).unwrap().instance.is_some());

    // Snapshot and design survive a save and reopen.
    h.session.save(&mut h.ed).unwrap();
    let design = h.ed.design.clone();
    h.session.create(&mut h.ed, "Other").unwrap();
    assert!(h.ed.library_docs.is_empty());
    h.session.open(&mut h.ed, &app).unwrap();
    assert_eq!(h.ed.design, design);
    assert!(h.ed.find_library_component(&key).is_some());
    let again = call(&mut h, "create_instance", json!({"componentKey": key}));
    assert!(h.ed.doc.get(num(&again["id"])).is_some());
}

#[test]
fn local_css_name_conflict_is_refused() {
    let mut h = host();
    make_kit(&mut h, "#ff0000");
    libraries::publish(&dir(&h), &h.ed, "").unwrap();
    h.session.create(&mut h.ed, "App").unwrap();
    call(&mut h, "set_variables", json!({"variables": {"brand": "#123456"}}));
    let before = h.ed.design.clone();
    let lib = kit_summary(&h);
    let e = libraries::enable(&dir(&h), &mut h.ed, &lib).unwrap_err();
    assert!(e.contains("--brand"), "{e}");
    assert_eq!(h.ed.design, before);
    assert!(h.ed.library_docs.is_empty());
    assert!(!dir(&h).join("libraries").join(format!("{}.json", lib.library_id)).exists());
}

fn fill_rgb(h: &exec::SessionHost, id: NodeId) -> (u8, u8, u8, Option<String>) {
    match h.ed.doc.get(id).unwrap().style.fills.last() {
        Some(Paint::Solid { color, var }) => (color.r, color.g, color.b, var.clone()),
        o => panic!("{o:?}"),
    }
}

/// Kit published (red), App enabled it with a bound artboard and an instance whose opacity is
/// overridden. Returns (kit dir, app dir, key, bound artboard, instance).
fn kit_and_app(h: &mut exec::SessionHost) -> (PathBuf, PathBuf, String, NodeId, NodeId) {
    let key = make_kit(h, "#ff0000");
    let kit = dir(h);
    libraries::publish(&kit, &h.ed, "").unwrap();
    h.session.create(&mut h.ed, "App").unwrap();
    let app = dir(h);
    let lib = kit_summary(h);
    libraries::enable(&app, &mut h.ed, &lib).unwrap();
    let ab = call(h, "create_artboard", json!({"name": "Card",
        "styles": {"width": "50px", "height": "50px", "backgroundColor": "var(--brand)"}}));
    let inst = call(h, "create_instance", json!({"componentKey": key}));
    call(h, "update_styles", json!({"updates": [{"nodeIds": [inst["id"]], "styles": {"opacity": "0.5"}}]}));
    h.session.save(&mut h.ed).unwrap();
    (kit, app, key, num(&ab["id"]), num(&inst["id"]))
}

#[test]
fn update_after_republish_keeps_overrides() {
    let mut h = host();
    let (kit, app, _key, card, inst) = kit_and_app(&mut h);
    let lib = kit_summary(&h);
    let st = libraries::status(&h.session.store, &app, &h.ed);
    assert_eq!((st[0].local_version, st[0].source_version, st[0].update_available), (1, Some(1), false));
    assert_eq!(st[0].name, "Kit");

    // Change the library: variable color and component size, republish.
    h.session.open(&mut h.ed, &kit).unwrap();
    call(&mut h, "set_variables", json!({"variables": {"brand": "#00ff00"}}));
    let c = call(&mut h, "get_components", json!({}));
    call(&mut h, "update_styles", json!({"updates": [{"nodeIds": [c["components"][0]["id"]], "styles": {"width": "120px"}}]}));
    libraries::publish(&kit, &h.ed, "").unwrap();
    h.session.save(&mut h.ed).unwrap();
    h.session.open(&mut h.ed, &app).unwrap();

    let st = libraries::status(&h.session.store, &app, &h.ed);
    assert_eq!((st[0].source_version, st[0].update_available), (Some(2), true));
    let d = libraries::update_diff(&h.session.store, &app, &lib.library_id).unwrap();
    assert_eq!((d.variables_changed.len(), d.components_changed.len()), (1, 1));
    assert_eq!(fill_rgb(&h, card).0, 255, "no silent update");

    libraries::update(&h.session.store, &app, &mut h.ed, &lib.library_id).unwrap();
    assert_eq!(fill_rgb(&h, card), (0, 255, 0, Some("brand".into())));
    let n = h.ed.doc.get(inst).unwrap();
    assert_eq!((n.rect.w, n.style.opacity), (120.0, 0.5), "size follows, override kept");
    assert_eq!(fill_rgb(&h, inst).0, 0);
    assert_eq!(h.ed.design.libraries[0].version, 2);
    let st = libraries::status(&h.session.store, &app, &h.ed);
    assert!(!st[0].update_available);
    h.ed.undo();
    assert_eq!(h.ed.doc.get(inst).unwrap().rect.w, 100.0, "one undo step restores the page");
    h.ed.redo();
    // Persisted: reopen keeps the new copy.
    h.session.save(&mut h.ed).unwrap();
    h.session.open(&mut h.ed, &app).unwrap();
    assert_eq!(h.ed.design.libraries[0].version, 2);
    assert_eq!(h.ed.library_docs.len(), 1);
}

#[test]
fn update_refused_on_clash_changes_nothing() {
    let mut h = host();
    let (kit, app, _key, _card, _inst) = kit_and_app(&mut h);
    let lib = kit_summary(&h);
    h.session.open(&mut h.ed, &kit).unwrap();
    call(&mut h, "set_variables", json!({"variables": {"accent": "#0000ff"}}));
    libraries::publish(&kit, &h.ed, "").unwrap();
    h.session.save(&mut h.ed).unwrap();
    h.session.open(&mut h.ed, &app).unwrap();
    call(&mut h, "set_variables", json!({"variables": {"accent": "#123456"}}));
    let before = h.ed.design.clone();
    let file = app.join("libraries").join(format!("{}.json", lib.library_id));
    let bytes = std::fs::read(&file).unwrap();
    let e = libraries::update(&h.session.store, &app, &mut h.ed, &lib.library_id).unwrap_err();
    assert!(e.contains("accent"), "{e}");
    assert_eq!(h.ed.design, before);
    assert_eq!(std::fs::read(&file).unwrap(), bytes, "snapshot file restored on refusal");
}

#[test]
fn missing_source_keeps_working_then_disable_detaches() {
    let mut h = host();
    let (kit, app, key, card, inst) = kit_and_app(&mut h);
    let lib = kit_summary(&h);
    // A field binding to a library variable would need a Number variable; use a local alias too.
    call(&mut h, "set_variables", json!({"variables": {"mine": {"alias": "brand"}}}));
    h.session.save(&mut h.ed).unwrap();
    h.session.create(&mut h.ed, "Elsewhere").unwrap();
    std::fs::remove_dir_all(&kit).unwrap();
    h.session.open(&mut h.ed, &app).unwrap();

    let st = libraries::status(&h.session.store, &app, &h.ed);
    assert_eq!((st[0].source_version, st[0].update_available), (None, false));
    assert_eq!(fill_rgb(&h, card), (255, 0, 0, Some("brand".into())));
    assert!(h.ed.is_instance_root(inst));
    let again = call(&mut h, "create_instance", json!({"componentKey": key}));
    assert!(h.ed.doc.get(num(&again["id"])).is_some());
    assert!(libraries::update(&h.session.store, &app, &mut h.ed, &lib.library_id).is_err());

    let r = libraries::disable(&app, &mut h.ed, &lib.library_id).unwrap();
    assert!(r.fills >= 2 && r.instances == 2, "{r:?}");
    assert_eq!(fill_rgb(&h, card), (255, 0, 0, None), "value kept, binding gone");
    let n = h.ed.doc.get(inst).unwrap();
    assert!(n.instance.is_none());
    assert_eq!(n.style.opacity, 0.5);
    assert!(h.ed.design.libraries.is_empty());
    assert!(h.ed.design.collections.iter().all(|c| c.library.is_none()));
    assert!(h.ed.library_docs.is_empty());
    // The snapshot stays for undo; the next open sweeps it (see the undo test).
    // The local alias kept the resolved color.
    let v = call(&mut h, "get_variables", json!({}));
    assert!(v.to_string().contains("#ff0000") || v.to_string().contains("#FF0000"), "{v}");
    // Undo restores the library on that page.
    h.ed.undo();
    assert_eq!(fill_rgb(&h, card).3.as_deref(), Some("brand"));
}

#[test]
fn mcp_library_round_trip() {
    let mut h = host();
    let key = make_kit(&mut h, "#ff0000");
    let kit = dir(&h);
    let p = call(&mut h, "publish_library", json!({"description": "Kit"}));
    assert_eq!(p["version"], 1);
    assert!(p["changes"].as_str().unwrap().contains("added"));
    let lid = p["libraryId"].as_str().unwrap().to_string();
    assert!(call(&mut h, "list_libraries", json!({}))["available"].as_array().unwrap().is_empty());

    h.session.create(&mut h.ed, "App").unwrap();
    let l = call(&mut h, "list_libraries", json!({}));
    assert_eq!((l["available"][0]["libraryId"].as_str(), l["available"][0]["enabled"].as_bool()), (Some(lid.as_str()), Some(false)));
    call(&mut h, "enable_library", json!({"libraryId": lid}));
    let l = call(&mut h, "list_libraries", json!({}));
    assert_eq!(l["enabled"][0]["updateAvailable"], false);
    let c = call(&mut h, "get_components", json!({}));
    let lc = c["components"].as_array().unwrap().iter().find(|c| c["source"]["library"] == lid.as_str()).unwrap();
    assert_eq!(lc["componentKey"], key.as_str());
    assert_eq!(lc["source"]["name"], "Kit");
    // swap_instance accepts a library key: swap a local frame's instance? Use a fresh instance pair.
    let inst = call(&mut h, "create_instance", json!({"componentKey": key}));
    let sw = call(&mut h, "swap_instance", json!({"nodeId": inst["id"], "componentKey": key}));
    assert_eq!(sw["swapped"], true);

    // Republish in Kit, update in App.
    h.session.save(&mut h.ed).unwrap();
    h.session.open(&mut h.ed, &kit).unwrap();
    call(&mut h, "set_variables", json!({"variables": {"brand": "#0000ff"}}));
    assert_eq!(call(&mut h, "publish_library", json!({}))["version"], 2);
    let app = dir_of(&h, "App");
    h.session.open(&mut h.ed, &app).unwrap();
    assert_eq!(call(&mut h, "list_libraries", json!({}))["enabled"][0]["updateAvailable"], true);
    let u = call(&mut h, "update_library", json!({"libraryId": lid}));
    assert!(u["changes"].as_str().unwrap().contains("changed"));
    assert_eq!(h.ed.design.libraries[0].version, 2);
    let d = call(&mut h, "disable_library", json!({"libraryId": lid}));
    assert!(d["detachedInstances"].as_u64().unwrap() >= 1);
    assert!(try_call(&mut h, "enable_library", json!({"libraryId": "nope"})).is_err());
    assert!(call(&mut h, "list_libraries", json!({}))["enabled"].as_array().unwrap().is_empty());
}

fn dir_of(h: &exec::SessionHost, name: &str) -> PathBuf {
    h.session.store.list().into_iter().find(|e| e.manifest.name == name).unwrap().dir
}

fn snapshot_file(app: &std::path::Path, lib: &LibrarySummary) -> PathBuf {
    app.join("libraries").join(format!("{}.json", lib.library_id))
}

#[test]
fn undo_disable_restores_library_components_and_open_sweeps_orphans() {
    let mut h = host();
    let (_kit, app, key, _card, _inst) = kit_and_app(&mut h);
    let lib = kit_summary(&h);
    libraries::disable(&app, &mut h.ed, &lib.library_id).unwrap();
    assert!(h.ed.library_docs.is_empty());
    assert!(snapshot_file(&app, &lib).exists(), "kept so undo can reload it");
    h.ed.undo();
    assert!(h.ed.find_library_component(&key).is_some(), "components are back after undo");
    h.ed.redo();
    h.session.save(&mut h.ed).unwrap();
    h.session.open(&mut h.ed, &app).unwrap();
    assert!(!snapshot_file(&app, &lib).exists(), "orphan swept on open");
}

#[test]
fn duplicate_project_is_not_a_published_library() {
    let mut h = host();
    make_kit(&mut h, "#ff0000");
    let kit = dir(&h);
    libraries::publish(&kit, &h.ed, "").unwrap();
    let copy = h.session.store.duplicate(&kit).unwrap();
    assert!(!copy.join(libraries::LIBRARY_FILE).exists());
}

#[test]
fn publish_survives_duplicate_component_keys() {
    let mut h = host();
    make_kit(&mut h, "#ff0000");
    let c = call(&mut h, "get_components", json!({}));
    let id = c["components"][0]["id"].clone();
    call(&mut h, "duplicate_nodes", json!({"nodes": [{"id": id}]}));
    let p = libraries::publish(&dir(&h), &h.ed, "").unwrap();
    assert_eq!(p.components.len(), 2);
    assert_ne!(p.components[0].key, p.components[1].key);
    assert_eq!(p.to_document().root.len(), 2);
    assert_eq!(p.to_document().nodes.len(), p.components.iter().map(|c| c.nodes.len()).sum::<usize>());
}

#[test]
fn library_instance_with_missing_library_is_main_missing_and_detachable() {
    let mut h = host();
    let (_kit, _app, _key, _card, inst) = kit_and_app(&mut h);
    assert!(!h.ed.is_main_missing(inst));
    h.ed.library_docs.clear();
    assert!(h.ed.is_main_missing(inst) && h.ed.is_instance_root(inst));
    assert_eq!(h.ed.missing_mains(h.ed.active_page), vec![inst]);
    h.ed.selection = vec![inst];
    h.ed.detach_instance();
    assert!(h.ed.doc.get(inst).unwrap().instance.is_none());
}
