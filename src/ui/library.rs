//! The Library tab: reusable shapes with vector previews; click to insert, add the selection,
//! rename or delete items.

use super::UiState;
use crate::editor::Editor;
use crate::library::{LibItem, Library};
use crate::theme::{self, Icon};
use egui::{Color32, RichText, Ui};
use std::collections::HashMap;

#[derive(Default)]
pub struct LibraryUi {
    pub library: Library,
    /// The library changed and should be saved.
    pub dirty: bool,
    pub renaming: Option<(String, String)>,
    previews: HashMap<String, (usize, Option<egui::TextureHandle>)>,
}

/// Narrowest a tile gets; tiles grow to share the row.
const TILE: f32 = 92.0;
const TILE_GAP: f32 = 8.0;

/// Rasterises an item (anti-aliased, 256 px square, transparent) for its tile.
fn preview_image(item: &LibItem) -> Option<egui::ColorImage> {
    const PX: u32 = 256;
    let doc = item.doc();
    let svg = crate::io::svg::export(&doc, None, None)?;
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let scale = ((PX as f32 - 40.0) / w.max(h).max(1.0)).min(3.0);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(PX, PX)?;
    let transform = resvg::tiny_skia::Transform::from_translate(
        (PX as f32 - w * scale) / 2.0,
        (PX as f32 - h * scale) / 2.0,
    )
    .pre_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied(
        [PX as usize, PX as usize],
        pixmap.data(),
    ))
}

pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    let lib = &mut st.library;
    let can_add = !ed.top_selection().is_empty();
    ui.add_enabled_ui(can_add, |ui| {
        if super::kit::button_wide(
            ui,
            super::kit::ButtonKind::Secondary,
            Some(Icon::Plus),
            "Add selection",
        )
        .on_hover_text("Save the selected layers as a reusable item")
        .clicked()
        {
            let ids = ed.top_selection();
            let name = ids
                .first()
                .and_then(|id| ed.doc.get(*id))
                .map_or("Item".to_string(), |n| n.name.clone());
            if let Some(item) = LibItem::from_selection(&ed.doc, &ids, &name) {
                lib.library.items.push(item);
                lib.dirty = true;
            }
        }
    });
    ui.add_space(8.0);
    if lib.library.items.is_empty() {
        ui.add_space(24.0);
        super::kit::empty_state(
            ui,
            Icon::Library,
            theme::accent(),
            "Library is empty",
            "Select layers and choose “Add selection” to keep them here.",
            |_| {},
        );
        return;
    }
    let avail = ui.available_width() - 8.0;
    let cols = ((avail + TILE_GAP) / (TILE + TILE_GAP)).floor().max(1.0) as usize;
    let tile = ((avail - TILE_GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    let items = lib.library.items.clone();
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt("library")
        .auto_shrink(false)
        .show(ui, |ui| {
            crate::theme::figma_fields(ui);
            for row in items.chunks(cols) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = TILE_GAP;
                    for it in row {
                        ui.vertical(|ui| {
                            ui.set_width(tile);
                            let (rect, resp) = ui
                                .allocate_exact_size(egui::vec2(tile, tile), egui::Sense::click());
                            let bg = if resp.hovered() { p.hover } else { p.control };
                            ui.painter().rect_filled(rect, 14.0, bg);
                            let key = it.id.clone();
                            let fp = it.nodes.len() ^ (it.name.len() << 16);
                            let entry = lib
                                .previews
                                .entry(key.clone())
                                .or_insert((usize::MAX, None));
                            if entry.0 != fp {
                                let tex = preview_image(it).map(|img| {
                                    ui.ctx().load_texture(
                                        format!("lib-{key}"),
                                        img,
                                        egui::TextureOptions::LINEAR,
                                    )
                                });
                                *entry = (fp, tex);
                            }
                            if let Some(tex) = &entry.1 {
                                ui.painter().image(
                                    tex.id(),
                                    rect.shrink(4.0),
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    Color32::WHITE,
                                );
                            }
                            let renaming =
                                lib.renaming.as_ref().is_some_and(|(id, _)| *id == it.id);
                            if renaming {
                                let (_, name) = lib.renaming.as_mut().unwrap();
                                let r = ui.add(
                                    egui::TextEdit::singleline(name)
                                        .margin(crate::theme::INPUT_MARGIN)
                                        .desired_width(tile),
                                );
                                r.request_focus();
                                if r.lost_focus() {
                                    let (id, name) = lib.renaming.take().unwrap();
                                    if let Some(x) =
                                        lib.library.items.iter_mut().find(|x| x.id == id)
                                    {
                                        x.name = name;
                                        lib.dirty = true;
                                    }
                                }
                            } else {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&it.name).size(theme::SECONDARY),
                                    )
                                    .truncate(),
                                );
                            }
                            if resp.clicked() {
                                let at = ed.camera.screen_to_world(ed.viewport * 0.5);
                                ed.insert_library_item(it, at);
                            }
                            resp.on_hover_text("Click to insert").context_menu(|ui| {
                                if super::icon_menu_item(ui, crate::theme::Icon::Plus, "Insert")
                                    .clicked()
                                {
                                    let at = ed.camera.screen_to_world(ed.viewport * 0.5);
                                    ed.insert_library_item(it, at);
                                    ui.close();
                                }
                                if super::icon_menu_item(ui, crate::theme::Icon::Text, "Rename")
                                    .clicked()
                                {
                                    lib.renaming = Some((it.id.clone(), it.name.clone()));
                                    ui.close();
                                }
                                if super::icon_menu_item(
                                    ui,
                                    crate::theme::Icon::Trash,
                                    "Remove from library",
                                )
                                .clicked()
                                {
                                    remove = Some(it.id.clone());
                                    ui.close();
                                }
                            });
                        });
                    }
                });
                ui.add_space(6.0);
            }
        });
    if let Some(id) = remove {
        lib.library.items.retain(|x| x.id != id);
        lib.dirty = true;
    }
    let _ = Icon::Library;
}
