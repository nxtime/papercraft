//! Renderer-independent draw list, in animation pixel coordinates.

/// Stroke endpoint decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    /// Flat endpoint.
    Butt,
    /// Semicircular endpoint.
    Round,
    /// Square projecting endpoint.
    Square,
}

/// Stroke corner decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// Extended corner.
    Miter,
    /// Circular corner.
    Round,
    /// Clipped corner.
    Bevel,
}

/// Flattened path in animation pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    /// Vertices in traversal order; a closed path has an implicit closing edge.
    pub points: Vec<[f32; 2]>,
    /// Whether the final vertex connects to the first.
    pub closed: bool,
}

/// Host-tinted stroke geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokePath {
    /// Paths sharing this stroke.
    pub polylines: Vec<Polyline>,
    /// Stroke width in animation pixels, scaled by the composed transform's
    /// geometric mean scale (including reflections and nonuniform scales).
    pub width: f32,
    /// Endpoint decoration.
    pub cap: Cap,
    /// Corner decoration.
    pub join: Join,
    /// Combined layer, group and stroke opacity in `0..=1`.
    pub opacity: f32,
}

/// Host-tinted fill geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct FillPath {
    /// Paths sharing this fill.
    pub polylines: Vec<Polyline>,
    /// Combined layer, group and fill opacity in `0..=1`.
    pub opacity: f32,
}

/// One painter operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Stroke the paths.
    Stroke(StrokePath),
    /// Fill the paths using the nonzero winding rule.
    Fill(FillPath),
}

/// Evaluated draw list, in painter order (bottom first).
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Animation width in pixels.
    pub width: f32,
    /// Animation height in pixels.
    pub height: f32,
    /// Painter operations.
    pub items: Vec<Item>,
}
