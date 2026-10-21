//! Design libraries: a project publishes its variables and components as `library.json`; other
//! projects enable a snapshot (`libraries/<id>.json`) and use them read-only.
//!
//! v1 limits: an instance inside a published component that points at another component
//! (another page, or outside the component) is published as a plain copy. Node ids in the
//! snapshot are derived from `(component key, original id)`, so they are unique across pages
//! and stay the same when the library is republished.

use crate::editor::Editor;
use crate::model::{Document, Node, NodeId, Source};
use crate::project::{Project, Store};
use crate::variables::{self, Collection, EnabledLibrary, PageModes, Value, stable_id};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const LIBRARY_FILE: &str = "library.json";
pub const LIBRARIES_DIR: &str = "libraries";
/// Longest side of a component preview, in px.
const PREVIEW_PX: f64 = 256.0;
/// Node ids stay below 2^53 so they survive a JSON round trip in any reader.
const ID_MASK: u64 = (1 << 53) - 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Published {
    pub library_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: u32,
    /// Unix ms.
    pub published: u64,
    pub collections: Vec<Collection>,
    pub components: Vec<PublishedComponent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishedComponent {
    pub key: String,
    pub name: String,
    pub page_name: String,
    /// The main's subtree, parent first.
    pub nodes: Vec<Node>,
    pub root: NodeId,
    /// 256px PNG, base64. `None` when published without a GPU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_png: Option<String>,
}

/// An available library, as listed by [`available`].
#[derive(Debug, Clone, PartialEq)]
pub struct LibrarySummary {
    pub library_id: String,
    pub name: String,
    pub description: String,
    pub version: u32,
    pub published: u64,
    pub variables: usize,
    pub components: usize,
    /// The library's project folder.
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PublishDiff {
    pub variables_added: Vec<String>,
    pub variables_changed: Vec<String>,
    pub variables_removed: Vec<String>,
    pub components_added: Vec<String>,
    pub components_changed: Vec<String>,
    pub components_removed: Vec<String>,
}

impl PublishDiff {
    pub fn is_empty(&self) -> bool {
        *self == PublishDiff::default()
    }
}

impl std::fmt::Display for PublishDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            return write!(f, "no changes");
        }
        let parts = [
            ("variable", "added", self.variables_added.len()),
            ("variable", "changed", self.variables_changed.len()),
            ("variable", "removed", self.variables_removed.len()),
            ("component", "added", self.components_added.len()),
            ("component", "changed", self.components_changed.len()),
            ("component", "removed", self.components_removed.len()),
        ];
        let s: Vec<String> = parts
            .iter()
            .filter(|p| p.2 > 0)
            .map(|(what, how, n)| format!("{n} {what}{} {how}", if *n == 1 { "" } else { "s" }))
            .collect();
        write!(f, "{}", s.join(", "))
    }
}

// ------------------------------------------------------------------------------ files

fn read_published(path: &Path) -> Result<Published, String> {
    let s = crate::vfs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, v: &T) -> Result<(), String> {
    if let Some(d) = path.parent() {
        crate::vfs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string(v).map_err(|e| e.to_string())?;
    crate::io::atomic::write(path, s.as_bytes()).map_err(|e| e.to_string())
}

fn previous(project_dir: &Path) -> Option<Published> {
    read_published(&project_dir.join(LIBRARY_FILE)).ok()
}

impl Published {
    /// The snapshot's components as one document (roots in publish order), what `Library(id)`
    /// instances sync from.
    pub fn to_document(&self) -> Document {
        let mut doc = Document {
            name: self.name.clone(),
            ..Document::default()
        };
        for c in &self.components {
            doc.root.push(c.root);
            for n in &c.nodes {
                doc.nodes.insert(n.id, n.clone());
            }
        }
        doc.next_id = doc.nodes.keys().map(|i| i.0).max().unwrap_or(0) + 1;
        doc
    }
}

/// Every `libraries/*.json` of a project as documents by library id. Unreadable files are
/// skipped (the next enable or update rewrites them).
pub fn load_docs(project_dir: &Path) -> BTreeMap<String, Document> {
    let mut out = BTreeMap::new();
    let Ok(rd) = crate::vfs::read_dir(project_dir.join(LIBRARIES_DIR)) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        match read_published(&p) {
            Ok(l) => {
                out.insert(l.library_id.clone(), l.to_document());
            }
            Err(e) => log::warn!("library snapshot skipped: {e}"),
        }
    }
    out
}

/// Documents of the enabled libraries only (a snapshot file may outlive a disable, for undo).
pub fn load_enabled_docs(project_dir: &Path, enabled: &[EnabledLibrary]) -> BTreeMap<String, Document> {
    let mut docs = load_docs(project_dir);
    docs.retain(|id, _| enabled.iter().any(|l| l.library_id == *id));
    docs
}

/// Delete snapshot files no enabled library references (left by disable so undo could bring
/// the library back).
pub fn sweep_orphans(project_dir: &Path, enabled: &[EnabledLibrary]) {
    let Ok(rd) = crate::vfs::read_dir(project_dir.join(LIBRARIES_DIR)) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if p.extension().is_some_and(|x| x == "json") && !enabled.iter().any(|l| l.library_id == stem) {
            let _ = crate::vfs::remove_file(&p);
        }
    }
}

// ------------------------------------------------------------------------------ build

fn snapshot_id(key: &str, id: NodeId) -> NodeId {
    let h = stable_id(&[key, &id.0.to_string()]);
    NodeId((u64::from_str_radix(&h, 16).unwrap_or(1) & ID_MASK).max(1))
}

fn published_collections(ed: &Editor, library_id: &str) -> Vec<Collection> {
    let ds = &ed.design;
    let kept: BTreeSet<&str> = ds
        .collections
        .iter()
        .filter(|c| c.library.is_none())
        .flat_map(|c| &c.variables)
        .filter(|v| !v.hidden)
        .map(|v| v.id.as_str())
        .collect();
    let var_id = |id: &str| stable_id(&[library_id, "var", id]);
    let mut out = Vec::new();
    for c in ds.collections.iter().filter(|c| c.library.is_none()) {
        let mut c2 = c.clone();
        c2.library = None;
        c2.id = stable_id(&[library_id, "col", &c.id]);
        c2.variables.retain(|v| !v.hidden);
        for v in &mut c2.variables {
            for (mode, val) in &mut v.values {
                if let Value::Alias(t) = val {
                    if kept.contains(t.as_str()) {
                        *t = var_id(t);
                    } else {
                        // Target is hidden or from another library: publish what it resolves to.
                        let modes: PageModes = [(c.id.clone(), mode.clone())].into();
                        *val = ds.resolve(&v.id, &modes).unwrap_or_else(|_| v.kind.default_value());
                    }
                }
            }
            v.id = var_id(&v.id);
        }
        if !c2.variables.is_empty() {
            out.push(c2);
        }
    }
    out
}

fn component_nodes(doc: &Document, root: NodeId, key: &str) -> Vec<Node> {
    let ids = doc.subtree(root);
    let inside: BTreeSet<NodeId> = ids.iter().copied().collect();
    ids.iter()
        .filter_map(|i| doc.get(*i))
        .map(|n| {
            let mut n = n.clone();
            n.id = snapshot_id(key, n.id);
            n.parent = n.parent.filter(|p| inside.contains(p)).map(|p| snapshot_id(key, p));
            n.children = n.children.iter().map(|c| snapshot_id(key, *c)).collect();
            n.link = None;
            n.comments.clear();
            match &mut n.instance {
                Some(l) if l.source.is_none() && inside.contains(&l.main) => l.main = snapshot_id(key, l.main),
                _ => n.instance = None,
            }
            if let crate::model::NodeKind::Connector { start, end, .. } = &mut n.kind {
                for b in [start, end] {
                    if let Some(t) = b.as_mut().filter(|t| inside.contains(&t.node)) {
                        t.node = snapshot_id(key, t.node);
                    } else {
                        *b = None;
                    }
                }
            }
            n
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn preview(gpu: &crate::render::gpu::Headless, doc: &Document, root: NodeId) -> Option<String> {
    let r = doc.world_rect(root)?;
    let scale = (PREVIEW_PX / r.w.max(r.h).max(1.0)).min(2.0);
    let img = crate::io::png::render(gpu, doc, Some(&[root]), scale, None).ok()?;
    let png = crate::io::export::encode_png(&img).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(png))
}

fn build(
    project_dir: &Path,
    ed: &Editor,
    description: &str,
    gpu: Option<&crate::render::gpu::Headless>,
) -> Result<(Published, Option<Published>), String> {
    let manifest = Project::open(project_dir).map_err(|e| e.to_string())?.manifest;
    let prev = previous(project_dir);
    let mut components = Vec::new();
    let mut seen = BTreeSet::new();
    for i in 0..ed.pages.len() {
        let page_name = &ed.pages[i].name;
        if page_name.starts_with(['_', '.']) {
            continue;
        }
        let doc = ed.page_doc(i);
        let mut mains: Vec<&Node> = doc
            .nodes
            .values()
            .filter(|n| n.component && !n.name.starts_with(['_', '.']))
            .filter(|n| !doc.ancestors(n.id).iter().any(|a| doc.get(*a).is_some_and(|a| a.component)))
            .collect();
        mains.sort_by_key(|n| n.id);
        for m in mains {
            let Some(mut key) = m.component_key.clone() else { continue };
            // Duplicate keys (copies made before keys were refreshed): the later one is rekeyed.
            if !seen.insert(key.clone()) {
                key = stable_id(&[&key, "dup", &page_name.to_string(), &m.id.0.to_string()]);
                seen.insert(key.clone());
            }
            #[cfg(not(target_arch = "wasm32"))]
            let preview_png = gpu.and_then(|g| preview(g, doc, m.id));
            #[cfg(target_arch = "wasm32")]
            let preview_png = {
                let _ = gpu;
                None
            };
            components.push(PublishedComponent {
                name: m.name.clone(),
                page_name: page_name.clone(),
                nodes: component_nodes(doc, m.id, &key),
                root: snapshot_id(&key, m.id),
                preview_png,
                key,
            });
        }
    }
    let description = if description.is_empty() {
        prev.as_ref().map(|p| p.description.clone()).unwrap_or_default()
    } else {
        description.to_string()
    };
    let next = Published {
        library_id: manifest.id.clone(),
        name: manifest.name,
        description,
        version: prev.as_ref().map_or(1, |p| p.version + 1),
        published: crate::project::now_ms(),
        collections: published_collections(ed, &manifest.id),
        components,
    };
    Ok((next, prev))
}

fn diff(prev: Option<&Published>, next: &Published) -> PublishDiff {
    let vars = |p: &Published| -> BTreeMap<String, Vec<(String, Value)>> {
        p.collections
            .iter()
            .flat_map(|c| &c.variables)
            .map(|v| {
                let mut vals: Vec<(String, Value)> =
                    v.values.iter().map(|(m, x)| (m.clone(), x.clone())).collect();
                vals.insert(0, (format!("{:?}", v.kind), Value::String(v.description.clone())));
                (v.name.clone(), vals)
            })
            .collect()
    };
    fn comps(p: &Published) -> BTreeMap<String, &PublishedComponent> {
        p.components.iter().map(|c| (c.key.clone(), c)).collect()
    }
    let empty = Published {
        library_id: String::new(),
        name: String::new(),
        description: String::new(),
        version: 0,
        published: 0,
        collections: Vec::new(),
        components: Vec::new(),
    };
    let prev = prev.unwrap_or(&empty);
    let mut d = PublishDiff::default();
    let (pv, nv) = (vars(prev), vars(next));
    for (k, v) in &nv {
        match pv.get(k) {
            None => d.variables_added.push(k.clone()),
            Some(o) if o != v => d.variables_changed.push(k.clone()),
            _ => {}
        }
    }
    d.variables_removed = pv.keys().filter(|k| !nv.contains_key(*k)).cloned().collect();
    let (pc, nc) = (comps(prev), comps(next));
    for (k, c) in &nc {
        match pc.get(k) {
            None => d.components_added.push(c.name.clone()),
            Some(o) if (&o.name, &o.nodes) != (&c.name, &c.nodes) => {
                d.components_changed.push(c.name.clone())
            }
            _ => {}
        }
    }
    d.components_removed = pc
        .iter()
        .filter(|(k, _)| !nc.contains_key(*k))
        .map(|(_, c)| c.name.clone())
        .collect();
    d
}

// ------------------------------------------------------------------------------ public API

/// Publish the open project as a library: `<project_dir>/library.json`, version = previous + 1.
/// No previews (they need a GPU); see [`publish_with_previews`].
pub fn publish(project_dir: &Path, ed: &Editor, description: &str) -> Result<Published, String> {
    publish_with_previews(project_dir, ed, description, None)
}

/// [`publish`] with 256px component previews rendered on `gpu` (skipped on wasm, where the
/// GPU read-back cannot block).
pub fn publish_with_previews(
    project_dir: &Path,
    ed: &Editor,
    description: &str,
    gpu: Option<&crate::render::gpu::Headless>,
) -> Result<Published, String> {
    let (next, _) = build(project_dir, ed, description, gpu)?;
    write_json(&project_dir.join(LIBRARY_FILE), &next)?;
    Ok(next)
}

/// What publishing now would change against the existing `library.json`.
pub fn publish_diff(project_dir: &Path, ed: &Editor) -> Result<PublishDiff, String> {
    let (next, prev) = build(project_dir, ed, "", None)?;
    Ok(diff(prev.as_ref(), &next))
}

/// Projects in the store that have a `library.json`, except `exclude` (the open project).
pub fn available(store: &Store, exclude: Option<&Path>) -> Vec<LibrarySummary> {
    #[derive(Deserialize)]
    struct Header {
        library_id: String,
        name: String,
        #[serde(default)]
        description: String,
        version: u32,
        published: u64,
        collections: Vec<Collection>,
        components: Vec<serde::de::IgnoredAny>,
    }
    store
        .list()
        .into_iter()
        .filter(|e| exclude != Some(e.dir.as_path()))
        .filter_map(|e| {
            let s = crate::vfs::read_to_string(e.dir.join(LIBRARY_FILE)).ok()?;
            let h: Header = serde_json::from_str(&s).ok()?;
            Some(LibrarySummary {
                variables: h.collections.iter().map(|c| c.variables.len()).sum(),
                components: h.components.len(),
                library_id: h.library_id,
                name: h.name,
                description: h.description,
                version: h.version,
                published: h.published,
                dir: e.dir,
            })
        })
        .collect()
}

/// Enable `library` in the project at `consumer_dir`: copy the snapshot to
/// `libraries/<id>.json`, add its collections read-only, record the version, make its
/// components placeable. Refused (nothing changes) when a CSS name is already taken.
pub fn enable(consumer_dir: &Path, ed: &mut Editor, library: &LibrarySummary) -> Result<(), String> {
    let snap = read_published(&library.dir.join(LIBRARY_FILE))?;
    let id = snap.library_id.clone();
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || id.is_empty() {
        return Err(format!("invalid library id {id:?}"));
    }
    let own = Project::open(consumer_dir).map_err(|e| e.to_string())?.manifest.id;
    if own == id {
        return Err("a project cannot use its own library".into());
    }
    if ed.design.libraries.iter().any(|l| l.library_id == id) {
        return Err(format!("{} is already enabled", snap.name));
    }
    let taken: BTreeSet<String> = ed.design.collections.iter().flat_map(|c| &c.variables).map(variables::css_name).collect();
    let clash: BTreeSet<String> = snap
        .collections
        .iter()
        .flat_map(|c| &c.variables)
        .map(variables::css_name)
        .filter(|n| taken.contains(n))
        .collect();
    if !clash.is_empty() {
        let names: Vec<String> = clash.iter().map(|n| format!("--{n}")).collect();
        return Err(format!(
            "cannot enable {}: CSS name(s) already defined: {}",
            snap.name,
            names.join(", ")
        ));
    }
    let file = consumer_dir.join(LIBRARIES_DIR).join(format!("{id}.json"));
    write_json(&file, &snap)?;
    let added = snap.collections.iter().cloned().map(|mut c| {
        c.library = Some(id.clone());
        c
    });
    let r = ed.edit_design("Enable library", |ds| {
        ds.collections.extend(added);
        ds.libraries.push(EnabledLibrary {
            library_id: id.clone(),
            version: u64::from(snap.version),
        });
        Ok(())
    });
    if let Err(e) = r {
        let _ = crate::vfs::remove_file(&file);
        return Err(e.to_string());
    }
    ed.library_docs.insert(id, snap.to_document());
    ed.meta_dirty = true;
    Ok(())
}

/// One enabled library against its source, as listed by [`status`].
#[derive(Debug, Clone, PartialEq)]
pub struct EnabledStatus {
    pub library_id: String,
    pub name: String,
    pub local_version: u32,
    /// `None`: the source project (or its `library.json`) is gone; the copy keeps working.
    pub source_version: Option<u32>,
    pub update_available: bool,
}

/// What [`disable`] detached.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisableReport {
    pub fills: usize,
    pub bindings: usize,
    pub instances: usize,
}

fn source(store: &Store, consumer_dir: &Path, library_id: &str) -> Option<LibrarySummary> {
    available(store, Some(consumer_dir)).into_iter().find(|l| l.library_id == library_id)
}

fn snapshot_path(consumer_dir: &Path, library_id: &str) -> PathBuf {
    consumer_dir.join(LIBRARIES_DIR).join(format!("{library_id}.json"))
}

/// Every enabled library of the open project against its source in the store.
pub fn status(store: &Store, consumer_dir: &Path, ed: &Editor) -> Vec<EnabledStatus> {
    ed.design
        .libraries
        .iter()
        .map(|l| {
            let src = source(store, consumer_dir, &l.library_id);
            let local = u32::try_from(l.version).unwrap_or(u32::MAX);
            EnabledStatus {
                library_id: l.library_id.clone(),
                name: ed.library_docs.get(&l.library_id).map_or_else(|| l.library_id.clone(), |d| d.name.clone()),
                local_version: local,
                source_version: src.as_ref().map(|s| s.version),
                update_available: src.is_some_and(|s| s.version > local),
            }
        })
        .collect()
}

/// What updating `library_id` would change: the consumer's copy against the source's current
/// `library.json`.
pub fn update_diff(store: &Store, consumer_dir: &Path, library_id: &str) -> Result<PublishDiff, String> {
    let src = source(store, consumer_dir, library_id).ok_or("library source not found")?;
    let local = read_published(&snapshot_path(consumer_dir, library_id))?;
    Ok(diff(Some(&local), &read_published(&src.dir.join(LIBRARY_FILE))?))
}

/// Replace the copy of `library_id` with the source's current publish: snapshot file, read-only
/// collections, version, and every `Library(id)` instance on every page (overrides kept), one undo
/// step per page. Refused with no change when the new collections break the design (CSS name
/// clash, a local alias to a removed variable).
pub fn update(store: &Store, consumer_dir: &Path, ed: &mut Editor, library_id: &str) -> Result<(), String> {
    if !ed.design.libraries.iter().any(|l| l.library_id == library_id) {
        return Err(format!("library {library_id} is not enabled"));
    }
    let src = source(store, consumer_dir, library_id).ok_or("library source not found")?;
    let snap = read_published(&src.dir.join(LIBRARY_FILE))?;
    let doc = snap.to_document();
    let id = library_id.to_string();
    let added = snap.collections.iter().cloned().map(|mut c| {
        c.library = Some(id.clone());
        c
    });
    let sync = |tx: &mut crate::history::Tx| {
        let roots: Vec<NodeId> = tx
            .doc()
            .nodes
            .values()
            .filter(|n| {
                n.instance.as_ref().is_some_and(|l| {
                    l.source == Some(Source::Library(id.clone())) && doc.get(l.main).is_some_and(|m| m.component)
                })
            })
            .map(|n| n.id)
            .collect();
        for r in roots {
            crate::editor::structure::sync_library_instance(tx, r, &doc);
        }
    };
    let path = snapshot_path(consumer_dir, library_id);
    let old_file = crate::vfs::read_to_string(&path).ok();
    write_json(&path, &snap)?;
    let r = ed.edit_design_each(
        "Update library",
        |ds| {
            ds.collections.retain(|c| c.library.as_deref() != Some(&id));
            ds.collections.extend(added);
            if let Some(l) = ds.libraries.iter_mut().find(|l| l.library_id == id) {
                l.version = u64::from(snap.version);
            }
            Ok(())
        },
        &sync,
    );
    if let Err(e) = r {
        match old_file {
            Some(s) => {
                let _ = crate::io::atomic::write(&path, s.as_bytes());
            }
            None => {
                let _ = crate::vfs::remove_file(&path);
            }
        }
        return Err(e.to_string());
    }
    ed.library_docs.insert(id, snap.to_document());
    ed.meta_dirty = true;
    Ok(())
}

/// Stop using `library_id`: bound fills and fields keep their current value and lose the binding,
/// local variables aliasing it keep the resolved value, its instances become plain copies; then
/// its collections and the design entry go (the snapshot file stays until the next open, for undo). One undo step per page.
pub fn disable(_consumer_dir: &Path, ed: &mut Editor, library_id: &str) -> Result<DisableReport, String> {
    if !ed.design.libraries.iter().any(|l| l.library_id == library_id) {
        return Err(format!("library {library_id} is not enabled"));
    }
    let own: BTreeSet<String> = ed
        .design
        .collections
        .iter()
        .filter(|c| c.library.as_deref() == Some(library_id))
        .flat_map(|c| &c.variables)
        .map(variables::css_name)
        .collect();
    let (fills, bindings, instances) = (Cell::new(0), Cell::new(0), Cell::new(0));
    let detach = |tx: &mut crate::history::Tx| {
        let ids: Vec<NodeId> = tx.doc().nodes.keys().copied().collect();
        for id in ids {
            let n = &tx.doc().nodes[&id];
            let lib = n.instance.as_ref().is_some_and(|l| l.source == Some(Source::Library(library_id.into())));
            let bound = n.bindings.values().any(|v| own.contains(v))
                || n.style.fills.iter().any(|p| p.var().is_some_and(|v| own.contains(v)));
            if !lib && !bound {
                continue;
            }
            let Some(n) = tx.node_mut(id) else { continue };
            if lib {
                n.instance = None;
                instances.set(instances.get() + 1);
            }
            for p in &mut n.style.fills {
                if p.var().is_some_and(|v| own.contains(v)) {
                    p.unlink_var();
                    fills.set(fills.get() + 1);
                }
            }
            let before = n.bindings.len();
            n.bindings.retain(|_, v| !own.contains(v));
            bindings.set(bindings.get() + before - n.bindings.len());
        }
    };
    ed.edit_design_each(
        "Disable library",
        |ds| {
            // Local variables aliasing the library keep what it resolved to, per mode.
            let mut literals = Vec::new();
            for c in ds.collections.iter().filter(|c| c.library.is_none()) {
                for v in &c.variables {
                    for (mode, x) in &v.values {
                        let Value::Alias(t) = x else { continue };
                        let from_lib = ds
                            .collections
                            .iter()
                            .any(|c| c.library.as_deref() == Some(library_id) && c.variables.iter().any(|v| v.id == *t));
                        if from_lib {
                            let modes: PageModes = [(c.id.clone(), mode.clone())].into();
                            literals.push((v.id.clone(), mode.clone(), ds.resolve(&v.id, &modes).unwrap_or_else(|_| v.kind.default_value())));
                        }
                    }
                }
            }
            for (vid, mode, lit) in literals {
                for v in ds.collections.iter_mut().flat_map(|c| &mut c.variables).filter(|v| v.id == vid) {
                    v.values.insert(mode.clone(), lit.clone());
                }
            }
            ds.collections.retain(|c| c.library.as_deref() != Some(library_id));
            ds.libraries.retain(|l| l.library_id != library_id);
            Ok(())
        },
        &detach,
    )
    .map_err(|e| e.to_string())?;
    // The snapshot file stays so undo can bring the library back; the next open sweeps it.
    ed.library_docs.remove(library_id);
    ed.meta_dirty = true;
    Ok(DisableReport { fills: fills.get(), bindings: bindings.get(), instances: instances.get() })
}

impl Editor {
    /// The enabled library holding a component with `key`: `(library id, root node in its snapshot)`.
    pub fn find_library_component(&self, key: &str) -> Option<(String, NodeId)> {
        self.library_docs.iter().find_map(|(lib, doc)| {
            doc.nodes
                .values()
                .find(|n| n.component && n.component_key.as_deref() == Some(key))
                .map(|n| (lib.clone(), n.id))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Rect, V2};
    use crate::model::{Binding, InstanceLink, NodeKind};

    #[test]
    fn component_nodes_remaps_inner_links_and_drops_outside_ones() {
        let frame = |id: u64| Node::new(NodeId(id), NodeKind::Frame { clip: false }, Rect::new(0., 0., 10., 10.));
        let mut doc = Document::default();
        let (mut root, mut inner, mut line) = (frame(1), frame(2), frame(3));
        let bind = |n: u64| Some(Binding { node: NodeId(n), fixed: V2::ZERO, inside: false });
        line.kind = NodeKind::Connector {
            points: vec![V2::ZERO, V2::ZERO],
            start: bind(2),
            end: bind(99),
            route: Default::default(),
            start_head: Default::default(),
            end_head: Default::default(),
        };
        inner.instance = Some(InstanceLink { main: NodeId(1), overrides: vec![], source: None });
        root.children = vec![NodeId(2), NodeId(3)];
        inner.parent = Some(NodeId(1));
        line.parent = Some(NodeId(1));
        for n in [root, inner, line] {
            doc.nodes.insert(n.id, n);
        }
        let out = component_nodes(&doc, NodeId(1), "k");
        let by = |id: u64| out.iter().find(|n| n.id == snapshot_id("k", NodeId(id))).unwrap();
        assert_eq!(by(2).instance.as_ref().unwrap().main, snapshot_id("k", NodeId(1)));
        let NodeKind::Connector { start, end, .. } = &by(3).kind else { panic!() };
        assert_eq!(start.unwrap().node, snapshot_id("k", NodeId(2)));
        assert!(end.is_none());
    }
}
