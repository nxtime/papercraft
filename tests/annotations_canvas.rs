//! The annotations overlay draws a page with notes in every mode without panicking.

use egui::{Pos2, Rect, Vec2};
use papercraft::editor::Editor;
use papercraft::geom::Rect as WRect;
use papercraft::model::annotation::Annotation;
use papercraft::model::{Node, NodeId, NodeKind};
use papercraft::ui::annotations_canvas::Mode;
use papercraft::ui::{self, UiState};

fn frame(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        ui::draw(ui, ed, st);
    });
    out.textures_delta.clear();
}

#[test]
fn draws_in_every_mode() {
    let ctx = egui::Context::default();
    ui::setup_style(&ctx);
    let mut ed = Editor::default();
    let id = ed.add_node(
        Node::new(NodeId(0), NodeKind::Rect, WRect::new(100.0, 100.0, 300.0, 200.0)),
        None,
    );
    let mut a = Annotation::new(Some(id), "Rule", "rule");
    a.body = "A long body that wraps over several lines so the card has to clip it to two.".into();
    a.position = papercraft::geom::V2 { x: 340.0, y: 0.0 };
    ed.add_annotation(a);
    ed.add_annotation(Annotation::new(None, "Loose", "a11y"));
    let mut st = UiState::default();
    for mode in [Mode::Full, Mode::Badges, Mode::Hidden] {
        st.annotations.mode = Some(mode);
        for _ in 0..2 {
            frame(&ctx, &mut ed, &mut st);
        }
    }
    st.annotations.mode = Some(Mode::Full);
    st.annotations.hidden_categories.insert("rule".into());
    st.annotations.collapsed.insert(ed.doc.annotations[1].id);
    frame(&ctx, &mut ed, &mut st);
}

#[test]
fn every_default_category_icon_has_a_painted_icon() {
    for c in papercraft::model::annotation::default_categories() {
        assert!(papercraft::theme::icon_by_name(&c.icon).is_some(), "{}", c.icon);
    }
}

/// Smoke: each new icon paints shapes.
#[test]
fn new_icons_paint() {
    use papercraft::theme::{Icon, paint_icon};
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        for (i, ic) in [Icon::Scale, Icon::Database, Icon::Accessibility, Icon::Code, Icon::Zap].into_iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(i as f32 * 40.0, 0.0), Vec2::splat(32.0));
            paint_icon(ui.painter(), r, ic, egui::Color32::WHITE);
        }
    });
    assert!(!out.shapes.is_empty());
    out.textures_delta.clear();
}
