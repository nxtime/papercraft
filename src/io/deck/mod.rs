//! Presentation export: a PDF deck of a project's screens, the way a designer sends work to a
//! client. A cover, contents, a divider per page, one slide per screen (narrow side panels share
//! one), a closing slide; every slide numbered. Slides are drawn from a generated document
//! (text and cards); the screens are drawn into them scaled, so they stay vector.
//! Spec: `specs/presentation-export/spec.md`.

use super::pdf::{Insert, PdfPage};
use crate::editor::Editor;
use crate::geom::{Rect, V2, v2};
use crate::model::{Color, Document, Node, NodeId, NodeKind, TextAlign};

/// A page of the project in the deck.
pub struct Section<'a> {
    pub name: String,
    pub doc: &'a Document,
    pub screens: Vec<NodeId>,
}

/// What the cover says.
pub struct Meta {
    pub title: String,
    pub subtitle: Option<String>,
    pub author: Option<String>,
    /// Unix milliseconds, for the date on the cover.
    pub now_ms: u64,
}

const W: f64 = 1920.0;
const H: f64 = 1080.0;
const MARGIN: f64 = 96.0;
/// Slides are laid out in the generated document one under the other with this gap.
const SLIDE_GAP: f64 = 200.0;
/// Frames smaller than this on either side are parts (components), not screens.
const MIN_SCREEN: f64 = 300.0;
/// Screens narrower than this share of their height are side panels, shown side by side.
const NARROW: f64 = 0.45;
const PANELS_PER_SLIDE: usize = 5;
const PANEL_GAP: f64 = 48.0;
/// A board fitted smaller than this scale is split top to bottom over several slides, each
/// part at the stage's width (at most 1:1). Narrower boards are panels and stay whole.
const READABLE: f64 = 0.5;
const MIN_SPLIT_W: f64 = 600.0;
/// Where a screen goes on its slide (under the title, above the footer).
const STAGE: Rect = Rect { x: MARGIN, y: 196.0, w: W - 2.0 * MARGIN, h: H - 196.0 - 110.0 };
const CAPTION_H: f64 = 44.0;

const BG: Color = Color { r: 22, g: 22, b: 30, a: 255 };
/// A screen's frame: light enough to edge a dark screen on the dark slide.
const CARD_EDGE: Color = Color { r: 84, g: 84, b: 104, a: 255 };
const CARD_EDGE_W: f64 = 2.0;
const INK: Color = Color { r: 232, g: 232, b: 242, a: 255 };
const MUTED: Color = Color { r: 150, g: 150, b: 170, a: 255 };
const ACCENT: Color = Color { r: 180, g: 160, b: 250, a: 255 };

const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// The screens of a page in reading order (top to bottom, then left to right): its declared
/// screens, else its visible top-level frames (not component mains) of at least [`MIN_SCREEN`]
/// on both sides. A
/// section holding no screen (a design-system board) is shown whole.
pub fn screens_of(doc: &Document) -> Vec<NodeId> {
    let declared = crate::editor::screens::declared(doc);
    let mut ids: Vec<NodeId> = if declared.is_empty() {
        doc.root
            .iter()
            .copied()
            .filter(|id| doc.get(*id).is_some_and(|n| n.is_frame() && n.visible && !n.section && !n.component && n.rect.w >= MIN_SCREEN && n.rect.h >= MIN_SCREEN))
            .collect()
    } else {
        declared.iter().copied().filter(|id| doc.get(*id).is_some_and(|n| n.visible)).collect()
    };
    let holds_screen = |id: NodeId| declared.iter().any(|s| doc.ancestors(*s).contains(&id));
    ids.extend(doc.root.iter().copied().filter(|id| doc.get(*id).is_some_and(|n| n.section && n.visible) && !holds_screen(*id)));
    let at = |id: &NodeId| doc.world_rect(*id).map_or((0.0, 0.0), |r| (r.y, r.x));
    ids.sort_by(|a, b| at(a).partial_cmp(&at(b)).unwrap_or(std::cmp::Ordering::Equal));
    ids
}

enum Slide {
    Cover,
    Contents,
    Divider(usize),
    /// One screen, whole, with its notes (numbered from 1 across its slides), which of its note
    /// slides this is (`k` of `n`), and how many more states of it the appendix shows.
    Screen { i: usize, id: NodeId, notes: Vec<(usize, Note)>, part: Option<(usize, usize)>, states: usize },
    /// The appendix's divider: the states of screens, after the last page.
    Appendix,
    /// A page's key screens as captioned thumbnails, after its divider.
    Map(usize, Vec<NodeId>),
    /// A page's notes that ask a question, with their screen's name.
    Questions(usize, Vec<(String, String)>),
    /// What is left to do: the open questions, the review, the appendix.
    NextSteps,
    /// Screens side by side with captions, `n` to a slide: side panels, or the other states
    /// of a screen.
    Row(usize, String, Vec<NodeId>, usize),
    /// An area of a layer: a board's cards, or a part of a tall screen. Title, source rect.
    Crop(usize, NodeId, String, Rect),
    End,
}

/// Layer-name prefixes that say what a layer is, not what it shows; left out of titles.
const KIND_PREFIXES: [&str; 4] = ["Section", "Screen", "Group", "Slot"];
/// States in the appendix (a reference): a 3 × 2 grid to a slide.
const APPENDIX_PER_SLIDE: usize = 6;
const APPENDIX_COLS: usize = 3;
/// Characters kept of an appendix slide's title (the groups on it).
const APPENDIX_TITLE_CHARS: usize = 70;
/// Cards pack onto a slide down to this share of their part scale.
const PACK_MIN_SCALE: f64 = 0.8;
/// A part of a tall card shrinks down to this share of its scale to keep blocks (a row of
/// cells) whole rather than cut through them.
const CUT_MIN_SCALE: f64 = 0.7;
/// Room around a board's cards on their slide.
const CARD_PAD: f64 = 24.0;
/// Notes shown beside a screen per slide; more continue on a repeat of the slide.
const NOTES_PER_SLIDE: usize = 8;
/// The share of the stage an annotated screen takes; its notes fill the rest.
const NOTED_SCREEN: f64 = 0.62;
const MARKER: f64 = 28.0;
/// Characters kept of a note's title and of its first body line.
const NOTE_TITLE_CHARS: usize = 80;
const NOTE_BODY_CHARS: usize = 120;
/// About how many title characters fit on one line of the note column.
const NOTE_TITLE_LINE_CHARS: usize = 50;

/// Thumbnails on a map slide (four across, three down).
const MAP_COLS: usize = 4;
const MAP_ROWS: usize = 3;
/// A map for fewer key screens than this adds nothing: the slides follow right after it.
const MAP_MIN: usize = 3;
const QUESTIONS_PER_SLIDE: usize = 8;

/// "12 screens · 6 boards": a page's screens and boards (sections shown by their cards).
fn tally(s: &Section) -> String {
    let boards = s.screens.iter().filter(|id| s.doc.get(**id).is_some_and(|n| n.section)).count();
    let screens = count(s.screens.len() - boards, "screen");
    if boards == 0 { screens } else { format!("{screens} \u{b7} {}", count(boards, "board")) }
}

/// A design note shown on a slide.
#[derive(Clone)]
struct Note {
    target: NodeId,
    title: String,
    body: String,
    /// The line of the note that asks something (`?`), if any.
    question: Option<String>,
    color: Color,
}

/// The notes on screen `id`: attached notes whose layer is the screen, inside it or laid over
/// it, in reading order of their layers. Detached notes (their layer is gone) are left out.
fn notes_of(doc: &Document, id: NodeId) -> Vec<Note> {
    let roots = super::export::with_layers_on_top(doc, &[id]);
    let cats = crate::model::annotation::default_categories();
    let mut notes: Vec<(Rect, Note)> = doc
        .annotations
        .iter()
        .filter(|a| !a.detached)
        .filter_map(|a| {
            let target = a.target?;
            if !(roots.contains(&target) || doc.ancestors(target).iter().any(|p| roots.contains(p))) {
                return None;
            }
            let color = crate::model::annotation::category_by_key(&cats, &a.category).map_or(ACCENT, |c| c.color);
            let body = a.body.lines().next().unwrap_or("").to_owned();
            let question = std::iter::once(a.label.as_str()).chain(a.body.lines()).find(|l| l.contains('?')).map(str::to_owned);
            Some((doc.world_rect(target)?, Note { target, title: a.label.clone(), body, question, color }))
        })
        .collect();
    notes.sort_by(|a, b| (a.0.y, a.0.x).partial_cmp(&(b.0.y, b.0.x)).unwrap_or(std::cmp::Ordering::Equal));
    notes.into_iter().map(|(_, n)| n).collect()
}

/// The slides of one screen: one, or one per [`NOTES_PER_SLIDE`] of its notes.
fn screen_slides(i: usize, doc: &Document, id: NodeId, states: usize) -> Vec<Slide> {
    let notes: Vec<(usize, Note)> = notes_of(doc, id).into_iter().enumerate().map(|(k, n)| (k + 1, n)).collect();
    if notes.is_empty() {
        return vec![Slide::Screen { i, id, notes, part: None, states }];
    }
    let chunks: Vec<&[(usize, Note)]> = notes.chunks(NOTES_PER_SLIDE).collect();
    let n = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(k, c)| Slide::Screen { i, id, notes: c.to_vec(), part: (n > 1).then_some((k + 1, n)), states })
        .collect()
}

/// `s` cut to `n` characters, with an ellipsis when cut.
fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_owned();
    }
    format!("{}\u{2026}", s.chars().take(n - 1).collect::<String>().trim_end())
}

/// A layer name's parts without its kind prefix: `Screen / Sessions / Empty` is
/// `[Sessions, Empty]`.
fn name_parts(name: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = name.split(" / ").collect();
    if parts.len() > 1 && KIND_PREFIXES.contains(&parts[0]) {
        parts.remove(0);
    }
    parts
}

/// A layer name as a slide title: `Screen / Sessions / Empty` reads `Sessions · Empty`.
fn clean(name: &str) -> String {
    name_parts(name).join(" \u{b7} ")
}

fn name_of(doc: &Document, id: NodeId) -> &str {
    doc.get(id).map_or("", |n| n.name.as_str())
}

/// The slides of one page in reading order, and its slides for the appendix. A screen whose
/// name has states (`Sessions / List`, `Sessions / Empty`) is shown once, its other states in
/// the appendix; narrow panels share rows; boards are shown by their cards; tall screens in parts.
fn section_slides(i: usize, s: &Section) -> (Vec<Slide>, Vec<Slide>) {
    enum Item {
        Done(Slide),
        States(usize),
    }
    let doc = s.doc;
    let mut items = Vec::new();
    let mut groups: Vec<(String, Vec<NodeId>)> = Vec::new();
    let mut panels = Vec::new();
    let flush = |panels: &mut Vec<NodeId>, items: &mut Vec<Item>| {
        if panels.is_empty() {
            return;
        }
        let mut heads: Vec<&str> = Vec::new();
        for id in panels.iter() {
            let head = name_parts(name_of(doc, *id))[0];
            if !heads.contains(&head) {
                heads.push(head);
            }
        }
        items.push(Item::Done(Slide::Row(i, heads.join(" \u{b7} "), std::mem::take(panels), PANELS_PER_SLIDE)));
    };
    for id in &s.screens {
        let Some(n) = doc.get(*id) else { continue };
        if n.section {
            flush(&mut panels, &mut items);
            items.extend(board_slides(i, doc, *id).into_iter().map(Item::Done));
            continue;
        }
        if let Some(parts) = tall_parts(doc, *id) {
            flush(&mut panels, &mut items);
            let k = parts.len();
            let title = clean(&n.name);
            items.extend(parts.into_iter().enumerate().map(|(j, from)| Item::Done(Slide::Crop(i, *id, format!("{title} ({}/{k})", j + 1), from))));
            continue;
        }
        if n.rect.w < n.rect.h * NARROW {
            panels.push(*id);
            if panels.len() == PANELS_PER_SLIDE {
                flush(&mut panels, &mut items);
            }
            continue;
        }
        flush(&mut panels, &mut items);
        let parts = name_parts(&n.name);
        // `Sessions / Empty` is a state of `Sessions`; a name without parts
        // (`Challenges List Archived`) is grouped by its first two words.
        let words: Vec<&str> = parts[0].split_whitespace().collect();
        let base = if parts.len() > 1 {
            parts[..parts.len() - 1].join(" \u{b7} ")
        } else if words.len() > 1 {
            words[..2].join(" ")
        } else {
            items.extend(screen_slides(i, doc, *id, 0).into_iter().map(Item::Done));
            continue;
        };
        match groups.iter().position(|(b, _)| *b == base) {
            Some(g) => groups[g].1.push(*id),
            None => {
                items.push(Item::States(groups.len()));
                groups.push((base, vec![*id]));
            }
        }
    }
    flush(&mut panels, &mut items);
    let mut slides = Vec::new();
    let mut states: Vec<(&str, NodeId)> = Vec::new();
    for item in items {
        match item {
            Item::Done(slide) => slides.push(slide),
            Item::States(g) => {
                let (base, ids) = &groups[g];
                slides.extend(screen_slides(i, doc, ids[0], ids.len() - 1));
                states.extend(ids[1..].iter().map(|id| (base.as_str(), *id)));
            }
        }
    }
    // The page's other states fill the appendix's grids in order, groups sharing slides.
    let appendix = states
        .chunks(APPENDIX_PER_SLIDE)
        .map(|c| {
            let mut bases: Vec<&str> = Vec::new();
            for (b, _) in c {
                if !bases.contains(b) {
                    bases.push(b);
                }
            }
            let title = if bases.len() == 1 { bases[0].to_owned() } else { clip(&bases.join(", "), APPENDIX_TITLE_CHARS) };
            Slide::Row(i, title, c.iter().map(|(_, id)| *id).collect(), APPENDIX_PER_SLIDE)
        })
        .collect();
    (slides, appendix)
}

/// A board (a section without screens) on slides. One that fits at 1:1 is shown whole;
/// otherwise its cards (the children of its widest card container) are packed in order onto
/// slides at 1:1, titled by their names. A card too tall for a slide is cut at gaps.
fn board_slides(i: usize, doc: &Document, id: NodeId) -> Vec<Slide> {
    let Some(b) = crate::io::svg::export_bounds(doc, &[id]) else { return Vec::new() };
    if fits(b) {
        return screen_slides(i, doc, id, 0);
    }
    let board = clean(name_of(doc, id));
    let parts_of = |title: &str, r: Rect| {
        let parts = cut(doc, id, r);
        let k = parts.len();
        parts.into_iter().enumerate().map(move |(j, from)| Slide::Crop(i, id, format!("{title} ({}/{k})", j + 1), from)).collect::<Vec<_>>()
    };
    let Some(cards) = cards_of(doc, id) else { return parts_of(&board, b) };
    let mut slides = Vec::new();
    let mut pack: Option<(Rect, Vec<String>)> = None;
    for c in cards {
        let Some(r) = doc.world_rect(c) else { continue };
        let r = Rect::new(r.x - CARD_PAD, r.y - CARD_PAD, r.w + 2.0 * CARD_PAD, r.h + 2.0 * CARD_PAD);
        let title = clean(name_of(doc, c));
        let label = title.strip_prefix(&format!("{board} \u{b7} ")).map_or(title.clone(), str::to_owned);
        if let Some((u, mut names)) = pack.take() {
            if fits(u.union(&r)) {
                names.push(label);
                pack = Some((u.union(&r), names));
                continue;
            }
            slides.push(Slide::Crop(i, id, names.join(" \u{b7} "), u));
        }
        if fits(r) {
            pack = Some((r, vec![label]));
        } else if (STAGE.w / r.w).min(STAGE.h / r.h) >= READABLE {
            slides.push(Slide::Crop(i, id, label, r));
        } else {
            slides.extend(parts_of(&label, r));
        }
    }
    if let Some((u, names)) = pack {
        slides.push(Slide::Crop(i, id, names.join(" \u{b7} "), u));
    }
    slides
}

/// Whether `r` fits on a slide at the scale parts are shown at.
fn fits(r: Rect) -> bool {
    r.h * part_scale(r) * PACK_MIN_SCALE <= STAGE.h
}

/// A board's cards: the children of whichever of the board and its child frames holds the
/// most frames (at least two). Text and single wrappers (a title, an intro) are not cards.
fn cards_of(doc: &Document, board: NodeId) -> Option<Vec<NodeId>> {
    let frames = |id: NodeId| -> Vec<NodeId> {
        doc.get(id).map_or_else(Vec::new, |n| n.children.iter().copied().filter(|c| doc.get(*c).is_some_and(|c| c.visible && c.is_frame())).collect())
    };
    std::iter::once(board).chain(frames(board)).map(frames).filter(|f| f.len() >= 2).max_by_key(Vec::len)
}

/// The deck as PDF bytes. Sections without screens are left out.
pub fn build(meta: &Meta, sections: &[Section]) -> Vec<u8> {
    let sections: Vec<&Section> = sections.iter().filter(|s| !s.screens.is_empty()).collect();
    let mut slides = vec![Slide::Cover, Slide::Contents];
    let mut starts = Vec::new();
    let mut appendix = Vec::new();
    let mut questions = Vec::new();
    for (i, s) in sections.iter().enumerate() {
        starts.push(slides.len() + 1);
        slides.push(Slide::Divider(i));
        let (main, extra) = section_slides(i, s);
        // The big picture first: the page's screens (one per state group) at a glance.
        let mut key: Vec<NodeId> = Vec::new();
        for slide in &main {
            if let Slide::Screen { id, .. } = slide
                && !key.contains(id)
                && s.doc.get(*id).is_some_and(|n| !n.section)
            {
                key.push(*id);
            }
        }
        if key.len() >= MAP_MIN {
            // Even maps: 17 screens are 9 and 8, not 12 and 5.
            let per = key.len().div_ceil(key.len().div_ceil(MAP_COLS * MAP_ROWS));
            slides.extend(key.chunks(per).map(|c| Slide::Map(i, c.to_vec())));
        }
        let mut asked = Vec::new();
        for id in s.screens.iter().filter(|id| s.doc.get(**id).is_some_and(|n| !n.section)) {
            for n in notes_of(s.doc, *id) {
                if let Some(line) = n.question {
                    let q = if line == n.title { ask(&line) } else { format!("{}: {}", n.title, ask(&line)) };
                    asked.push((clean(name_of(s.doc, *id)), q));
                }
            }
        }
        questions.extend(asked.chunks(QUESTIONS_PER_SLIDE).map(|c| Slide::Questions(i, c.to_vec())));
        slides.extend(main);
        appendix.extend(extra);
    }
    let questions_start = (!questions.is_empty()).then(|| slides.len() + 1);
    let asked: usize = questions.iter().map(|q| if let Slide::Questions(_, qs) = q { qs.len() } else { 0 }).sum();
    slides.extend(questions);
    slides.push(Slide::NextSteps);
    let appendix_start = (!appendix.is_empty()).then(|| slides.len() + 1);
    if appendix_start.is_some() {
        slides.push(Slide::Appendix);
        slides.extend(appendix);
    }
    slides.push(Slide::End);

    let total = slides.len();
    let screens: usize = sections.iter().map(|s| s.screens.iter().filter(|id| s.doc.get(**id).is_some_and(|n| !n.section)).count()).sum();
    let mut ed = Editor::default();
    // Each slide, and its overlay (drawn above the screens: note markers).
    let mut pages: Vec<(Rect, NodeId, Option<NodeId>, Vec<Insert>)> = Vec::new();
    for (k, slide) in slides.iter().enumerate() {
        let r = Rect::new(0.0, k as f64 * (H + SLIDE_GAP), W, H);
        let mut frame = Node::new(NodeId(0), NodeKind::Frame { clip: true }, r);
        frame.style.set_fill(Some(BG));
        let f = ed.add_node(frame, None);
        let mut inserts = Vec::new();
        let mut overlay = None;
        match slide {
            Slide::Cover => {
                t(&mut ed, f, "DESIGN PRESENTATION", 22.0, true, ACCENT, v2(MARGIN, 330.0));
                text(&mut ed, f, &meta.title, 88.0, true, INK, v2(MARGIN, 372.0), Some((800.0, TextAlign::Left)));
                let sub = meta.subtitle.clone().unwrap_or_else(|| format!("{} \u{b7} {}", count(screens, "screen"), count(sections.len(), "page")));
                text(&mut ed, f, &sub, 30.0, false, MUTED, v2(MARGIN, 520.0), Some((800.0, TextAlign::Left)));
                let by = match &meta.author {
                    Some(a) => format!("{} \u{b7} {a}", date(meta.now_ms)),
                    None => date(meta.now_ms),
                };
                text(&mut ed, f, &by, 22.0, false, MUTED, v2(MARGIN, H - MARGIN - 30.0), None);
                // The hero: the largest regular screen of the deck (the first of equals).
                let hero = sections
                    .iter()
                    .flat_map(|s| s.screens.iter().filter_map(|id| Some((s.doc, *id, s.doc.world_rect(*id)?))))
                    .filter(|(doc, id, b)| b.w >= b.h * NARROW && doc.get(*id).is_some_and(|n| !n.section))
                    .fold(None::<(&Document, NodeId, Rect)>, |best, c| match best {
                        Some(b) if b.2.w * b.2.h >= c.2.w * c.2.h => Some(b),
                        _ => Some(c),
                    })
                    .map(|(doc, id, _)| (doc, id));
                if let Some((doc, id)) = hero {
                    inserts.extend(place(&mut ed, f, doc, id, None, Rect::new(960.0, 140.0, W - 960.0 - MARGIN + 40.0, H - 280.0), r));
                }
            }
            Slide::Contents => {
                t(&mut ed, f, "Contents", 56.0, true, INK, v2(MARGIN, MARGIN));
                for (i, s) in sections.iter().enumerate() {
                    let y = 230.0 + i as f64 * 64.0;
                    t(&mut ed, f, &format!("{:02}", i + 1), 28.0, true, ACCENT, v2(MARGIN, y));
                    t(&mut ed, f, &s.name, 28.0, false, INK, v2(MARGIN + 84.0, y));
                    t(&mut ed, f, &tally(s), 22.0, false, MUTED, v2(1200.0, y + 4.0));
                    text(&mut ed, f, &starts[i].to_string(), 22.0, false, MUTED, v2(W - MARGIN - 200.0, y + 4.0), Some((200.0, TextAlign::Right)));
                }
                let extras = [("?", "Open questions", questions_start), ("A", "Appendix \u{b7} states", appendix_start)];
                for (j, (mark, name, start)) in extras.iter().filter_map(|(m, n, s)| Some((*m, *n, (*s)?))).enumerate() {
                    let y = 230.0 + (sections.len() + j) as f64 * 64.0;
                    t(&mut ed, f, mark, 28.0, true, ACCENT, v2(MARGIN, y));
                    t(&mut ed, f, name, 28.0, false, INK, v2(MARGIN + 84.0, y));
                    text(&mut ed, f, &start.to_string(), 22.0, false, MUTED, v2(W - MARGIN - 200.0, y + 4.0), Some((200.0, TextAlign::Right)));
                }
            }
            Slide::Map(i, ids) => {
                let s = sections[*i];
                t(&mut ed, f, &s.name.to_uppercase(), 20.0, true, ACCENT, v2(MARGIN, 64.0));
                t(&mut ed, f, "At a glance", 44.0, true, INK, v2(MARGIN, 96.0));
                const GAP: f64 = 32.0;
                let cw = (STAGE.w - GAP * (MAP_COLS - 1) as f64) / MAP_COLS as f64;
                let ch = (STAGE.h - GAP * (MAP_ROWS - 1) as f64) / MAP_ROWS as f64;
                for (j, id) in ids.iter().enumerate() {
                    let (cx, cy) = ((j % MAP_COLS) as f64, (j / MAP_COLS) as f64);
                    let cell = Rect::new(STAGE.x + cx * (cw + GAP), STAGE.y + cy * (ch + GAP), cw, ch - CAPTION_H + 12.0);
                    let placed = place(&mut ed, f, s.doc, *id, None, cell, r);
                    let under = placed.as_ref().map_or(cell.bottom(), |p| p.to.bottom() - r.y);
                    text(&mut ed, f, &clip(&clean(name_of(s.doc, *id)), 48), 16.0, false, MUTED, v2(cell.x, under + 8.0), Some((cw, TextAlign::Center)));
                    inserts.extend(placed);
                }
            }
            Slide::Questions(i, qs) => {
                t(&mut ed, f, &sections[*i].name.to_uppercase(), 20.0, true, ACCENT, v2(MARGIN, 64.0));
                t(&mut ed, f, "Open questions", 44.0, true, INK, v2(MARGIN, 96.0));
                let slot = STAGE.h / QUESTIONS_PER_SLIDE as f64;
                for (j, (screen, q)) in qs.iter().enumerate() {
                    let y = STAGE.y + j as f64 * slot;
                    t(&mut ed, f, "?", 28.0, true, ACCENT, v2(MARGIN, y));
                    text(&mut ed, f, &clip(q, 150), 22.0, false, INK, v2(MARGIN + 56.0, y), Some((STAGE.w - 56.0, TextAlign::Left)));
                    t(&mut ed, f, screen, 16.0, false, MUTED, v2(MARGIN + 56.0, y + 40.0));
                }
            }
            Slide::NextSteps => {
                t(&mut ed, f, "Next steps", 56.0, true, INK, v2(MARGIN, MARGIN));
                let boards: usize = sections.iter().map(|s| s.screens.len()).sum::<usize>() - screens;
                let mut steps = Vec::new();
                if let Some(q) = questions_start {
                    steps.push((format!("Answer {}", count(asked, "open question")), format!("From the notes on the screens; slide {q} on")));
                }
                let reviewed = if boards == 0 { count(screens, "screen") } else { format!("{} and {}", count(screens, "screen"), count(boards, "board")) };
                steps.push((format!("Review the {reviewed}"), "Feedback on a screen by its name, as titled on its slide".to_owned()));
                if let Some(a) = appendix_start {
                    steps.push(("Check the other states".to_owned(), format!("The states kept out of the main flow; appendix, slide {a} on")));
                }
                if let Some(a) = &meta.author {
                    steps.push(("Send feedback".to_owned(), format!("To {a}")));
                }
                for (j, (step, detail)) in steps.iter().enumerate() {
                    let y = 260.0 + j as f64 * 130.0;
                    t(&mut ed, f, &format!("{:02}", j + 1), 32.0, true, ACCENT, v2(MARGIN, y));
                    t(&mut ed, f, step, 32.0, true, INK, v2(MARGIN + 96.0, y));
                    t(&mut ed, f, detail, 22.0, false, MUTED, v2(MARGIN + 96.0, y + 50.0));
                }
            }
            Slide::Appendix => {
                t(&mut ed, f, "A", 160.0, true, ACCENT, v2(MARGIN, 300.0));
                t(&mut ed, f, "Appendix", 80.0, true, INK, v2(MARGIN, 500.0));
                t(&mut ed, f, "Other states of the screens", 28.0, false, MUTED, v2(MARGIN, 620.0));
            }
            Slide::Divider(i) => {
                let s = sections[*i];
                t(&mut ed, f, &format!("{:02}", i + 1), 160.0, true, ACCENT, v2(MARGIN, 300.0));
                text(&mut ed, f, &s.name, 80.0, true, INK, v2(MARGIN, 500.0), Some((W - 2.0 * MARGIN, TextAlign::Left)));
                t(&mut ed, f, &tally(s), 28.0, false, MUTED, v2(MARGIN, 620.0));
            }
            Slide::Screen { i, id, notes, part, states } => {
                let s = sections[*i];
                kicker(&mut ed, f, s, *id);
                let mut title = clean(name_of(s.doc, *id));
                if let Some((k, n)) = part {
                    title = format!("{title} (notes {k}/{n})");
                }
                t(&mut ed, f, &title, 44.0, true, INK, v2(MARGIN, 96.0));
                let mut aside = Vec::new();
                if *states > 0 {
                    aside.push(format!("+ {} in the appendix", count(*states, "state")));
                }
                if let Some(b) = s.doc.world_rect(*id) {
                    aside.push(format!("{} \u{d7} {}", b.w.round(), b.h.round()));
                }
                text(&mut ed, f, &aside.join("   \u{b7}   "), 22.0, false, MUTED, v2(W - MARGIN - 700.0, 116.0), Some((700.0, TextAlign::Right)));
                if notes.is_empty() {
                    inserts.extend(place(&mut ed, f, s.doc, *id, None, STAGE, r));
                } else {
                    let area = Rect::new(STAGE.x, STAGE.y, STAGE.w * NOTED_SCREEN, STAGE.h);
                    let placed = place(&mut ed, f, s.doc, *id, None, area, r);
                    let mut ov = Node::new(NodeId(0), NodeKind::Frame { clip: false }, r);
                    ov.style.set_fill(None);
                    let ov = ed.add_node(ov, None);
                    if let Some(p) = &placed {
                        let sc = p.to.w / p.from.w;
                        let to = p.to.translate(v2(0.0, -r.y));
                        for (num, note) in notes {
                            let Some(tr) = s.doc.world_rect(note.target) else { continue };
                            // Centred on the layer's top-left corner, so it points without hiding it.
                            let x = (to.x + (tr.x - p.from.x) * sc - MARKER / 2.0).clamp(to.x - MARKER / 2.0, to.right() - MARKER / 2.0);
                            let y = (to.y + (tr.y - p.from.y) * sc - MARKER / 2.0).clamp(to.y - MARKER / 2.0, to.bottom() - MARKER / 2.0);
                            marker(&mut ed, ov, *num, note.color, v2(x, y));
                        }
                    }
                    inserts.extend(placed);
                    overlay = Some(ov);
                    let col_x = area.right() + 56.0;
                    let col_w = W - MARGIN - col_x;
                    let slot = STAGE.h / NOTES_PER_SLIDE as f64;
                    for (j, (num, note)) in notes.iter().enumerate() {
                        let y = STAGE.y + j as f64 * slot;
                        marker(&mut ed, f, *num, note.color, v2(col_x, y));
                        let tx = col_x + MARKER + 14.0;
                        let title = clip(&note.title, NOTE_TITLE_CHARS);
                        // The body sits under the title's one or two lines.
                        let title_lines = if title.chars().count() > NOTE_TITLE_LINE_CHARS { 2.0 } else { 1.0 };
                        text(&mut ed, f, &title, 20.0, true, INK, v2(tx, y + 2.0), Some((col_w - MARKER - 14.0, TextAlign::Left)));
                        text(&mut ed, f, &clip(&note.body, NOTE_BODY_CHARS), 16.0, false, MUTED, v2(tx, y + 10.0 + title_lines * 26.0), Some((col_w - MARKER - 14.0, TextAlign::Left)));
                    }
                }
            }
            Slide::Row(i, title, ids, per) => {
                let s = sections[*i];
                kicker(&mut ed, f, s, ids[0]);
                t(&mut ed, f, title, 44.0, true, INK, v2(MARGIN, 96.0));
                // An appendix grid sized by what is on it: a last slide of two is not a grid of six.
                let (cols, rows) = if *per == APPENDIX_PER_SLIDE {
                    let cols = ids.len().min(APPENDIX_COLS);
                    (cols, ids.len().div_ceil(cols))
                } else {
                    (*per, 1)
                };
                let cw = (STAGE.w - PANEL_GAP * (cols - 1) as f64) / cols as f64;
                let ch = (STAGE.h - PANEL_GAP * (rows - 1) as f64) / rows as f64;
                let across = ids.len().min(cols);
                let used = cw * across as f64 + PANEL_GAP * (across - 1) as f64;
                let x0 = STAGE.x + (STAGE.w - used) / 2.0;
                for (j, id) in ids.iter().enumerate() {
                    let (cx, cy) = ((j % cols) as f64, (j / cols) as f64);
                    let col = Rect::new(x0 + cx * (cw + PANEL_GAP), STAGE.y + cy * (ch + PANEL_GAP), cw, ch - CAPTION_H);
                    let placed = place(&mut ed, f, s.doc, *id, None, col, r);
                    let under = placed.as_ref().map_or(col.bottom(), |p| p.to.bottom() - r.y);
                    // A state reads by what differs: `Empty` under a `Sessions` title.
                    let name = clean(name_of(s.doc, *id));
                    let caption = name.strip_prefix(&format!("{title} \u{b7} ")).or_else(|| name.strip_prefix(&format!("{title} "))).unwrap_or(&name);
                    text(&mut ed, f, caption, 18.0, false, MUTED, v2(col.x, under + 14.0), Some((cw, TextAlign::Center)));
                    inserts.extend(placed);
                }
            }
            Slide::Crop(i, id, title, from) => {
                let s = sections[*i];
                kicker(&mut ed, f, s, *id);
                t(&mut ed, f, title, 44.0, true, INK, v2(MARGIN, 96.0));
                // Top-aligned at the scale of the other parts, however short this one is.
                let area = Rect::new(STAGE.x, STAGE.y, STAGE.w, (from.h * part_scale(*from)).min(STAGE.h));
                inserts.extend(place(&mut ed, f, s.doc, *id, Some(*from), area, r));
            }
            Slide::End => {
                t(&mut ed, f, "Thank you", 96.0, true, INK, v2(MARGIN, 380.0));
                t(&mut ed, f, &meta.title, 30.0, false, MUTED, v2(MARGIN, 520.0));
                t(&mut ed, f, &format!("{} \u{b7} {}", count(screens, "screen"), date(meta.now_ms)), 22.0, false, MUTED, v2(MARGIN, 570.0));
            }
        }
        if k > 0 {
            text(&mut ed, f, &meta.title, 18.0, false, MUTED, v2(MARGIN, H - 64.0), None);
            text(&mut ed, f, &format!("{} / {total}", k + 1), 18.0, false, MUTED, v2(W - MARGIN - 200.0, H - 64.0), Some((200.0, TextAlign::Right)));
        }
        pages.push((r, f, overlay, inserts));
    }
    let doc = &ed.doc;
    let pdf_pages: Vec<PdfPage> = pages
        .into_iter()
        .map(|(r, f, overlay, mut inserts)| {
            inserts.extend(overlay.map(|ov| Insert { doc, roots: vec![ov], from: r, to: r }));
            PdfPage { doc, roots: vec![f], bounds: r, background: Some(BG), inserts }
        })
        .collect();
    super::pdf::export(&pdf_pages)
}

/// [`text`] that sizes to its content.
fn t(ed: &mut Editor, f: NodeId, s: &str, size: f64, bold: bool, color: Color, at: V2) -> NodeId {
    text(ed, f, s, size, bold, color, at, None)
}

/// A text layer on slide `f` at `at` (slide units). `width` fixes the box (wrapping, aligned).
#[allow(clippy::too_many_arguments)]
fn text(ed: &mut Editor, f: NodeId, s: &str, size: f64, bold: bool, color: Color, at: V2, width: Option<(f64, TextAlign)>) -> NodeId {
    let (w, align) = width.unwrap_or((1.0, TextAlign::Left));
    let mut n = Node::new(NodeId(0), NodeKind::Text { text: s.into(), font_size: size, auto_width: width.is_none(), align }, Rect::new(at.x, at.y, w, 1.0));
    n.style.set_fill(Some(color));
    n.text_style.bold = bold;
    ed.add_node(n, Some(f))
}

/// The source rects of a screen too tall to read fitted on one slide, top to bottom; None for
/// one that fits.
fn tall_parts(doc: &Document, id: NodeId) -> Option<Vec<Rect>> {
    let b = crate::io::svg::export_bounds(doc, &[id])?;
    if b.w < MIN_SPLIT_W || (STAGE.w / b.w).min(STAGE.h / b.h) >= READABLE {
        return None;
    }
    Some(cut(doc, id, b))
}

/// `b` (an area of layer `id`) cut top to bottom into parts of about a slide each. Each part
/// ends at the lowest line within a slide's reach that crosses no block small enough to fit
/// on a slide (a card, a row), so blocks are not split between slides.
fn cut(doc: &Document, id: NodeId, b: Rect) -> Vec<Rect> {
    /// A part is at least this share of a slide, else it is cut through a block.
    const MIN_PART: f64 = 0.4;
    let rects: Vec<Rect> = super::export::with_layers_on_top(doc, &[id])
        .iter()
        .flat_map(|r| doc.subtree(*r))
        .filter_map(|d| doc.world_rect(d))
        .filter(|r| r.h > 0.0 && r.intersects(&b) && r.h < b.h)
        .collect();
    let step = STAGE.h / part_scale(b);
    // How far a part may run, shown smaller (down to CUT_MIN_SCALE), to end in a gap.
    let reach = step / CUT_MIN_SCALE;
    let blocks: Vec<(f64, f64)> = rects.iter().filter(|r| r.h <= reach).map(|r| (r.y, r.bottom())).collect();
    let crosses = |y: f64| blocks.iter().any(|(top, bottom)| *top < y && y < *bottom);
    let mut parts = Vec::new();
    let mut top = b.y;
    // What is left fits on one last slide, shown a little smaller.
    while b.bottom() - top > reach {
        let gaps: Vec<f64> = blocks.iter().map(|(_, bottom)| *bottom).filter(|y| *y > top + step * MIN_PART && !crosses(*y)).collect();
        // The lowest gap within a slide; else the nearest one within reach; else cut at a slide.
        let y = gaps
            .iter()
            .copied()
            .filter(|y| *y <= top + step)
            .reduce(f64::max)
            .or_else(|| gaps.iter().copied().filter(|y| *y <= top + reach).reduce(f64::min))
            .unwrap_or(top + step);
        parts.push(Rect::new(b.x, top, b.w, y - top));
        top = y;
    }
    parts.push(Rect::new(b.x, top, b.w, b.bottom() - top));
    parts
}

/// A numbered note marker on `f` with its top-left at `at` (slide units).
fn marker(ed: &mut Editor, f: NodeId, num: usize, color: Color, at: V2) {
    let mut dot = Node::new(NodeId(0), NodeKind::Ellipse, Rect::new(at.x, at.y, MARKER, MARKER));
    dot.style.set_fill(Some(color));
    ed.add_node(dot, Some(f));
    text(ed, f, &num.to_string(), 15.0, true, Color { r: 255, g: 255, b: 255, a: 255 }, v2(at.x, at.y + 5.0), Some((MARKER, TextAlign::Center)));
}

/// The page name over a slide's title, and the board's for a board's cards.
fn kicker(ed: &mut Editor, f: NodeId, s: &Section, id: NodeId) {
    let mut k = s.name.to_uppercase();
    if s.doc.get(id).is_some_and(|n| n.section) {
        k = format!("{k} \u{b7} {}", clean(name_of(s.doc, id)).to_uppercase());
    }
    t(ed, f, &k, 20.0, true, ACCENT, v2(MARGIN, 64.0));
}

/// The scale every part of a tall board is shown at: the stage's width, at most 1:1.
fn part_scale(board: Rect) -> f64 {
    (STAGE.w / board.w).min(1.0)
}

/// Screen `id` (or the `crop` of it) fitted into `area` (slide units) on a hairline card;
/// `slide` is the slide's rect in the generated document. None when the screen has no area.
fn place<'d>(ed: &mut Editor, f: NodeId, doc: &'d Document, id: NodeId, crop: Option<Rect>, area: Rect, slide: Rect) -> Option<Insert<'d>> {
    let from = crop.or_else(|| crate::io::svg::export_bounds(doc, &[id]))?;
    let s = (area.w / from.w).min(area.h / from.h);
    let (w, h) = (from.w * s, from.h * s);
    let to = Rect::new(area.x + (area.w - w) / 2.0, area.y + (area.h - h) / 2.0, w, h);
    let mut card = Node::new(NodeId(0), NodeKind::Rect, Rect::new(to.x - CARD_EDGE_W, to.y - CARD_EDGE_W, w + 2.0 * CARD_EDGE_W, h + 2.0 * CARD_EDGE_W));
    card.style.set_fill(Some(CARD_EDGE));
    ed.add_node(card, Some(f));
    Some(Insert {
        doc,
        roots: super::export::with_layers_on_top(doc, &[id]),
        from,
        to: to.translate(v2(slide.x, slide.y)),
    })
}

/// A question line without the team markers the notes use: `Days ? (team)` is `Days?`, and
/// `(? team: limit)` is `(limit?)`.
fn ask(line: &str) -> String {
    const ASK: &str = "(? team: ";
    let mut s = line.to_owned();
    while let Some(a) = s.find(ASK) {
        let Some(b) = s[a..].find(')') else { break };
        let topic = s[a + ASK.len()..a + b].to_owned();
        s.replace_range(a..=a + b, &format!("({topic}?)"));
    }
    let s = s.replace("(? team)", "").replace("? (team)", "?").replace(": ?", "?").replace(" ?", "?").replace("??", "?");
    let s = s.trim();
    if s.contains('?') { s.to_owned() } else { format!("{s}?") }
}

fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// "28 October 2026" for Unix milliseconds (UTC; civil-from-days).
fn date(ms: u64) -> String {
    let z = (ms / 86_400_000) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}

#[cfg(test)]
mod tests;
