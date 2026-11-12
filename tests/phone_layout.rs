//! The narrow-window layout: a tab bar and a bottom sheet instead of side panels.

use egui::{Pos2, Rect, Vec2};
use papercraft::editor::Editor;
use papercraft::ui::phone::{Tab, MAX_W};
use papercraft::ui::{self, UiState};

const MIN_TARGET: f32 = 44.0;

struct Rig {
    ctx: egui::Context,
    ed: Editor,
    st: UiState,
    screen: Rect,
    canvas: Rect,
}

impl Rig {
    fn new(w: f32, h: f32) -> Rig {
        let ctx = egui::Context::default();
        ui::setup_style(&ctx);
        let mut rig = Rig {
            ctx,
            ed: Editor::default(),
            st: UiState::default(),
            screen: Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h)),
            canvas: Rect::NOTHING,
        };
        rig.frame();
        rig
    }

    fn frame(&mut self) {
        // Two passes: egui lays some panels out a frame late.
        for _ in 0..2 {
            let input = egui::RawInput { screen_rect: Some(self.screen), ..Default::default() };
            let (ed, st) = (&mut self.ed, &mut self.st);
            let mut canvas = Rect::NOTHING;
            let mut out = self.ctx.run_ui(input, |ui| canvas = ui::draw(ui, ed, st));
            out.textures_delta.clear();
            self.canvas = canvas;
        }
    }

    fn rect(&self, name: &str) -> Option<Rect> {
        self.st.phone.rects.iter().find(|(n, _)| *n == name).map(|(_, r)| *r)
    }
}

#[test]
fn a_narrow_window_gets_a_tab_bar_and_a_full_width_canvas() {
    let rig = Rig::new(390.0, 844.0);
    let tabs = rig.rect("tab-bar").expect("tab bar below 600px");
    let top = rig.rect("top-bar").expect("top bar");
    assert!(tabs.height() >= MIN_TARGET && top.height() >= MIN_TARGET);
    assert_eq!(rig.canvas.width(), 390.0, "no side panels: the canvas takes the full width");
    assert!(rig.canvas.top() >= top.bottom() - 0.5 && rig.canvas.bottom() <= tabs.top() + 0.5);
    assert!(rig.rect("sheet").is_none(), "the canvas tab has no sheet");
    assert_eq!(Tab::ALL.len(), 5);
}

#[test]
fn everything_drawn_stays_inside_the_screen_for_every_tab() {
    let mut rig = Rig::new(390.0, 844.0);
    for tab in Tab::ALL {
        rig.st.phone.tab = tab;
        rig.frame();
        for (name, r) in &rig.st.phone.rects {
            assert!(
                rig.screen.expand(0.5).contains_rect(*r),
                "{name} of the {tab:?} tab leaves the screen: {r:?}"
            );
        }
        if tab == Tab::Chat {
            let page = rig.rect("chat-page").expect("chat is a page, not a sheet");
            assert!(page.bottom() <= rig.rect("tab-bar").unwrap().top() + 0.5, "the page rests on the tab bar");
        } else if tab != Tab::Canvas {
            let sheet = rig.rect("sheet").expect("a sheet for the open tab");
            assert!(sheet.bottom() <= rig.rect("tab-bar").unwrap().top() + 0.5, "the sheet rests on the tab bar");
        }
    }
}

#[test]
fn the_sheet_stays_above_the_soft_keyboard() {
    let mut rig = Rig::new(390.0, 844.0);
    rig.st.phone.tab = Tab::Properties;
    rig.st.phone.keyboard = 300.0;
    rig.frame();
    assert!(rig.rect("sheet").expect("sheet").bottom() <= 844.0 - 300.0 + 0.5);
}

#[test]
fn safe_area_insets_push_the_bars_inward() {
    let mut rig = Rig::new(390.0, 844.0);
    rig.st.phone.safe = [47.0, 0.0, 34.0, 0.0];
    rig.frame();
    assert!(rig.rect("top-bar").unwrap().height() >= MIN_TARGET + 47.0);
    assert!(rig.rect("tab-bar").unwrap().height() >= 52.0 + 34.0);
}

#[test]
fn selecting_a_layer_with_properties_open_keeps_it_open() {
    use papercraft::geom::Rect as WRect;
    use papercraft::model::{Node, NodeId, NodeKind};
    let mut rig = Rig::new(390.0, 844.0);
    let id = rig.ed.add_node(Node::new(NodeId(0), NodeKind::Rect, WRect::new(0.0, 0.0, 50.0, 50.0)), None);
    rig.st.phone.tab = Tab::Properties;
    rig.ed.select(vec![id]);
    rig.frame();
    assert_eq!(rig.st.phone.tab, Tab::Properties);
}

#[test]
fn a_desktop_window_has_no_phone_chrome() {
    let rig = Rig::new(MAX_W + 680.0, 800.0);
    assert!(rig.rect("tab-bar").is_none());
    assert!(rig.canvas.width() < rig.screen.width(), "the side panels take room");
}
