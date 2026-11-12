//! The keyboard shortcuts sheet.

use super::{UiState, kit};
use crate::editor::shortcuts;
use crate::theme::{self, Surface as _};
use egui::{Align2, RichText, Ui};

const MAX_WIDTH: f32 = 1040.0;
const MARGIN: f32 = 24.0;
const COLUMN_GAP: f32 = 28.0;
/// Below these sheet widths a column gets too narrow for a label beside its keys.
const THREE_COLUMNS: f32 = 900.0;
const TWO_COLUMNS: f32 = 560.0;
const ROW_HEIGHT: f32 = 30.0;
const LABEL_GAP: f32 = 12.0;
/// Height of everything around the list: margins, header and footer, so it never pushes those
/// off the canvas.
const CHROME_HEIGHT: f32 = 190.0;
const MIN_LIST_HEIGHT: f32 = 200.0;

pub fn show(ctx: &egui::Context, st: &mut UiState, canvas: egui::Rect) {
    let width = MAX_WIDTH.min(canvas.width() - 40.0) - 2.0 * MARGIN;
    let list_height = (canvas.height() - CHROME_HEIGHT).max(MIN_LIST_HEIGHT);
    egui::Area::new("help".into())
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(canvas.center())
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.sidebar)
                .corner_radius(theme::r(24.0))
                .inner_margin(MARGIN)
                .stroke(egui::Stroke::NONE)
                .show_surface(ui, |ui| {
                    ui.set_width(width);
                    header(ui, st);
                    ui.add_space(12.0);
                    egui::ScrollArea::vertical()
                        .max_height(list_height)
                        // An area is only offered the room below its position, not all it can use.
                        .min_scrolled_height(list_height)
                        .show(ui, |ui| columns(ui, width));
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(footer())
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.show_help = false;
    }
}

fn header(ui: &mut Ui, st: &mut UiState) {
    let p = theme::pal(ui);
    let title = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(RichText::new("Keyboard shortcuts").font(theme::semibold(20.0)));
        ui.add(egui::Label::new(RichText::new("Press ? anytime to open this sheet.").color(p.secondary)).wrap());
    };
    let done = |ui: &mut Ui, st: &mut UiState| {
        if kit::button(ui, kit::ButtonKind::Secondary, None, "Done")
            .on_hover_text("Close (Esc)")
            .clicked()
        {
            st.show_help = false;
        }
    };
    if kit::compact(ui.ctx()) {
        // Beside the title the button would cover it: a phone sheet is narrower than both.
        ui.vertical(|ui| {
            title(ui);
            ui.add_space(8.0);
            done(ui, st);
        });
        return;
    }
    ui.horizontal(|ui| {
        ui.vertical(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| done(ui, st));
    });
}

fn columns(ui: &mut Ui, width: f32) {
    let count = if width >= THREE_COLUMNS {
        3
    } else if width >= TWO_COLUMNS {
        2
    } else {
        1
    };
    let sections = shortcuts::help_sections();
    ui.spacing_mut().item_spacing.x = COLUMN_GAP;
    ui.columns(count, |cols| {
        let mut heights = vec![0usize; count];
        for (title, rows) in &sections {
            let shortest = (0..count).min_by_key(|&i| heights[i]).unwrap_or(0);
            heights[shortest] += rows.len() + 2;
            let ui = &mut cols[shortest];
            kit::section_header(ui, title, None);
            ui.add_space(4.0);
            for (keys, label) in rows {
                row(ui, keys, label);
            }
            ui.add_space(14.0);
        }
    });
}

/// Keys on the right, and the label in what they leave, wrapping instead of running under them.
fn row(ui: &mut Ui, keys: &str, label: &str) {
    let top = ui.cursor().min;
    let width = ui.available_width();
    let keys_rect = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(
                    top,
                    egui::vec2(width, ROW_HEIGHT),
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                kit::key_caps_rtl(ui, keys);
            },
        )
        .response
        .rect;
    let room = (width - keys_rect.width() - LABEL_GAP).max(0.0);
    let ink = ui.visuals().text_color();
    let text = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(theme::BODY),
        ink,
        room,
    );
    let height = text.size().y;
    let y = top.y + ((ROW_HEIGHT - height) / 2.0).max(0.0);
    ui.painter().galley(egui::pos2(top.x, y), text, ink);
    let used = keys_rect.bottom() - top.y;
    ui.add_space((height.max(ROW_HEIGHT) - used).max(0.0));
}

fn footer() -> &'static str {
    if cfg!(target_os = "macos") {
        "Space-drag pans · wheel pans · ⌘+wheel or pinch zooms · ⌥-drag duplicates · hold ⌥ to measure"
    } else {
        "Space-drag pans · wheel pans · Ctrl+wheel or pinch zooms · Alt-drag duplicates · hold Alt to measure"
    }
}
