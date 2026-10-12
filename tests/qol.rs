//! Quality-of-life behaviour of the shell.

use papercraft::{demo, render::frame, ui::UiState};

fn run(ctx: &egui::Context, e: &mut papercraft::editor::Editor, st: &mut UiState) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 400.0),
        )),
        ..Default::default()
    };
    frame::layout(ctx, input, e, st)
        .output
        .textures_delta
        .clear();
}

/// Selecting a deep layer on the canvas unfolds the frames above it in the layers panel and
/// asks the panel to scroll to its row.
#[test]
fn canvas_selection_unfolds_and_reveals_the_layer() {
    let ctx = egui::Context::default();
    papercraft::ui::setup_style(&ctx);
    let mut e = demo::showcase();
    let mut st = UiState::default();
    // A leaf somewhere inside a frame.
    let leaf = e
        .doc
        .nodes
        .values()
        .find(|n| n.parent.is_some() && n.children.is_empty() && !e.doc.ancestors(n.id).is_empty())
        .map(|n| n.id)
        .unwrap();
    for a in e.doc.ancestors(leaf) {
        st.collapsed.insert(a);
    }
    run(&ctx, &mut e, &mut st);
    e.selection = vec![leaf];
    run(&ctx, &mut e, &mut st);
    for a in e.doc.ancestors(leaf) {
        assert!(!st.collapsed.contains(&a), "ancestor {a:?} stays folded");
    }
    assert!(
        st.layer_rows.iter().any(|(id, _, _)| *id == leaf),
        "the row exists"
    );
    assert_eq!(
        st.reveal, None,
        "the scroll request was consumed by the row"
    );
}
