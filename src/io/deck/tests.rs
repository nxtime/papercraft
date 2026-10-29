use super::*;
use crate::editor::screens::Role;

fn named(ed: &mut Editor, name: &str, r: Rect, parent: Option<NodeId>) -> NodeId {
    let mut n = Node::new(NodeId(0), NodeKind::Frame { clip: true }, r);
    n.name = name.into();
    ed.add_node(n, parent)
}

#[test]
fn titles_drop_the_kind_and_states_follow_their_screen() {
    assert_eq!(clean("Screen / Sessions / Empty"), "Sessions \u{b7} Empty");
    assert_eq!(clean("Section"), "Section");
    let mut ed = Editor::default();
    let list = named(&mut ed, "Screen / Sessions / List", Rect::new(0.0, 0.0, 1440.0, 900.0), None);
    let home = named(&mut ed, "Screen / Home", Rect::new(2000.0, 0.0, 1440.0, 900.0), None);
    let states: Vec<NodeId> = ["Empty", "Loading", "Error", "Offline"]
        .iter()
        .enumerate()
        .map(|(k, st)| named(&mut ed, &format!("Screen / Sessions / {st}"), Rect::new(k as f64 * 2000.0, 1200.0, 1440.0, 900.0), None))
        .collect();
    let s = Section { name: "Admin".into(), doc: &ed.doc, screens: screens_of(&ed.doc) };
    let (slides, appendix) = section_slides(0, &s);
    let shape = |slides: &[Slide]| -> Vec<String> {
        slides
            .iter()
            .map(|sl| match sl {
                Slide::Screen { id, states, .. } => format!("screen {} +{states}", name_of(&ed.doc, *id)),
                Slide::Row(_, title, ids, _) => format!("row {title} {}", ids.len()),
                _ => "other".into(),
            })
            .collect()
    };
    assert_eq!(shape(&slides), ["screen Screen / Sessions / List +4", "screen Screen / Home +0"]);
    assert_eq!(shape(&appendix), ["row Sessions 4"], "the other states wait in the appendix");
    assert!(matches!(&appendix[0], Slide::Row(_, _, ids, _) if ids[0] == states[0]) && list != home);
}

#[test]
fn notes_on_a_screen_number_its_layers_and_continue_on_a_second_slide() {
    use crate::model::annotation::Annotation;
    let mut ed = Editor::default();
    let screen = named(&mut ed, "Screen / Sessions", Rect::new(0.0, 0.0, 1440.0, 900.0), None);
    let rows: Vec<NodeId> = (0..10).map(|k| named(&mut ed, "Row", Rect::new(0.0, 820.0 - k as f64 * 90.0, 1440.0, 80.0), Some(screen))).collect();
    let other = named(&mut ed, "Screen / Elsewhere", Rect::new(2000.0, 0.0, 1440.0, 900.0), None);
    for (k, row) in rows.iter().enumerate() {
        ed.doc.annotations.push(Annotation::new(Some(*row), &format!("Rule {k}"), "data"));
    }
    ed.doc.annotations.push(Annotation::new(Some(other), "Not here", "rule"));
    let mut gone = Annotation::new(Some(rows[0]), "Detached", "rule");
    gone.detached = true;
    ed.doc.annotations.push(gone);
    let notes = notes_of(&ed.doc, screen);
    assert_eq!(notes.len(), 10, "this screen's attached notes only");
    assert_eq!(notes[0].title, "Rule 9", "top to bottom");
    let slides = screen_slides(0, &ed.doc, screen, 0);
    assert_eq!(slides.len(), 2);
    assert!(matches!(&slides[1], Slide::Screen { notes, part: Some((2, 2)), .. } if notes[0].0 == 9));
    let pdf = build(&Meta { title: "T".into(), subtitle: None, author: None, now_ms: 0, language: None }, &[Section { name: "P".into(), doc: &ed.doc, screens: vec![screen] }]);
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 7"), "cover, contents, divider, 2 note slides, next steps, end");
}

#[test]
fn a_page_opens_with_a_map_and_the_deck_closes_on_its_questions() {
    use crate::model::annotation::Annotation;
    let mut ed = Editor::default();
    let a = named(&mut ed, "Screen / Home", Rect::new(0.0, 0.0, 1440.0, 900.0), None);
    let b = named(&mut ed, "Screen / Settings", Rect::new(2000.0, 0.0, 1440.0, 900.0), None);
    ed.doc.annotations.push(Annotation::new(Some(b), "Who can change the plan?", "rule"));
    let pdf = build(&Meta { title: "T".into(), subtitle: None, author: None, now_ms: 0, language: None }, &[Section { name: "P".into(), doc: &ed.doc, screens: vec![a, b] }]);
    // Cover, contents, divider, map, Home, Settings (noted), open questions, end.
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 8"));
}

#[test]
fn a_tall_card_is_cut_between_its_cells_not_through_them() {
    let mut ed = Editor::default();
    let card = named(&mut ed, "Group / App shell", Rect::new(0.0, 0.0, 1376.0, 2900.0), None);
    for row in 0..3 {
        for col in 0..5 {
            named(&mut ed, "Cell", Rect::new(col as f64 * 270.0, 20.0 + row as f64 * 960.0, 248.0, 924.0), Some(card));
        }
    }
    let parts = cut(&ed.doc, card, ed.doc.world_rect(card).unwrap());
    assert_eq!(parts.len(), 3, "a row of cells to a slide, shown a little smaller");
    assert!(parts.iter().all(|p| (0..3).any(|row| (p.bottom() - (944.0 + row as f64 * 960.0)).abs() < 40.0 || p.bottom() == 2900.0)), "{parts:?}");
}

#[test]
fn a_tall_board_is_shown_by_its_cards() {
    let mut ed = Editor::default();
    let board = named(&mut ed, "Section / Colors", Rect::new(0.0, 0.0, 1440.0, 2000.0), None);
    named(&mut ed, "Section Intro", Rect::new(32.0, 32.0, 1376.0, 40.0), Some(board));
    let groups = named(&mut ed, "Groups", Rect::new(32.0, 100.0, 1376.0, 1880.0), Some(board));
    for (k, h) in [300.0, 300.0, 1700.0].iter().enumerate() {
        named(&mut ed, &format!("Colors / Card {k}"), Rect::new(0.0, k as f64 * 340.0, 1376.0, *h), Some(groups));
    }
    ed.set_role(&[board], Role::Section).unwrap();
    let titles: Vec<String> = board_slides(0, &ed.doc, board)
        .iter()
        .map(|sl| match sl {
            Slide::Crop(_, _, title, _) => title.clone(),
            _ => "other".into(),
        })
        .collect();
    assert_eq!(titles[0], "Card 0 \u{b7} Card 1", "small cards share a slide, named without the board");
    assert!(titles[1..].iter().all(|t| t.starts_with("Card 2 (")), "{titles:?}");
}

#[test]
fn questions_drop_the_team_markers() {
    assert_eq!(ask("Days ? (team)"), "Days?");
    assert_eq!(ask("Other roles (Interviewer): ? (team)"), "Other roles (Interviewer)?");
    assert_eq!(ask("Editable? ? (team)"), "Editable?");
    assert_eq!(ask("Lock after 5 (? team: limit)"), "Lock after 5 (limit?)");
    assert_eq!(ask("Hiring panel only (? team)"), "Hiring panel only?");
}

#[test]
fn dates_read_as_days() {
    assert_eq!(Lang::En.words().date(0), "1 January 1970");
    assert_eq!(Lang::En.words().date(1_793_145_600_000), "28 October 2026");
    assert_eq!(Lang::Pt.words().date(1_793_145_600_000), "28 de outubro de 2026");
}

#[test]
fn the_deck_speaks_the_language_its_screens_are_written_in() {
    let mut ed = Editor::default();
    let screen = ed.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: true }, Rect::new(0.0, 0.0, 390.0, 844.0)), None);
    let text = |t: &str| Node::new(NodeId(0), NodeKind::Text { text: t.into(), font_size: 16.0, auto_width: true, align: Default::default() }, Rect::new(0.0, 0.0, 200.0, 20.0));
    ed.add_node(text("Distribuição de alimentos para a sua comunidade"), Some(screen));
    assert_eq!(Lang::detect([&ed.doc]), Lang::Pt);
    assert_eq!(Lang::detect([&Editor::default().doc]), Lang::En, "nothing to go by");
    // The deck's words are real text: the font's ToUnicode map lists what it drew.
    let drawn = |language| {
        let pdf = build(&Meta { title: "T".into(), subtitle: None, author: None, now_ms: 0, language }, &[Section { name: "P".into(), doc: &ed.doc, screens: vec![screen] }]);
        pdf.windows(7)
            .enumerate()
            .filter(|(_, w)| *w == b"stream\n")
            .filter_map(|(at, _)| miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[at + 7..]).ok())
            .map(|s| String::from_utf8_lossy(&s).into_owned())
            .collect::<String>()
    };
    // Capitals the screen does not use: the cover's "APRESENTAÇÃO" and "PRÄSENTATION".
    assert!(drawn(None).contains("<00C7>"), "the detected Portuguese");
    assert!(!drawn(Some(Lang::En)).contains("<00C7>"), "English when asked for");
    assert!(drawn(Some(Lang::De)).contains("<00C4>"), "German when asked for");
}

#[test]
fn deck_has_cover_contents_divider_screens_panels_and_end() {
    let mut ed = Editor::default();
    let mut frame = |r: Rect| ed.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: true }, r), None);
    let wide = frame(Rect::new(0.0, 0.0, 1440.0, 900.0));
    let panels: Vec<NodeId> = (0..6).map(|i| frame(Rect::new(2000.0 + i as f64 * 400.0, 0.0, 344.0, 1440.0))).collect();
    frame(Rect::new(0.0, 2000.0, 80.0, 40.0)); // a component part, not a screen
    let main = frame(Rect::new(0.0, 2500.0, 400.0, 400.0));
    let board = frame(Rect::new(0.0, 3000.0, 1440.0, 200.0));
    ed.doc.nodes.get_mut(&main).unwrap().component = true; // a component main, not a screen
    let screens = screens_of(&ed.doc);
    assert_eq!(screens.len(), 7);
    assert_eq!(screens[0], wide);
    ed.set_role(&[board], Role::Section).unwrap();
    assert_eq!(screens_of(&ed.doc).last(), Some(&board), "a section is shown whole, whatever its size");
    let long = ed.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: true }, Rect::new(0.0, 4000.0, 1440.0, 3000.0)), None);
    for k in 0..4 {
        let card = Node::new(NodeId(0), NodeKind::Frame { clip: true }, Rect::new(0.0, k as f64 * 720.0, 1440.0, 700.0));
        ed.add_node(card, Some(long));
    }
    let parts = tall_parts(&ed.doc, long).unwrap();
    let cuts: Vec<f64> = parts.iter().map(|p| p.bottom()).collect();
    assert_eq!(cuts, vec![4700.0, 5420.0, 6140.0, 7000.0], "cut in the gaps between cards, not through them");
    assert!(tall_parts(&ed.doc, wide).is_none() && tall_parts(&ed.doc, panels[0]).is_none());
    ed.set_role(&[wide], Role::Screen).unwrap();
    assert_eq!(screens_of(&ed.doc), vec![wide, board], "a section without screens is shown beside declared screens");
    ed.set_role(&[wide, board], Role::None).unwrap();
    let meta = Meta { title: "Deck".into(), subtitle: None, author: None, now_ms: 0, language: None };
    let pdf = build(&meta, &[Section { name: "Page".into(), doc: &ed.doc, screens }]);
    // Cover, contents, divider (no map for one key screen), the wide screen, 5 + 1 panels,
    // next steps, end.
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 8"), "{} panels", panels.len());
}
