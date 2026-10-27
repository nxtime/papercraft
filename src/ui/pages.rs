//! The pages list at the top of the left sidebar (as in Figma): switch, add, rename,
//! duplicate, delete and drag to reorder.

use super::confirm::{self, Removal};
use super::{UiState, section};
use crate::editor::Editor;
use crate::theme::{self, Icon};
use egui::{Color32, RichText, Ui};
use std::collections::HashSet;

pub struct PagesUi {
    pub collapsed: bool,
    pub renaming: Option<(usize, String)>,
    drag: Option<(String, String)>,
    collapsed_groups: HashSet<String>,
    group_dialog: Option<GroupDialog>,
    duplicate: Option<String>,
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
            collapsed_groups: HashSet::new(),
            group_dialog: None,
            duplicate: None,
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
    theme::paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            egui::pos2(sel.left() + 18.0, sel.center().y),
            egui::vec2(16.0, 16.0),
        ),
        Icon::Page,
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
    }
    if st.pages.collapsed {
        return;
    }
    let groups = ed.page_groups.clone();
    let rows = ed.pages.len() + groups.len();
    let max_h = (rows as f32 * (ROW_H + 1.0)).min(9.5 * (ROW_H + 1.0));
    egui::ScrollArea::vertical()
        .id_salt("pages")
        .max_height(max_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let ungrouped: Vec<String> = ed
                .pages
                .iter()
                .filter(|page| ed.page_group(&page.id).is_none())
                .map(|page| page.id.clone())
                .collect();
            page_rows(ui, ed, st, &ungrouped);
            for (name, ids) in groups {
                let expanded = !st.pages.collapsed_groups.contains(&name);
                let response = ui
                    .horizontal(|ui| {
                        if theme::icon_button(
                            ui,
                            if expanded {
                                Icon::ChevronDown
                            } else {
                                Icon::ChevronRight
                            },
                            16.0,
                            false,
                        )
                        .clicked()
                        {
                            if expanded {
                                st.pages.collapsed_groups.insert(name.clone());
                            } else {
                                st.pages.collapsed_groups.remove(&name);
                            }
                        }
                        let response = ui.add(
                            egui::Label::new(RichText::new(&name).font(theme::medium(theme::BODY)))
                                .sense(egui::Sense::click()),
                        );
                        ui.label(
                            RichText::new(ids.len().to_string())
                                .weak()
                                .size(theme::SECONDARY),
                        );
                        response
                    })
                    .inner;
                response.context_menu(|ui| {
                    crate::theme::popup_backing(ui);
                    ui.set_width(MENU_WIDTH);
                    if super::icon_menu_item(ui, crate::theme::Icon::Text, "Rename group").clicked()
                    {
                        st.pages.group_dialog = Some(GroupDialog {
                            project_stamp: ed.project_stamp,
                            old_name: Some(name.clone()),
                            page_id: None,
                            name: name.clone(),
                        });
                        ui.close();
                    }
                    ui.separator();
                    if super::icon_menu_item(ui, crate::theme::Icon::Trash, "Remove group")
                        .clicked()
                    {
                        confirm::request(st, ed, Removal::Group(name.clone()));
                        ui.close();
                    }
                });
                if expanded {
                    let members: Vec<String> = ed
                        .pages
                        .iter()
                        .filter(|page| ids.contains(&page.id))
                        .map(|page| page.id.clone())
                        .collect();
                    ui.indent(("page-group", &name), |ui| page_rows(ui, ed, st, &members));
                }
            }
        });
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
                        st.pages.collapsed_groups.remove(old);
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
    theme::paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            egui::pos2(row.left() + 14.0, row.center().y),
            egui::vec2(13.0, 13.0),
        ),
        Icon::Page,
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
        if super::icon_menu_item(ui, crate::theme::Icon::Group, "Duplicate").clicked() {
            st.pages.duplicate = Some(page_id.clone());
            ui.close();
        }
        ui.menu_button("Move to group", |ui| {
            crate::theme::popup_backing(ui);
            ui.set_width(MENU_WIDTH);
            if ui
                .selectable_label(ed.page_group(&page_id).is_none(), "Ungrouped")
                .clicked()
            {
                ed.assign_page_group(&page_id, None);
                ui.close();
            }
            let groups: Vec<String> = ed.page_groups.keys().cloned().collect();
            for group in groups {
                if ui
                    .selectable_label(ed.page_group(&page_id) == Some(group.as_str()), &group)
                    .clicked()
                {
                    ed.assign_page_group(&page_id, Some(&group));
                    ui.close();
                }
            }
            ui.separator();
            if super::icon_menu_item(ui, crate::theme::Icon::Plus, "New group…").clicked() {
                st.pages.group_dialog = Some(GroupDialog {
                    project_stamp: ed.project_stamp,
                    old_name: None,
                    page_id: Some(page_id.clone()),
                    name: String::new(),
                });
                ui.close();
            }
        });
        ui.separator();
        if ui
            .add_enabled(ed.pages.len() > 1, egui::Button::new("Delete"))
            .clicked()
        {
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
