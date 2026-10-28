//! The open project: loading it into the editor, autosave and thumbnails.
//!
//! This layer has no windowing code so it can be driven by tests with a fake clock.

use crate::editor::Editor;
use crate::project::{FormatError, PageRef, Project, Store, now_ms};
use crate::render::gpu::Headless;
use std::path::Path;
use std::time::Duration;
use web_time::Instant;

/// Save this long after the first unsaved change (bounded latency, even while editing).
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(1000);
/// Re-render the home-screen thumbnail at most this often while editing.
pub const THUMBNAIL_INTERVAL: Duration = Duration::from_secs(20);
/// How often the shell looks at what the sync engines downloaded.
const SYNC_CHECK: Duration = Duration::from_secs(1);
pub const THUMB_SIZE: (f64, f64) = (464.0, 296.0);

pub struct Session {
    pub store: Store,
    pub project: Option<Project>,
    /// Shared projects kept in step with a relay.
    pub sync: crate::sync::Sync,
    sync_checked: Option<Instant>,
    dirty_since: Option<Instant>,
    /// `design.json` as last read or written (`None` = no file yet).
    design_disk: Option<crate::variables::DesignSystem>,
    thumb_stale: bool,
    last_thumb: Option<Instant>,
}

impl Session {
    pub fn new(store: Store) -> Session {
        let mut sync = crate::sync::Sync::default();
        sync.start_all(&store.root);
        Session {
            store,
            project: None,
            sync,
            sync_checked: None,
            dirty_since: None,
            design_disk: None,
            thumb_stale: false,
            last_thumb: None,
        }
    }

    pub fn project_name(&self) -> String {
        self.project
            .as_ref()
            .map(|p| p.manifest.name.clone())
            .unwrap_or_default()
    }

    /// Open a project folder into the editor.
    pub fn open(&mut self, ed: &mut Editor, dir: &Path) -> Result<(), FormatError> {
        let p = Project::open(dir)?;
        crate::assets::load_dir(&dir.join(crate::project::ASSETS_DIR));
        let loaded = p.load_first(p.manifest.active_page.as_deref())?;
        ed.open_pages(loaded.pages, p.manifest.active_page.as_deref());
        ed.design = loaded.design;
        crate::libraries::sweep_orphans(dir, &ed.design.libraries);
        ed.library_docs = crate::libraries::load_docs(dir);
        ed.project_dir = Some(dir.to_path_buf());
        for (slot, modes) in ed.pages.iter_mut().zip(loaded.modes) {
            slot.modes = modes;
        }
        // The groups of the opened project (an old project has none).
        ed.page_groups = p.manifest.page_groups.clone();
        ed.sync_page_group_order();
        // Freshly loaded pages are saved by definition.
        for i in 0..ed.pages.len() {
            ed.mark_page_saved(i);
        }
        // Pages migrated on load (cache rebuilt, component keys, still plain JSON) are written
        // by the next save.
        for &i in &loaded.changed {
            ed.pages[i].saved_gen = u64::MAX;
        }
        for i in 0..ed.pages.len() {
            if !loaded.pending.contains(&i) && crate::project::is_legacy_page(dir, &ed.pages[i].id) {
                ed.pages[i].saved_gen = u64::MAX;
            }
        }
        // The other pages are read in the background; the editor installs them as they arrive.
        ed.load_pages_in_background(dir, &loaded.pending);
        self.design_disk = (!loaded.design_new).then(|| ed.design.clone());
        ed.meta_dirty = loaded.design_new && !ed.design.collections.is_empty();
        self.project = Some(p);
        self.dirty_since = None;
        self.thumb_stale = false;
        self.last_thumb = None;
        Ok(())
    }

    /// Create a new project and open it.
    pub fn create(&mut self, ed: &mut Editor, name: &str) -> Result<(), FormatError> {
        let (p, _) = self.store.create(name)?;
        self.open(ed, &p.dir)
    }

    /// Note that time passed; returns true when a save is due now.
    pub fn autosave_due(&mut self, ed: &Editor, now: Instant) -> bool {
        if self.project.is_none() || !ed.project_dirty() {
            self.dirty_since = None;
            return false;
        }
        let since = *self.dirty_since.get_or_insert(now);
        now.duration_since(since) >= AUTOSAVE_DELAY && !ed.is_busy()
    }

    /// When the sync engines should next be looked at (None while nothing is shared).
    pub fn sync_deadline(&self) -> Option<Instant> {
        self.sync.is_active().then(|| {
            self.sync_checked
                .map_or_else(Instant::now, |t| t + SYNC_CHECK)
        })
    }

    /// Look at the sync engines: the open project is reloaded when they downloaded changes,
    /// unless it holds unsaved edits (those win and go up on the next save). True when a check
    /// ran, so the shell can refresh the home list and the sync status shown there.
    pub fn sync_poll(&mut self, ed: &mut Editor, now: Instant) -> bool {
        if self.sync_deadline().is_none_or(|d| now < d) {
            return false;
        }
        self.sync_checked = Some(now);
        if self.sync.library_active() {
            // The library task writes `.share` files for projects it joined or shared.
            self.sync.start_all(&self.store.root);
        }
        let open = self.project.as_ref().map(|p| p.dir.clone());
        for dir in self.sync.dirs() {
            let is_open = open.as_ref() == Some(&dir);
            // A project the server dropped waits until it is closed.
            if !is_open && let Some(gone) = self.sync.status(&dir).and_then(|s| s.gone()) {
                self.sync.unshare(&dir);
                if gone == crate::sync::Gone::Deleted {
                    self.store.trash(&dir).ok();
                }
                continue;
            }
            if is_open && ed.project_dirty() {
                continue;
            }
            if self.sync.status(&dir).is_some_and(|s| s.take_changed()) {
                if is_open && let Err(e) = self.open(ed, &dir) {
                    log::warn!("reload shared project: {e}");
                }
            }
        }
        true
    }

    /// Put the project in `dir` on the relay at `server`.
    pub fn share_project(&mut self, dir: &Path, server: &str) -> Result<(), String> {
        self.sync.share(dir, server).map(|_| ())
    }

    /// Open the shared project behind `link` as a project of its own; its files arrive with the
    /// first sync.
    pub fn join_shared(&mut self, link: &str) -> Result<(), String> {
        let token = crate::sync::Share::from_link(link)?.token;
        let dir = self.store.root.join(format!("shared-{}", &token[..8]));
        if crate::sync::Share::load(&dir).is_some() {
            return Ok(());
        }
        self.sync.join(&dir, link)
    }

    /// The shared projects for the home screen: their status and links.
    pub fn shared_info(
        &self,
    ) -> std::collections::HashMap<std::path::PathBuf, crate::ui::home::ShareInfo> {
        self.sync
            .dirs()
            .into_iter()
            .filter_map(|dir| {
                let s = self.sync.status(&dir)?;
                let info = crate::ui::home::ShareInfo {
                    status: s.label(),
                    link: s.link.clone(),
                };
                Some((dir, info))
            })
            .collect()
    }

    /// Keep a library with the relay at `server`, if it runs one (see `Sync::enable_library`).
    pub fn follow_library(&mut self, server: &str) {
        let root = self.store.root.clone();
        self.sync.enable_library(&root, server);
    }

    /// The server of any shared project here: the likely answer for the next "Share…".
    pub fn server_hint(&self) -> String {
        self.sync
            .dirs()
            .iter()
            .find_map(|d| crate::sync::Share::load(d))
            .map(|s| s.server)
            .unwrap_or_default()
    }

    /// Retry later (after a failed save) instead of spinning on an overdue deadline.
    pub fn defer(&mut self) {
        self.dirty_since = Some(Instant::now());
    }

    /// When the next autosave check should happen (for the event loop's wake-up timer).
    pub fn next_deadline(&self) -> Option<Instant> {
        self.dirty_since.map(|t| t + AUTOSAVE_DELAY)
    }

    /// Write dirty pages and the manifest. Returns how many page files were written.
    pub fn save(&mut self, ed: &mut Editor) -> Result<usize, FormatError> {
        let Some(p) = self.project.as_mut() else {
            return Ok(0);
        };
        ed.commit_pending();
        let dirty = ed.dirty_pages();
        // design.json goes first: a crash after it leaves pages whose cache is stale, which load
        // rebuilds from the design; the other order would leave page fields newer than the truth.
        if self.design_disk.as_ref() != Some(&ed.design) {
            if self.design_disk.is_none() {
                // First design.json: the manifest turns v2 before it exists, so an old app
                // refuses the project rather than editing pages behind the design's back.
                p.manifest.version = crate::project::format::PROJECT_VERSION;
                p.write_manifest()?;
            }
            p.save_design(&ed.design)?;
            self.design_disk = Some(ed.design.clone());
        }
        // The manifest's version goes first too: an app older than packed pages must refuse the
        // project before it can find a page missing and save it empty.
        if !dirty.is_empty() {
            p.write_manifest()?;
        }
        for &i in &dirty {
            let data = ed.page_data(i);
            // Images first, so a page never references a file that isn't there.
            crate::assets::save_to_dir(
                &p.dir.join(crate::project::ASSETS_DIR),
                crate::assets::used_by(&data.doc),
            )?;
            p.save_page_modes(&data, &ed.pages[i].modes)?;
            ed.mark_page_saved(i);
        }
        let pages: Vec<PageRef> = ed
            .pages
            .iter()
            .map(|s| PageRef {
                id: s.id.clone(),
                name: s.name.clone(),
            })
            .collect();
        // Remove files of deleted pages.
        for old in &p.manifest.pages {
            if !pages.iter().any(|n| n.id == old.id) {
                p.delete_page_file(&old.id);
            }
        }
        let wrote = !dirty.is_empty() || ed.meta_dirty;
        p.manifest.pages = pages;
        p.manifest.page_groups = ed.page_groups.clone();
        p.manifest.active_page = Some(ed.active_page_id().to_string());
        if wrote {
            p.manifest.modified = now_ms();
            self.thumb_stale = true;
        }
        p.write_manifest()?;
        ed.meta_dirty = false;
        self.dirty_since = None;
        Ok(dirty.len())
    }

    /// Save the camera too (on close / leaving the project), plus everything dirty.
    /// Rename the open project: saved first, then its manifest and folder follow the new name.
    pub fn rename_open(&mut self, ed: &mut Editor, name: &str) -> Result<std::path::PathBuf, String> {
        self.save_all(ed).map_err(|e| format!("could not save the open project: {e}"))?;
        let p = self.project.as_mut().ok_or("no project is open")?;
        let dir = self.store.rename(&p.dir, name).map_err(|e| e.to_string())?;
        p.dir = dir.clone();
        p.manifest.name = name.trim().to_owned();
        Ok(dir)
    }

    pub fn save_all(&mut self, ed: &mut Editor) -> Result<(), FormatError> {
        self.save(ed)?;
        if let Some(p) = &self.project {
            // The camera isn't tracked by history; persist the active page's view.
            p.save_page_modes(&ed.page_data(ed.active_page), &ed.pages[ed.active_page].modes)?;
        }
        Ok(())
    }

    /// Whether the thumbnail should be refreshed now.
    pub fn thumbnail_due(&self, now: Instant, force: bool) -> bool {
        self.project.is_some()
            && self.thumb_stale
            && (force
                || self
                    .last_thumb
                    .is_none_or(|t| now.duration_since(t) >= THUMBNAIL_INTERVAL))
    }

    /// Render the first page into `thumbnail.png`.
    pub fn write_thumbnail(&mut self, gpu: &Headless, ed: &Editor) -> Result<(), String> {
        let Some(p) = &self.project else {
            return Ok(());
        };
        // The thumbnail shows the first page: wait until it is read, or it would come out empty.
        if ed.pages[0].pending {
            return Ok(());
        }
        self.thumb_stale = false;
        self.last_thumb = Some(Instant::now());
        let doc = ed.page_doc(0);
        let path = p.thumbnail_path();
        match render_thumbnail(gpu, doc)? {
            Some(png) => crate::io::atomic::write(&path, &png).map_err(|e| e.to_string()),
            None => {
                let _ = crate::vfs::remove_file(&path);
                Ok(())
            }
        }
    }

    /// Mark the thumbnail as taken and hand back what has to be rendered (the browser renders it
    /// asynchronously and then calls [`finish_thumbnail`]).
    pub fn start_thumbnail(&mut self, ed: &Editor) -> Option<ThumbJob> {
        let p = self.project.as_ref()?;
        if ed.pages[0].pending {
            return None;
        }
        self.thumb_stale = false;
        self.last_thumb = Some(Instant::now());
        Some(ThumbJob {
            path: p.thumbnail_path(),
            doc: ed.page_doc(0).clone(),
        })
    }

    /// Close the project: final save; the caller then shows the home screen.
    pub fn close(&mut self, ed: &mut Editor) -> Result<(), FormatError> {
        let r = self.save_all(ed);
        self.project = None;
        self.dirty_since = None;
        r
    }
}

/// A thumbnail waiting to be rendered: the first page as it was when the job started.
pub struct ThumbJob {
    pub path: std::path::PathBuf,
    pub doc: crate::model::Document,
}

/// Scale that fits the document's bounds into the thumbnail box, None when it's empty.
fn thumbnail_scale(doc: &crate::model::Document) -> Option<f64> {
    let b = crate::io::svg::export_bounds(doc, &doc.root)?;
    Some((THUMB_SIZE.0 / b.w).min(THUMB_SIZE.1 / b.h).min(2.0))
}

/// [`render_thumbnail`] without blocking on the GPU (browser).
pub async fn render_thumbnail_async(
    gpu: &Headless,
    doc: &crate::model::Document,
) -> Result<Option<Vec<u8>>, String> {
    let Some(scale) = thumbnail_scale(doc) else {
        return Ok(None);
    };
    let img = crate::io::png::render_async(gpu, doc, None, scale, None).await?;
    crate::io::export::encode_png(&img).map(Some)
}

/// Store a rendered thumbnail (None: the page is empty, so remove any old one).
pub fn finish_thumbnail(path: &std::path::Path, png: Option<Vec<u8>>) -> Result<(), String> {
    match png {
        Some(png) => crate::io::atomic::write(path, &png).map_err(|e| e.to_string()),
        None => {
            let _ = crate::vfs::remove_file(path);
            Ok(())
        }
    }
}

/// PNG bytes of a document fitted into the thumbnail box, or None when it's empty.
pub fn render_thumbnail(
    gpu: &Headless,
    doc: &crate::model::Document,
) -> Result<Option<Vec<u8>>, String> {
    let Some(b) = crate::io::svg::export_bounds(doc, &doc.root) else {
        return Ok(None);
    };
    let scale = (THUMB_SIZE.0 / b.w).min(THUMB_SIZE.1 / b.h).min(2.0);
    let img = crate::io::png::render(gpu, doc, None, scale, None)?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(Some(out.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Node, NodeId, NodeKind};

    fn store() -> Store {
        Store::open(
            std::env::temp_dir().join(format!("papercraft-session-{:016x}", fastrand::u64(..))),
        )
        .unwrap()
    }

    fn add_rect(ed: &mut Editor, x: f64) -> NodeId {
        ed.add_node(
            Node::new(NodeId(0), NodeKind::Rect, Rect::new(x, 0.0, 10.0, 10.0)),
            None,
        )
    }

    #[test]
    fn autosave_after_delay_and_reload() {
        let s = store();
        let mut sess = Session::new(s.clone());
        let mut ed = Editor::default();
        sess.create(&mut ed, "Auto").unwrap();
        let t0 = Instant::now();
        assert!(!sess.autosave_due(&ed, t0));
        add_rect(&mut ed, 5.0);
        assert!(!sess.autosave_due(&ed, t0), "not before the delay");
        assert!(sess.autosave_due(&ed, t0 + AUTOSAVE_DELAY));
        assert_eq!(sess.save(&mut ed).unwrap(), 1);
        assert!(!ed.project_dirty());
        assert!(!sess.autosave_due(&ed, t0 + AUTOSAVE_DELAY * 3));

        // Reload from disk in a fresh editor.
        let dir = sess.project.as_ref().unwrap().dir.clone();
        let mut ed2 = Editor::default();
        Session::new(s.clone()).open(&mut ed2, &dir).unwrap();
        assert_eq!(ed2.doc.nodes, ed.doc.nodes);
        assert!(!ed2.project_dirty());
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn pages_persist_with_names_order_and_deletions() {
        let s = store();
        let mut sess = Session::new(s.clone());
        let mut ed = Editor::default();
        sess.create(&mut ed, "Pages").unwrap();
        add_rect(&mut ed, 1.0);
        ed.add_page();
        add_rect(&mut ed, 2.0);
        add_rect(&mut ed, 3.0);
        ed.rename_page(1, "Wireframes");
        ed.add_page();
        ed.move_page(2, 0);
        sess.save(&mut ed).unwrap();
        let removed = ed.pages[0].id.clone();
        ed.delete_page(0);
        sess.save(&mut ed).unwrap();
        let p = Project::open(&sess.project.as_ref().unwrap().dir).unwrap();
        assert!(!p.page_path(&removed).exists());
        let names: Vec<_> = p.manifest.pages.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Page 1", "Wireframes"]);
        let pages = p.load_pages().unwrap();
        assert_eq!(pages[0].doc.nodes.len(), 1);
        assert_eq!(pages[1].doc.nodes.len(), 2);
        assert_eq!(
            p.manifest.active_page.as_deref(),
            Some(pages[0].id.as_str())
        );
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    const V1_PAGE: &str = r##"{"format":"paper-rs-page","version":3,"id":"pg1","name":"Page 1",
        "next_id":3,"root":[1,2],"nodes":[
        {"id":1,"name":"Main","type":"frame","rect":{"x":0,"y":0,"w":5,"h":5},"component":true},
        {"id":2,"name":"Plain","type":"rect","rect":{"x":0,"y":0,"w":5,"h":5}}],
        "variables":{"bg":"#000000"},
        "modes":{"Light":{"bg":"#ffffff"},"Dark":{"bg":"#000000"}},
        "mode":"Dark","tokens":{"space-md":"16px","font":"Inter"}}"##;

    fn v1_project(root: &Store, page: &str) -> std::path::PathBuf {
        let dir = root.root.join("Old");
        crate::vfs::create_dir_all(dir.join("pages")).unwrap();
        let mut m = crate::project::Manifest::new("Old");
        m.pages[0].id = "pg1".into();
        m.active_page = Some("pg1".into());
        m.version = 1;
        crate::vfs::write(dir.join("project.json"), m.to_json()).unwrap();
        crate::vfs::write(dir.join("pages/pg1.json"), page).unwrap();
        dir
    }

    #[test]
    fn v1_project_migrates_to_design_json() {
        let s = store();
        let dir = v1_project(&s, V1_PAGE);
        let orig = crate::project::format::page_from_json(V1_PAGE).unwrap().doc;
        let mut sess = Session::new(s.clone());
        let mut ed = Editor::default();
        sess.open(&mut ed, &dir).unwrap();
        assert!(!dir.join("design.json").exists(), "written on next save");
        let cols = &ed.design.collections;
        assert_eq!(cols[0].name, "Colors");
        assert_eq!(cols[0].modes.len(), 2);
        let dark = cols[0].modes.iter().find(|m| m.name == "Dark").unwrap();
        assert_eq!(cols[0].default_mode, dark.id);
        assert_eq!(cols[1].name, "Tokens");
        let doc = &ed.doc;
        assert_eq!(doc.variables, orig.variables);
        assert_eq!(doc.modes, orig.modes);
        assert_eq!(doc.mode, orig.mode);
        assert_eq!(doc.tokens, orig.tokens);
        let key = doc.get(NodeId(1)).unwrap().component_key.clone().unwrap();
        assert!(doc.get(NodeId(2)).unwrap().component_key.is_none());
        assert!(ed.project_dirty());

        sess.save(&mut ed).unwrap();
        assert!(dir.join("design.json").is_file());
        let design = ed.design.clone();
        let mut sess2 = Session::new(s.clone());
        let mut ed2 = Editor::default();
        sess2.open(&mut ed2, &dir).unwrap();
        assert_eq!(ed2.design, design);
        assert_eq!(ed2.pages[0].modes, ed.pages[0].modes);
        assert_eq!(ed2.doc.get(NodeId(1)).unwrap().component_key, Some(key));
        assert_eq!(ed2.doc.variables, orig.variables);
        assert_eq!(ed2.doc.mode, orig.mode);
        assert!(!ed2.project_dirty());
        let raw = crate::vfs::read_to_string(dir.join("project.json")).unwrap();
        // The manifest is rewritten at the current version so older apps refuse the project
        // instead of writing pages without bindings or missing the packed page files.
        assert!(raw.contains("\"version\": 3"));
        // The page file itself still carries the cache fields.
        let old = crate::project::format::page_from_json(
            &crate::project::packed::read(&crate::project::page_file(&dir, "pg1")).unwrap(),
        )
        .unwrap();
        assert_eq!(old.doc.variables, orig.variables);
        assert_eq!(old.doc.modes, orig.modes);
        assert_eq!(old.doc.tokens, orig.tokens);
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn design_json_is_written_before_pages() {
        let s = store();
        let dir = v1_project(&s, V1_PAGE);
        let mut sess = Session::new(s.clone());
        let mut ed = Editor::default();
        sess.open(&mut ed, &dir).unwrap();
        let red = crate::model::Color::rgb(255, 0, 0);
        ed.edit_design("Add", |ds| {
            crate::editor::design::set_color(ds, &Default::default(), "brand", None, red)
        })
        .unwrap();
        // Page writes fail: `pages` is now a file.
        crate::vfs::remove_dir_all(dir.join("pages")).unwrap();
        crate::vfs::write(dir.join("pages"), "x").unwrap();
        assert!(sess.save(&mut ed).is_err());
        let on_disk = Project::open(&dir).unwrap().load_design().unwrap().unwrap();
        assert!(on_disk.find_by_css("brand").is_some(), "design saved before the page failed");
        // The manifest turned v2 with it.
        let raw = crate::vfs::read_to_string(dir.join("project.json")).unwrap();
        assert!(raw.contains("\"version\": 3"));
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn project_without_variables_gets_empty_design() {
        let s = store();
        let dir = v1_project(
            &s,
            r#"{"format":"paper-rs-page","version":3,"id":"pg1","name":"P","next_id":1,"root":[],"nodes":[]}"#,
        );
        let mut sess = Session::new(s.clone());
        let mut ed = Editor::default();
        sess.open(&mut ed, &dir).unwrap();
        assert!(ed.design.collections.is_empty());
        // Dirty only to pack the plain page file.
        assert!(ed.project_dirty());
        sess.save(&mut ed).unwrap();
        assert!(dir.join("design.json").is_file());
        assert!(!crate::project::is_legacy_page(&dir, "pg1"));
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn migration_unions_diverged_pages() {
        let s = store();
        let dir = v1_project(&s, V1_PAGE);
        let mut m = Project::open(&dir).unwrap().manifest;
        m.pages.push(crate::project::PageRef { id: "pg2".into(), name: "Two".into() });
        crate::vfs::write(dir.join("project.json"), m.to_json()).unwrap();
        crate::vfs::write(
            dir.join("pages/pg2.json"),
            r##"{"format":"paper-rs-page","version":3,"id":"pg2","name":"Two","next_id":1,"root":[],"nodes":[],
            "variables":{"bg":"#123456","accent":"#ff0000"},
            "modes":{"Light":{"bg":"#abcdef","accent":"#ff0000"}},
            "tokens":{"space-md":"99px","radius":"4px"}}"##,
        )
        .unwrap();
        let mut sess = Session::new(s.clone());
        let mut ed = Editor::default();
        sess.open(&mut ed, &dir).unwrap();
        let (c, t) = (&ed.design.collections[0], &ed.design.collections[1]);
        let var = |n: &str| c.variables.iter().find(|v| v.name == n).unwrap();
        let mode = |n: &str| c.modes.iter().find(|m| m.name == n).unwrap().id.clone();
        use crate::variables::Value;
        assert_eq!(var("bg").values[&mode("Light")], Value::Color(crate::model::Color::rgb(255, 255, 255)));
        assert!(var("accent").values.contains_key(&mode("Dark")));
        let tok = |n: &str| t.variables.iter().find(|v| v.name == n).map(|v| v.values.values().next().cloned());
        assert_eq!(tok("space-md"), Some(Some(Value::Number(16.0))));
        assert!(tok("radius").is_some());
        assert!(ed.project_dirty());
        crate::vfs::remove_dir_all(&s.root).ok();
    }
}
