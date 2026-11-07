//! The breakpoint preview window: the selected screen at 375 / 768 / 1280 or at the width of one
//! of its `Name @variant` frames, drawn from a copy of the document.

use super::{UiState, kit};
use crate::editor::{Editor, breakpoints};
use crate::theme::{self, Surface as _};
use egui::{Align2, RichText, Vec2};

const WINDOW_W: f32 = 420.0;
const MARGIN: f32 = 20.0;
/// Room taken by everything but the picture: margins, header and the row of widths.
const CHROME_H: f32 = 190.0;
const MIN_PICTURE_H: f32 = 160.0;

pub fn show(ctx: &egui::Context, ed: &mut Editor, _st: &mut UiState, canvas: egui::Rect) {
    let Some(p) = &ed.breakpoint else { return };
    let choices = breakpoints::choices(&ed.doc, p.frame);
    let name = ed.doc.get(p.frame).map(|n| n.name.clone()).unwrap_or_default();
    let width = p.width;
    let tex = p.image.as_ref().map(|img| texture(ctx, p.serial, img));
    let inner_w = WINDOW_W.min(canvas.width() - 40.0) - 2.0 * MARGIN;
    let picture_h = (canvas.height() - CHROME_H).max(MIN_PICTURE_H);
    let (mut picked, mut close) = (None, false);
    egui::Area::new("breakpoint-preview".into())
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(Align2::RIGHT_TOP)
        .fixed_pos(canvas.right_top() + egui::vec2(-20.0, 20.0))
        .show(ctx, |ui| {
            let pal = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(pal.sidebar)
                .corner_radius(theme::r(20.0))
                .inner_margin(MARGIN)
                .stroke(egui::Stroke::NONE)
                .show_surface(ui, |ui| {
                    ui.set_width(inner_w);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Breakpoint preview").font(theme::semibold(16.0)));
                            ui.add(
                                egui::Label::new(RichText::new(format!("{name} at {width}px")).color(pal.secondary))
                                    .truncate(),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            close = kit::close_button(ui).on_hover_text("Close (Esc)").clicked();
                        });
                    });
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        for (label, w) in &choices {
                            if kit::chip(ui, label, *w == width).clicked() {
                                picked = Some(*w);
                            }
                        }
                    });
                    ui.add_space(8.0);
                    picture(ui, tex.as_ref(), inner_w, picture_h);
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("A copy of the screen: nothing in your design changes.")
                            .size(theme::SECONDARY)
                            .color(pal.secondary),
                    );
                });
        });
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ed.breakpoint = None;
    } else if let (Some(w), Some(p)) = (picked, &mut ed.breakpoint) {
        p.width = w;
        ed.requests.push(crate::editor::AppRequest::PreviewBreakpoint);
    }
}

/// The picture scaled down to fit the window, scrolling when it is taller than the room left.
fn picture(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, max_w: f32, max_h: f32) {
    let Some(tex) = tex else {
        ui.label("Rendering…");
        return;
    };
    let size = tex.size_vec2();
    let scale = (max_w / size.x).min(1.0);
    egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
        ui.add(egui::Image::new(tex).fit_to_exact_size(Vec2::new(size.x * scale, size.y * scale)));
    });
}

/// The texture of render `serial`, rebuilt only when a new render arrives.
fn texture(ctx: &egui::Context, serial: u64, img: &image::RgbaImage) -> egui::TextureHandle {
    let id = egui::Id::new("breakpoint-texture");
    if let Some((s, t)) = ctx.data(|d| d.get_temp::<(u64, egui::TextureHandle)>(id))
        && s == serial
    {
        return t;
    }
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    let t = ctx.load_texture("breakpoint-preview", ci, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, (serial, t.clone())));
    t
}
