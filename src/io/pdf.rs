//! Vector PDF export from the shared draw ops.
//!
//! One PDF page per exported area (a selection, each frame, or a whole page). Shapes, strokes
//! and glyph outlines are vector paths; gradients are axial/radial shadings; images are
//! deflated RGB XObjects with an alpha soft mask; opacity and all 16 blend modes go through
//! ExtGState; clipping frames clip. Text is real text in the embedded Inter faces (`font`), so
//! it can be selected, searched and edited.

use crate::assets;
use crate::geom::{Rect, V2};
use crate::model::{BlendMode, Cap, Color, Document, Join, NodeId, NodeKind, Stop};
use crate::render::draw::{self, DrawOp, PaintSpec};
use crate::vector::{Seg, VPath};
use std::collections::HashMap;
use std::fmt::Write;

mod font;

/// One page of output: what to draw and the area it covers.
pub struct PdfPage<'a> {
    pub doc: &'a Document,
    pub roots: Vec<NodeId>,
    pub bounds: Rect,
    pub background: Option<Color>,
    /// Drawn after `roots`, each scaled into its own area (a presentation's screens).
    pub inserts: Vec<Insert<'a>>,
}

/// Layers of another document drawn scaled: `from` (world units of `doc`) maps onto `to`
/// (the page's units), uniformly by width, clipped to `to`. Stays vector.
pub struct Insert<'a> {
    pub doc: &'a Document,
    pub roots: Vec<NodeId>,
    pub from: Rect,
    pub to: Rect,
}

fn num(x: f64) -> String {
    let r = (x * 1000.0).round() / 1000.0;
    if r == 0.0 {
        "0".into()
    } else if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn blend_name(b: BlendMode) -> &'static str {
    match b {
        BlendMode::Normal => "Normal",
        BlendMode::Multiply => "Multiply",
        BlendMode::Screen => "Screen",
        BlendMode::Overlay => "Overlay",
        BlendMode::Darken => "Darken",
        BlendMode::Lighten => "Lighten",
        BlendMode::ColorDodge => "ColorDodge",
        BlendMode::ColorBurn => "ColorBurn",
        BlendMode::HardLight => "HardLight",
        BlendMode::SoftLight => "SoftLight",
        BlendMode::Difference => "Difference",
        BlendMode::Exclusion => "Exclusion",
        BlendMode::Hue => "Hue",
        BlendMode::Saturation => "Saturation",
        BlendMode::Color => "Color",
        BlendMode::Luminosity => "Luminosity",
    }
}

/// Collects objects; ids are 1-based.
struct Objects {
    objs: Vec<Vec<u8>>,
}

impl Objects {
    fn add(&mut self, body: Vec<u8>) -> usize {
        self.objs.push(body);
        self.objs.len()
    }
    fn reserve(&mut self) -> usize {
        self.add(Vec::new())
    }
    fn set(&mut self, id: usize, body: Vec<u8>) {
        self.objs[id - 1] = body;
    }
    fn stream(&mut self, dict: &str, data: &[u8]) -> usize {
        let z = miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
        let mut b = format!(
            "<< {dict} /Filter /FlateDecode /Length {} >>\nstream\n",
            z.len()
        )
        .into_bytes();
        b.extend_from_slice(&z);
        b.extend_from_slice(b"\nendstream");
        self.add(b)
    }
}

/// Per-page resource registry.
#[derive(Default)]
struct Res {
    gstates: HashMap<(u32, u32, BlendMode), String>,
    gstate_objs: Vec<(String, usize)>,
    shadings: Vec<(String, usize)>,
    images: HashMap<String, (String, usize)>,
    fonts: std::collections::BTreeMap<String, usize>,
}

struct PageWriter<'a> {
    objs: &'a mut Objects,
    fonts: &'a mut font::Fonts,
    res: Res,
    out: String,
    /// While drawing an insert: its source area. Layers outside it are left out of the file.
    cull: Option<Rect>,
}

impl PageWriter<'_> {
    fn gstate(&mut self, fill_a: f32, stroke_a: f32, blend: BlendMode) -> String {
        let key = (
            (fill_a.clamp(0.0, 1.0) * 1000.0) as u32,
            (stroke_a.clamp(0.0, 1.0) * 1000.0) as u32,
            blend,
        );
        if let Some(n) = self.res.gstates.get(&key) {
            return n.clone();
        }
        let name = format!("G{}", self.res.gstates.len());
        let id = self.objs.add(
            format!(
                "<< /Type /ExtGState /ca {} /CA {} /BM /{} >>",
                num(key.0 as f64 / 1000.0),
                num(key.1 as f64 / 1000.0),
                blend_name(blend)
            )
            .into_bytes(),
        );
        self.res.gstates.insert(key, name.clone());
        self.res.gstate_objs.push((name.clone(), id));
        name
    }

    /// PDF path construction operators (quadratics elevated to cubics).
    fn emit_path(&mut self, p: &VPath) -> String {
        let mut o = String::new();
        let mut cur = V2::ZERO;
        let mut start = V2::ZERO;
        let pt = |o: &mut String, a: V2| {
            let _ = write!(o, "{} {} ", num(a.x), num(a.y));
        };
        for s in &p.segs {
            match *s {
                Seg::M(a) => {
                    pt(&mut o, a);
                    o.push_str("m ");
                    cur = a;
                    start = a;
                }
                Seg::L(a) => {
                    pt(&mut o, a);
                    o.push_str("l ");
                    cur = a;
                }
                Seg::Q(c, a) => {
                    pt(&mut o, cur + (c - cur) * (2.0 / 3.0));
                    pt(&mut o, a + (c - a) * (2.0 / 3.0));
                    pt(&mut o, a);
                    o.push_str("c ");
                    cur = a;
                }
                Seg::C(a, b, c) => {
                    pt(&mut o, a);
                    pt(&mut o, b);
                    pt(&mut o, c);
                    o.push_str("c ");
                    cur = c;
                }
                Seg::Z => {
                    o.push_str("h ");
                    cur = start;
                }
            }
        }
        o
    }

    fn rgb(c: [f32; 4]) -> String {
        format!(
            "{} {} {}",
            num(c[0] as f64),
            num(c[1] as f64),
            num(c[2] as f64)
        )
    }

    fn function(&mut self, stops: &[Stop]) -> usize {
        let c = |s: &Stop| {
            format!(
                "{} {} {}",
                num(s.color.r as f64 / 255.0),
                num(s.color.g as f64 / 255.0),
                num(s.color.b as f64 / 255.0)
            )
        };
        if stops.len() <= 2 {
            let (a, b) = (stops.first().unwrap(), stops.last().unwrap());
            return self.objs.add(
                format!(
                    "<< /FunctionType 2 /Domain [0 1] /C0 [{}] /C1 [{}] /N 1 >>",
                    c(a),
                    c(b)
                )
                .into_bytes(),
            );
        }
        let mut fns = Vec::new();
        for w in stops.windows(2) {
            fns.push(
                self.objs.add(
                    format!(
                        "<< /FunctionType 2 /Domain [0 1] /C0 [{}] /C1 [{}] /N 1 >>",
                        c(&w[0]),
                        c(&w[1])
                    )
                    .into_bytes(),
                ),
            );
        }
        let first = stops[0].pos;
        let last = stops.last().unwrap().pos;
        let bounds: Vec<String> = stops[1..stops.len() - 1]
            .iter()
            .map(|s| num(s.pos))
            .collect();
        let encode: Vec<&str> = (0..fns.len()).map(|_| "0 1").collect();
        self.objs.add(
            format!(
                "<< /FunctionType 3 /Domain [{} {}] /Functions [{}] /Bounds [{}] /Encode [{}] >>",
                num(first.min(0.0)),
                num(last.max(1.0)),
                fns.iter()
                    .map(|f| format!("{f} 0 R"))
                    .collect::<Vec<_>>()
                    .join(" "),
                bounds.join(" "),
                encode.join(" ")
            )
            .into_bytes(),
        )
    }

    fn shading(&mut self, dict: String) -> String {
        let name = format!("Sh{}", self.res.shadings.len());
        let id = self.objs.add(dict.into_bytes());
        self.res.shadings.push((name.clone(), id));
        name
    }

    fn image(&mut self, asset: &str) -> Option<String> {
        if let Some((n, _)) = self.res.images.get(asset) {
            return Some(n.clone());
        }
        let rgba = assets::get(asset)?.rgba()?;
        let (w, h) = (rgba.width(), rgba.height());
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        let mut alpha = Vec::with_capacity((w * h) as usize);
        for p in rgba.pixels() {
            rgb.extend_from_slice(&p.0[..3]);
            alpha.push(p.0[3]);
        }
        let smask = self.objs.stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceGray /BitsPerComponent 8"
            ),
            &alpha,
        );
        let id = self.objs.stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask {smask} 0 R"
            ),
            &rgb,
        );
        let name = format!("Im{}", self.res.images.len());
        self.res.images.insert(asset.into(), (name.clone(), id));
        Some(name)
    }

    fn ops(&mut self, ops: &[DrawOp], blend: BlendMode) {
        for op in ops {
            match op {
                DrawOp::Fill {
                    path,
                    paint,
                    even_odd,
                } => {
                    let p = self.emit_path(path);
                    let clip_op = if *even_odd { "W* n" } else { "W n" };
                    match paint {
                        PaintSpec::Solid(c) => {
                            let gs = self.gstate(c[3], 1.0, blend);
                            let _ = writeln!(
                                self.out,
                                "q /{gs} gs {} rg {p}{}\nQ",
                                Self::rgb(*c),
                                if *even_odd { "f*" } else { "f" }
                            );
                        }
                        PaintSpec::Linear {
                            p0,
                            p1,
                            stops,
                            opacity,
                        } => {
                            let a = stops.first().map_or(1.0, |s| s.color.a as f32 / 255.0);
                            let gs = self.gstate(opacity * a, 1.0, blend);
                            let f = self.function(stops);
                            let sh = self.shading(format!(
                                "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [{} {} {} {}] /Function {f} 0 R /Extend [true true] >>",
                                num(p0.x),
                                num(p0.y),
                                num(p1.x),
                                num(p1.y)
                            ));
                            let _ = writeln!(self.out, "q /{gs} gs {p}{clip_op} /{sh} sh\nQ");
                        }
                        PaintSpec::Radial {
                            center,
                            rx,
                            ry,
                            stops,
                            opacity,
                        } => {
                            let a = stops.first().map_or(1.0, |s| s.color.a as f32 / 255.0);
                            let gs = self.gstate(opacity * a, 1.0, blend);
                            let f = self.function(stops);
                            let sh = self.shading(format!(
                                "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [0 0 0 0 0 1] /Function {f} 0 R /Extend [true true] >>"
                            ));
                            let _ = writeln!(
                                self.out,
                                "q /{gs} gs {p}{clip_op} {} {} {} {} {} {} cm /{sh} sh\nQ",
                                num(rx.x),
                                num(rx.y),
                                num(ry.x),
                                num(ry.y),
                                num(center.x),
                                num(center.y)
                            );
                        }
                        PaintSpec::Image {
                            asset,
                            place,
                            tile,
                            opacity,
                        } => {
                            let Some(im) = self.image(asset) else {
                                continue;
                            };
                            let gs = self.gstate(*opacity, 1.0, blend);
                            let _ = write!(self.out, "q /{gs} gs {p}{clip_op} ");
                            let mut placements = vec![*place];
                            if *tile {
                                placements.clear();
                                let b = path.bounds().unwrap_or(*place);
                                let (w, h) = (place.w.max(1.0), place.h.max(1.0));
                                let x0 = place.x + ((b.x - place.x) / w).floor() * w;
                                let y0 = place.y + ((b.y - place.y) / h).floor() * h;
                                let mut y = y0;
                                while y < b.bottom() && placements.len() < 2000 {
                                    let mut x = x0;
                                    while x < b.right() && placements.len() < 2000 {
                                        placements.push(Rect::new(x, y, w, h));
                                        x += w;
                                    }
                                    y += h;
                                }
                            }
                            for r in placements {
                                let _ = write!(
                                    self.out,
                                    "q {} 0 0 {} {} {} cm /{im} Do Q ",
                                    num(r.w),
                                    num(-r.h),
                                    num(r.x),
                                    num(r.y + r.h)
                                );
                            }
                            self.out.push_str("Q\n");
                        }
                    }
                }
                DrawOp::Stroke {
                    path,
                    color,
                    width,
                    cap,
                    join,
                    dash,
                } => {
                    let p = self.emit_path(path);
                    let gs = self.gstate(1.0, color[3], blend);
                    let cap = match cap {
                        Cap::Butt => 0,
                        Cap::Round => 1,
                        Cap::Square => 2,
                    };
                    let join = match join {
                        Join::Miter => 0,
                        Join::Round => 1,
                        Join::Bevel => 2,
                    };
                    let d: Vec<String> = dash.iter().map(|x| num(*x)).collect();
                    let _ = writeln!(
                        self.out,
                        "q /{gs} gs {} RG {} w {cap} J {join} j [{}] 0 d {p}S\nQ",
                        Self::rgb(*color),
                        num(*width),
                        d.join(" ")
                    );
                }
                DrawOp::Glyphs {
                    face,
                    size,
                    color,
                    glyphs,
                } => {
                    let (name, id) = self.fonts.name(self.objs, *face, glyphs.iter().map(|g| g.0));
                    self.res.fonts.insert(name.clone(), id);
                    let gs = self.gstate(color[3], 1.0, blend);
                    let _ = write!(self.out, "q /{gs} gs {} rg BT /{name} {} Tf", Self::rgb(*color), num(*size));
                    // One text matrix per baseline (it flips y back upright), then each glyph
                    // nudged from where the font's advance would put it to where layout did.
                    let mut pen: Option<V2> = None;
                    for (gid, o) in glyphs {
                        match pen {
                            Some(p) if (p.y - o.y).abs() < 0.01 => {
                                let adj = (p.x - o.x) * 1000.0 / size;
                                if adj.abs() > 0.05 {
                                    let _ = write!(self.out, " {}", num(adj));
                                }
                            }
                            _ => {
                                if pen.is_some() {
                                    self.out.push_str("] TJ");
                                }
                                let _ = write!(self.out, " 1 0 0 -1 {} {} Tm [", num(o.x), num(o.y));
                            }
                        }
                        let _ = write!(self.out, "<{gid:04X}>");
                        pen = Some(V2 { x: o.x + font::advance(*face, *gid, *size), y: o.y });
                    }
                    if pen.is_some() {
                        self.out.push_str("] TJ");
                    }
                    self.out.push_str(" ET Q\n");
                }
            }
        }
    }

    fn node(&mut self, doc: &Document, id: NodeId, opacity: f64) {
        let Some(n) = doc.get(id) else { return };
        if !n.visible {
            return;
        }
        let wr = doc.world_rect(id).unwrap();
        let op = opacity * n.style.opacity;
        let clip = matches!(n.kind, NodeKind::Frame { clip: true });
        // Outside the insert's area: nothing of it shows, unless an unclipped child pokes in.
        let outside = self.cull.is_some_and(|c| !wr.intersects(&c));
        if outside && (clip || n.children.is_empty()) {
            return;
        }
        if !outside {
            let d = draw::node_draw(n, wr, op);
            self.ops(&d.ops, d.blend);
        }
        if n.children.is_empty() {
            return;
        }
        if clip {
            let p = self.emit_path(&draw::shape_path(n, wr).unwrap_or_default());
            let _ = writeln!(self.out, "q {p}W n");
        }
        for c in &n.children {
            self.node(doc, *c, op);
        }
        if clip {
            self.out.push_str("Q\n");
        }
    }
}

/// Write a PDF with one page per entry.
pub fn export(pages: &[PdfPage]) -> Vec<u8> {
    let mut objs = Objects { objs: Vec::new() };
    let catalog = objs.reserve();
    let pages_id = objs.reserve();
    let mut kids = Vec::new();
    let mut fonts = font::Fonts::default();
    for pg in pages {
        let b = pg.bounds;
        let mut w = PageWriter {
            objs: &mut objs,
            fonts: &mut fonts,
            res: Res::default(),
            out: String::new(),
            cull: None,
        };
        // y-down user space with the page's top-left at the area's top-left.
        let _ = writeln!(w.out, "1 0 0 -1 {} {} cm", num(-b.x), num(b.y + b.h));
        if let Some(bg) = pg.background {
            let c = bg.to_f32(1.0);
            let _ = writeln!(
                w.out,
                "{} rg {} {} {} {} re f",
                PageWriter::rgb(c),
                num(b.x),
                num(b.y),
                num(b.w),
                num(b.h)
            );
        }
        for r in &pg.roots {
            let op = draw::inherited_opacity(pg.doc, *r);
            w.node(pg.doc, *r, op);
        }
        for ins in &pg.inserts {
            let s = ins.to.w / ins.from.w.max(f64::EPSILON);
            let (t, f) = (ins.to, ins.from);
            // Full precision: the scale multiplies world coordinates in the tens of thousands, so
            // rounding it to num()'s 3 decimals shifts the screen inside its window.
            let _ = writeln!(
                w.out,
                "q {} {} {} {} re W n {s:.9} 0 0 {s:.9} {:.4} {:.4} cm",
                num(t.x),
                num(t.y),
                num(t.w),
                num(t.h),
                t.x - f.x * s,
                t.y - f.y * s
            );
            // A margin for strokes drawn outside their layer.
            const CULL_MARGIN: f64 = 16.0;
            w.cull = Some(Rect::new(f.x - CULL_MARGIN, f.y - CULL_MARGIN, f.w + 2.0 * CULL_MARGIN, f.h + 2.0 * CULL_MARGIN));
            for r in &ins.roots {
                let op = draw::inherited_opacity(ins.doc, *r);
                w.node(ins.doc, *r, op);
            }
            w.cull = None;
            w.out.push_str("Q\n");
        }
        let content = std::mem::take(&mut w.out);
        let res = std::mem::take(&mut w.res);
        let content_id = objs.stream("", content.as_bytes());
        let dict = |items: &[(String, usize)]| {
            items
                .iter()
                .map(|(n, id)| format!("/{n} {id} 0 R"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let images: Vec<(String, usize)> = res.images.values().cloned().collect();
        let fonts_of_page: Vec<(String, usize)> = res.fonts.into_iter().collect();
        let page = objs.add(
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {} {}] /Contents {content_id} 0 R /Resources << /ExtGState << {} >> /Shading << {} >> /XObject << {} >> /Font << {} >> >> >>",
                num(b.w),
                num(b.h),
                dict(&res.gstate_objs),
                dict(&res.shadings),
                dict(&images),
                dict(&fonts_of_page)
            )
            .into_bytes(),
        );
        kids.push(page);
    }
    fonts.finish(&mut objs);
    objs.set(
        catalog,
        format!("<< /Type /Catalog /Pages {pages_id} 0 R >>").into_bytes(),
    );
    objs.set(
        pages_id,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.iter()
                .map(|k| format!("{k} 0 R"))
                .collect::<Vec<_>>()
                .join(" "),
            kids.len()
        )
        .into_bytes(),
    );
    let mut out: Vec<u8> = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let mut x = format!("xref\n0 {}\n0000000000 65535 f \n", objs.objs.len() + 1);
    for o in offsets {
        let _ = writeln!(x, "{o:010} 00000 n ");
    }
    let _ = write!(
        x,
        "trailer\n<< /Size {} /Root {catalog} 0 R /Info << /Producer (papercraft) >> >>\nstartxref\n{xref}\n%%EOF\n",
        objs.objs.len() + 1
    );
    out.extend_from_slice(x.as_bytes());
    out
}

/// Pages for a document: one per top-level frame (`frames`), else one for `roots` / all.
pub fn pages_for<'a>(
    doc: &'a Document,
    roots: Option<&[NodeId]>,
    frames: bool,
    background: Option<Color>,
) -> Vec<PdfPage<'a>> {
    let roots: Vec<NodeId> = roots
        .map(|r| r.to_vec())
        .unwrap_or_else(|| doc.root.clone());
    if frames {
        let fr: Vec<NodeId> = roots
            .iter()
            .copied()
            .filter(|id| doc.get(*id).is_some_and(|n| n.is_frame() && n.visible))
            .collect();
        if !fr.is_empty() {
            return fr
                .into_iter()
                .filter_map(|f| {
                    Some(PdfPage {
                        doc,
                        bounds: crate::io::svg::export_bounds(doc, &[f])?,
                        roots: crate::io::export::with_layers_on_top(doc, &[f]),
                        background,
                        inserts: Vec::new(),
                    })
                })
                .collect();
        }
    }
    let roots = crate::io::export::with_layers_on_top(doc, &roots);
    match crate::io::svg::export_bounds(doc, &roots) {
        Some(b) => vec![PdfPage {
            doc,
            roots,
            bounds: b,
            background,
            inserts: Vec::new(),
        }],
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_structure_is_valid() {
        let ed = crate::demo::showcase();
        let pages = pages_for(&ed.doc, None, true, Some(Color::WHITE));
        assert_eq!(pages.len(), 2, "one page per top-level frame");
        let pdf = export(&pages);
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.7"));
        assert!(s.contains("/Count 2"));
        assert!(s.trim_end().ends_with("%%EOF"));
        // The xref offsets point at the objects (check on raw bytes: streams are binary).
        let tail = &pdf[pdf.len() - 64..];
        let tail = String::from_utf8_lossy(tail);
        let xref_at: usize = tail
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(pdf[xref_at..].starts_with(b"xref"));
        let table = String::from_utf8_lossy(&pdf[xref_at..]);
        let first_off: usize = table.lines().nth(3).unwrap()[..10].parse().unwrap();
        assert!(pdf[first_off..].starts_with(b"1 0 obj"));
    }

    #[test]
    fn text_is_written_as_text_in_an_embedded_font() {
        use crate::model::{Node, NodeKind};
        let mut ed = crate::editor::Editor::default();
        let t = ed.add_node(Node::new(NodeId(0), NodeKind::Text { text: "Olá".into(), font_size: 16.0, auto_width: true, align: Default::default() }, Rect::new(0.0, 0.0, 200.0, 40.0)), None);
        let pdf = export(&pages_for(&ed.doc, Some(&[t]), false, None));
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("/Subtype /Type0") && s.contains("/ToUnicode") && s.contains("/FontFile2"), "embedded font");
        assert_eq!(s.matches("/Subtype /Type0").count(), 1, "one font object per face");
    }

    #[test]
    fn an_insert_far_from_the_origin_lands_where_it_is_placed() {
        use crate::geom::Rect;
        use crate::model::{Node, NodeKind};
        let mut ed = crate::editor::Editor::default();
        let from = Rect::new(4192.0, 25412.0, 1440.0, 900.0);
        let screen = ed.edit("Add", |tx| {
            let id = tx.alloc_id();
            tx.insert(Node::new(id, NodeKind::Frame { clip: true }, from), None, None);
            id
        });
        let to = Rect::new(100.0, 100.0, 1440.0 * 0.3361, 900.0 * 0.3361);
        let page = PdfPage { doc: &ed.doc, roots: Vec::new(), bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0), background: None, inserts: vec![Insert { doc: &ed.doc, roots: vec![screen], from, to }] };
        let pdf = export(&[page]);
        let content = pdf
            .windows(7)
            .position(|w| w == b"stream\n")
            .and_then(|at| miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[at + 7..]).ok())
            .expect("a page content stream");
        let line = String::from_utf8_lossy(&content).lines().find(|l| l.ends_with(" cm") && l.contains("re W n")).unwrap().to_owned();
        let n: Vec<f64> = line.split_whitespace().filter_map(|t| t.parse().ok()).collect();
        let (s, ty) = (n[4], n[9]);
        assert!((from.y * s + ty - to.y).abs() < 0.01, "the screen's top maps onto the window's top: {line}");
    }

    #[test]
    fn a_frame_page_includes_layers_laid_over_it() {
        use crate::geom::Rect;
        use crate::model::{Node, NodeKind};
        let mut ed = crate::editor::Editor::default();
        let mut add = |r: Rect| {
            ed.edit("Add", |tx| {
                let id = tx.alloc_id();
                tx.insert(Node::new(id, NodeKind::Frame { clip: true }, r), None, None);
                id
            })
        };
        let board = add(Rect::new(0.0, 0.0, 400.0, 300.0));
        let main = add(Rect::new(20.0, 20.0, 100.0, 40.0));
        let outside = add(Rect::new(500.0, 0.0, 100.0, 40.0));
        let pages = pages_for(&ed.doc, Some(&[board]), true, None);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].roots, vec![board, main], "the main on the board, not the one beside it ({outside:?})");
    }
}
