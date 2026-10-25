//! The rounded design language as reusable egui pieces: buttons, chips, section headers, setting
//! rows, key caps, empty states and banners. They mirror the components of the `PaperCraft` design
//! file (Button / Primary, Chip / Accent, Setting row, Key cap, Banner / Info ...).

use crate::theme::{self, Icon};
use egui::{
    Align2, Color32, CornerRadius, FontId, Rect, Response, RichText, Sense, Stroke, Ui, vec2,
};

/// Height of a regular button, matching the design's 36px controls.
pub const BUTTON_H: f32 = 34.0;
/// Radius of a pill (chips, tabs, toggles).
const PILL: u8 = 255;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Accent fill, the one main action of a surface.
    Primary,
    /// Raised field colour.
    Secondary,
    /// Destructive: error colour.
    Danger,
}

fn galley_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.fonts_mut(|f| {
        f.layout_no_wrap(text.to_owned(), font, Color32::WHITE)
            .size()
            .x
    })
}

/// A rounded button of the design system. `icon` goes before the label.
pub fn button(ui: &mut Ui, kind: ButtonKind, icon: Option<Icon>, label: &str) -> Response {
    paint_button(ui, kind, icon, label, false)
}

/// Like [`button`], stretched over the available width, label centred.
pub fn button_wide(ui: &mut Ui, kind: ButtonKind, icon: Option<Icon>, label: &str) -> Response {
    paint_button(ui, kind, icon, label, true)
}

fn paint_button(
    ui: &mut Ui,
    kind: ButtonKind,
    icon: Option<Icon>,
    label: &str,
    wide: bool,
) -> Response {
    let p = theme::pal(ui);
    let font = theme::semibold(theme::BODY);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let text_w = galley_width(ui, label, font.clone());
    let w = if wide {
        ui.available_width()
    } else {
        text_w + icon_w + 32.0
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, BUTTON_H), Sense::click());
    let enabled = ui.is_enabled();
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("btn"), &resp);
    let (fill, ink) = match kind {
        ButtonKind::Primary => (theme::accent(), ink_on(theme::accent())),
        ButtonKind::Secondary => (p.field, p.text),
        ButtonKind::Danger => (p.danger, ink_on(p.danger)),
    };
    let lift = if resp.is_pointer_button_down_on() {
        p.pressed
    } else {
        p.hover.gamma_multiply(hover)
    };
    let fill = if enabled {
        theme::blend(fill, lift)
    } else {
        theme::blend(p.field, Color32::from_white_alpha(0))
    };
    let ink = if enabled { ink } else { p.tertiary };
    ui.painter().rect_filled(rect, theme::RADIUS_LG, fill);
    let mut x = if wide {
        rect.center().x - (text_w + icon_w) / 2.0
    } else {
        rect.left() + 16.0
    };
    if let Some(i) = icon {
        let r = Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0));
        theme::paint_icon(ui.painter(), r, i, ink);
        x += 22.0;
    }
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        ink,
    );
    resp
}

/// Text colour that reads on `fill`: near-black on light fills, white on dark ones.
pub fn ink_on(fill: Color32) -> Color32 {
    let lum = (0.2126 * f32::from(fill.r())
        + 0.7152 * f32::from(fill.g())
        + 0.0722 * f32::from(fill.b()))
        / 255.0;
    if lum > 0.5 {
        Color32::from_rgb(0x18, 0x18, 0x25)
    } else {
        Color32::WHITE
    }
}

/// Text colour that reads on the accent (and error) fill.
pub fn on_accent(p: theme::Palette) -> Color32 {
    // The design's `on-accent` is the darkest surface of the flavour.
    if theme::accent().r() as u32 + theme::accent().g() as u32 + theme::accent().b() as u32 > 380 {
        p.backdrop
    } else {
        Color32::WHITE
    }
}

/// A small pill label. `active` uses the accent text, like Chip / Accent.
pub fn chip(ui: &mut Ui, label: &str, active: bool) -> Response {
    chip_ink(ui, label, active.then(theme::accent))
}

/// A chip with its own ink (for example `p.success` for Accepted); None is the idle grey.
/// Height of [`chip`], so rows that hold one can size around it.
pub const CHIP_H: f32 = 22.0;

pub fn chip_ink(ui: &mut Ui, label: &str, ink: Option<Color32>) -> Response {
    let p = theme::pal(ui);
    let font = theme::semibold(theme::SECONDARY);
    let w = galley_width(ui, label, font.clone()) + 20.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, CHIP_H), Sense::click());
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("chip"), &resp);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(PILL),
        theme::blend(p.field, p.hover.gamma_multiply(hover)),
    );
    let ink = ink.unwrap_or(p.secondary);
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, label, font, ink);
    resp
}

/// A static key cap such as `⌘`, `K`.
pub fn key_cap(ui: &mut Ui, label: &str) {
    let p = theme::pal(ui);
    let font = theme::semibold(theme::SECONDARY);
    let w = (galley_width(ui, label, font.clone()) + 14.0).max(24.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    ui.painter().rect_filled(rect, theme::RADIUS_MD, p.field);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        p.secondary,
    );
}

fn caps_of(keys: &str) -> Vec<String> {
    let split = keys
        .char_indices()
        .find(|(_, c)| !"⌘⇧⌥⌃".contains(*c))
        .map_or(keys.len(), |(i, _)| i);
    let mut caps: Vec<String> = keys[..split].chars().map(String::from).collect();
    if split < keys.len() {
        caps.push(keys[split..].to_owned());
    }
    caps
}

/// Splits a shortcut such as `⇧⌘Z` or `Space-drag` into key caps: each leading modifier symbol is
/// a cap of its own, the rest is one cap.
pub fn key_caps(ui: &mut Ui, keys: &str) {
    for cap in caps_of(keys) {
        key_cap(ui, &cap);
    }
}

/// [`key_caps`] for a right-to-left layout, so the caps read in order.
pub fn key_caps_rtl(ui: &mut Ui, keys: &str) {
    for cap in caps_of(keys).iter().rev() {
        key_cap(ui, cap);
    }
}

/// Shows a shortcut like `⌘ K` as key caps, right-aligned in the current layout.
pub fn shortcut(ui: &mut Ui, keys: &str) {
    for k in keys.split_whitespace().rev() {
        key_cap(ui, k);
    }
}

/// Section title in small uppercase, with an optional trailing icon button. Returns whether the
/// icon was clicked.
pub fn section_header(ui: &mut Ui, title: &str, action: Option<(Icon, &str)>) -> bool {
    let p = theme::pal(ui);
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.label(
            RichText::new(title.to_uppercase())
                .font(theme::semibold(theme::SECONDARY))
                .color(p.tertiary),
        );
        if let Some((icon, tip)) = action {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clicked = theme::icon_button(ui, icon, 22.0, false)
                    .on_hover_text(tip)
                    .clicked();
            });
        }
    });
    clicked
}

/// A settings row: title and description on the left, the control on the right.
pub fn setting_row<R>(
    ui: &mut Ui,
    title: &str,
    desc: &str,
    control: impl FnOnce(&mut Ui) -> R,
) -> R {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(title).font(theme::semibold(theme::BODY)));
            if !desc.is_empty() {
                ui.label(
                    RichText::new(desc)
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control)
            .inner
    })
    .inner
}

/// A rounded icon badge, as at the top of a dialog.
pub fn badge(ui: &mut Ui, icon: Icon, tint: Color32, size: f32) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(rect, theme::RADIUS_PANEL, p.field);
    theme::paint_icon(ui.painter(), rect.shrink(size * 0.28), icon, tint);
}

/// Centred empty or error state: icon badge, title, explanation, then `actions`.
pub fn empty_state(
    ui: &mut Ui,
    icon: Icon,
    tint: Color32,
    title: &str,
    body: &str,
    actions: impl FnOnce(&mut Ui),
) {
    let p = theme::pal(ui);
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        badge(ui, icon, tint, 56.0);
        ui.label(RichText::new(title).font(theme::semibold(18.0)));
        if !body.is_empty() {
            ui.label(RichText::new(body).color(p.secondary));
        }
        ui.add_space(4.0);
        ui.horizontal(actions);
    });
}

/// A full-width rounded notice with an icon and an optional action. Returns whether the action
/// was clicked.
pub fn banner(ui: &mut Ui, icon: Icon, tint: Color32, text: &str, action: Option<&str>) -> bool {
    let p = theme::pal(ui);
    let mut clicked = false;
    egui::Frame::new()
        .fill(p.field)
        .corner_radius(theme::RADIUS_PANEL)
        .inner_margin(egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                theme::paint_icon(ui.painter(), r, icon, tint);
                ui.label(RichText::new(text).font(theme::semibold(theme::BODY)));
                if let Some(a) = action {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        clicked = button(ui, ButtonKind::Primary, None, a).clicked();
                    });
                }
            });
        });
    clicked
}

/// Fills `rect` with the panel colour and rounds it: the surface of dialogs and popovers.
pub fn card_frame(ui: &Ui) -> egui::Frame {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.sidebar)
        .corner_radius(theme::RADIUS_XL + 8)
        .inner_margin(24)
        .stroke(Stroke::NONE)
}

/// The close button of a dialog: an animated cross.
pub fn close_button(ui: &mut Ui) -> Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("close"), &resp);
    ui.painter()
        .rect_filled(rect, theme::RADIUS_LG, p.hover.gamma_multiply(hover));
    crate::ui::lottie_icon::paint_hover(
        ui,
        resp.id,
        "x",
        rect.shrink(7.0),
        p.secondary,
        resp.hovered(),
    );
    resp.on_hover_text("Close")
}

/// A pill toggle switch.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let p = theme::pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(42.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = crate::ui::motion::progress(
        ui.ctx(),
        resp.id.with("toggle"),
        *on,
        crate::ui::motion::FAST,
        crate::ui::motion::ease_out_cubic,
    );
    let fill = theme::blend(p.field, theme::accent().gamma_multiply(t));
    ui.painter()
        .rect_filled(rect, CornerRadius::same(PILL), fill);
    let knob_x = egui::lerp((rect.left() + 12.0)..=(rect.right() - 12.0), t);
    let knob = if t > 0.5 { on_accent(p) } else { p.secondary };
    ui.painter()
        .circle_filled(egui::pos2(knob_x, rect.center().y), 8.0, knob);
    resp
}

/// A sidebar-style navigation row: icon, label, optional selected state. Returns whether it was clicked.
pub fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> bool {
    let p = theme::pal(ui);
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("nav"), &resp);
    if selected {
        ui.painter().rect_filled(row, theme::RADIUS_LG, p.field);
    } else if hover > 0.0 {
        ui.painter()
            .rect_filled(row, theme::RADIUS_LG, p.hover.gamma_multiply(hover));
    }
    let ir = Rect::from_center_size(
        egui::pos2(row.left() + 20.0, row.center().y),
        vec2(18.0, 18.0),
    );
    theme::paint_icon(
        ui.painter(),
        ir,
        icon,
        if selected {
            theme::accent()
        } else {
            p.secondary
        },
    );
    ui.painter().text(
        egui::pos2(row.left() + 40.0, row.center().y),
        Align2::LEFT_CENTER,
        label,
        if selected {
            theme::semibold(theme::BODY + 1.0)
        } else {
            FontId::proportional(theme::BODY + 1.0)
        },
        p.text,
    );
    resp.clicked()
}

/// A square icon button drawn with an animated Lucide icon (plays on hover).
pub fn lottie_button(ui: &mut Ui, name: &str, tip: &str, size: f32) -> Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("lb"), &resp);
    ui.painter().rect_filled(
        rect,
        (size * 0.3).clamp(6.0, 12.0),
        p.hover.gamma_multiply(hover),
    );
    crate::ui::lottie_icon::paint_hover(
        ui,
        resp.id,
        name,
        rect.shrink((size * 0.24).round()),
        p.text,
        resp.hovered(),
    );
    resp.on_hover_text(tip)
}

/// A rounded checkbox with a label.
pub fn checkbox(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = theme::pal(ui);
    let font = FontId::proportional(theme::BODY);
    let w = 26.0 + galley_width(ui, label, font.clone()) + 4.0;
    let (rect, mut resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = crate::ui::motion::progress(
        ui.ctx(),
        resp.id.with("check"),
        *on,
        crate::ui::motion::FAST,
        crate::ui::motion::ease_out_cubic,
    );
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("check-hover"), &resp);
    let bx = Rect::from_center_size(
        egui::pos2(rect.left() + 9.0, rect.center().y),
        vec2(18.0, 18.0),
    );
    let fill = theme::blend(
        theme::blend(p.field, p.hover.gamma_multiply(hover)),
        theme::accent().gamma_multiply(t),
    );
    ui.painter().rect_filled(bx, 6.0, fill);
    if t > 0.0 {
        let c = bx.center();
        let ink = on_accent(p).gamma_multiply(t);
        let stroke = Stroke::new(2.0, ink);
        ui.painter()
            .line_segment([c + vec2(-4.0, 0.5), c + vec2(-1.0, 3.5)], stroke);
        ui.painter()
            .line_segment([c + vec2(-1.0, 3.5), c + vec2(4.5, -3.0)], stroke);
    }
    ui.painter().text(
        egui::pos2(rect.left() + 26.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        p.text,
    );
    resp
}

/// A rounded dropdown: the current value in a field, a rounded menu of options below it.
/// Returns the newly chosen value, if any.
pub fn select<T: PartialEq + Copy>(
    ui: &mut Ui,
    current: T,
    options: &[(T, &str)],
    width: f32,
) -> Option<T> {
    let p = theme::pal(ui);
    let label = options
        .iter()
        .find(|(o, _)| *o == current)
        .map_or("—", |(_, l)| *l);
    let w = if width > 0.0 {
        width
    } else {
        ui.available_width()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, theme::FIELD_H), Sense::click());
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("select"), &resp);
    ui.painter().rect_filled(
        rect,
        theme::RADIUS_MD,
        theme::blend(p.field, p.hover.gamma_multiply(hover)),
    );
    ui.painter().text(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(theme::BODY),
        p.text,
    );
    let ch = Rect::from_center_size(
        egui::pos2(rect.right() - 14.0, rect.center().y),
        vec2(14.0, 14.0),
    );
    theme::paint_icon(ui.painter(), ch, Icon::ChevronDown, p.secondary);
    let mut chosen = None;
    egui::Popup::menu(&resp)
        .width(rect.width().max(150.0))
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (o, l) in options {
                let (row, r) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                let on = *o == current;
                let h = crate::ui::motion::hover(ui.ctx(), r.id.with("opt"), &r);
                if on {
                    ui.painter().rect_filled(row, theme::RADIUS_MD, p.field);
                } else if h > 0.0 {
                    ui.painter()
                        .rect_filled(row, theme::RADIUS_MD, p.hover.gamma_multiply(h));
                }
                ui.painter().text(
                    egui::pos2(row.left() + 12.0, row.center().y),
                    Align2::LEFT_CENTER,
                    *l,
                    if on {
                        theme::semibold(theme::BODY)
                    } else {
                        FontId::proportional(theme::BODY)
                    },
                    p.text,
                );
                if on {
                    let c = egui::pos2(row.right() - 16.0, row.center().y);
                    let s = Stroke::new(2.0, theme::accent());
                    ui.painter()
                        .line_segment([c + vec2(-4.0, 0.5), c + vec2(-1.0, 3.5)], s);
                    ui.painter()
                        .line_segment([c + vec2(-1.0, 3.5), c + vec2(4.5, -3.0)], s);
                }
                if r.clicked() {
                    chosen = Some(*o);
                    ui.close();
                }
            }
        });
    chosen.filter(|c| *c != current)
}

/// A rounded slider: rail, accent fill, round knob. Returns whether the value changed.
pub fn slider(ui: &mut Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>) -> Response {
    let p = theme::pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(
        vec2(ui.available_width().min(160.0), 24.0),
        Sense::click_and_drag(),
    );
    let (lo, hi) = (*range.start(), *range.end());
    if let Some(pos) = resp.interact_pointer_pos() {
        let t = f64::from(((pos.x - rect.left() - 8.0) / (rect.width() - 16.0)).clamp(0.0, 1.0));
        let v = lo + t * (hi - lo);
        if (v - *value).abs() > f64::EPSILON {
            *value = v;
            resp.mark_changed();
        }
    }
    let t = (((*value - lo) / (hi - lo).max(f64::EPSILON)).clamp(0.0, 1.0)) as f32;
    let rail = Rect::from_center_size(rect.center(), vec2(rect.width() - 8.0, 6.0));
    ui.painter().rect_filled(rail, 3.0, p.field);
    let x = rail.left() + 4.0 + t * (rail.width() - 8.0);
    let mut fill = rail;
    fill.set_right(x);
    ui.painter().rect_filled(fill, 3.0, theme::accent());
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("slider"), &resp);
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), 7.0 + hover, p.text);
    resp
}

/// A square icon button with custom painting: `paint(painter, rect, colour)`.
pub fn draw_button(
    ui: &mut Ui,
    tip: &str,
    size: f32,
    enabled: bool,
    paint: impl FnOnce(&Ui, Rect, Color32),
) -> Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("db"), &resp);
    if enabled {
        ui.painter().rect_filled(
            rect,
            (size * 0.3).clamp(6.0, 12.0),
            p.hover.gamma_multiply(hover),
        );
    }
    paint(
        ui,
        rect.shrink((size * 0.24).round()),
        if enabled { p.text } else { p.tertiary },
    );
    resp.on_hover_text(tip)
}

/// Undo or redo arrow button (redo is the undo icon flipped).
pub fn history_button(ui: &mut Ui, redo: bool, enabled: bool) -> Response {
    let tip = if redo { "Redo" } else { "Undo" };
    let mut resp = None;
    ui.add_enabled_ui(enabled, |ui| {
        resp = Some(draw_button(ui, tip, 26.0, enabled, |ui, rect, tint| {
            if redo {
                crate::ui::lottie_icon::paint_rest_mirrored(ui, "undo-2", rect, tint);
            } else {
                crate::ui::lottie_icon::paint_frame(ui, "undo-2", rect, tint, 0.0);
            }
        }));
    });
    resp.unwrap_or_else(|| ui.label(""))
}

/// Height of [`segmented`].
pub const SEGMENTED_H: f32 = 28.0;

/// A pill-shaped segmented control. Each item is `(label, disabled tooltip)`; an item with a
/// tooltip is disabled and shows it on hover. Returns the index the user picked this frame.
pub fn segmented(ui: &mut Ui, items: &[(&str, Option<&str>)], selected: usize) -> Option<usize> {
    let p = theme::pal(ui);
    let font = theme::semibold(theme::SECONDARY + 1.0);
    let pad = 12.0;
    let widths: Vec<f32> = items
        .iter()
        .map(|(l, _)| galley_width(ui, l, font.clone()) + 2.0 * pad)
        .collect();
    let inset = 2.0;
    let total = widths.iter().sum::<f32>() + 2.0 * inset;
    let (outer, _) = ui.allocate_exact_size(vec2(total, SEGMENTED_H), Sense::hover());
    ui.painter()
        .rect_filled(outer, CornerRadius::same(PILL), p.field);
    let mut x = outer.left() + inset;
    let mut picked = None;
    for (i, ((label, off), w)) in items.iter().zip(&widths).enumerate() {
        let rect = Rect::from_min_size(
            egui::pos2(x, outer.top() + inset),
            vec2(*w, SEGMENTED_H - 2.0 * inset),
        );
        x += w;
        let sense = if off.is_some() { Sense::hover() } else { Sense::click() };
        let resp = ui.interact(rect, ui.id().with(("segmented", i)), sense);
        let on = i == selected;
        let hover = crate::ui::motion::hover(ui.ctx(), resp.id.with("seg"), &resp);
        if on {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(PILL), theme::accent());
        } else if off.is_none() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(PILL),
                p.hover.gamma_multiply(hover),
            );
        }
        let ink = if on {
            on_accent(p)
        } else if off.is_some() {
            p.tertiary
        } else {
            p.secondary
        };
        ui.painter()
            .text(rect.center(), Align2::CENTER_CENTER, label, font.clone(), ink);
        if let Some(tip) = off {
            resp.on_hover_text(*tip);
        } else if resp.clicked() && !on {
            picked = Some(i);
        }
    }
    picked
}
