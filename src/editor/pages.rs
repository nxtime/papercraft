//! Multiple pages per project. The active page lives in `Editor::{doc, history, camera}`;
//! the others are parked in their [`PageSlot`] with their own history and camera.

use super::{Editor, Gesture};
use crate::geom::Camera;
use crate::history::History;
use crate::model::{Document, NodeId, Source};
use crate::project::{PageData, new_id};
use std::collections::HashMap;

#[derive(Debug)]
pub struct PageStash {
    pub doc: Document,
    pub history: History,
    pub camera: Camera,
    pub selection: Vec<NodeId>,
    /// The camera was never set (fit to content when first shown).
    pub needs_fit: bool,
}

#[derive(Debug)]
pub struct PageSlot {
    pub id: String,
    pub name: String,
    /// Parked state for inactive pages; `None` for the active one.
    pub stash: Option<Box<PageStash>>,
    /// History generation at the last save.
    pub saved_gen: u64,
    /// The project was opened without reading this page yet: `stash` holds an empty stand-in
    /// that is never saved. The loader installs the real page (`Editor::poll_loading`), or code
    /// that needs it now reads it first (`Editor::ensure_loaded`).
    pub pending: bool,
    /// Why a pending page could not be read. Such a page stays a stand-in: not shown, not saved,
    /// so the file on disk is left for the user to repair.
    pub failed: Option<String>,
    /// Active mode id per collection id on this page.
    pub modes: crate::variables::PageModes,
}

impl PageSlot {
    pub fn new(name: &str) -> PageSlot {
        PageSlot {
            id: new_id(),
            name: name.into(),
            stash: None,
            saved_gen: 0,
            pending: false,
            failed: None,
            modes: Default::default(),
        }
    }
}

impl Editor {
    /// Replace the content with a project's pages, keeping tool settings and the clipboard
    /// (so copy in one project and paste in another works).
    pub fn open_pages(&mut self, pages: Vec<PageData>, active: Option<&str>) {
        assert!(!pages.is_empty(), "a project has at least one page");
        let active_i = active
            .and_then(|a| pages.iter().position(|p| p.id == a))
            .unwrap_or(0);
        let clipboard = std::mem::take(&mut self.clipboard);
        let clipboard_roots = std::mem::take(&mut self.clipboard_roots);
        let clipboard_page = self.clipboard_page.take();
        let clipboard_world = std::mem::take(&mut self.clipboard_world);
        let first = pages[active_i].clone();
        let doc = Document {
            name: first.name.clone(),
            ..first.doc
        };
        self.load(doc, first.camera, None);
        self.clipboard = clipboard;
        self.clipboard_roots = clipboard_roots;
        self.clipboard_page = clipboard_page;
        self.clipboard_world = clipboard_world;
        self.pages = pages
            .into_iter()
            .enumerate()
            .map(|(i, p)| PageSlot {
                id: p.id,
                name: p.name,
                stash: (i != active_i).then(|| {
                    Box::new(PageStash {
                        needs_fit: p.camera.is_none(),
                        camera: p.camera.unwrap_or_default(),
                        doc: p.doc,
                        history: History::default(),
                        selection: Vec::new(),
                    })
                }),
                saved_gen: 0,
                pending: false,
                failed: None,
                modes: Default::default(),
            })
            .collect();
        self.active_page = active_i;
        self.meta_dirty = false;
        // The groups of the previous project are gone; the session sets the new ones.
        self.page_groups.clear();
    }

    pub fn active_page_id(&self) -> &str {
        &self.pages[self.active_page].id
    }

    pub fn page_name(&self) -> &str {
        &self.pages[self.active_page].name
    }

    /// Make page `i` active, parking the current one.
    pub fn switch_page(&mut self, i: usize) {
        if i == self.active_page || i >= self.pages.len() {
            return;
        }
        if !self.ensure_loaded(i) {
            return;
        }
        self.cancel_gesture();
        self.end_text_edit();
        self.commit();
        self.gesture = Gesture::None;
        let parked = PageStash {
            doc: std::mem::take(&mut self.doc),
            history: std::mem::take(&mut self.history),
            camera: self.camera,
            selection: std::mem::take(&mut self.selection),
            needs_fit: false,
        };
        self.pages[self.active_page].stash = Some(Box::new(parked));
        let s = self.pages[i]
            .stash
            .take()
            .expect("inactive page has a stash");
        self.doc = s.doc;
        self.history = s.history;
        self.camera = s.camera;
        self.selection = s.selection;
        self.active_page = i;
        self.hover = None;
        self.guides.clear();
        self.marquee = None;
        if s.needs_fit {
            self.zoom_to_fit();
            self.pending_fit = true;
        }
        self.meta_dirty = true; // the active page is remembered in the manifest
        self.revision += 1;
        self.doc_stamp = super::next_stamp();
    }

    fn unique_page_name(&self, base: &str) -> String {
        let mut n = base.to_string();
        let mut i = 2;
        while self.pages.iter().any(|p| p.name == n) {
            n = format!("{base} {i}");
            i += 1;
        }
        n
    }

    /// Add an empty page after the active one and switch to it.
    pub fn add_page(&mut self) -> usize {
        let at = self.active_page + 1;
        self.insert_empty_page(at, None);
        self.switch_page(at);
        self.status = format!("Added {}", self.pages[at].name);
        at
    }

    /// Append an empty page without leaving the current one (agents adding pages).
    pub fn add_page_in_background(&mut self, name: Option<&str>) -> usize {
        let at = self.pages.len();
        self.insert_empty_page(at, name);
        self.revision += 1;
        at
    }

    /// Re-create a deleted page with its original identity and name.
    pub(crate) fn add_page_with_id(&mut self, id: &str, snap: &Document) -> usize {
        let at = self.pages.len();
        self.pages.push(PageSlot {
            id: id.to_string(),
            name: snap.name.clone(),
            stash: Some(Box::new(PageStash {
                doc: Document {
                    name: snap.name.clone(),
                    next_id: snap.next_id,
                    ..Default::default()
                },
                history: History::default(),
                camera: Camera::default(),
                selection: Vec::new(),
                needs_fit: true,
            })),
            saved_gen: u64::MAX,
            pending: false,
            failed: None,
            modes: Default::default(),
        });
        self.meta_dirty = true;
        self.revision += 1;
        at
    }

    fn insert_empty_page(&mut self, at: usize, name: Option<&str>) {
        // A new page joins the group of the page that is active.
        let group = self.page_group(self.active_page_id()).map(str::to_string);
        let base = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(String::from)
            .unwrap_or_else(|| format!("Page {}", self.pages.len() + 1));
        let name = self.unique_page_name(&base);
        let mut slot = PageSlot::new(&name);
        // A new page starts with the active page's modes and the cache they give.
        slot.modes = self.pages[self.active_page].modes.clone();
        let mut doc = Document {
            name: name.clone(),
            ..Default::default()
        };
        if self.design.collections.is_empty() {
            doc.restore_vars(&self.doc.var_state());
        } else {
            let c = self.design.cache_for(&slot.modes);
            doc.variables = c.variables;
            doc.modes = c.modes;
            doc.mode = c.mode;
            doc.tokens = c.tokens;
        }
        slot.stash = Some(Box::new(PageStash {
            doc,
            history: History::default(),
            camera: Camera::default(),
            selection: Vec::new(),
            needs_fit: true,
        }));
        // Never saved: force the first autosave.
        slot.saved_gen = u64::MAX;
        let id = slot.id.clone();
        self.pages.insert(at, slot);
        if self.active_page >= at {
            self.active_page += 1;
        }
        self.meta_dirty = true;
        if let Some(g) = group {
            self.assign_page_group(&id, Some(&g));
        }
    }

    pub fn duplicate_page(&mut self, i: usize) -> usize {
        if i >= self.pages.len() || !self.ensure_loaded(i) {
            return self.active_page;
        }
        self.commit();
        let src = self.page_data(i);
        // A copy joins the group of the page it was made from.
        let group = self.page_group(&self.pages[i].id).map(str::to_string);
        let name = self.unique_page_name(&format!("{} copy", src.name));
        let mut slot = PageSlot::new(&name);
        slot.modes = self.pages[i].modes.clone();
        slot.saved_gen = u64::MAX;
        let mut doc = Document {
            name: name.clone(),
            ..src.doc
        };
        // A copied main is a new component, not a second source of the same key.
        for n in doc.nodes.values_mut().filter(|n| n.component) {
            n.component_key = Some(crate::variables::new_id());
        }
        slot.stash = Some(Box::new(PageStash {
            doc,
            history: History::default(),
            camera: src.camera.unwrap_or_default(),
            selection: Vec::new(),
            needs_fit: src.camera.is_none(),
        }));
        let id = slot.id.clone();
        self.pages.insert(i + 1, slot);
        if self.active_page > i {
            self.active_page += 1;
        }
        self.meta_dirty = true;
        if let Some(g) = group {
            self.assign_page_group(&id, Some(&g));
        }
        self.switch_page(i + 1);
        i + 1
    }

    pub fn rename_page(&mut self, i: usize, name: &str) {
        let name = name.trim();
        if name.is_empty() || i >= self.pages.len() || self.pages[i].name == name {
            return;
        }
        self.pages[i].name = name.to_string();
        if i == self.active_page {
            self.doc.name = name.to_string();
            self.doc_stamp = super::next_stamp();
        } else if let Some(s) = &mut self.pages[i].stash {
            s.doc.name = name.to_string();
        }
        self.meta_dirty = true;
        self.revision += 1;
    }

    /// Delete page `i` (never the last one). Returns the removed page id for file cleanup.
    pub fn delete_page(&mut self, i: usize) -> Option<String> {
        if self.pages.len() <= 1 || i >= self.pages.len() {
            return None;
        }
        if i == self.active_page {
            self.switch_page(if i + 1 < self.pages.len() {
                i + 1
            } else {
                i - 1
            });
        }
        let removed = self.pages.remove(i);
        // The page leaves its group; the group itself stays (it may be empty now).
        if let Some(group) = self.page_group(&removed.id).map(str::to_owned)
            && let Some(members) = self.page_groups.get_mut(&group)
        {
            members.retain(|id| id != &removed.id);
        }
        if self.active_page > i {
            self.active_page -= 1;
        }
        self.meta_dirty = true;
        self.revision += 1;
        self.status = format!("Deleted {}", removed.name);
        Some(removed.id)
    }

    /// Replaces the active page's content and variable values with a saved snapshot, as one
    /// undoable change (on the active page; the other pages' steps carry only their cache).
    ///
    /// The snapshot holds only the derived cache fields, so its values are written back into the
    /// project design system as a design edit: colors per mode and tokens that differ from what
    /// the page resolves now, and the snapshot's active mode. Every page follows. Variables the
    /// snapshot lacks are kept (other pages use them) and aliases that already give the snapshot's
    /// value stay aliases. If the design edit is refused (a name clash), only the content is
    /// restored and the status says so.
    pub fn restore_snapshot(&mut self, snap: &crate::model::Document) {
        use crate::editor::design::{OnActive, set_color, set_token_in};
        use crate::variables::Kind;
        let active = self.pages[self.active_page].modes.clone();
        let now = if self.design.collections.is_empty() {
            crate::variables::migrate_from_document(self.page_doc(0)).cache_for(&active)
        } else {
            self.design.cache_for(&active)
        };
        let design_values = |ds: &mut crate::variables::DesignSystem| {
            for (mode, vars) in &snap.modes {
                for (n, c) in vars {
                    if now.modes.get(mode).and_then(|m| m.get(n)) != Some(c) {
                        set_color(ds, &active, n, Some(mode), *c)?;
                    }
                }
            }
            if snap.modes.is_empty() {
                for (n, c) in &snap.variables {
                    if now.variables.get(n) != Some(c) {
                        set_color(ds, &active, n, None, *c)?;
                    }
                }
            }
            for (n, v) in &snap.tokens {
                let boolean = ds.find_by_css(n).is_some_and(|(_, x)| x.kind == Kind::Boolean);
                if !boolean && now.tokens.get(n) != Some(v) {
                    set_token_in(ds, &active, n, v, None)?;
                }
            }
            Ok(())
        };
        let on_active = OnActive {
            mode: snap.mode.clone(),
            apply: Box::new(|tx| replace_content(tx, snap)),
        };
        if let Err(e) = self.edit_design_with("Restore version", design_values, Some(on_active), None) {
            self.edit("Restore version", |tx| replace_content(tx, snap));
            self.status = format!("Restored the page; variables kept as they are ({e})");
        }
        self.selection.clear();
    }

    /// Move page `from` to position `to`.
    pub fn move_page(&mut self, from: usize, to: usize) {
        if from >= self.pages.len() || to >= self.pages.len() || from == to {
            return;
        }
        let active_id = self.active_page_id().to_string();
        let p = self.pages.remove(from);
        self.pages.insert(to, p);
        self.active_page = self.pages.iter().position(|p| p.id == active_id).unwrap();
        self.sync_page_group_order();
        self.meta_dirty = true;
        self.revision += 1;
    }

    fn page_gen(&self, i: usize) -> u64 {
        if i == self.active_page {
            self.history.generation()
        } else {
            self.pages[i]
                .stash
                .as_ref()
                .map_or(0, |s| s.history.generation())
        }
    }

    /// Pages whose content changed since their last save.
    pub fn dirty_pages(&self) -> Vec<usize> {
        (0..self.pages.len())
            .filter(|i| !self.pages[*i].pending && self.page_gen(*i) != self.pages[*i].saved_gen)
            .collect()
    }

    /// Anything (content, page list, names) needs saving.
    pub fn project_dirty(&self) -> bool {
        self.meta_dirty || !self.dirty_pages().is_empty()
    }

    pub fn mark_page_saved(&mut self, i: usize) {
        self.pages[i].saved_gen = self.page_gen(i);
        if i == self.active_page {
            self.mark_saved();
        }
    }

    /// Snapshot of page `i` for saving or export.
    pub fn page_data(&self, i: usize) -> PageData {
        let slot = &self.pages[i];
        let (doc, camera) = if i == self.active_page {
            (self.doc.clone(), Some(self.camera))
        } else {
            let s = slot.stash.as_ref().expect("inactive page has a stash");
            (s.doc.clone(), (!s.needs_fit).then_some(s.camera))
        };
        let camera = camera.filter(|_| !self.headless);
        PageData {
            id: slot.id.clone(),
            name: slot.name.clone(),
            doc: Document {
                name: slot.name.clone(),
                ..doc
            },
            camera,
        }
    }

    /// Run `f` with page `i` in the active slot (its document, undo history and selection)
    /// without switching the page the user sees. Edits made by `f` land in page `i`'s own
    /// history; the user's view, camera and dirty marker are left alone.
    pub fn with_page<R>(&mut self, i: usize, f: impl FnOnce(&mut Editor) -> R) -> R {
        if i == self.active_page || i >= self.pages.len() {
            return f(self);
        }
        self.ensure_loaded(i);
        self.commit();
        let mut s = self.pages[i]
            .stash
            .take()
            .expect("inactive page has a stash");
        let swap = |ed: &mut Editor, s: &mut PageStash| {
            std::mem::swap(&mut ed.doc, &mut s.doc);
            std::mem::swap(&mut ed.history, &mut s.history);
            std::mem::swap(&mut ed.selection, &mut s.selection);
        };
        let saved_marker = self.saved_marker;
        let outer = self.edit_page.replace(i);
        swap(self, &mut s);
        let r = f(self);
        self.commit();
        swap(self, &mut s);
        self.edit_page = outer;
        self.saved_marker = saved_marker;
        self.pages[i].stash = Some(s);
        self.revision += 1;
        self.doc_stamp = super::next_stamp();
        self.flush_cross_pages();
        r
    }

    /// Re-sync instances on other pages after a main component changed: each touched page
    /// gets its own undo step. Instances pick up their main from the source page's document.
    pub(super) fn flush_cross_pages(&mut self) {
        const MAX_ROUNDS: usize = 8; // mains that contain instances of each other
        if self.cross_busy || self.edit_page.is_some() {
            return;
        }
        // Instances on a page that is still being read must follow the change too.
        self.finish_loading();
        self.cross_busy = true;
        for _ in 0..MAX_ROUNDS {
            let Some((src, mains)) = (!self.cross_pending.is_empty())
                .then(|| self.cross_pending.remove(0))
            else {
                break;
            };
            let src_id = self.pages[src].id.clone();
            let src_doc = self.page_doc(src).clone();
            for page in (0..self.pages.len()).filter(|p| *p != src) {
                let roots: Vec<NodeId> = self
                    .page_doc(page)
                    .nodes
                    .values()
                    .filter(|n| {
                        n.instance.as_ref().is_some_and(|l| {
                            matches!(&l.source, Some(Source::Page(p)) if *p == src_id)
                                && mains.contains(&l.main)
                        })
                    })
                    .map(|n| n.id)
                    .collect();
                if roots.is_empty() {
                    continue;
                }
                self.with_page(page, |ed| {
                    ed.edit("Update instances", |tx| {
                        for r in roots {
                            super::structure::sync_node(tx, r, true, Some(&src_doc));
                        }
                    })
                });
            }
        }
        self.cross_pending.clear();
        self.cross_busy = false;
    }

    /// Hand over the pages `open_pages` got as stand-ins (`pending`: indexes into the page list)
    /// to a background reader for the project in `dir`.
    pub fn load_pages_in_background(&mut self, dir: &std::path::Path, pending: &[usize]) {
        if pending.is_empty() {
            return;
        }
        let refs = pending
            .iter()
            .map(|&i| crate::project::PageRef {
                id: self.pages[i].id.clone(),
                name: self.pages[i].name.clone(),
            })
            .collect();
        for &i in pending {
            self.pages[i].pending = true;
        }
        self.loader = Some(crate::project::loader::PageLoader::spawn(dir.to_path_buf(), refs));
    }

    /// Pages are still being read in the background.
    pub fn loading(&self) -> bool {
        self.pages.iter().any(|s| s.pending && s.failed.is_none())
    }

    /// Install the pages the background reader finished. Never blocks.
    pub fn poll_loading(&mut self) {
        while let Some(got) = self.loader.as_ref().and_then(|l| l.rx.try_recv().ok()) {
            self.install_page(&got.0, got.1);
        }
    }

    /// Wait for every page to be read. Call before anything that looks at or changes all pages
    /// (agent tools, design edits, cross-page syncing).
    pub fn finish_loading(&mut self) {
        while self.loading() {
            match self.loader.as_ref().and_then(|l| l.rx.recv().ok()) {
                Some(got) => self.install_page(&got.0, got.1),
                None => {
                    // The reader is gone: read what is left here.
                    for i in 0..self.pages.len() {
                        if self.pages[i].pending && self.pages[i].failed.is_none() {
                            self.read_page_now(i);
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Make sure page `i` is read. False when it could not be (the reason is in the status).
    pub fn ensure_loaded(&mut self, i: usize) -> bool {
        if !self.pages[i].pending {
            return true;
        }
        self.poll_loading();
        if self.pages[i].pending && self.pages[i].failed.is_none() {
            self.read_page_now(i);
        }
        !self.pages[i].pending
    }

    /// Read page `i` here, ahead of the background reader (whose copy is dropped on arrival).
    fn read_page_now(&mut self, i: usize) {
        let Some(dir) = self.loader.as_ref().map(|l| l.dir.clone()) else {
            self.pages[i].failed = Some("the project folder is unknown".into());
            return;
        };
        let r = crate::project::PageRef {
            id: self.pages[i].id.clone(),
            name: self.pages[i].name.clone(),
        };
        let path = dir.join(crate::project::PAGES_DIR).join(format!("{}.json", r.id));
        let read = crate::project::read_page_file(&path, &r);
        self.install_page(&r.id, read);
    }

    fn install_page(
        &mut self,
        id: &str,
        read: Result<crate::project::loader::ReadPage, crate::project::FormatError>,
    ) {
        let Some(i) = self.pages.iter().position(|s| s.id == id && s.pending) else {
            return;
        };
        let (mut p, mut modes) = match read {
            Ok(x) => x,
            Err(e) => {
                let msg = format!("Could not read page {}: {e}", self.pages[i].name);
                self.status.clone_from(&msg);
                self.pages[i].failed = Some(msg);
                return;
            }
        };
        let changed = crate::project::finalize_page(&self.design, &mut p, &mut modes);
        let slot = &mut self.pages[i];
        let Some(stash) = slot.stash.as_mut() else { return };
        // The manifest owns the name, and it may have been renamed meanwhile.
        p.doc.name.clone_from(&slot.name);
        stash.doc = p.doc;
        stash.camera = p.camera.unwrap_or_default();
        stash.needs_fit = p.camera.is_none();
        slot.modes = modes;
        slot.pending = false;
        // Read from disk is saved by definition, unless the load itself changed the page.
        slot.saved_gen = if changed { u64::MAX } else { stash.history.generation() };
        self.revision += 1;
    }

    /// The document of page `i` (active or parked).
    pub fn page_doc(&self, i: usize) -> &Document {
        if i == self.active_page {
            &self.doc
        } else {
            &self.pages[i].stash.as_ref().unwrap().doc
        }
    }
}

// ---------------------------------------------------------------------------------- groups

/// Page groups (folders). Groups are single level: a page belongs to at most one group,
/// and a group keeps its pages in the order of the page list.
impl Editor {
    /// Create an empty group. Fails on an empty or already used name.
    pub fn create_page_group(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.page_groups.contains_key(name) {
            return false;
        }
        self.page_groups.insert(name.to_string(), Vec::new());
        self.meta_dirty = true;
        self.revision += 1;
        true
    }

    /// Rename a group, keeping its pages. Fails when the old name is unknown, the new one
    /// is empty or already taken, or the name stays the same.
    pub fn rename_page_group(&mut self, old: &str, new: &str) -> bool {
        let old = old.trim();
        let new = new.trim();
        if old.is_empty() || new.is_empty() || old == new {
            return false;
        }
        if !self.page_groups.contains_key(old) || self.page_groups.contains_key(new) {
            return false;
        }
        let members = self.page_groups.remove(old).expect("group exists");
        self.page_groups.insert(new.to_string(), members);
        self.meta_dirty = true;
        self.revision += 1;
        true
    }

    /// Move a page into a group, or out of every group when `group` is `None`.
    /// Fails when the page or the group is unknown, or when nothing changes.
    pub fn assign_page_group(&mut self, page_id: &str, group: Option<&str>) -> bool {
        if !self.pages.iter().any(|p| p.id == page_id) {
            return false;
        }
        let current = self.page_group(page_id).map(str::to_owned);
        let Some(name) = group else {
            let Some(cur) = current else {
                return false;
            };
            self.page_groups
                .get_mut(&cur)
                .expect("group exists")
                .retain(|id| id != page_id);
            self.meta_dirty = true;
            self.revision += 1;
            return true;
        };
        let name = name.trim();
        if name.is_empty() || !self.page_groups.contains_key(name) {
            return false;
        }
        if current.as_deref() == Some(name) {
            return false;
        }
        if let Some(cur) = current {
            self.page_groups
                .get_mut(&cur)
                .expect("group exists")
                .retain(|id| id != page_id);
        }
        self.page_groups
            .get_mut(name)
            .expect("group exists")
            .push(page_id.to_string());
        self.sync_page_group_order();
        self.meta_dirty = true;
        self.revision += 1;
        true
    }

    /// Drop a group; its pages stay in the project, only without a group.
    pub fn remove_page_group(&mut self, name: &str) -> bool {
        let name = name.trim();
        if self.page_groups.remove(name).is_none() {
            return false;
        }
        self.meta_dirty = true;
        self.revision += 1;
        true
    }

    /// The group a page belongs to, if any.
    pub fn page_group(&self, page_id: &str) -> Option<&str> {
        self.page_groups
            .iter()
            .find(|(_, members)| members.iter().any(|id| id == page_id))
            .map(|(name, _)| name.as_str())
    }

    /// Drop page ids that no longer exist and order every group like the page list.
    pub fn sync_page_group_order(&mut self) {
        let order: HashMap<&str, usize> = self
            .pages
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.as_str(), i))
            .collect();
        for members in self.page_groups.values_mut() {
            members.retain(|id| order.contains_key(id.as_str()));
            members.sort_by_key(|id| order.get(id.as_str()).copied().unwrap_or(usize::MAX));
        }
    }
}

/// Swap the page content for the snapshot's nodes (the variables are handled by the caller).
fn replace_content(tx: &mut crate::history::Tx, snap: &Document) {
    for root in tx.doc().root.clone() {
        tx.remove(root);
    }
    tx.snapshot_annotations();
    tx.doc.annotations.clone_from(&snap.annotations);
    for root in snap.root.clone() {
        for (i, id) in snap.subtree(root).into_iter().enumerate() {
            let Some(node) = snap.get(id).cloned() else {
                continue;
            };
            if i == 0 {
                tx.insert(node, None, None);
            } else {
                tx.put(node);
            }
        }
    }
}

