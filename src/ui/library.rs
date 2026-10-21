//! The Assets side view: every main component of this file (grouped by page), the components of
//! each enabled library, and the personal shape library ("My shapes"). Click a tile to place it;
//! tiles do not drag onto the canvas (neither did the shape library).

use super::{UiState, kit};
use crate::editor::Editor;
use crate::geom::{Rect, V2, v2};
use crate::library::{LibItem, Library};
use crate::libraries::EnabledStatus;
use crate::model::{Document, NodeId, Source};
use crate::theme::{self, Icon};
use egui::{Color32, RichText, Ui};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[derive(Default)]
pub struct LibraryUi {
    pub library: Library,
    /// The library changed and should be saved.
    pub dirty: bool,
    /// Tile being renamed `(tile key, draft)`.
    pub renaming: Option<(String, String)>,
    /// Search box text.
    pub filter: String,
    /// Sections the user folded.
    collapsed: HashSet<String>,
    /// Enabled libraries against their sources, refreshed whenever the panel opens.
    status: Vec<EnabledStatus>,
    seen: Option<u64>,
    /// `(revision stamp, preview fingerprint, texture)` per tile.
    previews: HashMap<String, (u64, u64, Option<egui::TextureHandle>)>,
    /// `list_assets` / `missing_mains` results, rebuilt when the editor revision moves.
    assets: Option<(Stamp, String, Rc<AssetList>)>,
    missing: Option<(Stamp, Rc<Vec<NodeId>>)>,
}

/// What the cached asset lists depend on besides the filter.
type Stamp = (u64, usize, usize, usize);

fn stamp(ed: &Editor) -> Stamp {
    (ed.revision, ed.active_page, ed.pages.len(), ed.library_docs.len())
}

impl LibraryUi {
    fn assets(&mut self, ed: &Editor, filter: &str) -> Rc<AssetList> {
        let st = stamp(ed);
        match &self.assets {
            Some((s, f, l)) if *s == st && f == filter => l.clone(),
            _ => {
                let l = Rc::new(list_assets(ed, filter));
                self.assets = Some((st, filter.to_string(), l.clone()));
                l
            }
        }
    }

    fn missing(&mut self, ed: &Editor) -> Rc<Vec<NodeId>> {
        let st = stamp(ed);
        match &self.missing {
            Some((s, l)) if *s == st => l.clone(),
            _ => {
                let l = Rc::new(ed.missing_mains(ed.active_page));
                self.missing = Some((st, l.clone()));
                l
            }
        }
    }
}

/// Where a component lives.
#[derive(Clone, Debug, PartialEq)]
pub enum Origin {
    /// A page of this file (index).
    Page(usize),
    /// An enabled library (id).
    Library(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub origin: Origin,
    pub id: NodeId,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct AssetList {
    /// `(page name, components)`, pages in order, empty pages left out.
    pub pages: Vec<(String, Vec<Asset>)>,
    /// `(library id, library name, components)` for every enabled library.
    pub libraries: Vec<(String, String, Vec<Asset>)>,
}

impl AssetList {
    pub fn this_file(&self) -> usize {
        self.pages.iter().map(|p| p.1.len()).sum()
    }
}

fn mains(doc: &Document, origin: &Origin, needle: &str) -> Vec<Asset> {
    let mut out: Vec<Asset> = doc
        .nodes
        .values()
        .filter(|n| n.component && n.instance.is_none())
        .filter(|n| needle.is_empty() || n.name.to_lowercase().contains(needle))
        .map(|n| Asset {
            origin: origin.clone(),
            id: n.id,
            key: n.component_key.clone().unwrap_or_else(|| n.id.0.to_string()),
            name: n.name.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    out
}

/// The components the Assets view shows, filtered by a case-insensitive name search.
pub fn list_assets(ed: &Editor, filter: &str) -> AssetList {
    let needle = filter.trim().to_lowercase();
    let pages = (0..ed.pages.len())
        .map(|i| (ed.pages[i].name.clone(), mains(ed.page_doc(i), &Origin::Page(i), &needle)))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    let libraries = ed
        .design
        .libraries
        .iter()
        .filter_map(|l| {
            let doc = ed.library_docs.get(&l.library_id)?;
            let o = Origin::Library(l.library_id.clone());
            Some((l.library_id.clone(), doc.name.clone(), mains(doc, &o, &needle)))
        })
        .filter(|(_, _, v)| needle.is_empty() || !v.is_empty())
        .collect();
    AssetList { pages, libraries }
}

const OTHER_GROUP: &str = "Other";

/// `"A / B / C"` -> `(Some("A"), "B / C")`; a name without ` / ` has no group.
pub fn split_name(name: &str) -> (Option<&str>, &str) {
    match name.split_once(" / ") {
        Some((g, rest)) if !g.trim().is_empty() => (Some(g.trim()), rest),
        _ => (None, name),
    }
}

/// Components grouped by the part of the name before the first ` / `, groups by name,
/// names without a group under "Other" last.
pub fn group_assets(assets: &[Asset]) -> Vec<(String, Vec<Asset>)> {
    let mut out: Vec<(String, Vec<Asset>)> = Vec::new();
    for a in assets {
        let g = split_name(&a.name).0.unwrap_or(OTHER_GROUP);
        match out.iter_mut().find(|(n, _)| n == g) {
            Some((_, v)) => v.push(a.clone()),
            None => out.push((g.to_string(), vec![a.clone()])),
        }
    }
    out.sort_by_key(|(n, _)| (n == OTHER_GROUP, n.to_lowercase()));
    out
}

fn group_head(ui: &mut Ui, lib: &mut LibraryUi, key: &str, name: &str, count: usize, force_open: bool) -> bool {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
    let open = force_open || !lib.collapsed.contains(key);
    if resp.clicked() && !force_open {
        if open {
            lib.collapsed.insert(key.to_string());
        } else {
            lib.collapsed.remove(key);
        }
    }
    let chevron = if open { Icon::ChevronDown } else { Icon::ChevronRight };
    let c = rect.left_center() + egui::vec2(7.0, 0.0);
    theme::paint_icon(ui.painter(), egui::Rect::from_center_size(c, egui::vec2(12.0, 12.0)), chevron, p.secondary);
    let g = ui.painter().text(
        rect.left_center() + egui::vec2(20.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        theme::semibold(theme::BODY + 1.0),
        p.text,
    );
    ui.painter().text(
        egui::pos2(g.right() + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        count.to_string(),
        egui::FontId::proportional(theme::SECONDARY),
        p.tertiary,
    );
    open
}

fn grouped_grid(
    ui: &mut Ui,
    ed: &Editor,
    lib: &mut LibraryUi,
    section: &str,
    assets: &[Asset],
    searching: bool,
    acts: &mut Vec<Act>,
) {
    for (g, items) in group_assets(assets) {
        let key = format!("{section}|{g}");
        if group_head(ui, lib, &key, &g, items.len(), searching) {
            ui.add_space(2.0);
            asset_grid(ui, ed, lib, &items, acts);
        }
    }
}

fn asset_doc<'a>(ed: &'a Editor, a: &Asset) -> Option<&'a Document> {
    match &a.origin {
        Origin::Page(i) => Some(ed.page_doc(*i)),
        Origin::Library(l) => ed.library_docs.get(l),
    }
}

/// A component with exactly this name, preferring this page, then other pages, then libraries.
fn find_named(ed: &Editor, name: &str) -> Option<Asset> {
    let pages = std::iter::once(ed.active_page).chain((0..ed.pages.len()).filter(|i| *i != ed.active_page));
    let from_pages = pages.map(|i| mains(ed.page_doc(i), &Origin::Page(i), &name.to_lowercase()));
    let from_libs = ed
        .library_docs
        .iter()
        .map(|(l, d)| mains(d, &Origin::Library(l.clone()), &name.to_lowercase()));
    from_pages.chain(from_libs).flatten().find(|a| a.name == name)
}

/// The subtree of component `id` as a standalone document, for previews.
fn component_doc(src: &Document, id: NodeId) -> Document {
    let mut d = Document::default();
    for i in src.subtree(id) {
        let Some(mut n) = src.get(i).cloned() else { continue };
        if i == id {
            n.parent = None;
        }
        d.next_id = d.next_id.max(i.0 + 1);
        d.nodes.insert(i, n);
    }
    d.root = vec![id];
    d
}

/// The source document, source link and size for placing `a`.
fn source_of(ed: &Editor, a: &Asset) -> Option<(Option<Document>, Option<Source>, V2)> {
    let doc = asset_doc(ed, a)?;
    let m = doc.get(a.id)?;
    let size = v2(m.rect.w, m.rect.h);
    Some(match &a.origin {
        Origin::Page(i) if *i == ed.active_page => (None, None, size),
        Origin::Page(i) => (Some(doc.clone()), Some(Source::Page(ed.pages[*i].id.clone())), size),
        Origin::Library(l) => (Some(doc.clone()), Some(Source::Library(l.clone())), size),
    })
}

/// Place an instance of `a` on the active page, centred on `at` (world), and select it.
pub fn place(ed: &mut Editor, a: &Asset, at: V2) -> Option<NodeId> {
    let (src, source, size) = source_of(ed, a)?;
    let rect = Rect::new(at.x - size.x / 2.0, at.y - size.y / 2.0, size.x, size.y);
    let id = ed.edit("Create instance", |tx| {
        crate::editor::structure::instantiate_from(tx, src.as_ref(), source, a.id, None, None, Some(rect))
    });
    ed.selection = vec![id];
    Some(id)
}

/// Point instance root `inst` (active page) at component `a`.
fn restore(ed: &mut Editor, inst: NodeId, a: &Asset) {
    let Some((src, source, _)) = source_of(ed, a) else { return };
    ed.edit("Swap instance", |tx| {
        crate::editor::structure::swap_instance_tx(tx, src.as_ref(), source, inst, a.id)
    });
}

fn go_to_main(ed: &mut Editor, a: &Asset) {
    let Origin::Page(i) = a.origin else { return };
    ed.switch_page(i);
    ed.selection = vec![a.id];
    if let Some(r) = ed.doc.world_rect(a.id) {
        ed.camera.fit(&r, ed.viewport, 80.0, ed.camera.zoom.max(1.0));
        ed.revision += 1;
    }
}

fn rename(ed: &mut Editor, a: &Asset, name: String) {
    let Origin::Page(i) = a.origin else { return };
    ed.with_page(i, |ed| {
        ed.edit("Rename component", |tx| {
            if let Some(n) = tx.node_mut(a.id) {
                n.name = name;
            }
        })
    });
}

const TILE_PX: (u32, u32) = (268, 168);
const TILE_GAP: f32 = 8.0;
const TILE_RATIO: f32 = 0.63;
const MAX_PREVIEW_SCALE: f32 = 2.0;
const PREVIEW_MARGIN_PX: f32 = 28.0;

/// Rasterises a document (anti-aliased, transparent) to fit a tile.
fn preview_image(doc: &Document) -> Option<egui::ColorImage> {
    let (pw, ph) = TILE_PX;
    let svg = crate::io::svg::export(doc, None, None)?;
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let scale = ((pw as f32 - PREVIEW_MARGIN_PX) / w.max(1.0))
        .min((ph as f32 - PREVIEW_MARGIN_PX) / h.max(1.0))
        .min(MAX_PREVIEW_SCALE);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph)?;
    let transform = resvg::tiny_skia::Transform::from_translate(
        (pw as f32 - w * scale) / 2.0,
        (ph as f32 - h * scale) / 2.0,
    )
    .pre_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied([pw as usize, ph as usize], pixmap.data()))
}

enum Act {
    Place(Asset),
    Go(Asset),
    Rename(Asset, String),
    Detach(NodeId),
    Restore(NodeId, Asset),
    InsertShape(LibItem),
    RenameShape(String, String),
    RemoveShape(String),
    AddSelection,
    Close,
}

fn text_action(ui: &mut Ui, label: &str) -> egui::Response {
    let font = theme::semibold(theme::BODY);
    let color = if ui.is_enabled() { theme::accent() } else { theme::pal(ui).tertiary };
    let g = ui.painter().layout_no_wrap(label.into(), font, color);
    let (rect, resp) = ui.allocate_exact_size(g.size(), egui::Sense::click());
    ui.painter().galley(rect.min, g, color);
    resp
}

/// Foldable section title: chevron, small caps title, then `right` aligned at the end.
/// Returns whether the section is open.
fn section_head(
    ui: &mut Ui,
    lib: &mut LibraryUi,
    id: &str,
    icon: Option<Icon>,
    title: &str,
    right: impl FnOnce(&Ui, egui::Rect),
) -> bool {
    let p = theme::pal(ui);
    ui.add_space(4.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
    let open = !lib.collapsed.contains(id);
    if resp.clicked() {
        if open {
            lib.collapsed.insert(id.to_string());
        } else {
            lib.collapsed.remove(id);
        }
    }
    let at = rect.left_center();
    let chevron = if open { Icon::ChevronDown } else { Icon::ChevronRight };
    let icon_rect = |x: f32| egui::Rect::from_center_size(egui::pos2(at.x + x, at.y), egui::vec2(14.0, 14.0));
    let mut x = 7.0;
    if let Some(i) = icon {
        theme::paint_icon(ui.painter(), icon_rect(7.0), i, p.secondary);
    } else {
        theme::paint_icon(ui.painter(), icon_rect(7.0), chevron, p.secondary);
    }
    x += 14.0;
    ui.painter().text(
        egui::pos2(at.x + x, at.y),
        egui::Align2::LEFT_CENTER,
        title.to_uppercase(),
        theme::semibold(theme::SECONDARY),
        p.secondary,
    );
    right(ui, rect);
    open
}

fn right_text(ui: &Ui, rect: egui::Rect, text: &str, color: Color32) -> egui::Rect {
    ui.painter().text(
        rect.right_center() - egui::vec2(2.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(theme::BODY),
        color,
    )
}

/// A preview tile: rounded cell with the image, centred, and the name below.
fn tile(ui: &mut Ui, w: f32, tex: Option<&egui::TextureHandle>) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, (w * TILE_RATIO).round()), egui::Sense::click());
    let bg = if resp.hovered() { p.hover } else { p.control };
    ui.painter().rect_filled(rect, 12.0, bg);
    if let Some(t) = tex {
        ui.painter().image(
            t.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    resp
}

/// A preview texture rebuilt only when `fp` (computed once per `rev`, visible tiles only) changes.
fn cached(
    ui: &Ui,
    lib: &mut LibraryUi,
    key: String,
    rev: u64,
    visible: bool,
    fp: impl FnOnce() -> u64,
    build: impl FnOnce() -> Option<egui::ColorImage>,
) -> Option<egui::TextureHandle> {
    let e = lib.previews.entry(key.clone()).or_insert((u64::MAX, u64::MAX, None));
    if e.0 != rev && visible {
        let fp = fp();
        if e.1 != fp || e.2.is_none() {
            let tex = build().map(|img| ui.ctx().load_texture(format!("asset-{key}"), img, egui::TextureOptions::LINEAR));
            *e = (rev, fp, tex);
        } else {
            e.0 = rev;
        }
    }
    e.2.clone()
}

/// Hash of the component's subtree: edits elsewhere on the page leave it unchanged.
fn subtree_fingerprint(doc: &Document, id: NodeId) -> u64 {
    let nodes: Vec<&crate::model::Node> = doc.subtree(id).into_iter().filter_map(|i| doc.get(i)).collect();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&nodes).unwrap_or_default().hash(&mut h);
    h.finish()
}

/// The name under a tile, or its rename box. Returns the new name once editing ends.
fn name_label(ui: &mut Ui, lib: &mut LibraryUi, key: &str, name: &str, w: f32) -> Option<String> {
    if lib.renaming.as_ref().is_some_and(|(k, _)| k == key) {
        let (_, draft) = lib.renaming.as_mut().unwrap();
        let r = ui.add(
            egui::TextEdit::singleline(draft)
                .margin(theme::INPUT_MARGIN)
                .desired_width(w),
        );
        r.request_focus();
        if r.lost_focus() {
            return lib.renaming.take().map(|r| r.1);
        }
        return None;
    }
    ui.add(egui::Label::new(RichText::new(name).size(theme::SECONDARY)).truncate());
    None
}

fn asset_grid(
    ui: &mut Ui,
    ed: &Editor,
    lib: &mut LibraryUi,
    assets: &[Asset],
    acts: &mut Vec<Act>,
) {
    let w = ((ui.available_width() - TILE_GAP) / 2.0).floor();
    for row in assets.chunks(2) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = TILE_GAP;
            for a in row {
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let tile_key = format!("{:?}|{}", a.origin, a.id.0);
                    let (rev, doc) = match &a.origin {
                        Origin::Page(_) => (ed.revision, asset_doc(ed, a)),
                        Origin::Library(l) => (
                            ed.design.libraries.iter().find(|x| x.library_id == *l).map_or(0, |x| x.version),
                            asset_doc(ed, a),
                        ),
                    };
                    let tex_key = format!("{tile_key}|{}", a.key);
                    let (rect, _) = ui.allocate_exact_size(egui::Vec2::ZERO, egui::Sense::hover());
                    let visible = ui.clip_rect().intersects(egui::Rect::from_min_size(rect.min, egui::vec2(w, w * TILE_RATIO)));
                    let fp = || doc.map_or(rev, |d| match a.origin {
                        Origin::Page(_) => subtree_fingerprint(d, a.id),
                        Origin::Library(_) => rev,
                    });
                    let tex = cached(ui, lib, tex_key, rev, visible, fp, || preview_image(&component_doc(doc?, a.id)));
                    let resp = tile(ui, w, tex.as_ref());
                    if let Some(n) = name_label(ui, lib, &tile_key, split_name(&a.name).1, w) {
                        acts.push(Act::Rename(a.clone(), n));
                    }
                    if resp.clicked() {
                        acts.push(Act::Place(a.clone()));
                    }
                    resp.on_hover_text("Click to place an instance").context_menu(|ui| {
                        if let Origin::Page(_) = a.origin {
                            if super::icon_menu_item(ui, Icon::Component, "Go to main").clicked() {
                                acts.push(Act::Go(a.clone()));
                                ui.close();
                            }
                            if super::icon_menu_item(ui, Icon::Text, "Rename").clicked() {
                                lib.renaming = Some((tile_key.clone(), a.name.clone()));
                                ui.close();
                            }
                        }
                        if super::icon_menu_item(ui, Icon::Plus, "Place instance").clicked() {
                            acts.push(Act::Place(a.clone()));
                            ui.close();
                        }
                    });
                });
            }
        });
        ui.add_space(6.0);
    }
}

fn shape_grid(ui: &mut Ui, lib: &mut LibraryUi, acts: &mut Vec<Act>) {
    let w = ((ui.available_width() - TILE_GAP) / 2.0).floor();
    let items = lib.library.items.clone();
    for row in items.chunks(2) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = TILE_GAP;
            for it in row {
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let fp = (it.nodes.len() ^ (it.name.len() << 16)) as u64;
                    let key = format!("shape|{}", it.id);
                    let tex = cached(ui, lib, key.clone(), fp, true, || fp, || preview_image(&it.doc()));
                    let resp = tile(ui, w, tex.as_ref());
                    if let Some(n) = name_label(ui, lib, &it.id, &it.name, w) {
                        acts.push(Act::RenameShape(it.id.clone(), n));
                    }
                    if resp.clicked() {
                        acts.push(Act::InsertShape(it.clone()));
                    }
                    resp.on_hover_text("Click to insert").context_menu(|ui| {
                        if super::icon_menu_item(ui, Icon::Plus, "Insert").clicked() {
                            acts.push(Act::InsertShape(it.clone()));
                            ui.close();
                        }
                        if super::icon_menu_item(ui, Icon::Text, "Rename").clicked() {
                            lib.renaming = Some((it.id.clone(), it.name.clone()));
                            ui.close();
                        }
                        if super::icon_menu_item(ui, Icon::Trash, "Remove from library").clicked() {
                            acts.push(Act::RemoveShape(it.id.clone()));
                            ui.close();
                        }
                    });
                });
            }
        });
        ui.add_space(6.0);
    }
}

/// Re-reads the enabled libraries' status when the panel was not drawn on the previous frame.
fn refresh_status(ui: &Ui, ed: &Editor, st: &mut UiState) {
    let now = ui.ctx().cumulative_frame_nr();
    if st.library.seen.is_some_and(|s| s + 1 >= now) {
        st.library.seen = Some(now);
        return;
    }
    st.library.seen = Some(now);
    st.library.status = st
        .project_dir
        .as_deref()
        .and_then(|d| {
            let store = crate::project::Store::open(d.parent()?).ok()?;
            Some(crate::libraries::status(&store, d, ed))
        })
        .unwrap_or_default();
}

fn header(ui: &mut Ui, st: &mut UiState, acts: &mut Vec<Act>) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
        theme::paint_icon(ui.painter(), r, Icon::Component, theme::accent());
        ui.label(RichText::new("Assets").font(theme::semibold(15.0)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::close_button(ui).clicked() {
                acts.push(Act::Close);
            }
            if theme::icon_button(ui, Icon::Library, 28.0, st.libraries_open)
                .on_hover_text("Libraries")
                .clicked()
            {
                st.libraries_open = true;
            }
        });
    });
    ui.add_space(6.0);
    ui.add(
        egui::TextEdit::singleline(&mut st.library.filter)
            .hint_text("Search assets")
            .desired_width(f32::INFINITY)
            .margin(theme::INPUT_MARGIN),
    );
    ui.add_space(6.0);
}

fn missing_rows(ui: &mut Ui, ed: &Editor, missing: &[NodeId], acts: &mut Vec<Act>) {
    let p = theme::pal(ui);
    for &inst in missing {
        let name = ed.doc.get(inst).map_or_else(String::new, |n| n.name.clone());
        let target = find_named(ed, &name);
        egui::Frame::new()
            .fill(p.field)
            .corner_radius(theme::RADIUS_PANEL)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("!").font(theme::semibold(16.0)).color(p.warning));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new("Main missing").font(theme::semibold(theme::BODY)));
                        ui.add(egui::Label::new(RichText::new(&name).size(theme::SECONDARY).color(p.secondary)).truncate());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        ui.add_enabled_ui(target.is_some(), |ui| {
                            if text_action(ui, "Restore").on_hover_text("Point at the component with this name").clicked()
                                && let Some(t) = &target
                            {
                                acts.push(Act::Restore(inst, t.clone()));
                            }
                        });
                        if text_action(ui, "Detach").on_hover_text("Turn into a plain frame").clicked() {
                            acts.push(Act::Detach(inst));
                        }
                    });
                });
            });
        ui.add_space(6.0);
    }
}

pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let mut acts = Vec::new();
    refresh_status(ui, ed, st);
    header(ui, st, &mut acts);
    let filter = st.library.filter.trim().to_lowercase();
    let list = st.library.assets(ed, &filter);
    let missing = st.library.missing(ed);
    let shapes: Vec<LibItem> = st
        .library
        .library
        .items
        .iter()
        .filter(|i| filter.is_empty() || i.name.to_lowercase().contains(&filter))
        .cloned()
        .collect();
    let can_add = !ed.top_selection().is_empty();
    let lib = &mut st.library;
    let p = theme::pal(ui);
    let nothing = list.this_file() == 0
        && list.libraries.iter().all(|l| l.2.is_empty())
        && shapes.is_empty();
    egui::ScrollArea::vertical()
        .id_salt("assets")
        .auto_shrink(false)
        .show(ui, |ui| {
            theme::figma_fields(ui);
            missing_rows(ui, ed, &missing, &mut acts);
            if nothing {
                ui.add_space(24.0);
                let (title, body) = if filter.is_empty() {
                    ("No assets yet", "Make a component, enable a library, or add layers to My shapes.")
                } else {
                    ("No matches", "Nothing in this file, your libraries or My shapes has that name.")
                };
                kit::empty_state(ui, Icon::Component, theme::accent(), title, body, |_| {});
            }
            if list.this_file() > 0 {
                let n = list.this_file();
                let open = section_head(ui, lib, "file", None, "This file", |ui, r| {
                    right_text(ui, r, &n.to_string(), p.tertiary);
                });
                for (name, assets) in list.pages.iter().filter(|_| open) {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).font(theme::semibold(theme::BODY + 2.0)));
                        ui.label(RichText::new(assets.len().to_string()).size(theme::SECONDARY).color(p.tertiary));
                    });
                    grouped_grid(ui, ed, lib, &format!("page:{name}"), assets, !filter.is_empty(), &mut acts);
                }
            }
            for (id, name, assets) in &list.libraries {
                let update = lib.status.iter().any(|s| s.library_id == *id && s.update_available);
                let open = section_head(ui, lib, id, Some(Icon::Library), name, |ui, r| {
                    if update {
                        let x = right_text(ui, r, "Update", theme::accent()).left() - 8.0;
                        ui.painter().circle_filled(egui::pos2(x, r.center().y), 3.5, theme::accent());
                    } else {
                        right_text(ui, r, &assets.len().to_string(), p.tertiary);
                    }
                });
                if !open {
                    continue;
                }
                if assets.is_empty() {
                    ui.label(RichText::new("No components").size(theme::SECONDARY).color(p.tertiary));
                }
                grouped_grid(ui, ed, lib, id, assets, !filter.is_empty(), &mut acts);
            }
            if !shapes.is_empty() || filter.is_empty() {
                let n = shapes.len();
                let open = section_head(ui, lib, "shapes", None, "My shapes", |ui, r| {
                    right_text(ui, r, &n.to_string(), p.tertiary);
                });
                if open {
                    ui.add_space(4.0);
                    ui.add_enabled_ui(can_add, |ui| {
                        if kit::button_wide(ui, kit::ButtonKind::Secondary, Some(Icon::Plus), "Add selection")
                            .on_hover_text("Save the selected layers as a reusable item")
                            .clicked()
                        {
                            acts.push(Act::AddSelection);
                        }
                    });
                    ui.add_space(8.0);
                    shape_grid(ui, lib, &mut acts);
                }
            }
        });
    for act in acts {
        match act {
            Act::Place(a) => {
                let at = ed.camera.screen_to_world(ed.viewport * 0.5);
                place(ed, &a, at);
            }
            Act::Go(a) => go_to_main(ed, &a),
            Act::Rename(a, name) => rename(ed, &a, name),
            Act::Detach(id) => {
                ed.selection = vec![id];
                ed.detach_instance();
            }
            Act::Restore(inst, a) => restore(ed, inst, &a),
            Act::InsertShape(it) => {
                let at = ed.camera.screen_to_world(ed.viewport * 0.5);
                ed.insert_library_item(&it, at);
            }
            Act::RenameShape(id, name) => {
                if let Some(x) = st.library.library.items.iter_mut().find(|x| x.id == id) {
                    x.name = name;
                    st.library.dirty = true;
                }
            }
            Act::RemoveShape(id) => {
                st.library.library.items.retain(|x| x.id != id);
                st.library.dirty = true;
            }
            Act::AddSelection => {
                let ids = ed.top_selection();
                let name = ids
                    .first()
                    .and_then(|id| ed.doc.get(*id))
                    .map_or("Item".to_string(), |n| n.name.clone());
                if let Some(item) = LibItem::from_selection(&ed.doc, &ids, &name) {
                    st.library.library.items.push(item);
                    st.library.dirty = true;
                }
            }
            Act::Close => st.show_library = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, NodeKind};

    fn comp(ed: &mut Editor, name: &str) -> NodeId {
        let id = ed.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: false }, Rect::new(0., 0., 50., 30.)), None);
        ed.selection = vec![id];
        ed.create_component();
        ed.edit("n", |tx| tx.node_mut(id).unwrap().name = name.into());
        id
    }

    fn asset(name: &str) -> Asset {
        Asset { origin: Origin::Page(0), id: NodeId(1), key: String::new(), name: name.into() }
    }

    #[test]
    fn lists_pages_and_libraries() {
        let mut ed = Editor::default();
        comp(&mut ed, "Button");
        comp(&mut ed, "Card");
        ed.add_page_in_background(None);
        ed.with_page(1, |e| comp(e, "Toggle"));
        let mut lib = Document::default();
        lib.name = "Kit".into();
        let mut n = Node::new(NodeId(1), NodeKind::Frame { clip: false }, Rect::new(0., 0., 10., 10.));
        n.name = "Logo".into();
        n.component = true;
        n.component_key = Some("k".into());
        lib.nodes.insert(n.id, n);
        ed.library_docs.insert("kit".into(), lib);
        ed.design.libraries.push(crate::variables::EnabledLibrary { library_id: "kit".into(), version: 1 });

        let l = list_assets(&ed, "");
        assert_eq!(l.this_file(), 3);
        assert_eq!(l.pages.len(), 2);
        assert_eq!(l.pages[0].1.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Button", "Card"]);
        assert_eq!(l.libraries[0].1, "Kit");
        assert_eq!(l.libraries[0].2[0].name, "Logo");
        let g = group_assets(&[
            asset("Zed / A / B"),
            asset("Plain"),
            asset("Button / Primary"),
            asset("Button / Secondary"),
        ]);
        assert_eq!(g.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["Button", "Zed", "Other"]);
        assert_eq!(g[0].1.len(), 2);
        assert_eq!(split_name("Zed / A / B"), (Some("Zed"), "A / B"));
        assert_eq!(split_name("Plain"), (None, "Plain"));
        let f = list_assets(&ed, "tog");
        assert_eq!((f.this_file(), f.libraries.len()), (1, 0));
    }
}
