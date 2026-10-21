//! Cubic path flattening and primitive shape construction.

use crate::lottie::Polyline;
use crate::lottie::model::{Bezier, Matrix};

const TOLERANCE: f64 = 0.05;
const ELLIPSE_TANGENT: f32 = 0.552_284_8;
const MAX_SUBDIVISION_DEPTH: u32 = 256;
type Cubic = [[f32; 2]; 4];

/// Flattens a validated path to at most 0.05 transformed pixels of cubic error.
pub(crate) fn path(shape: &Bezier, matrix: Matrix) -> Polyline {
    let mut points = Vec::new();
    let scale = matrix.scale();
    if let Some(first) = shape.vertices.first() {
        points.push(matrix.apply(*first));
    }
    for curve in curves(shape) {
        let remaining = depth(curve, scale);
        let mut pending = vec![(curve.map(|point| matrix.apply(point)), remaining)];
        while let Some((curve, remaining)) = pending.pop() {
            let [a, b, c, d] = curve;
            if remaining == 0 || distance(b, a, d).max(distance(c, a, d)) <= TOLERANCE {
                points.push(d);
                continue;
            }
            let ab = midpoint(a, b);
            let bc = midpoint(b, c);
            let cd = midpoint(c, d);
            let abc = midpoint(ab, bc);
            let bcd = midpoint(bc, cd);
            let center = midpoint(abc, bcd);
            pending.push(([center, bcd, cd, d], remaining - 1));
            pending.push(([a, ab, abc, center], remaining - 1));
        }
    }
    Polyline {
        points,
        closed: shape.closed,
    }
}

/// Bounds output points for a transform whose operator norm is at most `scale`.
pub(crate) fn estimate(shape: &Bezier, scale: f32) -> usize {
    curves(shape).fold(usize::from(!shape.vertices.is_empty()), |total, curve| {
        total.saturating_add(
            1_usize
                .checked_shl(depth(curve, scale))
                .unwrap_or(usize::MAX),
        )
    })
}

fn curves(shape: &Bezier) -> impl Iterator<Item = Cubic> + '_ {
    let nodes = shape
        .vertices
        .iter()
        .zip(&shape.incoming)
        .zip(&shape.outgoing)
        .map(|((vertex, incoming), outgoing)| (*vertex, *incoming, *outgoing));
    let final_node = nodes.clone().next().filter(|_| shape.closed);
    nodes
        .clone()
        .zip(nodes.skip(1).chain(final_node))
        .map(|((a, _, outgoing), (d, incoming, _))| [a, add(a, outgoing), add(d, incoming), d])
}

fn depth([a, b, c, d]: Cubic, scale: f32) -> u32 {
    let second = |[ax, ay]: [f32; 2], [bx, by]: [f32; 2], [cx, cy]: [f32; 2]| {
        (f64::from(ax) - 2.0 * f64::from(bx) + f64::from(cx))
            .hypot(f64::from(ay) - 2.0 * f64::from(by) + f64::from(cy))
    };
    // Linear interpolation error is bounded by max |B''| / 8.
    let mut error = 0.75 * second(a, b, c).max(second(b, c, d)) * f64::from(scale.abs());
    let mut depth = 0;
    while error > TOLERANCE && depth < MAX_SUBDIVISION_DEPTH {
        error *= 0.25;
        depth += 1;
    }
    depth
}

fn add([ax, ay]: [f32; 2], [bx, by]: [f32; 2]) -> [f32; 2] {
    [ax + bx, ay + by]
}

fn midpoint([ax, ay]: [f32; 2], [bx, by]: [f32; 2]) -> [f32; 2] {
    [ax * 0.5 + bx * 0.5, ay * 0.5 + by * 0.5]
}

fn distance([px, py]: [f32; 2], [ax, ay]: [f32; 2], [bx, by]: [f32; 2]) -> f64 {
    let dx = f64::from(bx) - f64::from(ax);
    let dy = f64::from(by) - f64::from(ay);
    let x = f64::from(px) - f64::from(ax);
    let y = f64::from(py) - f64::from(ay);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        ((x * dx + y * dy) / length).clamp(0.0, 1.0)
    };
    (x - dx * t).hypot(y - dy * t)
}

/// Creates a clockwise rectangle, limiting corner radius to its half dimensions.
pub(crate) fn rectangle([x, y]: [f32; 2], [width, height]: [f32; 2], radius: f32) -> Bezier {
    let left = x - width * 0.5;
    let right = x + width * 0.5;
    let top = y - height * 0.5;
    let bottom = y + height * 0.5;
    let r = radius.min(width.abs() * 0.5).min(height.abs() * 0.5);
    if r == 0.0 {
        return Bezier {
            closed: true,
            vertices: vec![[left, top], [right, top], [right, bottom], [left, bottom]],
            incoming: vec![[0.0; 2]; 4],
            outgoing: vec![[0.0; 2]; 4],
        };
    }
    let tangent = r * ELLIPSE_TANGENT;
    Bezier {
        closed: true,
        vertices: vec![
            [left + r, top],
            [right - r, top],
            [right, top + r],
            [right, bottom - r],
            [right - r, bottom],
            [left + r, bottom],
            [left, bottom - r],
            [left, top + r],
        ],
        incoming: vec![
            [-tangent, 0.0],
            [0.0; 2],
            [0.0, -tangent],
            [0.0; 2],
            [tangent, 0.0],
            [0.0; 2],
            [0.0, tangent],
            [0.0; 2],
        ],
        outgoing: vec![
            [0.0; 2],
            [tangent, 0.0],
            [0.0; 2],
            [0.0, tangent],
            [0.0; 2],
            [-tangent, 0.0],
            [0.0; 2],
            [0.0, -tangent],
        ],
    }
}

/// Creates the standard four-cubic clockwise Lottie ellipse.
pub(crate) fn ellipse([x, y]: [f32; 2], [width, height]: [f32; 2]) -> Bezier {
    let rx = width * 0.5;
    let ry = height * 0.5;
    let tx = rx * ELLIPSE_TANGENT;
    let ty = ry * ELLIPSE_TANGENT;
    Bezier {
        closed: true,
        vertices: vec![[x, y - ry], [x + rx, y], [x, y + ry], [x - rx, y]],
        incoming: vec![[-tx, 0.0], [0.0, -ty], [tx, 0.0], [0.0, ty]],
        outgoing: vec![[tx, 0.0], [0.0, ty], [-tx, 0.0], [0.0, -ty]],
    }
}

#[cfg(test)]
mod tests {
    use super::{ellipse, estimate, path, rectangle};
    use crate::lottie::model::{Bezier, Matrix};

    fn identity() -> Matrix {
        Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    }

    #[test]
    fn straight_path_keeps_endpoints_and_transform() {
        let shape = Bezier {
            closed: false,
            vertices: vec![[0.0, 0.0], [10.0, 0.0]],
            incoming: vec![[0.0; 2]; 2],
            outgoing: vec![[0.0; 2]; 2],
        };
        let result = path(&shape, Matrix([2.0, 0.0, 0.0, 2.0, 3.0, 4.0]));
        assert_eq!(result.points, vec![[3.0, 4.0], [23.0, 4.0]]);
        assert!(!result.closed);
    }

    #[test]
    fn curved_path_deviates_less_than_tolerance() {
        let shape = Bezier {
            closed: false,
            vertices: vec![[0.0, 0.0], [100.0, 0.0]],
            incoming: vec![[0.0, 0.0], [0.0, 100.0]],
            outgoing: vec![[0.0, 100.0], [0.0, 0.0]],
        };
        let result = path(&shape, identity());
        assert!(result.points.len() > 10);
        assert!(estimate(&shape, 1.0) >= result.points.len());
        for step in 0..=1000 {
            let parameter = f64::from(step) / 1000.0;
            let x = 100.0 * parameter * parameter * (3.0 - 2.0 * parameter);
            let y = 300.0 * parameter * (1.0 - parameter);
            let distance = result
                .points
                .windows(2)
                .map(|pair| {
                    let [a, b] = pair else {
                        return f64::INFINITY;
                    };
                    let dx = f64::from(b[0] - a[0]);
                    let dy = f64::from(b[1] - a[1]);
                    let fraction = (((x - f64::from(a[0])) * dx + (y - f64::from(a[1])) * dy)
                        / (dx * dx + dy * dy))
                        .clamp(0.0, 1.0);
                    (x - f64::from(a[0]) - dx * fraction).hypot(y - f64::from(a[1]) - dy * fraction)
                })
                .fold(f64::INFINITY, f64::min);
            assert!(distance <= 0.05, "deviation {distance}");
        }
    }

    #[test]
    fn collinear_cubic_preserves_reversal() {
        let shape = Bezier {
            closed: false,
            vertices: vec![[0.0; 2], [1.0, 0.0]],
            incoming: vec![[0.0; 2], [-101.0, 0.0]],
            outgoing: vec![[100.0, 0.0], [0.0; 2]],
        };
        let result = path(&shape, identity());
        assert!(result.points.iter().any(|[x, _]| *x > 20.0));
        assert!(result.points.iter().any(|[x, _]| *x < -20.0));
    }

    #[test]
    fn cancelling_translation_retains_subpixel_curve_precision() {
        let mut shape = Bezier {
            closed: false,
            vertices: vec![[0.0, 0.0], [100.0, 0.0]],
            incoming: vec![[0.0, 0.0], [0.0, 100.0]],
            outgoing: vec![[0.0, 100.0], [0.0, 0.0]],
        };
        let expected = path(&shape, identity());
        for vertex in &mut shape.vertices {
            *vertex = vertex.map(|coordinate| coordinate + 10_000_000.0);
        }
        let translated = path(
            &shape,
            Matrix([1.0, 0.0, 0.0, 1.0, -10_000_000.0, -10_000_000.0]),
        );
        assert_eq!(translated.points, expected.points);
    }

    #[test]
    fn shapes_have_expected_extents_and_closed_edges() {
        for shape in [
            rectangle([10.0, 20.0], [40.0, 20.0], 4.0),
            ellipse([10.0, 20.0], [40.0, 20.0]),
        ] {
            let result = path(&shape, identity());
            assert!(result.closed);
            assert!(
                result
                    .points
                    .iter()
                    .all(|[x, y]| (-10.0..=30.0).contains(x) && (10.0..=30.0).contains(y))
            );
            assert_eq!(result.points.first(), result.points.last());
            assert!(estimate(&shape, 1.0) >= result.points.len());
        }
    }
}
