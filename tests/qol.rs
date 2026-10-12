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

/// The layers search shows matching layers with their parents and hides the rest.
#[test]
fn layer_search_filters_rows() {
    let ctx = egui::Context::default();
    papercraft::ui::setup_style(&ctx);
    let mut e = demo::showcase();
    let mut st = UiState::default();
    run(&ctx, &mut e, &mut st);
    let all = st.layer_rows.len();
    assert!(all > 5);
    st.layer_filter = "progress".into();
    run(&ctx, &mut e, &mut st);
    let ids: Vec<_> = st.layer_rows.iter().map(|r| r.0).collect();
    assert!(!ids.is_empty() && ids.len() < all, "{} of {all}", ids.len());
    let names: Vec<String> = ids
        .iter()
        .map(|i| e.doc.get(*i).unwrap().name.to_lowercase())
        .collect();
    assert!(names.iter().any(|n| n.contains("progress")));
    st.layer_filter.clear();
    run(&ctx, &mut e, &mut st);
    assert_eq!(st.layer_rows.len(), all);
}

/// Every Design-tab control runs on a document with variables, modes, tokens, feedback and a
/// component, and deleting a variable unlinks its fills.
#[test]
fn design_tab_renders_and_variables_can_be_deleted() {
    let ctx = egui::Context::default();
    papercraft::ui::setup_style(&ctx);
    let mut e = demo::showcase();
    e.edit("vars", |tx| {
        tx.set_variable_in_mode("Light", "accent", papercraft::model::Color::rgb(1, 2, 3));
        tx.set_variable_in_mode("Dark", "accent", papercraft::model::Color::rgb(4, 5, 6));
        tx.set_token("spacing-md", "16px");
    });
    let id = e.doc.root[0];
    e.edit("link", |tx| {
        let n = tx.node_mut(id).unwrap();
        n.style.fills = vec![papercraft::model::Paint::solid_var(
            papercraft::model::Color::rgb(1, 2, 3),
            "accent",
        )];
        n.comments.push(papercraft::model::Comment {
            id: 1,
            text: "fix".into(),
            author: "user".into(),
            resolved: false,
            reply: None,
        });
    });
    let mut st = UiState {
        show_design: true,
        ..Default::default()
    };
    run(&ctx, &mut e, &mut st);
    e.edit("delete", |tx| tx.remove_variable("accent"));
    assert!(
        !e.doc.variables.contains_key("accent")
            && e.doc.modes.values().all(|m| !m.contains_key("accent"))
    );
    assert_eq!(e.doc.get(id).unwrap().style.fills[0].var(), None);
    run(&ctx, &mut e, &mut st);
}
