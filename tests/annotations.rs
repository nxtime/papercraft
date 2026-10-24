//! Annotations follow duplicated frames through the MCP tools.

use papercraft::mcp::{Out, exec};
use papercraft::model::NodeId;
use papercraft::model::annotation::Annotation;
use papercraft::project::Store;
use serde_json::{Value, json};

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    let o = exec::call(h, name, &args).unwrap_or_else(|e| panic!("{name} failed: {e}"));
    match o.into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    }
}

fn num(id: &Value) -> NodeId {
    NodeId(id.as_str().unwrap().rsplit(':').next().unwrap().parse().unwrap())
}

#[test]
fn duplicate_nodes_copies_annotations_same_page_and_across_pages() {
    let mut fixture = Fixture::new();
    let h = &mut fixture.host;
    h.session.create(&mut h.ed, "Ann").unwrap();
    let ab = call(h, "create_artboard", json!({"name": "A",
        "styles": {"width": "100px", "height": "40px"}}));
    let src = num(&ab["id"]);
    h.ed.add_annotation(Annotation::new(Some(src), "rule", "data"));
    let d = call(h, "duplicate_nodes", json!({"nodes": [{"id": ab["id"]}]}));
    let copy = num(&d["duplicates"][0]["newId"]);
    let anns = &h.ed.page_doc(0).annotations;
    assert_eq!(anns.len(), 2);
    assert_eq!(anns.iter().filter(|a| a.target == Some(copy)).count(), 1);
    assert_ne!(anns[0].id, anns[1].id);

    let b = call(h, "create_page", json!({"name": "B"}))["pageId"].clone();
    let d = call(h, "duplicate_nodes", json!({"nodes": [{"id": ab["id"], "toPageId": b}]}));
    let copy = num(&d["duplicates"][0]["newId"]);
    let anns = &h.ed.page_doc(1).annotations;
    assert_eq!(anns.len(), 1);
    assert_eq!(anns[0].target, Some(copy));
}


struct Fixture {
    host: exec::SessionHost,
    path: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("papercraft-annotation-tools-{:016x}", fastrand::u64(..)));
        let store = Store::open(&path).unwrap();
        Self { host: exec::SessionHost::new(store), path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}

fn frame(h: &mut exec::SessionHost, name: &str) -> Value {
    call(h, "create_artboard", json!({"name": name,
        "styles": {"width": "300px", "height": "300px"}}))["id"].clone()
}

#[test]
fn annotation_batch_by_id_and_name_is_one_undo_step() {
    use papercraft::model::annotation::{Author, card_world};
    use papercraft::model::annotation_layout::card_size;
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = frame(h, "Sign in");
    let before = h.ed.doc.clone();
    let added = call(h, "annotate", json!({"notes": [
        {"node": id, "text": "Locks after attempts\nFor 15 minutes"},
        {"node": "sIgN IN", "text": "Keyboard support", "type": "content"}
    ]}));
    assert_eq!(added, json!([[1, id, "rule", "Locks after attempts"], [2, id, "content", "Keyboard support"]]));
    let notes = &h.ed.doc.annotations;
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].body, "For 15 minutes");
    assert_eq!(notes[0].author, Author::Agent);
    let rect = |n: &papercraft::model::annotation::Annotation| {
        let (at, s) = (card_world(&h.ed.doc, n), card_size(n));
        papercraft::geom::Rect::new(at.x, at.y, s.x, s.y)
    };
    assert!(!rect(&notes[1]).intersects(&rect(&notes[0])), "cards overlap");
    h.ed.undo();
    assert_eq!(h.ed.doc, before);
    h.ed.redo();
    assert_eq!(h.ed.doc.annotations.len(), 2);
}

#[test]
fn invalid_batch_is_atomic_and_ambiguity_lists_candidates() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let first = frame(h, "Same");
    let second = frame(h, "Same");
    let before = h.ed.doc.clone();
    let err = exec::call(h, "annotate", &json!({"notes": [
        {"node": first, "text": "valid"}, {"node": "same", "text": "ambiguous"}
    ]})).err().unwrap();
    assert!(err.contains("ambiguous"), "{err}");
    for id in [first.as_str().unwrap(), second.as_str().unwrap()] { assert!(err.contains(id), "{err}"); }
    assert_eq!(h.ed.doc, before);
    let err = exec::call(h, "annotate", &json!({"notes": [
        {"node": first, "text": "valid"}, {"node": second, "text": "bad", "type": "missing"}
    ]})).err().unwrap();
    for key in ["rule", "behaviour", "content", "data", "a11y", "dev"] { assert!(err.contains(key), "{err}"); }
    assert_eq!(h.ed.doc, before);
}

#[test]
fn annotation_compact_full_update_delete_and_refs() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = frame(h, "Sign in");
    call(h, "annotate", json!({"notes": [
        {"node": id, "text": "Label\nFirst body line\nSecond body line"},
        {"node": id, "text": "Other"}
    ]}));
    let rows = call(h, "get_annotations", json!({}));
    assert_eq!(rows[0], json!([1, "Sign in", "rule", "Label\nFirst body line"]));
    let full = call(h, "get_annotations", json!({"full": true}));
    assert_eq!(full[0]["body"], "First body line\nSecond body line");
    assert_eq!(full[0]["author"], "agent");
    assert_eq!(full[0]["nodeId"], id);
    call(h, "update_annotation", json!({"ref": "frame Sign in #1", "text": "Edited\nNew body", "type": "data"}));
    assert_eq!(call(h, "get_annotations", json!({}))[0], json!([1, "Sign in", "data", "Edited\nNew body"]));
    h.ed.undo();
    assert_eq!(h.ed.doc.annotations[0].label, "Label");
    call(h, "update_annotation", json!({"id": full[0]["id"], "text": "é".repeat(140)}));
    let rows = call(h, "get_annotations", json!({}));
    assert_eq!(rows[0][3].as_str().unwrap().chars().count(), 120);
    call(h, "delete_annotations", json!({"refs": ["Sign in #1", "Sign in #2"]}));
    assert!(h.ed.doc.annotations.is_empty());
    h.ed.undo();
    assert_eq!(h.ed.doc.annotations.len(), 2);
    call(h, "delete_annotations", json!({"ids": [full[0]["id"], full[1]["id"]]}));
    assert!(h.ed.doc.annotations.is_empty());
}

#[test]
fn annotation_page_scope_and_subtree_filter() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let original = h.ed.active_page;
    let other = call(h, "create_page", json!({"name": "Other"}))["pageId"].clone();
    let id = call(h, "create_artboard", json!({"pageId": other, "name": "Remote",
        "styles": {"width": "100px", "height": "100px"}}))["id"].clone();
    call(h, "annotate", json!({"pageId": other, "notes": [{"node": "remote", "text": "Remote note"}]}));
    assert_eq!(h.ed.active_page, original);
    assert!(call(h, "get_annotations", json!({})).as_array().unwrap().is_empty());
    assert_eq!(call(h, "get_annotations", json!({"nodeId": id}))[0][3], "Remote note");
}

#[test]
fn annotation_codegen_comments_use_project_category_names() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = frame(h, "Sign in");
    let unrelated = frame(h, "Other");
    h.ed.design.annotation_categories = papercraft::model::annotation::default_categories();
    h.ed.design.annotation_categories[0].name = "Account rule".into();
    call(h, "annotate", json!({"notes": [
        {"node": id, "text": "Locks for 15 minutes after 5 failed attempts.\nSecond line"},
        {"node": unrelated, "text": "Unrelated"}
    ]}));
    let expected = "// 1 Account rule: Locks for 15 minutes after 5 failed attempts.";
    for format in ["inline", "react", "tailwind", "angular", "html", "egui"] {
        let output = call(h, "get_jsx", json!({"nodeId": id, "format": format}));
        let code = output.as_str().unwrap();
        assert!(code.contains(expected), "{format}: {code}");
        assert!(code.contains("// Second line"));
        assert!(!code.contains("Unrelated"));
    }
    let md = call(h, "get_design_spec", json!({"nodeId": id}));
    assert!(md.as_str().unwrap().contains(expected));
    let spec = call(h, "get_design_spec", json!({"nodeId": id, "format": "json"}));
    assert!(spec["referenceCode"].as_str().unwrap().contains(expected));
    assert_eq!(spec["annotations"][0], expected);
}

#[test]
fn annotation_tool_classification_and_description_budget() {
    use papercraft::mcp::tools;
    for (name, edits) in [("annotate", true), ("get_annotations", false),
        ("get_annotation_template", false), ("set_annotation_template", true),
        ("upsert_annotation_template", true), ("delete_annotation_template", true),
        ("update_annotation", true), ("delete_annotations", true)] {
        assert_eq!(tools::edits_doc(name, &json!({})), edits);
        let def = tools::definitions().into_iter().find(|d| d["name"] == name).unwrap();
        assert!(def["description"].as_str().unwrap().split_whitespace().count() < 60);
        assert_eq!(def["annotations"]["readOnlyHint"], !edits);
    }
}

#[test]
fn annotation_comments_do_not_satisfy_code_verification() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = frame(h, "Sign in");
    call(h, "write_html", json!({"targetNodeId": id, "html": "<div>Required visible text</div>"}));
    let base = call(h, "verify_code", json!({"nodeId": id, "target": "react", "code": "<div />"}));
    let commented = call(h, "verify_code", json!({"nodeId": id, "target": "react", "code": "// 1 Content: Required visible text\n<div />"}));
    assert_eq!(base, commented);
}


#[test]
fn annotation_custom_keys_and_failed_edits_preserve_notes() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = frame(h, "Sign in");
    let mut category = papercraft::model::annotation::default_categories().remove(0);
    category.id = "custom-id".into();
    category.key = "policy".into();
    category.name = "Policy".into();
    h.ed.design.annotation_categories = vec![category];
    let mut t = papercraft::model::annotation::builtin_templates().remove(0);
    t.categories[0].key = "policy".into();
    h.ed.design.annotation_templates = vec![t];
    let rows = call(h, "annotate", json!({"notes": [{"node": id, "text": "Policy text", "type": "POLICY"}]}));
    assert_eq!(rows[0][2], "policy");
    assert_eq!(h.ed.doc.annotations[0].category, "custom-id");
    assert_eq!(call(h, "get_annotations", json!({}))[0][2], "policy");
    let before = h.ed.doc.clone();
    let err = exec::call(h, "update_annotation", &json!({"ref": "Sign in #1", "text": "Changed", "type": "rule"})).err().unwrap();
    assert!(err.contains("policy"));
    assert_eq!(h.ed.doc, before);
    assert!(exec::call(h, "delete_annotations", &json!({"refs": ["Sign in #1", "Sign in #99"]})).is_err());
    assert_eq!(h.ed.doc, before);
    let code = call(h, "get_jsx", json!({"nodeId": id}));
    assert!(code.as_str().unwrap().starts_with("// 1 Policy: Policy text"));
}

#[test]
fn builtin_templates_match_spec() {
    use papercraft::model::annotation::builtin_templates;
    let t = builtin_templates();
    let keys = |i: usize| t[i].enabled_keys().join(",");
    assert_eq!(t.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["Product spec", "Dev handoff", "Accessibility review", "Content review", "Minimal"]);
    assert_eq!(keys(0), "rule,behaviour,content,data");
    assert_eq!(keys(1), "behaviour,data,dev,a11y");
    assert_eq!(t[2].categories[0].fields, ["Issue", "WCAG", "Fix"]);
    assert_eq!(t[3].categories[0].fields, ["Tone", "Max length"]);
    assert_eq!(keys(4), "rule");
    assert!(t.iter().flat_map(|t| &t.categories).all(|c| !c.prompt.is_empty()));
}

#[test]
fn page_override_beats_project_default_and_undoes() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    assert_eq!(h.ed.active_template(0).id, "product-spec");
    h.ed.set_default_template(Some("dev-handoff")).unwrap();
    assert_eq!(h.ed.active_template(0).id, "dev-handoff");
    h.ed.set_page_template(0, Some("minimal")).unwrap();
    assert_eq!(h.ed.active_template(0).id, "minimal");
    h.ed.undo();
    assert_eq!(h.ed.active_template(0).id, "dev-handoff");
    h.ed.undo();
    assert_eq!(h.ed.active_template(0).id, "product-spec");
    h.ed.redo();
    h.ed.redo();
    assert_eq!(h.ed.active_template(0).id, "minimal");
    assert!(h.ed.set_page_template(0, Some("nope")).is_err());
}

#[test]
fn old_page_bytes_unchanged_and_override_roundtrips() {
    use papercraft::project::format::{page_from_json_modes, page_to_json_modes, page_to_json};
    use papercraft::variables::{PAGE_TEMPLATE_KEY, PageModes};
    let mut f = Fixture::new();
    let h = &mut f.host;
    let page = h.ed.page_data(0);
    let plain = page_to_json(&page);
    assert!(!plain.contains("annotationTemplate"));
    let (p, modes) = page_from_json_modes(&plain).unwrap();
    assert_eq!(page_to_json_modes(&p, &modes), plain);
    let mut m = PageModes::new();
    m.insert(PAGE_TEMPLATE_KEY.into(), "minimal".into());
    let s = page_to_json_modes(&page, &m);
    assert!(s.contains("\"annotationTemplate\": \"minimal\""));
    assert!(!s.contains("collectionModes"));
    let (p, modes) = page_from_json_modes(&s).unwrap();
    assert_eq!(modes, m);
    assert_eq!(page_to_json_modes(&p, &modes), s);
}

#[test]
fn annotate_rejects_disabled_type_and_keeps_fields() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = frame(h, "Sign in");
    let before = h.ed.doc.clone();
    let err = exec::call(h, "annotate", &json!({"notes": [{"node": id, "text": "x", "type": "a11y"}]}))
        .err().unwrap();
    assert!(err.contains("rule, behaviour, content, data") && err.contains("Product spec"), "{err}");
    assert_eq!(h.ed.doc, before);
    call(h, "set_annotation_template", json!({"template": "accessibility review"}));
    call(h, "annotate", json!({"notes": [{"node": id, "type": "a11y",
        "text": "Low contrast\nIssue: grey on white\nWCAG: 1.4.3\nFix: darken"}]}));
    assert_eq!(h.ed.doc.annotations[0].body, "Issue: grey on white\nWCAG: 1.4.3\nFix: darken");
}

#[test]
fn set_get_template_round_trip_and_scopes() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let t = call(h, "get_annotation_template", json!({}));
    assert_eq!(t["template"], "Product spec");
    assert_eq!(t["types"].as_array().unwrap().len(), 4);
    assert_eq!(t["types"][0][0], "rule");
    call(h, "set_annotation_template", json!({"template": "content-review", "scope": "project"}));
    let t = call(h, "get_annotation_template", json!({}));
    assert_eq!(t["template"], "Content review");
    assert_eq!(t["types"], json!([["content", t["types"][0][1], ["Tone", "Max length"]]]));
    call(h, "set_annotation_template", json!({"template": "Minimal"}));
    assert_eq!(call(h, "get_annotation_template", json!({}))["template"], "Minimal");
    assert!(exec::call(h, "set_annotation_template", &json!({"template": "zzz"})).err().unwrap().contains("Minimal"));
    assert!(exec::call(h, "set_annotation_template", &json!({"template": "minimal", "scope": "x"})).is_err());
}

#[test]
fn template_edits_are_undoable() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let base = h.ed.design.clone();
    let new = h.ed.duplicate_template("minimal").unwrap();
    assert_eq!(h.ed.design.templates().len(), 6);
    let mut t = h.ed.design.template_by_ref(&new).unwrap();
    t.name = "Mine".into();
    t.categories[0].prompt = "custom".into();
    h.ed.upsert_template(t).unwrap();
    assert_eq!(h.ed.design.template_by_ref("mine").unwrap().categories[0].prompt, "custom");
    h.ed.reset_template("product-spec").unwrap();
    assert!(h.ed.reset_template(&new).is_err());
    h.ed.set_default_template(Some(&new)).unwrap();
    h.ed.delete_template(&new).unwrap();
    assert_eq!(h.ed.design.annotation_template, None);
    assert_eq!(h.ed.design.templates().len(), 5);
    h.ed.undo();
    assert_eq!(h.ed.design.annotation_template.as_deref(), Some(new.as_str()));
    assert_eq!(h.ed.design.templates().len(), 6);
    h.ed.undo();
    h.ed.undo();
    h.ed.undo();
    h.ed.undo();
    assert_eq!(h.ed.design, base);
}

#[test]
fn update_annotation_rejects_type_disabled_by_template() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = frame(h, "Sign in");
    call(h, "annotate", json!({"notes": [{"node": id, "text": "x"}]}));
    let before = h.ed.doc.clone();
    let err = exec::call(h, "update_annotation", &json!({"ref": "Sign in #1", "type": "a11y"}))
        .err().unwrap();
    assert!(err.contains("rule, behaviour, content, data") && err.contains("Product spec"), "{err}");
    assert_eq!(h.ed.doc, before);
    call(h, "update_annotation", json!({"ref": "Sign in #1", "type": "data"}));
    assert_eq!(h.ed.doc.annotations[0].category, "data");
}

fn err(h: &mut exec::SessionHost, name: &str, args: Value) -> String {
    exec::call(h, name, &args).err().unwrap_or_else(|| panic!("{name} should fail"))
}

fn review() -> Value {
    json!({"name": "Legal review", "display": "badges", "categories": [
        {"key": "legal", "guidance": "Legal risk.", "fields": ["Owner", "Law"]},
        {"key": "rule"},
        {"key": "data", "enabled": false}]})
}

#[test]
fn upsert_template_creates_category_applies_to_page_and_gates_annotate() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = frame(h, "Sign in");
    let undo = h.ed.history.undo_len();
    let r = call(h, "upsert_annotation_template", json!({"template": review(), "setAs": "page",
        "categories": [{"key": "legal", "name": "Legal", "color": "#aa3355", "icon": "scale"}]}));
    assert_eq!(r["id"], "legal-review");
    assert_eq!(r["types"], json!([["legal", "Legal risk.", ["Owner", "Law"]], ["rule", "", []]]));
    assert_eq!(h.ed.design.category_by_key("legal").unwrap().color.to_hex(), "#aa3355");
    assert_eq!(h.ed.history.undo_len(), undo + 2, "edit + page override");
    assert_eq!(call(h, "get_annotation_template", json!({}))["template"], "Legal review");
    call(h, "annotate", json!({"notes": [{"node": id, "type": "legal", "text": "GDPR\nOwner: me"}]}));
    assert_eq!(h.ed.doc.annotations[0].category, "legal");
    assert!(err(h, "annotate", json!({"notes": [{"node": id, "type": "data", "text": "x"}]})).contains("legal, rule"));
    // update by name: same id, category restyled
    let r = call(h, "upsert_annotation_template", json!({"template": {"name": "legal REVIEW", "categories": [{"key": "legal"}]},
        "categories": [{"key": "legal", "color": "#112233"}]}));
    assert_eq!(r["id"], "legal-review");
    assert_eq!(h.ed.design.templates().iter().filter(|t| t.id == "legal-review").count(), 1);
    assert_eq!(h.ed.design.category_by_key("legal").unwrap().name, "Legal");
    assert_eq!(h.ed.design.category_by_key("legal").unwrap().color.to_hex(), "#112233");
    // second template with the same slug gets a unique id; project default
    let r = call(h, "upsert_annotation_template", json!({"setAs": "project",
        "template": {"id": "x", "name": "Legal Review!", "categories": [{"key": "rule"}]}}));
    assert_eq!(r["id"], "x");
    assert_eq!(h.ed.design.annotation_template.as_deref(), Some("x"));
}

#[test]
fn upsert_template_is_one_undo_step_and_errors_change_nothing() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let base = h.ed.design.clone();
    let undo = h.ed.history.undo_len();
    call(h, "upsert_annotation_template", json!({"template": review(), "setAs": "project",
        "categories": [{"key": "legal", "name": "Legal", "color": "#aa3355", "icon": "scale"}]}));
    assert_eq!(h.ed.history.undo_len(), undo + 1);
    h.ed.undo();
    assert_eq!(h.ed.design, base);
    let cat = |c: Value| json!({"template": {"name": "T", "categories": [{"key": "rule"}]}, "categories": [c]});
    let e = err(h, "upsert_annotation_template", cat(json!({"key": "z", "name": "Z", "color": "#aa3355", "icon": "nope"})));
    assert!(e.contains("unknown icon `nope`") && e.contains("scale") && e.contains("zap"), "{e}");
    assert!(err(h, "upsert_annotation_template", cat(json!({"key": "z", "name": "Z", "color": "red", "icon": "zap"}))).contains("bad color"));
    assert!(err(h, "upsert_annotation_template", cat(json!({"key": "z", "name": "Z", "icon": "zap"}))).contains("color"));
    let e = err(h, "upsert_annotation_template", json!({"template": {"name": "T", "categories": [{"key": "ghost"}]}}));
    assert!(e.contains("ghost") && e.contains("rule, behaviour"), "{e}");
    assert!(err(h, "upsert_annotation_template", json!({"template": {"name": "T", "categories": [{"key": "rule"}]}, "setAs": "x"})).contains("setAs"));
    assert_eq!(h.ed.design, base);
    assert_eq!(h.ed.history.undo_len(), undo);
}

#[test]
fn delete_template_rules() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    call(h, "set_annotation_template", json!({"template": "Minimal", "scope": "project"}));
    assert_eq!(call(h, "delete_annotation_template", json!({"template": "minimal"}))["deleted"], "Minimal");
    assert_eq!(h.ed.design.annotation_template, None);
    assert!(err(h, "delete_annotation_template", json!({"template": "zzz"})).contains("Product spec"));
    for t in ["dev-handoff", "accessibility-review", "content-review"] {
        call(h, "delete_annotation_template", json!({"template": t}));
    }
    call(h, "upsert_annotation_template", json!({"template": {"name": "Mine", "categories": [{"key": "rule"}]}}));
    call(h, "delete_annotation_template", json!({"template": "product-spec"}));
    assert!(err(h, "delete_annotation_template", json!({"template": "Mine"})).contains("last"));
    assert_eq!(h.ed.design.templates().len(), 1);
}
