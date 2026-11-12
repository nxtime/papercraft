//! Every surface a phone can reach, drawn headlessly at phone sizes with the system bars' insets
//! and the phone's UI scale: nothing leaves the safe area or its clip, no two texts overlap, no
//! tap target covers someone else's text, and tap targets are at least a finger tall.
//!
//! The screens are laid out in points of the scaled UI, so a 360 x 780 dp phone at scale 1.25 is
//! a 288 x 624 pt screen.

use egui::{Pos2, Rect, Vec2};
use papercraft::editor::Editor;
use papercraft::geom::{Rect as WRect, v2};
use papercraft::model::{Node, NodeId, NodeKind};
use papercraft::ui::phone::Tab;
use papercraft::ui::settings::Page;
use papercraft::ui::{self, UiState, home};
use std::collections::BTreeSet;

/// The 44 dp finger target, in egui points at the phone's UI scale.
const MIN_TARGET: f32 = 44.0 / papercraft::theme::PHONE_UI_SCALE;
/// Devices in dp: Xiaomi-class 20:9 and a larger 412 dp phone.
const DEVICES: [(f32, f32); 2] = [(360.0, 780.0), (412.0, 915.0)];
/// Status bar / cutout and gesture pill, in dp.
const INSETS_DP: [f32; 4] = [0.0, 32.0, 0.0, 24.0];
const SLACK: f32 = 0.75;

struct Text {
    layer: egui::LayerId,
    text: String,
    rect: Rect,
    clip: Rect,
}

struct Widget {
    layer: egui::LayerId,
    rect: Rect,
    clip: Rect,
    click: bool,
    enabled: bool,
}

#[derive(Default)]
struct Audit {
    texts: Vec<Text>,
    widgets: Vec<Widget>,
}

fn flatten(shape: &egui::Shape, clip: Rect, layer: egui::LayerId, out: &mut Vec<Text>) {
    match shape {
        egui::Shape::Text(t) if t.opacity_factor > 0.01 => out.push(Text {
            layer,
            text: t.galley.text().to_owned(),
            rect: t.galley.rect.translate(t.pos.to_vec2()),
            clip,
        }),
        egui::Shape::Vec(list) => list.iter().for_each(|s| flatten(s, clip, layer, out)),
        _ => {}
    }
}

fn collect(ctx: &egui::Context) -> Audit {
    let mut audit = Audit::default();
    let mut layers = ctx.memory(|m| m.areas().visible_layer_ids());
    layers.insert(egui::LayerId::background());
    ctx.viewport(|v| {
        for (layer, rects) in v.this_pass.widgets.layers() {
            layers.insert(*layer);
            for w in rects {
                audit.widgets.push(Widget {
                    layer: *layer,
                    rect: w.rect,
                    clip: w.interact_rect,
                    click: w.sense.senses_click(),
                    enabled: w.enabled,
                });
            }
        }
    });
    ctx.graphics(|g| {
        for layer in layers {
            for s in g.get(layer).into_iter().flat_map(|l| l.all_entries()) {
                flatten(&s.shape, s.clip_rect, layer, &mut audit.texts);
            }
        }
    });
    audit
}

struct Rig {
    ctx: egui::Context,
    ed: Editor,
    st: UiState,
    scale: f32,
    dp: (f32, f32),
    audit: Audit,
}

impl Rig {
    fn new(dp: (f32, f32)) -> Rig {
        let ctx = egui::Context::default();
        ui::setup_style(&ctx);
        let scale = papercraft::theme::PHONE_UI_SCALE;
        let mut rig = Rig { ctx, ed: Editor::default(), st: UiState::default(), scale, dp, audit: Audit::default() };
        rig.st.settings.chat = Default::default();
        rig
    }

    fn screen(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(self.dp.0, self.dp.1) / self.scale)
    }

    fn content(&self) -> Rect {
        let [l, t, r, b] = INSETS_DP.map(|v| v / self.scale);
        Rect::from_min_max(self.screen().min + Vec2::new(l, t), self.screen().max - Vec2::new(r, b))
    }

    fn frames(&mut self, events: Vec<egui::Event>) {
        let [l, t, r, b] = INSETS_DP.map(|v| v / self.scale);
        for i in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(self.screen()),
                safe_area_insets: Some(egui::SafeAreaInsets(egui::epaint::MarginF32 { left: l, top: t, right: r, bottom: b })),
                events: if i == 0 { events.clone() } else { Vec::new() },
                ..Default::default()
            };
            let (ed, st, ctx) = (&mut self.ed, &mut self.st, self.ctx.clone());
            let mut audit = None;
            let mut out = self.ctx.run_ui(input, |ui| {
                papercraft::render::frame::draw_in_safe_area(ui, ed, st);
                audit = Some(collect(&ctx));
            });
            out.textures_delta.clear();
            self.audit = audit.unwrap_or_default();
        }
    }

    fn visible(&self, layer: egui::LayerId, rect: Rect) -> bool {
        match self.ctx.layer_id_at(rect.center()) {
            Some(top) => top == layer,
            None => layer == egui::LayerId::background(),
        }
    }

    fn failures(&self) -> BTreeSet<String> {
        let content = self.content().expand(SLACK);
        let mut bad = BTreeSet::new();
        let texts: Vec<_> = self
            .audit
            .texts
            .iter()
            .filter(|t| !t.text.trim().is_empty())
            .filter(|t| t.rect.intersect(t.clip).is_positive() && self.visible(t.layer, t.rect.intersect(t.clip)))
            .collect();
        for t in &texts {
            let shown = t.rect.intersect(t.clip);
            if !content.contains_rect(shown) {
                bad.insert(format!("(a) text {:?} {:?} leaves the safe area {:?}", t.text, shown, self.content()));
            }
            // Text in an input scrolls inside the field: its clip is the field.
            let in_input = self.audit.widgets.iter().any(|w| w.click && w.layer == t.layer && w.rect.expand(1.0).contains_rect(t.clip));
            if !in_input && (t.rect.left() < t.clip.left() - SLACK || t.rect.right() > t.clip.right() + SLACK) {
                bad.insert(format!("(a) text {:?} {:?} is cut by its clip {:?}", t.text, t.rect, t.clip));
            }
        }
        for (i, a) in texts.iter().enumerate() {
            for b in &texts[i + 1..] {
                let shadow = a.text == b.text && (a.rect.min - b.rect.min).abs().max_elem() < 3.0;
                if a.layer == b.layer && !shadow && a.rect.intersect(a.clip).shrink(1.0).intersects(b.rect.intersect(b.clip).shrink(1.0)) {
                    bad.insert(format!("(b) texts {:?} {:?} and {:?} {:?} overlap", a.text, a.rect, b.text, b.rect));
                }
            }
        }
        for w in self.audit.widgets.iter().filter(|w| w.click && w.enabled) {
            let shown = w.rect.intersect(w.clip);
            if !shown.is_positive() || !self.visible(w.layer, shown) {
                continue;
            }
            if !content.contains_rect(shown) {
                bad.insert(format!("(a) widget {shown:?} leaves the safe area {:?}", self.content()));
            }
            // A small control inside a larger tap area (a switch in its row) is not the target.
            let under_bigger = self.audit.widgets.iter().any(|o| {
                o.click && o.layer == w.layer && o.rect != w.rect && o.rect.contains_rect(w.rect) && (o.rect.center().y - w.rect.center().y).abs() < 2.0 && o.rect.height() >= MIN_TARGET - SLACK
            });
            if w.rect.height() < MIN_TARGET - SLACK && !under_bigger {
                let label = texts.iter().find(|t| t.layer == w.layer && w.rect.expand(1.0).contains_rect(t.rect.intersect(t.clip))).map_or("", |t| t.text.as_str());
                bad.insert(format!("(d) target {:?} {label:?} is {:.0} tall", w.rect, w.rect.height()));
            }
            for t in texts.iter().filter(|t| t.layer == w.layer) {
                let own = w.rect.expand(1.0).contains_rect(t.rect.intersect(t.clip));
                if !own && shown.shrink(1.0).intersects(t.rect.intersect(t.clip).shrink(1.0)) {
                    bad.insert(format!("(c) target {shown:?} covers text {:?} {:?}", t.text, t.rect));
                }
            }
        }
        bad
    }
}

fn shapes(ed: &mut Editor) -> Vec<NodeId> {
    let mut add = |kind, r: WRect, parent| ed.add_node(Node::new(NodeId(0), kind, r), parent);
    let frame = add(NodeKind::Frame { clip: true }, WRect::new(0.0, 0.0, 300.0, 400.0), None);
    let rect = add(NodeKind::Rect, WRect::new(10.0, 10.0, 80.0, 60.0), Some(frame));
    let text = add(
        NodeKind::Text { text: "Hello".into(), font_size: 16.0, auto_width: true, align: Default::default() },
        WRect::new(10.0, 90.0, 80.0, 20.0),
        Some(frame),
    );
    let shape = add(NodeKind::Ellipse, WRect::new(120.0, 10.0, 60.0, 60.0), Some(frame));
    vec![frame, rect, text, shape]
}

type Scenario = (&'static str, fn(&mut Rig));

fn tab(rig: &mut Rig, tab: Tab) {
    rig.st.phone.tab = tab;
}

fn editor(rig: &mut Rig) -> Vec<NodeId> {
    let ids = shapes(&mut rig.ed);
    rig.st.project_name = "Project".into();
    ids
}

fn settings(rig: &mut Rig, page: Page, on_home: bool) {
    if on_home {
        rig.st.home = Some(home::HomeView::default());
    } else {
        editor(rig);
    }
    rig.st.settings.open = true;
    rig.st.settings.set_page(page);
}

const PAGES: [Page; 6] = [Page::Appearance, Page::Canvas, Page::Chat, Page::Plugins, Page::Performance, Page::Updates];

fn scenarios() -> Vec<Scenario> {
    let list: Vec<Scenario> = vec![
        ("home empty", |r| r.st.home = Some(home::HomeView::default())),
        ("home with recents", |r| {
            let mut hv = home::HomeView::default();
            hv.entries = (0..3)
                .map(|i| papercraft::project::Entry {
                    dir: format!("p{i}").into(),
                    manifest: papercraft::project::format::Manifest::new(&format!("A project with a rather long name {i}")),
                    thumbnail: None,
                })
                .collect();
            r.st.home = Some(hv);
        }),
        ("home trash", |r| {
            let mut hv = home::HomeView::default();
            hv.show_trash = true;
            r.st.home = Some(hv);
        }),
        ("editor canvas", |r| {
            editor(r);
        }),
        ("editor layers", |r| {
            editor(r);
            tab(r, Tab::Layers);
        }),
        ("editor pages", |r| {
            editor(r);
            tab(r, Tab::Pages);
        }),
        ("properties of nothing", |r| {
            editor(r);
            tab(r, Tab::Properties);
        }),
        ("properties of a frame", |r| {
            let ids = editor(r);
            r.ed.select(vec![ids[0]]);
            tab(r, Tab::Properties);
        }),
        ("properties of a rectangle", |r| {
            let ids = editor(r);
            r.ed.select(vec![ids[1]]);
            tab(r, Tab::Properties);
        }),
        ("properties of text", |r| {
            let ids = editor(r);
            r.ed.select(vec![ids[2]]);
            tab(r, Tab::Properties);
        }),
        ("properties of a shape", |r| {
            let ids = editor(r);
            r.ed.select(vec![ids[3]]);
            tab(r, Tab::Properties);
        }),
        ("properties of a component instance", |r| {
            let ids = editor(r);
            r.ed.select(vec![ids[0]]);
            r.ed.create_component();
            r.ed.create_instance(ids[0], v2(400.0, 0.0));
            tab(r, Tab::Properties);
        }),
        ("chat page", |r| {
            editor(r);
            tab(r, Tab::Chat);
        }),
        ("layer context menu", |r| {
            let ids = editor(r);
            tab(r, Tab::Layers);
            r.st.phone.menu = Some((Pos2::new(60.0, 200.0), ids[1], r.ed.doc_stamp));
        }),
        ("rename dialog", |r| {
            let ids = editor(r);
            r.st.renaming = Some((ids[1], "Rectangle".into()));
        }),
        ("shortcuts sheet", |r| {
            editor(r);
            r.st.show_help = true;
        }),
        ("command palette", |r| {
            editor(r);
            r.st.palette.toggle();
        }),
        ("present mode", |r| {
            let ids = editor(r);
            r.ed.select(vec![ids[0]]);
            ui::start_present(&mut r.ed, &mut r.st);
        }),
        ("settings on home", |r| settings(r, Page::Appearance, true)),
        ("settings chat page from the chat gear", |r| {
            editor(r);
            tab(r, Tab::Chat);
            r.st.settings.open_chat();
        }),
    ];
    list.into_iter()
    .chain(PAGES.map(|p| -> Scenario {
        match p {
            Page::Appearance => ("settings appearance", |r| settings(r, Page::Appearance, false)),
            Page::Canvas => ("settings canvas", |r| settings(r, Page::Canvas, false)),
            Page::Chat => ("settings chat", |r| settings(r, Page::Chat, false)),
            Page::Plugins => ("settings plugins", |r| settings(r, Page::Plugins, false)),
            Page::Performance => ("settings performance", |r| settings(r, Page::Performance, false)),
            Page::Updates => ("settings updates", |r| settings(r, Page::Updates, false)),
        }
    }))
    .collect()
}

#[test]
fn every_phone_surface_fits_the_safe_area_and_the_finger() {
    let mut report = Vec::new();
    for (w, h) in DEVICES {
        for (name, setup) in scenarios() {
            let run = std::panic::catch_unwind(|| {
                let mut rig = Rig::new((w, h));
                setup(&mut rig);
                rig.frames(Vec::new());
                rig.failures()
            });
            match run {
                Ok(found) => report.extend(found.into_iter().map(|f| format!("{name} @ {w}x{h}: {f}"))),
                Err(_) => report.push(format!("{name} @ {w}x{h}: panicked while drawing")),
            }
        }
    }
    assert!(report.is_empty(), "{} failures:\n{}", report.len(), report.join("\n"));
}

#[test]
fn the_size_label_clears_the_plus_knob_under_a_selected_shape() {
    let mut ed = Editor::default();
    let id = ed.add_node(Node::new(NodeId(0), NodeKind::Rect, WRect::new(0.0, 0.0, 164.0, 182.0)), None);
    ed.select(vec![id]);
    let (_, knob) = ed.quick_knobs()[1];
    let sb = ed.camera.world_rect_to_screen(&ed.doc.world_rect(id).unwrap());
    let badge = papercraft::render::scene::size_badge_rect(sb, 60.0, true);
    assert!(badge.y >= knob.y + 8.0, "badge top {} under the knob's bottom {}", badge.y, knob.y + 8.0);
}

#[test]
fn the_recent_designs_cards_sit_centred_in_the_safe_area() {
    for (w, h) in DEVICES {
        let mut rig = Rig::new((w, h));
        let mut hv = home::HomeView::default();
        hv.entries = (0..3)
            .map(|i| papercraft::project::Entry {
                dir: format!("p{i}").into(),
                manifest: papercraft::project::format::Manifest::new(&format!("Design {i}")),
                thumbnail: None,
            })
            .collect();
        rig.st.home = Some(hv);
        rig.frames(Vec::new());
        let content = rig.content();
        let card = rig.audit.widgets.iter().filter(|w| w.click && w.rect.width() > 200.0 && w.rect.height() > 100.0).map(|w| w.rect).next().expect("a card");
        let (left, right) = (card.left() - content.left(), content.right() - card.right());
        assert!((left - right).abs() < 1.0 && left > 8.0, "{w}x{h}: card {card:?} has {left} left and {right} right of the safe area");
    }
}
