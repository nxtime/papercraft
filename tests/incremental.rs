//! Incremental lint on a headless session: the change log behind `get_changes`, the `rev`
//! write tools report, and `lint_design {since, changedOnly}`.

use papercraft::editor::changes::{ChangeLog, Who};
use papercraft::mcp::{Out, exec};
use papercraft::project::Store;
use serde_json::{Value, json};
use std::sync::Arc;

fn host() -> exec::SessionHost {
    let s = Store::open(std::env::temp_dir().join(format!("papercraft-inc-{:016x}", fastrand::u64(..)))).unwrap();
    let mut h = exec::SessionHost::new(s);
    h.session.create(&mut h.ed, "Incremental").unwrap();
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

fn act_as(h: &mut exec::SessionHost, name: &str, session: &str) {
    let label = name.to_string();
    h.ed.changes.client = Some(Arc::new(Who { key: format!("{name}#{session}"), label }));
}

struct Design {
    /// Layer ids by role.
    card_a: String,
    card_b: String,
    far: String,
}

/// Two artboards: "Near" with two cards (the first bound to the `accent` variable), "Far" with
/// one card holding 10px text, which lint flags.
fn design(h: &mut exec::SessionHost) -> Design {
    call(h, "set_variables", json!({"variables": {"accent": "#2563eb"}}));
    let near = call(h, "create_artboard", json!({"name": "Near", "styles": {"width": "390px", "height": "400px", "padding": "16px", "gap": 16}}));
    let r = call(
        h,
        "write_html",
        json!({"targetNodeId": near["id"], "mode": "insert-children",
               "html": "<div layer-name=\"Card A\" style=\"padding:16px;background:var(--accent)\"><p style=\"font-size:16px\">A</p></div><div layer-name=\"Card B\" style=\"padding:16px;background:#eeeeee\"><p style=\"font-size:16px\">B</p></div>"}),
    );
    let far = call(h, "create_artboard", json!({"name": "Far", "styles": {"width": "390px", "height": "400px", "padding": "16px"}}));
    let f = call(
        h,
        "write_html",
        json!({"targetNodeId": far["id"], "mode": "insert-children",
               "html": "<div layer-name=\"Card F\" style=\"padding:16px;background:#eeeeee\"><p style=\"font-size:10px\">Tiny</p></div>"}),
    );
    Design {
        card_a: r["createdNodeIds"][0].as_str().unwrap().into(),
        card_b: r["createdNodeIds"][1].as_str().unwrap().into(),
        far: f["createdNodeIds"][0].as_str().unwrap().into(),
    }
}

fn changed_ids(r: &Value) -> Vec<String> {
    r["changes"].as_array().unwrap().iter().flat_map(|g| g["ids"].as_array().unwrap().iter().map(|i| i.as_str().unwrap().to_string())).collect()
}

fn issue_nodes(r: &Value) -> Vec<String> {
    r["issues"].as_array().unwrap().iter().map(|i| i["node"].as_str().unwrap().to_string()).collect()
}

fn restyle(h: &mut exec::SessionHost, node: &str, styles: Value) -> Value {
    call(h, "update_styles", json!({"updates": [{"nodeIds": [node], "styles": styles}]}))
}

#[test]
fn write_tools_report_the_revision_and_reads_leave_it_alone() {
    let mut h = host();
    let d = design(&mut h);
    let r = restyle(&mut h, &d.card_b, json!({"opacity": 0.9}));
    let rev = r["rev"].as_u64().expect("write result carries rev");
    assert!(rev > 0);
    for (name, args) in [
        ("get_basic_info", json!({})),
        ("get_node_info", json!({"nodeId": d.card_b})),
        ("get_children", json!({"nodeId": d.card_b})),
        ("get_variables", json!({})),
        ("lint_design", json!({})),
        ("get_changes", json!({"since": 0})),
    ] {
        call(&mut h, name, args);
    }
    let info = call(&mut h, "get_basic_info", json!({}));
    assert_eq!(info["rev"], rev, "reads do not bump the revision");
    assert_eq!(call(&mut h, "get_changes", json!({"since": rev}))["changes"], json!([]));
    let again = restyle(&mut h, &d.card_b, json!({"opacity": 0.8}));
    assert!(again["rev"].as_u64().unwrap() > rev);
}

#[test]
fn get_changes_lists_the_layers_a_call_touched() {
    let mut h = host();
    let d = design(&mut h);
    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    restyle(&mut h, &d.card_b, json!({"opacity": 0.5}));
    let r = call(&mut h, "get_changes", json!({"since": rev}));
    assert_eq!(r["truncated"], false);
    assert_eq!(r["since"], rev);
    assert_eq!(changed_ids(&r), vec![d.card_b.clone()], "{r}");
    assert!(r["rev"].as_u64().unwrap() > rev);
    let page = h.ed.pages[0].id.clone();
    assert_eq!(call(&mut h, "get_changes", json!({"since": rev, "page": page}))["changes"], r["changes"]);
    let other = call(&mut h, "create_page", json!({"name": "Other"}));
    let other_id = other["id"].as_str().or(other["pageId"].as_str()).unwrap_or_default().to_string();
    if !other_id.is_empty() {
        assert_eq!(call(&mut h, "get_changes", json!({"since": rev, "page": other_id}))["changes"], json!([]));
    }
}

#[test]
fn undo_and_redo_count_as_changes() {
    let mut h = host();
    let d = design(&mut h);
    restyle(&mut h, &d.card_b, json!({"opacity": 0.5}));
    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    let r = call(&mut h, "undo", json!({}));
    assert!(r["rev"].as_u64().unwrap() > rev);
    assert!(changed_ids(&call(&mut h, "get_changes", json!({"since": rev}))).contains(&d.card_b));
    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    call(&mut h, "redo", json!({}));
    assert!(changed_ids(&call(&mut h, "get_changes", json!({"since": rev}))).contains(&d.card_b));
}

#[test]
fn a_variable_edit_touches_every_layer_bound_to_it() {
    let mut h = host();
    let d = design(&mut h);
    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    call(&mut h, "set_variables", json!({"variables": {"accent": "#ef4444"}}));
    let ids = changed_ids(&call(&mut h, "get_changes", json!({"since": rev})));
    assert!(ids.contains(&d.card_a), "the bound card changed: {ids:?}");
    assert!(!ids.contains(&d.card_b) && !ids.contains(&d.far), "unbound layers did not: {ids:?}");
}

#[test]
fn the_log_says_truncated_when_it_no_longer_reaches_back() {
    let mut h = host();
    let d = design(&mut h);
    h.ed.changes = ChangeLog::with_capacity(4);
    for n in 0..6 {
        restyle(&mut h, &d.card_b, json!({"opacity": 0.5 + n as f64 / 20.0}));
    }
    let r = call(&mut h, "get_changes", json!({"since": 0}));
    assert_eq!(r["truncated"], true);
    assert_eq!(r["changes"], json!([]));
    // A lint that cannot rely on the log does a full pass.
    let l = call(&mut h, "lint_design", json!({"since": 0}));
    assert_eq!(l["covered"]["partial"], false, "{l}");
    assert_eq!(l["covered"]["checked"], l["covered"]["total"]);
    assert!(l["covered"]["fullPassBecause"].is_string());
    let fresh = call(&mut h, "get_changes", json!({"since": r["rev"].as_u64().unwrap() - 1}));
    assert_eq!(fresh["truncated"], false);
}

#[test]
fn lint_since_checks_only_what_a_change_can_affect() {
    let mut h = host();
    let d = design(&mut h);
    let full = call(&mut h, "lint_design", json!({}));
    assert_eq!(full["covered"]["partial"], false);
    assert_eq!(full["covered"]["since"], Value::Null);
    let total = full["covered"]["total"].as_u64().unwrap();
    let has_tiny = |r: &Value| r["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "text-size");
    let full_count = full["issues"].as_array().unwrap().len();
    assert!(has_tiny(&full), "the 10px text is flagged: {full}");

    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    restyle(&mut h, &d.card_b, json!({"opacity": 0.9}));
    let r = call(&mut h, "lint_design", json!({"since": rev}));
    let cov = &r["covered"];
    assert_eq!(cov["partial"], true);
    assert_eq!(cov["since"], rev);
    assert_eq!(cov["total"], total);
    assert!(cov["checked"].as_u64().unwrap() < total, "{cov}");
    assert!(issue_nodes(&r).iter().all(|n| *n != d.far), "the far card was not checked");
    assert!(!has_tiny(&r), "the far card's finding is not repeated: {r}");
    assert!(r["issues"].as_array().unwrap().len() < full_count);

    // Touching the far card brings its finding back.
    restyle(&mut h, &d.far, json!({"opacity": 0.9}));
    let r = call(&mut h, "lint_design", json!({"since": rev}));
    assert!(has_tiny(&r), "{r}");
}

#[test]
fn a_variable_edit_pulls_its_bound_layers_into_the_lint() {
    let mut h = host();
    let d = design(&mut h);
    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    call(&mut h, "set_variables", json!({"variables": {"accent": "#fde68a"}}));
    let r = call(&mut h, "lint_design", json!({"since": rev}));
    assert_eq!(r["covered"]["partial"], true);
    let near_total = h.ed.doc.nodes.len();
    assert!(r["covered"]["checked"].as_u64().unwrap() as usize <= near_total);
    let ids = changed_ids(&call(&mut h, "get_changes", json!({"since": rev})));
    assert!(ids.contains(&d.card_a));
}

#[test]
fn changed_only_follows_this_clients_last_lint() {
    let mut h = host();
    let d = design(&mut h);
    // Unknown client: nothing to compare with, so a full pass that says why.
    let anon = call(&mut h, "lint_design", json!({"changedOnly": true}));
    assert_eq!(anon["covered"]["partial"], false);
    assert!(anon["covered"]["fullPassBecause"].is_string(), "{anon}");

    act_as(&mut h, "rmcp", "s1");
    let first = call(&mut h, "lint_design", json!({"changedOnly": true}));
    assert_eq!(first["covered"]["partial"], false, "this client has not linted yet");
    restyle(&mut h, &d.card_b, json!({"opacity": 0.9}));
    let second = call(&mut h, "lint_design", json!({"changedOnly": true}));
    assert_eq!(second["covered"]["partial"], true, "{second}");
    assert!(second["covered"]["checked"].as_u64().unwrap() < second["covered"]["total"].as_u64().unwrap());
    // Nothing changed since the last lint: nothing to check.
    let third = call(&mut h, "lint_design", json!({"changedOnly": true}));
    assert_eq!(third["covered"]["checked"], 0, "{third}");

    // Another session of the same client name has its own memory.
    act_as(&mut h, "rmcp", "s2");
    let other = call(&mut h, "lint_design", json!({"changedOnly": true}));
    assert_eq!(other["covered"]["partial"], false);
}

#[test]
fn a_subtree_lint_does_not_count_as_a_whole_page_lint() {
    let mut h = host();
    let d = design(&mut h);
    act_as(&mut h, "rmcp", "s1");
    call(&mut h, "lint_design", json!({"nodeId": d.card_b}));
    let r = call(&mut h, "lint_design", json!({"changedOnly": true}));
    assert_eq!(r["covered"]["partial"], false, "{r}");
}

#[test]
fn fix_works_on_the_scoped_set_only() {
    let mut h = host();
    let d = design(&mut h);
    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    // Both cards get an off-grid gap; only the near one is in scope.
    restyle(&mut h, &d.card_b, json!({"gap": 13, "display": "flex", "flexDirection": "column"}));
    restyle(&mut h, &d.far, json!({"gap": 13, "display": "flex", "flexDirection": "column"}));
    let far_rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    let r = call(&mut h, "lint_design", json!({"since": far_rev - 1, "fix": true}));
    assert_eq!(r["covered"]["partial"], true);
    let gap = |h: &exec::SessionHost, key: &str| {
        let id = papercraft::model::NodeId(key.rsplit(':').next().unwrap().parse().unwrap());
        h.ed.doc.get(id).and_then(|n| n.layout).map(|l| l.gap)
    };
    let (near, far) = (gap(&h, &d.card_b), gap(&h, &d.far));
    assert_eq!(far, Some(12.0), "the layer touched last was in scope: {r}");
    assert_eq!(near, Some(13.0), "the earlier edit was out of scope: {r}");
    assert!(r["rev"].as_u64().unwrap() > far_rev, "fixing is a change");
    let _ = rev;
}

#[test]
fn changes_are_attributed_to_the_client_that_made_them() {
    let mut h = host();
    let d = design(&mut h);
    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    act_as(&mut h, "Claude Code", "s1");
    restyle(&mut h, &d.card_a, json!({"opacity": 0.9}));
    act_as(&mut h, "Claude Code \u{b7} 2", "s2");
    restyle(&mut h, &d.card_b, json!({"opacity": 0.9}));
    let r = call(&mut h, "get_changes", json!({"since": rev}));
    let groups = r["changes"].as_array().unwrap();
    assert_eq!(groups.len(), 2, "{r}");
    assert_eq!(groups[0]["by"], "Claude Code");
    assert_eq!(groups[0]["ids"], json!([d.card_a]));
    assert_eq!(groups[1]["by"], "Claude Code \u{b7} 2");
    assert_eq!(groups[1]["ids"], json!([d.card_b]));
    assert!(groups[1]["rev"].as_u64() > groups[0]["rev"].as_u64());

    // Session 2 asks what others changed, then session 1 does.
    let others = call(&mut h, "get_changes", json!({"since": rev, "exclude": "self"}));
    assert_eq!(changed_ids(&others), vec![d.card_a.clone()]);
    act_as(&mut h, "Claude Code", "s1");
    let others = call(&mut h, "get_changes", json!({"since": rev, "exclude": "self"}));
    assert_eq!(changed_ids(&others), vec![d.card_b.clone()]);
    let by = call(&mut h, "get_changes", json!({"since": rev, "by": "Claude Code"}));
    assert_eq!(changed_ids(&by), vec![d.card_a.clone()]);
}

#[test]
fn exclude_self_needs_a_known_client() {
    let mut h = host();
    design(&mut h);
    let e = exec::call(&mut h, "get_changes", &json!({"since": 0, "exclude": "self"})).unwrap_err();
    assert!(e.contains("exclude"), "{e}");
    let e = exec::call(&mut h, "get_changes", &json!({})).unwrap_err();
    assert!(e.contains("since"), "{e}");
}

#[test]
fn opening_another_project_resets_the_log() {
    let mut h = host();
    design(&mut h);
    let rev = call(&mut h, "get_basic_info", json!({}))["rev"].as_u64().unwrap();
    call(&mut h, "create_file", json!({"name": "Second"}));
    let r = call(&mut h, "get_changes", json!({"since": rev}));
    assert_eq!(r["truncated"], true, "{r}");
}
