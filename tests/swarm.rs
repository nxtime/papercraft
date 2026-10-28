//! Several agents on one project through the entry the app uses (`Swarm::call` over the run
//! tracker): conflicts are reported or refused, claims fence layers off, and agents working
//! apart never hear of each other.

use papercraft::mcp::swarm::Swarm;
use papercraft::mcp::{Client, Out, exec};
use papercraft::project::Store;
use papercraft::runs::Tracker;
use serde_json::{Value, json};

struct Sim {
    host: exec::SessionHost,
    tracker: Tracker,
    swarm: Swarm,
    now: u64,
}

fn sim() -> Sim {
    let store = Store::open(std::env::temp_dir().join(format!("papercraft-swarm-{:016x}", fastrand::u64(..)))).unwrap();
    let mut host = exec::SessionHost::new(store);
    host.session.create(&mut host.ed, "Swarm test").unwrap();
    Sim { host, tracker: Tracker::default(), swarm: Swarm::default(), now: 1_000 }
}

fn agent(name: &str) -> Client {
    Client::agent(name, None)
}

impl Sim {
    /// The tool's own JSON and the trailing swarm block, if any.
    fn call(&mut self, who: &Client, name: &str, args: Value) -> Result<(Value, Value), String> {
        self.now += 1_000;
        let outs = self.swarm.call(&mut self.tracker, &mut self.host, who, name, &args, self.now)?;
        let texts: Vec<Value> = outs
            .iter()
            .filter_map(|o| match o {
                Out::Text(t) => serde_json::from_str(t).ok(),
                Out::Image { .. } => None,
            })
            .collect();
        let swarm = texts.iter().find_map(|v| v.get("swarm").cloned()).unwrap_or(Value::Null);
        Ok((texts.into_iter().next().unwrap_or(Value::Null), swarm))
    }

    fn ok(&mut self, who: &Client, name: &str, args: Value) -> (Value, Value) {
        self.call(who, name, args).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn board(&mut self, who: &Client, name: &str, x: u32) -> Value {
        let ab = self.ok(who, "create_artboard", json!({"name": name, "styles": {"width": "390px", "height": "844px", "left": format!("{x}px"), "top": "0px"}})).0;
        ab["id"].clone()
    }
}

#[test]
fn agents_on_separate_artboards_never_hear_of_each_other() {
    let mut s = sim();
    let (a, b) = (agent("A"), agent("B"));
    let x = s.board(&a, "X", 0);
    let y = s.board(&b, "Y", 600);
    for _ in 0..3 {
        assert!(s.ok(&a, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.9}}]})).1.is_null());
        assert!(s.ok(&b, "update_styles", json!({"updates": [{"nodeIds": [y], "styles": {"opacity": 0.8}}]})).1.is_null());
    }
}

#[test]
fn a_write_over_someone_elses_newer_change_is_reported_or_refused() {
    let mut s = sim();
    let (a, b) = (agent("A"), agent("B"));
    let x = s.board(&a, "X", 0);
    let rev = s.ok(&a, "get_basic_info", json!({})).0["rev"].as_u64().unwrap();
    let after_b = s.ok(&b, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.5}}]})).0["rev"].clone();
    // Strict: A says what it last saw; nothing is applied.
    let err = s.call(&a, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.7}}], "ifRev": rev})).unwrap_err();
    let err: Value = serde_json::from_str(&err).unwrap();
    assert_eq!(err["error"], "conflict");
    assert_eq!(err["conflicts"][0]["by"], "B");
    assert_eq!(s.ok(&a, "get_basic_info", json!({})).0["rev"], after_b, "the refused write changed nothing");
    // Warn (no ifRev): applied, and the conflict comes back with it.
    let (_, swarm) = s.ok(&a, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.7}}]}));
    assert_eq!(swarm["conflicts"][0]["relation"], "self", "{swarm}");
    // Having written it, A is up to date: the next write is quiet.
    assert!(s.ok(&a, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.6}}]})).1.is_null());
}

#[test]
fn appending_to_the_same_frame_does_not_conflict() {
    let mut s = sim();
    let (a, b) = (agent("A"), agent("B"));
    let x = s.board(&a, "X", 0);
    s.ok(&b, "get_children", json!({"nodeId": x}));
    s.ok(&a, "write_html", json!({"targetNodeId": x, "html": "<div>from A</div>"}));
    let (_, swarm) = s.ok(&b, "write_html", json!({"targetNodeId": x, "html": "<div>from B</div>"}));
    assert!(swarm.is_null(), "{swarm}");
}

#[test]
fn a_claim_fences_off_a_subtree_until_it_is_released() {
    let mut s = sim();
    let (a, b) = (agent("A"), agent("B"));
    let x = s.board(&a, "X", 0);
    let child = s.ok(&a, "write_html", json!({"targetNodeId": x, "html": "<div>Header</div>"})).0["createdNodeIds"][0].clone();
    let claimed = s.ok(&a, "claim_nodes", json!({"ids": [x], "note": "rebuilding X"})).0;
    assert!(claimed["claimId"].is_string(), "{claimed}");
    let err = s.call(&b, "update_styles", json!({"updates": [{"nodeIds": [child], "styles": {"opacity": 0.5}}]})).unwrap_err();
    assert!(err.contains("\"claimed\"") && err.contains("rebuilding X"), "{err}");
    let denied = s.ok(&b, "claim_nodes", json!({"ids": [child]})).0;
    assert_eq!(denied["denied"][0]["relation"], "ancestor", "{denied}");
    assert_eq!(s.ok(&b, "list_claims", json!({})).0["claims"][0]["holder"], "A");
    s.ok(&a, "update_styles", json!({"updates": [{"nodeIds": [child], "styles": {"opacity": 0.9}}]}));
    s.ok(&a, "finish_working_on_nodes", json!({}));
    assert!(s.call(&b, "update_styles", json!({"updates": [{"nodeIds": [child], "styles": {"opacity": 0.5}}]})).is_ok(), "released");
}

#[test]
fn a_claim_expires_when_its_holder_goes_quiet() {
    let mut s = sim();
    let (a, b) = (agent("A"), agent("B"));
    let x = s.board(&a, "X", 0);
    s.ok(&a, "claim_nodes", json!({"ids": [x], "ttlSeconds": 10}));
    s.now += 11_000;
    assert!(s.call(&b, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.5}}]})).is_ok());
}

#[test]
fn a_build_helper_gets_its_scope_and_its_caller_may_still_step_in() {
    let mut s = sim();
    let (lead, other) = (agent("Lead"), agent("Other"));
    let x = s.board(&lead, "X", 0);
    let ed = &mut s.host.ed;
    assert!(s.swarm.claim_for_helper(&mut s.tracker, ed, &lead, "build", &json!(null), s.now).is_err(), "a build needs a scope");
    let helper = s.swarm.claim_for_helper(&mut s.tracker, &mut s.host.ed, &lead, "build", &json!({"ids": [x]}), s.now).unwrap();
    assert_eq!(helper.parent.as_deref(), Some("agent:Lead"));
    assert!(s.call(&other, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.5}}]})).is_err(), "others keep out");
    let (_, swarm) = s.ok(&lead, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.5}}]}));
    assert!(swarm["claimWarnings"].is_array(), "the caller is told, not stopped: {swarm}");
    s.ok(&helper, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.4}}]}));
    s.swarm.release_client(&helper);
    assert!(s.call(&other, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.5}}]})).is_ok());
}

#[test]
fn an_agent_undoes_its_own_step_and_leaves_the_others() {
    let mut s = sim();
    let (a, b) = (agent("A"), agent("B"));
    let x = s.board(&a, "X", 0);
    let y = s.board(&b, "Y", 600);
    s.ok(&a, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.5}}]}));
    s.ok(&b, "update_styles", json!({"updates": [{"nodeIds": [y], "styles": {"opacity": 0.4}}]}));
    let undone = s.ok(&a, "undo", json!({})).0;
    assert_eq!(undone["done"], true, "{undone}");
    let opacity = |s: &mut Sim, who: &Client, id: &Value| s.ok(who, "get_computed_styles", json!({"nodeIds": [id]})).0.to_string();
    assert!(!opacity(&mut s, &a, &x).contains("0.5"), "A's step is gone");
    assert!(opacity(&mut s, &b, &y).contains("0.4"), "B's later step stays");
    // B never undid anything, so B has nothing to redo; A gets its step back.
    assert_eq!(s.ok(&b, "redo", json!({})).0["done"], false);
    assert_eq!(s.ok(&a, "redo", json!({})).0["done"], true);
    assert!(opacity(&mut s, &a, &x).contains("0.5"));
}

#[test]
fn an_agent_cannot_undo_under_a_later_step_on_the_same_layer() {
    let mut s = sim();
    let (a, b) = (agent("A"), agent("B"));
    let x = s.board(&a, "X", 0);
    s.ok(&a, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.5}}]}));
    s.ok(&b, "update_styles", json!({"updates": [{"nodeIds": [x], "styles": {"opacity": 0.4}}]}));
    let refused = s.ok(&a, "undo", json!({})).0;
    assert_eq!(refused["reason"], "undo_blocked", "{refused}");
    assert!(refused["blockedBy"][0].is_string());
    // The page's undo still takes back the newest step, whoever made it.
    assert_eq!(s.ok(&a, "undo", json!({"scope": "page"})).0["done"], true);
}
