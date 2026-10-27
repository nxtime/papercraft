//! Pointing at the canvas from chat: Pick mode (click a layer, Shift+drag an area), Option+click
//! in Chat mode, the crops chips carry, and the on-canvas Ask box (⌘K on a selection).

use crate::theme::Surface as _;
use super::{SelChip, chip_label, push_chip};
use crate::editor::{Editor, InputEvent};
use crate::geom::{Rect, v2};
use crate::model::{Color, Document, NodeId};
use crate::render::gpu::Headless;
use crate::theme;
use crate::ui::{UiState, kit};
use egui::RichText;

/// Longest edge of a chip crop, in pixels (keeps the image cheap to send).
const CROP_MAX_EDGE: f64 = 1024.0;
/// Small layers are cropped at up to this many pixels per unit, so text stays legible.
const CROP_MAX_SCALE: f64 = 2.0;
/// Most layer ids an area chip lists.
const AREA_MAX_IDS: usize = 40;
const ASK_W: f32 = 320.0;
const INPUT_ID: &str = "chat-input";

/// The Ask box: one line of text sent to chat with the selection as a chip.
#[derive(Debug, Clone, Default)]
pub struct Ask {
    pub text: String,
    focus: bool,
}

/// PNG of `roots` on `page` with their notes drawn (as `get_screenshot {annotations: true}`),
/// limited to `clip` when given.
pub fn crop(gpu: &Headless, ed: &Editor, page: usize, roots: &[NodeId], clip: Option<Rect>) -> Option<Vec<u8>> {
    let (doc, roots) = crate::mcp::exec::with_notes(ed.page_doc(page), roots, &ed.design.categories());
    let b = crate::io::svg::export_bounds(&doc, &roots)?;
    let b = match clip {
        Some(c) => b.intersection(&c)?,
        None => b,
    };
    let scale = (CROP_MAX_EDGE / b.w.max(b.h).max(1.0)).min(CROP_MAX_SCALE);
    let img = crate::io::png::render_clip(gpu, &doc, Some(&roots), scale, Some(Color::WHITE), clip).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// A chip of layers on `page`, with their crop when a GPU is at hand.
pub fn layer_chip(ed: &Editor, gpu: Option<&Headless>, page: usize, ids: &[NodeId]) -> SelChip {
    let name = ids.first().and_then(|id| ed.page_doc(page).get(*id)).map(|n| n.name.clone()).unwrap_or_default();
    SelChip {
        label: chip_label(&name, ids.len()),
        keys: ids.iter().map(|id| crate::mcp::exec::node_key(ed, page, *id)).collect(),
        image: gpu.and_then(|g| crop(g, ed, page, ids, None)),
        ..Default::default()
    }
}

/// A chip of the world rect `r` on `page`: the layers inside it, and a crop of the area.
pub fn area_chip(ed: &Editor, gpu: Option<&Headless>, page: usize, r: Rect) -> SelChip {
    let doc = ed.page_doc(page);
    let roots: Vec<NodeId> = doc.root.iter().copied().filter(|id| doc.world_rect(*id).is_some_and(|w| w.intersects(&r))).collect();
    SelChip {
        label: format!("Area \u{b7} {:.0}\u{d7}{:.0}", r.w, r.h),
        keys: inside(doc, r).into_iter().map(|id| crate::mcp::exec::node_key(ed, page, id)).collect(),
        rect: Some([r.x, r.y, r.w, r.h]),
        page: ed.pages[page].name.clone(),
        image: gpu.filter(|_| !roots.is_empty()).and_then(|g| crop(g, ed, page, &roots, Some(r))),
    }
}

/// The topmost visible layers wholly inside `r`; a layer that only overlaps it is looked into.
fn inside(doc: &Document, r: Rect) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut todo: Vec<NodeId> = doc.root.iter().rev().copied().collect();
    while let Some(id) = todo.pop() {
        if out.len() >= AREA_MAX_IDS {
            break;
        }
        let Some(w) = doc.world_rect(id).filter(|w| w.intersects(&r) && doc.is_effectively_visible(id)) else { continue };
        if r.contains_rect(&w) {
            out.push(id);
        } else if let Some(n) = doc.get(id) {
            todo.extend(n.children.iter().rev());
        }
    }
    out
}

/// The layer a click at `world` picks, as a click would select it (groups and instances whole).
fn pick_at(ed: &Editor, world: crate::geom::V2) -> Option<NodeId> {
    crate::editor::hit::hit_test(&ed.doc, world, ed.camera.zoom).map(|h| ed.click_target(h, false))
}

/// Writes `@name ` into the draft at the composer's cursor (at the end when it has none) and
/// moves the cursor after it.
fn insert_mention(ctx: &egui::Context, draft: &mut String, name: &str) {
    use egui::text::{CCursor, CCursorRange};
    let id = egui::Id::new(INPUT_ID);
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let len = draft.chars().count();
    let at = state.cursor.char_range().map_or(len, |r| r.primary.index.0).min(len);
    let byte = draft.char_indices().nth(at).map_or(draft.len(), |(b, _)| b);
    let lead = if draft[..byte].chars().last().is_some_and(|c| !c.is_whitespace()) { " " } else { "" };
    let text = format!("{lead}@{name} ");
    draft.insert_str(byte, &text);
    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(at + text.chars().count()))));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

/// Adds a picked chip, names it at the composer cursor, and leaves Pick mode.
fn add_pick(ctx: &egui::Context, st: &mut UiState, chip: SelChip) {
    let name = chip.label.split(" \u{b7} ").next().unwrap_or_default().to_owned();
    if !st.chat.sels.iter().any(|c| c.keys == chip.keys && c.rect == chip.rect) {
        insert_mention(ctx, &mut st.chat.draft, &name);
    }
    push_chip(&mut st.chat.sels, chip);
    st.chat.picking = false;
}

/// Pick mode, and Option held in Chat mode: a clear layer over the canvas takes the click instead
/// of the editor, so the user's selection stays as it is. Scrolling still pans and zooms.
pub fn overlay(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    if !st.chat_mode || !(st.chat.picking || ctx.input(|i| i.modifiers.alt)) {
        return;
    }
    if st.chat.picking && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        st.chat.picking = false;
        return;
    }
    let to_world = |ed: &Editor, p: egui::Pos2| ed.camera.screen_to_world(v2((p.x - canvas.min.x) as f64, (p.y - canvas.min.y) as f64));
    let mut picked = None;
    egui::Area::new("chat-pick".into()).fixed_pos(canvas.min).fade_in(false).show(ctx, |ui| {
        let (_, resp) = ui.allocate_exact_size(canvas.size(), egui::Sense::click_and_drag());
        if st.chat.picking && ctx.memory(|m| m.focused().is_none()) {
            // Keys (Esc) come to egui, not to the editor, which would clear the selection.
            resp.request_focus();
        }
        if resp.hovered() || resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        let painter = ui.painter_at(canvas);
        let stroke = egui::Stroke::new(1.5, theme::accent());
        let screen = |ed: &Editor, r: &Rect| {
            let s = ed.camera.world_rect_to_screen(r);
            egui::Rect::from_min_size(canvas.min + egui::vec2(s.x as f32, s.y as f32), egui::vec2(s.w as f32, s.h as f32))
        };
        let (origin, shift, scroll, zoom) = ui.input(|i| (i.pointer.press_origin(), i.modifiers.shift, i.smooth_scroll_delta, i.zoom_delta()));
        let at = resp.interact_pointer_pos().or(resp.hover_pos());
        match (origin, at) {
            (Some(a), Some(b)) if shift && (resp.dragged() || resp.drag_stopped()) => {
                let r = egui::Rect::from_two_pos(a, b);
                painter.rect_filled(r, 0.0, theme::accent().gamma_multiply(0.08));
                painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
                if resp.drag_stopped() {
                    let r = Rect::from_points(to_world(ed, a), to_world(ed, b));
                    picked = Some(area_chip(ed, st.chat.gpu.as_ref(), ed.active_page, r));
                }
            }
            (_, Some(p)) => {
                let hit = pick_at(ed, to_world(ed, p));
                if let Some(r) = hit.and_then(|id| ed.doc.world_rect(id)) {
                    painter.rect_stroke(screen(ed, &r), 0.0, stroke, egui::StrokeKind::Outside);
                }
                if resp.clicked()
                    && let Some(id) = hit
                {
                    picked = Some(layer_chip(ed, st.chat.gpu.as_ref(), ed.active_page, &[id]));
                }
            }
            _ => {}
        }
        if let Some(p) = resp.hover_pos() {
            let pos = v2((p.x - canvas.min.x) as f64, (p.y - canvas.min.y) as f64);
            if zoom != 1.0 {
                ed.handle(InputEvent::Zoom { factor: zoom as f64, pos });
            } else if scroll != egui::Vec2::ZERO {
                ed.handle(InputEvent::Scroll { delta: v2(-scroll.x as f64, -scroll.y as f64), pos });
            }
        }
    });
    if let Some(chip) = picked {
        add_pick(ctx, st, chip);
    }
}

/// ⌘K (or Ask… in the Actions menu): the Ask box on the selection; the palette when none.
pub fn open_ask(ed: &mut Editor, st: &mut UiState) {
    if ed.selection.is_empty() {
        st.palette.toggle();
        return;
    }
    st.chat.ask = Some(Ask { text: String::new(), focus: true });
}

/// The chips the Ask box sends: the selection, as `@ Selection` would add it.
fn ask_chips(ed: &Editor, gpu: Option<&Headless>) -> Vec<SelChip> {
    vec![layer_chip(ed, gpu, ed.active_page, &ed.top_selection())]
}

/// Sends the Ask box's text to chat with the selection chip and its crop. The mode stays as it is;
/// the reply streams into the chat column.
fn send_ask(ctx: &egui::Context, ed: &Editor, st: &mut UiState, text: &str) -> Result<(), String> {
    let chips = ask_chips(ed, st.chat.gpu.as_ref());
    let notes = super::selection_notes(ed, &chips);
    st.chat.poll_detect(ctx, &st.settings.chat);
    st.chat.ensure_chat();
    st.chat.ensure_defaults(&st.settings.chat);
    st.chat.refresh_ds(ed, true);
    st.chat.send_with(ctx, &chips, &notes, &[], text.trim())
}

/// The Ask box, beside the selection where the New annotation popover opens. Enter sends, Esc closes.
pub fn ask_popover(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    let Some(mut ask) = st.chat.ask.take() else { return };
    let Some(bounds) = ed.selection_bounds() else { return };
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        return;
    }
    let name = ed.top_selection().first().and_then(|id| ed.doc.get(*id)).map(|n| n.name.clone()).unwrap_or_default();
    let pos = super::super::annotations::anchor_pos(ed, canvas, &bounds, ASK_W);
    let (mut send, mut close) = (false, false);
    egui::Area::new("chat-ask".into()).order(egui::Order::Foreground).constrain_to(canvas).fixed_pos(pos).show(ctx, |ui| {
        let p = theme::pal(ui);
        kit::card_frame(ui).inner_margin(12).show_surface(ui, |ui| {
            ui.set_width(ASK_W - 24.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("@").font(theme::semibold(theme::BODY)).color(theme::accent()));
                ui.add(egui::Label::new(RichText::new(&name).font(theme::semibold(theme::BODY))).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| close = kit::close_button(ui).clicked());
            });
            let r = crate::theme::lit(ui, |ui| ui.add(egui::TextEdit::singleline(&mut ask.text).hint_text("Ask about this layer").desired_width(f32::INFINITY).margin(theme::INPUT_MARGIN)));
            if std::mem::take(&mut ask.focus) {
                r.request_focus();
            }
            send = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !ask.text.trim().is_empty();
            ui.label(RichText::new("Enter sends to chat \u{b7} Esc closes").size(theme::SECONDARY).color(p.tertiary));
        });
    });
    if close {
        return;
    }
    if send {
        match send_ask(ctx, ed, st, &ask.text) {
            Ok(()) => return,
            Err(e) => {
                ed.status = e;
                ask.focus = true;
            }
        }
    }
    st.chat.ask = Some(ask);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, NodeKind};

    /// A frame at (0,0) 400×300 with a card (20,20 100×80) and a button (300,200 80×40) in it.
    fn fixture() -> (Editor, NodeId, NodeId, NodeId) {
        let mut ed = Editor::default();
        let f = ed.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: true }, Rect::new(0.0, 0.0, 400.0, 300.0)), None);
        let card = ed.add_node(Node::new(NodeId(0), NodeKind::Rect, Rect::new(20.0, 20.0, 100.0, 80.0)), Some(f));
        let button = ed.add_node(Node::new(NodeId(0), NodeKind::Rect, Rect::new(300.0, 200.0, 80.0, 40.0)), Some(f));
        (ed, f, card, button)
    }

    #[test]
    fn an_area_chip_lists_the_layers_inside_and_its_rect() {
        let (ed, f, card, button) = fixture();
        let key = |id| crate::mcp::exec::node_key(&ed, 0, id);
        let chip = area_chip(&ed, None, 0, Rect::new(10.0, 10.0, 150.0, 120.0));
        assert_eq!(chip.keys, [key(card)], "the frame only overlaps, so its card is listed");
        let line = super::super::chip_line(&chip);
        assert_eq!(line, format!("[Area] page {}, rect 10,10,150,120, contains: {}", ed.pages[0].name, key(card)));
        let all = area_chip(&ed, None, 0, Rect::new(-10.0, -10.0, 500.0, 400.0));
        assert_eq!(all.keys, [key(f)]);
        assert!(area_chip(&ed, None, 0, Rect::new(5000.0, 5000.0, 10.0, 10.0)).keys.is_empty());
        let _ = button;
    }

    #[test]
    fn several_picks_give_several_chips_and_mentions() {
        let (ed, _, card, button) = fixture();
        let ctx = egui::Context::default();
        let mut st = UiState::default();
        add_pick(&ctx, &mut st, layer_chip(&ed, None, 0, &[card]));
        add_pick(&ctx, &mut st, area_chip(&ed, None, 0, Rect::new(290.0, 190.0, 100.0, 60.0)));
        add_pick(&ctx, &mut st, layer_chip(&ed, None, 0, &[button]));
        add_pick(&ctx, &mut st, layer_chip(&ed, None, 0, &[card]));
        assert_eq!(st.chat.sels.len(), 3, "a layer picked twice is one chip");
        assert!(st.chat.sels[1].rect.is_some());
        assert_eq!(st.chat.draft.matches('@').count(), 3);
        assert!(!st.chat.picking);
        assert!(st.chat.sels.iter().all(|c| c.image.is_none()), "no GPU, no crop");
    }

    #[test]
    fn a_mention_goes_in_at_the_cursor() {
        use egui::text::{CCursor, CCursorRange};
        let ctx = egui::Context::default();
        let id = egui::Id::new(INPUT_ID);
        let mut state = egui::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(5))));
        state.store(&ctx, id);
        let mut draft = "make bigger".to_owned();
        insert_mention(&ctx, &mut draft, "Card");
        assert_eq!(draft, "make @Card bigger");
    }

    #[test]
    fn picking_does_not_touch_the_selection() {
        let (mut ed, _, card, button) = fixture();
        ed.selection = vec![button];
        let ctx = egui::Context::default();
        let mut st = UiState::default();
        let id = pick_at(&ed, v2(50.0, 50.0)).unwrap();
        add_pick(&ctx, &mut st, layer_chip(&ed, None, 0, &[id]));
        assert_eq!(ed.selection, [button]);
        assert_eq!(st.chat.sels[0].keys, [crate::mcp::exec::node_key(&ed, 0, card)]);
    }

    #[test]
    fn the_ask_box_sends_what_chip_and_composer_send() {
        let (mut ed, _, card, _) = fixture();
        ed.selection = vec![card];
        let dir = std::env::temp_dir().join(format!("papercraft-ask-{:016x}", fastrand::u64(..)));
        let mut c = super::super::ChatState { project: Some(dir.clone()), ..Default::default() };
        // Composer: @ Selection, then the text.
        super::super::add_selection(&mut ed, &mut c);
        let notes = super::super::selection_notes(&ed, &c.sels);
        let (sels, text) = (c.sels.clone(), "Make it pop");
        let composer = c.compose(&sels, &notes, &[], text).unwrap();
        // Ask box: the same text on the selection.
        let chips = ask_chips(&ed, None);
        let ask = c.compose(&chips, &super::super::selection_notes(&ed, &chips), &[], text).unwrap();
        assert_eq!(ask, composer);
        assert!(ask.starts_with("[Selection] ") && ask.ends_with(text));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn crops_become_attachments_and_an_area_crop_is_clipped() {
        let Ok(gpu) = Headless::new() else { return };
        let (mut ed, _, card, _) = fixture();
        ed.selection.clear();
        let area = area_chip(&ed, Some(&gpu), 0, Rect::new(10.0, 10.0, 150.0, 120.0));
        let png = image::load_from_memory(area.image.as_deref().expect("an area crop")).unwrap();
        assert_eq!((png.width(), png.height()), (300, 240), "the crop is the area at 2x, not the whole frame");
        let layer = layer_chip(&ed, Some(&gpu), 0, &[card]);
        assert!(layer.image.is_some(), "layer chips carry a crop too");
        let dir = std::env::temp_dir().join(format!("papercraft-crop-{:016x}", fastrand::u64(..)));
        let mut c = super::super::ChatState { project: Some(dir.clone()), ..Default::default() };
        let msg = c.compose(&[area, layer], &[], &[], "Why?").unwrap();
        let attached: Vec<&str> = msg.lines().filter(|l| l.starts_with("[Attached] ")).collect();
        assert_eq!(attached.len(), 2, "{msg}");
        assert!(attached.iter().all(|l| std::path::Path::new(&l["[Attached] ".len()..]).exists()), "CLI chats get the crops as files");
        assert!(msg.starts_with("[Area] page "), "{msg}");
        std::fs::remove_dir_all(dir).ok();
    }
}
