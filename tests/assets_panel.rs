//! The Assets view draws with components on two pages and a library, and placing works.

use egui::{Pos2, Rect, Vec2};
use papercraft::editor::Editor;
use papercraft::geom::{Rect as R, v2};
use papercraft::model::{Document, Node, NodeId, NodeKind};
use papercraft::ui::{self, UiState, library};

fn comp(e: &mut Editor, name: &str) -> NodeId {
    let id = e.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: false }, R::new(0., 0., 60., 30.)), None);
    e.selection = vec![id];
    e.create_component();
    e.edit("n", |tx| tx.node_mut(id).unwrap().name = name.into());
    id
}

#[test]
fn assets_panel_draws_and_places() {
    let ctx = egui::Context::default();
    ui::setup_style(&ctx);
    let mut ed = Editor::default();
    let button = comp(&mut ed, "Button");
    ed.add_page_in_background(None);
    ed.with_page(1, |e| comp(e, "Toggle"));
    let mut doc = Document::default();
    doc.name = "Kit".into();
    let mut n = Node::new(NodeId(1), NodeKind::Frame { clip: false }, R::new(0., 0., 10., 10.));
    n.name = "Logo".into();
    n.component = true;
    n.component_key = Some("k".into());
    doc.nodes.insert(n.id, n);
    ed.library_docs.insert("kit".into(), doc);
    ed.design.libraries.push(papercraft::variables::EnabledLibrary { library_id: "kit".into(), version: 1 });
    let mut st = UiState::default();
    st.show_library = true;
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(344.0, 1440.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| library::panel(ui, &mut ed, &mut st));
        out.textures_delta.clear();
    }
    let list = library::list_assets(&ed, "");
    let toggle = list.pages[1].1[0].clone();
    let before = ed.doc.nodes.len();
    assert!(library::place(&mut ed, &toggle, v2(100., 100.)).is_some());
    assert!(library::place(&mut ed, &list.libraries[0].2[0], v2(0., 0.)).is_some());
    assert_eq!(ed.doc.nodes.len(), before + 2);
    assert!(ed.doc.get(button).is_some_and(|b| b.component));
}
