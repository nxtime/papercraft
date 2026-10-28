//! Local-first projects.
//!
//! Every project is a folder under the store root (`~/Documents/paper-rs/` by default):
//!
//! ```text
//! <root>/<Project name>/
//!     project.json        manifest: id, name, timestamps, ordered page list
//!     design.json         the project's design system (collections of variables)
//!     pages/<page-id>.json.zst one infinite canvas per page (zstd JSON, see [`packed`])
//!     assets/<hash>.<ext> images used by image fills
//!     thumbnail.png       preview for the home screen (written by the app)
//! <root>/.trash/<Project name>~<timestamp>/   deleted projects, restorable
//! ```
//!
//! All writes go through [`crate::io::atomic`], so a crash never leaves a torn file.

pub mod format;
pub mod loader;
pub mod packed;

use crate::io::atomic;
pub use format::{FormatError, Manifest, PageData, PageRef, new_id, now_ms};
use crate::variables::{COLORS_COLLECTION, DesignSystem, PageModes};
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "project.json";
pub const DESIGN: &str = "design.json";
pub const PAGES_DIR: &str = "pages";
pub const THUMBNAIL: &str = "thumbnail.png";
pub const ASSETS_DIR: &str = "assets";
pub const TRASH_DIR: &str = ".trash";
const TRASH_INFO: &str = "trashinfo.json";

/// The folder holding all projects.
#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
}

/// A project as listed on the home screen.
#[derive(Debug, Clone)]
pub struct Entry {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub thumbnail: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct TrashEntry {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// Unix ms when it was deleted.
    pub deleted: u64,
    /// Folder name it had before deletion.
    pub original: String,
}

/// An open project on disk.
#[derive(Debug, Clone)]
pub struct Project {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

/// The first page's variables, modes and tokens plus whatever only later pages have (pages
/// can diverge: removals were page-only). The first page wins on a name conflict.
fn union_doc(pages: &[PageData]) -> crate::model::Document {
    let mut out = pages.first().map(|p| p.doc.clone()).unwrap_or_default();
    for p in pages.iter().skip(1) {
        for (k, c) in &p.doc.variables {
            out.variables.entry(k.clone()).or_insert(*c);
        }
        for (mode, vars) in &p.doc.modes {
            let m = out.modes.entry(mode.clone()).or_default();
            for (k, c) in vars {
                m.entry(k.clone()).or_insert(*c);
            }
        }
        for (k, v) in &p.doc.tokens {
            out.tokens.entry(k.clone()).or_insert(v.clone());
        }
    }
    out
}

/// What [`Project::load_all`] read.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub design: DesignSystem,
    pub pages: Vec<PageData>,
    /// Active mode per collection, per page (same order as `pages`).
    pub modes: Vec<PageModes>,
    /// Pages whose file differs from what was loaded (cache rebuilt, keys assigned).
    pub changed: Vec<usize>,
    /// `design.json` did not exist: it was built from the first page.
    pub design_new: bool,
    /// Pages not read yet (see [`Project::load_first`]): their entry in `pages` is an empty
    /// placeholder, read later by a [`loader::PageLoader`].
    pub pending: Vec<usize>,
}

/// File-system safe folder name for a project name.
pub fn folder_name(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let s = s.trim_start_matches('.').trim().to_string();
    if s.is_empty() { "Untitled".into() } else { s }
}

/// Where the app keeps its chat settings: `$PAPERCRAFT_HOME/.config` when set (so a second
/// copy for testing has its own), else `papercraft` in the OS config folder.
#[cfg(not(target_arch = "wasm32"))]
pub fn config_dir() -> Option<PathBuf> {
    match std::env::var_os("PAPERCRAFT_HOME") {
        Some(h) => Some(PathBuf::from(h).join(".config")),
        None => dirs::config_dir().map(|d| d.join("papercraft")),
    }
}

/// The web build has no config folder: settings keep their defaults.
#[cfg(target_arch = "wasm32")]
pub fn config_dir() -> Option<PathBuf> {
    None
}

impl Store {
    /// `$PAPERCRAFT_HOME`, else `~/Documents/papercraft` (`~/Documents/paper-rs` when only that exists).
    pub fn default_root() -> PathBuf {
        if let Some(p) = std::env::var_os("PAPERCRAFT_HOME") {
            return PathBuf::from(p);
        }
        let docs = dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        // Projects made before the rename (paper-rs) stay where they are.
        let legacy = docs.join("paper-rs");
        let root = docs.join("papercraft");
        if legacy.is_dir() && !root.exists() {
            legacy
        } else {
            root
        }
    }

    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<Store> {
        let root = root.into();
        crate::vfs::create_dir_all(&root)?;
        Ok(Store { root })
    }

    fn trash_root(&self) -> PathBuf {
        self.root.join(TRASH_DIR)
    }

    /// Projects, most recently modified first. Unreadable folders are skipped.
    pub fn list(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = crate::vfs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|e| {
                let dir = e.path();
                let manifest = read_manifest(&dir).ok()?;
                let thumb = dir.join(THUMBNAIL);
                Some(Entry {
                    thumbnail: crate::vfs::is_file(&thumb).then_some(thumb),
                    dir,
                    manifest,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            b.manifest
                .modified
                .cmp(&a.manifest.modified)
                .then_with(|| a.manifest.name.cmp(&b.manifest.name))
        });
        out
    }

    /// A folder path for `name` that doesn't exist yet ("Name", "Name 2", ...).
    fn unique_dir(&self, parent: &Path, name: &str) -> PathBuf {
        let base = folder_name(name);
        let mut p = parent.join(&base);
        let mut i = 2;
        while crate::vfs::exists(&p) {
            p = parent.join(format!("{base} {i}"));
            i += 1;
        }
        p
    }

    /// A display name not used by any listed project ("Untitled", "Untitled 2", ...).
    pub fn unique_name(&self, name: &str) -> String {
        let taken: Vec<String> = self.list().into_iter().map(|e| e.manifest.name).collect();
        let mut n = name.to_string();
        let mut i = 2;
        while taken.contains(&n) || crate::vfs::exists(&self.root.join(folder_name(&n))) {
            n = format!("{name} {i}");
            i += 1;
        }
        n
    }

    /// Create an empty project with one blank page.
    pub fn create(&self, name: &str) -> Result<(Project, PageData), FormatError> {
        let name = self.unique_name(name);
        let dir = self.unique_dir(&self.root, &name);
        crate::vfs::create_dir_all(dir.join(PAGES_DIR))?;
        let manifest = Manifest::new(&name);
        let page = PageData {
            id: manifest.pages[0].id.clone(),
            name: manifest.pages[0].name.clone(),
            doc: crate::model::Document {
                name: manifest.pages[0].name.clone(),
                ..Default::default()
            },
            camera: None,
        };
        let p = Project { dir, manifest };
        p.save_page(&page)?;
        p.write_manifest()?;
        Ok((p, page))
    }

    /// Create a project from existing pages (imports, legacy files).
    pub fn create_with_pages(
        &self,
        name: &str,
        pages: Vec<PageData>,
    ) -> Result<Project, FormatError> {
        let name = self.unique_name(name);
        let dir = self.unique_dir(&self.root, &name);
        crate::vfs::create_dir_all(dir.join(PAGES_DIR))?;
        let mut manifest = Manifest::new(&name);
        manifest.pages = pages
            .iter()
            .map(|p| PageRef {
                id: p.id.clone(),
                name: p.name.clone(),
            })
            .collect();
        manifest.active_page = manifest.pages.first().map(|p| p.id.clone());
        let p = Project { dir, manifest };
        for page in &pages {
            crate::assets::save_to_dir(&p.dir.join(ASSETS_DIR), crate::assets::used_by(&page.doc))?;
            p.save_page(page)?;
        }
        p.write_manifest()?;
        Ok(p)
    }

    /// Rename a project: updates the manifest and renames the folder to match.
    pub fn rename(&self, dir: &Path, new_name: &str) -> Result<PathBuf, FormatError> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(FormatError::Invalid("empty name".into()));
        }
        let mut p = Project::open(dir)?;
        p.manifest.name = new_name.to_string();
        p.manifest.modified = now_ms();
        p.write_manifest()?;
        let wanted = self.root.join(folder_name(new_name));
        if wanted == dir {
            return Ok(dir.to_path_buf());
        }
        let target = if crate::vfs::exists(&wanted) {
            self.unique_dir(&self.root, new_name)
        } else {
            wanted
        };
        crate::vfs::rename(dir, &target)?;
        Ok(target)
    }

    /// Copy a project (all pages) under a fresh id and "Name copy".
    pub fn duplicate(&self, dir: &Path) -> Result<PathBuf, FormatError> {
        let src = Project::open(dir)?;
        let name = self.unique_name(&format!("{} copy", src.manifest.name));
        let target = self.unique_dir(&self.root, &name);
        copy_dir(dir, &target)?;
        // A copy is its own project, not another editor of the shared one.
        crate::vfs::remove_file(target.join(crate::sync::SHARE_FILE)).ok();
        // Nor a published library: its `library.json` still carries the source's library id.
        crate::vfs::remove_file(target.join(crate::libraries::LIBRARY_FILE)).ok();
        let mut p = Project::open(&target)?;
        p.manifest.id = new_id();
        p.manifest.name = name;
        p.manifest.created = now_ms();
        p.manifest.modified = p.manifest.created;
        p.write_manifest()?;
        Ok(target)
    }

    /// Move a project to the trash folder. Returns its path in the trash.
    pub fn trash(&self, dir: &Path) -> Result<PathBuf, FormatError> {
        let original = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        crate::vfs::create_dir_all(self.trash_root())?;
        let deleted = now_ms();
        let target = self
            .trash_root()
            .join(format!("{}~{deleted}", folder_name(&original)));
        crate::vfs::rename(dir, &target)?;
        let info = serde_json::json!({ "deleted": deleted, "original": original });
        atomic::write(&target.join(TRASH_INFO), info.to_string().as_bytes())?;
        Ok(target)
    }

    pub fn list_trash(&self) -> Vec<TrashEntry> {
        let mut out: Vec<TrashEntry> = crate::vfs::read_dir(self.trash_root())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let dir = e.path();
                let manifest = read_manifest(&dir).ok()?;
                let info: serde_json::Value = crate::vfs::read_to_string(dir.join(TRASH_INFO))
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                Some(TrashEntry {
                    deleted: info["deleted"].as_u64().unwrap_or(0),
                    original: info["original"]
                        .as_str()
                        .unwrap_or(&manifest.name)
                        .to_string(),
                    dir,
                    manifest,
                })
            })
            .collect();
        out.sort_by_key(|e| std::cmp::Reverse(e.deleted));
        out
    }

    /// Put a trashed project back (under a unique folder name if the old one is taken).
    pub fn restore(&self, trash_dir: &Path) -> Result<PathBuf, FormatError> {
        let entry = self
            .list_trash()
            .into_iter()
            .find(|e| e.dir == trash_dir)
            .ok_or_else(|| FormatError::Invalid("not in trash".into()))?;
        let target = self.unique_dir(&self.root, &entry.original);
        crate::vfs::rename(trash_dir, &target)?;
        let _ = crate::vfs::remove_file(target.join(TRASH_INFO));
        Ok(target)
    }

    pub fn delete_forever(&self, trash_dir: &Path) -> std::io::Result<()> {
        if trash_dir.parent() != Some(self.trash_root().as_path()) {
            return Err(std::io::Error::other(
                "refusing to delete outside the trash",
            ));
        }
        crate::vfs::remove_dir_all(trash_dir)
    }

    pub fn empty_trash(&self) -> std::io::Result<()> {
        for e in self.list_trash() {
            self.delete_forever(&e.dir)?;
        }
        Ok(())
    }
}

/// An empty stand-in for a page that is read later.
fn placeholder_page(r: &PageRef) -> PageData {
    PageData {
        id: r.id.clone(),
        name: r.name.clone(),
        doc: crate::model::Document { name: r.name.clone(), ..Default::default() },
        camera: None,
    }
}

/// Read the page file `path` of manifest entry `r`; the manifest owns the name, and a missing
/// file is an empty page.
/// The file of page `id` in project folder `dir`.
pub fn page_file(dir: &Path, id: &str) -> PathBuf {
    packed::path(&dir.join(PAGES_DIR), id)
}

/// Page `id` is still a plain `.json` from an older build; saving it packs it.
pub fn is_legacy_page(dir: &Path, id: &str) -> bool {
    let f = page_file(dir, id);
    !crate::vfs::exists(&f) && crate::vfs::exists(&packed::legacy(&f))
}

pub fn read_page_file(path: &Path, r: &PageRef) -> Result<(PageData, PageModes), FormatError> {
    let (mut p, m) = match packed::read(path)
        .map_err(FormatError::from)
        .and_then(|s| format::page_from_json_modes(&s))
    {
        Ok(x) => x,
        Err(FormatError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            (placeholder_page(r), PageModes::new())
        }
        Err(e) => return Err(e),
    };
    // The manifest owns page names (renames touch only the manifest).
    p.name = r.name.clone();
    p.doc.name = r.name.clone();
    Ok((p, m))
}

/// Give a freshly read page the variable cache `design` derives for its modes and every main
/// component a key. True when that changed the page, so the next save must write it.
pub fn finalize_page(design: &DesignSystem, p: &mut PageData, m: &mut PageModes) -> bool {
    // A page without per-collection modes keeps the mode its old `mode` field names.
    if m.is_empty()
        && let Some(name) = &p.doc.mode
        && let Some(c) = design.collections.iter().find(|c| c.name == COLORS_COLLECTION)
        && let Some(mode) = c.modes.iter().find(|x| &x.name == name)
    {
        m.insert(c.id.clone(), mode.id.clone());
    }
    let cache = design.cache_for(m);
    let mut dirty = p.doc.variables != cache.variables
        || p.doc.modes != cache.modes
        || p.doc.mode != cache.mode
        || p.doc.tokens != cache.tokens;
    p.doc.variables = cache.variables;
    p.doc.modes = cache.modes;
    p.doc.mode = cache.mode;
    p.doc.tokens = cache.tokens;
    for n in p.doc.nodes.values_mut() {
        if n.component && n.component_key.is_none() {
            // From the page and node ids, so every device agrees on the key.
            n.component_key = Some(format!("{}:{}", p.id, n.id.0));
            dirty = true;
        }
    }
    dirty
}

fn read_manifest(dir: &Path) -> Result<Manifest, FormatError> {
    Manifest::from_json(&crate::vfs::read_to_string(dir.join(MANIFEST))?)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    crate::vfs::create_dir_all(to)?;
    for e in crate::vfs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            crate::vfs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

impl Project {
    pub fn open(dir: &Path) -> Result<Project, FormatError> {
        let manifest = read_manifest(dir)?;
        atomic::clean_stale_temps(dir);
        atomic::clean_stale_temps(&dir.join(PAGES_DIR));
        Ok(Project {
            dir: dir.to_path_buf(),
            manifest,
        })
    }

    pub fn page_path(&self, id: &str) -> PathBuf {
        page_file(&self.dir, id)
    }

    pub fn load_page(&self, id: &str) -> Result<PageData, FormatError> {
        let mut p = format::page_from_json(&packed::read(&self.page_path(id))?)?;
        if let Some(r) = self.manifest.pages.iter().find(|r| r.id == id) {
            // The manifest owns page names (renames touch only the manifest).
            p.name = r.name.clone();
            p.doc.name = r.name.clone();
        }
        Ok(p)
    }

    /// Every page, with its cache rebuilt from the design system.
    pub fn load_pages(&self) -> Result<Vec<PageData>, FormatError> {
        Ok(self.load_all()?.pages)
    }

    pub fn design_path(&self) -> PathBuf {
        self.dir.join(DESIGN)
    }

    /// `design.json`, or `None` when the project has none yet.
    pub fn load_design(&self) -> Result<Option<DesignSystem>, FormatError> {
        match crate::vfs::read_to_string(self.design_path()) {
            Ok(s) => Ok(Some(serde_json::from_str(&s).map_err(FormatError::Json)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_design(&self, design: &DesignSystem) -> Result<(), FormatError> {
        let s = serde_json::to_string_pretty(design).expect("design system serializes");
        atomic::write(&self.design_path(), s.as_bytes())?;
        Ok(())
    }

    /// `design.json`, or `None` when it is missing. A damaged one must not lock the project:
    /// the bad file is kept for recovery and the design is rebuilt from the page caches (which
    /// hold every value). The next save rewrites it.
    fn read_design(&self) -> Result<Option<DesignSystem>, FormatError> {
        match self.load_design() {
            Ok(d) => Ok(d),
            Err(FormatError::Json(e)) => {
                let backup = self.dir.join(format!("{DESIGN}.broken-{}", now_ms()));
                log::warn!("{DESIGN} is malformed ({e}); saved as {backup:?}, rebuilding");
                crate::vfs::rename(self.design_path(), backup)?;
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// One page of the manifest read from disk: a missing file comes back empty (e.g. after a
    /// crash between creating the page and its first save). Nothing is derived from the design
    /// yet, see [`finalize_page`].
    pub fn read_page(&self, r: &PageRef) -> Result<(PageData, PageModes), FormatError> {
        read_page_file(&self.page_path(&r.id), r)
    }

    /// Load the design system and every page listed in the manifest. A page whose file is
    /// missing comes back empty (e.g. after a crash between creating it and its first save).
    /// A project without `design.json` gets one built from the first page; pages get the
    /// derived variable cache and every main component a key. Nothing is written here:
    /// `Loaded::changed` and `Loaded::design_new` say what the next save must write.
    pub fn load_all(&self) -> Result<Loaded, FormatError> {
        self.load_pages_with(self.read_design()?, None)
    }

    /// Like [`Self::load_all`], but reads only the design system and the page named `active`
    /// (the first when it is unknown); the other pages come back as empty placeholders listed in
    /// `Loaded::pending`, for a [`loader::PageLoader`] to read in the background. A project
    /// without `design.json` needs every page to build it, and the browser has no threads, so
    /// both load everything here.
    pub fn load_first(&self, active: Option<&str>) -> Result<Loaded, FormatError> {
        let first = active
            .and_then(|a| self.manifest.pages.iter().position(|r| r.id == a))
            .unwrap_or(0);
        self.load_pages_with(self.read_design()?, Some(first))
    }

    fn load_pages_with(
        &self,
        stored: Option<DesignSystem>,
        first: Option<usize>,
    ) -> Result<Loaded, FormatError> {
        let lazy = first.filter(|_| stored.is_some() && !cfg!(target_arch = "wasm32"));
        let mut pages = Vec::new();
        let mut modes = Vec::new();
        let mut pending = Vec::new();
        for (i, r) in self.manifest.pages.iter().enumerate() {
            if lazy.is_some_and(|f| f != i) {
                pages.push(placeholder_page(r));
                modes.push(PageModes::new());
                pending.push(i);
                continue;
            }
            let (p, m) = self.read_page(r)?;
            pages.push(p);
            modes.push(m);
        }
        let design_new = stored.is_none();
        let design = stored.unwrap_or_else(|| {
            crate::variables::migrate_from_document(&union_doc(&pages))
        });
        let mut changed = Vec::new();
        for (i, (p, m)) in pages.iter_mut().zip(&mut modes).enumerate() {
            if !pending.contains(&i) && finalize_page(&design, p, m) {
                changed.push(i);
            }
        }
        Ok(Loaded {
            design,
            pages,
            modes,
            changed,
            design_new,
            pending,
        })
    }

    pub fn save_page(&self, page: &PageData) -> Result<(), FormatError> {
        self.save_page_modes(page, &PageModes::new())
    }

    pub fn save_page_modes(&self, page: &PageData, modes: &PageModes) -> Result<(), FormatError> {
        crate::vfs::create_dir_all(self.dir.join(PAGES_DIR))?;
        packed::write(&self.page_path(&page.id), &format::page_to_json_modes(page, modes))?;
        Ok(())
    }

    pub fn delete_page_file(&self, id: &str) {
        packed::remove(&self.page_path(id));
    }

    pub fn write_manifest(&self) -> Result<(), FormatError> {
        atomic::write(&self.dir.join(MANIFEST), self.manifest.to_json().as_bytes())?;
        Ok(())
    }

    pub fn thumbnail_path(&self) -> PathBuf {
        self.dir.join(THUMBNAIL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn temp_store() -> Store {
        let root =
            std::env::temp_dir().join(format!("papercraft-store-{:016x}", fastrand::u64(..)));
        Store::open(root).unwrap()
    }

    #[test]
    fn create_list_rename_duplicate_trash_restore() {
        let s = temp_store();
        let (a, page) = s.create("Alpha").unwrap();
        assert!(a.dir.join(MANIFEST).is_file());
        assert!(a.page_path(&page.id).is_file());
        std::thread::sleep(std::time::Duration::from_millis(5));
        let (b, _) = s.create("Alpha").unwrap();
        assert_eq!(b.manifest.name, "Alpha 2");
        let names: Vec<String> = s.list().into_iter().map(|e| e.manifest.name).collect();
        assert_eq!(names, ["Alpha 2", "Alpha"], "most recent first");

        let renamed = s.rename(&a.dir, "Beta: plan").unwrap();
        assert_eq!(renamed.file_name().unwrap(), "Beta- plan");
        assert_eq!(Project::open(&renamed).unwrap().manifest.name, "Beta: plan");

        let dup = s.duplicate(&renamed).unwrap();
        let d = Project::open(&dup).unwrap();
        assert_eq!(d.manifest.name, "Beta: plan copy");
        assert_ne!(d.manifest.id, Project::open(&renamed).unwrap().manifest.id);
        assert_eq!(d.load_pages().unwrap().len(), 1);

        let t = s.trash(&renamed).unwrap();
        assert!(!renamed.exists());
        assert_eq!(s.list().len(), 2);
        let trash = s.list_trash();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].manifest.name, "Beta: plan");
        let back = s.restore(&t).unwrap();
        assert_eq!(back, renamed);
        assert_eq!(s.list().len(), 3);

        s.trash(&dup).unwrap();
        s.empty_trash().unwrap();
        assert!(s.list_trash().is_empty());
        assert!(s.delete_forever(&s.root.join("Alpha")).is_err());
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn pages_roundtrip_and_missing_page_is_empty() {
        let s = temp_store();
        let (mut p, mut page) = s.create("P").unwrap();
        page.doc.name = "Page 1".into();
        let id = page.doc.alloc_id();
        let n = crate::model::Node::new(
            id,
            crate::model::NodeKind::Rect,
            crate::geom::Rect::new(1.0, 2.0, 3.0, 4.0),
        );
        page.doc.nodes.insert(id, n);
        page.doc.root.push(id);
        p.save_page(&page).unwrap();
        p.manifest.pages.push(PageRef {
            id: "ghost".into(),
            name: "Ghost".into(),
        });
        p.write_manifest().unwrap();
        let p = Project::open(&p.dir).unwrap();
        let pages = p.load_pages().unwrap();
        assert_eq!(pages[0], page);
        assert!(pages[1].doc.nodes.is_empty());
        assert_eq!(pages[1].name, "Ghost");
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn malformed_design_json_is_backed_up_and_rebuilt_from_pages() {
        let s = temp_store();
        let (p, mut page) = s.create("P").unwrap();
        page.doc.variables.insert("bg".into(), crate::model::Color::rgb(1, 2, 3));
        p.save_page(&page).unwrap();
        crate::vfs::write(p.design_path(), "{ not json").unwrap();
        let loaded = p.load_all().unwrap();
        assert!(loaded.design_new, "rebuilt design is written by the next save");
        assert!(loaded.design.find_by_css("bg").is_some());
        let backups: Vec<_> = std::fs::read_dir(&p.dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("design.json.broken-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read_to_string(backups[0].path()).unwrap(), "{ not json");
        assert!(!p.design_path().exists());
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn migrated_component_keys_are_stable_across_loads() {
        let s = temp_store();
        let (p, mut page) = s.create("P").unwrap();
        let id = page.doc.alloc_id();
        let mut n = crate::model::Node::new(id, crate::model::NodeKind::Frame { clip: false }, crate::geom::Rect::new(0., 0., 5., 5.));
        n.component = true;
        page.doc.nodes.insert(id, n);
        page.doc.root.push(id);
        p.save_page(&page).unwrap();
        let key = |l: &Loaded| l.pages[0].doc.get(id).unwrap().component_key.clone();
        let (a, b) = (p.load_all().unwrap(), p.load_all().unwrap());
        assert_eq!(key(&a), key(&b));
        assert_eq!(key(&a), Some(format!("{}:{}", page.id, id.0)));
        crate::vfs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn folder_names_are_safe() {
        assert_eq!(folder_name("a/b\\c"), "a-b-c");
        assert_eq!(folder_name("  ..hidden "), "hidden");
        assert_eq!(folder_name(""), "Untitled");
    }
}
