use egui::{Event, PointerButton, Pos2, Rect, Vec2};
use papercraft::{
    editor::Editor,
    ui::{self, UiState},
};

const SCREEN_SIZE: [u32; 2] = [1000, 700];

struct Harness {
    ctx: egui::Context,
    ed: Editor,
    st: UiState,
    output: egui::FullOutput,
    time: f64,
    capture: Option<(
        papercraft::render::gpu::Headless,
        papercraft::render::frame::Painter,
        wgpu::Texture,
    )>,
}

impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ui::setup_style(&ctx);
        let mut ed = Editor::default();
        ed.add_page();
        ed.add_page();
        ed.switch_page(0);
        let mut h = Self {
            ctx,
            ed,
            st: UiState::default(),
            output: Default::default(),
            time: 0.0,
            capture: None,
        };
        // The pages list is collapsed behind the page selector by default; these tests drive the list.
        h.st.pages.collapsed = false;
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.1;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(SCREEN_SIZE[0] as f32, SCREEN_SIZE[1] as f32),
            )),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let mut canvas = Rect::NOTHING;
        self.output = self.ctx.run_ui(input, |ui| {
            canvas = ui::draw(ui, &mut self.ed, &mut self.st);
        });
        if self.capture.is_none() {
            // Nothing uploads the textures; egui insists they are handled or cleared.
            self.output.textures_delta.clear();
        }
        if let Some((gpu, painter, target)) = &mut self.capture {
            let view = target.create_view(&Default::default());
            painter.paint(
                &gpu.device,
                &gpu.queue,
                &view,
                SCREEN_SIZE,
                &self.ctx,
                papercraft::render::frame::Layout {
                    output: self.output.clone(),
                    canvas,
                },
                &self.ed,
                true,
            );
            self.output.textures_delta.clear();
        }
    }

    fn text(&self, text: &str) -> Rect {
        // The page selector repeats the current page's name above the list, and the inspector shows
        // it again: the list row is the last match inside the sidebar.
        self.text_in_sidebar(text)
            .unwrap_or_else(|| self.text_match(text, false))
    }

    fn text_in_sidebar(&self, text: &str) -> Option<Rect> {
        fn collect(shape: &egui::epaint::Shape, text: &str, out: &mut Vec<Rect>) {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                    out.push(t.galley.rect.translate(t.pos.to_vec2()));
                }
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().for_each(|s| collect(s, text, out))
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for s in &self.output.shapes {
            collect(&s.shape, text, &mut found);
        }
        found.into_iter().filter(|r| r.left() < 300.0).next_back()
    }

    fn popup_text(&self, text: &str) -> Rect {
        self.text_match(text, true)
    }

    fn text_match(&self, text: &str, last: bool) -> Rect {
        fn find(shape: &egui::epaint::Shape, text: &str) -> Option<Rect> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.galley.rect.translate(t.pos.to_vec2()))
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
                _ => None,
            }
        }
        let mut matches = self
            .output
            .shapes
            .iter()
            .filter_map(|s| find(&s.shape, text));
        let found = if last {
            matches.next_back()
        } else {
            matches.next()
        };
        found.unwrap_or_else(|| panic!("missing text: {text}"))
    }

    fn click(&mut self, rect: Rect, button: PointerButton) {
        let pos = rect.center();
        self.frame(vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        self.frame(vec![Event::PointerButton {
            pos,
            button,
            pressed: false,
            modifiers: Default::default(),
        }]);
        self.frame(vec![]);
    }

    fn enable_capture(&mut self) -> bool {
        let Ok(gpu) = papercraft::render::gpu::Headless::new() else {
            return false;
        };
        let painter = papercraft::render::frame::Painter::new(
            &gpu.device,
            papercraft::render::gpu::OFFSCREEN_FORMAT,
        );
        let target = gpu.target(SCREEN_SIZE);
        self.capture = Some((gpu, painter, target));
        self.ctx = egui::Context::default();
        ui::setup_style(&self.ctx);
        self.frame(vec![]);
        self.frame(vec![]);
        true
    }

    fn screenshot(&self, name: &str) {
        let (gpu, _, target) = self.capture.as_ref().unwrap();
        gpu.read(target)
            .save(std::env::temp_dir().join(name))
            .unwrap();
    }

    fn request_page_deletion(&mut self) {
        self.click(self.text("Page 1"), PointerButton::Secondary);
        self.frame(vec![]);
        self.click(self.text("Delete"), PointerButton::Primary);
    }
}

#[test]
fn deleting_first_page_waits_for_confirmation_and_does_not_crash() {
    let mut h = Harness::new();
    h.request_page_deletion();
    assert_eq!(
        h.ed.pages.len(),
        3,
        "requesting deletion must not remove page"
    );
    h.click(h.text("Delete"), PointerButton::Primary);
    assert_eq!(h.ed.pages.len(), 2);
    assert!(h.ed.pages.iter().all(|p| p.name != "Page 1"));
    h.frame(vec![]);
}

#[test]
fn cancelling_page_deletion_preserves_pages() {
    let mut h = Harness::new();
    h.request_page_deletion();
    h.click(h.text("Cancel"), PointerButton::Primary);
    assert_eq!(h.ed.pages.len(), 3);
}

#[test]
fn confirmation_targets_page_id_after_reordering() {
    let mut h = Harness::new();
    let target = h.ed.pages[0].id.clone();
    h.request_page_deletion();
    h.ed.move_page(0, 2);
    h.ed.switch_page(0);
    h.frame(vec![]);
    h.click(h.text("Delete"), PointerButton::Primary);
    assert_eq!(h.ed.pages.len(), 2);
    assert!(h.ed.pages.iter().all(|p| p.id != target));
}

#[test]
fn stale_confirmation_cannot_delete_from_another_project() {
    let mut h = Harness::new();
    h.request_page_deletion();
    let pages = (0..h.ed.pages.len()).map(|i| h.ed.page_data(i)).collect();
    h.ed.open_pages(pages, None);
    h.frame(vec![]);
    assert!(h.st.pending_removal.is_none());
    assert_eq!(h.ed.pages.len(), 3);
}

#[test]
fn escape_cancels_destructive_confirmation() {
    let mut h = Harness::new();
    h.request_page_deletion();
    h.frame(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);
    assert!(h.st.pending_removal.is_none());
    assert_eq!(h.ed.pages.len(), 3);
}

#[test]
fn page_menu_has_consistent_width() {
    let mut h = Harness::new();
    h.click(h.text("Page 1"), PointerButton::Secondary);
    h.frame(vec![]);
    let layer = h.ctx.layer_id_at(h.text("Delete").center()).unwrap();
    let menu = h.ctx.memory(|m| m.area_rect(layer.id)).unwrap();
    assert!((220.0..=260.0).contains(&menu.width()), "menu: {menu:?}");
}

#[test]
fn group_menu_moves_pages_and_removal_keeps_them() {
    let mut h = Harness::new();
    let page = h.ed.pages[0].id.clone();
    h.ed.create_page_group("Screens");
    h.frame(vec![]);
    h.click(h.text("Page 1"), PointerButton::Secondary);
    let pos = h.text("Move to group").center();
    h.frame(vec![Event::PointerMoved(pos)]);
    h.frame(vec![]);
    h.frame(vec![]);
    h.click(h.popup_text("Screens"), PointerButton::Primary);
    assert_eq!(h.ed.page_group(&page), Some("Screens"));
    ui::confirm::request(
        &mut h.st,
        &h.ed,
        ui::confirm::Removal::Group("Screens".into()),
    );
    h.frame(vec![]);
    h.frame(vec![]);
    h.click(h.text("Remove"), PointerButton::Primary);
    assert!(!h.ed.page_groups.contains_key("Screens"));
    assert_eq!(h.ed.pages.len(), 3);
}

#[test]
fn design_removals_require_confirmation_and_apply_to_all_pages() {
    for kind in ["mode", "variable", "token"] {
        let mut h = Harness::new();
        for page in 0..h.ed.pages.len() {
            h.ed.with_page(page, |ed| {
                ed.edit("Setup", |tx| {
                    tx.set_variable_in_mode(
                        "Mocha",
                        "accent",
                        papercraft::model::Color::rgb(1, 2, 3),
                    );
                    tx.set_token("space-1", "4px");
                })
            });
        }
        let action = || match kind {
            "mode" => ui::confirm::Removal::Mode("Mocha".into()),
            "variable" => ui::confirm::Removal::Variable("accent".into()),
            _ => ui::confirm::Removal::Token("space-1".into()),
        };
        ui::confirm::request(&mut h.st, &h.ed, action());
        h.frame(vec![]);
        h.frame(vec![]);
        h.click(h.text("Cancel"), PointerButton::Primary);
        assert!(h.ed.doc.modes.contains_key("Mocha"));
        assert!(h.ed.doc.variables.contains_key("accent"));
        assert!(h.ed.doc.tokens.contains_key("space-1"));
        ui::confirm::request(&mut h.st, &h.ed, action());
        h.frame(vec![]);
        h.frame(vec![]);
        h.click(h.text("Delete"), PointerButton::Primary);
        for page in 0..h.ed.pages.len() {
            let doc = h.ed.page_doc(page);
            match kind {
                "mode" => assert!(!doc.modes.contains_key("Mocha")),
                "variable" => assert!(!doc.variables.contains_key("accent")),
                _ => assert!(!doc.tokens.contains_key("space-1")),
            }
        }
    }
}

#[test]
fn page_management_visuals() {
    let mut h = Harness::new();
    if !h.enable_capture() {
        return;
    }
    h.ed.create_page_group("Screens");
    let page = h.ed.pages[0].id.clone();
    h.ed.assign_page_group(&page, Some("Screens"));
    h.frame(vec![]);
    h.frame(vec![]);
    h.click(h.text("Page 1"), PointerButton::Secondary);
    h.frame(vec![]);
    h.screenshot("papercraft-page-menu.png");
    h.click(h.text("Delete"), PointerButton::Primary);
    h.frame(vec![]);
    h.screenshot("papercraft-page-confirmation.png");
    h.click(h.text("Cancel"), PointerButton::Primary);
    h.st.show_design = true;
    h.ed.edit("Design tokens", |tx| {
        tx.set_variable_in_mode(
            "Mocha",
            "accent",
            papercraft::model::Color::rgb(203, 166, 247),
        );
        tx.set_token("space-1", "4px");
    });
    h.frame(vec![]);
    h.frame(vec![]);
    h.screenshot("papercraft-token-icons.png");
}

#[test]
fn deleting_an_inactive_empty_page_keeps_active_page() {
    let mut h = Harness::new();
    h.ed.switch_page(2);
    let active = h.ed.active_page_id().to_string();
    h.frame(vec![]);
    h.request_page_deletion();
    h.click(h.text("Delete"), PointerButton::Primary);
    assert_eq!(h.ed.pages.len(), 2);
    assert_eq!(h.ed.active_page_id(), active);
    h.frame(vec![]);
}

#[test]
fn last_page_cannot_be_deleted_from_menu() {
    let mut h = Harness::new();
    h.ed.delete_page(2);
    h.ed.delete_page(1);
    h.frame(vec![]);
    h.request_page_deletion();
    assert_eq!(h.ed.pages.len(), 1);
    assert!(h.st.pending_removal.is_none());
}

#[test]
fn repeated_page_confirmations_delete_each_requested_page() {
    let mut h = Harness::new();
    h.st.native_chrome = true;
    for i in 0..3 {
        h.ed.add_page_in_background(Some(&format!("Empty page {i}")));
    }
    for name in ["Page 1", "Page 2", "Page 3", "Empty page 0", "Empty page 1"] {
        h.frame(vec![]);
        h.frame(vec![]);
        let before = h.ed.pages.len();
        h.click(h.text(name), PointerButton::Secondary);
        h.click(h.popup_text("Delete"), PointerButton::Primary);
        for _ in 0..20 {
            h.frame(vec![]);
        }
        h.click(h.popup_text("Delete"), PointerButton::Primary);
        assert_eq!(h.ed.pages.len(), before - 1, "failed to delete {name}");
    }
}

#[test]
fn confirmation_accepts_press_and_release_in_same_frame() {
    let mut h = Harness::new();
    h.request_page_deletion();
    let pos = h.popup_text("Delete").center();
    h.frame(vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        },
    ]);
    assert_eq!(h.ed.pages.len(), 2);
    assert_eq!(
        h.output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
        std::time::Duration::ZERO,
        "confirmed removal must redraw rows after mutation"
    );
}

#[test]
fn confirmation_click_after_idle_deletes_page() {
    let mut h = Harness::new();
    h.request_page_deletion();
    let pos = h.popup_text("Delete").center();
    h.frame(vec![Event::PointerMoved(pos)]);
    for _ in 0..40 {
        h.frame(vec![]);
    }
    h.frame(vec![Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }]);
    h.frame(vec![Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    assert_eq!(h.ed.pages.len(), 2);
    assert_eq!(
        h.output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
        std::time::Duration::ZERO,
        "confirmed removal must redraw rows after mutation"
    );
}

#[test]
fn bound_fill_inspector_visual() {
    use papercraft::model::{Color, Node, NodeId, NodeKind, Paint};
    let mut h = Harness::new();
    if !h.enable_capture() {
        return;
    }
    h.ctx.set_visuals(egui::Visuals::dark());
    h.ed.edit("Bound fill", |tx| {
        let mut node = Node::new(
            NodeId(1),
            NodeKind::Rect,
            papercraft::geom::Rect::new(0.0, 0.0, 200.0, 100.0),
        );
        node.style.fills = vec![Paint::solid_var(Color::BLACK, "surface-2")];
        tx.insert(node, None, None);
        tx.set_variable("surface-2", Color::BLACK);
    });
    h.ed.selection = vec![NodeId(1)];
    h.frame(vec![]);
    h.frame(vec![]);
    h.screenshot("papercraft-bound-fill.png");
}

#[test]
fn confirmed_empty_page_deletion_survives_project_reopen() {
    use papercraft::{
        project::{Project, Store},
        session::Session,
    };
    let root = std::env::temp_dir().join(format!(
        "papercraft-confirm-roundtrip-{:016x}",
        fastrand::u64(..)
    ));
    let store = Store::open(&root).unwrap();
    let mut session = Session::new(store.clone());
    let mut h = Harness::new();
    session
        .create(&mut h.ed, "Confirmation regression")
        .unwrap();
    for name in [
        "Design",
        "Plugins / Calendar",
        "Editor",
        "Page 11",
        "Knowledge",
        "Large vaults and code",
        "Mobile and sync",
        "Collaboration",
        "Home and vaults",
        "File viewers",
    ] {
        h.ed.add_page_in_background(Some(name));
    }
    h.ed.switch_page(9);
    h.st.native_chrome = true;
    session.save(&mut h.ed).unwrap();
    let removed = h.ed.pages[4].id.clone();
    let active = h.ed.active_page_id().to_owned();
    h.frame(vec![]);
    h.frame(vec![]);
    h.click(h.text("Page 11"), PointerButton::Secondary);
    h.click(h.popup_text("Delete"), PointerButton::Primary);
    h.click(h.popup_text("Delete"), PointerButton::Primary);
    assert_eq!(h.ed.pages.len(), 10);
    assert_eq!(h.ed.active_page_id(), active);
    session.save(&mut h.ed).unwrap();
    let dir = session.project.as_ref().unwrap().dir.clone();
    let saved = Project::open(&dir).unwrap();
    assert!(!saved.page_path(&removed).exists());
    let mut reopened = Editor::default();
    Session::new(store).open(&mut reopened, &dir).unwrap();
    assert_eq!(reopened.pages.len(), 10);
    assert!(reopened.pages.iter().all(|page| page.id != removed));
    assert_eq!(reopened.active_page_id(), active);
    std::fs::remove_dir_all(root).unwrap();
}
