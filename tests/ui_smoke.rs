//! The redesigned surfaces draw without panicking: home, settings, command palette, shortcuts sheet
//! and the editor chrome, in a light and a dark theme.

use craft_ui::theme::catppuccin::Flavor;
use egui::{Pos2, Rect, Vec2};
use papercraft::editor::Editor;
use papercraft::theme::{self, Choice};
use papercraft::chat::events::{Event, Turn};
use papercraft::chat::{Message, Role};
use papercraft::runs::{Run, RunState};
use papercraft::ui::chat::{Approval, Detect, EndpointState, TurnHandle};
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
        st.log_agent("get_basic_info", &serde_json::Value::Null, None);
        st.log_agent("write_html", &serde_json::Value::Null, Some("bad html".into()));
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
        hv.dialog = Some(home::ShareDialog {
            dir: None,
            text: String::new(),
        });
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

#[test]
fn interface_scale_changes_the_point_size_and_still_draws() {
    let ctx = egui::Context::default();
    ui::setup_style(&ctx);
    let mut ed = Editor::default();
    let mut st = UiState::default();
    frame(&ctx, |ui| {
        ui::draw(ui, &mut ed, &mut st);
    });
    let before = ctx.pixels_per_point();
    theme::apply_ui_scale(&ctx, 1.25, false);
    frame(&ctx, |ui| {
        ui::draw(ui, &mut ed, &mut st);
    });
    assert!((ctx.pixels_per_point() / before - 1.25).abs() < 1e-3);
    theme::apply_ui_scale(&ctx, 1.0, false);
}

#[test]
fn chat_mode_draws_with_inspector_overlay() {
    let dir = std::env::temp_dir().join(format!("papercraft-chat-ui-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    for (w, h) in [(1280.0, 800.0), (2560.0, 1440.0)] {
        let ctx = egui::Context::default();
        ui::setup_style(&ctx);
        let mut ed = Editor::default();
        let mut st = UiState { project_dir: Some(dir.clone()), ..UiState::default() };
        st.chat.project = Some(dir.clone()); // as if the project was already seen
        st.chat.detect = Some(Detect::default()); // no scan (it starts CLIs and reads the keychain)
        for inspector in [false, true] {
            // Set the fields, not `set_chat_mode`: that would write the user's chat prefs.
            st.chat_mode = true;
            st.chat.inspector = inspector;
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    ui::draw(ui, &mut ed, &mut st);
                });
                out.textures_delta.clear();
            }
        }
        assert!(st.chat.chat.is_some(), "an empty chat is opened");
        assert!(papercraft::chat::store::list(&dir).is_empty(), "nothing saved before the first message");
    }
    std::fs::remove_dir_all(dir).ok();
}

struct FakeTurn;

impl Turn for FakeTurn {
    fn stop(&mut self) {}
    fn is_running(&self) -> bool {
        true
    }
}

fn chat_ui(dir: &std::path::Path) -> UiState {
    let mut st = UiState { project_dir: Some(dir.to_path_buf()), ..UiState::default() };
    st.chat.project = Some(dir.to_path_buf());
    st.chat.detect = Some(Detect { claude: Some(("/bin/claude".into(), "2.3.1".into())), endpoints: vec![EndpointState { id: "anthropic".into(), key: true, ready: true }], default_endpoint: "anthropic".into(), ..Detect::default() });
    st.chat_mode = true;
    st
}

fn draw_chat(st: &mut UiState, w: f32, h: f32) {
    let ctx = egui::Context::default();
    ui::setup_style(&ctx);
    let mut ed = Editor::default();
    for _ in 0..3 {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            ui::draw(ui, &mut ed, st);
        });
        out.textures_delta.clear();
    }
}

fn tool(st: &mut UiState, id: &str, name: &str, args: serde_json::Value, done: Option<Result<serde_json::Value, &str>>) {
    st.chat.apply(Event::ToolStart { id: id.into(), name: name.into(), args });
    match done {
        Some(Ok(r)) => st.chat.apply(Event::ToolEnd { id: id.into(), result: Some(r), error: None }),
        Some(Err(e)) => st.chat.apply(Event::ToolEnd { id: id.into(), result: None, error: Some(e.into()) }),
        None => false,
    };
}

#[test]
fn chat_states_draw_from_recorded_events() {
    use serde_json::json;
    let dir = std::env::temp_dir().join(format!("papercraft-chat-states-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    // The 1x1 PNG a screenshot tool would return.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
    let shot = json!([{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": png}}]);

    // empty, setup, picker
    let mut st = chat_ui(&dir);
    draw_chat(&mut st, 1280.0, 800.0);
    st.chat.picker_open = true;
    draw_chat(&mut st, 1280.0, 800.0);
    st.chat.detect = Some(Detect::default());
    draw_chat(&mut st, 1280.0, 800.0);

    // streaming: reasoning, grouped tool cards (one running, one failed), a screenshot, the plan
    let mut st = chat_ui(&dir);
    st.chat.ensure_chat();
    st.chat.chat.as_mut().unwrap().messages.push(Message { role: Role::User, text: "[Selection] Card (ab12cd:3)\n\nBuild it".into(), at: 10, ..Default::default() });
    st.chat.apply(Event::Thinking("Planning the card".into()));
    st.chat.apply(Event::Text("I will build **the card**:\n- one\n- two `x`\n```html\n<div/>\n```".into()));
    tool(&mut st, "1", "create_artboard", json!({"name": "Test"}), Some(Ok(json!([{"type": "text", "text": "{\"nodeId\":\"ab12cd:4\"}"}]))));
    tool(&mut st, "2", "get_screenshot", json!({"nodeId": "ab12cd:4"}), Some(Ok(shot)));
    tool(&mut st, "3", "update_styles", json!({"nodeId": "ab12cd:5"}), Some(Err("Node ab12cd:5 not found")));
    tool(&mut st, "4", "TodoWrite", json!({"todos": [{"content": "A", "status": "completed"}, {"content": "B", "status": "in_progress"}, {"content": "C", "status": "pending"}]}), Some(Ok(json!("ok"))));
    tool(&mut st, "5", "write_html", json!({"html": "x ".repeat(900)}), None);
    st.chat.turn = Some(TurnHandle::Cli(Box::new(FakeTurn)));
    draw_chat(&mut st, 1280.0, 800.0);
    draw_chat(&mut st, 2560.0, 1440.0);

    // waiting for approval
    st.chat.approvals.push(Approval { id: "5".into(), name: "delete_pages".into(), args: json!({"name": "Landing"}) });
    draw_chat(&mut st, 1280.0, 800.0);

    // finished: usage line, checkpoint, run card, suggestions, error row
    let mut st = chat_ui(&dir);
    st.chat.ensure_chat();
    st.chat.chat.as_mut().unwrap().messages.push(Message { role: Role::User, text: "Build it".into(), at: 10, ..Default::default() });
    st.chat.apply(Event::Text("Done.".into()));
    tool(&mut st, "1", "create_artboard", json!({"name": "Test"}), Some(Ok(json!("ok"))));
    tool(&mut st, "2", "lint_design", json!({}), Some(Ok(json!("ok"))));
    st.chat.apply(Event::TurnEnd {
        usage: Some(papercraft::chat::Usage { input: 12_400, output: 1_100, cache_read: 9_800, cache_write: 0, cost_usd: Some(0.06) }),
        session: None,
        stop_reason: None,
    });
    st.runs.push(Run { id: "r1".into(), number: 3, started: 20, ended: Some(12_020), calls: 2, added: 6, state: RunState::Review, ..Default::default() });
    st.runs_key = Some(dir.clone());
    draw_chat(&mut st, 1280.0, 800.0);
    st.chat.apply(Event::Error("boom".into()));
    draw_chat(&mut st, 1280.0, 800.0);
    std::fs::remove_dir_all(dir).ok();
}
