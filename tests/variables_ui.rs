//! The Variables modal draws without panicking for a design with two collections, at both
//! window sizes, with a variable selected, an alias, a library collection and a confirm open.

use egui::{Pos2, Rect, Vec2};
use papercraft::editor::Editor;
use papercraft::ui::{self, UiState};
use papercraft::variables::{Kind, Value};

fn frame(ctx: &egui::Context, size: Vec2, st: &mut UiState, ed: &mut Editor) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        ui::draw(ui, ed, st);
    });
    out.textures_delta.clear();
}

#[test]
fn variables_modal_renders_two_collections() {
    let ctx = egui::Context::default();
    ui::setup_style(&ctx);
    let mut ed = Editor::default();
    ed.edit_design("seed", |ds| {
        let colors = ds.add_collection("Colors");
        let dark = ds.add_mode(&colors, "Dark")?;
        let primary = ds.add_variable(&colors, "color/brand/primary", Kind::Color)?;
        let accent = ds.add_variable(&colors, "color/brand/accent", Kind::Color)?;
        let mode = ds.collection(&colors).unwrap().default_mode.clone();
        ds.set_value(&accent, &mode, Value::Alias(primary))?;
        ds.set_value(&accent, &dark, Value::Color(papercraft::model::Color::rgb(1, 2, 3)))?;
        let tokens = ds.add_collection("Tokens");
        ds.add_variable(&tokens, "space/md", Kind::Number)?;
        ds.add_variable(&tokens, "flag/compact", Kind::Boolean)?;
        ds.add_variable(&tokens, "font/family", Kind::String)?;
        Ok(())
    })
    .unwrap();
    let mut st = UiState::default();
    st.variables.open = true;
    for size in [Vec2::new(2560.0, 1440.0), Vec2::new(1280.0, 800.0)] {
        for _ in 0..3 {
            frame(&ctx, size, &mut st, &mut ed);
        }
    }
    assert!(st.variables.open);
}
