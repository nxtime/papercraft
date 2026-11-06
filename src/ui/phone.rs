//! The phone layout, for windows narrower than [`MAX_W`] logical px (the browser build on a
//! phone): the canvas between a slim top bar and a bottom tab bar, and the side panels as a
//! bottom sheet over the canvas. The sheet reuses the desktop panels as they are.

use super::{UiState, kit, theme};
use crate::editor::{Editor, Tool, shortcuts::Action};
use crate::model::NodeId;
use crate::theme::Icon;
use egui::{Align, Align2, FontId, Layout, Rect, Sense, Ui, pos2, vec2};

/// Windows narrower than this (logical px) get the phone layout.
pub const MAX_W: f32 = 600.0;
const TAB_BAR_H: f32 = 52.0;
const SHEET_CORNER: u8 = 14;
const GRIP_W: f32 = 40.0;
const GRIP_H: f32 = 4.0;
const MARGIN: f32 = 12.0;
/// Under this share of the half height, letting go of the grip closes the sheet.
const CLOSE_BELOW: f32 = 0.6;
const HALF_SHARE: f32 = 0.5;

pub fn is_phone(width: f32) -> bool {
    width < MAX_W
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Canvas,
    Layers,
    Properties,
    Chat,
    Pages,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Canvas, Tab::Layers, Tab::Properties, Tab::Chat, Tab::Pages];

    fn label(self) -> &'static str {
        match self {
            Tab::Canvas => "Canvas",
            Tab::Layers => "Layers",
            Tab::Properties => "Properties",
            Tab::Chat => "Chat",
            Tab::Pages => "Pages",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Tab::Canvas => Icon::Frame,
            Tab::Layers => Icon::Group,
            Tab::Properties => Icon::Corners,
            Tab::Chat => Icon::Note,
            Tab::Pages => Icon::Page,
        }
    }
}

/// The two heights the sheet rests at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Detent {
    #[default]
    Half,
    Full,
}

#[derive(Default)]
pub struct PhoneState {
    pub tab: Tab,
    pub detent: Detent,
    /// How far the grip has been dragged from the detent's height (px, up is positive).
    drag: f32,
    /// One finger on empty canvas draws a marquee instead of panning.
    pub select_mode: bool,
    /// Safe-area insets (top, right, bottom, left) and the soft keyboard's height, in logical
    /// px. The browser shell fills them in.
    pub safe: [f32; 4],
    pub keyboard: f32,
    /// A long-press on a layer: where to show its menu, which layer, and the document stamp it
    /// was opened at (any edit closes it).
    pub menu: Option<(egui::Pos2, NodeId, u64)>,
    /// Rects of the chrome drawn last frame, by name, for tests.
    pub rects: Vec<(&'static str, Rect)>,
}

/// Height of the sheet at rest, with `avail` px between the top bar and the sheet's bottom edge.
pub fn sheet_height(detent: Detent, avail: f32) -> f32 {
    match detent {
        Detent::Half => avail * HALF_SHARE,
        Detent::Full => avail,
    }
}

/// Where a sheet let go at `height` settles: a detent, or `None` to close it.
pub fn settle(height: f32, avail: f32) -> Option<Detent> {
    let half = sheet_height(Detent::Half, avail);
    if height < half * CLOSE_BELOW {
        None
    } else if height > (half + avail) / 2.0 {
        Some(Detent::Full)
    } else {
        Some(Detent::Half)
    }
}

pub fn draw(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) -> Rect {
    let p = theme::pal(ui);
    let screen = ui.max_rect();
    let [safe_t, safe_r, safe_b, safe_l] = st.phone.safe;
    st.phone.rects.clear();

    let top = egui::Panel::top("phone-top")
        .exact_size(kit::TOUCH_H + safe_t)
        .frame(egui::Frame::NONE.fill(p.window).inner_margin(egui::Margin {
            left: (safe_l + 8.0) as i8,
            right: (safe_r + 8.0) as i8,
            top: safe_t as i8,
            bottom: 0,
        }))
        .show(ui, |ui| top_bar(ui, ed, st));
    st.phone.rects.push(("top-bar", top.response.rect));

    let tabs = egui::Panel::bottom("phone-tabs")
        .exact_size(TAB_BAR_H + safe_b)
        .frame(egui::Frame::NONE.fill(p.sidebar).inner_margin(egui::Margin {
            left: safe_l as i8,
            right: safe_r as i8,
            top: 0,
            bottom: safe_b as i8,
        }))
        .show(ui, |ui| tab_bar(ui, st));
    st.phone.rects.push(("tab-bar", tabs.response.rect));

    let canvas = ui.available_rect_before_wrap();
    super::overlays(ui, ed, st, canvas, false);
    if st.phone.tab == Tab::Canvas {
        tool_strip(ui.ctx(), ed, canvas);
    } else {
        sheet(ui.ctx(), ed, st, screen, canvas);
    }
    context_menu(ui.ctx(), ed, st);
    canvas
}

/// A square 44 px button.
fn touch_button(ui: &mut Ui, icon: Icon, selected: bool, enabled: bool) -> egui::Response {
    let mut r = None;
    ui.add_enabled_ui(enabled, |ui| r = Some(theme::icon_button(ui, icon, kit::TOUCH_H, selected)));
    r.unwrap_or_else(|| ui.label(""))
}

fn history_button(ui: &mut Ui, redo: bool, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(kit::TOUCH_H, kit::TOUCH_H), Sense::click());
    let p = theme::pal(ui);
    let tint = if enabled { p.text } else { p.tertiary };
    let icon = rect.shrink(11.0);
    if redo {
        super::lottie_icon::paint_rest_mirrored(ui, "undo-2", icon, tint);
    } else {
        super::lottie_icon::paint_frame(ui, "undo-2", icon, tint, 0.0);
    }
    resp
}

fn top_bar(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    let buttons_w = 4.0 * kit::TOUCH_H + 3.0 * ui.spacing().item_spacing.x;
    ui.horizontal_centered(|ui| {
        let name_w = (ui.available_width() - buttons_w - ui.spacing().item_spacing.x).max(40.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(name_w, kit::TOUCH_H), Sense::click());
        let galley = ui.painter().layout_no_wrap(ed.page_name().to_owned(), theme::semibold(theme::BODY), p.text);
        let at = pos2(rect.left() + 4.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().with_clip_rect(rect).galley(at, galley, p.text);
        if resp.clicked() {
            st.phone.tab = Tab::Pages;
        }
        if touch_button(ui, Icon::Pointer, st.phone.select_mode, true).on_hover_text("Select").clicked() {
            st.phone.select_mode = !st.phone.select_mode;
        }
        if history_button(ui, false, ed.history.can_undo()).clicked() {
            ed.run(Action::Undo);
        }
        if history_button(ui, true, ed.history.redo_len() > 0).clicked() {
            ed.run(Action::Redo);
        }
        if touch_button(ui, Icon::Play, false, true).on_hover_text("Present").clicked() {
            super::present::start_present(ed, st);
        }
    });
}

fn tab_bar(ui: &mut Ui, st: &mut UiState) {
    let p = theme::pal(ui);
    let cell_w = ui.available_width() / Tab::ALL.len() as f32;
    let h = ui.available_height().max(kit::TOUCH_H);
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    ui.horizontal(|ui| {
        for tab in Tab::ALL {
            let (rect, resp) = ui.allocate_exact_size(vec2(cell_w, h), Sense::click());
            let on = st.phone.tab == tab;
            let ink = if on { theme::accent() } else { p.secondary };
            let icon = Rect::from_center_size(pos2(rect.center().x, rect.top() + 14.0 + 10.0), vec2(20.0, 20.0));
            theme::paint_icon(ui.painter(), icon, tab.icon(), ink);
            ui.painter().text(
                pos2(rect.center().x, rect.bottom() - 10.0),
                Align2::CENTER_CENTER,
                tab.label(),
                FontId::proportional(theme::SECONDARY),
                ink,
            );
            if resp.clicked() {
                // The open tab closes its sheet.
                st.phone.tab = if on { Tab::Canvas } else { tab };
                st.phone.drag = 0.0;
            }
        }
    });
}

/// The tools in a row that scrolls sideways: the desktop palette is wider than a phone.
fn tool_strip(ctx: &egui::Context, ed: &mut Editor, canvas: Rect) {
    let width = canvas.width() - 2.0 * MARGIN;
    egui::Area::new("phone-tools".into())
        .order(egui::Order::Middle)
        .fixed_pos(pos2(canvas.left() + MARGIN, canvas.bottom() - MARGIN - kit::TOUCH_H - 8.0))
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .corner_radius(theme::radius_panel())
                .inner_margin(4.0)
                .stroke(egui::Stroke::NONE)
                .show(ui, |ui| {
                    ui.set_width(width - 8.0);
                    egui::ScrollArea::horizontal().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for t in Tool::ALL {
                                if theme::icon_button(ui, super::tool_icon(t), kit::TOUCH_H, ed.tool == t).clicked() {
                                    ed.run(Action::Tool(t));
                                }
                            }
                        });
                    });
                });
        });
}

fn sheet(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, screen: Rect, canvas: Rect) {
    let [_, safe_r, _, safe_l] = st.phone.safe;
    // With the soft keyboard up the sheet rests on it instead of the tab bar.
    let bottom = if st.phone.keyboard > 0.0 { screen.bottom() - st.phone.keyboard } else { canvas.bottom() };
    let avail = (bottom - canvas.top()).max(3.0 * kit::TOUCH_H);
    let height = (sheet_height(st.phone.detent, avail) + st.phone.drag).clamp(kit::TOUCH_H, avail);
    let rect = Rect::from_min_max(pos2(screen.left(), bottom - height), pos2(screen.right(), bottom));
    st.phone.rects.push(("sheet", rect));
    let p = theme::pal_from_ctx(ctx);

    egui::Area::new("phone-sheet".into())
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_clip_rect(rect);
            let (full, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
            let round = egui::CornerRadius { nw: SHEET_CORNER, ne: SHEET_CORNER, sw: 0, se: 0 };
            ui.painter().rect_filled(full, round, p.sidebar);
            ui.painter().hline(full.x_range(), full.top(), egui::Stroke::new(1.0, p.separator));

            let grip_area = Rect::from_min_size(full.min, vec2(full.width(), kit::TOUCH_H));
            let grip = ui.interact(grip_area, ui.id().with("grip"), Sense::drag());
            let pill = Rect::from_center_size(grip_area.center(), vec2(GRIP_W, GRIP_H));
            ui.painter().rect_filled(pill, GRIP_H / 2.0, p.tertiary);
            if grip.dragged() {
                st.phone.drag -= grip.drag_delta().y;
            }
            if grip.drag_stopped() {
                match settle(height, avail) {
                    Some(d) => st.phone.detent = d,
                    None => st.phone.tab = Tab::Canvas,
                }
                st.phone.drag = 0.0;
            }

            let content = Rect::from_min_max(
                pos2(full.left() + MARGIN + safe_l, grip_area.bottom()),
                pos2(full.right() - MARGIN - safe_r, full.bottom()),
            );
            if content.height() < 1.0 {
                return;
            }
            if st.phone.tab == Tab::Chat {
                chat_tab(ui.ctx(), ed, st, content);
                return;
            }
            let mut body = ui.new_child(egui::UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
            body.set_clip_rect(content.intersect(rect));
            theme::figma_fields(&mut body);
            match st.phone.tab {
                Tab::Layers => {
                    st.show_library = false;
                    st.show_design = false;
                    st.side_extra = None;
                    super::layers_body(&mut body, ed, st);
                }
                Tab::Properties => super::inspector::panel(&mut body, ed, st),
                Tab::Pages => {
                    egui::ScrollArea::vertical().auto_shrink(false).show(&mut body, |ui| super::pages::panel(ui, ed, st));
                }
                Tab::Canvas | Tab::Chat => {}
            }
        });
}

/// The chat column is laid out at its desktop minimum width; on a narrower screen it is drawn
/// scaled down to fit rather than cut off.
fn chat_tab(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, content: Rect) {
    let column = st.chat.column_width();
    let scale = (content.width() / column).min(1.0);
    let local = vec2(column.max(content.width()), content.height() / scale);
    let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 8));
    let fill = theme::pal_from_ctx(ctx).sidebar;
    let area = egui::Area::new("phone-chat".into())
        .order(egui::Order::Middle)
        .fixed_pos(content.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_clip_rect(Rect::from_min_size(content.min, local));
            ui.set_min_size(local);
            ui.set_max_size(local);
            super::chat::panel(ui, ed, st, frame, false, fill);
        });
    let to_screen = egui::emath::TSTransform::new(content.min.to_vec2() * (1.0 - scale), scale);
    ctx.set_transform_layer(area.response.layer_id, to_screen);
}

/// The menu a long-press on a layer opens, at the finger.
fn context_menu(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some((at, id, stamp)) = st.phone.menu else { return };
    if ed.doc_stamp != stamp || ed.doc.get(id).is_none() {
        st.phone.menu = None;
        return;
    }
    let p = theme::pal_from_ctx(ctx);
    let area = egui::Area::new("phone-menu".into())
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .corner_radius(theme::radius_panel())
                .stroke(egui::Stroke::NONE)
                .show(ui, |ui| {
                    ui.set_min_width(220.0);
                    ui.spacing_mut().interact_size.y = kit::TOUCH_H;
                    super::layer_menu(ui, ed, st, id);
                });
        });
    if area.response.clicked_elsewhere() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.phone.menu = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_rests_at_half_or_full_and_closes_when_dragged_low() {
        let avail = 600.0;
        assert_eq!(sheet_height(Detent::Half, avail), 300.0);
        assert_eq!(sheet_height(Detent::Full, avail), 600.0);
        assert_eq!(settle(300.0, avail), Some(Detent::Half));
        assert_eq!(settle(420.0, avail), Some(Detent::Half));
        assert_eq!(settle(480.0, avail), Some(Detent::Full));
        assert_eq!(settle(150.0, avail), None, "dragged down past the close point");
        assert_eq!(settle(190.0, avail), Some(Detent::Half), "short of it, it springs back");
    }

    #[test]
    fn windows_under_600_are_phones() {
        assert!(is_phone(390.0) && is_phone(599.9));
        assert!(!is_phone(600.0) && !is_phone(1280.0));
    }
}
