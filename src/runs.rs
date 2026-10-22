//! Agent runs: one record per burst of agent work, kept in the project folder under `runs/` so the
//! Agent view can list, review and revert them after a restart.

use crate::editor::Editor;
use crate::mcp::exec::{Host, find_page};
use crate::mcp::{
    ToolResult,
    diff::{NodeDiff, diff_pages},
    tools::edits_doc,
};
use crate::model::{Document, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const DIR: &str = "runs";
/// Seconds without a tool call before the open run closes.
pub const RUN_IDLE_SECS: u64 = 30;
/// Runs kept per project; older ones are pruned with their auto-versions.
pub const RUNS_KEEP: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RunState {
    #[default]
    Running,
    Review,
    Accepted,
    Reverted,
}

/// A page the run touched.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunPage {
    #[serde(default)]
    pub page_id: String,
    /// Auto-version taken before the run touched the page; None when the run created the page.
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    #[serde(default)]
    pub number: u32,
    #[serde(default)]
    pub title: String,
    /// Unix milliseconds.
    #[serde(default)]
    pub started: u64,
    #[serde(default)]
    pub ended: Option<u64>,
    #[serde(default)]
    pub calls: u32,
    #[serde(default)]
    pub pages: Vec<RunPage>,
    #[serde(default)]
    pub state: RunState,
    #[serde(default)]
    pub added: usize,
    #[serde(default)]
    pub changed: usize,
    #[serde(default)]
    pub removed: usize,
}

fn dir(project: &Path) -> PathBuf {
    project.join(DIR)
}

fn file_of(project: &Path, id: &str) -> PathBuf {
    dir(project).join(format!("{id}.json"))
}

pub fn save(project: &Path, run: &Run) -> std::io::Result<()> {
    crate::vfs::create_dir_all(dir(project))?;
    let bytes = serde_json::to_vec(run).map_err(std::io::Error::other)?;
    crate::vfs::write(file_of(project, &run.id), bytes)
}

/// Restore every page touched by a run, loading all versions before changing the editor.
pub fn revert(project: &Path, ed: &mut Editor, run: &mut Run) -> Result<(), String> {
    let snapshots: Vec<_> = run
        .pages
        .iter()
        .filter_map(|p| {
            p.version.as_deref().map(|version| {
                crate::versions::load(project, version)
                    .map(|doc| (p.page_id.clone(), doc))
                    .ok_or_else(|| {
                        let name = ed
                            .pages
                            .iter()
                            .find(|slot| slot.id == p.page_id)
                            .map_or(p.page_id.as_str(), |slot| slot.name.as_str());
                        format!("Cannot load version for page {name} ({})", p.page_id)
                    })
            })
        })
        .collect::<Result<_, _>>()?;
    let active = ed.active_page_id().to_string();
    for (id, snap) in snapshots {
        let i = ed
            .pages
            .iter()
            .position(|p| p.id == id)
            .unwrap_or_else(|| ed.add_page_with_id(&id, &snap));
        ed.switch_page(i);
        ed.restore_snapshot(&snap);
        ed.rename_page(i, &snap.name);
    }
    for page in run.pages.iter().filter(|p| p.version.is_none()) {
        if let Some(i) = ed.pages.iter().position(|p| p.id == page.page_id) {
            ed.delete_page(i);
        }
    }
    if let Some(i) = ed.pages.iter().position(|p| p.id == active) {
        ed.switch_page(i);
    }
    let mut reverted = run.clone();
    reverted.state = RunState::Reverted;
    save(project, &reverted).map_err(|err| err.to_string())?;
    *run = reverted;
    Ok(())
}

/// The run `id` and every later run of the project (all chats, all pages) that can still be
/// reverted, newest first. `runs` is in `list` order (newest first).
pub fn runs_from(runs: &[Run], id: &str) -> Vec<Run> {
    let Some(from) = runs.iter().find(|r| r.id == id) else { return Vec::new() };
    let key = (from.started, from.number);
    runs.iter()
        .filter(|r| (r.started, r.number) >= key && ![RunState::Reverted, RunState::Running].contains(&r.state))
        .cloned()
        .collect()
}

/// "Restore to here": reverts `runs` (newest first, see [`runs_from`]) one by one, so each page
/// ends as it was before the oldest of them. Stops at the first failure. Returns how many were reverted.
pub fn revert_all(project: &Path, ed: &mut Editor, runs: Vec<Run>) -> Result<usize, String> {
    let mut done = 0;
    for mut run in runs {
        revert(project, ed, &mut run).map_err(|e| format!("run {}: {e}", run.number))?;
        done += 1;
    }
    Ok(done)
}

/// Undoes one row of a run's diff on `page_id`, as one undo step on that page. A modified node gets
/// its snapshot properties back (children stay as they are now), a removed subtree returns under its
/// old parent at its old index (page root when the parent is gone), an added subtree is deleted.
/// Variable rows (`PageDiff::variables`) are not supported: they are `name: value` text, not ids.
pub fn revert_change(
    project: &Path,
    ed: &mut Editor,
    run: &Run,
    page_id: &str,
    node: &NodeDiff,
) -> Result<(), String> {
    let version = run
        .pages
        .iter()
        .find(|p| p.page_id == page_id)
        .and_then(|p| p.version.as_deref())
        .ok_or_else(|| format!("Page {page_id} has no version to revert to"))?;
    let snap = crate::versions::load(project, version)
        .ok_or_else(|| format!("Cannot load version for page {page_id}"))?;
    let page = ed
        .pages
        .iter()
        .position(|p| p.id == page_id)
        .ok_or_else(|| format!("Page {page_id} no longer exists"))?;
    let active = ed.active_page_id().to_string();
    ed.switch_page(page);
    let result = revert_node(ed, &snap, node);
    if let Some(i) = ed.pages.iter().position(|p| p.id == active) {
        ed.switch_page(i);
    }
    result
}

/// Index of `id` among its siblings in `snap`.
fn snap_index(snap: &Document, id: NodeId) -> Option<usize> {
    let siblings = match snap.get(id)?.parent {
        Some(p) => &snap.get(p)?.children,
        None => &snap.root,
    };
    siblings.iter().position(|c| *c == id)
}

fn revert_node(ed: &mut Editor, snap: &Document, node: &NodeDiff) -> Result<(), String> {
    let missing = || format!("Node {} is gone", node.name);
    match (node.id_a, node.id_b) {
        (None, Some(id)) => {
            ed.doc.get(id).ok_or_else(missing)?;
            ed.edit("Revert added", |tx| {
                let survivors: Vec<_> = tx
                    .doc()
                    .subtree(id)
                    .into_iter()
                    .filter(|sid| snap.get(*sid).is_some())
                    .collect();
                for sid in &survivors {
                    tx.reparent(*sid, None, None);
                }
                for sid in survivors {
                    let parent = snap
                        .get(sid)
                        .unwrap()
                        .parent
                        .filter(|p| tx.doc().get(*p).is_some());
                    tx.reparent(sid, parent, snap_index(snap, sid));
                }
                tx.remove(id);
            });
        }
        (Some(id), None) => {
            let top = snap.get(id).ok_or_else(missing)?;
            let parent = top.parent.filter(|p| ed.doc.get(*p).is_some());
            ed.edit("Revert removed", |tx| {
                for sid in snap.subtree(id) {
                    if tx.doc().get(sid).is_some() {
                        continue;
                    }
                    let mut n = snap.get(sid).unwrap().clone();
                    n.children.clear();
                    let parent = if sid == id { parent } else { n.parent };
                    tx.insert(n, parent, snap_index(snap, sid));
                }
            });
        }
        (Some(id), Some(_)) => {
            let old = snap.get(id).ok_or_else(missing)?;
            let cur = ed.doc.get(id).ok_or_else(missing)?;
            let mut restored = old.clone();
            restored.children.clone_from(&cur.children);
            restored.parent = cur.parent;
            let moved = node
                .changes
                .iter()
                .any(|c| ["parent", "order"].contains(&c.field));
            let parent = old.parent.filter(|p| ed.doc.get(*p).is_some());
            let index = snap_index(snap, id);
            ed.edit("Revert change", |tx| {
                if moved {
                    tx.reparent(id, parent, index);
                    restored.parent = tx.doc().get(id).unwrap().parent;
                }
                tx.put(restored);
            });
        }
        (None, None) => return Err(missing()),
    }
    Ok(())
}

/// The saved runs, newest first. Unreadable files are skipped.
pub fn list(project: &Path) -> Vec<Run> {
    let Ok(entries) = crate::vfs::read_dir(dir(project)) else {
        return Vec::new();
    };
    let mut out: Vec<Run> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "json" {
                return None;
            }
            serde_json::from_slice(&crate::vfs::read(&path).ok()?).ok()
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse((r.started, r.number)));
    out
}

/// Deletes a run record and the auto-versions it took.
pub fn delete(project: &Path, id: &str) {
    if let Some(run) = list(project).into_iter().find(|r| r.id == id) {
        for v in run.pages.iter().filter_map(|p| p.version.as_deref()) {
            crate::versions::delete(project, v);
        }
    }
    let _ = crate::vfs::remove_file(file_of(project, id));
}

/// Keeps the newest `RUNS_KEEP` runs, deletes the rest.
pub fn prune(project: &Path) {
    for run in list(project).into_iter().skip(RUNS_KEEP) {
        delete(project, &run.id);
    }
}

/// The number for the next run: one past the highest saved.
pub fn next_number(project: &Path) -> u32 {
    list(project).iter().map(|r| r.number).max().unwrap_or(0) + 1
}

/// An unfinished run on disk (the app died mid-run) becomes reviewable.
fn recover(project: &Path) {
    for mut run in list(project)
        .into_iter()
        .filter(|r| r.state == RunState::Running)
    {
        run.state = RunState::Review;
        run.ended = Some(run.started);
        let _ = save(project, &run);
    }
}

/// The run being recorded, with what the tracker needs between calls.
struct Open {
    run: Run,
    /// Clock of the last successful editing call.
    last: u64,
    /// Activity verb of the first editing call, the title unless `finish_working_on_nodes` names one.
    verb: &'static str,
}

/// Groups the agent's editing tool calls into runs. Hosts route every call through [`Tracker::call`]
/// and call [`Tracker::tick`] once per frame. `now` is Unix milliseconds, injected so tests can fake it.
#[derive(Default)]
pub struct Tracker {
    dir: Option<PathBuf>,
    open: Option<Open>,
    finished: Option<Run>,
}

impl Tracker {
    /// The run being recorded, if any.
    pub fn open_run(&self) -> Option<&Run> {
        self.open.as_ref().map(|o| &o.run)
    }

    /// The run that ended last, once (for the finish banner).
    pub fn take_finished(&mut self) -> Option<Run> {
        self.finished.take()
    }

    /// Runs `exec::call`, recording it in the open run when it edits the document.
    pub fn call(&mut self, host: &mut dyn Host, name: &str, args: &Value, now: u64) -> ToolResult {
        let dir = host.project().map(|p| p.dir.clone());
        self.sync_project(now, dir.as_deref());
        let Some(dir) = dir else {
            return crate::mcp::exec::call(host, name, args);
        };
        let outgoing = (name == "open_file")
            .then(|| self.counts(host.editor()))
            .flatten();
        let edits = edits_doc(name, args);
        let mut before = None;
        if edits {
            if self.open.as_ref().is_some_and(|o| idle(o.last, now)) {
                self.end(now, Some(host.editor()), None);
            }
            let opened = self.open.is_none();
            match self.before(&dir, host.editor(), name, args, now) {
                Ok(saved) => before = Some(saved),
                Err(err) => {
                    if opened && let Some(open) = self.open.take() {
                        for version in open.run.pages.iter().filter_map(|p| p.version.as_deref()) {
                            crate::versions::delete(&dir, version);
                        }
                    }
                    return Err(err);
                }
            }
        }
        let r = crate::mcp::exec::call(host, name, args);
        let current = host.project().map(|p| p.dir.clone());
        if current.as_deref() != Some(dir.as_path()) {
            self.end_counted(now, outgoing, None);
            self.sync_project(now, current.as_deref());
        }
        if let Some((opened, ids)) = before {
            self.after(&dir, host.editor(), r.is_ok(), opened, &ids, now);
        } else if name == "finish_working_on_nodes" && self.open.is_some() {
            let ed = host.editor();
            let title = finish_title(ed, args);
            self.end(now, Some(ed), title);
        }
        r
    }

    /// Closes the open run now (a chat turn is over), the way `finish_working_on_nodes` does,
    /// instead of waiting `RUN_IDLE_SECS`.
    pub fn finish(&mut self, now: u64, ed: &Editor) {
        self.end(now, Some(ed), None);
    }

    /// Per frame: closes the run after `RUN_IDLE_SECS` idle or when the user switched project.
    pub fn tick(&mut self, now: u64, dir: Option<&Path>, ed: &Editor) {
        self.sync_project(now, dir);
        if self.open.as_ref().is_some_and(|o| idle(o.last, now)) {
            self.end(now, Some(ed), None);
        }
    }

    /// Notices a project change: the old run ends (its pages are gone, so no diff), the new project's
    /// stale `Running` records are recovered.
    fn sync_project(&mut self, now: u64, dir: Option<&Path>) {
        if self.dir.as_deref() == dir {
            return;
        }
        self.end(now, None, None);
        self.dir = dir.map(Path::to_path_buf);
        if let Some(d) = dir {
            recover(d);
        }
    }

    /// Opens the run if needed and snapshots the pages the call targets. Returns whether this call
    /// opened the run, and the page ids before the call.
    fn before(
        &mut self,
        dir: &Path,
        ed: &Editor,
        name: &str,
        args: &Value,
        now: u64,
    ) -> Result<(bool, Vec<String>), String> {
        let opened = self.open.is_none();
        let open = self.open.get_or_insert_with(|| {
            let number = next_number(dir);
            let run = Run {
                id: format!("r{number}-{now}"),
                number,
                started: now,
                ..Run::default()
            };
            Open {
                run,
                last: now,
                verb: crate::ui::agent_verb(name),
            }
        });
        for i in target_pages(ed, name, args) {
            let page_id = &ed.pages[i].id;
            if open.run.pages.iter().any(|p| &p.page_id == page_id) {
                continue;
            }
            let label = format!("Before agent run {}", open.run.number);
            let v = crate::versions::save(dir, page_id, &label, true, ed.page_doc(i)).map_err(
                |err| format!("Cannot save recovery snapshot for page {page_id}: {err}"),
            )?;
            open.run.pages.push(RunPage {
                page_id: page_id.clone(),
                version: Some(v.id),
            });
        }
        let _ = save(dir, &open.run);
        Ok((opened, ed.pages.iter().map(|p| p.id.clone()).collect()))
    }

    fn after(&mut self, dir: &Path, ed: &Editor, ok: bool, opened: bool, ids: &[String], now: u64) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if !ok {
            if opened {
                delete(dir, &open.run.id);
                self.open = None;
            }
            return;
        }
        open.run.calls += 1;
        open.last = now;
        for p in ed.pages.iter().filter(|p| !ids.contains(&p.id)) {
            open.run.pages.push(RunPage {
                page_id: p.id.clone(),
                version: None,
            });
        }
        let _ = save(dir, &open.run);
    }

    /// Closes the open run: no changes drops it with its versions, otherwise it waits for review.
    /// Without `ed` (the project is gone) the changes cannot be counted and the run is kept.
    fn end(&mut self, now: u64, ed: Option<&Editor>, title: Option<String>) {
        let counts = ed.and_then(|ed| self.counts(ed));
        self.end_counted(now, counts, title);
    }

    pub(crate) fn counts(&self, ed: &Editor) -> Option<(usize, usize, usize, bool)> {
        let (open, dir) = (self.open.as_ref()?, self.dir.as_ref()?);
        let (mut added, mut changed, mut removed, mut empty) = (0, 0, 0, true);
        for p in &open.run.pages {
            let page = ed.pages.iter().position(|s| s.id == p.page_id);
            let after = page.map(|i| ed.page_doc(i)).cloned().unwrap_or_default();
            let Some(v) = &p.version else {
                if page.is_some() {
                    added += after.root.len().max(1);
                    empty = false;
                }
                continue;
            };
            let Some(before) = crate::versions::load(dir, v) else {
                empty = false;
                continue;
            };
            let d = diff_pages(&before, &after);
            let renamed = page.is_some_and(|i| ed.pages[i].name != before.name);
            let deleted = page.is_none();
            added += d.added;
            changed += d.modified + usize::from(renamed);
            removed += d.removed + usize::from(deleted);
            empty &= d.is_empty() && !renamed && !deleted;
        }
        Some((added, changed, removed, empty))
    }

    pub(crate) fn end_counted(
        &mut self,
        now: u64,
        counts: Option<(usize, usize, usize, bool)>,
        title: Option<String>,
    ) {
        let (Some(open), Some(dir)) = (self.open.take(), self.dir.clone()) else {
            return;
        };
        let mut run = open.run;
        if let Some((added, changed, removed, empty)) = counts {
            if empty {
                delete(&dir, &run.id);
                return;
            }
            (run.added, run.changed, run.removed) = (added, changed, removed);
        }
        run.title = title.unwrap_or_else(|| open.verb.to_string());
        run.state = RunState::Review;
        run.ended = Some(now);
        let _ = save(&dir, &run);
        prune(&dir);
        self.finished = Some(run);
    }
}

fn strs(v: &Value) -> impl Iterator<Item = &str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str)
}

fn idle(last: u64, now: u64) -> bool {
    now.saturating_sub(last) >= RUN_IDLE_SECS * 1000
}

/// Resolve source and destination pages before the tool changes either document.
fn target_pages(ed: &Editor, name: &str, args: &Value) -> Vec<usize> {
    if name == "create_page" {
        return Vec::new();
    }
    if name == "import_design_system" {
        return (0..ed.pages.len()).collect();
    }
    fn collect(
        ed: &Editor,
        args: &Value,
        default: usize,
        to: Option<usize>,
        pages: &mut Vec<usize>,
    ) {
        let default = args["pageId"]
            .as_str()
            .and_then(|p| find_page(ed, p).ok())
            .unwrap_or(default);
        for key in ["pageId", "fromPageId"] {
            if let Some(i) = args[key].as_str().and_then(|p| find_page(ed, p).ok()) {
                pages.push(i);
            }
        }
        pages.extend(strs(&args["pageIds"]).filter_map(|p| find_page(ed, p).ok()));
        let resolve = |key: &str, fallback| {
            let key = key.trim().trim_start_matches('#');
            key.split_once(':')
                .and_then(|(p, _)| find_page(ed, p).ok())
                .unwrap_or(fallback)
        };
        let mut source = default;
        for key in ["nodeId", "targetNodeId", "id", "componentId"] {
            if let Some(id) = args[key].as_str().filter(|s| !s.is_empty()) {
                source = resolve(id, default);
                pages.push(source);
            }
        }
        pages.extend(strs(&args["nodeIds"]).map(|id| resolve(id, default)));
        let to = args["toPageId"]
            .as_str()
            .and_then(|p| find_page(ed, p).ok())
            .or(to);
        if let Some(to) = to {
            pages.push(to);
        }
        let mut dest = to.unwrap_or(source);
        for key in ["before", "after", "parentId"] {
            if let Some(id) = args[key].as_str().filter(|s| !s.is_empty()) {
                dest = resolve(id, dest);
                pages.push(dest);
            }
        }
        for key in ["updates", "moves", "nodes"] {
            for item in args[key].as_array().into_iter().flatten() {
                collect(ed, item, default, to, pages);
            }
        }
    }
    let mut pages = Vec::new();
    collect(ed, args, ed.active_page, None, &mut pages);
    if pages.is_empty() {
        pages.push(ed.active_page);
    }
    pages.sort_unstable();
    pages.dedup();
    pages
}

/// Names of the nodes `finish_working_on_nodes` was given, as the run title.
fn finish_title(ed: &Editor, args: &Value) -> Option<String> {
    let names: Vec<String> = args["nodeIds"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|k| {
            let k = k.trim().trim_start_matches('#');
            let (page, num) = match k.split_once(':') {
                Some((p, n)) => (find_page(ed, p).ok()?, n),
                None => (ed.active_page, k),
            };
            let doc: &Document = ed.page_doc(page);
            Some(doc.get(NodeId(num.parse().ok()?))?.name.clone())
        })
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::design;
    use crate::model::Document;

    fn temp() -> PathBuf {
        std::env::temp_dir().join(format!("papercraft-runs-{:016x}", fastrand::u64(..)))
    }

    fn run(n: u32) -> Run {
        Run {
            id: format!("r{n}"),
            number: n,
            started: n as u64,
            ..Run::default()
        }
    }

    fn snapshot_page(dir: &Path, ed: &Editor, i: usize) -> RunPage {
        let page = ed.page_data(i);
        let version = crate::versions::save(dir, &page.id, "Before run", true, &page.doc).unwrap();
        RunPage {
            page_id: page.id,
            version: Some(version.id),
        }
    }

    fn assert_content(actual: &Document, expected: &Document) {
        assert_eq!(actual.nodes, expected.nodes);
        assert_eq!(actual.root, expected.root);
        assert_eq!(actual.var_state(), expected.var_state());
    }

    #[test]
    fn revert_restores_two_pages_and_each_undo_returns_agent_result() {
        use crate::geom::Rect;
        use crate::model::{Color, Node, NodeKind};

        let dir = temp();
        let mut ed = Editor::default();
        ed.add_page_in_background(Some("Second"));
        let mut r = run(1);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for i in 0..2 {
            ed.switch_page(i);
            for _ in 0..2 {
                ed.add_node(
                    Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 50.0, 50.0)),
                    None,
                );
            }
            // Variables are project-wide: every page shows the same values.
            let none = crate::variables::PageModes::new();
            ed.edit_design("Design", |ds| {
                design::set_color(ds, &none, "accent", Some("Light"), Color::rgb(200, 0, 0))?;
                design::set_color(ds, &none, "accent", Some("Dark"), Color::rgb(0, 0, 200))?;
                design::set_token(ds, "gap", "8px")
            })
            .unwrap();
            before.push(ed.page_data(i).doc);
            r.pages.push(snapshot_page(&dir, &ed, i));
            let root = ed.doc.root[0];
            ed.edit("Agent changes", |tx| {
                tx.node_mut(root).unwrap().name = "Agent".into();
                tx.move_to_index(root, 1);
            });
            ed.edit_design("Agent design", |ds| {
                design::set_color(ds, &none, "accent", None, Color::rgb(0, 200, 0))?;
                design::set_token(ds, "gap", "16px")
            })
            .unwrap();
            after.push(ed.doc.clone());
            ed.rename_page(i, "Renamed");
        }
        let active = ed.active_page_id().to_string();
        revert(&dir, &mut ed, &mut r).unwrap();
        assert_eq!(ed.active_page_id(), active);
        assert_eq!(r.state, RunState::Reverted);
        assert_eq!(list(&dir), vec![r]);
        // The design is shared, so the restored variables are the snapshots' (identical here).
        for i in 0..2 {
            ed.switch_page(i);
            assert_eq!(ed.doc.var_state(), before[0].var_state());
        }
        for i in 0..2 {
            ed.switch_page(i);
            assert_eq!(ed.doc.nodes, before[i].nodes);
            assert_eq!(ed.doc.root, before[i].root);
            assert_eq!(ed.page_name(), before[i].name);
            assert_eq!(ed.doc.name, before[i].name);
            ed.undo();
            assert_eq!(ed.doc.nodes, after[i].nodes);
            assert_eq!(ed.doc.root, after[i].root);
        }
    }

    #[test]
    fn restore_to_here_reverts_this_run_and_every_later_one_across_pages() {
        use crate::geom::Rect;
        use crate::model::{Node, NodeKind};

        let dir = temp();
        let mut ed = Editor::default();
        ed.add_page_in_background(Some("Second"));
        let add = |ed: &mut Editor, page: usize, name: &str| {
            ed.switch_page(page);
            let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 10.0, 10.0));
            n.name = name.into();
            ed.add_node(n, None);
        };
        // Run 1 touches page 0, run 2 page 1, run 3 page 0 again; each snapshots before it edits.
        let mut runs = Vec::new();
        let originals: Vec<_> = (0..2).map(|i| ed.page_data(i).doc.nodes.len()).collect();
        for (n, page) in [(1, 0), (2, 1), (3, 0)] {
            let mut r = run(n);
            r.state = RunState::Review;
            r.pages.push(snapshot_page(&dir, &ed, page));
            save(&dir, &r).unwrap();
            add(&mut ed, page, &format!("run{n}"));
            runs.push(r);
        }
        runs.reverse();
        let picked = runs_from(&runs, "r2");
        assert_eq!(picked.iter().map(|r| r.number).collect::<Vec<_>>(), [3, 2], "this run and the later ones, newest first");
        assert_eq!(revert_all(&dir, &mut ed, picked).unwrap(), 2);
        assert_eq!(ed.page_data(1).doc.nodes.len(), originals[1], "page 1 is back before run 2");
        assert_eq!(ed.page_data(0).doc.nodes.len(), originals[0] + 1, "run 1 stays");
        let states: Vec<_> = list(&dir).iter().map(|r| (r.number, r.state)).collect();
        assert_eq!(states, [(3, RunState::Reverted), (2, RunState::Reverted), (1, RunState::Review)]);
        assert!(runs_from(&list(&dir), "r2").is_empty(), "already reverted runs are not offered again");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn revert_one_change_leaves_the_others_and_is_one_undo() {
        use crate::geom::Rect;
        use crate::model::{Node, NodeKind};

        let dir = temp();
        let mut ed = Editor::default();
        let mut ids = Vec::new();
        for n in ["Keep", "Change", "Remove"] {
            let mut node = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 50.0, 50.0));
            node.name = n.into();
            ids.push(ed.add_node(node, None));
        }
        let mut r = run(1);
        r.pages.push(snapshot_page(&dir, &ed, 0));
        let snap = ed.doc.clone();
        let page_id = ed.active_page_id().to_string();
        ed.add_page_in_background(Some("Other"));
        let added = ed.edit("Agent", |tx| {
            tx.node_mut(ids[1]).unwrap().name = "Changed".into();
            tx.node_mut(ids[1]).unwrap().rect = Rect::new(5.0, 5.0, 80.0, 80.0);
            tx.remove(ids[2]);
            let id = tx.alloc_id();
            let mut n = Node::new(id, NodeKind::Rect, Rect::new(0.0, 0.0, 10.0, 10.0));
            n.name = "New".into();
            tx.insert(n, None, None)
        });
        let agent = ed.doc.clone();
        let rows = |ed: &Editor| {
            let d = diff_pages(&snap, &ed.doc);
            d.groups
                .into_iter()
                .flat_map(|g| g.nodes)
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(&ed).len(), 3);
        for status in ["added", "modified", "removed"] {
            let row = rows(&ed).into_iter().find(|n| n.status == status).unwrap();
            ed.switch_page(1);
            revert_change(&dir, &mut ed, &r, &page_id, &row).unwrap();
            assert_eq!(ed.active_page, 1);
            ed.switch_page(0);
            let left = rows(&ed);
            assert_eq!(left.len(), 2);
            assert!(left.iter().all(|n| n.status != status));
            match status {
                "added" => assert!(ed.doc.get(added).is_none()),
                "modified" => assert_eq!(ed.doc.get(ids[1]), snap.get(ids[1])),
                _ => assert_eq!(ed.doc.get(ids[2]), snap.get(ids[2])),
            }
            ed.undo();
            assert_content(&ed.doc, &agent);
        }
    }

    #[test]
    fn revert_recreates_deleted_page_before_deleting_created_only_page() {
        let dir = temp();
        let mut ed = Editor::default();
        ed.edit("Content", |tx| {
            let id = tx.alloc_id();
            tx.insert(
                crate::model::Node::new(
                    id,
                    crate::model::NodeKind::Rect,
                    crate::geom::Rect::new(0.0, 0.0, 50.0, 50.0),
                ),
                None,
                None,
            );
            tx.set_token("gap", "8px");
        });
        let before = ed.page_data(0);
        let mut r = run(1);
        let saved = snapshot_page(&dir, &ed, 0);
        let created = ed.add_page();
        let created_id = ed.pages[created].id.clone();
        ed.delete_page(0);
        r.pages = vec![
            RunPage {
                page_id: created_id.clone(),
                version: None,
            },
            saved,
        ];
        revert(&dir, &mut ed, &mut r).unwrap();
        assert_eq!(ed.pages.len(), 1);
        assert_eq!(ed.active_page_id(), before.id);
        assert_eq!(ed.page_name(), before.name);
        assert_content(&ed.doc, &before.doc);
        assert!(!ed.doc.nodes.contains_key(&NodeId(ed.doc.next_id)));
        assert!(!ed.pages.iter().any(|p| p.id == created_id));
    }

    #[test]
    fn revert_missing_version_changes_nothing() {
        let dir = temp();
        let mut ed = Editor::default();
        let mut r = run(1);
        r.pages.push(snapshot_page(&dir, &ed, 0));
        ed.edit("Agent", |tx| tx.set_token("gap", "16px"));
        let created = ed.add_page();
        r.pages.push(RunPage {
            page_id: ed.pages[created].id.clone(),
            version: None,
        });
        let missing = ed.add_page();
        ed.rename_page(missing, "Missing page");
        r.pages.push(RunPage {
            page_id: ed.pages[missing].id.clone(),
            version: Some("missing".into()),
        });
        save(&dir, &r).unwrap();
        let original = r.clone();
        let pages: Vec<_> = (0..ed.pages.len()).map(|i| ed.page_data(i)).collect();
        let active = ed.active_page_id().to_string();
        let revision = ed.revision;
        let err = revert(&dir, &mut ed, &mut r).unwrap_err();
        assert!(err.contains("Missing page"));
        assert_eq!(r, original);
        assert_eq!(list(&dir), vec![original]);
        assert_eq!(ed.active_page_id(), active);
        assert_eq!(ed.revision, revision);
        for (i, page) in pages.iter().enumerate() {
            assert_eq!(ed.page_data(i).doc, page.doc);
            assert_eq!(ed.pages[i].id, page.id);
        }
    }

    fn rect(ed: &mut Editor, parent: Option<NodeId>) -> NodeId {
        ed.add_node(
            crate::model::Node::new(
                NodeId(0),
                crate::model::NodeKind::Frame { clip: false },
                crate::geom::Rect::new(0.0, 0.0, 50.0, 50.0),
            ),
            parent,
        )
    }

    fn row(before: &Document, after: &Document, id: NodeId) -> NodeDiff {
        diff_pages(before, after)
            .groups
            .into_iter()
            .flat_map(|g| g.nodes)
            .find(|n| n.id_a == Some(id) || n.id_b == Some(id))
            .unwrap()
    }

    #[test]
    fn removed_subtree_keeps_surviving_descendants_and_undo() {
        let mut ed = Editor::default();
        let parent = rect(&mut ed, None);
        let child = rect(&mut ed, Some(parent));
        let grandchild = rect(&mut ed, Some(child));
        let missing = rect(&mut ed, Some(parent));
        let snap = ed.doc.clone();
        ed.edit("Agent", |tx| {
            tx.reparent(child, None, None);
            tx.node_mut(grandchild).unwrap().name = "Keep edit".into();
            tx.remove(parent);
        });
        let after = ed.doc.clone();
        revert_node(&mut ed, &snap, &row(&snap, &after, parent)).unwrap();
        assert_eq!(ed.doc.get(child), after.get(child));
        assert_eq!(ed.doc.get(grandchild), after.get(grandchild));
        assert_eq!(ed.doc.get(parent).unwrap().children, vec![missing]);
        assert_eq!(ed.doc.get(missing), snap.get(missing));
        ed.undo();
        assert_content(&ed.doc, &after);
    }

    #[test]
    fn added_subtree_rescues_snapshot_nodes_and_undo() {
        let mut ed = Editor::default();
        let original = rect(&mut ed, None);
        let child = rect(&mut ed, Some(original));
        let nested = rect(&mut ed, Some(child));
        let snap = ed.doc.clone();
        let added = rect(&mut ed, None);
        ed.edit("Agent", |tx| {
            tx.reparent(original, Some(added), None);
            tx.node_mut(nested).unwrap().name = "Keep edit".into();
        });
        let after = ed.doc.clone();
        revert_node(&mut ed, &snap, &row(&snap, &after, added)).unwrap();
        assert!(ed.doc.get(added).is_none());
        assert_eq!(ed.doc.get(original).unwrap().parent, None);
        assert_eq!(ed.doc.get(child).unwrap().parent, Some(original));
        assert_eq!(ed.doc.get(nested), after.get(nested));
        ed.undo();
        assert_content(&ed.doc, &after);
    }

    #[test]
    fn moved_node_with_missing_original_parent_returns_to_root() {
        let mut ed = Editor::default();
        let original = rect(&mut ed, None);
        let child = rect(&mut ed, Some(original));
        let dest = rect(&mut ed, None);
        let snap = ed.doc.clone();
        ed.edit("Agent", |tx| {
            tx.reparent(child, Some(dest), None);
            tx.remove(original);
        });
        let after = ed.doc.clone();
        revert_node(&mut ed, &snap, &row(&snap, &after, child)).unwrap();
        assert_eq!(ed.doc.get(child).unwrap().parent, None);
        assert!(ed.doc.root.contains(&child));
        assert!(!ed.doc.get(dest).unwrap().children.contains(&child));
        ed.undo();
        assert_content(&ed.doc, &after);
    }

    #[test]
    fn cyclic_revert_keeps_parent_and_child_lists_consistent() {
        let mut ed = Editor::default();
        let original = rect(&mut ed, None);
        let child = rect(&mut ed, Some(original));
        let snap = ed.doc.clone();
        ed.edit("Agent", |tx| {
            tx.reparent(child, None, None);
            tx.reparent(original, Some(child), None);
            tx.node_mut(child).unwrap().name = "Changed".into();
        });
        let after = ed.doc.clone();
        revert_node(&mut ed, &snap, &row(&snap, &after, child)).unwrap();
        assert_eq!(ed.doc.get(child).unwrap().parent, None);
        assert!(ed.doc.root.contains(&child));
        assert!(!ed.doc.get(original).unwrap().children.contains(&child));
        ed.undo();
        assert_content(&ed.doc, &after);
    }

    #[test]
    fn run_round_trips() {
        let dir = temp();
        let r = Run {
            title: "Drawing layout".into(),
            ended: Some(9),
            calls: 3,
            pages: vec![
                RunPage {
                    page_id: "p1".into(),
                    version: Some("p1__1".into()),
                },
                RunPage {
                    page_id: "p2".into(),
                    version: None,
                },
            ],
            state: RunState::Review,
            added: 4,
            changed: 2,
            removed: 1,
            ..run(1)
        };
        save(&dir, &r).unwrap();
        assert_eq!(list(&dir), vec![r]);
        assert_eq!(next_number(&dir), 2);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn list_without_runs_dir_is_empty() {
        let dir = temp();
        assert!(list(&dir).is_empty());
        assert_eq!(next_number(&dir), 1);
    }

    #[test]
    fn delete_removes_run_versions_only() {
        let dir = temp();
        let doc = Document::default();
        let auto = crate::versions::save(&dir, "p1", "auto", true, &doc).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let named = crate::versions::save(&dir, "p1", "keep me", false, &doc).unwrap();
        let mut r = run(1);
        r.pages = vec![RunPage {
            page_id: "p1".into(),
            version: Some(auto.id.clone()),
        }];
        save(&dir, &r).unwrap();
        delete(&dir, &r.id);
        assert!(list(&dir).is_empty());
        assert!(crate::versions::load(&dir, &auto.id).is_none());
        assert!(crate::versions::load(&dir, &named.id).is_some());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn prune_keeps_newest_fifty() {
        let dir = temp();
        for n in 1..=52 {
            save(&dir, &run(n)).unwrap();
        }
        prune(&dir);
        let nums: Vec<u32> = list(&dir).iter().map(|r| r.number).collect();
        assert_eq!(nums.len(), RUNS_KEEP);
        assert_eq!(nums.first(), Some(&52));
        assert_eq!(nums.last(), Some(&3));
        std::fs::remove_dir_all(dir).ok();
    }

    use crate::mcp::exec::SessionHost;
    use crate::project::Store;
    use serde_json::json;

    const T0: u64 = 1_000_000;
    const GAP: u64 = RUN_IDLE_SECS * 1000 + 1;

    fn host() -> SessionHost {
        let store = Store::open(temp()).unwrap();
        let mut h = SessionHost::new(store);
        h.session.create(&mut h.ed, "Runs").unwrap();
        h
    }

    fn dir_of(h: &SessionHost) -> PathBuf {
        h.session.project.as_ref().unwrap().dir.clone()
    }

    fn write(t: &mut Tracker, h: &mut SessionHost, now: u64) {
        let args = json!({"targetNodeId": "root", "html": "<div style=\"width: 50px; height: 50px\"></div>"});
        t.call(h, "write_html", &args, now).unwrap();
    }

    #[test]
    fn annotation_only_run_is_kept_counted_reverted_and_undone() {
        let (mut h, mut t) = (host(), Tracker::default());
        write(&mut t, &mut h, T0);
        t.tick(T0 + GAP + 1, Some(&dir_of(&h)), &h.ed);
        t.take_finished().unwrap();
        let node = h.ed.doc.root[0].0.to_string();
        let notes = json!({"notes": [{"node": node, "text": "Rule one"}, {"node": node, "text": "Rule two"}]});
        t.call(&mut h, "annotate", &notes, T0 + GAP + 2).unwrap();
        assert_eq!(h.ed.doc.annotations.len(), 2);
        assert_eq!(t.counts(&h.ed), Some((2, 0, 0, false)));
        t.tick(T0 + 2 * GAP + 3, Some(&dir_of(&h)), &h.ed);
        let mut run = t.take_finished().expect("annotation-only run kept");
        assert_eq!((run.added, run.changed, run.removed), (2, 0, 0));
        assert_eq!(run.state, RunState::Review);
        let with_notes = h.ed.doc.annotations.clone();
        revert(&dir_of(&h), &mut h.ed, &mut run).unwrap();
        assert!(h.ed.doc.annotations.is_empty());
        h.ed.undo();
        assert_eq!(h.ed.doc.annotations, with_notes);
    }

    #[test]
    fn target_pages_resolves_nested_sources_and_destinations() {
        let mut ed = Editor::default();
        for name in ["Source", "Destination", "Third"] {
            ed.add_page_in_background(Some(name));
        }
        let source = ed.pages[1].id.clone();
        let dest = ed.pages[2].id.clone();
        let third = ed.pages[3].id.clone();
        for (name, args, expected) in [
            (
                "rename_nodes",
                json!({"updates": [{"nodeId": format!(" #{source}:1 "), "name": "X"}]}),
                vec![1],
            ),
            (
                "update_styles",
                json!({"updates": [{"nodeIds": [format!("{source}:1"), format!("{third}:2")]}]}),
                vec![1, 3],
            ),
            (
                "move_nodes",
                json!({"moves": [{"nodeId": format!("{source}:1"), "toPageId": dest}]}),
                vec![1, 2],
            ),
            (
                "move_nodes",
                json!({"moves": [{"nodeId": format!("{source}:1"), "before": format!("{dest}:2")}]}),
                vec![1, 2],
            ),
            (
                "duplicate_nodes",
                json!({"nodes": [{"id": format!("{source}:1"), "parentId": format!("{dest}:2")}]}),
                vec![1, 2],
            ),
            (
                "duplicate_nodes",
                json!({"pageId": source, "toPageId": dest, "nodes": [{"id": "1", "parentId": "root"}]}),
                vec![1, 2],
            ),
            (
                "create_shape",
                json!({"parentId": format!("{dest}:2")}),
                vec![2],
            ),
            (
                "rename_nodes",
                json!({"pageId": source, "updates": [{"nodeId": "1"}, {"nodeId": format!("{dest}:2")}]}),
                vec![1, 2],
            ),
            (
                "move_nodes",
                json!({"moves": [{"nodeId": "1", "parentId": format!("{dest}:2")}]}),
                vec![0, 2],
            ),
        ] {
            let mut actual = target_pages(&ed, name, &args);
            actual.sort_unstable();
            assert_eq!(actual, expected, "{name}: {args}");
        }
    }

    #[test]
    fn snapshot_failure_aborts_edit_and_can_retry() {
        let (mut h, mut t) = (host(), Tracker::default());
        let dir = dir_of(&h);
        std::fs::write(dir.join("versions"), "blocked").unwrap();
        let before = h.ed.doc.clone();
        let err = t
            .call(
                &mut h,
                "write_html",
                &json!({"targetNodeId": "root", "html": "<div>Hi</div>"}),
                T0,
            )
            .unwrap_err();
        assert!(err.contains("snapshot"), "{err}");
        assert_content(&h.ed.doc, &before);
        assert_eq!(t.open_run().map_or(0, |r| r.calls), 0);
        std::fs::remove_file(dir.join("versions")).unwrap();
        write(&mut t, &mut h, T0 + 1);
        assert_eq!(t.open_run().unwrap().calls, 1);
        assert_eq!(t.open_run().unwrap().pages.len(), 1);
    }

    #[test]
    fn unavailable_snapshot_keeps_run_and_remaining_versions() {
        for corrupt in [false, true] {
            let (mut h, mut t) = (host(), Tracker::default());
            write(&mut t, &mut h, T0);
            h.ed.undo();
            let dir = dir_of(&h);
            let missing = t.open_run().unwrap().pages[0].version.clone().unwrap();
            h.ed.add_page_in_background(Some("Other"));
            let other = h.ed.pages[1].id.clone();
            t.call(
                &mut h,
                "rename_pages",
                &json!({"updates": [{"pageId": other, "name": "Changed"}]}),
                T0 + 1,
            )
            .unwrap();
            h.ed.rename_page(1, "Other");
            let remaining = t.open_run().unwrap().pages[1].version.clone().unwrap();
            if corrupt {
                std::fs::write(
                    dir.join("versions").join(format!("{missing}.json")),
                    "broken",
                )
                .unwrap();
            } else {
                crate::versions::delete(&dir, &missing);
            }
            t.tick(T0 + GAP + 1, Some(&dir), &h.ed);
            assert_eq!(t.take_finished().unwrap().state, RunState::Review);
            assert_eq!(list(&dir).len(), 1);
            assert!(crate::versions::load(&dir, &remaining).is_some());
        }
    }

    #[test]
    fn page_metadata_changes_keep_runs_and_count_changes() {
        for deleted in [false, true] {
            let (mut h, mut t) = (host(), Tracker::default());
            let page = h.ed.pages[0].id.clone();
            if deleted {
                h.ed.add_page_in_background(Some("Other"));
                t.call(&mut h, "delete_pages", &json!({"pageIds": [page]}), T0)
                    .unwrap();
            } else {
                t.call(
                    &mut h,
                    "rename_pages",
                    &json!({"updates": [{"pageId": page, "name": "Renamed"}]}),
                    T0,
                )
                .unwrap();
            }
            t.tick(T0 + GAP, Some(&dir_of(&h)), &h.ed);
            let run = t.take_finished().unwrap();
            assert_eq!(
                (run.changed, run.removed),
                if deleted { (0, 1) } else { (1, 0) }
            );
        }
    }

    #[test]
    fn six_edits_make_one_run_and_one_version() {
        let (mut h, mut t) = (host(), Tracker::default());
        for i in 0..6 {
            write(&mut t, &mut h, T0 + i * 1000);
        }
        assert_eq!(t.open_run().unwrap().calls, 6);
        t.tick(T0 + GAP + 5000, Some(&dir_of(&h)), &h.ed);
        let run = t.take_finished().unwrap();
        let dir = dir_of(&h);
        assert_eq!((run.number, run.calls, run.added), (1, 6, 6));
        assert_eq!(run.state, RunState::Review);
        assert_eq!(run.title, "Drawing layout");
        assert_eq!(list(&dir), vec![run]);
        assert_eq!(crate::versions::list(&dir, &h.ed.pages[0].id).len(), 1);
    }

    #[test]
    fn gap_starts_next_run() {
        let (mut h, mut t) = (host(), Tracker::default());
        write(&mut t, &mut h, T0);
        std::thread::sleep(std::time::Duration::from_millis(3));
        write(&mut t, &mut h, T0 + GAP);
        assert_eq!(t.take_finished().unwrap().number, 1);
        assert_eq!(t.open_run().unwrap().number, 2);
    }

    #[test]
    fn read_only_calls_make_no_run() {
        let (mut h, mut t) = (host(), Tracker::default());
        for (n, a) in [
            ("get_basic_info", json!({})),
            ("lint_design", json!({})),
            ("select_nodes", json!({"nodeIds": []})),
        ] {
            let _ = t.call(&mut h, n, &a, T0);
        }
        assert!(t.open_run().is_none());
        assert!(list(&dir_of(&h)).is_empty());
    }

    #[test]
    fn lint_fix_opens_a_run() {
        let (mut h, mut t) = (host(), Tracker::default());
        let _ = t.call(&mut h, "lint_design", &json!({"fix": true}), T0);
        assert_eq!(t.open_run().map(|r| r.calls), Some(1));
    }

    #[test]
    fn create_page_is_recorded_without_version() {
        let (mut h, mut t) = (host(), Tracker::default());
        t.call(&mut h, "create_page", &json!({}), T0).unwrap();
        let run = t.open_run().unwrap();
        assert_eq!(run.pages.len(), 1);
        assert_eq!(run.pages[0].page_id, h.ed.pages[1].id);
        assert_eq!(run.pages[0].version, None);
    }

    #[test]
    fn project_switch_ends_the_run_once_with_outgoing_counts() {
        let (mut h, mut t) = (host(), Tracker::default());
        write(&mut t, &mut h, T0);
        let old_dir = dir_of(&h);
        let (other, _) = h.session.store.create("Other").unwrap();
        t.call(
            &mut h,
            "open_file",
            &json!({"fileId": other.manifest.id}),
            T0 + 1,
        )
        .unwrap();
        assert!(t.open_run().is_none());
        let finished = t.take_finished().unwrap();
        assert_eq!((finished.number, finished.added), (1, 1));
        assert_eq!(list(&old_dir), vec![finished]);
        t.tick(T0 + 2, Some(&dir_of(&h)), &h.ed);
        assert!(t.take_finished().is_none());
    }

    #[test]
    fn user_switch_counts_outgoing_editor_and_drops_noop_runs() {
        for undone in [false, true] {
            let (mut h, mut t) = (host(), Tracker::default());
            write(&mut t, &mut h, T0);
            if undone {
                h.ed.undo();
            }
            let old = dir_of(&h);
            let counts = t.counts(&h.ed);
            let (other, _) = h.session.store.create("Other").unwrap();
            h.session.open(&mut h.ed, &other.dir).unwrap();
            t.end_counted(T0 + 1, counts, None);
            t.tick(T0 + 2, Some(&other.dir), &h.ed);
            if undone {
                assert!(t.take_finished().is_none());
                assert!(list(&old).is_empty());
            } else {
                let r = t.take_finished().unwrap();
                assert_eq!((r.added, r.changed, r.removed), (1, 0, 0));
                assert_eq!(list(&old), vec![r]);
            }
            assert!(t.take_finished().is_none());
        }
    }

    #[test]
    fn create_and_failed_or_same_project_open_keep_run() {
        let (mut h, mut t) = (host(), Tracker::default());
        write(&mut t, &mut h, T0);
        let dir = dir_of(&h);
        t.call(&mut h, "create_file", &json!({"name": "Other"}), T0 + 1)
            .unwrap();
        assert!(t.open_run().is_some());
        assert!(
            t.call(&mut h, "open_file", &json!({"fileId": "missing"}), T0 + 2)
                .is_err()
        );
        assert!(t.open_run().is_some());
        t.call(&mut h, "open_file", &json!({"fileId": dir}), T0 + 3)
            .unwrap();
        assert!(t.open_run().is_some());
        assert!(t.take_finished().is_none());
    }

    #[test]
    fn write_then_undo_drops_the_run_and_its_version() {
        let (mut h, mut t) = (host(), Tracker::default());
        write(&mut t, &mut h, T0);
        t.call(&mut h, "undo", &json!({}), T0 + 1).unwrap();
        t.tick(T0 + GAP, Some(&dir_of(&h)), &h.ed);
        assert!(t.take_finished().is_none());
        let dir = dir_of(&h);
        assert!(list(&dir).is_empty());
        assert!(crate::versions::list(&dir, &h.ed.pages[0].id).is_empty());
    }

    #[test]
    fn running_record_on_disk_becomes_review() {
        let (mut h, mut t) = (host(), Tracker::default());
        let dir = dir_of(&h);
        save(&dir, &run(1)).unwrap();
        let _ = t.call(&mut h, "get_basic_info", &json!({}), T0);
        assert_eq!(list(&dir)[0].state, RunState::Review);
    }
}
