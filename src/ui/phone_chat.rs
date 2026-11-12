//! The Chat tab on a phone: a full page between the bars, with a small live thumbnail of the
//! canvas in its corner. Tapping the thumbnail goes to the canvas.

use super::phone::Tab;
use super::{UiState, theme};
use crate::editor::Editor;
use egui::{Rect, Sense, pos2, vec2};

/// Height of the strip that holds the thumbnail above the chat.
const STRIP_H: f32 = 72.0;
const THUMB_SIZE: egui::Vec2 = vec2(96.0, 60.0);
const EDGE: f32 = 8.0;
/// Rasterising the page is not free; an agent editing fast should not redo it every frame.
const REFRESH_SECS: f64 = 0.5;

pub(super) fn show(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: Rect) {
    let p = theme::pal_from_ctx(ctx);
    refresh_minimap(ctx, ed, st);
    st.phone.rects.push(("chat-page", canvas));
    egui::Area::new("phone-chat".into())
        .order(egui::Order::Middle)
        .fixed_pos(canvas.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_clip_rect(canvas);
            ui.painter().rect_filled(canvas, 0.0, p.sidebar);
            // Takes taps that would fall through to the canvas below.
            ui.interact(canvas, egui::Id::new("phone-chat-backing"), Sense::click_and_drag());
            let thumb = Rect::from_min_size(pos2(canvas.right() - EDGE - THUMB_SIZE.x, canvas.top() + EDGE), THUMB_SIZE);
            minimap(ui, st, thumb);
            let body = Rect::from_min_max(pos2(canvas.left(), canvas.top() + STRIP_H), canvas.max);
            let mut col = ui.new_child(egui::UiBuilder::new().max_rect(body));
            col.set_clip_rect(body.intersect(canvas));
            let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(10, 8));
            super::chat::panel(&mut col, ed, st, frame, false, p.sidebar);
        });
}

fn minimap(ui: &mut egui::Ui, st: &mut UiState, rect: Rect) {
    let p = theme::pal(ui);
    theme::fill_control(ui, rect, theme::radius_md(), p.field);
    if let Some((_, tex)) = &st.phone.minimap {
        egui::Image::new(tex).fit_to_exact_size(rect.size()).paint_at(ui, rect.shrink(2.0));
    }
    let r = ui.interact(rect, egui::Id::new("phone-minimap"), Sense::click()).on_hover_text("Show the canvas");
    if r.clicked() {
        st.phone.tab = Tab::Canvas;
    }
}

/// Redraws the thumbnail when the document changed, at most twice a second.
fn refresh_minimap(ctx: &egui::Context, ed: &Editor, st: &mut UiState) {
    let stamp = ed.doc_stamp;
    let now = ctx.input(|i| i.time);
    let stale = st.phone.minimap.as_ref().is_none_or(|(s, _)| *s != stamp);
    if !stale {
        return;
    }
    if now - st.phone.minimap_at < REFRESH_SECS && st.phone.minimap.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(REFRESH_SECS));
        return;
    }
    st.phone.minimap_at = now;
    let Some(img) = super::library::preview_image(&ed.doc) else { return };
    match &mut st.phone.minimap {
        Some((s, tex)) => {
            tex.set(img, egui::TextureOptions::LINEAR);
            *s = stamp;
        }
        None => st.phone.minimap = Some((stamp, ctx.load_texture("phone-minimap", img, egui::TextureOptions::LINEAR))),
    }
}
