//! Rotated drawing: a node's cached triangles are built upright and turned afterwards, so the
//! cache stays valid whatever the rotation of the node or its ancestors.

use super::tess::Piece;
use crate::geom::{Rect, V2, bounds_of, v2};
use crate::model::rotation::{Rot, apply_chain};

/// `pieces` as drawn under `chain` (the rotations of the node and its ancestors, page first).
pub fn rotate_pieces(pieces: Vec<Piece>, chain: &[Rot]) -> Vec<Piece> {
    if chain.is_empty() {
        return pieces;
    }
    pieces
        .into_iter()
        .map(|mut p| {
            for v in &mut p.mesh.vertices {
                let q = apply_chain(chain, v2(v.pos[0] as f64, v.pos[1] as f64));
                v.pos = [q.x as f32, q.y as f32];
            }
            p
        })
        .collect()
}

/// Bounding box of `r` as drawn under `chain`.
pub fn rotated_bounds(r: Rect, chain: &[Rot]) -> Rect {
    if chain.is_empty() {
        return r;
    }
    let corners = r.corners().map(|p| apply_chain(chain, p));
    bounds_of(&corners).unwrap_or(r)
}

/// `r` as drawn under `chain`, as an upright rectangle around the moved centre and the total
/// turn in degrees: what the layer shader needs to mask a rotated shape.
pub fn rotated_shape(r: Rect, chain: &[Rot]) -> (Rect, f64) {
    let c: V2 = apply_chain(chain, r.center());
    let deg = chain.iter().map(|x| x.deg).sum();
    (Rect::new(c.x - r.w * 0.5, c.y - r.h * 0.5, r.w, r.h), deg)
}

/// The chain below a node: its ancestors' rotations plus its own turn around `wr`'s centre.
pub fn extend_chain(chain: &[Rot], deg: f64, wr: &Rect) -> Vec<Rot> {
    let mut out = chain.to_vec();
    if deg != 0.0 {
        out.push(Rot { centre: wr.center(), deg });
    }
    out
}

