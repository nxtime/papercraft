//! Evaluates validated shape layers into renderer-independent paths.

use std::ops::Range;

use crate::lottie::model::{Layer, Matrix, Position, Shape, Transform};
use crate::lottie::{Animation, FillPath, Frame, Item, Polyline, StrokePath};

pub(crate) mod flatten;
pub(crate) mod keyframes;
mod trim;

const PERCENT: f32 = 100.0;
const MAX_PARENT_DEPTH: usize = 64;

struct Geometry {
    paths: Vec<Polyline>,
    opacity: f32,
    scale: f32,
}

struct Paint<'a> {
    shape: &'a Shape,
    geometry: Range<usize>,
}

#[derive(Default)]
struct Scene<'a> {
    geometry: Vec<Geometry>,
    paints: Vec<Paint<'a>>,
}

pub(crate) fn frame(animation: &Animation, requested: f32) -> Frame {
    let time = if requested.is_finite() {
        requested.clamp(animation.first, animation.last)
    } else {
        animation.first
    };
    let mut output = Frame {
        width: animation.width,
        height: animation.height,
        items: Vec::new(),
    };
    for layer in animation.layers.iter().rev() {
        // `op` is exclusive, but the animation's own last frame must still show the layers that run to the end.
        let ends_here = time >= animation.last && layer.last >= animation.last;
        if layer.hidden || time < layer.first || (time >= layer.last && !ends_here) {
            continue;
        }
        let local_time = time - layer.start;
        let (local, opacity) = transform(&layer.transform, local_time);
        let matrix = parents(animation, layer, time).then(local);
        let mut scene = Scene::default();
        scene.collect(&layer.shapes, local_time, matrix, opacity);
        scene.paint(local_time, &mut output.items);
    }
    output
}

fn parents(animation: &Animation, layer: &Layer, time: f32) -> Matrix {
    let mut chain = Vec::new();
    let mut parent_id = layer.parent;
    for _ in 0..MAX_PARENT_DEPTH {
        let Some(parent) =
            parent_id.and_then(|id| animation.layers.iter().find(|candidate| candidate.id == id))
        else {
            break;
        };
        chain.push(parent);
        parent_id = parent.parent;
    }
    chain
        .into_iter()
        .rev()
        .fold(Matrix::IDENTITY, |matrix, parent| {
            matrix.then(transform(&parent.transform, time - parent.start).0)
        })
}

fn transform(input: &Transform, time: f32) -> (Matrix, f32) {
    let [ax, ay] = pair(&input.anchor.at(time));
    let [px, py] = match &input.position {
        Position::Combined(position) => pair(&position.at(time)),
        Position::Split(x, y) => [x.scalar(time), y.scalar(time)],
    };
    let [sx, sy] = pair(&input.scale.at(time)).map(|value| value / PERCENT);
    let (sin, cos) = input.rotation.scalar(time).to_radians().sin_cos();
    let [a, b, c, d] = [cos * sx, sin * sx, -sin * sy, cos * sy];
    (
        Matrix([a, b, c, d, px - a * ax - c * ay, py - b * ax - d * ay]),
        input.opacity.scalar(time) / PERCENT,
    )
}

fn pair(values: &[f32]) -> [f32; 2] {
    let mut values = values.iter().copied();
    [
        values.next().unwrap_or_default(),
        values.next().unwrap_or_default(),
    ]
}

impl<'a> Scene<'a> {
    fn collect(&mut self, shapes: &'a [Shape], time: f32, matrix: Matrix, opacity: f32) {
        let first = self.geometry.len();
        for shape in shapes {
            let bezier = match shape {
                Shape::Path(path) => path.at(time),
                Shape::Rectangle {
                    position,
                    size,
                    radius,
                    reversed,
                } => {
                    let mut path = flatten::rectangle(
                        pair(&position.at(time)),
                        pair(&size.at(time)),
                        radius.scalar(time),
                    );
                    if *reversed {
                        reverse(&mut path);
                    }
                    path
                }
                Shape::Ellipse {
                    position,
                    size,
                    reversed,
                } => {
                    let mut path = flatten::ellipse(pair(&position.at(time)), pair(&size.at(time)));
                    if *reversed {
                        reverse(&mut path);
                    }
                    path
                }
                Shape::Group(children, input) => {
                    let (local, alpha) = transform(input, time);
                    self.collect(children, time, matrix.then(local), opacity * alpha);
                    continue;
                }
                Shape::Stroke { .. } | Shape::Fill { .. } => {
                    self.paints.push(Paint {
                        shape,
                        geometry: first..self.geometry.len(),
                    });
                    continue;
                }
                Shape::Trim { start, end, offset } => {
                    self.trim(
                        first,
                        start.scalar(time),
                        end.scalar(time),
                        offset.scalar(time),
                    );
                    continue;
                }
            };
            let [a, b, c, d, _, _] = matrix.0;
            self.geometry.push(Geometry {
                paths: vec![flatten::path(&bezier, matrix)],
                opacity,
                scale: (a * d - b * c).abs().sqrt(),
            });
        }
    }

    fn trim(&mut self, first: usize, start: f32, end: f32, offset: f32) {
        for geometry in self.geometry.iter_mut().skip(first) {
            let paths = std::mem::take(&mut geometry.paths)
                .into_iter()
                .enumerate()
                .collect();
            geometry.paths = trim::apply_tagged(paths, start, end, offset)
                .into_iter()
                .map(|(_, path)| path)
                .collect();
        }
    }

    fn paint(&self, time: f32, items: &mut Vec<Item>) {
        for paint in self.paints.iter().rev() {
            let is_fill = matches!(paint.shape, Shape::Fill { .. });
            for batch in self
                .geometry
                .get(paint.geometry.clone())
                .unwrap_or_default()
                .chunk_by(|a, b| {
                    a.opacity.to_bits() == b.opacity.to_bits()
                        && (is_fill || a.scale.to_bits() == b.scale.to_bits())
                })
            {
                let Some(geometry) = batch.first() else {
                    continue;
                };
                if geometry.opacity <= 0.0 {
                    continue;
                }
                let polylines: Vec<_> = batch
                    .iter()
                    .flat_map(|geometry| geometry.paths.iter().cloned())
                    .collect();
                if polylines.is_empty() {
                    continue;
                }
                let item = match paint.shape {
                    Shape::Stroke {
                        width,
                        opacity,
                        cap,
                        join,
                    } => {
                        let width = width.scalar(time) * geometry.scale;
                        let opacity = opacity.scalar(time) / PERCENT * geometry.opacity;
                        if width <= 0.0 || opacity <= 0.0 {
                            continue;
                        }
                        Item::Stroke(StrokePath {
                            polylines,
                            width,
                            opacity,
                            cap: *cap,
                            join: *join,
                        })
                    }
                    Shape::Fill { opacity } => {
                        let opacity = opacity.scalar(time) / PERCENT * geometry.opacity;
                        if opacity <= 0.0 {
                            continue;
                        }
                        Item::Fill(FillPath { polylines, opacity })
                    }
                    _ => continue,
                };
                items.push(item);
            }
        }
    }
}

fn reverse(path: &mut crate::lottie::model::Bezier) {
    path.vertices.reverse();
    path.incoming.reverse();
    path.outgoing.reverse();
    std::mem::swap(&mut path.incoming, &mut path.outgoing);
}

#[cfg(test)]
mod tests;
