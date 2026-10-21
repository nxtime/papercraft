//! Settings dialog: appearance, editor, updates and a way to the keyboard shortcuts sheet.

use super::{UiState, kit, motion};
use crate::theme::{self, Icon};
use egui::{RichText, Ui};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    #[default]
    Appearance,
    Canvas,
    Updates,
}

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    page: Page,
}

pub fn show(ctx: &egui::Context, st: &mut UiState) {
    let t = motion::progress(
        ctx,
        "settings-open",
        st.settings.open,
        motion::FAST,
        motion::ease_out_cubic,
    );
    if !st.settings.open && t <= 0.0 {
        return;
    }
    let live = st.settings.open;
    let p = theme::pal_from_ctx(ctx);
    let frame = egui::Frame::new().fill(p.sidebar).corner_radius(24.0);
    let modal = egui::Modal::new(egui::Id::new("settings-modal"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.multiply_opacity(t);
            if !live {
                ui.disable();
            }
            ui.set_width(780.0f32.min(ctx.content_rect().width() - 48.0));
            ui.set_height(520.0f32.min(ctx.content_rect().height() - 96.0));
            ui.horizontal_top(|ui| {
                egui::Frame::new()
                    .fill(p.backdrop)
                    .corner_radius(24.0)
                    .inner_margin(egui::Margin::symmetric(16, 24))
                    .show(ui, |ui| {
                        ui.set_min_width(180.0);
                        ui.set_max_width(180.0);
                        ui.set_min_height(ui.available_height());
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Settings").font(theme::semibold(20.0)));
                            ui.add_space(12.0);
                            for (page, icon, label) in [
                                (Page::Appearance, Icon::Sketch, "Appearance"),
                                (Page::Canvas, Icon::Grid, "Canvas"),
                                (Page::Updates, Icon::Restore, "Updates"),
                            ] {
                                if kit::nav_item(ui, icon, label, st.settings.page == page) {
                                    st.settings.page = page;
                                }
                            }
                        });
                    });
                ui.vertical(|ui| {
                    ui.add_space(24.0);
                    ui.set_min_width(ui.available_width() - 24.0);
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width() - 8.0);
                            content(ui, ctx, st);
                        });
                    });
                });
            });
        });
    if live && (modal.should_close() || ctx.input(|i| i.key_pressed(egui::Key::Escape))) {
        st.settings.open = false;
    }
}

fn content(ui: &mut Ui, ctx: &egui::Context, st: &mut UiState) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        let (title, sub) = match st.settings.page {
            Page::Appearance => ("Appearance", "Theme and motion."),
            Page::Canvas => ("Canvas", "How the canvas looks."),
            Page::Updates => ("Updates", "Keep PaperCraft current."),
        };
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(title).font(theme::semibold(20.0)));
            ui.label(RichText::new(sub).color(p.secondary));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if kit::close_button(ui).clicked() {
                st.settings.open = false;
            }
        });
    });
    ui.add_space(16.0);
    match st.settings.page {
        Page::Appearance => {
            kit::section_header(ui, "Theme", None);
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 8.0);
                for c in theme::Choice::ALL {
                    if kit::chip(ui, c.label(), theme::choice() == c).clicked() {
                        theme::apply_choice(ctx, c);
                        c.save();
                    }
                }
            });
            ui.add_space(12.0);
            let mut animations = motion::enabled(ctx);
            kit::setting_row(
                ui,
                "Animations",
                "Play motion on hover, open and close.",
                |ui| {
                    if kit::toggle(ui, &mut animations).changed() {
                        motion::set_enabled(ctx, animations);
                    }
                },
            );
        }
        Page::Canvas => {
            let mut dots = !st.show_grid_off;
            kit::setting_row(ui, "Dot grid", "Show the dotted canvas background.", |ui| {
                if kit::toggle(ui, &mut dots).changed() {
                    st.show_grid_off = !dots;
                }
            });
            ui.add_space(8.0);
            kit::setting_row(
                ui,
                "Keyboard shortcuts",
                "See every shortcut in one sheet.",
                |ui| {
                    if kit::button(ui, kit::ButtonKind::Secondary, None, "Open").clicked() {
                        st.settings.open = false;
                        st.show_help = true;
                    }
                },
            );
        }
        Page::Updates => {
            kit::setting_row(
                ui,
                "Installed version",
                &format!("PaperCraft {}", env!("CARGO_PKG_VERSION")),
                |ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    if kit::button(ui, kit::ButtonKind::Primary, None, "Check for updates")
                        .clicked()
                    {
                        st.settings.open = false;
                        st.updater.open = true;
                    }
                    #[cfg(target_arch = "wasm32")]
                    ui.label("Reload the page to get the latest version.");
                },
            );
        }
    }
}
