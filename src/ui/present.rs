//! Present mode: a page's frames as a clickable prototype. Links jump to frames, open overlays,
//! go back or open an address; frames too tall for the window scroll.

use super::{kit, motion, smart, UiState, RUN_TOAST_LIFT};
use crate::editor::shortcuts::Action;
use crate::editor::{Editor, PresentOverlay, Tool};
use crate::geom::{v2, Camera, Rect, V2};
use crate::model::link::valid_url;
use crate::model::{LinkAction, LinkTrigger, Node, NodeId};
use crate::theme::{self, Surface as _};
use egui::{Align2, RichText};

/// Bottom strip of the canvas taken by the presenter bar; clicks there never follow links.
const PRESENT_BAR_H: f32 = 80.0;
/// How long the camera takes to glide from one presented frame to the next.
const PRESENT_GLIDE_SECS: f32 = 0.45;
/// Most frames the back history remembers.
const MAX_HISTORY: usize = 100;
/// A frame that would be shown smaller than this fraction of its size to fit the window scrolls
/// instead, at full width.
const SCROLL_BELOW_FIT: f64 = 0.5;
/// Share of the window an overlay may take, per axis; a taller one scrolls inside.
const OVERLAY_W_SHARE: f64 = 0.95;
const OVERLAY_H_SHARE: f64 = 0.9;

/// What a click on a layer asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Click {
    Go(NodeId),
    Act(LinkAction),
}

/// What changed after a click or a key.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    /// The shown frame changed (the camera must go there).
    Show,
    /// An overlay opened or closed.
    Overlay,
    /// Open this address (already validated).
    Url(String),
}

struct OpenOverlay {
    target: NodeId,
    scroll: f64,
    /// The camera when it opened: the page behind it stays put.
    camera: Camera,
}

/// Where Present mode is: the shown frame, the frames before it and the open overlay.
#[derive(Default)]
pub struct Nav {
    pub index: usize,
    history: Vec<usize>,
    overlay: Option<OpenOverlay>,
    /// The tool to give back on exit: Present uses the hand so clicks never select or edit.
    prev_tool: Option<Tool>,
    /// The hover, press or focus link that is applied now, until its state ends.
    hover: Option<Hover>,
    /// The layer with keyboard focus (Tab).
    pub focus: Option<NodeId>,
}

/// A hover, press or focus link in effect: what it was started from, so ending it can undo it.
struct Hover {
    kind: LinkTrigger,
    /// On screen (canvas px), where the pointer must stay. Kept in screen space because the
    /// camera moves to the frame the link shows.
    hotspot: Rect,
    from: usize,
    /// The overlay it opened, or the frame it moved to.
    opened_overlay: bool,
    smart: bool,
}

impl Nav {
    fn new(index: usize, tool: Tool) -> Nav {
        Nav { index, prev_tool: Some(tool), ..Nav::default() }
    }

    /// Shows frame `i`, remembering where it came from. Closes any overlay.
    pub fn go(&mut self, i: usize) -> Effect {
        let had_overlay = self.overlay.take().is_some();
        if i == self.index {
            return if had_overlay { Effect::Overlay } else { Effect::None };
        }
        self.history.push(self.index);
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
        self.index = i;
        Effect::Show
    }

    pub fn can_back(&self) -> bool {
        !self.history.is_empty() || self.overlay.is_some()
    }

    /// Closes an open overlay, or else goes back to the previous frame.
    pub fn back(&mut self) -> Effect {
        if self.overlay.take().is_some() {
            return Effect::Overlay;
        }
        match self.history.pop() {
            Some(i) => {
                self.index = i;
                Effect::Show
            }
            None => Effect::None,
        }
    }

    /// Starts a hover, press or focus link (`kind`) when the pointer enters `hotspot` (screen
    /// px). Only frame jumps and overlays can be held; anything else (an address, back) is ignored.
    pub fn hover_enter(&mut self, kind: LinkTrigger, click: Click, hotspot: Rect, smart: bool, frames: &[NodeId], camera: Camera) -> Effect {
        if self.hover.is_some() || !matches!(click, Click::Go(_) | Click::Act(LinkAction::Overlay { .. })) {
            return Effect::None;
        }
        let from = self.index;
        let opened_overlay = matches!(click, Click::Act(LinkAction::Overlay { .. }));
        let effect = self.follow(click, frames, camera);
        self.hover = Some(Hover { kind, hotspot, from, opened_overlay, smart });
        effect
    }

    /// Ends the hover link when the pointer at `at` (screen px) is outside its hotspot: the
    /// overlay closes, or the presentation returns to the frame it left.
    pub fn hover_leave(&mut self, at: V2) -> Effect {
        if self.hover.as_ref().is_none_or(|h| h.hotspot.contains(at)) {
            return Effect::None;
        }
        self.hover_end()
    }

    /// Undoes the held link whatever the pointer does.
    pub fn hover_end(&mut self) -> Effect {
        let Some(h) = self.hover.take() else { return Effect::None };
        if h.opened_overlay {
            return if self.close_overlay() { Effect::Overlay } else { Effect::None };
        }
        if self.index != h.from && self.history.last() == Some(&h.from) {
            self.history.pop();
            self.index = h.from;
            return Effect::Show;
        }
        Effect::None
    }

    pub fn hovering(&self) -> bool {
        self.hover.is_some()
    }

    /// What holds the link in effect, if any.
    pub fn hold_kind(&self) -> Option<LinkTrigger> {
        self.hover.as_ref().map(|h| h.kind)
    }

    /// The overlay that is open, if any.
    pub fn overlay_target(&self) -> Option<NodeId> {
        self.overlay.as_ref().map(|o| o.target)
    }

    /// Whether the hover link in effect tweens layers (leaving tweens them back).
    pub fn hover_smart(&self) -> bool {
        self.hover.as_ref().is_some_and(|h| h.smart)
    }

    pub fn close_overlay(&mut self) -> bool {
        self.overlay.take().is_some()
    }

    pub fn overlay_open(&self) -> bool {
        self.overlay.is_some()
    }

    /// Does what a click asks. `frames` are the presentable frames; a target outside them does
    /// nothing, and so does an address that is not allowed (a page file can say anything).
    pub fn follow(&mut self, click: Click, frames: &[NodeId], camera: Camera) -> Effect {
        match click {
            Click::Go(t) => frames.iter().position(|f| *f == t).map_or(Effect::None, |i| self.go(i)),
            Click::Act(LinkAction::Back) => self.back(),
            Click::Act(LinkAction::Close) => {
                if self.close_overlay() {
                    Effect::Overlay
                } else {
                    Effect::None
                }
            }
            Click::Act(LinkAction::Overlay { target }) => {
                self.overlay = Some(OpenOverlay { target, scroll: 0.0, camera });
                Effect::Overlay
            }
            Click::Act(LinkAction::Url { url }) => valid_url(&url).map_or(Effect::None, Effect::Url),
        }
    }

    /// Steps to the previous (-1) or next (1) frame of `total`, wrapping.
    pub fn step(&mut self, by: i32, total: usize) -> Effect {
        if total == 0 || by == 0 {
            return Effect::None;
        }
        self.go((self.index as i32 + by).rem_euclid(total as i32) as usize)
    }
}

/// The click a layer carries: an action, else a jump to a frame.
pub fn click_of(n: &Node) -> Option<Click> {
    n.link_action.clone().map(Click::Act).or(n.link.map(Click::Go))
}

/// Where an overlay frame sits in the window.
pub struct Placement {
    /// Where the overlay is visible: it is clipped to this and centred in the view.
    pub window: Rect,
    /// How far to move the overlay from where it sits on the canvas.
    pub shift: V2,
    /// The scroll offset used, clamped to what the overlay can scroll.
    pub scroll: f64,
}

/// Places the overlay frame (world rect `frame`) centred in the world rect `view`, scrolled by
/// `scroll`. One taller than the view shows a window of it.
pub fn place_overlay(frame: Rect, view: Rect, scroll: f64) -> Placement {
    let (w, h) = (frame.w.min(view.w * OVERLAY_W_SHARE), frame.h.min(view.h * OVERLAY_H_SHARE));
    let window = Rect::new(view.x + (view.w - w) / 2.0, view.y + (view.h - h) / 2.0, w, h);
    let scroll = scroll.clamp(0.0, (frame.h - h).max(0.0));
    let shift = v2(window.x + (w - frame.w) / 2.0 - frame.x, window.y - scroll - frame.y);
    Placement { window, shift, scroll }
}

/// The camera for a frame too tall to fit the window legibly: full width (at most 1:1), with the
/// top at `pan_y` kept inside the frame. `None` for frames that fit, which are shown whole.
pub fn tall_camera(frame: Rect, viewport: V2, pan_y: f64) -> Option<Camera> {
    if frame.w <= 0.0 || frame.h <= 0.0 || viewport.x <= 0.0 || viewport.y <= 0.0 {
        return None;
    }
    let zoom = (viewport.x / frame.w).min(1.0);
    let fit = (viewport.x / frame.w).min(viewport.y / frame.h);
    if fit >= SCROLL_BELOW_FIT || frame.h * zoom <= viewport.y {
        return None;
    }
    let view_h = viewport.y / zoom;
    Some(Camera {
        pan: v2(frame.x + frame.w / 2.0 - viewport.x / zoom / 2.0, pan_y.clamp(frame.y, frame.y + frame.h - view_h)),
        zoom,
    })
}

/// The page's top-level frames, in layer order: what Present mode steps through.
fn present_frames(ed: &Editor) -> Vec<NodeId> {
    ed.doc
        .root
        .iter()
        .copied()
        .filter(|id| ed.doc.get(*id).is_some_and(|n| n.is_frame() && n.visible && !n.component))
        .collect()
}

/// Starts Present mode on the first frame (or the selected one).
pub fn start_present(ed: &mut Editor, st: &mut UiState) {
    let frames = present_frames(ed);
    let index = ed.selection.first().and_then(|s| frames.iter().position(|f| f == s)).unwrap_or(0);
    st.present_nav = Nav::new(index, ed.tool);
    ed.tool = Tool::Hand;
    st.present = true;
    st.present_glide = None;
    st.present_glide_ready = false;
    show_present_frame(ed, st, None);
}

fn exit_present(ed: &mut Editor, st: &mut UiState) {
    finish_tween(st);
    st.present = false;
    ed.present_overlay = None;
    if let Some(t) = st.present_nav.prev_tool.take() {
        ed.tool = t;
    }
}

/// Shows the current frame. `smart_from`: the frame left, whose same-named layers tween into
/// this one in place of the camera glide.
fn show_present_frame(ed: &mut Editor, st: &mut UiState, smart_from: Option<NodeId>) {
    finish_tween(st);
    // The glide skips sync_overlay, which would leave a closed overlay drawn until it ends.
    ed.present_overlay = None;
    let frames = present_frames(ed);
    let Some(&id) = frames.get(st.present_nav.index) else {
        return;
    };
    let from = ed.camera;
    ed.selection = vec![id];
    ed.run(Action::ZoomSelection);
    ed.selection.clear();
    let tall = ed.doc.world_rect(id).and_then(|r| tall_camera(r, ed.viewport, r.y));
    let to = tall.unwrap_or(ed.camera);
    ed.camera = to;
    let pairs = smart_from.filter(|f| *f != id).map(|f| smart::plan(&ed.doc, f, id)).filter(|p| !p.is_empty());
    if let Some(pairs) = pairs {
        // The layers carry the motion, so the camera cuts.
        st.present_glide = None;
        st.present_glide_ready = true;
        st.present_tween = Some((pairs, web_time::Instant::now()));
        return;
    }
    // Glide instead of cutting (the first frame of a presentation still snaps).
    st.present_glide = (st.present_glide_ready && from != to).then(|| {
        ed.camera = from;
        (from, to, web_time::Instant::now())
    });
    st.present_glide_ready = true;
}

/// Drops a running smart transition. The document never holds the tween outside a paint, so
/// there is nothing to put back.
fn finish_tween(st: &mut UiState) {
    st.present_tween = None;
}

fn step_present_tween(ctx: &egui::Context, st: &mut UiState) {
    let Some((_, start)) = &st.present_tween else { return };
    if start.elapsed().as_secs_f32() >= smart::SMART_SECS || !motion::enabled(ctx) {
        finish_tween(st);
        return;
    }
    ctx.request_repaint();
}

/// Writes the running smart transition into the document for this frame's paint only; hand the
/// result to `unpaint_tween` right after painting.
pub fn paint_tween(ed: &mut Editor, st: &UiState) -> Option<smart::Saved> {
    let (pairs, start) = st.present_tween.as_ref()?;
    let t = (start.elapsed().as_secs_f32() / smart::SMART_SECS).min(1.0);
    let saved = smart::apply(&mut ed.doc, pairs, f64::from(motion::ease_in_out_cubic(t)));
    ed.mark(crate::editor::Changed::Nodes(pairs.iter().map(smart::Pair::id).collect()));
    Some(saved)
}

/// Takes the tween back out after the paint, so saves, sync and agent calls see the real layers.
pub fn unpaint_tween(ed: &mut Editor, saved: Option<smart::Saved>) {
    if let Some(saved) = saved {
        let touched = smart::restore(&mut ed.doc, saved);
        ed.mark(crate::editor::Changed::Nodes(touched.into()));
    }
}

fn step_present_glide(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some((from, to, start)) = st.present_glide else {
        return;
    };
    let t = (start.elapsed().as_secs_f32() / PRESENT_GLIDE_SECS).min(1.0);
    let e = f64::from(motion::ease_in_out_cubic(t));
    if t >= 1.0 || !motion::enabled(ctx) {
        ed.camera = to;
        st.present_glide = None;
        return;
    }
    ed.camera.pan = v2(from.pan.x + (to.pan.x - from.pan.x) * e, from.pan.y + (to.pan.y - from.pan.y) * e);
    // Zoom glides in log space so it feels even.
    ed.camera.zoom = (from.zoom.ln() + (to.zoom.ln() - from.zoom.ln()) * e).exp();
    ctx.request_repaint();
}

/// Keeps a tall frame scrolled inside its bounds (the wheel and a drag pan the camera; this
/// clamps them). Frames that fit are left alone.
fn clamp_tall_frame(ed: &mut Editor, st: &UiState) {
    let Some(&id) = present_frames(ed).get(st.present_nav.index) else { return };
    let cam = ed.doc.world_rect(id).and_then(|r| tall_camera(r, ed.viewport, ed.camera.pan.y));
    if let Some(cam) = cam {
        ed.camera = cam;
    }
}

/// Shows the open overlay (or none) to the renderer and scrolls it by the wheel or a drag.
fn sync_overlay(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(ov) = st.present_nav.overlay.as_mut() else {
        ed.present_overlay = None;
        return;
    };
    let frame = ed.doc.get(ov.target).filter(|n| n.is_frame()).and_then(|_| ed.doc.world_rect(ov.target));
    let Some(frame) = frame else {
        st.present_nav.overlay = None;
        ed.present_overlay = None;
        return;
    };
    ed.camera = ov.camera;
    let dy = ctx.input(|i| i.smooth_scroll_delta.y + if i.pointer.primary_down() { i.pointer.delta().y } else { 0.0 });
    ov.scroll -= f64::from(dy) / ed.camera.zoom;
    let p = place_overlay(frame, ed.camera.visible_world(ed.viewport), ov.scroll);
    ov.scroll = p.scroll;
    ed.present_overlay = Some(PresentOverlay { id: ov.target, shift: p.shift, window: p.window });
}

/// The click under the pointer. With an overlay open only its layers count, and a click outside
/// it is on the backdrop, which closes it.
fn click_at(ed: &Editor, overlay: Option<&PresentOverlay>) -> Option<(Click, bool)> {
    let at = ed.screen_to_world(ed.pointer);
    let (at, root) = match overlay {
        Some(o) if o.window.contains(at) => (at - o.shift, Some(o.id)),
        Some(_) => return Some((Click::Act(LinkAction::Close), false)),
        None => (at, None),
    };
    let hit = crate::editor::hit::hit_test(&ed.doc, at, ed.camera.zoom)?;
    let chain: Vec<NodeId> = std::iter::once(hit).chain(ed.doc.ancestors(hit)).collect();
    if root.is_some_and(|r| !chain.contains(&r)) {
        return None;
    }
    // Hover, press and focus links do nothing on click.
    let (click, node) = chain.iter().find_map(|id| {
        let n = ed.doc.get(*id).filter(|n| n.link_trigger() == LinkTrigger::Click)?;
        Some((click_of(n)?, n.link_smart))
    })?;
    match &click {
        Click::Act(LinkAction::Overlay { target }) if !ed.doc.get(*target).is_some_and(|n| n.is_frame()) => None,
        _ => Some((click, node)),
    }
}

/// What a hover, press or focus link on `n` does: only a jump to a frame or an overlay of one.
fn held_click(ed: &Editor, n: &Node) -> Option<Click> {
    click_of(n).filter(|c| match c {
        Click::Go(_) => true,
        Click::Act(LinkAction::Overlay { target }) => ed.doc.get(*target).is_some_and(|t| t.is_frame()),
        Click::Act(_) => false,
    })
}

/// The `kind` link under the pointer: what it does, its hotspot on screen and whether it tweens.
/// None while an overlay is open or when the target is not a frame.
fn hover_at(ed: &Editor, kind: LinkTrigger) -> Option<(Click, Rect, bool)> {
    if ed.present_overlay.is_some() {
        return None;
    }
    let hit = crate::editor::hit::hit_test(&ed.doc, ed.screen_to_world(ed.pointer), ed.camera.zoom)?;
    let n = std::iter::once(hit).chain(ed.doc.ancestors(hit)).filter_map(|id| ed.doc.get(id)).find(|n| n.link_trigger() == kind)?;
    let click = held_click(ed, n)?;
    let spot = ed.camera.world_rect_to_screen(&ed.doc.world_rect(n.id)?);
    Some((click, spot, n.link_smart))
}

/// The layers Tab visits in `root` (a frame or the open overlay): those with a link, in layer order.
fn focus_order(ed: &Editor, root: NodeId) -> Vec<NodeId> {
    ed.doc.subtree(root).into_iter().filter(|id| ed.doc.get(*id).is_some_and(|n| click_of(n).is_some())).collect()
}

/// The layer Tab (or Shift+Tab with `back`) moves focus to, wrapping; the first (last) when
/// nothing in `order` has focus.
fn next_focus(order: &[NodeId], current: Option<NodeId>, back: bool) -> Option<NodeId> {
    let len = order.len();
    let at = current.and_then(|c| order.iter().position(|id| *id == c));
    let i = match (at, back) {
        (Some(i), false) => (i + 1) % len,
        (Some(i), true) => (i + len - 1) % len,
        (None, false) => 0,
        (None, true) => len.checked_sub(1)?,
    };
    order.get(i).copied()
}

/// Moves keyboard focus on Tab or Shift+Tab: ends the focus link of the layer left (which gives
/// back its frame, so the order is the base frame's own), then holds the one of the layer reached.
fn tab_focus(ed: &Editor, nav: &mut Nav, frames: &[NodeId], back: bool) -> Effect {
    let ended = if nav.hold_kind() == Some(LinkTrigger::Focus) { nav.hover_end() } else { Effect::None };
    let Some(root) = nav.overlay_target().or_else(|| frames.get(nav.index).copied()) else { return ended };
    nav.focus = next_focus(&focus_order(ed, root), nav.focus, back);
    let held = nav.focus.and_then(|id| ed.doc.get(id)).filter(|n| n.link_trigger() == LinkTrigger::Focus);
    let Some((n, click)) = held.and_then(|n| held_click(ed, n).map(|c| (n, c))) else { return ended };
    match nav.hover_enter(LinkTrigger::Focus, click, Rect::default(), n.link_smart, frames, ed.camera) {
        Effect::None => ended,
        e => e,
    }
}

/// Enter or Space on the focused layer follows its click link.
fn activate_focus(ed: &Editor, nav: &mut Nav, frames: &[NodeId]) -> Effect {
    let click = nav.focus.and_then(|id| ed.doc.get(id)).filter(|n| n.link_trigger() == LinkTrigger::Click).and_then(click_of);
    click.map_or(Effect::None, |c| nav.follow(c, frames, ed.camera))
}

/// Outlines the focused layer when no focus link is in effect (a focus link shows its own look).
fn paint_focus_ring(ctx: &egui::Context, ed: &Editor, nav: &Nav, canvas: egui::Rect) {
    let Some(id) = nav.focus.filter(|_| nav.hold_kind() != Some(LinkTrigger::Focus)) else { return };
    let Some(mut w) = ed.doc.world_rect(id) else { return };
    if let Some(o) = &ed.present_overlay {
        w = Rect::new(w.x + o.shift.x, w.y + o.shift.y, w.w, w.h);
    }
    let s = ed.camera.world_rect_to_screen(&w);
    let r = egui::Rect::from_min_size(canvas.min + egui::vec2(s.x as f32, s.y as f32), egui::vec2(s.w as f32, s.h as f32));
    let p = theme::pal_from_ctx(ctx);
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, "present-focus".into()))
        .rect_stroke(r.expand(2.0), 4.0, egui::Stroke::new(2.0, p.text), egui::StrokeKind::Outside);
}

/// The presenter bar: back, previous and next frame, a counter and Exit; Esc, Backspace and the
/// arrow keys work too.
pub fn presenter(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    step_present_glide(ctx, ed, st);
    step_present_tween(ctx, st);
    let frames = present_frames(ed);
    let total = frames.len();
    let (left, right, esc, backspace, tab, activate, down) = ctx.input_mut(|i| {
        let tab = i
            .consume_key(egui::Modifiers::SHIFT, egui::Key::Tab)
            .then_some(true)
            .or_else(|| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab).then_some(false));
        (
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::Backspace),
            tab,
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) || i.consume_key(egui::Modifiers::NONE, egui::Key::Space),
            i.pointer.primary_down(),
        )
    });
    let mut step: i32 = i32::from(right) - i32::from(left);
    let mut back = backspace;
    // Esc closes an open overlay first, then leaves.
    let mut exit = esc && !st.present_nav.close_overlay();
    if st.present_glide.is_none() {
        sync_overlay(ctx, ed, st);
        if !st.present_nav.overlay_open() {
            clamp_tall_frame(ed, st);
        }
    }
    let clicked = ctx.input(|i| i.pointer.primary_clicked())
        && ctx.pointer_interact_pos().is_some_and(|p| canvas.contains(p) && p.y < canvas.bottom() - PRESENT_BAR_H);
    let mut effect = Effect::None;
    let mut smart = false;
    let left = frames.get(st.present_nav.index).copied();
    if let Some((c, s)) = clicked.then(|| click_at(ed, ed.present_overlay.as_ref())).flatten() {
        effect = st.present_nav.follow(c, &frames, ed.camera);
        smart = s;
    }
    let settled = st.present_glide.is_none() && st.present_tween.is_none();
    if effect == Effect::None && settled {
        if let Some(back) = tab {
            smart = true;
            effect = tab_focus(ed, &mut st.present_nav, &frames, back);
        } else if activate {
            effect = activate_focus(ed, &mut st.present_nav, &frames);
        } else if clicked && st.present_nav.hold_kind() == Some(LinkTrigger::Focus) {
            st.present_nav.focus = None;
            effect = st.present_nav.hover_end();
        } else if let Some(kind) = st.present_nav.hold_kind() {
            smart = st.present_nav.hover_smart();
            match kind {
                LinkTrigger::Hover => effect = st.present_nav.hover_leave(ed.pointer),
                LinkTrigger::Press if !down => effect = st.present_nav.hover_end(),
                _ => {}
            }
        } else {
            let found = [(LinkTrigger::Hover, true), (LinkTrigger::Press, down)]
                .into_iter()
                .filter(|(_, on)| *on)
                .find_map(|(kind, _)| hover_at(ed, kind).map(|h| (kind, h)));
            if let Some((kind, (c, spot, s))) = found {
                effect = st.present_nav.hover_enter(kind, c, spot, s, &frames, ed.camera);
                smart = s;
            }
        }
    }
    paint_focus_ring(ctx, ed, &st.present_nav, canvas);
    let p = theme::pal_from_ctx(ctx);
    egui::Area::new("presenter".into())
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - RUN_TOAST_LIFT))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.sidebar)
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show_surface(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.add_enabled_ui(st.present_nav.can_back(), |ui| {
                            if kit::lottie_button(ui, "undo-2", "Back (Backspace)", kit::TOUCH_H).clicked() {
                                back = true;
                            }
                        });
                        if kit::lottie_button(ui, "chevron-left", "Previous frame (\u{2190})", kit::TOUCH_H).clicked() {
                            step = -1;
                        }
                        ui.label(
                            RichText::new(format!("{} / {}", (st.present_nav.index + 1).min(total.max(1)), total.max(1)))
                                .font(theme::semibold(theme::BODY)),
                        );
                        if kit::lottie_button(ui, "chevron-right", "Next frame (\u{2192})", kit::TOUCH_H).clicked() {
                            step = 1;
                        }
                        if kit::button_touch(ui, kit::ButtonKind::Primary, None, "Exit").clicked() {
                            exit = true;
                        }
                    });
                });
        });
    egui::Area::new("presenting".into())
        .order(egui::Order::Foreground)
        .anchor(Align2::LEFT_TOP, egui::vec2(canvas.left() + 24.0, canvas.top() + 24.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.sidebar)
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show_surface(ui, |ui| {
                    ui.label(RichText::new(format!("Presenting \u{b7} {}", ed.page_name())).size(theme::SECONDARY).color(p.secondary));
                });
        });
    if back {
        effect = st.present_nav.back();
        smart = false;
    } else if step != 0 {
        effect = st.present_nav.step(step, total);
        smart = false;
    }
    match effect {
        Effect::Show => show_present_frame(ed, st, left.filter(|_| smart)),
        Effect::Url(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        Effect::Overlay | Effect::None => {}
    }
    if exit {
        exit_present(ed, st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAM: Camera = Camera { pan: V2::ZERO, zoom: 1.0 };

    fn frames() -> Vec<NodeId> {
        (1..=4).map(NodeId).collect()
    }

    fn nav() -> Nav {
        Nav::new(0, Tool::Select)
    }

    #[test]
    fn links_build_a_history_that_back_unwinds() {
        let (mut n, f) = (nav(), frames());
        assert!(!n.can_back());
        assert_eq!(n.follow(Click::Go(NodeId(3)), &f, CAM), Effect::Show);
        assert_eq!(n.follow(Click::Go(NodeId(2)), &f, CAM), Effect::Show);
        assert_eq!((n.index, n.can_back()), (1, true));
        assert_eq!(n.follow(Click::Act(LinkAction::Back), &f, CAM), Effect::Show);
        assert_eq!(n.index, 2);
        assert_eq!(n.back(), Effect::Show);
        assert_eq!((n.index, n.can_back()), (0, false));
        assert_eq!(n.back(), Effect::None, "nothing before the first frame");
    }

    #[test]
    fn a_link_to_the_shown_frame_or_to_nowhere_changes_nothing() {
        let (mut n, f) = (nav(), frames());
        assert_eq!(n.follow(Click::Go(NodeId(1)), &f, CAM), Effect::None);
        assert_eq!(n.follow(Click::Go(NodeId(99)), &f, CAM), Effect::None);
        assert!(!n.can_back());
    }

    #[test]
    fn history_is_capped() {
        let mut n = nav();
        for i in 0..MAX_HISTORY * 2 {
            n.go(1 + i % 2);
        }
        assert_eq!(n.history.len(), MAX_HISTORY);
    }

    #[test]
    fn hover_links_apply_until_the_pointer_leaves_the_hotspot() {
        let (mut n, f) = (nav(), frames());
        let spot = Rect::new(10.0, 10.0, 100.0, 40.0);
        assert_eq!(n.hover_enter(LinkTrigger::Hover, Click::Go(NodeId(3)), spot, false, &f, CAM), Effect::Show);
        assert_eq!((n.index, n.hovering()), (2, true));
        assert_eq!(n.hover_enter(LinkTrigger::Hover, Click::Go(NodeId(2)), spot, false, &f, CAM), Effect::None, "one at a time");
        assert_eq!(n.hover_leave(v2(50.0, 30.0)), Effect::None, "still over it");
        assert_eq!(n.hover_leave(v2(500.0, 30.0)), Effect::Show);
        assert_eq!((n.index, n.can_back(), n.hovering()), (0, false, false), "back where it was, no history left");
        assert_eq!(n.hover_leave(v2(500.0, 30.0)), Effect::None);
    }

    #[test]
    fn a_hover_overlay_closes_on_leave_and_other_actions_never_hover() {
        let (mut n, f) = (nav(), frames());
        let spot = Rect::new(0.0, 0.0, 10.0, 10.0);
        let url = Click::Act(LinkAction::Url { url: "https://a.dev".into() });
        assert_eq!(n.hover_enter(LinkTrigger::Hover, url, spot, false, &f, CAM), Effect::None);
        assert!(!n.hovering());
        let open = Click::Act(LinkAction::Overlay { target: NodeId(4) });
        assert_eq!(n.hover_enter(LinkTrigger::Hover, open, spot, false, &f, CAM), Effect::Overlay);
        assert_eq!(n.hover_leave(v2(99.0, 99.0)), Effect::Overlay);
        assert!(!n.overlay_open());
    }

    #[test]
    fn press_and_focus_links_hold_until_ended_whatever_the_pointer_does() {
        let (mut n, f) = (nav(), frames());
        let spot = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(n.hover_enter(LinkTrigger::Press, Click::Go(NodeId(3)), spot, false, &f, CAM), Effect::Show);
        assert_eq!((n.index, n.hold_kind()), (2, Some(LinkTrigger::Press)));
        assert_eq!(n.hover_leave(v2(500.0, 30.0)), Effect::Show, "leaving the layer also lets go");
        let mut n = nav();
        n.hover_enter(LinkTrigger::Focus, Click::Go(NodeId(2)), Rect::default(), false, &f, CAM);
        assert_eq!(n.hold_kind(), Some(LinkTrigger::Focus));
        assert_eq!(n.hover_end(), Effect::Show);
        assert_eq!((n.index, n.can_back(), n.hold_kind()), (0, false, None));
        assert_eq!(n.hover_end(), Effect::None);
    }

    #[test]
    fn tab_moves_focus_through_the_layers_and_wraps() {
        let order = [NodeId(5), NodeId(6), NodeId(7)];
        assert_eq!(next_focus(&order, None, false), Some(NodeId(5)));
        assert_eq!(next_focus(&order, None, true), Some(NodeId(7)));
        assert_eq!(next_focus(&order, Some(NodeId(7)), false), Some(NodeId(5)));
        assert_eq!(next_focus(&order, Some(NodeId(5)), true), Some(NodeId(7)));
        assert_eq!(next_focus(&order, Some(NodeId(6)), false), Some(NodeId(7)));
        assert_eq!(next_focus(&order, Some(NodeId(99)), false), Some(NodeId(5)), "a layer from another frame starts over");
        assert_eq!(next_focus(&[], None, false), None);
        assert_eq!(next_focus(&[], Some(NodeId(1)), true), None);
    }

    #[test]
    fn stepping_wraps_and_counts_as_navigation() {
        let mut n = nav();
        assert_eq!(n.step(-1, 4), Effect::Show);
        assert_eq!(n.index, 3);
        assert_eq!(n.step(1, 4), Effect::Show);
        assert_eq!(n.index, 0);
        assert_eq!(n.step(1, 0), Effect::None);
    }

    #[test]
    fn overlays_open_over_the_frame_and_close_three_ways() {
        let (mut n, f) = (nav(), frames());
        let open = Click::Act(LinkAction::Overlay { target: NodeId(4) });
        assert_eq!(n.follow(open.clone(), &f, CAM), Effect::Overlay);
        assert!(n.overlay_open());
        assert_eq!(n.index, 0, "the frame behind stays");
        assert_eq!(n.follow(Click::Act(LinkAction::Close), &f, CAM), Effect::Overlay);
        assert!(!n.overlay_open());
        assert_eq!(n.follow(Click::Act(LinkAction::Close), &f, CAM), Effect::None, "nothing open");
        n.follow(open.clone(), &f, CAM);
        assert!(n.close_overlay(), "Escape or the backdrop");
        n.follow(open, &f, CAM);
        assert_eq!(n.follow(Click::Go(NodeId(2)), &f, CAM), Effect::Show, "going on closes it");
        assert!(!n.overlay_open());
    }

    #[test]
    fn back_with_an_overlay_but_no_history_only_closes_it() {
        let (mut n, f) = (nav(), frames());
        n.follow(Click::Act(LinkAction::Overlay { target: NodeId(4) }), &f, CAM);
        assert!(n.can_back());
        assert_eq!(n.back(), Effect::Overlay);
        assert!(!n.overlay_open() && !n.can_back());
    }

    #[test]
    fn back_with_an_overlay_and_history_uses_up_the_back_on_the_overlay() {
        let (mut n, f) = (nav(), frames());
        n.go(2);
        n.follow(Click::Act(LinkAction::Overlay { target: NodeId(4) }), &f, CAM);
        assert_eq!(n.back(), Effect::Overlay);
        assert_eq!(n.index, 2, "still on the frame behind the overlay");
        assert_eq!(n.back(), Effect::Show);
        assert_eq!(n.index, 0);
    }

    #[test]
    fn showing_a_frame_clears_the_overlay_the_renderer_draws() {
        let mut ed = Editor::new(crate::model::Document::default());
        let mut st = UiState::default();
        ed.present_overlay = Some(PresentOverlay { id: NodeId(1), shift: V2::ZERO, window: Rect::new(0.0, 0.0, 1.0, 1.0) });
        show_present_frame(&mut ed, &mut st, None);
        assert!(ed.present_overlay.is_none());
    }

    #[test]
    fn only_allowed_addresses_are_opened() {
        let (mut n, f) = (nav(), frames());
        let url = |u: &str| Click::Act(LinkAction::Url { url: u.into() });
        assert_eq!(n.follow(url("https://a.dev"), &f, CAM), Effect::Url("https://a.dev".into()));
        assert_eq!(n.follow(url("file:///etc/passwd"), &f, CAM), Effect::None);
        assert_eq!(n.index, 0);
    }

    #[test]
    fn a_layer_prefers_its_action_over_a_frame_link() {
        let mut node = Node::new(NodeId(9), crate::model::NodeKind::Rect, Rect::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(click_of(&node), None);
        node.link = Some(NodeId(2));
        assert_eq!(click_of(&node), Some(Click::Go(NodeId(2))));
        node.link_action = Some(LinkAction::Back);
        assert_eq!(click_of(&node), Some(Click::Act(LinkAction::Back)));
    }

    #[test]
    fn an_overlay_is_centred_and_a_tall_one_scrolls_inside_the_window() {
        let view = Rect::new(100.0, 200.0, 400.0, 800.0);
        let small = place_overlay(Rect::new(0.0, 0.0, 200.0, 300.0), view, 50.0);
        assert_eq!((small.window.x, small.window.y), (200.0, 450.0), "centred");
        assert_eq!(small.scroll, 0.0, "it fits: no scroll");
        assert_eq!((small.shift.x, small.shift.y), (200.0, 450.0));
        let tall = Rect::new(10.0, 20.0, 200.0, 2000.0);
        let p = place_overlay(tall, view, 100.0);
        assert_eq!(p.window.h, 720.0, "90% of the view");
        assert_eq!(p.scroll, 100.0);
        assert_eq!(p.shift.y, p.window.y - 100.0 - 20.0, "content moves up as it scrolls");
        assert_eq!(place_overlay(tall, view, 1e9).scroll, 2000.0 - 720.0, "clamped at the end");
        assert_eq!(place_overlay(tall, view, -5.0).scroll, 0.0);
    }

    #[test]
    fn tall_frames_scroll_inside_their_bounds_and_others_are_left_alone() {
        let vp = v2(1000.0, 800.0);
        assert!(tall_camera(Rect::new(0.0, 0.0, 390.0, 844.0), vp, 0.0).is_none(), "a phone fits");
        assert!(tall_camera(Rect::new(0.0, 0.0, 1000.0, 1200.0), vp, 0.0).is_none(), "shrinking to 2/3 is fine");
        let page = Rect::new(50.0, 100.0, 400.0, 3000.0);
        let c = tall_camera(page, vp, 0.0).expect("a long page scrolls");
        assert_eq!(c.zoom, 1.0, "never enlarged");
        assert_eq!(c.pan.y, 100.0, "clamped to the top");
        assert_eq!(c.pan.x, 50.0 + 200.0 - 500.0, "centred");
        assert_eq!(tall_camera(page, vp, 1e9).unwrap().pan.y, 100.0 + 3000.0 - 800.0, "clamped to the bottom");
        assert_eq!(tall_camera(page, vp, 700.0).unwrap().pan.y, 700.0);
        let wide = tall_camera(Rect::new(0.0, 0.0, 2000.0, 9000.0), vp, 0.0).unwrap();
        assert_eq!(wide.zoom, 0.5, "wider than the window: fit the width");
    }
}
