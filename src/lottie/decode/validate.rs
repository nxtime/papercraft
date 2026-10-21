//! Conservative animation-wide bounds, parent validation, and output budgets.

use crate::lottie::error::{invalid, unsupported};
use crate::lottie::model::{PathTrack, Position, Shape, Transform};
use crate::lottie::{Animation, Result};

const MAX_POINTS: usize = 20_000;
const MAX_MAGNITUDE: f32 = 1_000_000_000.0;
const FLATNESS: f64 = 0.05;

#[derive(Clone, Copy)]
struct Bounds {
    scale: f32,
    extent: f32,
}

impl Bounds {
    fn transform(self, transform: &Transform, at: &str) -> Result<Self> {
        let local_scale = transform.scale.magnitude() / 100.0;
        let position = match &transform.position {
            Position::Combined(position) => position.magnitude(),
            Position::Split(x, y) => x.magnitude().max(y.magnitude()),
        };
        let scale = self.scale * local_scale;
        let extent = self.extent
            + self.scale * (position + local_scale * transform.anchor.magnitude()) * 2.0;
        if !scale.is_finite()
            || !extent.is_finite()
            || scale > MAX_MAGNITUDE
            || extent > MAX_MAGNITUDE
        {
            return Err(unsupported("transform numeric budget", at));
        }
        Ok(Self { scale, extent })
    }
}

pub(crate) fn animation(animation: &Animation) -> Result<()> {
    let mut total = 0_usize;
    for (index, layer) in animation.layers.iter().enumerate() {
        let at = format!("layer[{index}] (ind={})", layer.id);
        if animation
            .layers
            .iter()
            .filter(|candidate| candidate.id == layer.id)
            .count()
            != 1
        {
            return Err(invalid("duplicate layer ind", &at));
        }
        let mut visited = Vec::new();
        let mut chain = Vec::new();
        let mut current = Some(layer);
        while let Some(layer) = current {
            if visited.contains(&layer.id) {
                return Err(invalid("parent cycle", &at));
            }
            visited.push(layer.id);
            chain.push(&layer.transform);
            current = match layer.parent {
                Some(id) => Some(
                    animation
                        .layers
                        .iter()
                        .find(|candidate| candidate.id == id)
                        .ok_or_else(|| invalid("unknown parent ind", &at))?,
                ),
                None => None,
            };
        }
        let mut bounds = Bounds {
            scale: 1.0,
            extent: 0.0,
        };
        for transform in chain.into_iter().rev() {
            bounds = bounds.transform(transform, &at)?;
        }
        let (points, paths, styles, trims) = shapes(&layer.shapes, bounds, &at)?;
        let point_count = points.saturating_add(paths.saturating_mul(trims).saturating_mul(4));
        total = total.saturating_add(point_count.saturating_mul(styles.max(1)));
        if total > MAX_POINTS {
            return Err(unsupported("more than 20000 points per frame", &at));
        }
    }
    Ok(())
}

fn shapes(input: &[Shape], bounds: Bounds, at: &str) -> Result<(usize, usize, usize, usize)> {
    let (mut points, mut paths, mut styles, mut trims) = (0_usize, 0_usize, 0_usize, 0_usize);
    for shape in input {
        let count = match shape {
            Shape::Path(path) => {
                check_extent(path.track.magnitude(), bounds, at)?;
                path_points(path, bounds.scale)
            }
            Shape::Rectangle {
                position,
                size,
                radius,
                ..
            } => {
                let extent = position.magnitude() + size.magnitude() + radius.magnitude();
                check_extent(extent, bounds, at)?;
                // Eight edges, each with a control polygon bounded by the size.
                primitive_points(8, size.magnitude(), bounds.scale)
            }
            Shape::Ellipse { position, size, .. } => {
                check_extent(position.magnitude() + size.magnitude(), bounds, at)?;
                primitive_points(4, size.magnitude(), bounds.scale)
            }
            Shape::Group(children, transform) => {
                let (child_points, child_paths, child_styles, child_trims) =
                    shapes(children, bounds.transform(transform, at)?, at)?;
                points = points.saturating_add(child_points);
                paths = paths.saturating_add(child_paths);
                styles = styles.saturating_add(child_styles);
                trims = trims.saturating_add(child_trims);
                continue;
            }
            Shape::Stroke { width, .. } => {
                check_extent(width.magnitude(), bounds, at)?;
                styles = styles.saturating_add(1);
                continue;
            }
            Shape::Fill { .. } => {
                styles = styles.saturating_add(1);
                continue;
            }
            Shape::Trim { .. } => {
                trims = trims.saturating_add(1);
                continue;
            }
        };
        points = points.saturating_add(count);
        paths = paths.saturating_add(1);
    }
    Ok((points, paths, styles, trims))
}

fn check_extent(extent: f32, bounds: Bounds, at: &str) -> Result<()> {
    let maximum = bounds.extent + extent * bounds.scale * 4.0;
    if !maximum.is_finite() || maximum > MAX_MAGNITUDE {
        return Err(unsupported("geometry numeric budget", at));
    }
    Ok(())
}

fn primitive_points(edges: usize, size: f32, scale: f32) -> usize {
    // Every primitive control coordinate lies within a size-wide box. This
    // includes rounded rectangles whose radius saturates at half the size.
    let second = f64::from(size) * 8.0 * std::f64::consts::SQRT_2;
    edges
        .saturating_mul(segments(second, scale))
        .saturating_add(1)
}

fn path_points(path: &PathTrack, scale: f32) -> usize {
    if path.track.keys.len() == 1 {
        return crate::lottie::eval::flatten::estimate(
            &path.at(0.0),
            scale * std::f32::consts::SQRT_2,
        );
    }
    let values = path.track.bounds();
    let vertices: Vec<_> = values.as_chunks::<6>().0.iter().collect();
    let first = vertices.first().copied().filter(|_| path.closed);
    let mut count = 1_usize;
    for (start, end) in vertices
        .iter()
        .copied()
        .zip(vertices.iter().copied().skip(1).chain(first))
    {
        let ([vx, vy, _, _, ox, oy], [ex, ey, ix, iy, _, _]) = (start, end);
        let x = second_bound(*vx, *ex, *ox, *ix);
        let y = second_bound(*vy, *ey, *oy, *iy);
        count = count.saturating_add(segments(x.hypot(y), scale));
    }
    count
}

fn second_bound(start: [f32; 2], end: [f32; 2], outgoing: [f32; 2], incoming: [f32; 2]) -> f64 {
    let [start_min, start_max] = start.map(f64::from);
    let [end_min, end_max] = end.map(f64::from);
    let [out_min, out_max] = outgoing.map(f64::from);
    let [in_min, in_max] = incoming.map(f64::from);
    [
        end_min - start_max + in_min - 2.0 * out_max,
        end_max - start_min + in_max - 2.0 * out_min,
        start_min - end_max + out_min - 2.0 * in_max,
        start_max - end_min + out_max - 2.0 * in_min,
    ]
    .into_iter()
    .map(f64::abs)
    .fold(0.0, f64::max)
}

fn segments(second: f64, scale: f32) -> usize {
    // Rotation can align either axis: sqrt(2) bounds Euclidean scale from the
    // maximum component scale, including nonuniform parent compositions.
    let mut error = second * 0.75 * f64::from(scale) * std::f64::consts::SQRT_2;
    let mut points = 1_usize;
    for _ in 0..64 {
        if error <= FLATNESS {
            return points;
        }
        error *= 0.25;
        points = points.saturating_mul(2);
    }
    usize::MAX
}
