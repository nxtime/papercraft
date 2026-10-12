//! Regression: an inline rename used to re-request keyboard focus every frame, so once started
//! it blocked every other input (number fields, shortcuts) until Enter.
use papercraft::{demo, editor::Editor, render::frame, ui::UiState};

fn run(ctx: &egui::Context, events: Vec<egui::Event>, e: &mut Editor, st: &mut UiState) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 760.0),
        )),
        events,
        ..Default::default()
    };
    frame::layout(ctx, input, e, st)
        .output
        .textures_delta
        .clear();
}

#[test]
fn clicking_another_input_ends_a_page_rename_and_takes_focus() {
    let ctx = egui::Context::default();
    papercraft::ui::setup_style(&ctx);
    let mut e = demo::showcase();
    let first = e.doc.children(None).first().copied().unwrap();
    e.select(vec![first]);
    let mut st = UiState::default();
    st.pages.renaming = Some((0, "Renamed".into()));
    for _ in 0..4 {
        run(&ctx, vec![], &mut e, &mut st);
    }
    assert!(
        st.pages.renaming.is_some(),
        "rename field stays open while focused"
    );
    // Click the X field of the inspector.
    let p = egui::pos2(1075.0, 190.0);
    let btn = |pressed| egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    run(&ctx, vec![egui::Event::PointerMoved(p)], &mut e, &mut st);
    run(&ctx, vec![btn(true)], &mut e, &mut st);
    run(&ctx, vec![btn(false)], &mut e, &mut st);
    for _ in 0..2 {
        run(&ctx, vec![], &mut e, &mut st);
    }
    assert!(st.pages.renaming.is_none(), "the rename ended");
    assert!(
        ctx.egui_wants_keyboard_input(),
        "the number field is being edited"
    );
}
