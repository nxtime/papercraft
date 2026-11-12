//! Settings on a phone: a full-screen sheet instead of the desktop modal. The page list becomes
//! a dropdown under the header, so the one column keeps the whole width.

use super::{Page, chat_page, page_body, page_text, pages};
use crate::editor::Editor;
use crate::theme;
use crate::ui::{UiState, kit};
use egui::{Align, Layout, RichText};

/// Side padding of the sheet's content.
const MARGIN: i8 = 16;
/// Room under the last row so a bottom control clears the gesture area.
const BOTTOM_GAP: f32 = 24.0;

pub(super) fn show(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, t: f32) {
    let screen = ctx.content_rect();
    let p = theme::pal_from_ctx(ctx);
    let live = st.settings.open;
    let offered = pages(st.home.is_some());
    // A page this build does not offer (a saved Updates page, say) falls back to the first.
    if let Some((first, ..)) = offered.first()
        && !offered.iter().any(|(page, ..)| *page == st.settings.page)
    {
        st.settings.page = *first;
    }
    egui::Area::new(egui::Id::new("settings-sheet"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.multiply_opacity(t);
            if !live {
                ui.disable();
            }
            ui.set_clip_rect(screen);
            ui.set_min_size(screen.size());
            ui.set_max_size(screen.size());
            // Takes the taps that would fall through to the canvas.
            ui.interact(screen, egui::Id::new("settings-sheet-backing"), egui::Sense::click_and_drag());
            ui.painter().rect_filled(screen, 0.0, p.sidebar);
            egui::Frame::new().inner_margin(egui::Margin::symmetric(MARGIN, 12)).show(ui, |ui| {
                ui.set_width(screen.width() - 2.0 * MARGIN as f32);
                header(ui, st);
                ui.add_space(12.0);
                let options: Vec<(Page, &str)> = offered.iter().map(|(page, _, label)| (*page, *label)).collect();
                let width = ui.available_width();
                if let Some(page) = kit::select(ui, st.settings.page, &options, width) {
                    st.settings.page = page;
                }
                ui.add_space(8.0);
                let sub = page_text(st.settings.page).1;
                ui.add(egui::Label::new(RichText::new(sub).color(p.secondary)).wrap());
                ui.add_space(12.0);
                let page = st.settings.page;
                egui::ScrollArea::vertical()
                    .id_salt(("settings-page", page as u8))
                    .auto_shrink([false, false])
                    // A floating bar would lie over the text; a finger scrolls without one.
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        if page == Page::Chat {
                            chat_page(ui, ctx, &mut st.settings);
                        } else {
                            page_body(ui, ctx, ed, st);
                        }
                        ui.add_space(BOTTOM_GAP);
                    });
            });
        });
    if live && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.settings.open = false;
    }
}

/// The title and its close button on one row.
fn header(ui: &mut egui::Ui, st: &mut UiState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Settings").font(theme::semibold(20.0)));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if kit::close_button(ui).clicked() {
                st.settings.open = false;
            }
        });
    });
}
