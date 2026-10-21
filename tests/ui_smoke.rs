//! The redesigned surfaces draw without panicking: home, settings, command palette, shortcuts sheet
//! and the editor chrome, in a light and a dark theme.

use craft_ui::theme::catppuccin::Flavor;
use egui::{Pos2, Rect, Vec2};
use papercraft::editor::Editor;
use papercraft::theme::{self, Choice};
use papercraft::ui::{self, UiState, home};

fn frame(ctx: &egui::Context, mut f: impl FnMut(&mut egui::Ui)) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| f(ui));
    out.textures_delta.clear();
}

#[test]
fn redesigned_surfaces_draw_in_every_theme() {
    for choice in [
        Choice::Dark,
        Choice::Light,
        Choice::Catppuccin(Flavor::Mocha),
    ] {
        let ctx = egui::Context::default();
        ui::setup_style(&ctx);
        theme::apply_choice(&ctx, choice);
        let mut ed = Editor::default();
        let mut st = UiState::default();
        st.log_agent("get_basic_info", None);
        st.log_agent("write_html", Some("bad html".into()));
        st.agent_endpoint = "http://127.0.0.1:29980/mcp".into();
        st.settings.open = true;
        st.show_help = true;
        st.palette.open = true;
        for _ in 0..3 {
            frame(&ctx, |ui| {
                ui::draw(ui, &mut ed, &mut st);
            });
        }
        for view in [
            ui::panels::SideView::History,
            ui::panels::SideView::Comments,
            ui::panels::SideView::Agent,
        ] {
            st.settings.open = false;
            st.show_help = false;
            st.palette.open = false;
            st.side_extra = Some(view);
            for _ in 0..2 {
                frame(&ctx, |ui| {
                    ui::draw(ui, &mut ed, &mut st);
                });
            }
        }
        st.side_extra = None;
        ui::start_present(&mut ed, &mut st);
        for _ in 0..2 {
            frame(&ctx, |ui| {
                ui::draw(ui, &mut ed, &mut st);
            });
        }
        st.present = false;
        let mut hv = home::HomeView::default();
        hv.dialog = Some(home::ShareDialog { dir: None, text: String::new() });
        frame(&ctx, |ui| home::draw(ui, &mut hv, false, false));
        hv.dialog = None;
        for trash in [false, true] {
            hv.show_trash = trash;
            for _ in 0..2 {
                frame(&ctx, |ui| home::draw(ui, &mut hv, false, false));
            }
        }
    }
}

#[test]
fn prototype_link_and_shadow_round_trip() {
    use papercraft::geom::Rect as R;
    use papercraft::model::{Node, NodeId, NodeKind, Shadow};
    let mut n = Node::new(NodeId(1), NodeKind::Rect, R::new(0.0, 0.0, 10.0, 10.0));
    n.link = Some(NodeId(2));
    n.style.shadow = Some(Shadow {
        x: 0.0,
        y: 4.0,
        blur: 24.0,
        spread: 0.0,
        color: papercraft::model::Color::INK,
    });
    let back: Node = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
    assert_eq!(back.link, Some(NodeId(2)));
    assert_eq!(back.style.shadow.unwrap().blur, 24.0);
}

#[test]
fn present_click_follows_prototype_link() {
    use papercraft::geom::Rect as R;
    use papercraft::model::{Node, NodeId, NodeKind};
    let mut ed = Editor::default();
    let mk =
        |ed: &mut Editor, kind, r: R, parent| ed.add_node(Node::new(NodeId(0), kind, r), parent);
    let a = mk(
        &mut ed,
        NodeKind::Frame { clip: true },
        R::new(0.0, 0.0, 400.0, 300.0),
        None,
    );
    let b = mk(
        &mut ed,
        NodeKind::Frame { clip: true },
        R::new(600.0, 0.0, 400.0, 300.0),
        None,
    );
    let btn = mk(
        &mut ed,
        NodeKind::Rect,
        R::new(50.0, 50.0, 100.0, 100.0),
        Some(a),
    );
    ed.doc.nodes.get_mut(&btn).unwrap().link = Some(b);
    let ctx = egui::Context::default();
    ui::setup_style(&ctx);
    let mut st = UiState::default();
    ed.selection = vec![a];
    ui::start_present(&mut ed, &mut st);
    assert_eq!(st.present_index, 0);
    let at = ed
        .camera
        .world_to_screen(papercraft::geom::v2(100.0, 100.0));
    ed.pointer = at;
    let pos = Pos2::new(at.x as f32, at.y as f32);
    let run = |ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            ui::draw(ui, ed, st);
        });
        out.textures_delta.clear();
    };
    use egui::{Event, PointerButton};
    let btn_ev = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    run(&ctx, &mut ed, &mut st, vec![Event::PointerMoved(pos)]);
    run(&ctx, &mut ed, &mut st, vec![btn_ev(true)]);
    run(&ctx, &mut ed, &mut st, vec![btn_ev(false)]);
    assert_eq!(st.present_index, 1, "click on linked layer opens its frame");
}
