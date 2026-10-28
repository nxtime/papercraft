//! The pages list at the top of the left sidebar (as in Figma): switch, add, rename,
//! duplicate, delete and drag to reorder.

use super::confirm::{self, Removal};
use super::{UiState, section};
use crate::editor::Editor;
use crate::theme::{self, Icon};
use egui::{Color32, RichText, Ui};

pub struct PagesUi {
    /// The pages popover is closed.
    pub collapsed: bool,
    pub renaming: Option<(usize, String)>,
    drag: Option<(String, String)>,
    group_dialog: Option<GroupDialog>,
    duplicate: Option<String>,
    /// Filters the popover's pages by name.
    query: String,
    icon_picker: Option<IconPicker>,
}

/// "Change icon…" for one page: a search over the installed icon sets.
struct IconPicker {
    page_id: String,
    query: String,
    /// The query `hits` were found for: the sets are read from disk once per query.
    searched: Option<String>,
    hits: Vec<String>,
}

struct GroupDialog {
    project_stamp: u64,
    old_name: Option<String>,
    page_id: Option<String>,
    name: String,
}

impl Default for PagesUi {
    fn default() -> Self {
        Self {
            collapsed: true,
            renaming: None,
            drag: None,
            group_dialog: None,
            duplicate: None,
            query: String::new(),
            icon_picker: None,
        }
    }
}

impl PagesUi {
    pub(super) fn clear_interaction(&mut self) {
        self.renaming = None;
        self.drag = None;
    }
}

const ROW_H: f32 = 30.0;
const MENU_WIDTH: f32 = 220.0;
/// Rows the popover shows before it scrolls.
const POPOVER_ROWS: f32 = 12.0;
/// A group's name above its pages in the popover (spacing included).
const GROUP_HEADER_H: f32 = 38.0;
const DISABLED_OPACITY: f32 = 0.4;
/// Icons the picker shows for a query.
const ICON_HITS: usize = 120;
const ICON_CELL: f32 = 36.0;
const ICON_PICKER_W: f32 = 9.0 * ICON_CELL;

/// A page's icon (`set:name` from the installed sets), or the default page icon.
fn paint_page_icon(ui: &Ui, rect: egui::Rect, icon: Option<&str>, ink: Color32) {
    if let Some(t) = icon.and_then(|i| icon_texture(ui.ctx(), i)) {
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(t.id(), rect, uv, ink);
    } else {
        theme::paint_icon(ui.painter(), rect, Icon::Page, ink);
    }
}

fn icon_texture(ctx: &egui::Context, icon: &str) -> Option<egui::TextureHandle> {
    let (set, name) = icon.split_once(':')?;
    let svg = crate::vfs::read(crate::icons::dir().join(set).join(format!("{name}.svg"))).ok()?;
    super::chat::chat_agents::svg_texture(ctx, &format!("page-icon-{icon}"), &svg)
}

pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        section(ui, "Pages");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::icon_button(ui, Icon::Group, 20.0, false)
                .on_hover_text("New page group")
                .clicked()
            {
                st.pages.group_dialog = Some(GroupDialog {
                    project_stamp: ed.project_stamp,
                    old_name: None,
                    page_id: None,
                    name: String::new(),
                });
            }
            if theme::icon_button(ui, Icon::Plus, 20.0, false)
                .on_hover_text("Add page")
                .clicked()
            {
                ed.add_page();
            }
        });
    });
    // The page selector: the current page and how many there are; click to open the full list.
    let (sel, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::click());
    let hover = super::motion::hover(ui.ctx(), resp.id.with("page-selector"), &resp);
    theme::fill_control(
        ui,
        sel,
        theme::radius_lg(),
        theme::blend(p.field, p.hover.gamma_multiply(hover)),
    );
    paint_page_icon(
        ui,
        egui::Rect::from_center_size(
            egui::pos2(sel.left() + 18.0, sel.center().y),
            egui::vec2(16.0, 16.0),
        ),
        ed.page_icons.get(ed.active_page_id()).map(String::as_str),
        theme::accent(),
    );
    ui.painter()
        .with_clip_rect(egui::Rect::from_min_max(
            egui::pos2(sel.left() + 34.0, sel.top()),
            egui::pos2(sel.right() - 74.0, sel.bottom()),
        ))
        .text(
            egui::pos2(sel.left() + 34.0, sel.center().y),
            egui::Align2::LEFT_CENTER,
            ed.page_name(),
            theme::semibold(theme::BODY),
            p.text,
        );
    let count = ed.pages.len().to_string();
    let cw = 22.0 + 7.0 * count.len() as f32;
    let chip = egui::Rect::from_center_size(
        egui::pos2(sel.right() - 44.0 - cw / 2.0 + 14.0, sel.center().y),
        egui::vec2(cw, 20.0),
    );
    let chip_fill = if theme::is_acrylic() {
        theme::well_fill(p, ui.visuals().dark_mode)
    } else {
        p.sidebar
    };
    ui.painter()
        .rect_filled(chip, egui::CornerRadius::same(255), chip_fill);
    ui.painter().text(
        chip.center(),
        egui::Align2::CENTER_CENTER,
        count,
        theme::semibold(theme::SECONDARY),
        p.secondary,
    );
    theme::paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            egui::pos2(sel.right() - 18.0, sel.center().y),
            egui::vec2(14.0, 14.0),
        ),
        if st.pages.collapsed {
            Icon::ChevronDown
        } else {
            Icon::ChevronUp
        },
        p.secondary,
    );
    if resp.on_hover_text("Show all pages").clicked() {
        st.pages.collapsed = !st.pages.collapsed;
        st.pages.query.clear();
    }
    if !st.pages.collapsed {
        popover(ui.ctx(), ed, st, sel);
    }
    if ui.input(|i| i.pointer.any_released())
        && let Some((from, to)) = st.pages.drag.take()
        && let (Some(from), Some(to)) = (
            ed.pages.iter().position(|p| p.id == from),
            ed.pages.iter().position(|p| p.id == to),
        )
    {
        ed.move_page(from, to);
    }
    if let Some(id) = st.pages.duplicate.take()
        && let Some(i) = ed.pages.iter().position(|p| p.id == id)
    {
        ed.duplicate_page(i);
    }
    group_dialog(ui.ctx(), ed, st);
    icon_picker(ui.ctx(), ed, st);
}

/// The full page list under the selector: a search field, ungrouped pages, then each group
/// under its name. Closes on a page pick, Escape or a click outside.
fn popover(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, sel: egui::Rect) {
    let area = egui::Area::new(egui::Id::new("pages-popover"))
        .order(egui::Order::Foreground)
        .fixed_pos(sel.left_bottom() + egui::vec2(0.0, 6.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                theme::popup_backing(ui);
                ui.set_width(sel.width() - 12.0);
                let search = crate::theme::lit(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut st.pages.query)
                            .hint_text("Search pages")
                            .desired_width(f32::INFINITY)
                            .margin(theme::INPUT_MARGIN),
                    )
                });
                if search.gained_focus() || !ui.memory(|m| m.focused().is_some()) && st.pages.renaming.is_none() {
                    search.request_focus();
                }
                ui.add_space(4.0);
                let q = st.pages.query.trim().to_lowercase();
                let matches = |id: &String| {
                    q.is_empty() || ed.pages.iter().any(|p| &p.id == id && p.name.to_lowercase().contains(&q))
                };
                let ungrouped: Vec<String> = ed
                    .pages
                    .iter()
                    .filter(|page| ed.page_group(&page.id).is_none())
                    .map(|page| page.id.clone())
                    .filter(matches)
                    .collect();
                let groups: Vec<(String, Vec<String>)> = ed
                    .page_groups
                    .clone()
                    .into_iter()
                    .map(|(name, ids)| {
                        let members = ed.pages.iter().filter(|p| ids.contains(&p.id)).map(|p| p.id.clone()).filter(matches).collect();
                        (name, members)
                    })
                    .filter(|(_, ids): &(String, Vec<String>)| q.is_empty() || !ids.is_empty())
                    .collect();
                // Sized from the rows, not last frame's content, so the popover never starts
                // shorter than its list.
                let rows = ungrouped.len() + groups.iter().map(|(_, ids)| ids.len()).sum::<usize>();
                let height = (rows as f32 * (ROW_H + 1.0) + groups.len() as f32 * GROUP_HEADER_H)
                    .clamp(ROW_H, POPOVER_ROWS * (ROW_H + 1.0));
                egui::ScrollArea::vertical()
                    .id_salt("pages")
                    .max_height(height)
                    .min_scrolled_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        page_rows(ui, ed, st, &ungrouped);
                        for (name, ids) in groups {
                            group_header(ui, ed, st, &name, ids.len());
                            page_rows(ui, ed, st, &ids);
                        }
                        if ungrouped.is_empty() && ed.page_groups.is_empty() || (!q.is_empty() && ed.pages.iter().all(|p| !p.name.to_lowercase().contains(&q))) {
                            ui.label(RichText::new("No pages match").weak());
                        }
                    });
                if ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && let Some(i) = ed.pages.iter().position(|p| !q.is_empty() && p.name.to_lowercase().contains(&q))
                {
                    ed.switch_page(i);
                    st.pages.collapsed = true;
                }
            });
        });
    let busy = st.pages.renaming.is_some()
        || st.pages.drag.is_some()
        || st.pages.group_dialog.is_some()
        || st.pages.icon_picker.is_some()
        || egui::Popup::is_any_open(ctx)
        || ctx.memory(|m| m.top_modal_layer().is_some());
    let outside = ctx.input(|i| {
        i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !sel.contains(p))
    });
    if !busy && (outside || ctx.input(|i| i.key_pressed(egui::Key::Escape))) {
        st.pages.collapsed = true;
    }
}

/// A group's name over its pages, with its menu (rename, remove).
fn group_header(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, name: &str, count: usize) {
    ui.add_space(6.0);
    let response = ui
        .horizontal(|ui| {
            ui.add_space(6.0);
            let r = ui.add(
                egui::Label::new(RichText::new(name).font(theme::medium(theme::SECONDARY)).color(theme::pal(ui).secondary))
                    .sense(egui::Sense::click()),
            );
            ui.label(RichText::new(count.to_string()).weak().size(theme::SECONDARY));
            r
        })
        .inner;
    response.context_menu(|ui| {
        crate::theme::popup_backing(ui);
        ui.set_width(MENU_WIDTH);
        if super::icon_menu_item(ui, crate::theme::Icon::Text, "Rename group").clicked() {
            st.pages.group_dialog = Some(GroupDialog {
                project_stamp: ed.project_stamp,
                old_name: Some(name.to_string()),
                page_id: None,
                name: name.to_string(),
            });
            ui.close();
        }
        ui.separator();
        if super::icon_menu_item(ui, crate::theme::Icon::Trash, "Remove group").clicked() {
            confirm::request(st, ed, Removal::Group(name.to_string()));
            ui.close();
        }
    });
}

/// The page icon picker: search the installed icon sets, click to set; Reset goes back to the
/// default page icon.
fn icon_picker(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(mut pick) = st.pages.icon_picker.take() else {
        return;
    };
    if !ed.pages.iter().any(|p| p.id == pick.page_id) {
        return;
    }
    if pick.searched.as_deref() != Some(pick.query.as_str()) {
        pick.hits = crate::icons::search(&crate::icons::dir(), &pick.query, None, ICON_HITS);
        pick.searched = Some(pick.query.clone());
    }
    let mut done = false;
    let response = egui::Modal::new(egui::Id::new("page-icon-picker")).show(ctx, |ui| {
        theme::modal_backing(ui, ui.visuals().window_fill, theme::radius_xl());
        ui.set_width(ICON_PICKER_W);
        ui.heading("Page icon");
        let input = crate::theme::lit(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut pick.query)
                    .hint_text("Search icons (folder, users, code…)")
                    .desired_width(f32::INFINITY)
                    .margin(theme::INPUT_MARGIN),
            )
        });
        if input.gained_focus() || !ui.memory(|m| m.focused().is_some()) {
            input.request_focus();
        }
        let p = theme::pal(ui);
        if pick.hits.is_empty() {
            let note = if crate::icons::installed(&crate::icons::dir()).is_empty() {
                "No icon sets installed. Add one with: papercraft-cli icons add phosphor"
            } else {
                "No icons match"
            };
            ui.label(RichText::new(note).weak());
        }
        let current = ed.page_icons.get(&pick.page_id).cloned();
        egui::ScrollArea::vertical().max_height(6.0 * ICON_CELL).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                for hit in &pick.hits {
                    let (cell, resp) = ui.allocate_exact_size(egui::vec2(ICON_CELL, ICON_CELL), egui::Sense::click());
                    let on = current.as_deref() == Some(hit.as_str());
                    if on {
                        theme::fill_selected(ui, cell.shrink(2.0), theme::ru(8), p.field);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(cell.shrink(2.0), theme::r(8.0), p.hover);
                    }
                    paint_page_icon(ui, egui::Rect::from_center_size(cell.center(), egui::vec2(18.0, 18.0)), Some(hit), if on { theme::accent() } else { p.text });
                    if resp.on_hover_text(hit.as_str()).clicked() {
                        ed.set_page_icon(&pick.page_id, Some(hit));
                        done = true;
                    }
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if theme::small_button(ui, "Cancel").clicked() {
                done = true;
            }
            if current.is_some() && theme::small_button(ui, "Reset").clicked() {
                ed.set_page_icon(&pick.page_id, None);
                done = true;
            }
        });
    });
    if !done && !response.should_close() {
        st.pages.icon_picker = Some(pick);
    }
}

fn page_rows(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[String]) {
    for id in ids {
        if let Some(i) = ed.pages.iter().position(|page| page.id == *id) {
            ui.push_id(id, |ui| page_row(ui, ed, st, i));
        }
    }
}

fn group_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(mut draft) = st.pages.group_dialog.take() else {
        return;
    };
    if ed.project_stamp != draft.project_stamp {
        return;
    }
    if draft
        .page_id
        .as_ref()
        .is_some_and(|id| !ed.pages.iter().any(|p| p.id == *id))
    {
        return;
    }
    let title = if draft.old_name.is_some() {
        "Rename page group"
    } else {
        "New page group"
    };
    let mut done = false;
    let response = egui::Modal::new(egui::Id::new("page-group-name")).show(ctx, |ui| {
        theme::modal_backing(ui, ui.visuals().window_fill, theme::radius_xl());
        ui.set_width(MENU_WIDTH);
        ui.heading(title);
        let input = crate::theme::lit(ui, |ui| ui.add(
            egui::TextEdit::singleline(&mut draft.name)
                .hint_text("Group name")
                .margin(theme::INPUT_MARGIN),
        ));
        if input.gained_focus() || !ui.memory(|m| m.focused().is_some()) {
            input.request_focus();
        }
        let name = draft.name.trim();
        let valid = !name.is_empty()
            && (draft.old_name.as_deref() == Some(name) || !ed.page_groups.contains_key(name));
        if !name.is_empty() && !valid {
            ui.label("A group with this name already exists.");
        }
        ui.horizontal(|ui| {
            if theme::small_button(ui, "Cancel").clicked() {
                done = true;
            }
            let submit = ui.add_enabled(valid, egui::Button::new("Save")).clicked()
                || (valid && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if submit {
                match &draft.old_name {
                    Some(old) => {
                        ed.rename_page_group(old, name);
                    }
                    None => {
                        ed.create_page_group(name);
                    }
                }
                if let Some(id) = &draft.page_id {
                    ed.assign_page_group(id, Some(name));
                }
                done = true;
            }
        });
    });
    if !done && !response.should_close() {
        st.pages.group_dialog = Some(draft);
    }
}

fn page_row(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, i: usize) {
    let p = theme::pal(ui);
    let active = i == ed.active_page;
    let (row, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_H),
        egui::Sense::click_and_drag(),
    );
    if active {
        theme::fill_selected(ui, row, crate::theme::ru(10), p.field);
    } else if resp.hovered() {
        ui.painter().rect_filled(row, crate::theme::r(10.0), p.hover);
    }
    // Drop indicator while dragging.
    if let Some((from, to)) = &st.pages.drag
        && from != to
        && *to == ed.pages[i].id
    {
        let y = row.top();
        ui.painter()
            .hline(row.x_range(), y, egui::Stroke::new(2.0, theme::accent()));
    }
    paint_page_icon(
        ui,
        egui::Rect::from_center_size(
            egui::pos2(row.left() + 14.0, row.center().y),
            egui::vec2(13.0, 13.0),
        ),
        ed.page_icons.get(&ed.pages[i].id).map(String::as_str),
        if active { theme::accent() } else { p.secondary },
    );
    let name_rect = egui::Rect::from_min_max(
        egui::pos2(row.left() + 28.0, row.top()),
        egui::pos2(row.right() - 4.0, row.bottom()),
    );
    let renaming = st.pages.renaming.as_ref().is_some_and(|(r, _)| *r == i);
    if renaming {
        let (_, name) = st.pages.renaming.as_mut().unwrap();
        let r = theme::rename_field(
            ui,
            name_rect.shrink2(egui::vec2(0.0, 2.0)),
            name,
            ("page", i),
        );
        if r.lost_focus() {
            let (idx, name) = st.pages.renaming.take().unwrap();
            if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ed.rename_page(idx, &name);
            }
        }
    } else {
        let font = if active {
            theme::medium(theme::BODY)
        } else {
            egui::FontId::proportional(theme::BODY)
        };
        let g = ui
            .painter()
            .layout(
                ed.pages[i].name.clone(),
                font,
                // Dimmed while the page is still being read in the background.
                if ed.pages[i].pending { p.secondary } else { p.text },
                f32::INFINITY,
            );
        ui.painter()
            .with_clip_rect(name_rect.intersect(ui.clip_rect()))
            .galley(
                egui::pos2(name_rect.left(), row.center().y - g.size().y / 2.0),
                g,
                Color32::PLACEHOLDER,
            );
    }
    if resp.double_clicked() {
        st.pages.renaming = Some((i, ed.pages[i].name.clone()));
    } else if resp.clicked() {
        ed.switch_page(i);
        st.pages.collapsed = true;
    }
    let page_id = ed.pages[i].id.clone();
    if resp.drag_started() {
        st.pages.drag = Some((page_id.clone(), page_id.clone()));
    }
    if let Some(pos) = ui.input(|inp| inp.pointer.interact_pos())
        && row.contains(pos)
        && let Some((_, to)) = &mut st.pages.drag
    {
        *to = page_id.clone();
    }
    resp.context_menu(|ui| {
        crate::theme::popup_backing(ui);
        ui.set_width(MENU_WIDTH);
        if super::icon_menu_item(ui, crate::theme::Icon::Text, "Rename").clicked() {
            st.pages.renaming = Some((i, ed.pages[i].name.clone()));
            ui.close();
        }
        if super::icon_menu_item(ui, crate::theme::Icon::Page, "Change icon\u{2026}").clicked() {
            st.pages.icon_picker = Some(IconPicker { page_id: page_id.clone(), query: String::new(), searched: None, hits: Vec::new() });
            ui.close();
        }
        if super::icon_menu_item(ui, crate::theme::Icon::Group, "Duplicate").clicked() {
            st.pages.duplicate = Some(page_id.clone());
            ui.close();
        }
        // Destinations inline (no submenu): every group but the page's own, then a new group.
        ui.separator();
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("Move to group").size(theme::SECONDARY).color(theme::pal(ui).secondary));
        });
        let current = ed.page_group(&page_id).map(str::to_owned);
        if current.is_some() && super::icon_menu_item(ui, crate::theme::Icon::Page, "Ungrouped").clicked() {
            ed.assign_page_group(&page_id, None);
            ui.close();
        }
        let groups: Vec<String> = ed.page_groups.keys().filter(|g| current.as_ref() != Some(*g)).cloned().collect();
        for group in groups {
            if super::icon_menu_item(ui, crate::theme::Icon::Group, &group).clicked() {
                ed.assign_page_group(&page_id, Some(&group));
                ui.close();
            }
        }
        if super::icon_menu_item(ui, crate::theme::Icon::Plus, "New group\u{2026}").clicked() {
            st.pages.group_dialog = Some(GroupDialog {
                project_stamp: ed.project_stamp,
                old_name: None,
                page_id: Some(page_id.clone()),
                name: String::new(),
            });
            ui.close();
        }
        ui.separator();
        // The last page can't go: shown dimmed and inert.
        let can_delete = ed.pages.len() > 1;
        let delete = ui
            .scope(|ui| {
                if !can_delete {
                    ui.disable();
                    ui.multiply_opacity(DISABLED_OPACITY);
                }
                super::icon_menu_item(ui, crate::theme::Icon::Trash, "Delete")
            })
            .inner;
        if delete.clicked() {
            confirm::request(
                st,
                ed,
                Removal::Page {
                    id: page_id.clone(),
                    name: ed.pages[i].name.clone(),
                },
            );
            ui.close();
        }
    });
}
