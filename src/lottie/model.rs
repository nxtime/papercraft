//! Validated animation data and affine transforms.

use crate::lottie::{Cap, Join};

#[derive(Debug, Clone)]
pub(crate) struct Track {
    pub keys: Vec<Key>,
}

#[derive(Debug, Clone)]
pub(crate) struct Key {
    pub time: f32,
    pub value: Vec<f32>,
    pub incoming: Vec<[f32; 2]>,
    pub outgoing: Vec<[f32; 2]>,
    pub hold: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Bezier {
    pub closed: bool,
    pub vertices: Vec<[f32; 2]>,
    pub incoming: Vec<[f32; 2]>,
    pub outgoing: Vec<[f32; 2]>,
}

#[derive(Debug, Clone)]
pub(crate) struct PathTrack {
    pub track: Track,
    pub closed: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Position {
    Combined(Track),
    Split(Track, Track),
}

#[derive(Debug, Clone)]
pub(crate) struct Transform {
    pub anchor: Track,
    pub position: Position,
    pub scale: Track,
    pub rotation: Track,
    pub opacity: Track,
}

#[derive(Debug, Clone)]
pub(crate) enum Shape {
    Group(Vec<Self>, Transform),
    Path(PathTrack),
    Rectangle {
        position: Track,
        size: Track,
        radius: Track,
        reversed: bool,
    },
    Ellipse {
        position: Track,
        size: Track,
        reversed: bool,
    },
    Stroke {
        width: Track,
        opacity: Track,
        cap: Cap,
        join: Join,
    },
    Fill {
        opacity: Track,
    },
    Trim {
        start: Track,
        end: Track,
        offset: Track,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Layer {
    pub id: i64,
    pub parent: Option<i64>,
    pub first: f32,
    pub last: f32,
    pub start: f32,
    pub hidden: bool,
    pub transform: Transform,
    pub shapes: Vec<Shape>,
}

/// Affine matrix `[a, b, c, d, tx, ty]`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Matrix(pub [f32; 6]);

impl Matrix {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn apply(self, [x, y]: [f32; 2]) -> [f32; 2] {
        let [xx, yx, xy, yy, tx, ty] = self.0;
        [xx * x + xy * y + tx, yx * x + yy * y + ty]
    }

    pub fn then(self, child: Self) -> Self {
        let [xx, yx, xy, yy, tx, ty] = self.0;
        let [cxx, cyx, cxy, cyy, ux, uy] = child.0;
        Self([
            xx * cxx + xy * cyx,
            yx * cxx + yy * cyx,
            xx * cxy + xy * cyy,
            yx * cxy + yy * cyy,
            xx * ux + xy * uy + tx,
            yx * ux + yy * uy + ty,
        ])
    }

    pub fn scale(self) -> f32 {
        let [a, b, c, d, _, _] = self.0;
        // Frobenius norm bounds the largest singular value.
        (a * a + b * b + c * c + d * d).sqrt()
    }
}
