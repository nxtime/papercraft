//! Note placement and leader routing on dense layouts: cards never sit on a node or on each
//! other, leader lines avoid other nodes, and Tidy is one undo step.

use papercraft::editor::Editor;
use papercraft::geom::{Rect, V2};
use papercraft::model::annotation::{Annotation, auto_place, card_world, frame_of};
use papercraft::model::annotation_layout::{card_size, obstacles, route, anchor_point, anchor_toward};
use papercraft::model::{Document, Node, NodeId, NodeKind};

fn add(e: &mut Editor, parent: Option<NodeId>, kind: NodeKind, r: Rect) -> NodeId {
    e.add_node(Node::new(NodeId(0), kind, r), parent)
}

fn frame(e: &mut Editor, r: Rect) -> NodeId {
    add(e, None, NodeKind::Frame { clip: true }, r)
}

const LONG_BODY: &str = "Only one Primary button per view.\nDanger is only for destructive actions and always asks to confirm.\nDisabled buttons keep their label and explain why on hover.\nLoading keeps the width.\nIcon-only buttons need an aria-label.\nMinimum touch target is 44 by 44.\nGroup related buttons with 8 px gaps.\nNever stack more than two.\nSecondary sits left of Primary.\nTertiary is a text link.";

fn note(e: &mut Editor, target: NodeId, label: &str, body: &str) {
    let mut a = Annotation::new(Some(target), label, "rule");
    a.body = body.into();
    auto_place(&e.doc, &mut a);
    e.add_annotation(a);
}

/// A 5x5 grid of buttons in one frame, three notes in different rows.
fn button_grid() -> Editor {
    let mut e = Editor::default();
    let f = frame(&mut e, Rect::new(0.0, 0.0, 760.0, 360.0));
    let mut btn = Vec::new();
    for row in 0..5 {
        for col in 0..5 {
            let r = Rect::new(40.0 + col as f64 * 140.0, 40.0 + row as f64 * 60.0, 120.0, 40.0);
            btn.push(add(&mut e, Some(f), NodeKind::Rect, r));
        }
    }
    note(&mut e, btn[7], "One Primary button per view, and it is always the rightmost", LONG_BODY);
    note(&mut e, btn[13], "Danger only for destructive actions", "Always asks to confirm.\nNever the default.");
    note(&mut e, btn[21], "Disabled state", "Keeps its label.");
    e
}

/// Design System page: a column of group frames on the left, a dense grid of loose components
/// to the right of it.
fn design_system() -> Editor {
    let mut e = Editor::default();
    let mut targets = Vec::new();
    for g in 0..4 {
        let f = frame(&mut e, Rect::new(0.0, g as f64 * 320.0, 300.0, 280.0));
        for i in 0..4 {
            let r = Rect::new(20.0 + (i % 2) as f64 * 140.0, 20.0 + (i / 2) as f64 * 70.0, 120.0, 50.0);
            targets.push(add(&mut e, Some(f), NodeKind::Rect, r));
        }
    }
    for row in 0..8 {
        for col in 0..6 {
            add(&mut e, None, NodeKind::Rect, Rect::new(360.0 + col as f64 * 150.0, row as f64 * 100.0, 130.0, 80.0));
        }
    }
    for (i, t) in [0, 3, 5, 9, 10, 14].into_iter().enumerate() {
        note(&mut e, targets[t], &format!("Component rule number {i}: keep this exact"), "Body line one\nBody line two that is a little longer than the first\nthree");
    }
    e
}

fn card_rect(doc: &Document, a: &Annotation) -> Rect {
    let (at, s) = (card_world(doc, a), card_size(a));
    Rect::new(at.x, at.y, s.x, s.y)
}

fn strictly_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

fn path_hits(path: &[V2], r: &Rect) -> bool {
    path.windows(2).any(|w| strictly_overlap(&Rect::from_points(w[0], w[1]).inflate(0.5), r))
}

fn check_layout(e: &Editor, name: &str) {
    let doc = &e.doc;
    let cards: Vec<Rect> = doc.annotations.iter().map(|a| card_rect(doc, a)).collect();
    for (i, a) in doc.annotations.iter().enumerate() {
        let t = doc.world_rect(a.target.unwrap()).unwrap();
        let mut obs = obstacles(doc, a.target);
        assert!(!strictly_overlap(&cards[i], &t), "{name}: card {i} on its target");
        for o in &obs {
            assert!(!strictly_overlap(&cards[i], o), "{name}: card {i} on a node {o:?}");
        }
        for (j, c) in cards.iter().enumerate().filter(|(j, _)| *j != i) {
            assert!(!strictly_overlap(&cards[i], c), "{name}: cards {i} and {j} overlap");
            obs.push(*c);
        }
        let end = anchor_point(t, anchor_toward(t, cards[i]));
        let path = route(cards[i], t, end, &obs);
        for o in &obs {
            assert!(!path_hits(&path, o), "{name}: line {i} crosses {o:?}");
        }
    }
}

#[test]
fn grid_of_25_buttons_with_3_notes_keeps_clear() {
    let e = button_grid();
    check_layout(&e, "grid");
}

#[test]
fn design_system_layout_keeps_clear() {
    let e = design_system();
    check_layout(&e, "design-system");
}

#[test]
fn tidy_is_one_undo_step_and_fixes_a_mess() {
    let mut e = design_system();
    // Pile every card onto its target.
    let ids: Vec<u64> = e.doc.annotations.iter().map(|a| a.id).collect();
    for id in &ids {
        e.update_annotation(*id, |a| a.position = V2 { x: 5.0, y: 5.0 });
    }
    let messy = e.doc.clone();
    assert_eq!(e.tidy_annotations(None), ids.len());
    check_layout(&e, "tidied");
    e.undo();
    assert_eq!(e.doc, messy, "one undo step restores the mess");
    e.redo();
    check_layout(&e, "redone");
}

/// A section frame packed with screens 32 apart, notes on small nodes in the screens' middles:
/// no card may sit on any screen's background (the Interview Candidate page).
#[test]
fn notes_on_packed_screens_stay_off_every_screen() {
    let mut e = Editor::default();
    let section = frame(&mut e, Rect::new(0.0, 0.0, 3.0 * 1472.0 + 32.0, 2.0 * 932.0 + 32.0));
    let mut screens = Vec::new();
    for row in 0..2 {
        for col in 0..3 {
            let r = Rect::new(32.0 + col as f64 * 1472.0, 32.0 + row as f64 * 932.0, 1440.0, 900.0);
            let s = add(&mut e, Some(section), NodeKind::Frame { clip: true }, r);
            let t = add(&mut e, Some(s), NodeKind::Rect, Rect::new(600.0, 400.0, 240.0, 48.0));
            note(&mut e, t, "Invite link works only while the invite is valid", "Applies when: candidate opens the link\nEnforced by: server");
            screens.push(s);
        }
    }
    e.tidy_annotations(Some(section));
    let doc = &e.doc;
    for (i, a) in doc.annotations.iter().enumerate() {
        let c = card_rect(doc, a);
        for s in &screens {
            assert!(!strictly_overlap(&c, &doc.world_rect(*s).unwrap()), "card {i} sits on a screen");
        }
    }
    check_layout(&e, "packed-screens");
}

/// A line into the target's own screen goes straight past the screen's header and panels
/// instead of detouring around them.
#[test]
fn line_into_its_screen_takes_no_detour() {
    let mut e = Editor::default();
    let section = frame(&mut e, Rect::new(0.0, 0.0, 1504.0, 964.0));
    let s = add(&mut e, Some(section), NodeKind::Frame { clip: true }, Rect::new(32.0, 32.0, 1440.0, 900.0));
    add(&mut e, Some(s), NodeKind::Rect, Rect::new(0.0, 0.0, 1440.0, 120.0));
    add(&mut e, Some(s), NodeKind::Rect, Rect::new(0.0, 120.0, 240.0, 780.0));
    let t = add(&mut e, Some(s), NodeKind::Rect, Rect::new(600.0, 400.0, 240.0, 48.0));
    note(&mut e, t, "Unknown session id shows 'Session not found'", "Trigger: bad id\nResult: error card");
    let doc = &e.doc;
    let a = &doc.annotations[0];
    let (card, tr) = (card_rect(doc, a), doc.world_rect(t).unwrap());
    let end = anchor_point(tr, anchor_toward(tr, card));
    let path = route(card, tr, end, &papercraft::model::annotation_layout::route_obstacles(doc, t));
    let len: f64 = path.windows(2).map(|w| (w[1].x - w[0].x).abs() + (w[1].y - w[0].y).abs()).sum();
    let direct = (path[0].x - end.x).abs() + (path[0].y - end.y).abs();
    assert!(len <= direct + 1.0, "detour: {len} vs {direct}, {path:?}");
}

/// Writes a schematic PNG (nodes, cards, routes) when PAPERCRAFT_LAYOUT_PNG names a directory.
#[test]
fn schematic_png() {
    let Ok(dir) = std::env::var("PAPERCRAFT_LAYOUT_PNG") else { return };
    for (name, e, before) in [
        ("grid", button_grid(), false),
        ("design-system", design_system(), false),
        ("grid-before", button_grid(), true),
        ("design-system-before", design_system(), true),
    ] {
        let doc = &e.doc;
        let mut cards: Vec<Rect> = doc.annotations.iter().map(|a| card_rect(doc, a)).collect();
        if before {
            // The old placement: a fixed 220x72 card right of the top-level frame, level with the
            // target, pushed down past earlier cards.
            cards.clear();
            for a in &doc.annotations {
                let t = doc.world_rect(a.target.unwrap()).unwrap();
                let fr = doc.world_rect(frame_of(doc, a.target.unwrap()).unwrap()).unwrap();
                let mut c = Rect::new(fr.right() + 24.0, t.y, 220.0, 72.0);
                while cards.iter().any(|o| strictly_overlap(&c, o)) {
                    c.y += 84.0;
                }
                cards.push(c);
            }
        }
        let all: Vec<Rect> = doc.nodes.keys().filter_map(|id| doc.world_rect(*id)).chain(cards.iter().copied()).collect();
        let b = Rect::union_all(all.iter()).unwrap().inflate(20.0);
        let k = 0.8;
        let mut img = image::RgbaImage::from_pixel((b.w * k) as u32, (b.h * k) as u32, image::Rgba([250, 250, 250, 255]));
        let mut fill = |r: &Rect, c: [u8; 4], solid: bool| {
            let (x0, y0, x1, y1) = (((r.x - b.x) * k) as i64, ((r.y - b.y) * k) as i64, ((r.right() - b.x) * k) as i64, ((r.bottom() - b.y) * k) as i64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let edge = x == x0 || y == y0 || x == x1 - 1 || y == y1 - 1;
                    if (solid || edge) && x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
                        img.put_pixel(x as u32, y as u32, image::Rgba(c));
                    }
                }
            }
        };
        for id in doc.paint_order() {
            let leaf = doc.get(id).unwrap().children.is_empty();
            fill(&doc.world_rect(id).unwrap(), if leaf { [190, 200, 215, 255] } else { [120, 120, 120, 255] }, leaf);
        }
        for c in &cards {
            fill(c, [255, 224, 150, 255], true);
            fill(c, [200, 120, 0, 255], false);
        }
        for (i, a) in doc.annotations.iter().enumerate() {
            let t = doc.world_rect(a.target.unwrap()).unwrap();
            fill(&t, [124, 92, 255, 255], false);
            let mut obs = obstacles(doc, a.target);
            obs.extend(cards.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, c)| *c));
            let end = anchor_point(t, anchor_toward(t, cards[i]));
            let path = if before { papercraft::model::annotation_layout::elbow(cards[i], end) } else { route(cards[i], t, end, &obs) };
            for w in path.windows(2) {
                fill(&Rect::from_points(w[0], w[1]).inflate(1.0), [124, 92, 255, 255], true);
            }
        }
        img.save(std::path::Path::new(&dir).join(format!("{name}.png"))).unwrap();
    }
}

/// Diagnostics on a real page file: PAPERCRAFT_LAYOUT_PAGE=<page.json> PAPERCRAFT_LAYOUT_FRAME=<node number>.
/// Tidies the frame's notes and prints each note's leader length and bends.
#[test]
fn real_page_report() {
    let (Ok(path), Ok(frame)) = (std::env::var("PAPERCRAFT_LAYOUT_PAGE"), std::env::var("PAPERCRAFT_LAYOUT_FRAME")) else { return };
    let page = papercraft::project::format::page_from_json(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut doc = page.doc;
    let frame = NodeId(frame.parse().unwrap());
    for (id, p) in papercraft::model::annotation_layout::tidy(&doc, Some(frame)) {
        let a = doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
        (a.position, a.anchor) = (p.position, p.anchor);
    }
    let cards: Vec<(u64, Rect)> = doc.annotations.iter().map(|a| (a.id, card_rect(&doc, a))).collect();
    for a in doc.annotations.iter().filter(|a| a.target.and_then(|t| frame_of(&doc, t)) == Some(frame)) {
        let t = doc.world_rect(a.target.unwrap()).unwrap();
        let c = card_rect(&doc, a);
        let mut obs = papercraft::model::annotation_layout::route_obstacles(&doc, a.target.unwrap());
        obs.extend(cards.iter().filter(|(i, _)| *i != a.id).map(|(_, r)| *r));
        let end = anchor_point(t, anchor_toward(t, c));
        let path = route(c, t, end, &obs);
        let len: f64 = path.windows(2).map(|w| (w[1].x - w[0].x).abs() + (w[1].y - w[0].y).abs()).sum();
        println!("{:>6} len {:>6.0} bends {} card ({:.0},{:.0}) target ({:.0},{:.0}) {}", a.id, len, path.len().saturating_sub(2), c.x, c.y, t.x, t.y, a.label);
    }
}
