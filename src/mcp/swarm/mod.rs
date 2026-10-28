//! Several agents on one project (`specs/swarm-safety/spec.md`). Every call from the app's MCP
//! server and chat goes through [`Swarm::call`]: a write is checked against other agents'
//! claims (blocked) and against the layers others changed since this agent last looked at that
//! artboard (reported, or refused when the caller passed `ifRev`), then runs through the run
//! tracker, and what the check found rides back in a trailing `{"swarm": ...}` block.
//!
//! Nothing here waits: a check answers at once, so agents cannot deadlock. The person at the
//! canvas is never checked; their edits reach agents as conflicts by `You`.

pub mod claims;

use super::exec::{self, Host};
use super::{Client, Out, ToolResult};
use crate::editor::Editor;
use crate::editor::changes::Filter;
use crate::model::NodeId;
use crate::runs::Tracker;
use claims::{Claims, Denied, Holder, Layer, Request};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

/// Conflicts listed per call; the rest are counted.
const MAX_CONFLICTS: usize = 20;
/// Parent links followed when deciding whether a claim belongs to an agent's own helper.
const MAX_DELEGATION_DEPTH: usize = 8;
/// A delegated helper's claim lasts as long as the helper may run.
pub const HELPER_TTL_SECS: u64 = 900;

/// How far a write reaches below the layers it names.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Reach {
    /// The layer's own fields.
    Node,
    /// Adds layers under it: two agents appending to one frame do not conflict.
    Children,
    /// Rebuilds, moves or deletes it with everything under it.
    Subtree,
}

fn reach(name: &str, args: &Value) -> Reach {
    match name {
        "write_html" if args["mode"] == "replace" => Reach::Subtree,
        "write_html" | "create_shape" | "duplicate_nodes" | "create_instance" | "create_artboard" | "annotate" => Reach::Children,
        "delete_nodes" | "move_nodes" | "detach_instance" | "swap_instance" => Reach::Subtree,
        _ => Reach::Node,
    }
}

/// The shared resource a tool changes, which a claim can hold for one agent.
fn resource_of(name: &str) -> Option<&'static str> {
    match name {
        "set_variables" | "delete_variables" | "create_collection" | "set_variable_mode" | "set_tokens" | "import_design_system"
        | "enable_library" | "update_library" | "disable_library" => Some("variables"),
        "create_component" | "import_components" | "merge_variants" | "set_component_props" | "apply_ui_kit" => Some("components"),
        "create_page" | "rename_pages" | "delete_pages" => Some("pages"),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub struct Swarm {
    pub claims: Claims,
    /// Revision each client last read or wrote each artboard at: what its picture of it is from.
    seen: HashMap<(String, String, NodeId), u64>,
    /// Revision of each client's first call, for artboards it never looked at.
    first: HashMap<String, u64>,
    /// Session of the agent that delegated each session.
    parents: HashMap<String, String>,
    project: Option<PathBuf>,
}

/// The page id and the layer of each (page index, node) pair.
fn layers(ed: &Editor, named: &[(usize, NodeId)]) -> Vec<Layer> {
    named.iter().map(|(p, id)| (ed.pages[*p].id.clone(), *id)).collect()
}

fn ancestors_in(ed: &Editor) -> impl Fn(&Layer) -> Vec<NodeId> + '_ {
    |l: &Layer| ed.pages.iter().position(|p| p.id == l.0).map_or_else(Vec::new, |i| ed.page_doc(i).ancestors(l.1))
}

fn holder(tracker: &mut Tracker, client: &Client) -> Holder {
    Holder { key: client.key(), label: tracker.label(client), session: client.session.clone() }
}

fn denied_json(claims: &Claims, denied: &[Denied], now: u64) -> Vec<Value> {
    denied
        .iter()
        .map(|d| {
            let c = claims.live.iter().find(|c| c.id == d.claim);
            json!({
                "id": d.what, "relation": d.relation, "claimId": d.claim,
                "heldBy": c.map(|c| c.holder.label.clone()),
                "note": c.map(|c| c.note.clone()),
                "expiresInSeconds": c.map(|c| c.expires_in_secs(now)),
            })
        })
        .collect()
}

impl Swarm {
    /// Another project means other layers: claims and what agents saw no longer apply.
    fn sync_project(&mut self, dir: Option<PathBuf>) {
        if dir != self.project {
            *self = Swarm { project: dir, parents: std::mem::take(&mut self.parents), ..Swarm::default() };
        }
    }

    /// `session` was delegated (directly or through helpers) by `ancestor`.
    fn delegated_by(&self, session: &str, ancestor: &str) -> bool {
        let mut cur = session;
        for _ in 0..MAX_DELEGATION_DEPTH {
            match self.parents.get(cur) {
                Some(p) if p == ancestor => return true,
                Some(p) => cur = p,
                None => return false,
            }
        }
        false
    }

    /// Run one call for `client`: the claim tools here, everything else checked, then through
    /// the run tracker.
    pub fn call(&mut self, tracker: &mut Tracker, host: &mut dyn Host, client: &Client, name: &str, args: &Value, now: u64) -> ToolResult {
        self.sync_project(host.project().map(|p| p.dir.clone()));
        if let Some(p) = &client.parent {
            self.parents.insert(client.session.clone(), p.clone());
        }
        self.claims.expire(now);
        self.claims.renew(&client.key(), now);
        let key = client.key();
        let start = host.editor().changes.rev();
        self.first.entry(key.clone()).or_insert(start);
        match name {
            "claim_nodes" => return self.claim_tool(tracker, host, client, args, now),
            "release_claims" => return self.release_tool(host, client, args),
            "list_claims" => return text(self.list(host.editor(), now)),
            _ => {}
        }
        let ed = host.editor();
        ed.finish_loading();
        let named = exec::named_layers(ed, args);
        let mut notes = serde_json::Map::new();
        if crate::mcp::tools::edits_doc(name, args) {
            let wanted = layers(ed, &named);
            let resources: Vec<String> = resource_of(name).map(String::from).into_iter().collect();
            let found = self.claims.overlaps(&wanted, &resources, &|h| h.key == key, &ancestors_in(ed));
            // An agent may work inside its own helpers' claims; it is told, not stopped.
            let (warn, block): (Vec<Denied>, Vec<Denied>) = found.into_iter().partition(|d| {
                self.claims.live.iter().find(|c| c.id == d.claim).is_some_and(|c| self.delegated_by(&c.holder.session, &client.session))
            });
            if !block.is_empty() {
                return Err(json!({
                    "error": "claimed", "applied": false,
                    "claims": denied_json(&self.claims, &block, now),
                    "hint": "Another agent is working there. Work elsewhere, or retry after expiresInSeconds; never wait in a loop.",
                })
                .to_string());
            }
            if !warn.is_empty() {
                notes.insert("claimWarnings".into(), json!(denied_json(&self.claims, &warn, now)));
            }
            let if_rev = args["ifRev"].as_u64();
            let reference = if_rev.unwrap_or_else(|| self.reference(&key, ed, &named));
            let strict = if_rev.is_some() && args["onConflict"] != "warn"
                || std::env::var("PAPERCRAFT_CONFLICTS").is_ok_and(|v| v == "strict") && args["onConflict"] != "warn";
            let conflicts = conflicts(ed, &key, reference, &named, reach(name, args));
            if strict && !conflicts.is_empty() {
                return Err(json!({
                    "error": "conflict", "applied": false, "rev": ed.changes.rev(), "refRev": reference,
                    "conflicts": conflicts,
                    "hint": format!("Re-read the layers above (get_changes {{since: {reference}, exclude: \"self\"}}), then retry with ifRev {}.", ed.changes.rev()),
                })
                .to_string());
            }
            if !conflicts.is_empty() {
                notes.insert("refRev".into(), json!(reference));
                notes.insert(
                    "hint".into(),
                    json!(format!("Others changed layers you just wrote. get_changes {{since: {reference}, exclude: \"self\"}}, re-read those layers, adjust; do not retry blindly.")),
                );
                notes.insert("conflicts".into(), json!(conflicts));
            }
        }
        let artboards: Vec<(String, NodeId)> = named.iter().map(|(p, id)| (ed.pages[*p].id.clone(), artboard(ed, *p, *id))).collect();
        let r = tracker.call(host, client, name, args, now);
        if r.is_ok() && name == "finish_working_on_nodes" {
            let mine = layers(host.editor(), &named);
            self.claims.release(&key, None, &mine);
        }
        let rev = host.editor().changes.rev();
        for (page, a) in artboards {
            self.seen.insert((key.clone(), page, a), rev);
        }
        if notes.is_empty() {
            return r;
        }
        notes.insert("rev".into(), json!(rev));
        notes.insert("applied".into(), json!(r.is_ok()));
        r.map(|mut out| {
            out.push(Out::Text(json!({"swarm": notes}).to_string()));
            out
        })
    }

    /// The oldest revision the client last saw any of these layers' artboards at (its first
    /// call's for an artboard it never looked at): coarse, so it over-reports rather than misses.
    fn reference(&self, key: &str, ed: &Editor, named: &[(usize, NodeId)]) -> u64 {
        let first = self.first.get(key).copied().unwrap_or(0);
        named
            .iter()
            .map(|(p, id)| self.seen.get(&(key.to_string(), ed.pages[*p].id.clone(), artboard(ed, *p, *id))).copied().unwrap_or(first))
            .min()
            .unwrap_or(first)
    }

    fn claim_tool(&mut self, tracker: &mut Tracker, host: &mut dyn Host, client: &Client, args: &Value, now: u64) -> ToolResult {
        let ed = host.editor();
        ed.finish_loading();
        let keys: Vec<String> = args["ids"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        let named = exec::named_layers(ed, &json!({"nodeIds": keys, "pageId": args["pageId"]}));
        if named.len() < keys.len() {
            return Err("some ids are not layers of this project (ids look like \"3fa9c1:14\")".into());
        }
        let resources: Vec<String> = args["resources"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        if let Some(bad) = resources.iter().find(|r| !claims::RESOURCES.contains(&r.as_str())) {
            return Err(format!("no resource `{bad}` (resources: {})", claims::RESOURCES.join(", ")));
        }
        if named.is_empty() && resources.is_empty() {
            return Err("claim ids, resources or both".into());
        }
        let ttl = args["ttlSeconds"].as_u64().unwrap_or(claims::DEFAULT_TTL_SECS);
        let note: String = args["note"].as_str().unwrap_or("").chars().take(120).collect();
        let req = Request { holder: holder(tracker, client), layers: layers(ed, &named), resources: resources.clone(), note, ttl_secs: ttl };
        let me = client.session.clone();
        let mine = |h: &Holder| self.delegated_by(&h.session, &me);
        let exempt: Vec<String> = self.claims.live.iter().filter(|c| mine(&c.holder)).map(|c| c.holder.key.clone()).collect();
        match self.claims.claim(req, now, &|h| exempt.contains(&h.key), &ancestors_in(ed)) {
            Ok(id) => {
                let c = self.claims.live.iter().find(|c| c.id == id).ok_or("claim vanished")?;
                text(json!({
                    "claimId": id,
                    "claimed": named.iter().map(|(p, n)| exec::node_key(ed, *p, *n)).collect::<Vec<_>>(),
                    "resources": resources,
                    "ttlSeconds": c.ttl_ms / 1000,
                    "rev": ed.changes.rev(),
                }))
            }
            Err(denied) => text(json!({"claimed": [], "denied": denied_json(&self.claims, &denied, now),
                "hint": "Nothing was claimed. Do other work first or report back; never wait in a loop."})),
        }
    }

    fn release_tool(&mut self, host: &mut dyn Host, client: &Client, args: &Value) -> ToolResult {
        let ed = host.editor();
        let named = exec::named_layers(ed, &json!({"nodeIds": args["ids"], "pageId": args["pageId"]}));
        let mut released = self.claims.release(&client.key(), args["claimId"].as_str(), &layers(ed, &named));
        if let Some(rs) = args["resources"].as_array() {
            for c in self.claims.live.iter_mut().filter(|c| c.holder.key == client.key()) {
                c.resources.retain(|r| !rs.iter().any(|x| x == r.as_str()));
            }
            let empty: Vec<String> = self.claims.live.iter().filter(|c| c.layers.is_empty() && c.resources.is_empty()).map(|c| c.id.clone()).collect();
            self.claims.live.retain(|c| !empty.contains(&c.id));
            released.extend(empty);
        }
        text(json!({"released": released}))
    }

    fn list(&self, ed: &Editor, now: u64) -> Value {
        let key = |l: &Layer| ed.pages.iter().position(|p| p.id == l.0).map(|p| exec::node_key(ed, p, l.1));
        json!({"claims": self.claims.live.iter().map(|c| json!({
            "claimId": c.id, "holder": c.holder.label, "ids": c.layers.iter().filter_map(key).collect::<Vec<_>>(),
            "resources": c.resources, "note": c.note, "expiresInSeconds": c.expires_in_secs(now),
        })).collect::<Vec<_>>()})
    }

    /// Claims a delegated helper's `scope` for it before it starts, so its work is fenced off
    /// from the start. Returns the helper's identity, or the denial to give the caller.
    pub fn claim_for_helper(&mut self, tracker: &mut Tracker, ed: &mut Editor, caller: &Client, role: &str, scope: &Value, now: u64) -> Result<Client, String> {
        ed.finish_loading();
        let n = self.parents.values().filter(|p| **p == caller.session).count() + 1;
        let helper = Client::agent(&format!("{} / {role} {n}", tracker.label(caller)), Some(caller.session.clone()));
        self.parents.insert(helper.session.clone(), caller.session.clone());
        if role != "build" {
            return Ok(helper);
        }
        let keys = scope["ids"].as_array().filter(|a| !a.is_empty());
        let resources: Vec<String> = scope["resources"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        if keys.is_none() && resources.is_empty() {
            return Err("a build helper needs `scope` {ids, resources}: the layers it may change, so other agents keep out".into());
        }
        let named = exec::named_layers(ed, &json!({"nodeIds": keys}));
        let req = Request { holder: holder(tracker, &helper), layers: layers(ed, &named), resources, note: "delegated build".into(), ttl_secs: HELPER_TTL_SECS };
        let me = caller.key();
        self.claims
            .claim(req, now, &|h| h.key == me, &ancestors_in(ed))
            .map(|_| helper)
            .map_err(|d| json!({"error": "claimed", "denied": denied_json(&self.claims, &d, now), "hint": "Give the helper layers no one else holds."}).to_string())
    }

    /// A helper ended, a chat turn finished: its claims go.
    pub fn release_client(&mut self, client: &Client) {
        let key = client.key();
        self.claims.release_where(|h| h.key == key);
    }

    /// Every chat's claims go (the chat turn is over).
    pub fn release_chats(&mut self) {
        self.claims.release_where(|h| h.key.starts_with(&format!("{}#", super::CHAT_CLIENT)));
    }
}

/// The top-level layer `id` sits in (itself when top-level).
fn artboard(ed: &Editor, page: usize, id: NodeId) -> NodeId {
    ed.page_doc(page).ancestors(id).last().copied().unwrap_or(id)
}

/// Layers named by a write that others (anyone but `me`, the person included) changed after
/// `since`: the layer itself, or for a subtree write anything under it.
fn conflicts(ed: &Editor, me: &str, since: u64, named: &[(usize, NodeId)], reach: Reach) -> Vec<Value> {
    if reach == Reach::Children {
        return Vec::new();
    }
    let mut out: Vec<Value> = Vec::new();
    let mut more = 0;
    for (p, id) in named {
        let page = &ed.pages[*p].id;
        let doc = ed.page_doc(*p);
        let changed = ed.changes.since(since, Some(page), Filter::Exclude(me));
        if changed.truncated {
            return vec![json!({"relation": "history_reset", "hint": "the change log does not reach back that far; re-read what you are about to change"})];
        }
        for g in changed.changes {
            let hit = g.ids.iter().find_map(|c| {
                if c == id {
                    Some("self")
                } else if reach == Reach::Subtree && doc.ancestors(*c).contains(id) {
                    Some("descendant")
                } else {
                    None
                }
            });
            let Some(relation) = hit else { continue };
            if out.len() >= MAX_CONFLICTS {
                more += 1;
                continue;
            }
            out.push(json!({
                "node": exec::node_key(ed, *p, *id), "name": doc.get(*id).map(|n| n.name.clone()),
                "relation": relation, "by": g.by.unwrap_or_else(|| "You".into()), "rev": g.rev,
            }));
        }
    }
    if more > 0 {
        out.push(json!({"more": more}));
    }
    out
}

fn text(v: Value) -> ToolResult {
    Ok(vec![Out::Text(v.to_string())])
}
