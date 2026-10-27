//! Nodes → backend-neutral vector draw operations in world coordinates.
//!
//! The GPU tessellator, the SVG writer and the PDF writer all consume these ops, so every
//! backend draws the same geometry: shape outlines, gradients, images, strokes and glyphs.

use crate::assets;
use crate::geom::{Rect, V2, v2};
use crate::ink;
use crate::model::{
    BlendMode, Cap, Color, Document, FillRule, ImageMode, Join, Node, NodeId, NodeKind, Paint,
    Stop, StrokeAlign,
};
use crate::text::{self, FaceId};
use crate::vector::{self, VPath};

#[derive(Debug, Clone, PartialEq)]
pub enum PaintSpec {
    /// Straight-alpha RGBA, opacity already applied.
    Solid([f32; 4]),
    Linear {
        p0: V2,
        p1: V2,
        stops: Vec<Stop>,
        opacity: f32,
    },
    /// Ellipse with center and the two radius vectors.
    Radial {
        center: V2,
        rx: V2,
        ry: V2,
        stops: Vec<Stop>,
        opacity: f32,
    },
    Image {
        asset: String,
        /// World rect of one copy of the image.
        place: Rect,
        tile: bool,
        opacity: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawOp {
    Fill {
        path: VPath,
        paint: PaintSpec,
        even_odd: bool,
    },
    Stroke {
        path: VPath,
        color: [f32; 4],
        width: f64,
        cap: Cap,
        join: Join,
        dash: Vec<f64>,
    },
    /// Glyphs sharing one face, size and color; positions are world baselines.
    Glyphs {
        face: FaceId,
        size: f64,
        color: [f32; 4],
        glyphs: Vec<(u16, V2)>,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeDraw {
    pub ops: Vec<DrawOp>,
    pub blend: BlendMode,
}

pub fn rgba(c: Color, opacity: f64) -> [f32; 4] {
    c.to_f32(opacity)
}

/// The node's outline in world space (None for text and groups).
pub fn shape_path(n: &Node, wr: Rect) -> Option<VPath> {
    Some(match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Rect => vector::rounded_rect(wr, n.style.corner_radii()),
        NodeKind::Ellipse => vector::ellipse(wr),
        NodeKind::Polygon { sides } => {
            vector::rounded_polygon(&vector::polygon_points(wr, *sides), n.style.corner_radius)
        }
        NodeKind::Star { points, inner } => vector::rounded_polygon(
            &vector::star_points(wr, *points, *inner),
            n.style.corner_radius,
        ),
        NodeKind::Path { subpaths, .. } => vector::from_subpaths(wr.min(), subpaths),
        NodeKind::Line { a, b } => VPath::polyline(&[wr.min() + *a, wr.min() + *b], false),
        NodeKind::Stroke {
            points,
            width,
            thinning,
        } => {
            let o = ink::outline(points, *width, *thinning);
            let pts: Vec<V2> = o.iter().map(|q| wr.min() + *q).collect();
            VPath::polyline(&pts, true)
        }
        NodeKind::Connector { points, route, .. } => {
            let pts: Vec<V2> = points.iter().map(|p| wr.min() + *p).collect();
            crate::connector::line_path(&pts, *route)
        }
        NodeKind::Sticky { .. } => vector::rounded_rect(wr, [n.style.corner_radius.max(2.0); 4]),
        NodeKind::Text { .. } | NodeKind::Group => return None,
    })
}

/// Whether the outline is a closed area (fills apply).
pub fn is_area(n: &Node) -> bool {
    !matches!(
        n.kind,
        NodeKind::Line { .. }
            | NodeKind::Text { .. }
            | NodeKind::Group
            | NodeKind::Connector { .. }
    ) && !matches!(&n.kind, NodeKind::Path { subpaths, .. } if subpaths.iter().all(|s| !s.closed))
}

/// Stroke outline honoring inside/outside alignment for box shapes.
fn stroke_path(n: &Node, wr: Rect) -> Option<VPath> {
    let d = match n.style.stroke_align {
        StrokeAlign::Center => 0.0,
        StrokeAlign::Inside => -n.style.stroke_width * 0.5,
        StrokeAlign::Outside => n.style.stroke_width * 0.5,
    };
    if d == 0.0 {
        return shape_path(n, wr);
    }
    let r = wr.inflate(d);
    if r.w <= 0.0 || r.h <= 0.0 {
        return None;
    }
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Rect => Some(vector::rounded_rect(
            r,
            n.style
                .corner_radii()
                .map(|x| if x > 0.0 { (x + d).max(0.0) } else { 0.0 }),
        )),
        NodeKind::Ellipse => Some(vector::ellipse(r)),
        NodeKind::Polygon { sides } => Some(vector::rounded_polygon(
            &vector::polygon_points(r, *sides),
            n.style.corner_radius,
        )),
        NodeKind::Star { points, inner } => Some(vector::rounded_polygon(
            &vector::star_points(r, *points, *inner),
            n.style.corner_radius,
        )),
        _ => shape_path(n, wr),
    }
}

/// Where one copy of an image goes for a given mode.
pub fn image_place(mode: ImageMode, wr: Rect, iw: f64, ih: f64) -> Rect {
    let (iw, ih) = (iw.max(1.0), ih.max(1.0));
    match mode {
        ImageMode::Stretch => wr,
        ImageMode::Tile => Rect::new(wr.x, wr.y, iw, ih),
        ImageMode::Fill | ImageMode::Fit => {
            let s = if mode == ImageMode::Fill {
                (wr.w / iw).max(wr.h / ih)
            } else {
                (wr.w / iw).min(wr.h / ih)
            };
            let size = v2(iw * s, ih * s);
            let c = wr.center();
            Rect::new(c.x - size.x * 0.5, c.y - size.y * 0.5, size.x, size.y)
        }
    }
}

pub fn paint_spec(p: &Paint, wr: Rect, opacity: f64) -> Option<PaintSpec> {
    let at = |u: V2| v2(wr.x + u.x * wr.w, wr.y + u.y * wr.h);
    Some(match p {
        Paint::Solid { color, .. } => PaintSpec::Solid(rgba(*color, opacity)),
        Paint::Linear { stops, from, to } => PaintSpec::Linear {
            p0: at(*from),
            p1: at(*to),
            stops: sorted(stops),
            opacity: opacity as f32,
        },
        Paint::Radial {
            stops,
            center,
            radius,
        } => PaintSpec::Radial {
            center: at(*center),
            rx: v2(radius.x * wr.w, 0.0),
            ry: v2(0.0, radius.y * wr.h),
            stops: sorted(stops),
            opacity: opacity as f32,
        },
        Paint::Image {
            asset,
            mode,
            opacity: o,
        } => {
            let a = assets::get(asset);
            let (iw, ih) = a
                .as_ref()
                .map_or((wr.w, wr.h), |a| (a.width as f64, a.height as f64));
            if a.is_none() {
                // Missing asset: a neutral placeholder.
                return Some(PaintSpec::Solid(rgba(
                    Color::rgb(0xcc, 0xcc, 0xd2),
                    opacity * o,
                )));
            }
            PaintSpec::Image {
                asset: asset.clone(),
                place: image_place(*mode, wr, iw, ih),
                tile: *mode == ImageMode::Tile,
                opacity: (opacity * o) as f32,
            }
        }
    })
}

fn sorted(stops: &[Stop]) -> Vec<Stop> {
    let mut s = stops.to_vec();
    s.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    s
}

/// A soft shadow as nested translucent shapes, like CSS `box-shadow`: the blur is a Gaussian with
/// sigma = blur / 2, so it fades from the color's alpha to nothing across ±blur around the edge.
/// Each layer's alpha is chosen so the composited stack follows that curve. The shadow is cut out
/// under the box itself, as in CSS and Figma, so translucent fills don't darken.
fn shadow_ops(n: &Node, wr: Rect, opacity: f64, ops: &mut Vec<DrawOp>) {
    // ponytail: rings every ~3px, capped; wide blurs (>100px) step a little coarser.
    const MIN_LAYERS: usize = 6;
    const MAX_LAYERS: usize = 32;
    const RING_STEP: f64 = 3.0;
    let Some(s) = n.style.shadow else { return };
    if s.color.a == 0 || !is_area(n) || wr.w <= 0.0 || wr.h <= 0.0 {
        return;
    }
    let base = wr.translate(v2(s.x, s.y));
    let radii = if matches!(n.kind, NodeKind::Ellipse) {
        [wr.w.min(wr.h) * 0.5; 4]
    } else {
        n.style.corner_radii()
    };
    let solid = Color::rgb(s.color.r, s.color.g, s.color.b);
    let alpha = opacity * (s.color.a as f64 / 255.0);
    let blur = s.blur.max(0.5);
    let layers = ((2.0 * blur / RING_STEP).ceil() as usize).clamp(MIN_LAYERS, MAX_LAYERS);
    let step = 2.0 * blur / layers as f64;
    // Coverage at distance d outside the edge: alpha * (1 - Φ(d / σ)), Φ by its logistic fit.
    let coverage = |d: f64| alpha / (1.0 + (1.702 * d / (blur * 0.5)).exp());
    let hole = (!(s.x == 0.0 && s.y == 0.0 && s.spread >= 0.0))
        .then(|| contours(&vector::rounded_rect(wr, radii)));
    // Outermost layer first; `outer` is the coverage the layers outside this one already give.
    let mut outer = 0.0;
    for i in (0..layers).rev() {
        let e = s.spread - blur + step * (i as f64 + 1.0);
        let target = coverage(e - step * 0.5);
        let a = 1.0 - (1.0 - target) / (1.0 - outer);
        outer = target;
        let r = base.inflate(e);
        if r.w <= 0.0 || r.h <= 0.0 || a <= 0.0 {
            continue;
        }
        let ring = vector::rounded_rect(r, radii.map(|x| (x + e).max(0.0)));
        let (path, even_odd) = match &hole {
            // Centered and not shrunk: a layer either lies inside the box (hidden) or contains it,
            // so even-odd cuts the box out.
            None if e < 0.0 => continue,
            None => {
                let mut p = ring;
                p.segs.extend(vector::rounded_rect(wr, radii).segs);
                (p, true)
            }
            Some(h) => {
                use i_overlay::core::fill_rule::FillRule as OvFill;
                use i_overlay::core::overlay_rule::OverlayRule;
                use i_overlay::float::single::SingleFloatOverlay;
                let cut = contours(&ring).overlay(h, OverlayRule::Difference, OvFill::NonZero);
                let mut p = VPath::new();
                for c in cut.into_iter().flatten() {
                    let pts: Vec<V2> = c.iter().map(|q| v2(q[0], q[1])).collect();
                    p.segs.extend(VPath::polyline(&pts, true).segs);
                }
                (p, false)
            }
        };
        ops.push(DrawOp::Fill {
            path,
            paint: PaintSpec::Solid(rgba(solid, a)),
            even_odd,
        });
    }
}

fn contours(p: &VPath) -> Vec<Vec<[f64; 2]>> {
    p.flatten(0.25)
        .into_iter()
        .filter(|c| c.pts.len() >= 3)
        .map(|c| c.pts.iter().map(|q| [q.x, q.y]).collect())
        .collect()
}

/// Draw ops for one node (not its children), in world space.
pub fn node_draw(n: &Node, wr: Rect, opacity: f64) -> NodeDraw {
    let mut ops = Vec::new();
    if let Some(r) = n.style.rough {
        ops = super::rough::node_ops(n, wr, opacity, &r);
        return NodeDraw {
            ops,
            blend: n.style.blend,
        };
    }
    shadow_ops(n, wr, opacity, &mut ops);
    match &n.kind {
        NodeKind::Group => {}
        NodeKind::Text { .. } => text_ops(n, wr, opacity, &mut ops),
        NodeKind::Connector { .. } => connector_ops(n, wr, opacity, &mut ops, false),
        NodeKind::Sticky { .. } => {
            let c = n
                .style
                .fills
                .last()
                .map_or(Color::rgb(0xff, 0xdf, 0x6b), |p| p.preview_color());
            let rad = [n.style.corner_radius.max(2.0); 4];
            // Soft drop shadow, then the note and its text.
            for (i, a) in [(3.0, 0.05), (1.5, 0.07)] {
                ops.push(DrawOp::Fill {
                    path: vector::rounded_rect(wr.translate(v2(0.0, i)).inflate(i * 0.5), rad),
                    paint: PaintSpec::Solid(rgba(Color::rgba(0, 0, 0, (a * 255.0) as u8), opacity)),
                    even_odd: false,
                });
            }
            ops.push(DrawOp::Fill {
                path: vector::rounded_rect(wr, rad),
                paint: PaintSpec::Solid(rgba(c, opacity)),
                even_odd: false,
            });
            if let Some(sc) = n.style.stroke
                && n.style.stroke_width > 0.0
            {
                ops.push(DrawOp::Stroke {
                    path: vector::rounded_rect(wr, rad),
                    color: rgba(sc, opacity),
                    width: n.style.stroke_width,
                    cap: n.style.cap,
                    join: n.style.join,
                    dash: n.style.dash.clone(),
                });
            }
            text_ops(n, wr, opacity, &mut ops);
        }
        NodeKind::Line { .. } => {
            if let (Some(c), Some(path)) = (n.style.stroke, shape_path(n, wr)) {
                ops.push(DrawOp::Stroke {
                    path,
                    color: rgba(c, opacity),
                    width: n.style.stroke_width.max(0.0),
                    cap: if n.style.cap == Cap::Butt {
                        Cap::Round
                    } else {
                        n.style.cap
                    },
                    join: n.style.join,
                    dash: n.style.dash.clone(),
                });
            }
        }
        NodeKind::Stroke { .. } => {
            let c = n
                .style
                .fills
                .last()
                .map_or(Color::INK, |p| p.preview_color());
            if let Some(path) = shape_path(n, wr) {
                ops.push(DrawOp::Fill {
                    path,
                    paint: PaintSpec::Solid(rgba(c, opacity)),
                    even_odd: false,
                });
            }
        }
        _ => {
            let Some(path) = shape_path(n, wr) else {
                return NodeDraw::default();
            };
            let even_odd = matches!(
                n.kind,
                NodeKind::Path {
                    fill_rule: FillRule::EvenOdd,
                    ..
                }
            );
            if is_area(n) {
                for p in &n.style.fills {
                    if let Some(paint) = paint_spec(p, wr, opacity) {
                        ops.push(DrawOp::Fill {
                            path: path.clone(),
                            paint,
                            even_odd,
                        });
                    }
                }
            }
            // A clipping frame's border goes over its children (`border_draw`), as in CSS.
            if !border_on_top(n) {
                stroke_ops(n, wr, opacity, &mut ops);
            }
        }
    }
    NodeDraw {
        ops,
        blend: n.style.blend,
    }
}

/// A clipping frame with children draws its border after them, so filled children cannot hide it.
pub fn border_on_top(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Frame { clip: true }) && !n.children.is_empty() && n.style.stroke.is_some() && n.style.rough.is_none()
}

/// The border of a [`border_on_top`] frame, drawn after its children.
pub fn border_draw(n: &Node, wr: Rect, opacity: f64) -> NodeDraw {
    let mut ops = Vec::new();
    stroke_ops(n, wr, opacity, &mut ops);
    NodeDraw { ops, blend: n.style.blend }
}

/// A shape's stroke: per-side borders for frames and rectangles, else along its outline.
fn stroke_ops(n: &Node, wr: Rect, opacity: f64, ops: &mut Vec<DrawOp>) {
    if let Some(c) = n.style.stroke {
        if let Some(sides) = n.style.stroke_sides
            && matches!(n.kind, NodeKind::Frame { .. } | NodeKind::Rect)
            && sides.iter().any(|&w| (w - sides[0]).abs() > 1e-9)
        {
            let col = rgba(c, opacity);
            let side_specs = [
                (
                    sides[0],
                    Rect::new(wr.x, wr.y, wr.w, sides[0].min(wr.h)),
                    [
                        v2(wr.x, wr.y + sides[0] * 0.5),
                        v2(wr.right(), wr.y + sides[0] * 0.5),
                    ],
                ),
                (
                    sides[1],
                    Rect::new(
                        (wr.right() - sides[1]).max(wr.x),
                        wr.y,
                        sides[1].min(wr.w),
                        wr.h,
                    ),
                    [
                        v2(wr.right() - sides[1] * 0.5, wr.y),
                        v2(wr.right() - sides[1] * 0.5, wr.bottom()),
                    ],
                ),
                (
                    sides[2],
                    Rect::new(
                        wr.x,
                        (wr.bottom() - sides[2]).max(wr.y),
                        wr.w,
                        sides[2].min(wr.h),
                    ),
                    [
                        v2(wr.x, wr.bottom() - sides[2] * 0.5),
                        v2(wr.right(), wr.bottom() - sides[2] * 0.5),
                    ],
                ),
                (
                    sides[3],
                    Rect::new(wr.x, wr.y, sides[3].min(wr.w), wr.h),
                    [
                        v2(wr.x + sides[3] * 0.5, wr.y),
                        v2(wr.x + sides[3] * 0.5, wr.bottom()),
                    ],
                ),
            ];
            for (w, sr, line_pts) in side_specs {
                if w > 0.0 && sr.w > 0.0 && sr.h > 0.0 {
                    if !n.style.dash.is_empty() {
                        ops.push(DrawOp::Stroke {
                            path: VPath::polyline(&line_pts, false),
                            color: col,
                            width: w,
                            cap: Cap::Butt,
                            join: n.style.join,
                            dash: n.style.dash.clone(),
                        });
                    } else {
                        ops.push(DrawOp::Fill {
                            path: vector::rounded_rect(sr, [0.0; 4]),
                            paint: PaintSpec::Solid(col),
                            even_odd: false,
                        });
                    }
                }
            }
        } else if n.style.stroke_width > 0.0
            && let Some(sp) = stroke_path(n, wr)
        {
            ops.push(DrawOp::Stroke {
                path: sp,
                color: rgba(c, opacity),
                width: n.style.stroke_width,
                cap: n.style.cap,
                join: n.style.join,
                dash: n.style.dash.clone(),
            });
        }
    }
}

/// A connector's line and heads. With `rough`, only the heads are produced (the line comes
/// from the rough renderer).
pub fn connector_ops(n: &Node, wr: Rect, opacity: f64, ops: &mut Vec<DrawOp>, heads_only: bool) {
    let NodeKind::Connector {
        points,
        route,
        start_head,
        end_head,
        ..
    } = &n.kind
    else {
        return;
    };
    let Some(c) = n.style.stroke else { return };
    let color = rgba(c, opacity);
    let sw = n.style.stroke_width.max(0.5);
    let pts: Vec<V2> = points.iter().map(|p| wr.min() + *p).collect();
    if pts.len() < 2 {
        return;
    }
    if !heads_only {
        ops.push(DrawOp::Stroke {
            path: crate::connector::line_path(&pts, *route),
            color,
            width: sw,
            cap: Cap::Round,
            join: Join::Round,
            dash: n.style.dash.clone(),
        });
    }
    let last = pts.len() - 1;
    for (head, tip, from) in [
        (*start_head, pts[0], pts[1]),
        (*end_head, pts[last], pts[last - 1]),
    ] {
        let (lines, polys) = crate::connector::head_shapes(head, tip, from, sw, tip.dist(from));
        for l in lines {
            ops.push(DrawOp::Stroke {
                path: VPath::polyline(&l, false),
                color,
                width: sw,
                cap: Cap::Round,
                join: Join::Round,
                dash: Vec::new(),
            });
        }
        for p in polys {
            let path = VPath::polyline(&p, true);
            ops.push(DrawOp::Fill {
                path: path.clone(),
                paint: PaintSpec::Solid(color),
                even_odd: false,
            });
            ops.push(DrawOp::Stroke {
                path,
                color,
                width: sw,
                cap: Cap::Round,
                join: Join::Round,
                dash: Vec::new(),
            });
        }
    }
}

fn text_ops(n: &Node, wr: Rect, opacity: f64, ops: &mut Vec<DrawOp>) {
    let Some(l) = text::layout_node(n) else {
        return;
    };
    let o = wr.min() + text::text_offset(n);
    let mut i = 0;
    while i < l.glyphs.len() {
        let g0 = l.glyphs[i];
        let mut glyphs = Vec::new();
        while i < l.glyphs.len() {
            let g = &l.glyphs[i];
            if g.face != g0.face || g.size != g0.size || g.color != g0.color {
                break;
            }
            glyphs.push((g.gid, o + v2(g.x, g.baseline)));
            i += 1;
        }
        ops.push(DrawOp::Glyphs {
            face: g0.face,
            size: g0.size,
            color: rgba(g0.color, opacity),
            glyphs,
        });
    }
    // Underline / strikethrough as thin rectangles (or wavy stroke for wavy underlines).
    for g in l.glyphs.iter().filter(|g| g.underline || g.strike) {
        let th = (g.size / 14.0).max(0.5);
        if g.underline {
            let u_col = g.underline_color.unwrap_or(g.color);
            if g.underline_wavy {
                let x0 = o.x + g.x;
                let x1 = x0 + g.advance;
                let y0 = o.y + g.baseline + g.size * 0.14;
                let step = (g.size * 0.14).max(1.8);
                let amp = (g.size * 0.06).max(0.8);
                let mut pts = Vec::new();
                let mut x = x0;
                let mut k = 0usize;
                while x < x1 - 1e-6 {
                    let dy = if k.is_multiple_of(2) { -amp } else { amp };
                    pts.push(v2(x, y0 + dy));
                    x += step;
                    k += 1;
                }
                let dy = if k.is_multiple_of(2) { -amp } else { amp };
                pts.push(v2(x1, y0 + dy));
                if pts.len() >= 2 {
                    ops.push(DrawOp::Stroke {
                        path: VPath::polyline(&pts, false),
                        color: rgba(u_col, opacity),
                        width: th,
                        cap: Cap::Round,
                        join: Join::Round,
                        dash: Vec::new(),
                    });
                }
            } else {
                let r = Rect::new(o.x + g.x, o.y + g.baseline + g.size * 0.12, g.advance, th);
                ops.push(DrawOp::Fill {
                    path: vector::rounded_rect(r, [0.0; 4]),
                    paint: PaintSpec::Solid(rgba(u_col, opacity)),
                    even_odd: false,
                });
            }
        }
        if g.strike {
            let r = Rect::new(o.x + g.x, o.y + g.baseline - g.size * 0.28, g.advance, th);
            ops.push(DrawOp::Fill {
                path: vector::rounded_rect(r, [0.0; 4]),
                paint: PaintSpec::Solid(rgba(g.color, opacity)),
                even_odd: false,
            });
        }
    }
}

/// Product of ancestor opacities.
pub fn inherited_opacity(doc: &Document, id: NodeId) -> f64 {
    doc.ancestors(id)
        .iter()
        .filter_map(|a| doc.get(*a))
        .map(|n| n.style.opacity)
        .product()
}

/// World bounds a node draws into (rect plus stroke overhang).
pub fn draw_bounds(n: &Node, wr: Rect) -> Rect {
    let mut pad = match n.style.stroke_align {
        StrokeAlign::Inside => 0.0,
        StrokeAlign::Center => n.style.stroke_width * 0.5,
        StrokeAlign::Outside => n.style.stroke_width,
    };
    if n.style.stroke.is_none() && !matches!(n.kind, NodeKind::Line { .. }) {
        pad = 0.0;
    }
    if n.style.rough.is_some() {
        pad += n.style.stroke_width * 2.0 + 4.0;
    }
    if matches!(n.kind, NodeKind::Line { .. }) {
        pad = pad.max(n.style.stroke_width * 0.5);
    }
    if matches!(n.kind, NodeKind::Connector { .. }) {
        // Arrowheads reach past the end points.
        pad = pad.max(n.style.stroke_width + 20.0);
    }
    if matches!(n.kind, NodeKind::Sticky { .. }) {
        pad = pad.max(4.0);
    }
    if let Some(s) = n.style.shadow {
        pad = pad.max(s.spread + s.blur + s.x.abs().max(s.y.abs()));
    }
    wr.inflate(pad)
}
