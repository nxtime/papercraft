//! A bounded log of which layers changed at which revision, so an agent can ask "what changed
//! since I last looked" (`get_changes`) and lint only that (`lint_design {since}`).
//!
//! The revision here is the log's own counter, not `Editor::revision`: that one also moves when
//! the camera does and once per batch of MCP calls, so a read would bump it. This one moves only
//! when a committed edit, undo or redo changes layers.

use super::Editor;
use crate::model::{Document, NodeId};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

/// Entries kept; a caller older than the oldest one gets `truncated` and does a full pass.
pub const MAX_ENTRIES: usize = 10_000;

/// The MCP client behind a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Who {
    /// Name and session: two agents with the same client name stay apart.
    pub key: String,
    /// What the canvas tag shows ("Claude Code", "Claude Code \u{b7} 2").
    pub label: String,
}

/// One layer changed at one revision.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    rev: u64,
    page: String,
    id: NodeId,
    /// None for the person using the app (and for calls made without a client).
    by: Option<Arc<Who>>,
}

/// Which changes `since` returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter<'a> {
    All,
    /// Everything but this client key's own changes.
    Exclude(&'a str),
    /// Only this client's changes, by label or key.
    Only(&'a str),
}

/// Layers one client changed on one page since the asked revision.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub page: String,
    pub by: Option<String>,
    /// The latest revision in the group.
    pub rev: u64,
    /// Each layer once, in first-change order.
    pub ids: Vec<NodeId>,
}

/// The answer to "what changed since revision N".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Since {
    /// The log no longer reaches back to the asked revision (or it is from the future): the
    /// caller cannot trust `changes` and should look at everything.
    pub truncated: bool,
    pub changes: Vec<Group>,
}

#[derive(Debug, Clone)]
pub struct ChangeLog {
    rev: u64,
    entries: VecDeque<Entry>,
    /// Highest revision of an entry dropped for space: asking since less than this is incomplete.
    floor: u64,
    max: usize,
    /// The MCP client whose call is running, set by the run tracker around each call.
    pub client: Option<Arc<Who>>,
    /// Revision of the last whole-page lint per (client key, page id), for `changedOnly`.
    linted: HashMap<(String, String), u64>,
}

impl Default for ChangeLog {
    fn default() -> Self {
        ChangeLog::with_capacity(MAX_ENTRIES)
    }
}

impl ChangeLog {
    pub fn with_capacity(max: usize) -> ChangeLog {
        ChangeLog { rev: 0, entries: VecDeque::new(), floor: 0, max, client: None, linted: HashMap::new() }
    }

    /// The revision after the latest change.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// Logs `ids` of `page` as changed in a new revision by the running client. Nothing
    /// changed, no new revision.
    pub fn record(&mut self, page: &str, ids: impl IntoIterator<Item = NodeId>) {
        let mut ids = ids.into_iter().peekable();
        if ids.peek().is_none() {
            return;
        }
        self.rev += 1;
        for id in ids {
            self.entries.push_back(Entry { rev: self.rev, page: page.to_string(), id, by: self.client.clone() });
        }
        while self.entries.len() > self.max {
            if let Some(dropped) = self.entries.pop_front() {
                self.floor = self.floor.max(dropped.rev);
            }
        }
    }

    /// Forgets everything (another project was opened): every caller must start over.
    pub fn reset(&mut self) {
        self.rev += 1;
        self.floor = self.rev;
        self.entries.clear();
        self.linted.clear();
    }

    /// Layers changed after revision `since`, optionally only on `page`, grouped by page and
    /// client in first-change order.
    pub fn since(&self, since: u64, page: Option<&str>, filter: Filter) -> Since {
        if since > self.rev || since < self.floor {
            return Since { truncated: true, changes: Vec::new() };
        }
        let keep = |e: &Entry| match filter {
            Filter::All => true,
            Filter::Exclude(k) => e.by.as_ref().is_none_or(|w| w.key != k),
            Filter::Only(k) => e.by.as_ref().is_some_and(|w| w.key == k || w.label == k),
        };
        let mut changes: Vec<Group> = Vec::new();
        for e in self.entries.iter().filter(|e| e.rev > since && page.is_none_or(|p| p == e.page) && keep(e)) {
            let by = e.by.as_ref().map(|w| w.label.clone());
            let at = changes.iter().position(|g| g.page == e.page && g.by == by);
            let i = at.unwrap_or_else(|| {
                changes.push(Group { page: e.page.clone(), by, rev: e.rev, ids: Vec::new() });
                changes.len() - 1
            });
            let g = &mut changes[i];
            g.rev = g.rev.max(e.rev);
            if !g.ids.contains(&e.id) {
                g.ids.push(e.id);
            }
        }
        Since { truncated: false, changes }
    }

    /// Notes that the running client linted all of `page` as of the current revision.
    pub fn note_lint(&mut self, page: &str) {
        if let Some(c) = &self.client {
            self.linted.insert((c.key.clone(), page.to_string()), self.rev);
        }
    }

    /// Revision of the running client's last whole-page lint of `page`.
    pub fn last_lint(&self, page: &str) -> Option<u64> {
        let key = self.client.as_ref()?.key.clone();
        self.linted.get(&(key, page.to_string())).copied()
    }
}

/// Layers of `doc` bound to a variable in `names` (CSS names), or to any variable when `names`
/// is None: through a fill that follows it or a field binding. Sorted by id.
pub fn bound_layers(doc: &Document, names: Option<&BTreeSet<String>>) -> Vec<NodeId> {
    let hit = |v: &str| names.is_none_or(|n| n.contains(v));
    let mut out: Vec<NodeId> = doc
        .nodes
        .values()
        .filter(|n| n.style.fills.iter().filter_map(|p| p.var()).any(hit) || n.bindings.values().any(|v| hit(v)))
        .map(|n| n.id)
        .collect();
    out.sort();
    out
}

impl Editor {
    /// Logs the layers bound to the variables `names` (all variables when None) on every page:
    /// an edit to a variable changes how they look or lay out even when no layer was written.
    pub fn log_variable_users(&mut self, names: Option<&BTreeSet<String>>) {
        self.finish_loading();
        for i in 0..self.pages.len() {
            let ids = bound_layers(self.page_doc(i), names);
            let page = self.pages[i].id.clone();
            self.changes.record(&page, ids);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Color, Node, NodeKind, Paint};

    #[test]
    fn bound_layers_follow_fills_and_field_bindings() {
        let mut doc = Document::default();
        let mut add = |f: &dyn Fn(&mut Node)| {
            let id = doc.alloc_id();
            let mut n = Node::new(id, NodeKind::Frame { clip: false }, Rect::new(0.0, 0.0, 10.0, 10.0));
            f(&mut n);
            doc.nodes.insert(id, n);
            id
        };
        let fill = add(&|n| n.style.fills = vec![Paint::solid_var(Color::rgb(1, 2, 3), "brand")]);
        let gap = add(&|n| {
            n.bindings.insert("gap".into(), "space-4".into());
        });
        let _plain = add(&|n| n.style.fills = vec![Paint::solid(Color::rgb(1, 2, 3))]);
        let some = |s: &str| Some(BTreeSet::from([s.to_string()]));
        assert_eq!(bound_layers(&doc, some("brand").as_ref()), vec![fill]);
        assert_eq!(bound_layers(&doc, some("space-4").as_ref()), vec![gap]);
        assert!(bound_layers(&doc, some("other").as_ref()).is_empty());
        assert_eq!(bound_layers(&doc, None), vec![fill, gap]);
    }

    fn ids(v: &[u64]) -> Vec<NodeId> {
        v.iter().map(|n| NodeId(*n)).collect()
    }

    fn who(key: &str, label: &str) -> Option<Arc<Who>> {
        Some(Arc::new(Who { key: key.into(), label: label.into() }))
    }

    fn all(log: &ChangeLog, since: u64) -> Since {
        log.since(since, None, Filter::All)
    }

    #[test]
    fn each_recorded_edit_is_a_revision() {
        let mut log = ChangeLog::default();
        assert_eq!(log.rev(), 0);
        log.record("p1", ids(&[1, 2]));
        log.record("p1", ids(&[3]));
        assert_eq!(log.rev(), 2);
        let g = &all(&log, 0).changes;
        assert_eq!((g.len(), g[0].ids.clone(), g[0].rev), (1, ids(&[1, 2, 3]), 2));
        assert_eq!(all(&log, 1).changes[0].ids, ids(&[3]));
        assert!(all(&log, 2).changes.is_empty());
    }

    #[test]
    fn an_empty_edit_keeps_the_revision() {
        let mut log = ChangeLog::default();
        log.record("p1", []);
        assert_eq!(log.rev(), 0);
    }

    #[test]
    fn a_layer_changed_twice_is_listed_once() {
        let mut log = ChangeLog::default();
        log.record("p1", ids(&[1]));
        log.record("p1", ids(&[1, 2]));
        assert_eq!(all(&log, 0).changes[0].ids, ids(&[1, 2]));
    }

    #[test]
    fn changes_can_be_limited_to_a_page() {
        let mut log = ChangeLog::default();
        log.record("p1", ids(&[1]));
        log.record("p2", ids(&[1]));
        let s = log.since(0, Some("p2"), Filter::All);
        assert_eq!((s.changes.len(), s.changes[0].page.as_str()), (1, "p2"));
        assert_eq!(all(&log, 0).changes.len(), 2);
    }

    #[test]
    fn asking_older_than_the_oldest_entry_is_truncated() {
        let mut log = ChangeLog::with_capacity(3);
        log.record("p", ids(&[1])); // rev 1
        log.record("p", ids(&[2, 3])); // rev 2
        assert!(!all(&log, 0).truncated);
        log.record("p", ids(&[4])); // rev 3 drops the rev 1 entry
        assert!(all(&log, 0).truncated);
        // Everything after the dropped revision is still complete.
        let s = all(&log, 1);
        assert!(!s.truncated);
        assert_eq!(s.changes[0].ids, ids(&[2, 3, 4]));
    }

    #[test]
    fn the_log_stays_bounded() {
        let mut log = ChangeLog::with_capacity(5);
        for n in 0..20 {
            log.record("p", ids(&[n]));
        }
        assert_eq!(log.entries.len(), 5);
        assert_eq!(all(&log, 15).changes[0].ids, ids(&[15, 16, 17, 18, 19]));
        assert!(all(&log, 14).truncated);
    }

    #[test]
    fn a_future_revision_is_truncated() {
        let mut log = ChangeLog::default();
        log.record("p", ids(&[1]));
        assert!(all(&log, 9).truncated);
    }

    #[test]
    fn reset_forces_a_full_pass() {
        let mut log = ChangeLog::default();
        log.record("p", ids(&[1]));
        let old = log.rev();
        log.reset();
        assert!(all(&log, old).truncated);
        assert!(!all(&log, log.rev()).truncated);
    }

    #[test]
    fn lint_revisions_are_per_client_session_and_page() {
        let mut log = ChangeLog::default();
        log.record("p", ids(&[1]));
        assert_eq!(log.last_lint("p"), None, "no client, nothing remembered");
        log.client = who("rmcp#1", "rmcp");
        log.note_lint("p");
        assert_eq!(log.last_lint("p"), Some(1));
        assert_eq!(log.last_lint("q"), None);
        // Same client name, another session.
        log.client = who("rmcp#2", "rmcp \u{b7} 2");
        assert_eq!(log.last_lint("p"), None);
    }

    #[test]
    fn two_clients_changes_are_attributed_separately() {
        let mut log = ChangeLog::default();
        log.client = who("rmcp#1", "rmcp");
        log.record("p", ids(&[1, 2]));
        log.client = who("rmcp#2", "rmcp \u{b7} 2");
        log.record("p", ids(&[2, 3]));
        log.client = None;
        log.record("p", ids(&[4]));
        let g = all(&log, 0).changes;
        assert_eq!(g.len(), 3);
        assert_eq!((g[0].by.as_deref(), g[0].ids.clone(), g[0].rev), (Some("rmcp"), ids(&[1, 2]), 1));
        assert_eq!((g[1].by.as_deref(), g[1].ids.clone(), g[1].rev), (Some("rmcp \u{b7} 2"), ids(&[2, 3]), 2));
        assert_eq!((g[2].by.clone(), g[2].ids.clone()), (None, ids(&[4])));
    }

    #[test]
    fn filters_pick_one_client_or_everyone_else() {
        let mut log = ChangeLog::default();
        log.client = who("a#1", "A");
        log.record("p", ids(&[1]));
        log.client = who("b#1", "B");
        log.record("p", ids(&[2]));
        log.client = None;
        log.record("p", ids(&[3]));
        let others = log.since(0, None, Filter::Exclude("a#1"));
        let labels: Vec<_> = others.changes.iter().map(|g| g.by.as_deref()).collect();
        assert_eq!(labels, vec![Some("B"), None], "the person's edits are someone else's too");
        let only = log.since(0, None, Filter::Only("B"));
        assert_eq!(only.changes.len(), 1);
        assert_eq!(only.changes[0].ids, ids(&[2]));
        assert_eq!(log.since(0, None, Filter::Only("b#1")).changes, only.changes);
    }
}
