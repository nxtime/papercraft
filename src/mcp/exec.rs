//! Runs MCP tool calls against the editor, on the thread that owns it.
//!
//! Node ids are strings `"<page>:<n>"`: the first six hex digits of the page id and the
//! node's number in that page's document (numbers are only unique per page). A bare number
//! means the page from `pageId`, else the page the user is viewing. Writes run as one
//! transaction each (`Editor::edit`), so they undo in one step and refresh the window;
//! writes to other pages go to that page's own document and history (`Editor::with_page`).

use super::css::{self, Context, Styles};
use super::html::{self, Element, Html};
use super::{Out, ToolResult, tools};
use crate::editor::{Editor, design};
use crate::geom::{MAX_ZOOM, MIN_ZOOM, Rect, V2, v2};
use crate::history::Tx;
use crate::model::annotation::AnnotationCategory;
use crate::model::{
    AutoLayout, Binding, Color, Direction, Document, Head, ImageMode, Node, NodeId, NodeKind,
    InstanceLink, Paint, PathPoint, Route, Sizing, Source, SubPath, Swap, TextAlign,
};
use crate::project::{Project, Store};
use crate::variables::{self, DesignSystem, Kind, Value as DsVal};
use crate::render::gpu::Headless;
use crate::session::Session;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// What a tool call needs from its surroundings.
pub trait Host {
    fn editor(&mut self) -> &mut Editor;
    fn store(&self) -> &Store;
    /// The project shown in the app (None for a scratch document or the home screen).
    fn project(&self) -> Option<&Project>;
    /// A canvas is showing (false on the home screen).
    fn canvas_open(&self) -> bool;
    /// Open a project folder (saving the current one).
    fn open_project(&mut self, dir: &Path) -> Result<(), String>;
    /// Save the open project now.
    fn save(&mut self) -> Result<(), String>;
    /// Rename the open project (its manifest and folder); returns the new folder.
    fn rename_project(&mut self, name: &str) -> Result<PathBuf, String>;
    /// An offscreen renderer for screenshots and PNG export.
    fn gpu(&mut self) -> Option<Headless>;
    /// The host cannot render synchronously but will render a [`ShotRequest`] itself and answer
    /// the call later (the browser: GPU readback is asynchronous there). When true and
    /// [`Host::gpu`] is None, `get_screenshot` returns a [`NEEDS_RENDER`] marker error.
    fn defer_pixels(&self) -> bool {
        false
    }
}

/// A host without a window: an editor plus a project session. Used by tests and anything
/// that wants to drive papercraft headlessly.
pub struct SessionHost {
    pub ed: Editor,
    pub session: Session,
    gpu: Option<Headless>,
    gpu_tried: bool,
}

impl SessionHost {
    pub fn new(store: Store) -> SessionHost {
        let mut ed = Editor::default();
        ed.headless = true;
        SessionHost {
            ed,
            session: Session::new(store),
            gpu: None,
            gpu_tried: false,
        }
    }
}

impl Host for SessionHost {
    fn editor(&mut self) -> &mut Editor {
        &mut self.ed
    }
    fn store(&self) -> &Store {
        &self.session.store
    }
    fn project(&self) -> Option<&Project> {
        self.session.project.as_ref()
    }
    fn canvas_open(&self) -> bool {
        true
    }
    fn open_project(&mut self, dir: &Path) -> Result<(), String> {
        if self.session.project.is_some() {
            self.session
                .save_all(&mut self.ed)
                .map_err(|e| e.to_string())?;
        }
        self.session
            .open(&mut self.ed, dir)
            .map_err(|e| format!("{}: {e}", dir.display()))
    }
    fn save(&mut self) -> Result<(), String> {
        if self.session.project.is_some() {
            self.session
                .save_all(&mut self.ed)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn rename_project(&mut self, name: &str) -> Result<PathBuf, String> {
        self.session.rename_open(&mut self.ed, name)
    }
    fn gpu(&mut self) -> Option<Headless> {
        if !self.gpu_tried {
            self.gpu_tried = true;
            self.gpu = Headless::new()
                .map_err(|e| log::warn!("mcp: no offscreen renderer: {e}"))
                .ok();
        }
        self.gpu.as_ref().map(|g| Headless {
            device: g.device.clone(),
            queue: g.queue.clone(),
            adapter_info: g.adapter_info.clone(),
        })
    }
}

// ------------------------------------------------------------------------------ arguments

struct Args<'a>(&'a Map<String, Value>);

impl<'a> Args<'a> {
    fn get(&self, k: &str) -> Option<&'a Value> {
        self.0.get(k).filter(|v| !v.is_null())
    }
    fn str(&self, k: &str) -> Option<&'a str> {
        self.get(k).and_then(Value::as_str).map(str::trim)
    }
    fn req_str(&self, k: &str) -> Result<&'a str, String> {
        self.str(k)
            .ok_or_else(|| format!("missing required string argument `{k}`"))
    }
    fn num(&self, k: &str) -> Option<f64> {
        self.get(k).and_then(|v| match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().trim_end_matches("px").parse().ok(),
            _ => None,
        })
    }
    fn bool(&self, k: &str) -> Option<bool> {
        self.get(k).and_then(Value::as_bool)
    }
    fn arr(&self, k: &str) -> Result<&'a Vec<Value>, String> {
        self.get(k)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("missing required array argument `{k}`"))
    }
    fn strs(&self, k: &str) -> Result<Vec<String>, String> {
        Ok(self
            .arr(k)?
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect())
    }
}

fn text(v: Value) -> ToolResult {
    Ok(vec![Out::Text(
        serde_json::to_string_pretty(&v).unwrap_or_default(),
    )])
}

fn r2(v: f64) -> Value {
    json!((v * 100.0).round() / 100.0)
}

// ------------------------------------------------------------------------------ ids

const ROOT: &str = "root";

fn page_prefix(ed: &Editor, page: usize) -> &str {
    let id = &ed.pages[page].id;
    &id[..id.len().min(6)]
}

pub fn node_key(ed: &Editor, page: usize, id: NodeId) -> String {
    format!("{}:{}", page_prefix(ed, page), id.0)
}

pub(crate) fn find_page(ed: &Editor, key: &str) -> Result<usize, String> {
    let key = key.trim();
    ed.pages
        .iter()
        .position(|p| p.id == key)
        .or_else(|| {
            ed.pages
                .iter()
                .position(|p| p.id.starts_with(key) && key.len() >= 4)
        })
        .or_else(|| {
            ed.pages
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(key))
        })
        .ok_or_else(|| {
            format!(
                "page `{key}` not found; pages are {}",
                ed.pages
                    .iter()
                    .map(|p| format!("{} ({})", p.id, p.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

fn page_arg(ed: &Editor, a: &Args) -> Result<usize, String> {
    match a.str("pageId") {
        Some(p) if !p.is_empty() => find_page(ed, p),
        _ => Ok(ed.active_page),
    }
}

/// Resolve a node id string to (page index, node id).
fn resolve(ed: &Editor, key: &str, default_page: usize) -> Result<(usize, NodeId), String> {
    let key = key.trim().trim_start_matches('#');
    let (page, num) = match key.split_once(':') {
        Some((p, n)) => (find_page(ed, p)?, n),
        None => (default_page, key),
    };
    let id = NodeId(
        num.parse::<u64>()
            .map_err(|_| format!("`{key}` is not a node id (expected e.g. \"3fa9c1:14\")"))?,
    );
    if ed.page_doc(page).get(id).is_none() {
        return Err(format!("node `{key}` does not exist"));
    }
    Ok((page, id))
}

/// Resolve several ids that must all live on one page.
fn resolve_all(
    ed: &Editor,
    keys: &[String],
    default_page: usize,
) -> Result<(usize, Vec<NodeId>), String> {
    let mut page = None;
    let mut out = Vec::new();
    for k in keys {
        let (p, id) = resolve(ed, k, default_page)?;
        if page.is_some_and(|q| q != p) {
            return Err("all nodes in one call must be on the same page".into());
        }
        page = Some(p);
        out.push(id);
    }
    Ok((page.unwrap_or(default_page), out))
}

// ------------------------------------------------------------------------------ dispatch

/// Run one tool. Errors are returned as messages for the agent.
pub fn call(host: &mut dyn Host, name: &str, args: &Value) -> ToolResult {
    let empty = Map::new();
    let a = Args(args.as_object().unwrap_or(&empty));
    match name {
        "list_resources" => list_resources(host, &a),
        "open_file" => open_file(host, &a),
        "create_file" => create_file(host, &a),
        "rename_file" => rename_file(host, &a),
        "get_font_family_info" => font_info(&a),
        "get_guide" => guide(&a),
        "publish_library" | "list_libraries" | "enable_library" | "update_library"
        | "disable_library" => {
            check_file(host, &a)?;
            host.editor().commit_pending();
            host.editor().finish_loading();
            library_tool(host, name, &a)
        }
        _ => {
            check_file(host, &a)?;
            let ed = host.editor();
            ed.commit_pending();
            // Tools may name any page, and a call must see the whole project.
            ed.finish_loading();
            let scope = page_arg(ed, &a).unwrap_or(ed.active_page);
            css::set_variables_in_scope(&ed.page_doc(scope).variables, &ed.page_doc(scope).tokens);
            // The first content on the page the user is looking at is brought into view.
            let was_empty = ed.doc.root.is_empty();
            let r = match name {
                "create_page" => create_page(ed, &a),
                "rename_pages" => rename_pages(ed, &a),
                "delete_pages" => delete_pages(ed, &a),
                "get_basic_info" => basic_info(host, &a),
                "get_selection" => selection(ed),
                "get_node_info" => node_info(ed, &a),
                "get_children" => children(ed, &a),
                "get_tree_summary" => tree_summary(ed, &a),
                "get_screenshot" => screenshot(host, &a),
                "get_jsx" => jsx(host, &a),
                "get_design_spec" => get_design_spec(ed, &a),
                "annotate" => annotate(ed, &a),
                "get_annotations" => get_annotations(ed, &a),
                "get_annotation_template" => get_annotation_template(ed, &a),
                "set_annotation_template" => set_annotation_template(ed, &a),
                "upsert_annotation_template" => upsert_annotation_template(ed, &a),
                "delete_annotation_template" => delete_annotation_template(ed, &a),
                "update_annotation" => update_annotation(ed, &a),
                "delete_annotations" => delete_annotations(ed, &a),
                "verify_code" => verify_code(ed, &a),
                "get_computed_styles" => computed_styles(ed, &a),
                "get_fill_image" => fill_image(ed, &a),
                "zoom_image" => zoom_image(host, &a),
                "ask_user" => ask_user(&a),
                "report_issue" => report_issue(&a),
                #[cfg(not(target_arch = "wasm32"))]
                "design_rules" => design_rules(&a),
                #[cfg(not(target_arch = "wasm32"))]
                "fetch_reference_images" => super::refs::fetch(host.project().map(|p| p.dir.clone()).as_deref(), None, &Value::Object(a.0.clone())),
                #[cfg(not(target_arch = "wasm32"))]
                "generate_image" => {
                    let provider = crate::ui::settings::ChatPrefs::load().image_provider();
                    super::imagegen::generate(host.project().map(|p| p.dir.clone()).as_deref(), None, &provider, &Value::Object(a.0.clone()))
                }
                #[cfg(not(target_arch = "wasm32"))]
                "delegate" => super::delegate::run(&crate::ui::settings::ChatPrefs::load().helper_command, None, &Value::Object(a.0.clone())),
                "find_nodes" => find_nodes(ed, &a),
                "set_component_props" => set_component_props(ed, &a),
                "set_instance_props" => set_instance_props(ed, &a),
                "merge_variants" => merge_variants(ed, &a),
                "export" => export(host, &a),
                "export_combined_pdf" => export_pdf(host, &a),
                "create_artboard" => create_artboard(ed, &a),
                "write_html" => write_html(ed, &a),
                "create_shape" => create_shape(ed, &a),
                "update_styles" => update_styles(ed, &a),
                "set_text_content" => set_text_content(ed, &a),
                "get_variables" => get_variables(ed, &a),
                "lint_design" => lint_design(ed, &a),
                "get_feedback" => get_feedback(ed, &a),
                "add_feedback" => add_feedback(ed, &a),
                "resolve_feedback" => resolve_feedback(ed, &a),
                "set_variables" => set_variables(ed, &a),
                "set_variable_mode" => set_variable_mode(ed, &a),
                "set_tokens" => set_tokens(ed, &a),
                "create_collection" => create_collection(ed, &a),
                "delete_variables" => delete_variables(ed, &a),
                "import_design_system" => import_design_system(host, &a),
                "export_design_system" => export_design_system(host, &a),
                "get_components" => get_components(ed, &a),
                "import_components" => import_components(ed, &a),
                "create_component" => create_component(ed, &a),
                "create_instance" => create_instance(ed, &a),
                "swap_instance" => swap_instance(ed, &a),
                "bind_variable" => bind_variable(ed, &a),
                "detach_instance" => detach_instance(ed, &a),
                "diff_frames" => diff_frames(ed, &a),
                "search_icons" => search_icons(&a),
                "rename_nodes" => rename_nodes(ed, &a),
                "set_code_link" => set_code_link(ed, &a),
                "apply_ui_kit" => apply_ui_kit(host, &a),
                "delete_nodes" => delete_nodes(ed, &a),
                "duplicate_nodes" => duplicate_nodes(ed, &a),
                "move_nodes" => move_nodes(ed, &a),
                "select_nodes" => select_nodes(ed, &a),
                "set_viewport" => set_viewport(ed, &a),
                "undo" | "redo" => undo_redo(ed, &a, name == "undo"),
                "finish_working_on_nodes" => {
                    host.save()?;
                    text(json!({"ok": true, "saved": host.project().is_some()}))
                }
                _ => Err(format!("unknown tool `{name}`")),
            };
            let ed = host.editor();
            if r.is_ok() && was_empty && !ed.doc.root.is_empty() && name != "set_viewport" {
                ed.zoom_to_fit();
            }
            r
        }
    }
}

/// Projects match by manifest id, name or folder path.
fn find_project(store: &Store, key: &str) -> Result<PathBuf, String> {
    let key = key.trim().trim_start_matches("/file/");
    let p = Path::new(key);
    if p.is_absolute() && crate::vfs::is_file(&p.join(crate::project::MANIFEST)) {
        return Ok(p.to_path_buf());
    }
    let list = store.list();
    list.iter()
        .find(|e| e.manifest.id == key)
        .or_else(|| list.iter().find(|e| e.manifest.name == key))
        .or_else(|| {
            list.iter()
                .find(|e| e.manifest.name.eq_ignore_ascii_case(key))
        })
        .or_else(|| {
            list.iter()
                .find(|e| e.dir.file_name().is_some_and(|f| f == key))
        })
        .map(|e| e.dir.clone())
        .ok_or_else(|| format!("project `{key}` not found (see list_resources)"))
}

fn check_file(host: &mut dyn Host, a: &Args) -> Result<(), String> {
    if !host.canvas_open() {
        return Err(
            "No project is open in papercraft (the home screen is showing). Call list_resources \
             and open_file, or create_file then open_file."
                .into(),
        );
    }
    let Some(key) = a.str("fileId").filter(|k| !k.is_empty()) else {
        return Ok(());
    };
    let open = host
        .project()
        .map(|p| (p.manifest.id.clone(), p.dir.clone()));
    let want = find_project(host.store(), key)?;
    match open {
        Some((_, dir)) if same_dir(&dir, &want) => Ok(()),
        Some((id, _)) => Err(format!(
            "`{key}` is not the project open in the app ({id}). Call open_file first."
        )),
        None => Err(format!(
            "`{key}` is not open (a scratch document is showing). Call open_file first."
        )),
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b || a.canonicalize().ok() == b.canonicalize().ok()
}

// ------------------------------------------------------------------------------ projects

fn list_resources(host: &mut dyn Host, a: &Args) -> ToolResult {
    let limit = a.num("limit").unwrap_or(50.0).max(1.0) as usize;
    let open = host.project().map(|p| p.dir.clone());
    let list = host.store().list();
    let total = list.len();
    let items: Vec<Value> = list
        .into_iter()
        .take(limit)
        .map(|e| {
            let is_open = open.as_ref().is_some_and(|d| same_dir(d, &e.dir));
            json!({
                "id": e.manifest.id,
                "type": "file",
                "name": e.manifest.name,
                "path": e.dir,
                "pages": e.manifest.pages.len(),
                "updatedAt": e.manifest.modified,
                "open": is_open,
                "active": is_open && host.canvas_open(),
            })
        })
        .collect();
    let mut v = json!({"items": items, "root": host.store().root});
    if total > limit {
        v["truncated"] = json!(true);
    }
    text(v)
}

fn open_file(host: &mut dyn Host, a: &Args) -> ToolResult {
    let dir = find_project(host.store(), a.req_str("fileId")?)?;
    let already = host.canvas_open() && host.project().is_some_and(|p| same_dir(&p.dir, &dir));
    if !already {
        host.open_project(&dir)?;
    }
    if let Some(p) = a.str("pageId") {
        let ed = host.editor();
        let i = find_page(ed, p)?;
        ed.switch_page(i);
    }
    basic_info(host, &Args(&Map::new()))
}

fn create_file(host: &mut dyn Host, a: &Args) -> ToolResult {
    let name = a
        .str("name")
        .filter(|n| !n.is_empty())
        .unwrap_or("Untitled");
    let (p, page) = host.store().create(name).map_err(|e| e.to_string())?;
    text(json!({
        "fileId": p.manifest.id,
        "name": p.manifest.name,
        "path": p.dir,
        "pageId": page.id,
        "next": "call open_file with this fileId to work in it",
    }))
}

/// Rename a project: the open one by default (saved first), or any project by `fileId`.
fn rename_file(host: &mut dyn Host, a: &Args) -> ToolResult {
    let name = a.req_str("name")?.trim();
    if name.is_empty() {
        return Err("name must not be empty".into());
    }
    let open = host.project().map(|p| p.dir.clone());
    let dir = match a.str("fileId").filter(|f| !f.is_empty()) {
        Some(f) => find_project(host.store(), f)?,
        None => open.clone().ok_or("no project is open: pass fileId")?,
    };
    let path = if open.is_some_and(|o| same_dir(&o, &dir)) {
        host.rename_project(name)?
    } else {
        host.store().rename(&dir, name).map_err(|e| e.to_string())?
    };
    text(json!({"name": name, "path": path}))
}

fn create_page(ed: &mut Editor, a: &Args) -> ToolResult {
    // The new page copies the active page's modes and cache (`insert_empty_page`).
    let i = ed.add_page_in_background(a.str("name"));
    text(json!({"pageId": ed.pages[i].id, "name": ed.pages[i].name}))
}

fn rename_pages(ed: &mut Editor, a: &Args) -> ToolResult {
    let mut done = Vec::new();
    for u in a.arr("updates")? {
        let u = Args(u.as_object().ok_or("each update must be an object")?);
        let i = find_page(ed, u.req_str("pageId")?)?;
        ed.rename_page(i, u.req_str("name")?);
        done.push(json!({"pageId": ed.pages[i].id, "name": ed.pages[i].name}));
    }
    text(json!({"renamed": done}))
}

fn delete_pages(ed: &mut Editor, a: &Args) -> ToolResult {
    let confirm = a.bool("confirm").unwrap_or(false);
    let mut idx = Vec::new();
    for v in a.arr("pageIds")? {
        let i = find_page(ed, v.as_str().ok_or("pageIds must be strings")?)?;
        if !idx.contains(&i) {
            idx.push(i);
        }
    }
    if idx.len() >= ed.pages.len() {
        return Err("a project keeps at least one page".into());
    }
    if let Some(i) = idx
        .iter()
        .find(|i| !confirm && !ed.page_doc(**i).nodes.is_empty())
    {
        return Err(format!(
            "page `{}` has layers; pass confirm: true to delete it",
            ed.pages[*i].name
        ));
    }
    // Highest index first, so earlier indices stay valid.
    idx.sort_unstable_by(|a, b| b.cmp(a));
    let deleted: Vec<String> = idx
        .into_iter()
        .filter_map(|i| {
            let name = ed.pages[i].name.clone();
            ed.delete_page(i).map(|_| name)
        })
        .collect();
    text(json!({"deleted": deleted}))
}

// ------------------------------------------------------------------------------ reading

/// Node summary with parent-relative and world positions.
fn node_brief(ed: &Editor, page: usize, id: NodeId) -> Value {
    let doc = ed.page_doc(page);
    let n = doc.get(id).unwrap();
    let w = doc.world_rect(id).unwrap_or(n.rect);
    json!({
        "id": node_key(ed, page, id),
        "name": n.name,
        "type": n.kind.type_name(),
        "x": r2(n.rect.x), "y": r2(n.rect.y),
        "worldX": r2(w.x), "worldY": r2(w.y),
        "width": r2(n.rect.w), "height": r2(n.rect.h),
        "childCount": n.children.len(),
    })
}

/// The top-level frame a node belongs to.
fn artboard_of(doc: &Document, id: NodeId) -> Option<NodeId> {
    let top = doc.ancestors(id).last().copied().unwrap_or(id);
    doc.get(top).filter(|n| n.is_frame()).map(|n| n.id)
}

fn basic_info(host: &mut dyn Host, a: &Args) -> ToolResult {
    let project = host.project().map(|p| {
        (
            p.manifest.id.clone(),
            p.manifest.name.clone(),
            p.dir.clone(),
        )
    });
    let ed = host.editor();
    // Reports every page's node count (also reached right after `open_file`).
    ed.finish_loading();
    let page = page_arg(ed, a)?;
    let doc = ed.page_doc(page);
    let mut artboards = Vec::new();
    let mut others: HashMap<&str, usize> = HashMap::new();
    for id in &doc.root {
        let n = doc.get(*id).unwrap();
        if n.is_frame() {
            artboards.push(node_brief(ed, page, *id));
        } else {
            *others.entry(n.kind.type_name()).or_default() += 1;
        }
    }
    let pages: Vec<Value> = ed
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let pdoc = ed.page_doc(i);
            let open_fb: usize = pdoc.nodes.values().map(|n| n.open_comments()).sum();
            let resolved_fb: usize = pdoc.nodes.values().map(|n| n.resolved_comments()).sum();
            json!({
                "id": p.id, "name": p.name, "isActive": i == ed.active_page,
                "nodeCount": pdoc.nodes.len(),
                "openFeedback": open_fb,
                "resolvedFeedback": resolved_fb,
            })
        })
        .collect();
    let (file_id, file_name, path) = match project {
        Some((id, name, dir)) => (json!(id), json!(name), json!(dir)),
        None => (Value::Null, json!(doc.name), Value::Null),
    };
    text(json!({
        "fileId": file_id,
        "fileName": file_name,
        "path": path,
        "page": {"id": ed.pages[page].id, "name": ed.pages[page].name, "isActive": page == ed.active_page},
        "pages": pages,
        "nodeCount": doc.nodes.len(),
        "artboards": artboards,
        "otherTopLevelLayers": others,
        "fontFamilies": ["Inter"],
        "selectionCount": if page == ed.active_page { ed.selection.len() } else { 0 },
        "openFeedback": (0..ed.pages.len())
            .map(|p| ed.page_doc(p).nodes.values().map(|n| n.open_comments()).sum::<usize>())
            .sum::<usize>(),
        "resolvedFeedback": (0..ed.pages.len())
            .map(|p| ed.page_doc(p).nodes.values().map(|n| n.resolved_comments()).sum::<usize>())
            .sum::<usize>(),
        "nodeIdFormat": "\"<page>:<n>\", e.g. \"".to_string() + &node_key(ed, page, NodeId(1)) + "\"",
        "viewport": viewport_json(ed),
    }))
}

/// What the user sees: the active page, the visible world rect and the zoom.
fn viewport_json(ed: &Editor) -> Value {
    let v = ed.camera.visible_world(ed.viewport);
    json!({
        "pageId": ed.pages[ed.active_page].id,
        "rect": [r2(v.x), r2(v.y), r2(v.w), r2(v.h)],
        "zoom": (ed.camera.zoom * 1000.0).round() / 1000.0,
    })
}

fn comments_json(n: &Node, with_resolved: bool) -> Vec<Value> {
    n.comments
        .iter()
        .filter(|c| with_resolved || !c.resolved)
        .map(|c| {
            let mut v = json!({"id": c.id, "author": c.author, "text": c.text});
            if c.resolved {
                v["resolved"] = json!(true);
            }
            if let Some(r) = &c.reply {
                v["reply"] = json!(r);
            }
            v
        })
        .collect()
}

/// "Artboard > Frame > Text" for a node.
fn node_path(doc: &Document, id: NodeId) -> String {
    let mut names: Vec<String> = doc
        .ancestors(id)
        .into_iter()
        .filter_map(|a| doc.get(a).map(|n| n.name.clone()))
        .collect();
    names.reverse();
    names.push(doc.get(id).map(|n| n.name.clone()).unwrap_or_default());
    names.join(" > ")
}

/// Open feedback from the user, across the project (or one page / layer): what to change, where.
fn get_feedback(ed: &mut Editor, a: &Args) -> ToolResult {
    let with_resolved = a.get("includeResolved").and_then(Value::as_bool) == Some(true);
    let only = match a.str("nodeId").filter(|n| !n.is_empty()) {
        Some(n) => Some(resolve(ed, n, page_arg(ed, a)?)?),
        None => None,
    };
    let pages: Vec<usize> = match (&only, a.str("pageId").filter(|p| !p.is_empty())) {
        (Some((p, _)), _) => vec![*p],
        (None, Some(p)) => vec![find_page(ed, p)?],
        (None, None) => (0..ed.pages.len()).collect(),
    };
    let mut items = Vec::new();
    let mut total_open = 0usize;
    let mut total_resolved = 0usize;
    for page in pages {
        let doc = ed.page_doc(page);
        for n in doc.nodes.values() {
            if only.is_none_or(|(_, id)| n.id == id) {
                total_open += n.open_comments();
                total_resolved += n.resolved_comments();
            }
        }
        let mut nodes: Vec<&Node> = doc
            .nodes
            .values()
            .filter(|n| only.is_none_or(|(_, id)| n.id == id))
            .filter(|n| {
                if with_resolved {
                    !n.comments.is_empty()
                } else {
                    n.open_comments() > 0
                }
            })
            .collect();
        nodes.sort_by_key(|n| n.id);
        for n in nodes {
            let mut v = json!({
                "nodeId": node_key(ed, page, n.id),
                "pageId": ed.pages[page].id,
                "page": ed.pages[page].name,
                "name": n.name, "type": n.kind.type_name(), "path": node_path(doc, n.id),
                "width": r2(n.rect.w), "height": r2(n.rect.h),
                "feedback": comments_json(n, with_resolved),
            });
            if let Some(t) = n.kind.text() {
                v["text"] = json!(t);
            }
            items.push(v);
        }
    }
    text(json!({
        "count": items.len(),
        "openCount": total_open,
        "resolvedCount": total_resolved,
        "items": items,
        "next": "fix what each note asks, then resolve_feedback with a short reply",
    }))
}

fn add_feedback(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let body = a.req_str("text")?.chars().take(500).collect::<String>();
    let author = a
        .str("author")
        .filter(|s| !s.is_empty())
        .unwrap_or("agent")
        .to_string();
    let cid = ed.with_page(page, |ed| {
        ed.edit("Add feedback", |tx| {
            let n = tx.node_mut(id)?;
            let next = n.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1;
            n.comments.push(crate::model::Comment {
                id: next,
                text: body,
                author,
                resolved: false,
                reply: None,
            });
            Some(next)
        })
    });
    text(json!({"nodeId": node_key(ed, page, id), "commentId": cid}))
}

fn resolve_feedback(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let which = a.num("commentId").map(|n| n as u32);
    let reply = a
        .str("reply")
        .filter(|r| !r.is_empty())
        .map(|r| r.chars().take(500).collect::<String>());
    let delete = a.get("delete").and_then(Value::as_bool) == Some(true);
    let n = ed.with_page(page, |ed| {
        ed.edit("Resolve feedback", |tx| {
            let Some(node) = tx.node_mut(id) else {
                return 0;
            };
            let mut n = 0;
            if delete {
                let before = node.comments.len();
                node.comments.retain(|c| which.is_some_and(|w| w != c.id));
                if which.is_none() {
                    node.comments.clear();
                }
                return before - node.comments.len();
            }
            for c in node
                .comments
                .iter_mut()
                .filter(|c| !c.resolved && which.is_none_or(|w| w == c.id))
            {
                c.resolved = true;
                c.reply = reply.clone();
                n += 1;
            }
            n
        })
    });
    text(json!({"nodeId": node_key(ed, page, id), "resolved": n}))
}

fn selection(ed: &mut Editor) -> ToolResult {
    let page = ed.active_page;
    let items: Vec<Value> = ed
        .selection
        .iter()
        .filter(|id| ed.doc.get(**id).is_some())
        .map(|id| {
            let mut v = node_brief(ed, page, *id);
            if let Some(ab) = artboard_of(&ed.doc, *id) {
                v["artboardId"] = json!(node_key(ed, page, ab));
                v["artboardName"] = json!(ed.doc.get(ab).unwrap().name);
            }
            if let Some(t) = ed.doc.get(*id).unwrap().kind.text() {
                v["text"] = json!(t);
            }
            let n = ed.doc.get(*id).unwrap();
            if !n.comments.is_empty() {
                v["feedback"] = json!(comments_json(n, false));
            }
            v
        })
        .collect();
    text(json!({"pageId": ed.pages[page].id, "selection": items, "viewport": viewport_json(ed)}))
}

fn node_info(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let doc = ed.page_doc(page);
    let n = doc.get(id).unwrap();
    let mut v = node_brief(ed, page, id);
    v["pageId"] = json!(ed.pages[page].id);
    v["visible"] = json!(n.visible);
    v["locked"] = json!(n.locked);
    v["parentId"] = json!(n.parent.map_or(ROOT.to_string(), |p| node_key(ed, page, p)));
    v["childIds"] = json!(
        n.children
            .iter()
            .map(|c| node_key(ed, page, *c))
            .collect::<Vec<_>>()
    );
    if let Some(t) = n.kind.text() {
        v["text"] = json!(t);
    }
    if let Some(ab) = artboard_of(doc, id) {
        v["artboardId"] = json!(node_key(ed, page, ab));
    }
    if n.component {
        v["component"] = json!(true);
    }
    if n.instance.is_some() {
        v["instance"] = json!(true);
    }
    v["autoLayout"] = json!(n.layout.is_some());
    text(v)
}

fn children(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let key = a.req_str("nodeId")?;
    let (page, parent) = if key == ROOT {
        (def, None)
    } else {
        let (p, id) = resolve(ed, key, def)?;
        (p, Some(id))
    };
    let kids: Vec<Value> = ed
        .page_doc(page)
        .children(parent)
        .iter()
        .map(|c| node_brief(ed, page, *c))
        .collect();
    text(json!({"children": kids}))
}

/// Text shown per text layer in `get_tree_summary` unless `fullText` is set.
const TREE_TEXT_CHARS: usize = 40;

fn tree_summary(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let key = a.req_str("nodeId")?;
    let depth = a.num("depth").unwrap_or(3.0).clamp(1.0, 10.0) as usize;
    let full = a.bool("fullText").unwrap_or(false);
    let (page, roots) = if key == ROOT {
        (def, ed.page_doc(def).root.clone())
    } else {
        let (p, id) = resolve(ed, key, def)?;
        (p, vec![id])
    };
    fn walk(ed: &Editor, page: usize, id: NodeId, d: usize, max: usize, full: bool, out: &mut String) {
        let doc = ed.page_doc(page);
        let n = doc.get(id).unwrap();
        let size = if matches!(n.kind, NodeKind::Text { .. }) {
            format!("{}×{}", n.rect.w.round(), n.rect.h.round())
        } else {
            let s = |ax: usize, v: f64| match n.layout_child.sizing[ax] {
                Sizing::Hug => format!("hug({})", v.round()),
                Sizing::Fill => format!("fill({})", v.round()),
                Sizing::Fixed => format!("{}", v.round()),
            };
            format!("{}×{}", s(0, n.rect.w), s(1, n.rect.h))
        };
        let mut line = format!(
            "{}{} \"{}\" [{}] {}",
            "  ".repeat(d),
            n.kind.type_name(),
            n.name,
            node_key(ed, page, id),
            size
        );
        if let Some(t) = n.kind.text() {
            let short: String = if full { t.clone() } else { t.chars().take(TREE_TEXT_CHARS).collect() };
            line += &format!(" {:?}", short);
        }
        if !n.visible {
            line += " (hidden)";
        }
        out.push_str(&line);
        out.push('\n');
        if d + 1 >= max && !n.children.is_empty() {
            out.push_str(&format!(
                "{}… {} children\n",
                "  ".repeat(d + 1),
                n.children.len()
            ));
            return;
        }
        for c in &n.children {
            walk(ed, page, *c, d + 1, max, full, out);
        }
    }
    let mut out = String::new();
    for r in roots {
        walk(ed, page, r, 0, depth, full, &mut out);
    }
    if out.is_empty() {
        out = "(empty page)".into();
    }
    Ok(vec![Out::Text(out)])
}

/// Largest screenshot edge, in pixels.
const SHOT_MAX_EDGE: f64 = 2048.0;

/// Prefix of the error `get_screenshot` returns on a host that renders asynchronously: the rest
/// is a JSON [`ShotRequest`]. The shell renders it, then replies with [`screenshot_async`].
pub const NEEDS_RENDER: &str = "papercraft:needs-render:";

/// What a screenshot has to draw, resolved against the editor at call time.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShotRequest {
    pub page: usize,
    pub roots: Vec<NodeId>,
    pub scale: f64,
    /// Draw the notes on the captured roots (see [`with_notes`]).
    #[serde(default)]
    pub annotations: bool,
}

impl ShotRequest {
    /// The request inside a [`NEEDS_RENDER`] marker error, if `err` is one.
    pub fn from_marker(err: &str) -> Option<ShotRequest> {
        serde_json::from_str(err.strip_prefix(NEEDS_RENDER)?).ok()
    }
}

/// PNG-encode a screenshot. None (with `scale` reduced) when the payload is too big to send.
fn shot_out(img: &image::RgbaImage, scale: &mut f64) -> Result<Option<Vec<Out>>, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let png = buf.into_inner();
    // Keep the base64 payload comfortably under API image limits (~5 MB).
    if png.len() > 3_500_000 && *scale > 0.2 {
        *scale *= 0.7;
        return Ok(None);
    }
    let (w, h) = img.dimensions();
    Ok(Some(vec![
        Out::Image {
            data: png,
            mime: "image/png".into(),
        },
        Out::Text(
            json!({"width": w, "height": h, "scale": (*scale * 1000.0).round() / 1000.0})
                .to_string(),
        ),
    ]))
}

fn screenshot(host: &mut dyn Host, a: &Args) -> ToolResult {
    let gpu = host.gpu();
    let defer = host.defer_pixels();
    if gpu.is_none() && !defer {
        return Err("screenshots need a GPU and none is available".into());
    }
    let out_path = a
        .str("out")
        .or_else(|| a.str("path"))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let ed = host.editor();
    let def = page_arg(ed, a)?;
    let (page, roots) = match a.str("nodeId").filter(|k| !k.is_empty() && *k != ROOT) {
        Some(k) => {
            let (p, id) = resolve(ed, k, def)?;
            (p, vec![id])
        }
        None => (def, ed.page_doc(def).root.clone()),
    };
    let annotations = a.bool("annotations").unwrap_or(false);
    let noted = annotations.then(|| with_notes(ed.page_doc(page), &roots, &ed.design.categories()));
    let (doc, shot_roots) = match &noted {
        Some((d, r)) => (d, r),
        None => (ed.page_doc(page), &roots),
    };
    let b = crate::io::svg::export_bounds(doc, shot_roots).ok_or("nothing to capture (empty)")?;
    let want = a.num("scale").unwrap_or(1.0).clamp(0.1, 4.0);
    let mut scale = want.min(SHOT_MAX_EDGE / b.w.max(b.h).max(1.0));
    let Some(gpu) = gpu else {
        let req = ShotRequest { page, roots, scale, annotations };
        return Err(format!(
            "{NEEDS_RENDER}{}",
            serde_json::to_string(&req).map_err(|e| e.to_string())?
        ));
    };
    loop {
        let img = crate::io::png::render(&gpu, doc, Some(shot_roots), scale, Some(Color::WHITE))?;
        if let Some(mut out) = shot_out(&img, &mut scale)? {
            if let Some(p) = &out_path {
                if let Some(parent) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Some(Out::Image { data, .. }) = out.first() {
                    std::fs::write(p, data).map_err(|e| format!("{}: {e}", p.display()))?;
                }
                if let Some(Out::Text(txt)) = out.get_mut(1)
                    && let Ok(mut v) = serde_json::from_str::<Value>(txt)
                {
                    v["path"] = json!(p.to_string_lossy());
                    *txt = v.to_string();
                }
            }
            return Ok(out);
        }
    }
}

/// Finish a deferred screenshot: the same rendering as [`screenshot`], awaiting the readback.
/// `doc` is the page the [`ShotRequest`] named, as it is now.
pub async fn screenshot_async(
    gpu: &Headless,
    doc: &Document,
    cats: &[AnnotationCategory],
    req: &ShotRequest,
) -> ToolResult {
    let noted = req.annotations.then(|| with_notes(doc, &req.roots, cats));
    let (doc, roots) = match &noted {
        Some((d, r)) => (d, r),
        None => (doc, &req.roots),
    };
    let mut scale = req.scale;
    loop {
        let img = crate::io::png::render_async(gpu, doc, Some(roots), scale, Some(Color::WHITE))
            .await?;
        if let Some(out) = shot_out(&img, &mut scale)? {
            return Ok(out);
        }
    }
}

/// A copy of `doc` with the note cards, badges and leader lines of the notes on `roots` added
/// as ordinary layers (placed and routed as the canvas draws them), and the roots to render:
/// `roots`, then the lines, then the cards. The renderer needs nothing else.
pub fn with_notes(
    doc: &Document,
    roots: &[NodeId],
    cats: &[AnnotationCategory],
) -> (Document, Vec<NodeId>) {
    use crate::model::annotation::{Side, anchor_world, card_world, numbers};
    use crate::model::annotation_layout::{self as layout, BADGE, BODY_LH, BODY_PT, BODY_W, HEAD_GAP, LABEL_LH, LABEL_PT, LABEL_W, PAD};
    const RADIUS: f64 = 10.0;
    const LINE_W: f64 = 1.5;
    const DOT_R: f64 = 3.5;
    const NUMBER_PT: f64 = 11.0;
    let card_fill = Color::WHITE;
    let card_border = Color::rgb(0xe0, 0xe0, 0xe0);
    let body_color = Color::rgb(0x6b, 0x72, 0x80);
    let notes: Vec<&crate::model::annotation::Annotation> = doc
        .annotations
        .iter()
        .filter(|a| !a.detached)
        .filter(|a| a.target.is_some_and(|t| roots.iter().any(|r| *r == t || doc.is_ancestor(*r, t))))
        .collect();
    let mut out = doc.clone();
    let mut shot = roots.to_vec();
    if notes.is_empty() {
        return (out, shot);
    }
    let nums = numbers(doc);
    let cards: Vec<Rect> = notes
        .iter()
        .map(|a| {
            let at = card_world(doc, a);
            let s = layout::card_size(a);
            Rect::new(at.x, at.y, s.x, s.y)
        })
        .collect();
    let add = |out: &mut Document, mut n: Node, parent: Option<NodeId>| {
        n.parent = parent;
        let id = n.id;
        if let Some(p) = parent.and_then(|p| out.nodes.get_mut(&p)) {
            p.children.push(id);
        }
        out.nodes.insert(id, n);
        id
    };
    let text_node = |out: &mut Document, s: &str, pt: f64, lh: f64, color: Color, r: Rect| {
        let mut n = Node::new(
            out.alloc_id(),
            NodeKind::Text { text: s.into(), font_size: pt, auto_width: false, align: TextAlign::Left },
            r,
        );
        n.style = crate::model::Style::with_fill(color);
        n.text_style.line_height = Some(lh / pt);
        n
    };
    let mut card_ids = Vec::new();
    for (i, a) in notes.iter().enumerate() {
        let target = a.target.unwrap();
        let Some(t) = doc.world_rect(target) else { continue };
        let card = cards[i];
        let color = cats
            .iter()
            .find(|c| c.id == a.category)
            .or(cats.first())
            .map_or(Color::rgb(0x88, 0x88, 0x88), |c| c.color);
        // Leader line, routed like the canvas does it.
        let end = if a.anchor.side == Side::Center {
            layout::anchor_point(t, layout::anchor_toward(t, card))
        } else {
            anchor_world(doc, a)
        };
        let mut obs = layout::route_obstacles(doc, target);
        obs.extend(cards.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, r)| *r));
        let pts = layout::route(card, t, end, &obs);
        let bb = Rect::from_points(pts[0], pts[0]);
        let bb = pts.iter().fold(bb, |b, p| b.union(&Rect::from_points(*p, *p)));
        let mut line = Node::new(
            out.alloc_id(),
            NodeKind::Path {
                subpaths: vec![SubPath {
                    points: pts.iter().map(|p| PathPoint::corner(*p - bb.min())).collect(),
                    closed: false,
                }],
                fill_rule: Default::default(),
            },
            bb,
        );
        line.style = crate::model::Style { stroke: Some(color), stroke_width: LINE_W, ..Default::default() };
        shot.push(add(&mut out, line, None));
        let mut dot = Node::new(
            out.alloc_id(),
            NodeKind::Ellipse,
            Rect::new(end.x - DOT_R, end.y - DOT_R, DOT_R * 2.0, DOT_R * 2.0),
        );
        dot.style = crate::model::Style::with_fill(color);
        shot.push(add(&mut out, dot, None));
        // Card: frame, number badge, label, body.
        let mut frame = Node::new(out.alloc_id(), NodeKind::Frame { clip: true }, card);
        frame.name = format!("Note {}", nums.get(&a.id).map_or(0, |n| n.1));
        frame.style = crate::model::Style {
            stroke: Some(card_border),
            stroke_width: 1.0,
            corner_radius: RADIUS,
            ..crate::model::Style::with_fill(card_fill)
        };
        let f = add(&mut out, frame, None);
        card_ids.push(f);
        let mut badge = Node::new(out.alloc_id(), NodeKind::Ellipse, Rect::new(PAD, PAD, BADGE, BADGE));
        badge.style = crate::model::Style::with_fill(color);
        add(&mut out, badge, Some(f));
        let n = nums.get(&a.id).map_or(0, |n| n.1).to_string();
        let mut num = text_node(&mut out, &n, NUMBER_PT, BADGE, Color::WHITE, Rect::new(PAD, PAD, BADGE, BADGE));
        num.text_style.bold = true;
        if let NodeKind::Text { align, .. } = &mut num.kind {
            *align = TextAlign::Center;
        }
        add(&mut out, num, Some(f));
        let l = layout::card_lines(&a.label, &a.body, false);
        let head = BADGE.max(l.label as f64 * LABEL_LH);
        let label_r = Rect::new(PAD + BADGE + 8.0, PAD, LABEL_W, l.label as f64 * LABEL_LH);
        let mut label = text_node(&mut out, &a.label, LABEL_PT, LABEL_LH, Color::INK, label_r);
        label.text_style.bold = true;
        add(&mut out, label, Some(f));
        if !a.body.is_empty() {
            // ponytail: a long body is cut by the card's clip, without the canvas's "+N more lines".
            let body_r = Rect::new(PAD, PAD + head + HEAD_GAP, BODY_W, (l.body + usize::from(l.more > 0)) as f64 * BODY_LH);
            let b = text_node(&mut out, &a.body, BODY_PT, BODY_LH, body_color, body_r);
            add(&mut out, b, Some(f));
        }
    }
    shot.extend(card_ids);
    out.root.extend(shot[roots.len()..].iter().copied());
    (out, shot)
}

fn parent_layout(doc: &Document, n: &Node) -> Option<AutoLayout> {
    n.parent.and_then(|p| doc.get(p)).and_then(|p| p.layout)
}

fn computed_styles(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let mut out = Map::new();
    for k in a.strs("nodeIds")? {
        let (page, id) = resolve(ed, &k, def)?;
        let doc = ed.page_doc(page);
        let n = doc.get(id).unwrap();
        out.insert(
            node_key(ed, page, id),
            Value::Object(css::computed(
                n,
                parent_layout(doc, n).as_ref(),
                n.parent.is_none(),
            )),
        );
    }
    text(Value::Object(out))
}

fn fill_image(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let n = ed.page_doc(page).get(id).unwrap();
    let asset = n
        .style
        .fills
        .iter()
        .rev()
        .find_map(|p| match p {
            Paint::Image { asset, .. } => Some(asset.clone()),
            _ => None,
        })
        .ok_or("the node has no image fill")?;
    let img = crate::assets::get(&asset).ok_or("the image data is not loaded")?;
    Ok(vec![
        Out::Image {
            data: img.bytes.clone(),
            mime: img.mime().into(),
        },
        Out::Text(json!({"asset": asset, "width": img.width, "height": img.height}).to_string()),
    ])
}

/// Questions for the user; the chat draws them as a card from the call's arguments, so this only
/// checks the shape and tells the agent to wait.
fn ask_user(a: &Args) -> ToolResult {
    let qs = a.arr("questions")?;
    if !(1..=ASK_MAX_QUESTIONS).contains(&qs.len()) {
        return Err(format!("ask 1 to {ASK_MAX_QUESTIONS} questions"));
    }
    for (i, q) in qs.iter().enumerate() {
        let n = q["options"].as_array().map_or(0, Vec::len);
        if q["question"].as_str().is_none_or(str::is_empty) || !(2..=ASK_MAX_OPTIONS).contains(&n) {
            return Err(format!("question {} has {n} options: give each question text and 2 to {ASK_MAX_OPTIONS} options", i + 1));
        }
    }
    text(json!({"shown": true, "next": "End your turn now. The answers arrive as the user's next message."}))
}

const ASK_MAX_QUESTIONS: usize = 7;

/// An issue draft: the chat shows it with Send and Discard; nothing is filed from here.
fn report_issue(a: &Args) -> ToolResult {
    let kind = a.req_str("kind")?;
    if !["bug", "docs", "feature"].contains(&kind) {
        return Err("kind must be bug, docs or feature".into());
    }
    if a.req_str("title")?.is_empty() || a.req_str("body")?.is_empty() {
        return Err("an issue needs a title and a body".into());
    }
    text(json!({"draft": true, "next": "The user sees the draft and sends or discards it. Do not repeat it in text; go on with the task."}))
}
const ASK_MAX_OPTIONS: usize = 6;

/// design-playbook rules through its CLI: `playbook search` or `playbook rule`.
#[cfg(not(target_arch = "wasm32"))]
fn design_rules(a: &Args) -> ToolResult {
    let bin = crate::chat::path::which("playbook").ok_or("the design-playbook CLI (`playbook`) is not installed")?;
    let args = match (a.str("rule"), a.str("query")) {
        (Some(r), _) if !r.trim().is_empty() => vec!["rule", r.trim()],
        (_, Some(q)) if !q.trim().is_empty() => vec!["search", "-n", "5", q.trim()],
        _ => return Err("give query or rule".into()),
    };
    let out = std::process::Command::new(bin).args(&args).output().map_err(|e| e.to_string())?;
    let found = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if found.is_empty() {
        return Err(format!("no playbook match: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    text(json!({"rules": found}))
}

/// A region of a chat attachment at full resolution: the `.full` original kept next to the sent
/// image when it was scaled down, else the image itself. Only files in the project's chat folder.
fn zoom_image(host: &mut dyn Host, a: &Args) -> ToolResult {
    let dir = host.project().map(|p| p.dir.join(crate::chat::TMP_DIR)).ok_or("open a project first")?;
    let sent = std::path::Path::new(a.req_str("path")?);
    let (Ok(root), Ok(sent)) = (std::fs::canonicalize(&dir), std::fs::canonicalize(sent)) else {
        return Err("no such attached image".into());
    };
    if !sent.starts_with(&root) {
        return Err("zoom_image reads only images attached in this project's chat".into());
    }
    let full = crate::chat::full_image_path(&sent);
    let bytes = std::fs::read(if full.is_file() { &full } else { &sent }).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("not an image: {e}"))?;
    let (w, h) = (f64::from(img.width()), f64::from(img.height()));
    let frac = |k: &str, default: f64| a.num(k).unwrap_or(default).clamp(0.0, 1.0);
    let (x, y) = (frac("x", 0.0), frac("y", 0.0));
    let (cw, ch) = (frac("width", 1.0).min(1.0 - x), frac("height", 1.0).min(1.0 - y));
    let (px, py, pw, ph) = ((x * w) as u32, (y * h) as u32, ((cw * w) as u32).max(1), ((ch * h) as u32).max(1));
    let crop = img.crop_imm(px, py, pw, ph);
    let max = crate::chat::ZOOM_MAX_EDGE;
    let crop = if pw.max(ph) > max { crop.resize(max, max, image::imageops::FilterType::Lanczos3) } else { crop };
    Ok(vec![
        Out::Image { data: crate::io::export::encode_png(&crop.to_rgba8())?, mime: "image/png".into() },
        Out::Text(json!({"imageWidth": img.width(), "imageHeight": img.height(), "region": [px, py, pw, ph]}).to_string()),
    ])
}

/// Case-insensitive glob with `*`, anchored at both ends.
pub fn glob(pattern: &str, s: &str) -> bool {
    let (p, s) = (pattern.to_lowercase(), s.to_lowercase());
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == s;
    }
    let mut rest = s.as_str();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(j) => rest = &rest[j + part.len()..],
                None => return false,
            }
        }
    }
    true
}

fn value_matches(want: &str, got: &str) -> Option<String> {
    if glob(want, got) {
        return Some(got.to_string());
    }
    let c = css::color(want)?;
    css::split_ws(&got.replace(',', " , "))
        .into_iter()
        .chain(std::iter::once(got.to_string()))
        .find(|part| css::color(part) == Some(c))
}

fn find_nodes(ed: &mut Editor, a: &Args) -> ToolResult {
    let mut scope: Vec<(usize, Vec<NodeId>)> = Vec::new();
    if let Some(k) = a.str("nodeId").filter(|k| !k.is_empty()) {
        let (p, id) = resolve(ed, k, page_arg(ed, a)?)?;
        scope.push((p, ed.page_doc(p).subtree(id)));
    } else if a.str("pageId").is_some() {
        let p = page_arg(ed, a)?;
        scope.push((p, ed.page_doc(p).paint_order()));
    } else {
        for p in 0..ed.pages.len() {
            scope.push((p, ed.page_doc(p).paint_order()));
        }
    }
    let text_q = a.str("textValue");
    let filters: Vec<(String, String)> = a
        .get("filters")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_object)
                .map(|o| {
                    let s = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("*").to_string();
                    (css::camel(&s("styleName")), s("styleValue"))
                })
                .collect()
        })
        .unwrap_or_default();
    if text_q.is_none() && filters.is_empty() {
        return Err("pass textValue and/or filters".into());
    }
    let mut results = Vec::new();
    for (page, ids) in scope {
        let doc = ed.page_doc(page);
        'node: for id in ids {
            let n = doc.get(id).unwrap();
            let mut matched = Vec::new();
            if let Some(q) = text_q {
                match n.kind.text() {
                    Some(t) if text_matches(q, t) => matched.push(json!({"textValue": t})),
                    _ => continue,
                }
            }
            if !filters.is_empty() {
                let st = css::computed(n, parent_layout(doc, n).as_ref(), n.parent.is_none());
                for (name, want) in &filters {
                    let hit = st.iter().find_map(|(k, v)| {
                        let v = v.as_str()?;
                        (glob(name, k))
                            .then(|| value_matches(want, v))
                            .flatten()
                            .map(|m| (k, m))
                    });
                    match hit {
                        Some((k, m)) => matched.push(json!({"styleName": k, "styleValue": m})),
                        None => continue 'node,
                    }
                }
            }
            results.push(json!({
                "id": node_key(ed, page, id),
                "name": n.name,
                "type": n.kind.type_name(),
                "pageId": ed.pages[page].id,
                "matched": matched,
            }));
            if results.len() >= FIND_MAX {
                break;
            }
        }
    }
    // Say when there are more, so a caller narrows the query instead of trusting a partial list.
    let mut v = json!({"count": results.len(), "results": results});
    if results.len() >= FIND_MAX {
        v["truncated"] = json!(true);
        v["note"] = json!(format!("only the first {FIND_MAX} matches; narrow the query (pageId, a frame, a filter)"));
    }
    text(v)
}

/// Most matches `find_nodes` returns.
const FIND_MAX: usize = 500;

/// Case- and spacing-insensitive text match: a substring unless `q` has `*` wildcards, which
/// match the whole value. Line breaks, non-breaking spaces and curly quotes compare as typed.
fn text_matches(q: &str, t: &str) -> bool {
    let norm = |s: &str| {
        s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .replace(['\u{2018}', '\u{2019}'], "'")
            .replace(['\u{201c}', '\u{201d}'], "\"")
            .to_lowercase()
    };
    let (q, t) = (norm(q), norm(t));
    if q.contains('*') { glob(&q, &t) } else { t.contains(&q) }
}

fn font_info(a: &Args) -> ToolResult {
    let fams: Vec<Value> = a
        .strs("familyNames")?
        .iter()
        .map(|f| {
            let inter = f.trim().eq_ignore_ascii_case("inter");
            json!({
                "family": f,
                "available": inter,
                "styles": if inter { json!([
                    {"weight": 400, "style": "normal"}, {"weight": 700, "style": "normal"},
                    {"weight": 400, "style": "italic"}, {"weight": 700, "style": "italic"}]) } else { json!([]) },
                "note": if inter { "" } else { "not available; text renders in Inter" },
            })
        })
        .collect();
    text(
        json!({"families": fams, "weights": "fontWeight ≥ 600 renders bold, below renders regular"}),
    )
}

fn guide(a: &Args) -> ToolResult {
    let t = a.req_str("topic")?;
    let s = match t {
        "papercraft-mcp-instructions" | "paper-mcp-instructions" | "instructions" => {
            tools::GUIDE_INSTRUCTIONS
        }
        "html" | "write_html" => tools::GUIDE_HTML,
        "whiteboard" | "excalidraw" => tools::GUIDE_WHITEBOARD,
        _ => {
            return Err(format!(
                "unknown topic `{t}`; topics: papercraft-mcp-instructions, html, whiteboard"
            ));
        }
    };
    Ok(vec![Out::Text(s.into())])
}

// ------------------------------------------------------------------------------ annotations

fn annotation_rows(rows: Vec<Value>) -> ToolResult {
    Ok(vec![Out::Text(serde_json::to_string(&rows).unwrap())])
}

fn annotation_category(ed: &Editor, key: &str) -> Result<String, String> {
    ed.design.category_by_key(key).map(|c| c.id).ok_or_else(|| {
        format!(
            "unknown annotation type `{key}`; valid keys: {}",
            ed.design
                .categories()
                .iter()
                .map(|c| c.key().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn annotation_node(ed: &Editor, key: &str, page: usize) -> Result<NodeId, String> {
    if let Ok((p, id)) = resolve(ed, key, page) {
        return if p == page {
            Ok(id)
        } else {
            Err("all notes must be on the selected page".into())
        };
    }
    let mut matches: Vec<_> = ed
        .page_doc(page)
        .nodes
        .values()
        .filter(|n| n.name.eq_ignore_ascii_case(key))
        .map(|n| n.id)
        .collect();
    matches.sort_by_key(|id| id.0);
    match matches.as_slice() {
        [id] => Ok(*id),
        [] => Err(format!("node `{key}` not found on page")),
        _ => Err(format!(
            "ambiguous node `{key}`; candidates: {}",
            matches
                .iter()
                .take(5)
                .map(|id| node_key(ed, page, *id))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn annotation_text(text: &str) -> (String, String) {
    let (label, body) = text.split_once('\n').unwrap_or((text, ""));
    (label.trim_end_matches('\r').into(), body.into())
}

/// The category id for type `key`, if the page's active template enables it.
fn enabled_category(ed: &Editor, page: usize, key: &str) -> Result<String, String> {
    let category = annotation_category(ed, key)?;
    let template = ed.active_template(page);
    if template.enabled(key).is_none() {
        return Err(format!(
            "type `{key}` is not enabled by template `{}`; allowed: {}",
            template.name,
            template.enabled_keys().join(", ")
        ));
    }
    Ok(category)
}

fn annotate(ed: &mut Editor, a: &Args) -> ToolResult {
    use crate::model::annotation::{Annotation, Author, auto_place, numbers};
    let page = page_arg(ed, a)?;
    let tidy = a.get("tidy").and_then(Value::as_bool).unwrap_or(false);
    let frame = a
        .str("frame")
        .filter(|_| tidy)
        .map(|f| annotation_node(ed, f, page))
        .transpose()?;
    let none = Vec::new();
    let raw = if tidy && a.get("notes").is_none() { &none } else { a.arr("notes")? };
    let notes = raw
        .iter()
        .map(|v| {
            let args = Args(v.as_object().ok_or("notes must be objects")?);
            let node = annotation_node(ed, args.req_str("node")?, page)?;
            let key = args
                .get("type")
                .map(|v| v.as_str().ok_or("type must be a string"))
                .transpose()?
                .unwrap_or("rule");
            let category = enabled_category(ed, page, key)?;
            let (label, body) = annotation_text(
                args.get("text")
                    .and_then(Value::as_str)
                    .ok_or("missing text string")?,
            );
            let mut note = Annotation::new(Some(node), &label, &category);
            note.body = body;
            note.author = Author::Agent;
            Ok(note)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let only_tidy = tidy && notes.is_empty();
    let (ids, moved) = ed.with_page(page, |ed| {
        ed.edit("Annotate", |tx| {
            let ids = notes
                .into_iter()
                .map(|mut note| {
                    auto_place(tx.doc(), &mut note);
                    tx.add_annotation(note)
                })
                .collect::<Vec<_>>();
            let mut moved = 0;
            if tidy {
                for (id, p) in crate::model::annotation_layout::tidy(tx.doc(), frame) {
                    let same = tx.doc().annotations.iter().any(|n| {
                        n.id == id && n.position == p.position && n.anchor == p.anchor
                    });
                    if !same {
                        tx.update_annotation(id, |n| (n.position, n.anchor) = (p.position, p.anchor));
                        moved += 1;
                    }
                }
            }
            (ids, moved)
        })
    });
    if only_tidy {
        return text(json!({"tidied": moved}));
    }
    let doc = ed.page_doc(page);
    let numbers = numbers(doc);
    let cats = ed.design.categories();
    annotation_rows(
        ids.iter()
            .map(|id| {
                let note = doc.annotations.iter().find(|n| n.id == *id).unwrap();
                let key = cats
                    .iter()
                    .find(|c| c.id == note.category)
                    .map(|c| c.key())
                    .unwrap_or(&note.category);
                json!([
                    numbers[id].1,
                    node_key(ed, page, note.target.unwrap()),
                    key,
                    note.label
                ])
            })
            .collect::<Vec<_>>(),
    )
}

fn get_annotation_template(ed: &Editor, a: &Args) -> ToolResult {
    let t = ed.active_template(page_arg(ed, a)?);
    text(json!({
        "template": t.name,
        "types": t.categories.iter().filter(|c| c.enabled)
            .map(|c| json!([c.key, c.prompt, c.fields])).collect::<Vec<_>>(),
    }))
}

fn set_annotation_template(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let r = a.req_str("template")?;
    let t = ed.design.template_by_ref(r).ok_or_else(|| {
        format!(
            "unknown template `{r}`; available: {}",
            ed.design.templates().iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
        )
    })?;
    match a.str("scope").unwrap_or("page") {
        "page" => ed.set_page_template(page, Some(&t.id)),
        "project" => ed.set_default_template(Some(&t.id)),
        s => return Err(format!("scope must be page or project, got `{s}`")),
    }
    .map_err(|e| e.to_string())?;
    text(json!({"template": t.name, "scope": a.str("scope").unwrap_or("page")}))
}

fn template_types(t: &crate::model::annotation::AnnotationTemplate) -> Value {
    json!({"id": t.id, "name": t.name, "types": t.categories.iter().filter(|c| c.enabled)
        .map(|c| json!([c.key, c.prompt, c.fields])).collect::<Vec<_>>()})
}

fn upsert_annotation_template(ed: &mut Editor, a: &Args) -> ToolResult {
    use crate::model::annotation::{AnnotationCategory, AnnotationTemplate, Display, TemplateCategory, category_by_key};
    use crate::model::Color;
    let spec = a.get("template").and_then(Value::as_object).ok_or("missing required object argument `template`")?;
    let t = Args(spec);
    let name = t.req_str("name")?;
    let page = page_arg(ed, a)?;
    let mut cats = ed.design.categories();
    for v in a.get("categories").map(|v| v.as_array().ok_or("categories must be an array")).transpose()?.into_iter().flatten() {
        let c = Args(v.as_object().ok_or("categories must be objects")?);
        let key = c.req_str("key")?.to_lowercase();
        if key.is_empty() || !key.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
            return Err(format!("category key `{key}` must be letters, digits and dashes"));
        }
        let color = c.str("color").map(|s| Color::from_hex(s).ok_or(format!("bad color `{s}`; use #rrggbb"))).transpose()?;
        let icon = c.str("icon").map(|i| {
            if crate::theme::icon_by_name(i).is_some() { Ok(i.to_string()) }
            else { Err(format!("unknown icon `{i}`; valid: {}", crate::theme::ICON_NAMES.join(", "))) }
        }).transpose()?;
        let existing = cats.iter().position(|x| x.key().eq_ignore_ascii_case(&key));
        match existing {
            Some(i) => {
                let x = &mut cats[i];
                if let Some(n) = c.str("name") { x.name = n.into(); }
                if let Some(col) = color { x.color = col; }
                if let Some(ic) = icon { x.icon = ic; }
            }
            None => cats.push(AnnotationCategory {
                id: key.clone(),
                key: key.clone(),
                name: c.req_str("name")?.into(),
                color: color.ok_or("new category needs `color`")?,
                icon: icon.ok_or("new category needs `icon`")?,
            }),
        }
    }
    let all = ed.design.templates();
    let id = match t.str("id") {
        Some(id) => id.to_string(),
        None => match all.iter().find(|x| x.name.eq_ignore_ascii_case(name)) {
            Some(x) => x.id.clone(),
            None => {
                let base = match crate::design_system::slug(name) { s if s.is_empty() => "template".into(), s => s };
                let mut id = base.clone();
                let mut n = 1;
                while all.iter().any(|x| x.id == id) { n += 1; id = format!("{base}-{n}"); }
                id
            }
        },
    };
    let display = match t.str("display") {
        None | Some("full") => Display::Full,
        Some("badges") => Display::Badges,
        Some(d) => return Err(format!("display must be full or badges, got `{d}`")),
    };
    let mut tcats: Vec<TemplateCategory> = Vec::new();
    for v in t.arr("categories")? {
        let c = Args(v.as_object().ok_or("template categories must be objects")?);
        let key = c.req_str("key")?;
        let Some(cat) = category_by_key(&cats, key) else {
            return Err(format!(
                "unknown category key `{key}`; known: {} (add it in `categories`)",
                cats.iter().map(AnnotationCategory::key).collect::<Vec<_>>().join(", ")
            ));
        };
        if tcats.iter().any(|x| x.key == cat.key()) {
            return Err(format!("category `{key}` listed twice"));
        }
        tcats.push(TemplateCategory {
            key: cat.key().into(),
            enabled: c.bool("enabled").unwrap_or(true),
            prompt: c.str("guidance").unwrap_or_default().into(),
            fields: if c.get("fields").is_some() { c.strs("fields")? } else { Vec::new() },
        });
    }
    let tpl = AnnotationTemplate { id: id.clone(), name: name.into(), categories: tcats, display };
    let set_as = match a.str("setAs") {
        None => None,
        Some(s @ ("project" | "page")) => Some(s),
        Some(s) => return Err(format!("setAs must be project or page, got `{s}`")),
    };
    let out = template_types(&tpl);
    let project = set_as == Some("project");
    ed.edit_design("Edit annotation template", |d| {
        let mut list = d.templates();
        match list.iter_mut().find(|x| x.id == id) {
            Some(x) => *x = tpl,
            None => list.push(tpl),
        }
        d.annotation_templates = list;
        d.annotation_categories = cats;
        if project {
            d.annotation_template = Some(id.clone());
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    if set_as == Some("page") {
        ed.set_page_template(page, Some(&id)).map_err(|e| e.to_string())?;
    }
    text(out)
}

fn delete_annotation_template(ed: &mut Editor, a: &Args) -> ToolResult {
    let r = a.req_str("template")?;
    let t = ed.design.template_by_ref(r).ok_or_else(|| {
        format!(
            "unknown template `{r}`; available: {}",
            ed.design.templates().iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
        )
    })?;
    if ed.design.templates().len() < 2 {
        return Err("cannot delete the last template".into());
    }
    ed.delete_template(&t.id).map_err(|e| e.to_string())?;
    text(json!({"deleted": t.name}))
}

fn get_annotations(ed: &Editor, a: &Args) -> ToolResult {
    let default_page = page_arg(ed, a)?;
    let scope = a
        .str("nodeId")
        .map(|id| resolve(ed, id, default_page))
        .transpose()?;
    let page = scope.map_or(default_page, |s| s.0);
    let doc = ed.page_doc(page);
    let subtree = scope.map(|s| doc.subtree(s.1));
    let numbers = crate::model::annotation::numbers(doc);
    let cats = ed.design.categories();
    let mut notes: Vec<_> = doc
        .annotations
        .iter()
        .filter(|note| {
            subtree
                .as_ref()
                .is_none_or(|ids| note.target.is_some_and(|id| ids.contains(&id)))
        })
        .collect();
    notes.sort_by_key(|n| (numbers[&n.id].0.map(|id| id.0), numbers[&n.id].1));
    annotation_rows(
        notes
            .into_iter()
            .map(|note| {
                let number = numbers[&note.id].1;
                let key = cats
                    .iter()
                    .find(|c| c.id == note.category)
                    .map(|c| c.key())
                    .unwrap_or(&note.category);
                if a.bool("full").unwrap_or(false) {
                    let mut value = serde_json::to_value(note).unwrap();
                    value["number"] = json!(number);
                    value["type"] = json!(key);
                    value["nodeId"] = json!(note.target.map(|id| node_key(ed, page, id)));
                    return value;
                }
                let name = note
                    .target
                    .and_then(|id| doc.get(id))
                    .map(|n| n.name.as_str())
                    .unwrap_or("Detached");
                let text = if let Some(line) = note.body.lines().next() {
                    format!("{}\n{line}", note.label)
                } else {
                    note.label.clone()
                };
                json!([
                    number,
                    name,
                    key,
                    text.chars().take(120).collect::<String>()
                ])
            })
            .collect::<Vec<_>>(),
    )
}

fn annotation_ref(ed: &Editor, value: &Value, page: usize) -> Result<u64, String> {
    let doc = ed.page_doc(page);
    let key = value.as_str().map(str::trim);
    if let Some(id) = value.as_u64().or_else(|| key.and_then(|s| s.parse().ok())) {
        return doc
            .annotations
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.id)
            .ok_or_else(|| format!("annotation {id} not found"));
    }
    let (frame, number) = key
        .and_then(|s| s.rsplit_once('#'))
        .ok_or("expected annotation id or frame reference such as Sign in #3")?;
    let number: u32 = number
        .trim()
        .parse()
        .map_err(|_| "invalid annotation number")?;
    let frame = frame.trim().strip_prefix("frame ").unwrap_or(frame.trim());
    let numbers = crate::model::annotation::numbers(doc);
    let matches: Vec<_> = doc
        .annotations
        .iter()
        .filter(|n| {
            let (root, num) = numbers[&n.id];
            // get_annotations lists notes without a frame as "Detached #n".
            num == number
                && match root.and_then(|id| doc.get(id)) {
                    Some(node) => node.name.eq_ignore_ascii_case(frame),
                    None => frame.eq_ignore_ascii_case("Detached"),
                }
        })
        .collect();
    match matches.as_slice() {
        [note] => Ok(note.id),
        [] => Err(format!(
            "annotation reference `{}` not found",
            key.unwrap_or_default()
        )),
        _ => Err("ambiguous annotation reference; use an annotation id".into()),
    }
}

fn update_annotation(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let id = annotation_ref(
        ed,
        a.get("id")
            .or_else(|| a.get("ref"))
            .ok_or("missing id or ref")?,
        page,
    )?;
    let text_value = a
        .get("text")
        .map(|v| v.as_str().ok_or("text must be a string"))
        .transpose()?;
    let category = a
        .get("type")
        .map(|v| enabled_category(ed, page, v.as_str().ok_or("type must be a string")?))
        .transpose()?;
    ed.with_page(page, |ed| {
        ed.update_annotation(id, |note| {
            if let Some(text) = text_value {
                (note.label, note.body) = annotation_text(text);
            }
            if let Some(category) = category {
                note.category = category;
            }
        })
    });
    text(json!({"id": id}))
}

fn delete_annotations(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let values = a
        .get("ids")
        .or_else(|| a.get("refs"))
        .and_then(Value::as_array)
        .ok_or("missing ids or refs array")?;
    let ids = values
        .iter()
        .map(|v| annotation_ref(ed, v, page))
        .collect::<Result<Vec<_>, _>>()?;
    ed.with_page(page, |ed| {
        ed.edit("Delete annotations", |tx| {
            for id in &ids {
                tx.delete_annotation(*id);
            }
        })
    });
    text(json!({"deleted": ids}))
}

// ------------------------------------------------------------------------------ jsx

fn jsx(host: &mut dyn Host, a: &Args) -> ToolResult {
    let assets_dir = host
        .project()
        .map(|p| p.dir.join(crate::project::ASSETS_DIR));
    let ed = host.editor();
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let doc = ed.page_doc(page);
    match a.str("format") {
        None | Some("") | Some("inline") | Some("inline-styles") => {
            let mut out = super::codegen::annotation_comments(doc, id, &ed.design.categories());
            for import in super::codegen::code_imports(doc, id) {
                out.push_str(&format!("{import}\n"));
            }
            jsx_node(doc, id, 0, true, assets_dir.as_deref(), &mut out);
            Ok(vec![Out::Text(out)])
        }
        Some(
            f @ ("tailwind" | "jsx" | "react" | "tsx" | "angular" | "ng" | "html" | "css" | "egui"
            | "rust"),
        ) => Ok(vec![Out::Text(super::codegen::code_for_target_with_categories(
            doc, id, f, &ed.design.categories(),
        )?)]),
        Some(f) => Err(format!(
            "unknown format `{f}` (inline, tailwind, react, angular, html, or egui)"
        )),
    }
}

fn get_design_spec(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let doc = ed.page_doc(page);
    let target = a.str("target").filter(|s| !s.is_empty()).unwrap_or("react");
    match a.str("format").unwrap_or("markdown") {
        "" | "markdown" | "md" => {
            let md = super::codegen::to_ai_spec_markdown_with_categories(doc, id, target, &ed.design.categories())?;
            Ok(vec![Out::Text(md)])
        }
        "json" => {
            let spec = super::codegen::to_ai_spec_with_categories(doc, id, target, &ed.design.categories())?;
            text(spec)
        }
        other => Err(format!(
            "unknown format `{other}` (expected markdown or json)"
        )),
    }
}

fn verify_code(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let doc = ed.page_doc(page);
    let target = a.req_str("target")?;
    let code = a.req_str("code")?;
    let report = super::enforcer::verify_code(doc, id, target, code)?;
    text(super::enforcer::to_json(&report))
}

fn style_obj(m: &Map<String, Value>) -> String {
    let parts: Vec<String> = m
        .iter()
        .map(|(k, v)| format!("{k}: {}", serde_json::to_string(v).unwrap_or_default()))
        .collect();
    format!("{{{{ {} }}}}", parts.join(", "))
}

fn jsx_text(t: &str) -> String {
    if t.chars()
        .all(|c| !matches!(c, '{' | '}' | '<' | '>' | '\n' | '"' | '\\'))
    {
        t.to_string()
    } else {
        format!("{{{}}}", serde_json::to_string(t).unwrap_or_default())
    }
}

fn jsx_node(
    doc: &Document,
    id: NodeId,
    depth: usize,
    root: bool,
    assets: Option<&Path>,
    out: &mut String,
) {
    let n = doc.get(id).unwrap();
    let ind = "  ".repeat(depth);
    // A linked component is written as its library usage.
    if let Some(jsx) = super::codegen::linked_react(doc, id) {
        super::codegen::push_indented(out, &ind, &jsx);
        return;
    }
    let mut st = css::computed(n, parent_layout(doc, n).as_ref(), n.parent.is_none());
    if root {
        for k in ["position", "left", "top"] {
            st.remove(k);
        }
    }
    let name = n.name.replace('"', "'");
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Group => {
            if n.children.is_empty() {
                out.push_str(&format!(
                    "{ind}<div data-name=\"{name}\" style={} />\n",
                    style_obj(&st)
                ));
            } else {
                out.push_str(&format!(
                    "{ind}<div data-name=\"{name}\" style={}>\n",
                    style_obj(&st)
                ));
                for c in &n.children {
                    jsx_node(doc, *c, depth + 1, false, assets, out);
                }
                out.push_str(&format!("{ind}</div>\n"));
            }
        }
        NodeKind::Text { text, .. } => {
            out.push_str(&format!(
                "{ind}<span style={}>{}</span>\n",
                style_obj(&st),
                jsx_text(text)
            ));
        }
        NodeKind::Rect | NodeKind::Ellipse => {
            if matches!(n.kind, NodeKind::Ellipse) {
                st.insert("borderRadius".into(), json!("50%"));
            }
            let img = n.style.fills.iter().rev().find_map(|p| match p {
                Paint::Image { asset, .. } => Some(asset.clone()),
                _ => None,
            });
            match img {
                Some(asset) => {
                    let ext = crate::assets::get(&asset).map_or("png", |a| a.ext);
                    let src = match assets {
                        Some(d) => d.join(format!("{asset}.{ext}")).display().to_string(),
                        None => format!("paper-asset://{asset}"),
                    };
                    st.remove("background");
                    st.insert("objectFit".into(), json!("cover"));
                    out.push_str(&format!(
                        "{ind}<img src=\"{src}\" alt=\"{name}\" style={} />\n",
                        style_obj(&st)
                    ));
                }
                None => out.push_str(&format!("{ind}<div style={} />\n", style_obj(&st))),
            }
        }
        NodeKind::Sticky { text, .. } => {
            st.insert("padding".into(), json!("16px"));
            out.push_str(&format!(
                "{ind}<div style={}>{}</div>\n",
                style_obj(&st),
                jsx_text(text)
            ));
        }
        _ => {
            // Vectors, lines, arrows, ink: inline SVG of just this node.
            let svg = crate::io::svg::export(doc, Some(&[id]), None).unwrap_or_default();
            let svg = svg
                .lines()
                .filter(|l| !l.starts_with("<?xml"))
                .collect::<Vec<_>>()
                .join("");
            let mut pos = Map::new();
            for k in ["position", "left", "top", "width", "height", "flexShrink"] {
                if let Some(v) = st.get(k) {
                    pos.insert(k.into(), v.clone());
                }
            }
            out.push_str(&format!(
                "{ind}<div data-name=\"{name}\" style={}>{}</div>\n",
                style_obj(&pos),
                svg
            ));
        }
    }
}

// ------------------------------------------------------------------------------ export

#[cfg(not(target_arch = "wasm32"))]
fn parse_scale(s: Option<&str>, b: Rect) -> Result<f64, String> {
    let Some(s) = s.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(2.0);
    };
    let (num, unit) = s.split_at(s.len() - 1);
    let v: f64 = num
        .parse()
        .map_err(|_| format!("bad scale `{s}` (use e.g. 2x, 512w, 512h, 720p)"))?;
    Ok(match unit {
        "x" => v,
        "w" => v / b.w.max(1.0),
        "h" => v / b.h.max(1.0),
        "p" => v / b.w.min(b.h).max(1.0),
        _ => s.parse().map_err(|_| format!("bad scale `{s}`"))?,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn out_dir(a: &Args) -> PathBuf {
    a.str("directory")
        .map(PathBuf::from)
        .or_else(dirs::download_dir)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(target_arch = "wasm32"))]
fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let stem = crate::project::folder_name(stem);
    let mut p = dir.join(format!("{stem}.{ext}"));
    let mut i = 2;
    while p.exists() {
        p = dir.join(format!("{stem} {i}.{ext}"));
        i += 1;
    }
    p
}

/// The browser has no folder to write to: exports there are downloads from the UI.
#[cfg(target_arch = "wasm32")]
const NO_DISK: &str = "this tool writes files to disk, which a browser tab cannot do; \
                       use get_screenshot, or the Export menu in the PaperCraft tab";

#[cfg(target_arch = "wasm32")]
fn export(_: &mut dyn Host, _: &Args) -> ToolResult {
    Err(NO_DISK.into())
}

#[cfg(target_arch = "wasm32")]
fn export_pdf(_: &mut dyn Host, _: &Args) -> ToolResult {
    Err(NO_DISK.into())
}

#[cfg(not(target_arch = "wasm32"))]
fn export(host: &mut dyn Host, a: &Args) -> ToolResult {
    use crate::io::export::{Format, Item, Scope, Spec};
    let gpu = host.gpu();
    let ed = host.editor();
    let def = page_arg(ed, a)?;
    let nodes = a
        .get("nodes")
        .and_then(Value::as_object)
        .ok_or("`nodes` must be an object of nodeId → export settings")?;
    let dir = out_dir(a);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut written = Vec::new();
    for (key, settings) in nodes {
        let (page, id) = resolve(ed, key, def)?;
        let doc = ed.page_doc(page);
        let b = crate::io::svg::export_bounds(doc, &[id]).ok_or("nothing to export")?;
        let name = doc.get(id).unwrap().name.clone();
        let list: Vec<Value> = match settings.as_array() {
            Some(v) if !v.is_empty() => v.clone(),
            _ => vec![json!({"format": "png", "scale": "2x"})],
        };
        for s in list {
            let s = Args(
                s.as_object()
                    .ok_or("each export setting must be an object")?,
            );
            let format = match s.str("format").unwrap_or("png") {
                "png" => Format::Png,
                "svg" => Format::Svg,
                "pdf" => Format::Pdf,
                f => return Err(format!("format `{f}` is not supported (png, svg, pdf)")),
            };
            let scale = parse_scale(s.str("scale"), b)?;
            let stem = if format == Format::Png && scale != 1.0 {
                format!("{name}@{}x", (scale * 100.0).round() / 100.0)
            } else {
                name.clone()
            };
            let path = unique_path(&dir, &stem, format.ext());
            let spec = Spec {
                format,
                scope: Scope::Selection,
                scale,
                transparent: false,
            };
            let item = Item {
                name: name.clone(),
                doc,
                roots: vec![id],
            };
            written.extend(crate::io::export::write(
                &spec,
                &[item],
                &path,
                gpu.as_ref(),
            )?);
        }
    }
    text(json!({"files": written}))
}

#[cfg(not(target_arch = "wasm32"))]
fn export_pdf(host: &mut dyn Host, a: &Args) -> ToolResult {
    use crate::io::export::{Format, Item, Scope, Spec};
    let project_name = host.project().map(|p| p.manifest.name.clone());
    let ed = host.editor();
    let (page, ids) = resolve_all(ed, &a.strs("nodeIds")?, page_arg(ed, a)?)?;
    let doc = ed.page_doc(page);
    let mut ids: Vec<(Rect, NodeId)> = ids
        .into_iter()
        .map(|id| (doc.world_rect(id).unwrap(), id))
        .collect();
    ids.sort_by(|(a, _), (b, _)| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let items: Vec<Item> = ids
        .iter()
        .map(|(_, id)| Item {
            name: doc.get(*id).unwrap().name.clone(),
            doc,
            roots: vec![*id],
        })
        .collect();
    let path = match a.str("path") {
        Some(p) => PathBuf::from(p),
        None => {
            let dir = out_dir(a);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            unique_path(&dir, project_name.as_deref().unwrap_or("papercraft"), "pdf")
        }
    };
    let spec = Spec {
        format: Format::Pdf,
        scope: Scope::Selection,
        scale: 1.0,
        transparent: false,
    };
    let files = crate::io::export::write(&spec, &items, &path, None)?;
    text(json!({"file": files.first(), "pages": items.len()}))
}

// ------------------------------------------------------------------------------ writing

/// Containers whose `align-items` was explicitly not `stretch` (CSS's default). Our auto
/// layout has no stretch alignment, so this remembers which `Start`-aligned frames should not
/// stretch new children. Keyed by page id and node.
fn no_stretch() -> &'static Mutex<HashSet<(String, NodeId)>> {
    static S: OnceLock<Mutex<HashSet<(String, NodeId)>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn stretches(page_id: &str, n: &Node) -> bool {
    n.layout
        .is_some_and(|l| l.align_cross == crate::model::Align::Start)
        && !no_stretch()
            .lock()
            .unwrap()
            .contains(&(page_id.to_string(), n.id))
}

fn remember_stretch(page_id: &str, id: NodeId, styles: &Styles) {
    if let Some((_, v)) = styles.iter().rev().find(|(k, _)| k == "alignItems") {
        let key = (page_id.to_string(), id);
        let mut s = no_stretch().lock().unwrap();
        if matches!(v.trim(), "stretch" | "normal") {
            s.remove(&key);
        } else {
            s.insert(key);
        }
    }
}

/// Recompute a text node's box from its content (inside a transaction).
fn fit_text(tx: &mut Tx, id: NodeId, keep_height: bool) {
    let Some(n) = tx.doc().get(id) else { return };
    let NodeKind::Text { auto_width, .. } = n.kind else {
        return;
    };
    let Some(l) = crate::text::layout_node(n) else {
        return;
    };
    let w = if auto_width {
        l.width.max(1.0)
    } else {
        n.rect.w
    };
    let h = if keep_height { n.rect.h } else { l.height };
    if (n.rect.w - w).abs() > 1e-9 || (n.rect.h - h).abs() > 1e-9 {
        let n = tx.node_mut(id).unwrap();
        n.rect.w = w;
        n.rect.h = h;
    }
}

/// Lay out the outermost auto-layout frame around `id` until it settles (hug sizes depend on
/// wrapped text heights, which depend on fill widths).
fn relayout(tx: &mut Tx, id: Option<NodeId>) {
    let Some(mut top) = id.filter(|i| tx.doc().get(*i).is_some()) else {
        return;
    };
    let mut cur = Some(top);
    while let Some(c) = cur {
        let n = tx.doc().get(c).unwrap();
        if n.layout.is_some() {
            top = c;
        }
        cur = n.parent;
    }
    if tx.doc().get(top).is_some_and(|n| n.layout.is_some()) {
        for _ in 0..6 {
            if !crate::layout::layout_frame(tx, top) {
                break;
            }
        }
    }
}

/// Where new content goes and how it sizes there.
#[derive(Clone)]
struct Parent {
    id: Option<NodeId>,
    layout: Option<AutoLayout>,
    size: Option<(f64, f64)>,
    /// Size along x / y is known (not hugging its content).
    definite: [bool; 2],
    /// Children without an explicit cross size stretch (CSS `align-items: normal`).
    stretch: bool,
}

impl Parent {
    fn of(doc: &Document, page_id: &str, id: Option<NodeId>) -> Parent {
        match id.and_then(|i| doc.get(i)) {
            None => Parent {
                id: None,
                layout: None,
                size: None,
                definite: [false; 2],
                stretch: false,
            },
            Some(n) => Parent {
                id: Some(n.id),
                layout: n.layout,
                size: Some((n.rect.w, n.rect.h)),
                definite: [
                    n.layout_child.sizing[0] != Sizing::Hug,
                    n.layout_child.sizing[1] != Sizing::Hug,
                ],
                stretch: stretches(page_id, n),
            },
        }
    }
    fn ctx(&self) -> Context {
        Context {
            parent_layout: self.layout,
            parent_size: self.size,
        }
    }
    /// The cross axis, when children are laid out.
    fn cross(&self) -> Option<usize> {
        self.layout.map(|l| {
            if l.direction == Direction::Horizontal {
                1
            } else {
                0
            }
        })
    }
}

/// Resolve an image reference (paper-asset://, file://, absolute path, data: URI) to a
/// registered asset id.
fn image_asset(src: &str) -> Result<String, String> {
    let src = src.trim();
    let bytes = if let Some(data) = src.strip_prefix("data:") {
        let (meta, payload) = data.split_once(',').ok_or("malformed data: URI")?;
        if meta.ends_with(";base64") {
            crate::assets::base64_decode(payload.trim()).ok_or("bad base64 in data: URI")?
        } else {
            payload.as_bytes().to_vec()
        }
    } else if src.starts_with("http://") || src.starts_with("https://") {
        return Err(format!(
            "{src}: remote images aren't fetched; download it and use paper-asset:///absolute/path"
        ));
    } else {
        let path = src
            .strip_prefix("paper-asset://")
            .or_else(|| src.strip_prefix("file://"))
            .unwrap_or(src);
        // An asset id already registered (e.g. from get_jsx output).
        if crate::assets::get(path).is_some() {
            return Ok(path.to_string());
        }
        let path = percent_decode(path);
        crate::vfs::read(&path).map_err(|e| format!("{path}: {e}"))?
    };
    if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        return Err("SVG images: put the <svg> markup inline instead".into());
    }
    crate::assets::register(&bytes)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// User-agent defaults for an element (before its inline style).
fn ua_styles(tag: &str) -> Styles {
    let s = |v: &[(&str, &str)]| {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    match tag {
        "h1" => s(&[("fontSize", "32px"), ("fontWeight", "700")]),
        "h2" => s(&[("fontSize", "24px"), ("fontWeight", "700")]),
        "h3" => s(&[("fontSize", "20px"), ("fontWeight", "700")]),
        "h4" | "h5" | "h6" => s(&[("fontWeight", "700")]),
        "table" | "thead" | "tbody" | "tfoot" => {
            s(&[("display", "flex"), ("flexDirection", "column")])
        }
        "tr" => s(&[
            ("display", "flex"),
            ("flexDirection", "row"),
            ("alignItems", "center"),
        ]),
        "td" => s(&[
            ("display", "flex"),
            ("alignItems", "center"),
            ("flexGrow", "1"),
        ]),
        "th" => s(&[
            ("display", "flex"),
            ("alignItems", "center"),
            ("flexGrow", "1"),
            ("fontWeight", "700"),
        ]),
        "b" | "strong" => s(&[("fontWeight", "700")]),
        "i" | "em" | "cite" => s(&[("fontStyle", "italic")]),
        "u" | "ins" => s(&[("textDecoration", "underline")]),
        "s" | "del" => s(&[("textDecoration", "line-through")]),
        "small" => s(&[("fontSize", "13px")]),
        "code" | "kbd" => s(&[("fontSize", "14px")]),
        "pre" => s(&[("fontSize", "14px"), ("whiteSpace", "pre")]),
        "button" | "nav" | "header" | "footer" | "section" | "main" | "article" | "aside" => {
            Vec::new()
        }
        _ => Vec::new(),
    }
}

const INHERITED: &[&str] = &[
    "color",
    "fontSize",
    "fontWeight",
    "fontStyle",
    "lineHeight",
    "letterSpacing",
    "textAlign",
    "fontFamily",
    "whiteSpace",
    "textDecoration",
    "textDecorationLine",
    "textDecorationColor",
    "textDecorationStyle",
];

fn get<'s>(st: &'s Styles, k: &str) -> Option<&'s str> {
    st.iter()
        .rev()
        .find(|(kk, _)| kk == k)
        .map(|(_, v)| v.as_str())
}

/// Inherited text properties for children, with font-size resolved to px.
fn inherit(parent: &Styles, own: &Styles) -> Styles {
    let pfs = get(parent, "fontSize")
        .and_then(|v| css::length(v, 16.0))
        .unwrap_or(16.0);
    let mut out: Styles = parent.clone();
    for (k, v) in own {
        if INHERITED.contains(&k.as_str()) {
            let v = if k == "fontSize" {
                css::length(v, pfs).map_or(v.clone(), |px| format!("{px}px"))
            } else {
                v.clone()
            };
            out.push((k.clone(), v));
        }
    }
    out
}

/// Inline content as a styled character stream.
#[derive(Clone, Copy, Default, PartialEq)]
struct Run {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    underline_color: Option<Color>,
    underline_wavy: Option<bool>,
    strike: Option<bool>,
    color: Option<Color>,
    size: Option<f64>,
}

fn text_only(el: &Element) -> bool {
    el.children.iter().all(|c| match c {
        Html::Text(_) => true,
        Html::Element(e) => html::is_inline(&e.tag) && text_only(e),
    })
}

fn is_flex(st: &Styles) -> bool {
    get(st, "display").is_some_and(|d| d.contains("flex"))
}

fn has_element_children(el: &Element) -> bool {
    el.children
        .iter()
        .any(|c| matches!(c, Html::Element(e) if e.tag != "br"))
}

fn has_text(el: &Element) -> bool {
    el.children.iter().any(|c| match c {
        Html::Text(t) => !t.trim().is_empty(),
        Html::Element(e) => e.tag == "br" || has_text(e),
    })
}

fn collect_runs(nodes: &[Html], run: Run, base_fs: f64, out: &mut Vec<(char, Run)>) {
    for c in nodes {
        match c {
            Html::Text(t) => out.extend(t.chars().map(|ch| (ch, run))),
            Html::Element(e) if e.tag == "br" => out.push(('\u{2028}', run)),
            Html::Element(e) => {
                let mut st = ua_styles(&e.tag);
                st.extend(css::parse_inline(e.attr("style").unwrap_or("")));
                let mut r = run;
                for (k, v) in &st {
                    let lower = v.to_ascii_lowercase();
                    match k.as_str() {
                        "fontWeight" => {
                            r.bold = Some(
                                lower == "bold" || lower.parse::<f64>().is_ok_and(|w| w >= 600.0),
                            )
                        }
                        "fontStyle" => r.italic = Some(lower == "italic"),
                        "textDecoration" | "textDecorationLine" => {
                            r.underline = Some(lower.contains("underline"));
                            r.strike = Some(lower.contains("line-through"));
                            if lower.contains("wavy") {
                                r.underline_wavy = Some(true);
                            }
                            for tok in css::split_ws(v) {
                                if let Some((c, _)) = css::resolve_color_value(&tok) {
                                    r.underline_color = Some(c);
                                }
                            }
                        }
                        "textDecorationColor" => {
                            if let Some((c, _)) = css::resolve_color_value(v) {
                                r.underline_color = Some(c);
                            }
                        }
                        "textDecorationStyle" => {
                            r.underline_wavy = Some(lower.contains("wavy"));
                        }
                        "color" => r.color = css::color(v).or(r.color),
                        "fontSize" => {
                            r.size = css::length(v, run.size.unwrap_or(base_fs)).or(r.size)
                        }
                        _ => {}
                    }
                }
                collect_runs(&e.children, r, base_fs, out);
            }
        }
    }
}

/// Collapse whitespace like CSS `white-space: normal` (hard breaks from `<br>` survive).
fn collapse(chars: Vec<(char, Run)>, pre: bool) -> Vec<(char, Run)> {
    if pre {
        return chars
            .into_iter()
            .map(|(c, r)| (if c == '\u{2028}' { '\n' } else { c }, r))
            .collect();
    }
    let mut out: Vec<(char, Run)> = Vec::new();
    for (c, r) in chars {
        if c == '\u{2028}' {
            while out.last().is_some_and(|(c, _)| *c == ' ') {
                out.pop();
            }
            out.push(('\n', r));
        } else if c.is_whitespace() && c != '\u{a0}' {
            if !out.is_empty() && !matches!(out.last(), Some((' ' | '\n', _))) {
                out.push((' ', r));
            }
        } else {
            out.push((if c == '\u{a0}' { ' ' } else { c }, r));
        }
    }
    while out.last().is_some_and(|(c, _)| *c == ' ') {
        out.pop();
    }
    out
}

fn match_page_key(pid: &str, pname: &str, key: &str) -> bool {
    let key = key.trim();
    pid == key
        || (pid.starts_with(key) && key.len() >= 4)
        || (!pname.is_empty() && pname.eq_ignore_ascii_case(key))
}

struct Builder<'t, 'a> {
    tx: &'t mut Tx<'a>,
    page_id: String,
    all_pages: &'t [(String, String, Document)],
    ignored: BTreeSet<String>,
    warnings: Vec<String>,
}

impl Builder<'_, '_> {
    fn insert(&mut self, n: Node, parent: &Parent, index: &mut Option<usize>) -> NodeId {
        let id = self.tx.insert(n, parent.id, *index);
        if let Some(i) = index {
            *i += 1;
        }
        id
    }

    fn note(&mut self, a: &css::Applied) {
        self.ignored.extend(a.ignored.iter().cloned());
    }

    /// Positioning that needs the final size (right/bottom).
    fn place(&mut self, id: NodeId, a: &css::Applied, parent: &Parent) {
        if let (Some(ps), true) = (parent.size, a.right.is_some() || a.bottom.is_some()) {
            let n = self.tx.node_mut(id).unwrap();
            if let Some(r) = a.right {
                n.rect.x = ps.0 - n.rect.w - r;
            }
            if let Some(b) = a.bottom {
                n.rect.y = ps.1 - n.rect.h - b;
            }
        }
    }

    /// Default sizing before explicit styles: stretch on the cross axis when the parent's
    /// size there is definite.
    fn stretch_default(parent: &Parent, n: &mut Node, styles: &Styles) {
        let Some(ca) = parent.cross() else { return };
        let explicit = get(styles, if ca == 0 { "width" } else { "height" }).is_some()
            || get(styles, "alignSelf").is_some_and(|v| v != "stretch")
            || matches!(get(styles, "position"), Some("absolute" | "fixed"));
        if parent.stretch && parent.definite[ca] && !explicit {
            n.layout_child.sizing[ca] = Sizing::Fill;
            if ca == 0
                && let NodeKind::Text { auto_width, .. } = &mut n.kind
            {
                *auto_width = false;
            }
            if let Some(ps) = parent.size {
                let pad = parent.layout.map_or([0.0; 4], |l| l.padding);
                if ca == 0 {
                    n.rect.w = (ps.0 - pad[1] - pad[3]).max(1.0);
                } else {
                    n.rect.h = (ps.1 - pad[0] - pad[2]).max(1.0);
                }
            }
        }
    }

    /// Lay out a list of children. In a flex container (`flex_items`) every child element and
    /// every text run is its own item (CSS blockification); elsewhere consecutive inline
    /// content flows into one text layer.
    fn nodes(
        &mut self,
        list: &[Html],
        inherited: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
        flex_items: bool,
    ) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut inline: Vec<Html> = Vec::new();
        let flush = |b: &mut Self,
                     inline: &mut Vec<Html>,
                     out: &mut Vec<NodeId>,
                     index: &mut Option<usize>| {
            if inline.is_empty() {
                return;
            }
            let el = Element {
                tag: "span".into(),
                children: std::mem::take(inline),
                ..Default::default()
            };
            if has_text(&el) {
                out.extend(b.text(&el, &Vec::new(), inherited, parent, index));
            }
        };
        for h in list {
            match h {
                Html::Element(e) if flex_items || !html::is_inline(&e.tag) || !text_only(e) => {
                    flush(self, &mut inline, &mut out, index);
                    out.extend(self.element(e, inherited, parent, index));
                }
                other => {
                    inline.push(other.clone());
                    if flex_items {
                        flush(self, &mut inline, &mut out, index);
                    }
                }
            }
        }
        flush(self, &mut inline, &mut out, index);
        out
    }

    fn element(
        &mut self,
        el: &Element,
        inherited: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
    ) -> Option<NodeId> {
        let mut own = ua_styles(&el.tag);
        own.extend(css::parse_inline(el.attr("style").unwrap_or("")));
        match el.tag.as_str() {
            "head" | "meta" | "link" | "title" | "script" | "style" => None,
            "html" | "body" => {
                // Treat as transparent wrappers.
                let inh = inherit(inherited, &own);
                let ids = self.nodes(&el.children, &inh, parent, index, false);
                ids.first().copied()
            }
            "img" => self.image(el, &own, parent, index),
            "svg" => self.svg(el, &own, inherited, parent, index),
            "x-paper-clone" | "paper-clone" => self.clone_el(el, &own, parent, index),
            "x-paper-instance" => self.instance_el(el, &own, parent, index),
            "x-paper-icon" => self.icon_el(el, &own, inherited, parent, index),
            #[cfg(not(target_arch = "wasm32"))]
            "x-plugin" => self.plugin_el(el, &own, inherited, parent, index),
            "hr" => {
                let mut st: Styles = vec![
                    ("height".into(), "1px".into()),
                    ("backgroundColor".into(), "#e0e0e0".into()),
                ];
                st.extend(own);
                self.frame(el, &st, inherited, parent, index, false)
            }
            _ if text_only(el) && has_text(el) && !(is_flex(&own) && has_element_children(el)) => {
                let boxy = own.iter().any(|(k, v)| {
                    k.starts_with("background")
                        || k.starts_with("border") && v != "none" && v != "0"
                        || k.starts_with("padding")
                        || k == "display" && v.contains("flex")
                }) || matches!(el.tag.as_str(), "button" | "li" | "a")
                    && own.iter().any(|(k, _)| k.starts_with("padding"));
                if boxy {
                    self.frame(el, &own, inherited, parent, index, true)
                } else {
                    self.text(el, &own, inherited, parent, index)
                        .into_iter()
                        .next()
                }
            }
            _ => self.frame(el, &own, inherited, parent, index, false),
        }
    }

    fn layer_name(el: &Element) -> Option<String> {
        el.attr("layer-name")
            .or_else(|| el.attr("data-name"))
            .or_else(|| el.attr("aria-label"))
            .or_else(|| el.attr("id"))
            .map(|s| s.chars().take(50).collect())
    }

    /// A text layer for an element whose content is inline only.
    fn text(
        &mut self,
        el: &Element,
        own: &Styles,
        inherited: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
    ) -> Option<NodeId> {
        let mut styles = inherited.clone();
        styles.extend(own.iter().cloned());
        let fs = get(&styles, "fontSize")
            .and_then(|v| css::length(v, 16.0))
            .unwrap_or(16.0);
        let pre = get(&styles, "whiteSpace").is_some_and(|v| v.starts_with("pre"));
        let mut chars = Vec::new();
        collect_runs(&el.children, Run::default(), fs, &mut chars);
        let chars = collapse(chars, pre);
        if chars.is_empty() {
            return None;
        }
        let content: String = chars.iter().map(|(c, _)| *c).collect();
        let mut n = Node::new(
            NodeId(0),
            NodeKind::Text {
                text: content.clone(),
                font_size: fs,
                auto_width: true,
                align: TextAlign::Left,
            },
            Rect::new(0.0, 0.0, 1.0, 1.0),
        );
        n.id = self.tx.alloc_id();
        n.style.set_fill(Some(Color::BLACK));
        n.name = Self::layer_name(el).unwrap_or_else(|| {
            content
                .lines()
                .next()
                .unwrap_or("Text")
                .chars()
                .take(30)
                .collect()
        });
        Self::stretch_default(parent, &mut n, &styles);
        let a = css::apply(&mut n, &styles, &parent.ctx());
        self.note(&a);
        // Rich text runs.
        let mut i = 0;
        while i < chars.len() {
            let r = chars[i].1;
            let mut j = i + 1;
            while j < chars.len() && chars[j].1 == r {
                j += 1;
            }
            if r != Run::default() {
                n.text_style.apply_range(i, j, |s| {
                    if r.bold.is_some() {
                        s.bold = r.bold;
                    }
                    if r.italic.is_some() {
                        s.italic = r.italic;
                    }
                    if r.underline.is_some() {
                        s.underline = r.underline;
                    }
                    if r.underline_color.is_some() {
                        s.underline_color = r.underline_color;
                    }
                    if r.underline_wavy.is_some() {
                        s.underline_wavy = r.underline_wavy;
                    }
                    if r.strike.is_some() {
                        s.strike = r.strike;
                    }
                    if r.color.is_some() {
                        s.color = r.color;
                    }
                    if r.size.is_some() {
                        s.size = r.size;
                    }
                });
            }
            i = j;
        }
        n.text_style.normalize();
        let keep_h = get(own, "height").is_some_and(|v| css::length(v, fs).is_some());
        let id = self.insert(n, parent, index);
        fit_text(self.tx, id, keep_h);
        self.place(id, &a, parent);
        Some(id)
    }

    /// A frame for a container element (`wrap_text`: its inline content becomes one text child).
    fn frame(
        &mut self,
        el: &Element,
        own: &Styles,
        inherited: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
        wrap_text: bool,
    ) -> Option<NodeId> {
        let mut n = Node::new(
            NodeId(0),
            NodeKind::Frame { clip: false },
            Rect::new(0.0, 0.0, 0.0, 0.0),
        );
        n.id = self.tx.alloc_id();
        n.style.fills.clear();
        n.name = Self::layer_name(el).unwrap_or_else(|| match el.tag.as_str() {
            "div" | "span" => "Frame".into(),
            t => {
                let mut c = t.chars();
                c.next()
                    .map_or("Frame".into(), |f| f.to_uppercase().chain(c).collect())
            }
        });
        let flex = get(own, "display").is_some_and(|d| d.contains("flex"));
        n.layout = Some(AutoLayout {
            direction: if flex {
                Direction::Horizontal
            } else {
                Direction::Vertical
            },
            gap: 0.0,
            cross_gap: 0.0,
            padding: [0.0; 4],
            ..AutoLayout::default()
        });
        n.layout_child.sizing = [Sizing::Hug, Sizing::Hug];
        Self::stretch_default(parent, &mut n, own);
        let a = css::apply(&mut n, own, &parent.ctx());
        self.note(&a);
        if let Some((src, mode)) = &a.image {
            match image_asset(src) {
                Ok(asset) => n.style.fills.push(Paint::Image {
                    asset,
                    mode: *mode,
                    opacity: 1.0,
                }),
                Err(e) => self.warnings.push(e),
            }
        }
        let id = self.insert(n, parent, index);
        remember_stretch(&self.page_id, id, own);
        let me = Parent::of(self.tx.doc(), &self.page_id, Some(id));
        let inh = inherit(inherited, own);
        let mut child_index = None;
        if wrap_text {
            let inner = Element {
                tag: "span".into(),
                children: el.children.clone(),
                ..Default::default()
            };
            self.text(&inner, &Vec::new(), &inh, &me, &mut child_index);
        } else {
            self.nodes(&el.children, &inh, &me, &mut child_index, is_flex(own));
        }
        relayout(self.tx, Some(id));
        self.place(id, &a, parent);
        Some(id)
    }

    fn image(
        &mut self,
        el: &Element,
        own: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
    ) -> Option<NodeId> {
        let src = el.attr("src").unwrap_or("");
        let asset = match image_asset(src) {
            Ok(a) => Some(a),
            Err(e) => {
                self.warnings.push(e);
                None
            }
        };
        let natural = asset
            .as_deref()
            .and_then(crate::assets::get)
            .map(|a| (a.width as f64, a.height as f64))
            .unwrap_or((100.0, 100.0));
        let attr_px = |k: &str| el.attr(k).and_then(|v| css::length(v, 16.0));
        let (mut w, mut h) = (attr_px("width"), attr_px("height"));
        if w.is_none() && h.is_none() {
            let s = (1000.0 / natural.0.max(natural.1)).min(1.0);
            (w, h) = (Some(natural.0 * s), Some(natural.1 * s));
        }
        let w2 = w.unwrap_or_else(|| h.unwrap() * natural.0 / natural.1);
        let h2 = h.unwrap_or_else(|| w2 * natural.1 / natural.0);
        let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, w2, h2));
        n.id = self.tx.alloc_id();
        n.name = Self::layer_name(el)
            .or_else(|| el.attr("alt").map(String::from))
            .unwrap_or_else(|| "Image".into());
        n.style.fills = match &asset {
            Some(a) => vec![Paint::Image {
                asset: a.clone(),
                mode: ImageMode::Fill,
                opacity: 1.0,
            }],
            None => vec![Paint::solid(Color::rgb(0xd9, 0xd9, 0xd9))],
        };
        Self::stretch_default(parent, &mut n, own);
        let has_w = get(own, "width").is_some();
        let a = css::apply(&mut n, own, &parent.ctx());
        // Keep the aspect ratio when only the width was styled.
        if has_w && get(own, "height").is_none() && n.layout_child.sizing[0] != Sizing::Fill {
            n.rect.h = n.rect.w * natural.1 / natural.0;
        }
        self.note(&a);
        let id = self.insert(n, parent, index);
        self.place(id, &a, parent);
        Some(id)
    }

    fn svg(
        &mut self,
        el: &Element,
        own: &Styles,
        inherited: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
    ) -> Option<NodeId> {
        let raw = el.raw.clone().unwrap_or_default();
        let color = get(own, "color")
            .or_else(|| get(inherited, "color"))
            .and_then(css::color)
            .unwrap_or(Color::BLACK);
        let fs = 16.0;
        let size_of = |k: &str| {
            get(own, k)
                .and_then(|v| css::length(v, fs))
                .or_else(|| el.attr(k).and_then(|v| css::length(v, fs)))
        };
        let (mut w, mut h) = (size_of("width"), size_of("height"));
        let body = raw.find('>').map_or("", |i| &raw[i + 1..]);
        let self_closing = raw[..raw.find('>').unwrap_or(0)].ends_with('/');
        // The importer keeps viewBox units: fit the viewBox into width × height ourselves, as a
        // browser does (centred, aspect kept), and make the viewBox the final size.
        let vb: Vec<f64> = el.attr("viewBox").map(|v| v.split([' ', ',']).filter_map(|n| n.trim().parse().ok()).collect()).unwrap_or_default();
        let mut fitted = None;
        if let [vx, vy, vw, vh] = vb[..]
            && vw > 0.0
            && vh > 0.0
            && (w.is_some() || h.is_some())
            && !self_closing
        {
            let (fw, fh) = (w.unwrap_or_else(|| h.unwrap() * vw / vh), h.unwrap_or_else(|| w.unwrap() * vh / vw));
            let k = (fw / vw).min(fh / vh);
            let (ox, oy) = ((fw - vw * k) / 2.0, (fh - vh * k) / 2.0);
            let inner = body.trim_end().strip_suffix("</svg>").unwrap_or(body);
            fitted = Some(format!("<g transform=\"translate({ox} {oy}) scale({k}) translate({} {})\">{inner}</g></svg>", -vx, -vy));
            (w, h) = (Some(fw), Some(fh));
        }
        let mut open = String::from("<svg");
        for (k, v) in &el.attrs {
            if matches!(k.as_str(), "width" | "height" | "style") || (fitted.is_some() && k == "viewBox") {
                continue;
            }
            open += &format!(" {k}=\"{}\"", v.replace('"', "&quot;"));
        }
        if fitted.is_some() {
            open += &format!(" viewBox=\"0 0 {} {}\"", w.unwrap(), h.unwrap());
        }
        if !el.attrs.iter().any(|(k, _)| k == "xmlns") {
            open += " xmlns=\"http://www.w3.org/2000/svg\"";
        }
        if let Some(w) = w {
            open += &format!(" width=\"{w}\"");
        }
        if let Some(h) = h {
            open += &format!(" height=\"{h}\"");
        }
        let src = format!("{open}>{}", fitted.as_deref().unwrap_or(if self_closing { "</svg>" } else { body }))
            .replace("currentColor", &color.to_hex());
        // var(--name) inside the markup (fill="var(--accent)", stroke=...) resolves to its value.
        let src = if src.contains("var(--") {
            css::substitute_vars(&src).0
        } else {
            src
        };
        let shapes = match crate::io::svg_import::parse(&src) {
            Ok(s) => s,
            Err(e) => {
                self.warnings.push(format!("<svg>: {e}"));
                return None;
            }
        };
        let b =
            Rect::union_all(&shapes.iter().map(|n| n.rect).collect::<Vec<_>>()).unwrap_or_default();
        let fw = w.unwrap_or(b.x.max(0.0) + b.w);
        let fh = h.unwrap_or(b.y.max(0.0) + b.h);
        let mut f = Node::new(
            NodeId(0),
            NodeKind::Frame { clip: false },
            Rect::new(0.0, 0.0, fw, fh),
        );
        f.id = self.tx.alloc_id();
        f.style.fills.clear();
        f.name = Self::layer_name(el).unwrap_or_else(|| "Icon".into());
        let st: Styles = own
            .iter()
            .filter(|(k, _)| k != "width" && k != "height")
            .cloned()
            .collect();
        let a = css::apply(&mut f, &st, &parent.ctx());
        self.note(&a);
        let fid = self.insert(f, parent, index);
        for mut s in shapes {
            s.id = self.tx.alloc_id();
            s.children.clear();
            self.tx.insert(s, Some(fid), None);
        }
        self.place(fid, &a, parent);
        Some(fid)
    }

    fn instance_el(
        &mut self,
        el: &Element,
        own: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
    ) -> Option<NodeId> {
        let key = el
            .attr("component-id")
            .or_else(|| el.attr("component"))
            .or_else(|| el.attr("name"))
            .unwrap_or("")
            .trim()
            .trim_start_matches('#');
        let (page_part, num_part) = match key.split_once(':') {
            Some((p, n)) => (Some(p), n.parse::<u64>().ok().map(NodeId)),
            None => (None, key.parse::<u64>().ok().map(NodeId)),
        };
        // By id on this page, or by component name (`component="Button / Primary"`).
        let mut main = num_part
            .filter(|_| page_part.is_none_or(|p| match_page_key(&self.page_id, "", p)))
            .filter(|id| self.tx.doc().get(*id).is_some_and(|n| n.component))
            .or_else(|| {
                let mut named: Vec<&Node> = self
                    .tx
                    .doc()
                    .nodes
                    .values()
                    .filter(|n| n.component && n.name.eq_ignore_ascii_case(key))
                    .collect();
                named.sort_by_key(|n| n.id);
                named.first().map(|n| n.id)
            });
        // Not on this page: link to the main on another page, like create_instance does.
        let mut ext = None;
        if main.is_none() {
            ext = self.all_pages.iter().find_map(|(pid, pname, doc)| {
                if pid == &self.page_id {
                    return None;
                }
                if let (Some(p), Some(id)) = (page_part, num_part)
                    && match_page_key(pid, pname, p)
                    && doc.get(id).is_some_and(|n| n.component)
                {
                    return Some((pid, doc, id));
                }
                let mut named: Vec<&Node> = doc
                    .nodes
                    .values()
                    .filter(|n| n.component && n.name.eq_ignore_ascii_case(key))
                    .collect();
                named.sort_by_key(|n| n.id);
                named.first().map(|n| (pid, doc, n.id))
            });
            main = ext.map(|(_, _, id)| id);
        }
        let Some(main) = main else {
            self.warnings.push(format!(
                "x-paper-instance: `{key}` is not a component on this page (create_component first, \
                 or use its exact name)"
            ));
            return None;
        };
        let id = crate::editor::structure::instantiate_from(
            self.tx,
            ext.map(|(_, d, _)| d),
            ext.map(|(pid, _, _)| Source::Page(pid.clone())),
            main,
            parent.id,
            *index,
            None,
        );
        if let Some(i) = index {
            *i += 1;
        }
        let n = self.tx.node_mut(id).unwrap();
        if parent.layout.is_some() {
            n.layout_child.absolute = false;
        }
        let a = css::apply(n, own, &parent.ctx());
        self.note(&a);
        let keys = css::override_keys(own);
        crate::editor::structure::mark_overrides(self.tx, id, &keys);
        if let Some(name) = Self::layer_name(el) {
            self.tx.node_mut(id).unwrap().name = name;
        }
        self.apply_overrides(el, id);
        if let Some(raw) = el.attr("props") {
            match serde_json::from_str::<Map<String, Value>>(raw) {
                Ok(m) => {
                    let values = m.into_iter().map(|(k, v)| (k, v.as_str().map_or_else(|| v.to_string(), str::to_owned))).collect();
                    let defs = self.main_props(id);
                    self.apply_props(id, &defs, &values);
                }
                Err(_) => self.warnings.push("x-paper-instance: `props` must be a JSON object".into()),
            }
        }
        Some(id)
    }

    /// `overrides='{"Label": "Save", "Icon": {"color": "var(--accent)"}}'`: set text (string) or
    /// styles (object) on the instance's layers, found by name.
    fn apply_overrides(&mut self, el: &Element, instance: NodeId) {
        let Some(raw) = el.attr("overrides") else {
            return;
        };
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) else {
            self.warnings
                .push("x-paper-instance: `overrides` must be a JSON object".into());
            return;
        };
        self.apply_override_map(instance, &map);
    }

    /// The layer an override key names under `instance`: `$root`, a layer name, or a path of
    /// names joined by ` > ` (`Lojas > Label`) for repeated names.
    fn override_target(&self, instance: NodeId, key: &str) -> Option<NodeId> {
        if key == "$root" {
            return Some(instance);
        }
        let doc = self.tx.doc();
        key.split(" > ").try_fold(instance, |at, name| doc.subtree(at).into_iter().skip(1).find(|i| doc.get(*i).is_some_and(|n| n.name == name.trim())))
    }

    /// Apply a map of layer key (see [`Self::override_target`]) to a text (string) or styles
    /// (object; `icon` swaps an icon layer's icon, `visibility` shows or hides) on an instance.
    fn apply_override_map(&mut self, instance: NodeId, map: &Map<String, Value>) {
        for (layer, v) in map {
            let Some(target) = self.override_target(instance, layer) else {
                self.warnings.push(format!("no layer named `{layer}` in the instance"));
                continue;
            };
            match v {
                Value::String(t) => {
                    let Some(n) = self.tx.node_mut(target) else {
                        continue;
                    };
                    let Some(slot) = n.kind.text_mut() else {
                        self.warnings.push(format!("`{layer}` is not a text layer"));
                        continue;
                    };
                    *slot = t.clone();
                    crate::editor::structure::mark_overrides(self.tx, target, &["text"]);
                    fit_text(self.tx, target, false);
                }
                Value::Object(o) => {
                    let mut o = o.clone();
                    if let Some(icon) = o.remove("icon").and_then(|i| i.as_str().map(str::to_owned)) {
                        self.swap_icon(target, &icon);
                    }
                    if let Some(t) = o.remove("text").and_then(|t| t.as_str().map(str::to_owned)) {
                        self.apply_override_map(instance, &[(layer.clone(), json!(t))].into_iter().collect());
                    }
                    let Ok(styles) = css::from_json(&Value::Object(o)) else {
                        continue;
                    };
                    if styles.is_empty() {
                        continue;
                    }
                    let parent = Parent::of(
                        self.tx.doc(),
                        &self.page_id,
                        self.tx.doc().get(target).and_then(|n| n.parent),
                    );
                    let mut n = self.tx.doc().get(target).unwrap().clone();
                    let a = css::apply(&mut n, &styles, &parent.ctx());
                    self.note(&a);
                    *self.tx.node_mut(target).unwrap() = n;
                    let keys = css::override_keys(&styles);
                    crate::editor::structure::mark_overrides(self.tx, target, &keys);
                    recolor_vectors(self.tx, target, &styles);
                    fit_text(self.tx, target, false);
                }
                _ => {}
            }
        }
    }

    /// Replace the artwork of icon layer `target` with icon `name` (`set:name`), same size and
    /// colour; the layer keeps its place, name and link, and its new content is an override.
    fn swap_icon(&mut self, target: NodeId, name: &str) {
        let Some(n) = self.tx.doc().get(target).cloned() else { return };
        let color = vector_color(self.tx.doc(), target).map_or_else(|| "currentColor".to_owned(), |c| c.to_hex());
        let markup = format!("<x-paper-icon name=\"{name}\" size=\"{}\" color=\"{color}\"/>", n.rect.w.max(n.rect.h));
        let Some(Html::Element(el)) = html::parse(&markup).into_iter().next() else { return };
        let parent = Parent::of(self.tx.doc(), &self.page_id, Some(target));
        let Some(made) = self.icon_el(&el, &Vec::new(), &Vec::new(), &parent, &mut None) else { return };
        for old in n.children.clone() {
            self.tx.remove(old);
        }
        for k in self.tx.doc().get(made).map(|m| m.children.clone()).unwrap_or_default() {
            self.tx.reparent(k, Some(target), None);
        }
        self.tx.remove(made);
        // The layer keeps its name: properties find it by that name.
        crate::editor::structure::mark_overrides(self.tx, target, &["children"]);
    }

    /// Apply property `values` (by name) of an instance, using its main's definitions: each
    /// becomes overrides on its layers. The values are kept on the instance root.
    fn apply_props(&mut self, root: NodeId, defs: &[crate::model::PropDef], values: &BTreeMap<String, String>) {
        for (name, value) in values {
            let Some(d) = defs.iter().find(|d| d.name == *name) else {
                self.warnings.push(format!("the component has no property `{name}`"));
                continue;
            };
            let layer = d.layer.clone().unwrap_or_default();
            let map: Map<String, Value> = match d.kind.as_str() {
                "text" => [(layer, json!(value))].into_iter().collect(),
                "bool" => [(layer, json!({"visibility": if value == "true" { "visible" } else { "hidden" }}))].into_iter().collect(),
                "icon" => [(layer, json!({"icon": value}))].into_iter().collect(),
                _ => match d.options.iter().find(|o| o.value == *value) {
                    Some(o) => o.set.as_object().cloned().unwrap_or_default(),
                    None => {
                        let opts: Vec<&str> = d.options.iter().map(|o| o.value.as_str()).collect();
                        self.warnings.push(format!("`{value}` is not an option of `{name}` ({})", opts.join(", ")));
                        continue;
                    }
                },
            };
            self.apply_override_map(root, &map);
        }
        if let Some(n) = self.tx.node_mut(root) {
            n.prop_values.extend(values.clone());
        }
    }

    /// The property definitions of the main an instance root follows (this page or another).
    fn main_props(&self, root: NodeId) -> Vec<crate::model::PropDef> {
        let Some(l) = self.tx.doc().get(root).and_then(|n| n.instance.clone()) else { return Vec::new() };
        let (main, source) = l.target();
        let doc = match source {
            None => Some(self.tx.doc()),
            Some(Source::Page(p)) => self.all_pages.iter().find(|(id, _, _)| id == p).map(|(_, _, d)| d),
            Some(Source::Library(_)) => None,
        };
        doc.and_then(|d| d.get(main)).map(|m| m.props.clone()).unwrap_or_default()
    }

    /// `<x-plugin use="avataaars" size="64" …/>`: an asset from a plugin the user turned on
    /// (`crate::plugins`), as vector layers (SVG) or an image.
    #[cfg(not(target_arch = "wasm32"))]
    fn plugin_el(
        &mut self,
        el: &Element,
        own: &Styles,
        inherited: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
    ) -> Option<NodeId> {
        use crate::plugins::{self, Kind};
        let id = el.attr("use").unwrap_or("");
        let mut fail = |e: String| {
            self.warnings.push(format!("x-plugin: {e}"));
            None
        };
        let Some(p) = plugins::plugin(id) else {
            let known: Vec<&str> = plugins::PLUGINS.iter().map(|p| p.id).collect();
            return fail(format!("no plugin `{id}` (plugins: {})", known.join(", ")));
        };
        if !crate::ui::settings::ChatPrefs::load().plugins.iter().any(|x| x == p.id) {
            return fail(format!("the {} plugin is off; the user can turn it on in Settings, Chat, Plugins", p.name));
        }
        let (w, h) = plugins::layer_size(p, &el.attrs, 1.0);
        let url = match plugins::url(p, &el.attrs, w, h) {
            Ok(u) => u,
            Err(e) => return fail(e),
        };
        let file = match plugins::fetch(&url, p.kind) {
            Ok(f) => f,
            Err(e) => return fail(format!("{}: {e}", p.name)),
        };
        let label = Self::layer_name(el).unwrap_or_else(|| format!("{} / {}", p.name, el.attr("seed").or(el.attr("icon")).or(el.attr("code")).unwrap_or("asset")));
        let placed = match p.kind {
            Kind::Raster => {
                let img = Element { tag: "img".into(), attrs: vec![("src".into(), file.display().to_string()), ("width".into(), w.to_string()), ("height".into(), h.to_string())], children: vec![], raw: None };
                self.image(&img, own, parent, index)
            }
            Kind::Svg => {
                let svg = crate::vfs::read_to_string(&file).unwrap_or_default();
                let Some(mut svg_el) = html::parse(&svg).into_iter().find_map(|h| match h {
                    Html::Element(e) if e.tag == "svg" => Some(e),
                    _ => None,
                }) else {
                    return fail(format!("{} did not return an SVG", p.name));
                };
                // No viewBox (flagcdn's jp.svg has only width and height): the size is the box.
                let px = |k: &str| svg_el.attr(k).and_then(|v| v.trim_end_matches("px").parse::<f64>().ok());
                let vb: Vec<f64> = match svg_el.attr("viewBox") {
                    Some(v) => v.split([' ', ',']).filter_map(|n| n.parse().ok()).collect(),
                    None => px("width").zip(px("height")).map(|(w, h)| vec![0.0, 0.0, w, h]).unwrap_or_default(),
                };
                // The importer keeps viewBox units: scale the artwork to the layer size.
                if let [vx, vy, vw, vh] = vb[..]
                    && vw > 0.0
                    && vh > 0.0
                    && let Some(raw) = svg_el.raw.take()
                {
                    let (lw, lh) = plugins::layer_size(p, &el.attrs, vh / vw);
                    let body = raw.find('>').map_or("", |i| &raw[i + 1..]);
                    let inner = body.trim_end().strip_suffix("</svg>").unwrap_or(body);
                    let k = lw / vw;
                    let defs_ns = "xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"";
                    svg_el.raw = Some(format!("<svg {defs_ns}><g transform=\"scale({k}) translate({} {})\">{inner}</g></svg>", -vx, -vy));
                    svg_el.attrs.retain(|(k, _)| !matches!(k.as_str(), "viewBox" | "width" | "height"));
                    svg_el.attrs.push(("viewBox".into(), format!("0 0 {lw} {lh}")));
                    svg_el.attrs.push(("width".into(), lw.to_string()));
                    svg_el.attrs.push(("height".into(), lh.to_string()));
                }
                self.svg(&svg_el, own, inherited, parent, index)
            }
        }?;
        self.tx.node_mut(placed).unwrap().name = label;
        Some(placed)
    }

    /// `<x-paper-icon name="lucide:check" size="20" color="var(--text)" stroke-width="2"/>`.
    fn icon_el(
        &mut self,
        el: &Element,
        own: &Styles,
        inherited: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
    ) -> Option<NodeId> {
        let name = el.attr("name").unwrap_or("");
        let svg = match crate::icons::load(&crate::icons::dir(), name) {
            Ok(s) => s,
            Err(e) => {
                self.warnings.push(format!("x-paper-icon: {e}"));
                return None;
            }
        };
        let Some(mut svg_el) = html::parse(&svg).into_iter().find_map(|h| match h {
            Html::Element(e) if e.tag == "svg" => Some(e),
            _ => None,
        }) else {
            self.warnings
                .push(format!("x-paper-icon: `{name}` is not an SVG file"));
            return None;
        };
        let size = el
            .attr("size")
            .or_else(|| el.attr("width"))
            .and_then(|s| css::length(s, 16.0))
            .unwrap_or(24.0);
        // The color can be a `var(--x)` that does not exist. Report it and use the
        // inherited color, so the icon stays visible instead of turning black.
        let color_src = el
            .attr("color")
            .or_else(|| get(own, "color"))
            .or_else(|| get(inherited, "color"));
        let (color, var) = match color_src.and_then(css::resolve_color_value) {
            Some(cv) => cv,
            None => {
                let fallback = get(inherited, "color").and_then(css::resolve_color_value);
                if let Some((name, _)) = color_src.and_then(|v| css::var_name(v)) {
                    self.warnings.push(format!(
                        "x-paper-icon: no variable `--{name}`; {}",
                        if fallback.is_some() {
                            "using the inherited color"
                        } else {
                            "using black"
                        }
                    ));
                }
                match fallback {
                    Some((c, v)) => (c, v),
                    None => (Color::BLACK, None),
                }
            }
        };
        svg_el
            .attrs
            .retain(|(k, _)| !matches!(k.as_str(), "width" | "height" | "class"));
        // The importer keeps viewBox units, so bring the artwork to `size` ourselves: wrap it in
        // a scale group and make the viewBox match the final size.
        let vb: Vec<f64> = svg_el
            .attr("viewBox")
            .map(|v| v.split([' ', ',']).filter_map(|n| n.parse().ok()).collect())
            .unwrap_or_default();
        if let [vx, vy, vw, vh] = vb[..]
            && vw > 0.0
            && vh > 0.0
            && let Some(raw) = svg_el.raw.take()
        {
            let body = raw.find('>').map_or("", |i| &raw[i + 1..]);
            let inner = body.trim_end().strip_suffix("</svg>").unwrap_or(body);
            let k = size / vw.max(vh);
            svg_el.raw = Some(format!(
                "<svg><g transform=\"scale({k}) translate({} {})\">{inner}</g></svg>",
                -vx, -vy
            ));
            svg_el.attrs.retain(|(k, _)| k != "viewBox");
            svg_el
                .attrs
                .push(("viewBox".into(), format!("0 0 {size} {size}")));
        }
        svg_el.attrs.push(("width".into(), size.to_string()));
        svg_el.attrs.push(("height".into(), size.to_string()));
        if let Some(sw) = el.attr("stroke-width") {
            svg_el.attrs.retain(|(k, _)| k != "stroke-width");
            svg_el.attrs.push(("stroke-width".into(), sw.to_string()));
        }
        let mut st: Styles = own.clone();
        st.retain(|(k, _)| k != "color");
        st.push(("color".into(), color.to_hex()));
        let id = self.svg(&svg_el, &st, inherited, parent, index)?;
        let label = Self::layer_name(el).unwrap_or_else(|| format!("Icon / {name}"));
        self.tx.node_mut(id).unwrap().name = label;
        if let Some(var) = var {
            for sid in self.tx.doc().subtree(id) {
                let linked = self.tx.doc().get(sid).is_some_and(|n| {
                    n.style
                        .fills
                        .iter()
                        .any(|p| matches!(p, Paint::Solid { color: c, var: None } if *c == color))
                });
                if linked && let Some(n) = self.tx.node_mut(sid) {
                    for p in &mut n.style.fills {
                        if matches!(p, Paint::Solid { color: c, var: None } if *c == color) {
                            *p = Paint::solid_var(color, &var);
                        }
                    }
                }
            }
        }
        Some(id)
    }

    fn clone_el(
        &mut self,
        el: &Element,
        own: &Styles,
        parent: &Parent,
        index: &mut Option<usize>,
    ) -> Option<NodeId> {
        let key = el
            .attr("node-id")
            .unwrap_or("")
            .trim()
            .trim_start_matches('#');
        let (page_part, num) = match key.split_once(':') {
            Some((p, n)) => (Some(p), n.parse::<u64>().ok().map(NodeId)),
            None => (None, key.parse::<u64>().ok().map(NodeId)),
        };
        let Some(src) = num else {
            self.warnings
                .push(format!("x-paper-clone: node `{key}` not found"));
            return None;
        };
        let same_page = page_part.is_none_or(|p| match_page_key(&self.page_id, "", p));
        let id = if same_page && self.tx.doc().get(src).is_some() {
            clone_subtree(self.tx, src, parent.id, *index, V2::ZERO).0
        } else if let Some((_, _, src_doc)) =
            self.all_pages
                .iter()
                .find(|(pid, pname, doc)| match page_part {
                    Some(p) => match_page_key(pid, pname, p) && doc.get(src).is_some(),
                    None => pid != &self.page_id && doc.get(src).is_some(),
                })
        {
            let w = src_doc
                .world_rect(src)
                .unwrap_or_else(|| src_doc.get(src).unwrap().rect);
            let pid = self.all_pages.iter().find(|(_, _, d)| std::ptr::eq(d, src_doc)).map(|(pid, _, _)| pid.as_str());
            clone_subtree_from_doc(self.tx, src_doc, src, parent.id, *index, -w.min(), pid.map(|p| (p, self.page_id.as_str())), false).0
        } else {
            self.warnings
                .push(format!("x-paper-clone: node `{key}` not found"));
            return None;
        };
        if let Some(i) = index {
            *i += 1;
        }
        let ctx = parent.ctx();
        let n = self.tx.node_mut(id).unwrap();
        if parent.layout.is_some() {
            n.layout_child.absolute = false;
        }
        let a = css::apply(n, own, &ctx);
        self.note(&a);
        if let Some(name) = Self::layer_name(el) {
            self.tx.node_mut(id).unwrap().name = name;
        }
        Some(id)
    }
}

/// Deep-copy a subtree from `src_doc` into `tx` under `parent` (keeping its world position plus
/// `offset`). Returns the new root and the (old → new) id pairs.
/// `pages`: the (source, destination) page ids, so links to mains left on the source page and
/// links that become same-page are kept. `keep_keys`: a move, so mains keep their component keys.
#[allow(clippy::too_many_arguments)]
fn clone_subtree_from_doc(
    tx: &mut Tx,
    src_doc: &Document,
    src: NodeId,
    parent: Option<NodeId>,
    index: Option<usize>,
    offset: V2,
    pages: Option<(&str, &str)>,
    keep_keys: bool,
) -> (NodeId, Vec<(NodeId, NodeId)>) {
    let ids = src_doc.subtree(src);
    let nodes: Vec<Node> = ids
        .iter()
        .map(|i| src_doc.get(*i).unwrap().clone())
        .collect();
    let mut map = HashMap::new();
    for n in &nodes {
        map.insert(n.id, tx.alloc_id());
    }
    let world = src_doc
        .world_rect(src)
        .unwrap_or_else(|| src_doc.get(src).unwrap().rect);
    let origin = tx.doc().parent_origin(parent);
    for n in &nodes {
        let mut c = n.clone();
        c.id = map[&n.id];
        c.children = n
            .children
            .iter()
            .filter_map(|x| map.get(x).copied())
            .collect();
        if c.component && c.component_key.is_some() && !keep_keys {
            c.component_key = Some(crate::variables::new_id());
        }
        // Only a same-page link can point into the copied subtree; one to a main left on the
        // source page becomes a cross-page link, one to the destination page a same-page link.
        if let Some(l) = &mut c.instance {
            match &l.source {
                None => {
                    if let Some(m) = map.get(&l.main) {
                        l.main = *m;
                    } else if let Some((from, _)) = pages {
                        l.source = Some(Source::Page(from.to_owned()));
                    } else if !tx.doc().get(l.main).is_some_and(|mn| mn.component) {
                        c.instance = None;
                    }
                }
                Some(Source::Page(p)) if pages.is_some_and(|(_, to)| to == p) => l.source = None,
                _ => {}
            }
        }
        if let NodeKind::Connector { start, end, .. } = &mut c.kind {
            for b in [start, end].into_iter().flatten() {
                if let Some(nn) = map.get(&b.node) {
                    b.node = *nn;
                }
            }
        }
        if n.id == src {
            c.rect = world.translate(offset - origin);
            tx.insert(c, parent, index);
        } else {
            c.parent = n.parent.and_then(|p| map.get(&p).copied());
            tx.put(c);
        }
    }
    for id in map.values() {
        crate::editor::whiteboard::update_connector(tx, *id);
    }
    let pairs = ids.iter().map(|i| (*i, map[i])).collect();
    (map[&src], pairs)
}

/// Deep-copy a subtree under `parent` (keeping its world position plus `offset`). Returns the
/// new root and the (old → new) id pairs.
fn clone_subtree(
    tx: &mut Tx,
    src: NodeId,
    parent: Option<NodeId>,
    index: Option<usize>,
    offset: V2,
) -> (NodeId, Vec<(NodeId, NodeId)>) {
    let ids = tx.doc().subtree(src);
    let nodes: Vec<Node> = ids
        .iter()
        .map(|i| tx.doc().get(*i).unwrap().clone())
        .collect();
    let mut map = HashMap::new();
    for n in &nodes {
        map.insert(n.id, tx.alloc_id());
    }
    let world = tx.doc().world_rect(src).unwrap();
    let origin = tx.doc().parent_origin(parent);
    for n in &nodes {
        let mut c = n.clone();
        c.id = map[&n.id];
        c.children = n.children.iter().map(|x| map[x]).collect();
        if c.component && c.component_key.is_some() {
            c.component_key = Some(crate::variables::new_id());
        }
        if let NodeKind::Connector { start, end, .. } = &mut c.kind {
            for b in [start, end].into_iter().flatten() {
                if let Some(nn) = map.get(&b.node) {
                    b.node = *nn;
                }
            }
        }
        if n.id == src {
            c.rect = world.translate(offset - origin);
            tx.insert(c, parent, index);
        } else {
            c.parent = n.parent.map(|p| map[&p]);
            tx.put(c);
        }
    }
    for id in map.values() {
        crate::editor::whiteboard::update_connector(tx, *id);
    }
    let pairs = ids.iter().map(|i| (*i, map[i])).collect();
    (map[&src], pairs)
}

/// Copy the annotations on a duplicated subtree, retargeted to the new nodes.
fn copy_annotations(tx: &mut Tx, from: &[crate::model::annotation::Annotation], pairs: &[(NodeId, NodeId)]) {
    let map: HashMap<NodeId, NodeId> = pairs.iter().copied().collect();
    let copies = crate::model::annotation::copies_for(from, &map);
    tx.add_annotation_copies(copies);
}

/// A free spot for a new top-level frame: right of the existing artboards, 80 px apart.
fn free_spot(doc: &Document, w: f64) -> V2 {
    let rects: Vec<Rect> = doc
        .root
        .iter()
        .filter_map(|id| doc.world_rect(*id))
        .collect();
    match Rect::union_all(&rects) {
        None => V2::ZERO,
        Some(b) => {
            let _ = w;
            v2((b.x + b.w + 80.0).round(), b.y.round())
        }
    }
}

fn create_artboard(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let name = a.str("name").unwrap_or("Artboard").to_string();
    let styles = css::from_json(
        a.get("styles")
            .ok_or("missing `styles` (width and height are required)")?,
    )?;
    if get(&styles, "width").is_none() || get(&styles, "height").is_none() {
        return Err("styles.width and styles.height are required (e.g. \"1440px\")".into());
    }
    let page_id = ed.pages[page].id.clone();
    let (id, ignored) = ed.with_page(page, |ed| {
        ed.edit("Create artboard", |tx| {
            let mut n = Node::new(
                NodeId(0),
                NodeKind::Frame { clip: true },
                Rect::new(0.0, 0.0, 100.0, 100.0),
            );
            n.id = tx.alloc_id();
            n.name = name.chars().take(50).collect();
            n.layout = Some(AutoLayout {
                direction: Direction::Vertical,
                gap: 0.0,
                cross_gap: 0.0,
                padding: [0.0; 4],
                ..AutoLayout::default()
            });
            let a = css::apply(&mut n, &styles, &Context::default());
            let mut ignored = a.ignored.clone();
            if let Some((src, mode)) = &a.image {
                match image_asset(src) {
                    Ok(asset) => n.style.fills.push(Paint::Image {
                        asset,
                        mode: *mode,
                        opacity: 1.0,
                    }),
                    Err(e) => ignored.push(e),
                }
            }
            if get(&styles, "left").is_none() && get(&styles, "top").is_none() {
                let p = free_spot(tx.doc(), n.rect.w);
                n.rect.x = p.x;
                n.rect.y = p.y;
            }
            let id = tx.insert(n, None, None);
            remember_stretch(&page_id, id, &styles);
            relayout(tx, Some(id));
            (id, ignored)
        })
    });
    let mut v = json!({"id": node_key(ed, page, id), "name": name, "pageId": ed.pages[page].id});
    if !ignored.is_empty() {
        v["ignoredStyles"] = json!(ignored);
    }
    text(v)
}

fn write_html(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let src = a.req_str("html")?;
    let target = a.req_str("targetNodeId")?;
    let mode = a.str("mode").unwrap_or("insert-children");
    let (page, target) = if target == ROOT {
        (def, None)
    } else {
        let (p, id) = resolve(ed, target, def)?;
        (p, Some(id))
    };
    let replace = match mode {
        "insert-children" | "insert" | "append" => false,
        "replace" => true,
        m => {
            return Err(format!(
                "mode must be \"insert-children\" or \"replace\", not `{m}`"
            ));
        }
    };
    if replace && target.is_none() {
        return Err("replace needs a targetNodeId (not \"root\")".into());
    }
    if let Some(t) = target
        && !replace
        && !ed.page_doc(page).get(t).unwrap().is_container()
    {
        return Err("insert-children needs a frame or group target; use mode \"replace\" or target its parent".into());
    }
    let tree = html::parse(src);
    #[cfg(not(target_arch = "wasm32"))]
    crate::plugins::prefetch(&tree, &crate::ui::settings::ChatPrefs::load().plugins);
    let page_id = ed.pages[page].id.clone();
    let all_pages: Vec<(String, String, Document)> = ed
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.clone(), p.name.clone(), ed.page_doc(i).clone()))
        .collect();
    let (created, ignored, warnings) = ed.with_page(page, |ed| {
        ed.edit("Write HTML", |tx| {
            // Where the new content goes.
            let (parent_id, mut index, old) = match (replace, target) {
                (true, Some(t)) => {
                    let n = tx.doc().get(t).unwrap();
                    let p = n.parent;
                    let i = tx.doc().children(p).iter().position(|c| *c == t);
                    (p, i, Some(n.clone()))
                }
                (_, t) => (t, None, None),
            };
            if let Some(t) = target.filter(|_| replace) {
                tx.remove(t);
            }
            let parent = Parent::of(tx.doc(), &page_id, parent_id);
            let mut inherited: Styles = Vec::new();
            // Text inherits the target's text color when inserting into a text-less frame.
            if let Some(o) = &old
                && let Some(c) = o.style.fill().filter(|_| o.kind.text().is_some())
            {
                inherited.push(("color".into(), c.to_hex()));
            }
            let mut b = Builder {
                tx,
                page_id: page_id.clone(),
                all_pages: &all_pages,
                ignored: BTreeSet::new(),
                warnings: Vec::new(),
            };
            let flex = parent.layout.is_some();
            let made = b.nodes(&tree, &inherited, &parent, &mut index, flex);
            // An instance written over a nested instance becomes a swap override: it keeps the
            // old node's place in the outer instance, so the next sync of the outer main keeps it.
            if let (Some(o), [id]) = (&old, made.as_slice())
                && let Some(place) = o.instance.clone()
                && parent_id.and_then(|p| b.tx.doc().get(p)).is_some_and(|p| p.instance.is_some())
                && let Some(n) = b.tx.node_mut(*id)
                && let Some(l) = n.instance.as_mut()
            {
                let to = l.swap.take().unwrap_or(Swap { main: l.main, source: l.source.take() });
                *l = InstanceLink { main: place.main, source: place.source, swap: Some(to), overrides: std::mem::take(&mut l.overrides) };
            }
            // Root-level content without a position goes where the replaced node was, or
            // into free space.
            if parent_id.is_none() {
                for (k, id) in made.iter().enumerate() {
                    let n = b.tx.doc().get(*id).unwrap();
                    if let Some(o) = &old
                        && k == 0
                    {
                        let (x, y) = (o.rect.x, o.rect.y);
                        let n = b.tx.node_mut(*id).unwrap();
                        n.rect.x = x;
                        n.rect.y = y;
                    } else if n.rect.x == 0.0 && n.rect.y == 0.0 {
                        let w = n.rect.w;
                        let p = free_spot(&doc_without(b.tx.doc(), *id), w);
                        let n = b.tx.node_mut(*id).unwrap();
                        n.rect.x = p.x;
                        n.rect.y = p.y;
                    }
                }
            }
            relayout(b.tx, parent_id.or(made.first().copied()));
            let (ig, w) = (b.ignored, b.warnings);
            (made, ig, w)
        })
    });
    if created.is_empty() && !warnings.is_empty() {
        return Err(warnings.join("; "));
    }
    let mut v = json!({
        "createdNodeIds": created.iter().map(|id| node_key(ed, page, *id)).collect::<Vec<_>>(),
        "pageId": ed.pages[page].id,
    });
    if let Some(t) = target {
        let parent = ed
            .page_doc(page)
            .get(created.first().copied().unwrap_or(t))
            .and_then(|n| n.parent);
        v["parentId"] = json!(parent.map_or(ROOT.into(), |p| node_key(ed, page, p)));
    }
    if !ignored.is_empty() {
        v["ignoredStyles"] = json!(ignored);
    }
    if !warnings.is_empty() {
        v["warnings"] = json!(warnings);
    }
    text(v)
}

/// The page without one root node (for placing that node in free space).
fn doc_without(doc: &Document, id: NodeId) -> Document {
    Document {
        root: doc.root.iter().copied().filter(|r| *r != id).collect(),
        ..doc.clone()
    }
}

fn head(s: Option<&str>, default: Head) -> Result<Head, String> {
    Ok(match s.map(str::to_ascii_lowercase).as_deref() {
        None => default,
        Some("none") => Head::None,
        Some("arrow") => Head::Arrow,
        Some("triangle") => Head::Triangle,
        Some("dot" | "circle") => Head::Dot,
        Some("bar") => Head::Bar,
        Some("diamond") => Head::Diamond,
        Some(o) => return Err(format!("unknown arrowhead `{o}`")),
    })
}

fn create_shape(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let ty = a.req_str("type")?.to_ascii_lowercase();
    let (mut page, mut parent) = (def, None);
    if let Some(p) = a.str("parentId").filter(|p| !p.is_empty() && *p != ROOT) {
        let (pg, id) = resolve(ed, p, def)?;
        if !ed.page_doc(pg).get(id).unwrap().is_container() {
            return Err("parentId must be a frame or group".into());
        }
        (page, parent) = (pg, Some(id));
    }
    let bind = |k: &str| -> Result<Option<NodeId>, String> {
        match a.str(k).filter(|s| !s.is_empty()) {
            None => Ok(None),
            Some(s) => {
                let (pg, id) = resolve(ed, s, page)?;
                if pg != page {
                    return Err(format!("{k} is on another page"));
                }
                Ok(Some(id))
            }
        }
    };
    let (start_node, end_node) = (bind("startNodeId")?, bind("endNodeId")?);
    let points: Vec<V2> = a
        .get("points")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some(v2(p.first()?.as_f64()?, p.get(1)?.as_f64()?))
                })
                .collect()
        })
        .unwrap_or_default();
    let styles = match a.get("styles") {
        Some(s) => css::from_json(s)?,
        None => Vec::new(),
    };
    let sides = a.num("sides").map(|s| s.clamp(3.0, 64.0) as u32);
    let txt = a.str("text").unwrap_or("").to_string();
    // A top-level shape with no position goes where a new artboard would, not on top of
    // the first one at the origin.
    let free = parent.is_none() && a.num("x").is_none() && a.num("y").is_none();
    let (x, y) = (a.num("x").unwrap_or(0.0), a.num("y").unwrap_or(0.0));
    let (w, h) = (a.num("width"), a.num("height"));
    let route = match a.str("route") {
        Some("curved") => Route::Curved,
        Some("elbow") => Route::Elbow,
        _ => Route::Straight,
    };
    let (start_head, end_head) = (
        head(a.str("startHead"), Head::None)?,
        head(a.str("endHead"), Head::Arrow)?,
    );
    let name = a.str("name").map(String::from);
    let sketch = ed.sketch;
    let page_id = ed.pages[page].id.clone();
    let (id, ignored) = ed.with_page(page, |ed| {
        ed.edit("Add shape", |tx| -> Result<(NodeId, Vec<String>), String> {
            let o = tx.doc().parent_origin(parent);
            let (x, y) = if free {
                let p = free_spot(tx.doc(), w.unwrap_or(160.0));
                (p.x, p.y)
            } else {
                (x, y)
            };
            let rect = Rect::new(x, y, w.unwrap_or(160.0), h.unwrap_or(100.0));
            let kind = match ty.as_str() {
                "rectangle" | "rect" => NodeKind::Rect,
                "ellipse" | "circle" => NodeKind::Ellipse,
                "diamond" => NodeKind::Polygon { sides: 4 },
                "triangle" => NodeKind::Polygon { sides: 3 },
                "polygon" => NodeKind::Polygon { sides: sides.unwrap_or(6) },
                "star" => NodeKind::Star { points: sides.unwrap_or(5), inner: 0.38 },
                "sticky" | "note" => NodeKind::Sticky { text: txt.clone(), font_size: 24.0 },
                "text" => NodeKind::Text { text: txt.clone(), font_size: 20.0, auto_width: w.is_none(), align: TextAlign::Left },
                "line" | "arrow" => NodeKind::Line { a: V2::ZERO, b: V2::ZERO },
                t => return Err(format!("unknown shape type `{t}`")),
            };
            let mut n = Node::new(NodeId(0), kind, rect);
            n.id = tx.alloc_id();
            if ty == "sticky" || ty == "note" {
                n.rect.w = w.unwrap_or(200.0);
                n.rect.h = h.unwrap_or(200.0);
            }
            if let Some(nm) = &name {
                n.name = nm.chars().take(50).collect();
            } else if !txt.is_empty() {
                n.name = txt.chars().take(30).collect();
            }
            if ty == "line" || ty == "arrow" {
                // World points (parent-relative when a parent is given), else between the bound shapes.
                let mut pts: Vec<V2> = points.iter().map(|p| *p + o).collect();
                let center = |id: Option<NodeId>| id.and_then(|i| tx.doc().world_rect(i)).map(|r| r.center());
                if pts.len() < 2 {
                    match (center(start_node), center(end_node)) {
                        (Some(s), Some(e)) => pts = vec![s, e],
                        _ => return Err("lines and arrows need `points` ([[x1,y1],[x2,y2]]) or both startNodeId and endNodeId".into()),
                    }
                }
                let b = Rect::union_all(&pts.iter().map(|p| Rect::new(p.x, p.y, 0.0, 0.0)).collect::<Vec<_>>()).unwrap();
                n.rect = Rect::new(b.x - o.x, b.y - o.y, b.w.max(1.0), b.h.max(1.0));
                if ty == "arrow" || start_node.is_some() || end_node.is_some() {
                    let local: Vec<V2> = pts.iter().map(|p| *p - b.min()).collect();
                    let binding = |id: Option<NodeId>| id.map(|node| Binding { node, fixed: v2(0.5, 0.5), inside: false });
                    let style = n.style.clone();
                    n.kind = NodeKind::Connector {
                        points: local,
                        start: binding(start_node),
                        end: binding(end_node),
                        route,
                        start_head,
                        end_head: if ty == "line" { head(None, Head::None)? } else { end_head },
                    };
                    n.style = style;
                    n.name = name.clone().unwrap_or_else(|| "Arrow".into());
                } else if pts.len() == 2 {
                    n.kind = NodeKind::Line { a: pts[0] - b.min(), b: pts[1] - b.min() };
                } else {
                    n.kind = NodeKind::Path {
                        subpaths: vec![SubPath { points: pts.iter().map(|p| PathPoint::corner(*p - b.min())).collect(), closed: false }],
                        fill_rule: Default::default(),
                    };
                    n.style.fills.clear();
                    n.style.stroke = Some(Color::INK);
                    n.style.stroke_width = 2.0;
                }
            }
            if let Some(r) = sketch
                && !matches!(n.kind, NodeKind::Text { .. } | NodeKind::Sticky { .. })
            {
                n.style.rough = Some(crate::model::rough::Rough { seed: (n.id.0 as u32).wrapping_mul(2_654_435_761).max(1), ..r });
            }
            let ctx = Parent::of(tx.doc(), &page_id, parent).ctx();
            if parent.is_some() {
                n.layout_child.absolute = ctx.parent_layout.is_some();
            }
            let applied = css::apply(&mut n, &styles, &ctx);
            let mut ignored = applied.ignored.clone();
            if let Some((src, mode)) = &applied.image {
                match image_asset(src) {
                    Ok(asset) => n.style.fills.push(Paint::Image { asset, mode: *mode, opacity: 1.0 }),
                    Err(e) => ignored.push(e),
                }
            }
            let id = tx.insert(n, parent, None);
            fit_text(tx, id, false);
            crate::editor::whiteboard::update_connector(tx, id);
            Ok((id, ignored))
        })
    })?;
    let mut v = json!({"id": node_key(ed, page, id), "pageId": ed.pages[page].id});
    if !ignored.is_empty() {
        v["ignoredStyles"] = json!(ignored);
    }
    text(v)
}

fn update_styles(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let mut jobs: Vec<(usize, Vec<NodeId>, Styles)> = Vec::new();
    for u in a.arr("updates")? {
        let u = Args(u.as_object().ok_or("each update must be an object")?);
        let (page, ids) = resolve_all(ed, &u.strs("nodeIds")?, def)?;
        let styles = css::from_json(u.get("styles").ok_or("each update needs `styles`")?)?;
        jobs.push((page, ids, styles));
    }
    let mut ignored: Map<String, Value> = Map::new();
    let mut warnings = Vec::new();
    let mut count = 0;
    for (page, ids, styles) in jobs {
        let page_id = ed.pages[page].id.clone();
        let res = ed.with_page(page, |ed| {
            ed.edit("Update styles", |tx| {
                let mut out = Vec::new();
                for id in &ids {
                    let parent =
                        Parent::of(tx.doc(), &page_id, tx.doc().get(*id).and_then(|n| n.parent));
                    let old = tx.doc().get(*id).unwrap().rect;
                    let mut n = tx.doc().get(*id).unwrap().clone();
                    let applied = css::apply(&mut n, &styles, &parent.ctx());
                    let override_keys = css::override_keys(&styles);
                    let mut warn = None;
                    if let Some((src, mode)) = &applied.image {
                        match image_asset(src) {
                            Ok(asset) => {
                                n.style.fills.retain(|p| !matches!(p, Paint::Image { .. }));
                                n.style.fills.push(Paint::Image {
                                    asset,
                                    mode: *mode,
                                    opacity: 1.0,
                                });
                            }
                            Err(e) => warn = Some(e),
                        }
                    }
                    if let Some(ps) = parent.size {
                        if let Some(r) = applied.right {
                            n.rect.x = ps.0 - n.rect.w - r;
                        }
                        if let Some(b) = applied.bottom {
                            n.rect.y = ps.1 - n.rect.h - b;
                        }
                    }
                    let new = n.rect;
                    let keep_h =
                        get(&styles, "height").is_some_and(|v| css::length(v, 16.0).is_some());
                    *tx.node_mut(*id).unwrap() = n;
                    crate::editor::structure::mark_overrides(tx, *id, &override_keys);
                    recolor_vectors(tx, *id, &styles);
                    remember_stretch(&page_id, *id, &styles);
                    if (old.w - new.w).abs() > 1e-9 || (old.h - new.h).abs() > 1e-9 {
                        crate::layout::resize_children(tx, *id, old, new);
                    }
                    fit_text(tx, *id, keep_h);
                    relayout(tx, Some(*id));
                    out.push((*id, applied.ignored, warn));
                }
                out
            })
        });
        for (id, ig, warn) in res {
            count += 1;
            if !ig.is_empty() {
                ignored.insert(node_key(ed, page, id), json!(ig));
            }
            warnings.extend(warn);
        }
    }
    let mut v = json!({"updated": count});
    if !ignored.is_empty() {
        v["ignoredStyles"] = Value::Object(ignored);
    }
    if !warnings.is_empty() {
        v["warnings"] = json!(warnings);
    }
    text(v)
}

/// Group per-node updates by page and run each page's batch as one edit.
fn batch<T: Clone>(
    ed: &mut Editor,
    a: &Args,
    key: &str,
    label: &str,
    parse: impl Fn(&Args) -> Result<T, String>,
    mut f: impl FnMut(&mut Tx, NodeId, &T) -> Result<(), String>,
) -> Result<usize, String> {
    let def = page_arg(ed, a)?;
    let mut by_page: Vec<(usize, Vec<(NodeId, T)>)> = Vec::new();
    for u in a.arr(key)? {
        let u = Args(u.as_object().ok_or("each update must be an object")?);
        let (page, id) = resolve(ed, u.req_str("nodeId")?, def)?;
        let v = parse(&u)?;
        match by_page.iter_mut().find(|(p, _)| *p == page) {
            Some((_, list)) => list.push((id, v)),
            None => by_page.push((page, vec![(id, v)])),
        }
    }
    let mut n = 0;
    for (page, list) in by_page {
        ed.with_page(page, |ed| {
            ed.edit(label, |tx| {
                for (id, v) in &list {
                    f(tx, *id, v)?;
                    n += 1;
                }
                Ok::<(), String>(())
            })
        })?;
    }
    Ok(n)
}

fn set_text_content(ed: &mut Editor, a: &Args) -> ToolResult {
    let n = batch(
        ed,
        a,
        "updates",
        "Set text",
        |u| {
            Ok(u.get("textContent")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string())
        },
        |tx, id, t| {
            let node = tx.node_mut(id).unwrap();
            let old_len = node.kind.text().map(|s| s.chars().count()).ok_or_else(|| {
                format!(
                    "{} is a {}, not a text layer",
                    node.name,
                    node.kind.type_name()
                )
            })?;
            *node.kind.text_mut().unwrap() = t.clone();
            crate::editor::structure::mark_overrides(tx, id, &["text"]);
            let node = tx.node_mut(id).unwrap();
            let len = t.chars().count();
            for s in &mut node.text_style.spans {
                if s.start == 0 && s.end >= old_len {
                    s.end = len;
                }
                s.start = s.start.min(len);
                s.end = s.end.min(len);
            }
            node.text_style.spans.retain(|s| s.end > s.start);
            fit_text(tx, id, false);
            relayout(tx, Some(id));
            Ok(())
        },
    )?;
    text(json!({"updated": n}))
}

// ------------------------------------------------------------------------------ variables

fn get_variables(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = page_arg(ed, a)?;
    let doc = ed.page_doc(page);
    match a.str("format") {
        Some("css") => return Ok(vec![Out::Text(super::codegen::export_variables_css(doc))]),
        Some("json") => {
            let ds = design_of(ed);
            return text(super::codegen::export_dtcg_system(
                &ds,
                &ed.pages[page].modes,
            ));
        }
        Some("tailwind") => {
            return Ok(vec![Out::Text(super::codegen::export_variables_tailwind(
                doc,
            ))]);
        }
        Some(f) => return Err(format!("unknown format `{f}` (css, json, or tailwind)")),
        None => {}
    }
    let vars: Map<String, Value> = doc
        .variables
        .iter()
        .map(|(k, c)| (k.clone(), json!(c.to_hex())))
        .collect();
    let mut v = json!({"variables": vars, "use": "var(--name) in color styles"});
    if !doc.tokens.is_empty() {
        v["tokens"] = json!(doc.tokens);
    }
    if !doc.modes.is_empty() {
        v["mode"] = json!(doc.mode);
        v["modes"] = json!(doc.modes.keys().collect::<Vec<_>>());
    }
    let ds = design_of(ed);
    let page_modes = &ed.pages[page].modes;
    v["collections"] = ds
        .collections
        .iter()
        .map(|c| collection_json(&ds, c))
        .collect();
    v["pageModes"] = ds
        .collections
        .iter()
        .map(|c| {
            let active = page_modes.get(&c.id).unwrap_or(&c.default_mode);
            let name = c.modes.iter().find(|m| &m.id == active).map(|m| &m.name);
            (c.name.clone(), json!(name))
        })
        .collect::<Map<String, Value>>()
        .into();
    text(v)
}

/// The project design system; a project that never went through the session reads as its first
/// page does, like `edit_design`.
fn design_of(ed: &Editor) -> DesignSystem {
    if ed.design.collections.is_empty() {
        return variables::migrate_from_document(ed.page_doc(0));
    }
    ed.design.clone()
}

fn collection_json(ds: &DesignSystem, c: &variables::Collection) -> Value {
    let value = |v: &DsVal| match v {
        DsVal::Color(c) => json!(c.to_hex()),
        DsVal::Number(n) => json!(n),
        DsVal::String(s) => json!(s),
        DsVal::Boolean(b) => json!(b),
        DsVal::Alias(id) => json!({"alias": ds.find(id).map(|(_, t)| t.name.clone())}),
    };
    let vars: Vec<Value> = c
        .variables
        .iter()
        .map(|v| {
            let values: Map<String, Value> = c
                .modes
                .iter()
                .filter_map(|m| Some((m.name.clone(), value(v.values.get(&m.id)?))))
                .collect();
            json!({"id": v.id, "name": v.name, "cssName": variables::css_name(v),
                   "group": variables::group_of(&v.name), "type": v.kind,
                   "values": values, "description": v.description, "hidden": v.hidden})
        })
        .collect();
    let default = c.modes.iter().find(|m| m.id == c.default_mode).map(|m| &m.name);
    json!({"id": c.id, "name": c.name, "library": c.library,
           "modes": c.modes.iter().map(|m| &m.name).collect::<Vec<_>>(),
           "defaultMode": default, "variables": vars})
}

/// One `set_variables` entry after parsing.
struct VarSpec {
    name: String,
    kind: Option<Kind>,
    value: Option<DsVal>,
    alias: Option<String>,
    collection: Option<String>,
    mode: Option<String>,
    description: Option<String>,
    hidden: Option<bool>,
    /// The old shape: a plain color by name.
    plain_color: Option<Color>,
}

fn parse_kind(s: &str) -> Result<Kind, String> {
    match s {
        "color" => Ok(Kind::Color),
        "number" => Ok(Kind::Number),
        "string" => Ok(Kind::String),
        "boolean" => Ok(Kind::Boolean),
        _ => Err(format!("unknown type `{s}` (color, number, string, boolean)")),
    }
}

fn parse_value(name: &str, kind: Option<Kind>, v: &Value) -> Result<(Kind, DsVal), String> {
    let bad = |k: &str| format!("variable `{name}`: `{v}` is not a {k}");
    match (kind, v) {
        (Some(Kind::Color) | None, Value::String(s)) if css::color(s).is_some() => {
            Ok((Kind::Color, DsVal::Color(css::color(s).unwrap())))
        }
        (Some(Kind::Color), _) => Err(bad("color")),
        (Some(Kind::Number) | None, Value::Number(n)) => {
            Ok((Kind::Number, DsVal::Number(n.as_f64().unwrap_or_default())))
        }
        (Some(Kind::Number), Value::String(s)) => s
            .trim()
            .trim_end_matches("px")
            .trim()
            .parse()
            .map(|n| (Kind::Number, DsVal::Number(n)))
            .map_err(|_| bad("number")),
        (Some(Kind::Number), _) => Err(bad("number")),
        (Some(Kind::String), Value::String(s)) => Ok((Kind::String, DsVal::String(s.clone()))),
        (Some(Kind::String), _) => Err(bad("string")),
        (Some(Kind::Boolean) | None, Value::Bool(b)) => Ok((Kind::Boolean, DsVal::Boolean(*b))),
        (Some(Kind::Boolean), _) => Err(bad("boolean")),
        (None, _) => Err(bad("color (pass type for other kinds)")),
    }
}

fn parse_var_name(name: &str) -> Result<String, String> {
    let name = name.trim().trim_start_matches("--").trim_matches('/').to_string();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/'))
    {
        return Err(format!(
            "variable name `{name}` may only use letters, digits, -, _ and / (groups)"
        ));
    }
    Ok(name)
}

fn parse_spec(name: &str, entry: &Value, a: &Args) -> Result<VarSpec, String> {
    let name = parse_var_name(name)?;
    let obj = entry.as_object();
    let field = |k: &str| {
        obj.and_then(|o| o.get(k))
            .or_else(|| a.get(k))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let kind = field("type").as_deref().map(parse_kind).transpose()?;
    let alias = obj
        .and_then(|o| o.get("alias"))
        .and_then(Value::as_str)
        .map(|s| s.trim().trim_start_matches("--").to_string());
    let raw = match obj {
        Some(o) => o.get("value"),
        None => Some(entry),
    };
    let (kind, value) = match (raw, &alias) {
        (Some(v), None) => {
            let (k, v) = parse_value(&name, kind, v)?;
            (Some(k), Some(v))
        }
        (Some(_), Some(_)) => return Err(format!("variable `{name}`: give a value or an alias, not both")),
        (None, _) => (kind, None),
    };
    let description = obj
        .and_then(|o| o.get("description"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let hidden = obj.and_then(|o| o.get("hidden")).and_then(Value::as_bool);
    let collection = field("collection");
    let mode = field("mode");
    let plain_color = match (&value, obj, &collection) {
        (Some(DsVal::Color(c)), None, None) if a.get("type").is_none() => Some(*c),
        _ => None,
    };
    Ok(VarSpec {
        name,
        kind,
        value,
        alias,
        collection,
        mode,
        description,
        hidden,
        plain_color,
    })
}

/// `(collection id, variable id, kind)` of a variable by name or CSS name.
fn find_var(ds: &DesignSystem, name: &str) -> Option<(String, String, Kind)> {
    let css = crate::design_system::sanitize_name(name);
    ds.collections.iter().find_map(|c| {
        c.variables
            .iter()
            .find(|v| v.name == name || variables::css_name(v) == css)
            .map(|v| (c.id.clone(), v.id.clone(), v.kind))
    })
}

fn mode_for(ds: &mut DesignSystem, cid: &str, name: &str) -> Result<String, variables::Error> {
    let c = ds
        .collection(cid)
        .ok_or_else(|| variables::Error::NotFound(cid.into()))?;
    if let Some(m) = c.modes.iter().find(|m| m.name == name || m.id == name) {
        return Ok(m.id.clone());
    }
    // A collection with only its unnamed `Default` mode adopts the first name defined.
    if c.modes.len() == 1 && c.modes[0].name == variables::DEFAULT_MODE && c.library.is_none() {
        let id = c.modes[0].id.clone();
        if let Some(c) = ds.collections.iter_mut().find(|c| c.id == cid) {
            c.modes[0].name = name.into();
        }
        return Ok(id);
    }
    ds.add_mode(cid, name)
}

fn set_entry(
    ds: &mut DesignSystem,
    active: &variables::PageModes,
    s: &VarSpec,
) -> Result<(), variables::Error> {
    use variables::Error;
    if let Some(c) = s.plain_color {
        return design::set_color(ds, active, &s.name, s.mode.as_deref(), c);
    }
    let target = match &s.alias {
        Some(t) => Some(find_var(ds, t).ok_or_else(|| Error::NotFound(t.clone()))?),
        None => None,
    };
    let existing = find_var(ds, &s.name);
    let kind = s
        .kind
        .or(existing.as_ref().map(|e| e.2))
        .or(target.as_ref().map(|t| t.2))
        .ok_or_else(|| Error::Missing(s.name.clone()))?;
    let cid = match (&s.collection, &existing) {
        (Some(c), _) => match ds.collections.iter().find(|x| &x.id == c || &x.name == c) {
            Some(x) => x.id.clone(),
            None => ds.add_collection(c),
        },
        (None, Some(e)) => e.0.clone(),
        (None, None) if kind == Kind::Color => design::color_collection(ds),
        (None, None) => match ds
            .collections
            .iter()
            .find(|c| c.library.is_none() && c.name == variables::TOKENS_COLLECTION)
        {
            Some(c) => c.id.clone(),
            None => ds.add_collection(variables::TOKENS_COLLECTION),
        },
    };
    let (vid, fresh) = match existing.filter(|e| e.0 == cid) {
        Some(e) if e.2 != kind => return Err(Error::KindMismatch(s.name.clone())),
        Some(e) => (e.1, false),
        None => (ds.add_variable(&cid, &s.name, kind)?, true),
    };
    let value = match (&s.value, &target) {
        (Some(v), _) => Some(v.clone()),
        (None, Some(t)) => Some(DsVal::Alias(t.1.clone())),
        (None, None) => None,
    };
    if let Some(value) = value {
        let mid = match &s.mode {
            Some(m) => mode_for(ds, &cid, m)?,
            None => {
                let c = ds.collection(&cid).expect("collection exists");
                active
                    .get(&cid)
                    .filter(|m| c.modes.iter().any(|x| &x.id == *m))
                    .unwrap_or(&c.default_mode)
                    .clone()
            }
        };
        if fresh {
            let all: Vec<String> = ds.collection(&cid).unwrap().modes.iter().map(|m| m.id.clone()).collect();
            for m in all {
                ds.set_value(&vid, &m, value.clone())?;
            }
        }
        ds.set_value(&vid, &mid, value)?;
    }
    if s.description.is_some() || s.hidden.is_some() {
        let c = ds.collections.iter_mut().find(|c| c.id == cid).unwrap();
        if c.library.is_some() {
            return Err(Error::ReadOnly(c.name.clone()));
        }
        let v = c.variables.iter_mut().find(|v| v.id == vid).unwrap();
        if let Some(d) = &s.description {
            v.description = d.clone();
        }
        if let Some(h) = s.hidden {
            v.hidden = h;
        }
    }
    Ok(())
}

fn set_variables(ed: &mut Editor, a: &Args) -> ToolResult {
    let vars = a
        .get("variables")
        .and_then(Value::as_object)
        .filter(|m| !m.is_empty())
        .ok_or("`variables` must be an object like {\"accent\": \"#cba6f7\"}")?;
    let mut specs = vars
        .iter()
        .map(|(n, v)| parse_spec(n, v, a))
        .collect::<Result<Vec<_>, _>>()?;
    // Aliases last, so they may point at a variable defined in the same call.
    specs.sort_by_key(|s| s.alias.is_some());
    let active = ed.pages[ed.active_page].modes.clone();
    let ((), fills) = ed
        .edit_design("Set variables", |ds| {
            specs.iter().try_for_each(|s| set_entry(ds, &active, s))
        })
        .map_err(|e| e.to_string())?;
    let updated: Map<String, Value> = specs
        .iter()
        .map(|s| {
            let css = crate::design_system::sanitize_name(&s.name);
            (s.name.clone(), json!(fills.get(&css).copied().unwrap_or(0)))
        })
        .collect();
    text(json!({"fillsUpdated": updated, "pages": ed.pages.len()}))
}

fn create_collection(ed: &mut Editor, a: &Args) -> ToolResult {
    let name = a.req_str("name")?.trim().to_string();
    if name.is_empty() {
        return Err("`name` is empty".into());
    }
    let modes: Vec<String> = a
        .get("modes")
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let (id, _) = ed
        .edit_design("Create collection", |ds| {
            if ds.collections.iter().any(|c| c.library.is_none() && c.name == name) {
                return Err(variables::Error::Duplicate(format!("collection {name}")));
            }
            let id = ds.add_collection(&name);
            if let Some((first, rest)) = modes.split_first() {
                let c = ds.collections.iter_mut().find(|c| c.id == id).unwrap();
                c.modes[0].name = first.clone();
                for m in rest {
                    ds.add_mode(&id, m)?;
                }
            }
            Ok(id)
        })
        .map_err(|e| e.to_string())?;
    let c = ed.design.collection(&id).expect("just created");
    text(json!({"id": id, "name": c.name, "modes": c.modes.iter().map(|m| &m.name).collect::<Vec<_>>()}))
}

fn delete_variables(ed: &mut Editor, a: &Args) -> ToolResult {
    let names: Vec<String> = a
        .get("names")
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .filter(|v: &Vec<String>| !v.is_empty())
        .ok_or("`names` must be an array of variable names")?;
    let force = a.bool("force").unwrap_or(false);
    let ds = design_of(ed);
    let ids = names
        .iter()
        .map(|n| {
            find_var(&ds, n.trim().trim_start_matches("--"))
                .map(|e| e.1)
                .ok_or_else(|| format!("no variable `{n}`"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !force {
        for (n, id) in names.iter().zip(&ids) {
            // Dependents that go in the same call do not count.
            let mut deps = ds.dependents(id);
            deps.retain(|d| !names.contains(d));
            if !deps.is_empty() {
                return Err(format!(
                    "`{n}` is aliased by {}; pass force to detach them to their resolved values",
                    deps.join(", ")
                ));
            }
        }
    }
    ed.edit_design("Delete variables", |ds| {
        for id in &ids {
            // Gone already when the same variable was named twice.
            if ds.find(id).is_some() {
                ds.remove_variable(id, true)?;
            }
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    text(json!({"deleted": names.len()}))
}

fn lint_design(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let (page, roots) = match a.str("nodeId").filter(|n| !n.is_empty() && *n != "root") {
        Some(n) => {
            let (p, id) = resolve(ed, n, def)?;
            (p, vec![id])
        }
        None => (def, ed.page_doc(def).root.clone()),
    };
    let scale: Vec<f64> = match a.get("scale").and_then(Value::as_array) {
        Some(v) => v.iter().filter_map(Value::as_f64).collect(),
        None => super::lint::DEFAULT_SCALE.to_vec(),
    };
    let fixed = if a.bool("fix").unwrap_or(false) {
        ed.with_page(page, |ed| {
            ed.edit("Auto-fix design lint", |tx| {
                super::lint::autofix(tx, &roots, &scale)
            })
        })
    } else {
        0
    };
    let doc = ed.page_doc(page);
    let mut issues = super::lint::lint(doc, &roots, &scale);
    issues.extend(super::lint::not_an_instance(doc, &roots, &super::lint::component_mains(ed)));
    let count = |sev: &str| issues.iter().filter(|i| i.severity == sev).count();
    let mut by_rule: Map<String, Value> = Map::new();
    for i in &issues {
        let e = by_rule.entry(i.rule.to_string()).or_insert(json!(0));
        *e = json!(e.as_u64().unwrap_or(0) + 1);
    }
    let list: Vec<Value> = issues
        .iter()
        .take(200)
        .map(|i| {
            json!({"rule": i.rule, "severity": i.severity, "node": node_key(ed, page, i.node),
                   "name": doc.get(i.node).map(|n| n.name.clone()), "message": i.message, "fix": i.fix})
        })
        .collect();
    text(json!({
        "errors": count("error"), "warnings": count("warning"), "info": count("info"),
        "fixedNodes": fixed,
        "byRule": by_rule, "issues": list,
        "truncated": issues.len() > 200,
    }))
}

// ------------------------------------------------------------------------------ components

fn set_tokens(ed: &mut Editor, a: &Args) -> ToolResult {
    let toks = a
        .get("tokens")
        .and_then(Value::as_object)
        .filter(|m| !m.is_empty())
        .ok_or("`tokens` must be an object like {\"spacing-md\": \"16px\"}")?;
    let mut parsed: Vec<(String, String)> = Vec::new();
    for (name, v) in toks {
        let name = name.trim().trim_start_matches("--").to_string();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "token name `{name}` may only use letters, digits, - and _"
            ));
        }
        let value = match v {
            Value::String(s) => s.trim().to_string(),
            Value::Number(n) => format!("{n}px"),
            _ => return Err(format!("token `{name}` must be a string like \"16px\"")),
        };
        if value.is_empty() {
            return Err(format!("token `{name}` is empty"));
        }
        parsed.push((name, value));
    }
    ed.edit_design("Set tokens", |ds| {
        parsed
            .iter()
            .try_for_each(|(n, v)| design::set_token(ds, n, v))
    })
    .map_err(|e| e.to_string())?;
    text(
        json!({"tokens": parsed.len(), "use": "var(--name) in any style value, e.g. padding: var(--spacing-md); substituted when written"}),
    )
}

fn set_variable_mode(ed: &mut Editor, a: &Args) -> ToolResult {
    let mode = a.req_str("mode")?.to_string();
    if !ed.page_doc(ed.active_page).modes.contains_key(&mode) {
        let known: Vec<String> = ed.page_doc(ed.active_page).modes.keys().cloned().collect();
        return Err(format!(
            "no mode `{mode}`; define it with set_variables {{mode}} first (modes: {})",
            known.join(", ")
        ));
    }
    // Project-wide like before: every page switches (the Design tab switches one page).
    let (cid, mid) = design::find_mode(&ed.design, &mode).ok_or(format!("no mode `{mode}`"))?;
    let mut fills = 0;
    for page in 0..ed.pages.len() {
        fills += ed
            .set_page_mode_on(page, &cid, &mid)
            .map_err(|e| e.to_string())?;
    }
    text(json!({"mode": mode, "fillsUpdated": fills}))
}

fn get_components(ed: &mut Editor, a: &Args) -> ToolResult {
    let only = a.str("pageId").filter(|p| !p.is_empty()).map(|p| find_page(ed, p)).transpose()?;
    // Instances anywhere, by the page and node of their main.
    let mut uses: HashMap<(usize, NodeId), usize> = HashMap::new();
    for page in 0..ed.pages.len() {
        for n in ed.page_doc(page).nodes.values() {
            let Some(l) = &n.instance else { continue };
            // A swapped nested instance counts for the component it was swapped to.
            let (main, source) = l.target();
            let main_page = match source {
                None => page,
                Some(Source::Page(p)) => match ed.pages.iter().position(|s| s.id == *p) {
                    Some(i) => i,
                    None => continue,
                },
                Some(Source::Library(_)) => continue,
            };
            if n.id != main || main_page != page {
                *uses.entry((main_page, main)).or_default() += 1;
            }
        }
    }
    let mut items: Vec<Value> = Vec::new();
    for page in (0..ed.pages.len()).filter(|p| only.is_none_or(|o| o == *p)) {
        let doc = ed.page_doc(page);
        let mut comps: Vec<&Node> = doc.nodes.values().filter(|n| n.component).collect();
        comps.sort_by_key(|n| n.id);
        for c in comps {
            let n = uses.get(&(page, c.id)).copied().unwrap_or(0);
            let mut item = json!({"id": node_key(ed, page, c.id), "name": c.name,
                "componentKey": c.component_key, "pageId": ed.pages[page].id,
                "pageName": ed.pages[page].name,
                "width": r2(c.rect.w), "height": r2(c.rect.h), "uses": n, "instances": n});
            if !c.props.is_empty() {
                item["props"] = json!(c.props.iter().map(|d| json!({"name": d.name, "kind": d.kind, "options": d.options.iter().map(|o| o.value.clone()).collect::<Vec<_>>(), "default": d.default})).collect::<Vec<_>>());
            }
            items.push(item);
        }
    }
    if only.is_none() {
        for (lib, doc) in &ed.library_docs {
            let mut comps: Vec<&Node> = doc.nodes.values().filter(|n| n.component).collect();
            comps.sort_by_key(|n| n.id);
            for c in comps {
                items.push(json!({"name": c.name, "componentKey": c.component_key,
                    "source": {"library": lib, "name": doc.name},
                    "width": r2(c.rect.w), "height": r2(c.rect.h)}));
            }
        }
    }
    text(json!({"components": items}))
}

/// `color` on an icon (vector shapes with no text, e.g. an <x-paper-icon>) recolors the shapes'
/// fills and strokes, keeping a `var(--name)` link for fills. Returns whether it applied.
fn recolor_vectors(tx: &mut Tx, id: NodeId, styles: &Styles) -> bool {
    let Some((color, var)) = get(styles, "color").and_then(css::resolve_color_value) else {
        return false;
    };
    let shapes: Vec<NodeId> = tx
        .doc()
        .subtree(id)
        .into_iter()
        .filter(|i| {
            tx.doc()
                .get(*i)
                .is_some_and(|n| matches!(n.kind, NodeKind::Path { .. }))
        })
        .collect();
    let has_text = tx
        .doc()
        .subtree(id)
        .into_iter()
        .any(|i| tx.doc().get(i).is_some_and(|n| n.kind.text().is_some()));
    if shapes.is_empty() || has_text {
        return false;
    }
    for s in shapes {
        if let Some(n) = tx.node_mut(s) {
            for p in &mut n.style.fills {
                if matches!(p, Paint::Solid { .. }) {
                    *p = match &var {
                        Some(v) => Paint::solid_var(color, v),
                        None => Paint::solid(color),
                    };
                }
            }
            if n.style.stroke.is_some() {
                n.style.stroke = Some(color);
            }
        }
        // Inside an instance the shapes keep their new colour when the main syncs.
        crate::editor::structure::mark_overrides(tx, s, &["fills", "stroke"]);
    }
    crate::editor::structure::mark_overrides(tx, id, &["fills", "stroke"]);
    true
}

/// The colour an icon's shapes are drawn in: the first shape's stroke, else its fill.
fn vector_color(doc: &Document, id: NodeId) -> Option<Color> {
    doc.subtree(id).into_iter().filter_map(|i| doc.get(i)).filter(|n| matches!(n.kind, NodeKind::Path { .. })).find_map(|n| n.style.stroke.or_else(|| n.style.fill()))
}

/// Copy main components from another page (a design-system page) onto this one, so instances can
/// be placed here. Components already here (same name) are left alone.
fn import_components(ed: &mut Editor, a: &Args) -> ToolResult {
    let from = find_page(ed, a.req_str("fromPageId")?)?;
    let to = page_arg(ed, a)?;
    if from == to {
        return Err("fromPageId is the page itself".into());
    }
    let wanted: Option<Vec<String>> = a.get("names").and_then(Value::as_array).map(|v| {
        v.iter()
            .filter_map(Value::as_str)
            .map(str::to_lowercase)
            .collect()
    });
    // Each component as (name, its subtree's nodes, root first).
    let mut found: Vec<(String, Vec<Node>)> = {
        let src = ed.page_doc(from);
        let mut roots: Vec<&Node> = src
            .nodes
            .values()
            .filter(|n| {
                n.component
                    && !src
                        .ancestors(n.id)
                        .iter()
                        .any(|a| src.get(*a).is_some_and(|an| an.component))
            })
            .filter(|n| {
                wanted
                    .as_ref()
                    .is_none_or(|w| w.contains(&n.name.to_lowercase()))
            })
            .collect();
        roots.sort_by_key(|n| n.id);
        roots
            .into_iter()
            .map(|r| {
                let nodes = src
                    .subtree(r.id)
                    .into_iter()
                    .filter_map(|i| src.get(i).cloned())
                    .collect();
                (r.name.clone(), nodes)
            })
            .collect()
    };
    if found.is_empty() {
        return Err("no matching components on that page (create_component first)".into());
    }
    let (imported, skipped) = ed.with_page(to, |ed| {
        ed.edit("Import components", |tx| {
            let have: HashSet<String> = tx
                .doc()
                .nodes
                .values()
                .filter(|n| n.component)
                .map(|n| n.name.to_lowercase())
                .collect();
            // Start to the right of whatever is on the page.
            let mut x = tx
                .doc()
                .root
                .iter()
                .filter_map(|r| tx.doc().world_rect(*r))
                .map(|r| r.right())
                .fold(f64::MIN, f64::max);
            x = if x == f64::MIN { 0.0 } else { x + 160.0 };
            // Wrap into rows so a big library does not become one endless strip.
            let (x0, mut y, mut row_h) = (x, 0.0_f64, 0.0_f64);
            const ROW_W: f64 = 3200.0;
            let (mut imported, mut skipped) = (Vec::new(), Vec::new());
            for (name, nodes) in found.drain(..) {
                if have.contains(&name.to_lowercase()) {
                    skipped.push(name);
                    continue;
                }
                let map: HashMap<NodeId, NodeId> =
                    nodes.iter().map(|n| (n.id, tx.alloc_id())).collect();
                let root = nodes[0].id;
                let mut new_root = None;
                for n in &nodes {
                    let mut c = n.clone();
                    c.id = map[&n.id];
                    c.children = n
                        .children
                        .iter()
                        .filter_map(|k| map.get(k).copied())
                        .collect();
                    c.comments.clear();
                    if c.component && c.component_key.is_some() {
                        c.component_key = Some(crate::variables::new_id());
                    }
                    if let Some(l) = &mut c.instance
                        && l.source.is_none()
                    {
                        match map.get(&l.main) {
                            Some(m) => l.main = *m,
                            None => c.instance = None,
                        }
                    }
                    if n.id == root {
                        if x > x0 && x + c.rect.w > x0 + ROW_W {
                            x = x0;
                            y += row_h + 96.0;
                            row_h = 0.0;
                        }
                        c.rect.x = x;
                        c.rect.y = y;
                        row_h = row_h.max(c.rect.h);
                        x += c.rect.w + 80.0;
                        new_root = Some(tx.insert(c, None, None));
                    } else {
                        c.parent = n.parent.and_then(|p| map.get(&p).copied());
                        tx.put(c);
                    }
                }
                imported.push((name, new_root));
            }
            (imported, skipped)
        })
    });
    let items: Vec<Value> = imported
        .iter()
        .map(|(n, id)| json!({"name": n, "id": id.map(|i| node_key(ed, to, i))}))
        .collect();
    text(json!({"imported": items, "skipped": skipped,
        "note": "copies: re-import after deleting a copy to refresh it (its instances here lose overrides)"}))
}

fn create_component(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let name = a
        .str("name")
        .map(|n| n.chars().take(50).collect::<String>());
    ed.with_page(page, |ed| {
        let n = ed.doc.get(id).ok_or("node not found")?;
        if !n.is_frame() || n.instance.is_some() {
            return Err(format!(
                "{} is a {}; only a frame or artboard that isn't an instance can become a component",
                n.name,
                n.kind.type_name()
            ));
        }
        ed.edit("Create component", |tx| {
            let n = tx.node_mut(id).unwrap();
            n.component = true;
            n.component_key.get_or_insert_with(crate::variables::new_id);
            if let Some(name) = name {
                n.name = name;
            }
        });
        Ok(())
    })?;
    let name = ed.page_doc(page).get(id).map(|n| n.name.clone());
    text(json!({"id": node_key(ed, page, id), "name": name, "component": true}))
}

/// The main component a call names, on any page: `componentId` ("pageId:num") or
/// `componentKey` (the stable key from get_components).
fn find_component(ed: &Editor, a: &Args, default_page: usize) -> Result<(usize, NodeId), String> {
    if let Some(key) = a.str("componentKey").filter(|k| !k.is_empty()) {
        return (0..ed.pages.len())
            .find_map(|p| {
                ed.page_doc(p)
                    .nodes
                    .values()
                    .find(|n| n.component && n.component_key.as_deref() == Some(key))
                    .map(|n| (p, n.id))
            })
            .ok_or_else(|| format!("no component with componentKey `{key}`"));
    }
    let (page, id) = resolve(ed, a.req_str("componentId")?, default_page)?;
    if !ed.page_doc(page).get(id).is_some_and(|n| n.component) {
        return Err(format!(
            "{} is not a component (see create_component)",
            ed.page_doc(page).get(id).map_or("", |n| n.name.as_str())
        ));
    }
    Ok((page, id))
}

fn create_instance(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    // A componentKey that no page owns may name a component of an enabled library.
    let lib = a
        .str("componentKey")
        .filter(|_| find_component(ed, a, def).is_err())
        .and_then(|k| ed.find_library_component(k));
    let (cpage, comp) = match &lib {
        Some((_, id)) => (def, *id),
        None => find_component(ed, a, def)?,
    };
    let (page, parent) = match a.str("parentId") {
        None | Some("") | Some("root") => (def, None),
        Some(p) => {
            let (pp, id) = resolve(ed, p, def)?;
            if a.str("pageId").is_some_and(|s| !s.is_empty()) && pp != def {
                return Err("the parent must be on the page given by pageId".into());
            }
            (pp, Some(id))
        }
    };
    let index = a.num("index").map(|i| i.max(0.0) as usize);
    let at = a.num("x").zip(a.num("y"));
    let name = a
        .str("name")
        .map(|n| n.chars().take(50).collect::<String>());
    let (src_doc, source) = match &lib {
        Some((l, _)) => (ed.library_docs.get(l).cloned(), Some(Source::Library(l.clone()))),
        None => {
            let d = (cpage != page).then(|| ed.page_doc(cpage).clone());
            let s = d.as_ref().map(|_| Source::Page(ed.pages[cpage].id.clone()));
            (d, s)
        }
    };
    let id = ed.with_page(page, |ed| {
        let size = {
            let m = src_doc.as_ref().unwrap_or(&ed.doc).get(comp).ok_or("component not found")?;
            (m.rect.w, m.rect.h)
        };
        Ok::<_, String>(ed.edit("Create instance", |tx| {
            let rect = match (parent, at) {
                (None, Some((x, y))) => Some(Rect::new(x, y, size.0, size.1)),
                (None, None) => {
                    // Beside the existing top-level content, not on top of it.
                    let right = tx
                        .doc()
                        .root
                        .iter()
                        .filter_map(|r| tx.doc().world_rect(*r))
                        .map(|r| r.right())
                        .fold(f64::MIN, f64::max);
                    let y = src_doc
                        .as_ref()
                        .unwrap_or(tx.doc())
                        .world_rect(comp)
                        .map_or(0.0, |r| r.y);
                    Some(Rect::new(right.max(0.0) + 80.0, y, size.0, size.1))
                }
                _ => None,
            };
            let id = crate::editor::structure::instantiate_from(
                tx,
                src_doc.as_ref(),
                source,
                comp,
                parent,
                index,
                rect,
            );
            if let Some(name) = name {
                tx.node_mut(id).unwrap().name = name;
            }
            id
        }))
    })?;
    let doc = ed.page_doc(page);
    let parts: Vec<&Node> = doc
        .subtree(id)
        .into_iter()
        .skip(1)
        .filter_map(|i| doc.get(i))
        .collect();
    let layers: Vec<Value> = parts
        .iter()
        .map(|n| json!({"id": node_key(ed, page, n.id), "name": n.name, "type": n.kind.type_name(), "text": n.kind.text()}))
        .collect();
    let texts: Vec<Value> = parts
        .iter()
        .filter(|n| n.kind.text().is_some())
        .map(|n| json!({"id": node_key(ed, page, n.id), "name": n.name, "text": n.kind.text()}))
        .collect();
    text(json!({"id": node_key(ed, page, id), "textLayers": texts, "layers": layers}))
}

/// The inspector's way to set instance properties on the page being viewed.
pub fn set_props_ui(ed: &mut Editor, ids: &[NodeId], name: &str, value: &str) -> Result<(), String> {
    let page = ed.active_page;
    let keys: Vec<String> = ids.iter().map(|i| node_key(ed, page, *i)).collect();
    let args = json!({"nodeIds": keys, "values": {name: value}});
    set_instance_props(ed, &Args(args.as_object().unwrap())).map(|_| ())
}

/// The inspector's way to add properties to a main component from a template.
pub fn props_template_ui(ed: &mut Editor, main: NodeId, template: &str) -> Result<(), String> {
    let key = node_key(ed, ed.active_page, main);
    let args = json!({"componentId": key, "template": template});
    set_component_props(ed, &Args(args.as_object().unwrap())).map(|_| ())
}

/// Every page as (id, name, document), for a [`Builder`] that resolves mains on other pages.
fn all_pages(ed: &Editor) -> Vec<(String, String, Document)> {
    ed.pages.iter().enumerate().map(|(i, p)| (p.id.clone(), p.name.clone(), ed.page_doc(i).clone())).collect()
}

/// Define a main component's properties: `props` (added or replaced by name), a `template`
/// (`text`, `active`, `status`), `remove` (names). Instances everywhere re-apply their values.
fn set_component_props(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let key = a.str("componentId").or(a.str("nodeId")).ok_or("give componentId (the main component)")?;
    let (page, id) = resolve(ed, key, def)?;
    let doc = ed.page_doc(page);
    let main = doc.get(id).filter(|n| n.component).ok_or("not a main component: create_component first")?;
    let mut defs = main.props.clone();
    let mut renames: Vec<(NodeId, String)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut incoming: Vec<crate::model::PropDef> = match a.get("props") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("props: {e}"))?,
        None => Vec::new(),
    };
    if let Some(t) = a.str("template") {
        let (made, r) = template_props(doc, id, t, a)?;
        incoming.extend(made);
        renames = r;
    }
    let names: BTreeSet<String> = doc.subtree(id).into_iter().filter_map(|i| doc.get(i)).map(|n| n.name.clone()).collect();
    for d in incoming {
        if !["text", "bool", "icon", "enum"].contains(&d.kind.as_str()) {
            return Err(format!("`{}`: kind must be text, bool, icon or enum", d.name));
        }
        if d.kind == "enum" && d.options.is_empty() {
            return Err(format!("`{}`: an enum needs options", d.name));
        }
        if d.kind != "enum" && !d.layer.as_ref().is_some_and(|l| names.contains(l) || renames.iter().any(|(_, n)| n == l)) {
            warnings.push(format!("`{}`: no layer named `{}` in the component", d.name, d.layer.clone().unwrap_or_default()));
        }
        match defs.iter_mut().find(|x| x.name == d.name) {
            Some(x) => *x = d,
            None => defs.push(d),
        }
    }
    for r in a.strs("remove").unwrap_or_default() {
        defs.retain(|d| d.name != r);
    }
    let stored = defs.clone();
    ed.with_page(page, |ed| {
        ed.edit("Component properties", |tx| {
            for (n, name) in &renames {
                if let Some(x) = tx.node_mut(*n) {
                    x.name = name.clone();
                }
            }
            tx.node_mut(id).unwrap().props = stored;
        })
    });
    let instances = reapply_props(ed, page, id, &mut warnings);
    let summary: Vec<Value> = defs.iter().map(|d| json!({"name": d.name, "kind": d.kind, "layer": d.layer, "options": d.options.iter().map(|o| o.value.clone()).collect::<Vec<_>>()})).collect();
    let mut v = json!({"props": summary, "instancesUpdated": instances, "next": "Set values with set_instance_props, or place with <x-paper-instance component=\"Name\" props='{\"Prop\":\"Value\"}'/>."});
    if !warnings.is_empty() {
        v["warnings"] = json!(warnings);
    }
    text(v)
}

/// Re-apply every instance's property values after main `main` (on `main_page`) changed its
/// definitions, on every page. Returns how many instances were updated.
fn reapply_props(ed: &mut Editor, main_page: usize, main: NodeId, warnings: &mut Vec<String>) -> usize {
    ed.finish_loading();
    let main_id = ed.pages[main_page].id.clone();
    let defs = ed.page_doc(main_page).get(main).map(|n| n.props.clone()).unwrap_or_default();
    let mut count = 0;
    for page in 0..ed.pages.len() {
        let want = if page == main_page { None } else { Some(Source::Page(main_id.clone())) };
        let roots: Vec<(NodeId, BTreeMap<String, String>)> = ed
            .page_doc(page)
            .nodes
            .values()
            .filter(|n| !n.prop_values.is_empty() && n.instance.as_ref().is_some_and(|l| l.target() == (main, &want)))
            .map(|n| (n.id, n.prop_values.clone()))
            .collect();
        if roots.is_empty() {
            continue;
        }
        let pages = all_pages(ed);
        let page_id = ed.pages[page].id.clone();
        let w = ed.with_page(page, |ed| {
            ed.edit("Component properties", |tx| {
                let mut b = Builder { tx, page_id, all_pages: &pages, ignored: BTreeSet::new(), warnings: Vec::new() };
                for (r, values) in &roots {
                    b.apply_props(*r, &defs, values);
                }
                b.warnings
            })
        });
        warnings.extend(w);
        count += roots.len();
    }
    count
}

/// Set property values on instances: `nodeIds` and `values` ({"Status": "Rejeitada"}).
fn set_instance_props(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let (page, ids) = resolve_all(ed, &a.strs("nodeIds")?, def)?;
    let values: BTreeMap<String, String> = a
        .get("values")
        .and_then(Value::as_object)
        .ok_or("values must be an object of property name to value")?
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().map_or_else(|| v.to_string(), str::to_owned)))
        .collect();
    let pages = all_pages(ed);
    let page_id = ed.pages[page].id.clone();
    let (done, warnings) = ed.with_page(page, |ed| {
        ed.edit("Instance properties", |tx| {
            let mut b = Builder { tx, page_id, all_pages: &pages, ignored: BTreeSet::new(), warnings: Vec::new() };
            let mut done = 0;
            for id in &ids {
                let defs = b.main_props(*id);
                if defs.is_empty() {
                    b.warnings.push(format!("{} is not an instance of a component with properties", id.0));
                    continue;
                }
                b.apply_props(*id, &defs, &values);
                done += 1;
            }
            (done, b.warnings)
        })
    });
    let mut v = json!({"updated": done});
    if !warnings.is_empty() {
        v["warnings"] = json!(warnings);
    }
    text(v)
}

/// Properties made from a template on main `id`, and item renames it needs (repeated item names
/// cannot be told apart in an option's changes).
fn template_props(doc: &Document, id: NodeId, template: &str, a: &Args) -> Result<(Vec<crate::model::PropDef>, Vec<(NodeId, String)>), String> {
    use crate::model::{PropDef, PropOption};
    let first_text = |at: NodeId| doc.subtree(at).into_iter().skip(1).find(|i| doc.get(*i).is_some_and(|n| n.kind.text().is_some()));
    let first_icon = |at: NodeId| doc.subtree(at).into_iter().skip(1).find(|i| doc.get(*i).is_some_and(|n| n.name.starts_with("Icon / ")));
    let var_or = |names: &[&str], fallback: &str| names.iter().find(|n| doc.variables.contains_key(**n)).map_or_else(|| fallback.to_owned(), |n| format!("var(--{n})"));
    let name = a.str("name");
    match template {
        "text" => {
            let mut seen = BTreeSet::new();
            let defs = doc
                .subtree(id)
                .into_iter()
                .skip(1)
                .filter_map(|i| doc.get(i))
                .filter(|n| n.kind.text().is_some() && seen.insert(n.name.clone()))
                .map(|n| PropDef { name: n.name.clone(), kind: "text".into(), layer: Some(n.name.clone()), default: n.kind.text().map(|t| t.to_string()), ..Default::default() })
                .collect();
            Ok((defs, Vec::new()))
        }
        "active" => {
            let root = doc.get(id).unwrap();
            let container = match root.children.as_slice() {
                [only] if doc.get(*only).is_some_and(|c| c.children.len() >= 2) => *only,
                _ => id,
            };
            let items = doc.get(container).map(|c| c.children.clone()).unwrap_or_default();
            if items.len() < 2 {
                return Err("the active template needs a row of two or more items (tabs, nav, segments)".into());
            }
            // Each item is told apart by name: repeated names take their label's text.
            let mut renames = Vec::new();
            let mut item_names = Vec::new();
            for it in &items {
                let n = doc.get(*it).unwrap();
                let label = first_text(*it).and_then(|t| doc.get(t)).and_then(|t| t.kind.text().map(|s| s.to_string()));
                let repeated = items.iter().filter(|o| doc.get(**o).is_some_and(|x| x.name == n.name)).count() > 1;
                let nm = if repeated { label.clone().unwrap_or_else(|| format!("{} {}", n.name, item_names.len() + 1)) } else { n.name.clone() };
                if nm != n.name {
                    renames.push((*it, nm.clone()));
                }
                item_names.push((nm, label));
            }
            let on = a.get("activeStyle").cloned().unwrap_or_else(|| json!({"color": var_or(&["primary", "accent", "brand"], "#111827"), "fontWeight": "600"}));
            let off = a.get("inactiveStyle").cloned().unwrap_or_else(|| json!({"color": var_or(&["text-muted", "muted", "text-secondary"], "#6b7280"), "fontWeight": "400"}));
            let color_of = |s: &Value| json!({"color": s["color"].clone()});
            let options = item_names
                .iter()
                .enumerate()
                .map(|(i, (nm, label))| {
                    let mut set = Map::new();
                    for (j, it) in items.iter().enumerate() {
                        let style = if i == j { &on } else { &off };
                        let item_name = &item_names[j].0;
                        if let Some(t) = first_text(*it).and_then(|t| doc.get(t)) {
                            set.insert(format!("{item_name} > {}", t.name), style.clone());
                        }
                        if let Some(ic) = first_icon(*it).and_then(|t| doc.get(t)) {
                            set.insert(format!("{item_name} > {}", ic.name), color_of(style));
                        }
                    }
                    PropOption { value: label.clone().unwrap_or_else(|| nm.clone()), set: Value::Object(set) }
                })
                .collect::<Vec<_>>();
            let default = options.first().map(|o| o.value.clone());
            Ok((vec![PropDef { name: name.unwrap_or("Active").into(), kind: "enum".into(), options, default, ..Default::default() }], renames))
        }
        "status" => {
            let opts = a.get("options").and_then(Value::as_array).ok_or("the status template needs options: [{\"value\": \"Autorizada\", \"color\": \"#16a34a\"}, ...]")?;
            let label = a.str("label").map(str::to_owned).or_else(|| first_text(id).and_then(|t| doc.get(t)).map(|t| t.name.clone()));
            let icon = first_icon(id).and_then(|t| doc.get(t)).map(|t| t.name.clone());
            let options = opts
                .iter()
                .map(|o| {
                    let value = o["value"].as_str().unwrap_or_default().to_owned();
                    let color = o["color"].as_str().unwrap_or("#6b7280").to_owned();
                    // The root gets a light tint of the colour (hex) or the colour itself (a variable).
                    let tint = if color.starts_with('#') && color.len() == 7 { format!("{color}1f") } else { o["background"].as_str().unwrap_or(&color).to_owned() };
                    let mut set = Map::new();
                    set.insert("$root".into(), json!({"background": tint}));
                    if let Some(l) = &label {
                        set.insert(l.clone(), json!({"text": value, "color": color}));
                    }
                    if let Some(i) = &icon {
                        let mut s = json!({"color": color});
                        if let Some(name) = o["icon"].as_str() {
                            s["icon"] = json!(name);
                        }
                        set.insert(i.clone(), s);
                    }
                    PropOption { value, set: Value::Object(set) }
                })
                .collect::<Vec<_>>();
            if options.is_empty() {
                return Err("the status template needs at least one option".into());
            }
            let default = options.first().map(|o| o.value.clone());
            Ok((vec![PropDef { name: name.unwrap_or("Status").into(), kind: "enum".into(), options, default, ..Default::default() }], Vec::new()))
        }
        t => Err(format!("unknown template `{t}`: text, active or status")),
    }
}

/// Style keys compared between variants, with the value that means "none" when a side lacks it.
const VARIANT_KEYS: [(&str, &str); 9] = [
    ("backgroundColor", "transparent"),
    ("background", "transparent"),
    ("color", ""),
    ("border", "none"),
    ("borderTop", "none"),
    ("borderBottom", "none"),
    ("opacity", "1"),
    ("fontWeight", "400"),
    ("borderRadius", "0px"),
];

/// A component's layers by path of names (`Item > Label`); icon names are matched as one name so
/// two variants with different icons line up. Returns (matching key, real path, node); a path
/// two siblings share is left out (an override could not tell them apart).
fn variant_paths(doc: &Document, root: NodeId) -> Vec<(String, String, NodeId)> {
    let mut out = Vec::new();
    let mut stack: Vec<(NodeId, String, String)> = vec![(root, String::new(), String::new())];
    while let Some((id, key, real)) = stack.pop() {
        let Some(n) = doc.get(id) else { continue };
        let kids: Vec<(NodeId, String)> = n.children.iter().filter_map(|c| doc.get(*c).map(|k| (*c, k.name.clone()))).collect();
        for (c, name) in &kids {
            if kids.iter().filter(|(_, n2)| n2 == name).count() > 1 {
                continue;
            }
            let norm = if name.starts_with("Icon / ") { "Icon / *".to_owned() } else { name.clone() };
            let k = if key.is_empty() { norm } else { format!("{key} > {norm}") };
            let r = if real.is_empty() { name.clone() } else { format!("{real} > {name}") };
            out.push((k.clone(), r.clone(), *c));
            stack.push((*c, k, r));
        }
    }
    out
}

/// Turn per-state components (`Tab bar / Hoje`, `Tab bar / Lojas`...) into one component with an
/// enum property: each variant's differences from the base (fills, colours, borders, weight,
/// visibility, icons; texts with includeText) become an option, every instance moves to the base
/// with its value, and the emptied variants are deleted.
fn merge_variants(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let (page, ids) = resolve_all(ed, &a.strs("componentIds")?, def)?;
    if ids.len() < 2 {
        return Err("give two or more componentIds".into());
    }
    let doc = ed.page_doc(page).clone();
    for id in &ids {
        if !doc.get(*id).is_some_and(|n| n.component) {
            return Err(format!("{} is not a main component", node_key(ed, page, *id)));
        }
    }
    let base = match a.str("baseId") {
        Some(k) => resolve(ed, k, page)?.1,
        None => ids[0],
    };
    let prop = a.str("name").unwrap_or("Variant").to_owned();
    let include_text = a.get("includeText").and_then(Value::as_bool).unwrap_or(false);
    let given = a.strs("values").unwrap_or_default();
    let value_of = |i: usize, id: NodeId| given.get(i).cloned().unwrap_or_else(|| doc.get(id).unwrap().name.rsplit(" / ").next().unwrap_or_default().trim().to_owned());
    let base_paths = variant_paths(&doc, base);
    let parent_layout = |id: NodeId| doc.get(id).and_then(|n| n.parent).and_then(|p| doc.get(p)).and_then(|p| p.layout);
    let style_of = |id: NodeId| css::computed(doc.get(id).unwrap(), parent_layout(id).as_ref(), false);
    let mut warnings = Vec::new();
    // (real key, property) -> value per variant (by index in `kept`).
    let mut kept: Vec<(usize, NodeId)> = Vec::new();
    let mut table: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for (i, id) in ids.iter().enumerate() {
        let paths = variant_paths(&doc, *id);
        let extra: Vec<&str> = paths.iter().filter(|(k, _, _)| !base_paths.iter().any(|(bk, _, _)| bk == k)).map(|(_, r, _)| r.as_str()).collect();
        if !extra.is_empty() {
            warnings.push(format!("left `{}` out: it has layers the base lacks ({})", doc.get(*id).unwrap().name, extra.join(", ")));
            continue;
        }
        // Differences an option cannot carry (layout, size, spacing) would be lost: leave it out.
        let mut pairs = vec![(String::from("$root"), base, *id)];
        pairs.extend(base_paths.iter().filter_map(|(bk, real, b)| paths.iter().find(|(k, _, _)| k == bk).map(|(_, _, v)| (real.clone(), *b, *v))));
        let lost: Vec<String> = pairs
            .iter()
            .flat_map(|(real, b, v)| {
                let (bs, vs) = (style_of(*b), style_of(*v));
                let keys: BTreeSet<&String> = bs.keys().chain(vs.keys()).collect();
                // Where a main sits on the canvas is not part of its look.
                let placement = |k: &str| real == "$root" && ["left", "top", "right", "bottom", "position"].contains(&k);
                keys.into_iter()
                    .filter(|k| !placement(k) && !VARIANT_KEYS.iter().any(|(vk, _)| vk == k) && bs.get(*k) != vs.get(*k))
                    .map(|k| format!("{real}: {k}"))
                    .collect::<Vec<_>>()
            })
            .collect();
        if !lost.is_empty() {
            warnings.push(format!("left `{}` out: it differs in what an option cannot carry ({})", doc.get(*id).unwrap().name, lost.join(", ")));
            continue;
        }
        kept.push((i, *id));
    }
    let n = kept.len();
    let mut record = |key: String, prop: &str, at: usize, v: Value| {
        table.entry((key, prop.to_owned())).or_insert_with(|| vec![Value::Null; n])[at] = v;
    };
    for (at, (_, id)) in kept.iter().enumerate() {
        let paths = variant_paths(&doc, *id);
        let mut pairs: Vec<(String, NodeId, Option<NodeId>)> = vec![("$root".into(), base, Some(*id))];
        for (bk, real, b) in &base_paths {
            pairs.push((real.clone(), *b, paths.iter().find(|(k, _, _)| k == bk).map(|(_, _, v)| *v)));
        }
        for (real, b, v) in pairs {
            let bn = doc.get(b).unwrap();
            let Some(v) = v else {
                record(real, "visibility", at, json!("hidden"));
                continue;
            };
            let vn = doc.get(v).unwrap();
            record(real.clone(), "visibility", at, json!(if vn.visible { "visible" } else { "hidden" }));
            let vs = style_of(v);
            for (k, none) in VARIANT_KEYS {
                let pick = |m: &Map<String, Value>| m.get(k).and_then(Value::as_str).map_or_else(|| none.to_owned(), str::to_owned);
                if !(none.is_empty() && vs.get(k).is_none()) {
                    record(real.clone(), k, at, json!(pick(&vs)));
                }
            }
            if vn.name.starts_with("Icon / ") && bn.name.starts_with("Icon / ") {
                record(real.clone(), "icon", at, json!(vn.name.trim_start_matches("Icon / ")));
            }
            if include_text && bn.kind.text().is_some() && let Some(vt) = vn.kind.text() {
                record(real.clone(), "text", at, json!(vt));
            }
        }
    }
    // Only what differs between the variants becomes part of the options.
    table.retain(|_, vals| vals.iter().any(|v| *v != vals[0]) && vals.iter().all(|v| !v.is_null()));
    let options: Vec<crate::model::PropOption> = kept
        .iter()
        .enumerate()
        .map(|(at, (i, id))| {
            let mut set = Map::new();
            for ((layer, key), vals) in &table {
                let e = set.entry(layer.clone()).or_insert_with(|| json!({}));
                e[key.as_str()] = vals[at].clone();
            }
            crate::model::PropOption { value: value_of(*i, *id), set: Value::Object(set) }
        })
        .collect();
    let base_value = kept.iter().find(|(_, id)| *id == base).map(|(i, id)| value_of(*i, *id)).ok_or("the base was left out")?;
    let family = doc.get(base).unwrap().name.rsplit_once(" / ").map(|(f, _)| f.to_owned());
    let base_key = node_key(ed, page, base);
    // The base becomes the family: its name, and the property.
    ed.with_page(page, |ed| {
        ed.edit("Merge variants", |tx| {
            let m = tx.node_mut(base).unwrap();
            if let Some(f) = &family {
                m.name = f.clone();
            }
            m.props.retain(|d| d.name != prop);
            m.props.push(crate::model::PropDef { name: prop.clone(), kind: "enum".into(), options, default: Some(base_value.clone()), ..Default::default() });
        })
    });
    // Every instance of a merged variant moves to the base, then takes its value.
    let base_page_id = ed.pages[page].id.clone();
    let mut moved = 0;
    for p in 0..ed.pages.len() {
        let want = if p == page { None } else { Some(Source::Page(base_page_id.clone())) };
        let mut by_value: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut swaps = Vec::new();
        for n in ed.page_doc(p).nodes.values() {
            let Some(l) = &n.instance else { continue };
            let (main, source) = l.target();
            if *source != want {
                continue;
            }
            let Some((i, id)) = kept.iter().find(|(_, id)| *id == main) else { continue };
            let key = node_key(ed, p, n.id);
            if *id != base {
                swaps.push(key.clone());
            }
            by_value.entry(value_of(*i, *id)).or_default().push(key);
        }
        let pid = ed.pages[p].id.clone();
        for key in &swaps {
            let args = json!({"nodeId": key, "componentId": base_key, "pageId": pid});
            if let Err(e) = swap_instance(ed, &Args(args.as_object().unwrap())) {
                warnings.push(format!("{key}: {e}"));
            }
        }
        for (value, keys) in by_value {
            moved += keys.len();
            let args = json!({"nodeIds": keys, "values": {prop.as_str(): value}, "pageId": pid});
            if let Err(e) = set_instance_props(ed, &Args(args.as_object().unwrap())) {
                warnings.push(e);
            }
        }
    }
    let gone: Vec<String> = kept.iter().filter(|(_, id)| *id != base).map(|(_, id)| node_key(ed, page, *id)).collect();
    if a.get("deleteMerged").and_then(Value::as_bool).unwrap_or(true) && !gone.is_empty() {
        let args = json!({"nodeIds": gone, "pageId": ed.pages[page].id});
        delete_nodes(ed, &Args(args.as_object().unwrap()))?;
    }
    let mut v = json!({"component": base_key, "name": family, "property": prop, "options": kept.iter().map(|(i, id)| value_of(*i, *id)).collect::<Vec<_>>(), "changes": table.keys().map(|(l, k)| format!("{l}: {k}")).collect::<Vec<_>>(), "instancesMoved": moved});
    if !warnings.is_empty() {
        v["warnings"] = json!(warnings);
    }
    text(v)
}

fn swap_instance(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let (page, inst) = resolve(ed, a.req_str("nodeId")?, def)?;
    let lib = a
        .str("componentKey")
        .filter(|_| find_component(ed, a, page).is_err())
        .and_then(|k| ed.find_library_component(k));
    let (cpage, new_main) = match &lib {
        Some((_, id)) => (page, *id),
        None => find_component(ed, a, page)?,
    };
    let (src_doc, source) = match &lib {
        Some((l, _)) => (ed.library_docs.get(l).cloned(), Some(Source::Library(l.clone()))),
        None => {
            let d = (cpage != page).then(|| ed.page_doc(cpage).clone());
            let s = d.as_ref().map(|_| Source::Page(ed.pages[cpage].id.clone()));
            (d, s)
        }
    };
    ed.with_page(page, |ed| {
        // An instance root: linked, and not itself inside an instance. A missing main is fine,
        // so a swap restores such an instance.
        let n = ed.doc.get(inst).ok_or("node not found")?;
        let is_root = n.instance.is_some()
            && n.parent
                .and_then(|p| ed.doc.get(p))
                .is_none_or(|p| p.instance.is_none());
        if n.instance.is_none() {
            return Err(format!(
                "`{}` is not a component instance",
                a.req_str("nodeId").unwrap_or_default()
            ));
        }
        if cpage == page && lib.is_none() && (inst == new_main || ed.doc.is_ancestor(inst, new_main)) {
            return Err("could not swap instance to that component".into());
        }
        // A nested instance (inside another instance) keeps its place in it: a swap override.
        let ok = ed.edit("Swap instance", |tx| {
            if is_root {
                crate::editor::structure::swap_instance_tx(tx, src_doc.as_ref(), source, inst, new_main)
            } else {
                crate::editor::structure::swap_nested_tx(tx, src_doc.as_ref(), source, inst, new_main)
            }
        });
        if !ok {
            return Err("could not swap instance to that component".into());
        }
        Ok(())
    })?;
    text(json!({
        "id": node_key(ed, page, inst),
        "componentId": if lib.is_some() { Value::Null } else { json!(node_key(ed, cpage, new_main)) },
        "swapped": true,
    }))
}

fn bind_variable(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let keys = a.strs("nodeIds")?;
    let (page, ids) = resolve_all(ed, &keys, def)?;
    let field = a.req_str("field")?;
    let css = match a.str("variable").filter(|v| !v.is_empty()) {
        None => None,
        Some(name) => {
            let ds = design_of(ed);
            let (_, vid, _) = find_var(&ds, name.trim().trim_start_matches("--"))
                .ok_or_else(|| format!("no variable `{name}`"))?;
            Some(variables::css_name(ds.find(&vid).unwrap().1))
        }
    };
    ed.with_page(page, |ed| ed.bind(&ids, field, css.as_deref()))?;
    text(json!({"nodes": ids.len(), "field": field, "variable": css}))
}

fn detach_instance(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let keys = match a.strs("nodeIds") {
        Ok(k) => k,
        Err(_) => vec![a.req_str("nodeId")?.to_string()],
    };
    let (page, ids) = resolve_all(ed, &keys, def)?;
    let detached = ed.with_page(page, |ed| {
        ed.edit("Detach instance", |tx| {
            let mut count = 0;
            for r in &ids {
                if tx.doc().get(*r).and_then(|n| n.instance.as_ref()).is_some() {
                    for id in tx.doc().subtree(*r) {
                        if let Some(n) = tx.node_mut(id) {
                            n.instance = None;
                        }
                    }
                    count += 1;
                }
            }
            count
        })
    });
    text(json!({
        "detached": detached,
        "nodeIds": ids.iter().map(|i| node_key(ed, page, *i)).collect::<Vec<_>>(),
    }))
}

fn diff_frames(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let (page_a, id_a) = resolve(ed, a.req_str("beforeId")?, def)?;
    let (page_b, id_b) = resolve(ed, a.req_str("afterId")?, def)?;
    let doc_a = ed.page_doc(page_a);
    let doc_b = ed.page_doc(page_b);
    let diff = super::diff::diff_subtrees(doc_a, id_a, doc_b, id_b)?;
    text(super::diff::to_json(&diff))
}

// ------------------------------------------------------------------------------ design system packages

/// Import an OpenDesign design-system package (a folder with `manifest.json` and
/// `tokens.css`) into the project: the colors of `:root` become variables, the colors of the
/// `[data-theme="name"]` blocks become modes and every other value becomes a token. The
/// design system is project-wide, so every page gets it in one undo step.
#[cfg(not(target_arch = "wasm32"))]
fn import_design_system(host: &mut dyn Host, a: &Args) -> ToolResult {
    let dir = PathBuf::from(a.req_str("path")?);
    let pkg = crate::design_system::import_package(&dir)?;
    let ed = host.editor();
    // The page the caller names has to exist; the design system itself is project-wide.
    if let Some(p) = a.str("pageId").filter(|p| !p.is_empty()) {
        find_page(ed, p)?;
    }
    let (_, per_var) = ed
        .edit_design("Import design system", |ds| {
            design::replace_system(
                ds,
                &pkg.tokens.variables,
                &pkg.tokens.modes,
                &pkg.tokens.tokens,
            );
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    let fills: usize = per_var.values().sum();
    text(json!({
        "package": {"id": pkg.manifest.id, "name": pkg.manifest.name,
                    "category": pkg.manifest.category, "description": pkg.manifest.description},
        "variables": pkg.tokens.variables.len(),
        "modes": pkg.tokens.modes.len(),
        "tokens": pkg.tokens.tokens.len(),
        "pages": ed.pages.len(),
        "fillsUpdated": fills,
        "design": pkg.design.is_some(),
    }))
}

#[cfg(target_arch = "wasm32")]
fn import_design_system(_: &mut dyn Host, _: &Args) -> ToolResult {
    Err(NO_DISK.into())
}

/// Write the page's variables, modes and tokens as an OpenDesign design-system package
/// (`manifest.json`, `tokens.css` and `DESIGN.md`) in `path`.
#[cfg(not(target_arch = "wasm32"))]
fn export_design_system(host: &mut dyn Host, a: &Args) -> ToolResult {
    let dir = PathBuf::from(a.req_str("path")?);
    let project = host.project().map(|p| p.manifest.name.clone());
    let ed = host.editor();
    let page = page_arg(ed, a)?;
    let doc = ed.page_doc(page).clone();
    let mut meta = crate::design_system::Meta::for_page(&dir, &doc);
    if let Some(name) = project {
        meta.name = name;
    }
    let ds = design_of(ed);
    let written =
        crate::design_system::export_package_system(&dir, &doc, &meta, &ds, &ed.pages[page].modes)?;
    text(json!({
        "path": dir,
        "files": written,
        "variables": doc.variables.len(),
        "modes": doc.modes.len(),
        "tokens": doc.tokens.len(),
    }))
}

#[cfg(target_arch = "wasm32")]
fn export_design_system(_: &mut dyn Host, _: &Args) -> ToolResult {
    Err(NO_DISK.into())
}

// ------------------------------------------------------------------------------ libraries

fn library_tool(host: &mut dyn Host, name: &str, a: &Args) -> ToolResult {
    use crate::libraries as lib;
    let Some((dir, project_id)) = host.project().map(|p| (p.dir.clone(), p.manifest.id.clone())) else {
        return Err("libraries need a saved project (create_file, then open_file)".into());
    };
    let store = host.store().clone();
    let summary = |l: &lib::LibrarySummary, enabled: bool| {
        json!({"libraryId": l.library_id, "name": l.name, "description": l.description,
            "version": l.version, "variables": l.variables, "components": l.components,
            "enabled": enabled})
    };
    let r = match name {
        "publish_library" => {
            let gpu = host.gpu();
            let ed = host.editor();
            let changes = lib::publish_diff(&dir, ed)?;
            let p = lib::publish_with_previews(&dir, ed, a.str("description").unwrap_or(""), gpu.as_ref())?;
            json!({"libraryId": p.library_id, "name": p.name, "version": p.version,
                "components": p.components.len(), "changes": changes.to_string()})
        }
        "list_libraries" => {
            let ed = host.editor();
            let enabled = lib::status(&store, &dir, ed);
            let available: Vec<Value> = lib::available(&store, Some(&dir))
                .iter()
                .map(|l| summary(l, enabled.iter().any(|e| e.library_id == l.library_id)))
                .collect();
            let enabled: Vec<Value> = enabled
                .iter()
                .map(|e| json!({"libraryId": e.library_id, "name": e.name, "version": e.local_version,
                    "sourceVersion": e.source_version, "sourceFound": e.source_version.is_some(),
                    "updateAvailable": e.update_available}))
                .collect();
            return text(json!({"libraryId": project_id, "available": available, "enabled": enabled}));
        }
        "enable_library" => {
            let id = a.req_str("libraryId")?;
            let l = lib::available(&store, Some(&dir))
                .into_iter()
                .find(|l| l.library_id == id || l.name == id)
                .ok_or_else(|| format!("no available library `{id}` (see list_libraries)"))?;
            lib::enable(&dir, host.editor(), &l)?;
            json!({"libraryId": l.library_id, "name": l.name, "version": l.version,
                "variables": l.variables, "components": l.components})
        }
        "update_library" => {
            let id = a.req_str("libraryId")?;
            let changes = lib::update_diff(&store, &dir, id)?;
            lib::update(&store, &dir, host.editor(), id)?;
            json!({"libraryId": id, "changes": changes.to_string()})
        }
        _ => {
            let id = a.req_str("libraryId")?;
            let r = lib::disable(&dir, host.editor(), id)?;
            json!({"libraryId": id, "detachedFills": r.fills, "detachedBindings": r.bindings,
                "detachedInstances": r.instances})
        }
    };
    host.save()?;
    text(r)
}

// ------------------------------------------------------------------------------ icons

fn search_icons(a: &Args) -> ToolResult {
    let root = crate::icons::dir();
    let limit = a.num("limit").map_or(20, |n| n.clamp(1.0, 200.0) as usize);
    let mut v = json!({"installed": crate::icons::installed(&root)
        .into_iter()
        .map(|(s, n)| json!({"set": s, "icons": n}))
        .collect::<Vec<_>>()});
    match a.str("query").filter(|q| !q.is_empty()) {
        Some(q) => v["results"] = json!(crate::icons::search(&root, q, a.str("set"), limit)),
        None => {
            v["installable"] = json!(
                crate::icons::SETS
                    .iter()
                    .map(|s| json!({"set": s.id, "license": s.license, "about": s.about}))
                    .collect::<Vec<_>>()
            );
            v["install"] = json!("papercraft-cli icons add <set>");
        }
    }
    text(v)
}

/// Link a main component (or the main of an instance) to its code in a UI library.
fn set_code_link(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, id) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
    let doc = ed.page_doc(page);
    let n = doc.get(id).ok_or("no such node")?;
    let main = match (&n.instance, n.component) {
        (_, true) => id,
        (Some(l), false) if l.target().1.is_none() && doc.get(l.target().0).is_some() => l.target().0,
        (Some(_), false) => return Err("the main component is on another page: link it there".into()),
        (None, false) => return Err("set_code_link needs a main component or an instance of one (create_component first)".into()),
    };
    let s = |k: &str| a.str(k).unwrap_or("").to_owned();
    let link = crate::model::CodeLink { kit: s("kit"), import: s("import"), react: s("react"), egui: s("egui") };
    let clear = link.import.is_empty() && link.react.is_empty() && link.egui.is_empty();
    let name = doc.get(main).map(|m| m.name.clone()).unwrap_or_default();
    ed.with_page(page, |ed| {
        ed.edit("Set code link", |tx| {
            tx.node_mut(main).unwrap().code = (!clear).then_some(link);
        })
    });
    text(json!({"component": node_key(ed, page, main), "name": name, "linked": !clear}))
}

/// The JSON a tool call answers with (its first text block).
fn call_json(host: &mut dyn Host, name: &str, args: Value) -> Result<Value, String> {
    let outs = call(host, name, &args)?;
    let t = outs.iter().find_map(|o| if let Out::Text(t) = o { Some(t.as_str()) } else { None }).ok_or_else(|| format!("{name} answered no text"))?;
    serde_json::from_str(t).map_err(|e| format!("{name}: {e}"))
}

/// Build a "UI Kit / <name>" page from a kit (`crate::ui_kits`): its variables (Light and Dark),
/// tokens, and its components, each a main component linked to the library's code.
fn apply_ui_kit(host: &mut dyn Host, a: &Args) -> ToolResult {
    let id = a.req_str("kit")?;
    let Some(kit) = crate::ui_kits::kit(id) else {
        let all: Vec<&str> = crate::ui_kits::KITS.iter().map(|k| k.id).collect();
        return Err(format!("no kit `{id}` (kits: {})", all.join(", ")));
    };
    let map = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<Map<String, Value>>();
    let page = call_json(host, "create_page", json!({"name": format!("UI Kit / {}", kit.name)}))?;
    let pid = page["pageId"].as_str().ok_or("create_page gave no page")?.to_owned();
    call_json(host, "set_variables", json!({"pageId": pid, "variables": map(kit.light), "mode": "Light"}))?;
    call_json(host, "set_variables", json!({"pageId": pid, "variables": map(kit.dark), "mode": "Dark"}))?;
    call_json(host, "set_tokens", json!({"pageId": pid, "tokens": map(kit.tokens)}))?;
    let body: String = kit.components.iter().map(|c| c.html).collect();
    let html = format!(
        r#"<div layer-name="UI Kit / {}" style="display:flex;flex-wrap:wrap;gap:32px;align-items:flex-start;padding:48px;width:1200px;background:var(--{})">{body}</div>"#,
        kit.name, kit.surface
    );
    let written = call_json(host, "write_html", json!({"pageId": pid, "targetNodeId": "root", "html": html}))?;
    let board = written["createdNodeIds"][0].as_str().ok_or("write_html made no frame")?.to_owned();
    let kids = call_json(host, "get_children", json!({"pageId": pid, "nodeId": board}))?;
    let mut made = Vec::new();
    for c in kit.components {
        let Some(node) = kids["children"].as_array().into_iter().flatten().find(|k| k["name"] == c.name).and_then(|k| k["id"].as_str()) else { continue };
        let comp = call_json(host, "create_component", json!({"pageId": pid, "nodeId": node}))?;
        let cid = comp["id"].as_str().unwrap_or(node).to_owned();
        call_json(host, "set_code_link", json!({"pageId": pid, "nodeId": cid, "kit": kit.id, "import": c.import, "react": c.react, "egui": c.egui}))?;
        made.push(json!({"name": c.name, "id": cid}));
    }
    text(json!({
        "pageId": pid, "frame": board, "kit": kit.id, "components": made,
        "next": "Place these with <x-paper-instance component=\"Name\" overrides='{\"Label\":\"Text\"}'/>; the code export and the AI spec use the library components for them. Use the kit's variables (var(--primary) and so on) for everything else."
    }))
}

fn rename_nodes(ed: &mut Editor, a: &Args) -> ToolResult {
    let n = batch(
        ed,
        a,
        "updates",
        "Rename",
        |u| Ok(u.req_str("name")?.chars().take(50).collect::<String>()),
        |tx, id, name| {
            tx.node_mut(id).unwrap().name = name.clone();
            crate::editor::structure::mark_overrides(tx, id, &["name"]);
            Ok(())
        },
    )?;
    text(json!({"renamed": n}))
}

fn delete_nodes(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let mut missing = Vec::new();
    let mut by_page: Vec<(usize, Vec<NodeId>)> = Vec::new();
    for k in a.strs("nodeIds")? {
        match resolve(ed, &k, def) {
            Ok((p, id)) => match by_page.iter_mut().find(|(q, _)| *q == p) {
                Some((_, l)) => l.push(id),
                None => by_page.push((p, vec![id])),
            },
            Err(_) => missing.push(k),
        }
    }
    let mut deleted = 0;
    for (page, ids) in by_page {
        deleted += ed.with_page(page, |ed| {
            let n = ed.edit("Delete", |tx| {
                let mut n = 0;
                for id in &ids {
                    if tx.doc().get(*id).is_some() {
                        tx.remove(*id);
                        n += 1;
                    }
                }
                n
            });
            ed.selection.retain(|s| ed.doc.get(*s).is_some());
            n
        });
    }
    let mut v = json!({"deleted": deleted});
    if !missing.is_empty() {
        v["notFound"] = json!(missing);
    }
    text(v)
}

fn duplicate_nodes(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let top_to_page = match a.str("toPageId").filter(|p| !p.is_empty()) {
        Some(p) => Some(find_page(ed, p)?),
        None => None,
    };
    let mut jobs = Vec::new();
    for item in a.arr("nodes")? {
        let item = Args(item.as_object().ok_or("each item must be an object")?);
        let (src_page, id) = resolve(ed, item.req_str("id")?, def)?;
        let item_to_page = match item.str("toPageId").filter(|p| !p.is_empty()) {
            Some(p) => Some(find_page(ed, p)?),
            None => top_to_page,
        };
        let (dest_page, parent) = match item.str("parentId").filter(|p| !p.is_empty()) {
            None => match item_to_page {
                Some(dp) if dp != src_page => (dp, Some(None)),
                _ => (src_page, None),
            },
            Some(ROOT) => (item_to_page.unwrap_or(src_page), Some(None)),
            Some(p) => {
                let (pp, pid) = resolve(ed, p, item_to_page.unwrap_or(src_page))?;
                if !ed.page_doc(pp).get(pid).unwrap().is_container() {
                    return Err("parentId must be a frame or group".into());
                }
                (pp, Some(Some(pid)))
            }
        };
        jobs.push((src_page, dest_page, id, parent));
    }
    let mut out = Vec::new();
    for (src_page, dest_page, id, parent) in jobs {
        let (new, pairs) = if src_page == dest_page {
            ed.with_page(src_page, |ed| {
                ed.edit("Duplicate", |tx| {
                    let n = tx.doc().get(id).unwrap();
                    let same_parent = parent.is_none_or(|p| p == n.parent);
                    let target = parent.unwrap_or(n.parent);
                    let index = if same_parent {
                        tx.doc()
                            .children(target)
                            .iter()
                            .position(|c| *c == id)
                            .map(|i| i + 1)
                    } else {
                        None
                    };
                    let offset = if target.is_none() && n.is_frame() {
                        let w = tx.doc().world_rect(id).unwrap();
                        free_spot(tx.doc(), w.w) - w.min()
                    } else {
                        V2::ZERO
                    };
                    let r = clone_subtree(tx, id, target, index, offset);
                    let from = tx.doc().annotations.clone();
                    copy_annotations(tx, &from, &r.1);
                    relayout(tx, Some(r.0));
                    r
                })
            })
        } else {
            let src_doc = ed.page_doc(src_page).clone();
            let ids = (ed.pages[src_page].id.clone(), ed.pages[dest_page].id.clone());
            ed.with_page(dest_page, |ed| {
                ed.edit("Duplicate", |tx| {
                    let n = src_doc.get(id).unwrap();
                    let target = parent.flatten();
                    let w = src_doc.world_rect(id).unwrap_or(n.rect);
                    let offset = if target.is_none() {
                        free_spot(tx.doc(), w.w) - w.min()
                    } else {
                        -w.min()
                    };
                    let r = clone_subtree_from_doc(tx, &src_doc, id, target, None, offset, Some((&ids.0, &ids.1)), false);
                    copy_annotations(tx, &src_doc.annotations, &r.1);
                    relayout(tx, target.or(Some(r.0)));
                    r
                })
            })
        };
        let map: Map<String, Value> = pairs
            .iter()
            .filter(|(o, _)| *o != id)
            .map(|(o, n)| {
                (
                    node_key(ed, src_page, *o),
                    json!(node_key(ed, dest_page, *n)),
                )
            })
            .collect();
        out.push(json!({
            "sourceId": node_key(ed, src_page, id),
            "newId": node_key(ed, dest_page, new),
            "descendantIdMap": map,
        }));
    }
    text(json!({"duplicates": out}))
}

fn move_nodes(ed: &mut Editor, a: &Args) -> ToolResult {
    let def = page_arg(ed, a)?;
    let top_to_page = match a.str("toPageId").filter(|p| !p.is_empty()) {
        Some(p) => Some(find_page(ed, p)?),
        None => None,
    };
    let moves = a.arr("moves")?;
    let mut done = 0;
    for m in moves {
        let m = Args(m.as_object().ok_or("each move must be an object")?);
        let (src_page, id) = resolve(ed, m.req_str("nodeId")?, def)?;
        let mut dest_page = match m.str("toPageId").filter(|p| !p.is_empty()) {
            Some(p) => find_page(ed, p)?,
            None => top_to_page.unwrap_or(src_page),
        };
        let sib = |k: &str, dp: &mut usize| -> Result<Option<NodeId>, String> {
            match m.str(k).filter(|s| !s.is_empty()) {
                None => Ok(None),
                Some(s) => {
                    let (p, sid) = resolve(ed, s, *dp)?;
                    *dp = p;
                    Ok(Some(sid))
                }
            }
        };
        let before = sib("before", &mut dest_page)?;
        let after = sib("after", &mut dest_page)?;
        let parent: Option<Option<NodeId>> = match m.str("parentId").filter(|p| !p.is_empty()) {
            None => {
                if dest_page != src_page && before.is_none() && after.is_none() {
                    Some(None)
                } else {
                    None
                }
            }
            Some(ROOT) => Some(None),
            Some(p) => {
                let (pp, pid) = resolve(ed, p, dest_page)?;
                dest_page = pp;
                Some(Some(pid))
            }
        };
        let index = m.num("index").map(|i| i.max(0.0) as usize);
        if dest_page == src_page {
            ed.with_page(src_page, |ed| {
                ed.edit("Move", |tx| -> Result<(), String> {
                    let doc = tx.doc();
                    let (target, sibling) = if let Some(s) = before.or(after) {
                        (doc.get(s).unwrap().parent, Some(s))
                    } else if let Some(p) = parent {
                        (p, None)
                    } else {
                        return Err("each move needs before, after or parentId".into());
                    };
                    if sibling == Some(id) {
                        return Err("a node can't be moved next to itself".into());
                    }
                    if let Some(t) = target {
                        if t == id || doc.is_ancestor(id, t) {
                            return Err("can't move a node into itself or its descendants".into());
                        }
                        if !doc.get(t).unwrap().is_container() {
                            return Err("the destination can't have children".into());
                        }
                    }
                    tx.reparent(id, target, None);
                    let list: Vec<NodeId> = tx
                        .doc()
                        .children(target)
                        .iter()
                        .copied()
                        .filter(|c| *c != id)
                        .collect();
                    let i = match sibling {
                        Some(s) => {
                            list.iter().position(|c| *c == s).unwrap()
                                + usize::from(after.is_some())
                        }
                        None => index.unwrap_or(list.len()).min(list.len()),
                    };
                    tx.move_to_index(id, i);
                    Ok(())
                })
            })?;
        } else {
            let ddoc = ed.page_doc(dest_page);
            let (target, sibling) = if let Some(s) = before.or(after) {
                (ddoc.get(s).unwrap().parent, Some(s))
            } else if let Some(p) = parent {
                (p, None)
            } else {
                return Err("each move needs before, after, parentId or toPageId".into());
            };
            if let Some(t) = target
                && !ddoc.get(t).unwrap().is_container()
            {
                return Err("the destination can't have children".into());
            }
            let src_doc = ed.page_doc(src_page).clone();
            let ids = (ed.pages[src_page].id.clone(), ed.pages[dest_page].id.clone());
            // Notes on the moved nodes go with them instead of staying behind detached.
            let sub: HashSet<NodeId> = src_doc.subtree(id).into_iter().collect();
            let notes: Vec<u64> = src_doc.annotations.iter().filter(|n| n.target.is_some_and(|t| sub.contains(&t))).map(|n| n.id).collect();
            ed.with_page(src_page, |ed| {
                ed.edit("Move", |tx| {
                    for n in &notes {
                        tx.delete_annotation(*n);
                    }
                    tx.remove(id);
                });
                ed.selection.retain(|s| ed.doc.get(*s).is_some());
            });
            let pairs = ed.with_page(dest_page, |ed| {
                ed.edit("Move", |tx| {
                    let w = src_doc
                        .world_rect(id)
                        .unwrap_or_else(|| src_doc.get(id).unwrap().rect);
                    let offset = if target.is_none() {
                        free_spot(tx.doc(), w.w) - w.min()
                    } else {
                        -w.min()
                    };
                    let list_len = tx.doc().children(target).len();
                    let idx = match sibling {
                        Some(s) => tx
                            .doc()
                            .children(target)
                            .iter()
                            .position(|c| *c == s)
                            .map(|p| p + usize::from(after.is_some())),
                        None => Some(index.unwrap_or(list_len).min(list_len)),
                    };
                    let (new_root, pairs) =
                        clone_subtree_from_doc(tx, &src_doc, id, target, idx, offset, Some((&ids.0, &ids.1)), true);
                    copy_annotations(tx, &src_doc.annotations, &pairs);
                    relayout(tx, target.or(Some(new_root)));
                    pairs
                })
            });
            relink_moved(ed, src_page, dest_page, &pairs.into_iter().collect());
        }
        done += 1;
    }
    text(json!({"moved": done}))
}

/// After nodes moved from `src` to `dest` page (`moved`: old id to new id), instances anywhere
/// that used them as their main follow them, so moving components keeps every instance linked.
fn relink_moved(ed: &mut Editor, src: usize, dest: usize, moved: &HashMap<NodeId, NodeId>) {
    ed.finish_loading();
    let (src_id, dest_id) = (ed.pages[src].id.clone(), ed.pages[dest].id.clone());
    for page in 0..ed.pages.len() {
        // On the source page the links were same-page; elsewhere they named the source page.
        let was = if page == src { None } else { Some(Source::Page(src_id.clone())) };
        let now = if page == dest { None } else { Some(Source::Page(dest_id.clone())) };
        let hits: Vec<NodeId> = ed
            .page_doc(page)
            .nodes
            .values()
            .filter(|n| n.instance.as_ref().is_some_and(|l| l.source == was && moved.contains_key(&l.main)))
            .map(|n| n.id)
            .collect();
        if hits.is_empty() {
            continue;
        }
        ed.with_page(page, |ed| {
            ed.edit("Move", |tx| {
                for id in hits {
                    let l = tx.node_mut(id).unwrap().instance.as_mut().unwrap();
                    l.main = moved[&l.main];
                    l.source = now.clone();
                }
            })
        });
    }
}

fn select_nodes(ed: &mut Editor, a: &Args) -> ToolResult {
    let page = ed.active_page;
    let (p, ids) = resolve_all(ed, &a.strs("nodeIds")?, page)?;
    if p != page {
        return Err(
            "only nodes on the page the user is viewing can be selected; call set_viewport \
             with that pageId first to switch the user's page"
                .into(),
        );
    }
    ed.select(ids.clone());
    if a.get("zoom").and_then(Value::as_bool).unwrap_or(false)
        && let Some(b) = Rect::union_all(
            &ids.iter()
                .filter_map(|i| ed.doc.world_rect(*i))
                .collect::<Vec<_>>(),
        )
    {
        ed.camera.fit(&b, ed.viewport, 64.0, 1.0);
    }
    ed.revision += 1;
    text(json!({"selected": ids.len()}))
}

/// How long after the user moved the camera agents leave it alone (as chat's follow does).
pub const USER_CAMERA_QUIET: std::time::Duration = std::time::Duration::from_secs(10);
/// Screen margin around what `set_viewport` fits, and the most it zooms in to fit.
const FIT_PADDING: f64 = 64.0;
const FIT_MAX_ZOOM: f64 = 8.0;

fn set_viewport(ed: &mut Editor, a: &Args) -> ToolResult {
    let force = a.bool("force").unwrap_or(false);
    if !force && ed.cam_moved.is_some_and(|t| t.elapsed() < USER_CAMERA_QUIET) {
        return Err("user is moving the canvas; try again later, or pass force: true".into());
    }
    let page = page_arg(ed, a)?;
    let target = match (a.get("nodeIds"), a.get("rect")) {
        (Some(_), _) => {
            let (p, ids) = resolve_all(ed, &a.strs("nodeIds")?, page)?;
            if p != page && a.str("pageId").is_some_and(|s| !s.is_empty()) {
                return Err("nodeIds are on another page than pageId".into());
            }
            let doc = ed.page_doc(p);
            let rects: Vec<Rect> = ids.iter().filter_map(|i| doc.world_rect(*i)).collect();
            Some((p, Rect::union_all(&rects).ok_or("no nodes to fit")?))
        }
        (None, Some(r)) => {
            let n: Vec<f64> = r
                .as_array()
                .map(|v| v.iter().filter_map(Value::as_f64).collect())
                .unwrap_or_default();
            let [x, y, w, h] = n[..] else {
                return Err("rect must be [x, y, width, height] in world units".into());
            };
            Some((page, Rect::new(x, y, w.max(1.0), h.max(1.0))))
        }
        (None, None) => None,
    };
    let page = target.map_or(page, |(p, _)| p);
    ed.switch_page(page);
    if ed.active_page != page {
        return Err("cannot switch to that page".into());
    }
    let zoom = a.num("zoom").map(|z| z.clamp(MIN_ZOOM, MAX_ZOOM));
    let pad = a.num("padding").unwrap_or(FIT_PADDING).max(0.0);
    match (target, zoom) {
        (Some((_, r)), Some(z)) => {
            ed.camera.zoom = z;
            ed.camera.pan = r.center() - ed.viewport / (2.0 * z);
        }
        (Some((_, r)), None) => ed.camera.fit(&r, ed.viewport, pad, FIT_MAX_ZOOM),
        (None, Some(z)) => ed.camera.zoom_about(ed.viewport * 0.5, z),
        (None, None) => {}
    }
    if target.is_some() || zoom.is_some() {
        // A page opened for the first time would refit on the next frame.
        ed.pending_fit = false;
    }
    ed.revision += 1;
    text(json!({"viewport": viewport_json(ed)}))
}

fn undo_redo(ed: &mut Editor, a: &Args, undo: bool) -> ToolResult {
    let page = page_arg(ed, a)?;
    let label = ed.with_page(page, |ed| {
        let before = ed.history.generation();
        if undo {
            ed.undo();
        } else {
            ed.redo();
        }
        (ed.history.generation() != before).then(|| ed.status.clone())
    });
    text(json!({"done": label.is_some(), "what": label}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globbing() {
        assert!(glob("Get *", "get started"));
        assert!(glob("*start*", "Get started"));
        assert!(glob("foo*bar", "foo-x-bar"));
        assert!(!glob("foo", "foobar"));
        assert!(!glob("a*c", "abd"));
    }

    #[test]
    fn text_collapses_whitespace_and_keeps_breaks() {
        let tree = html::parse("<p>  Hello\n   <b>bold</b> world<br> next </p>");
        let Html::Element(p) = &tree[0] else { panic!() };
        let mut chars = Vec::new();
        collect_runs(&p.children, Run::default(), 16.0, &mut chars);
        let s: String = collapse(chars.clone(), false)
            .iter()
            .map(|(c, _)| *c)
            .collect();
        assert_eq!(s, "Hello bold world\nnext");
        let bold: String = collapse(chars, false)
            .iter()
            .filter(|(_, r)| r.bold == Some(true))
            .map(|(c, _)| *c)
            .collect();
        assert_eq!(bold, "bold");
    }

    #[test]
    fn cross_page_components_and_layers() {
        let mut ed = Editor::default();
        let p0_id = ed.pages[0].id.clone();
        // Create an artboard with a nested component on page 0.
        write_html(
            &mut ed,
            &Args(
                json!({
                    "targetNodeId": "root",
                    "html": "<div layer-name=\"Sheet\" style=\"width: 400px; height: 300px\"><div layer-name=\"Button / Primary\" style=\"display: flex; background: #313244; padding: 8px; width: 100px; height: 40px\"><span>Save</span></div></div>"
                })
                .as_object()
                .unwrap(),
            ),
        )
        .unwrap();
        let btn_id = ed
            .page_doc(0)
            .nodes
            .values()
            .find(|n| n.name == "Button / Primary" && n.is_frame())
            .unwrap()
            .id;
        let btn_key = node_key(&ed, 0, btn_id);
        create_component(
            &mut ed,
            &Args(json!({"nodeId": btn_key}).as_object().unwrap()),
        )
        .unwrap();

        // Create page 1 and import nested component from page 0.
        create_page(
            &mut ed,
            &Args(json!({"name": "Page 2"}).as_object().unwrap()),
        )
        .unwrap();
        let p1_id = ed.pages[1].id.clone();
        let imp = import_components(
            &mut ed,
            &Args(
                json!({"fromPageId": p0_id, "pageId": p1_id})
                    .as_object()
                    .unwrap(),
            ),
        )
        .unwrap();
        let Out::Text(imp_txt) = &imp[0] else {
            panic!()
        };
        assert!(imp_txt.contains("Button / Primary"), "{imp_txt}");

        // Cross-page <x-paper-clone> from page 0 into page 1.
        let cloned = write_html(
            &mut ed,
            &Args(
                json!({
                    "pageId": p1_id,
                    "targetNodeId": "root",
                    "html": format!("<x-paper-clone node-id=\"{btn_key}\" layer-name=\"Cloned Btn\"/>")
                })
                .as_object()
                .unwrap(),
            ),
        )
        .unwrap();
        let Out::Text(cloned_txt) = &cloned[0] else {
            panic!()
        };
        assert!(cloned_txt.contains("createdNodeIds"), "{cloned_txt}");
        assert!(
            ed.page_doc(1)
                .nodes
                .values()
                .any(|n| n.name == "Cloned Btn")
        );

        // Also test feedback counts across pages.
        add_feedback(
            &mut ed,
            &Args(
                json!({"nodeId": btn_key, "text": "Make taller"})
                    .as_object()
                    .unwrap(),
            ),
        )
        .unwrap();
        let fb = get_feedback(&mut ed, &Args(json!({}).as_object().unwrap())).unwrap();
        let Out::Text(fb_txt) = &fb[0] else { panic!() };
        let fb_val: Value = serde_json::from_str(fb_txt).unwrap();
        assert_eq!(fb_val["openCount"], 1, "{fb_txt}");
        assert!(fb_val["items"][0]["pageId"].is_string(), "{fb_txt}");
    }
}
