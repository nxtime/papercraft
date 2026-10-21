//! Bounded, pure-data Lottie shape animation player for host-tinted UI icons.
//!
//! Spatial position tangents (`ti`/`to`) are ignored: positions interpolate
//! linearly in space, with the specified temporal Bezier easing. Colours are
//! validated but discarded so the host can tint every stroke and fill.
//! Parent layers contribute transforms, while opacity comes from the drawn
//! layer, its shape groups, and each paint item. Stroke widths use the geometric
//! mean of the composed horizontal and vertical scales.
//!
//! Input numbers have magnitude at most one million; composed geometry and
//! transforms have a further numeric budget. Point estimates are conservative
//! over all keyframes and may reject animations whose sampled frames are smaller.
//! The 0.05 pixel cubic tessellation tolerance is subject to `f32` rounding.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod decode;
mod draw;
mod error;
mod eval;
mod model;

pub use draw::{Cap, FillPath, Frame, Item, Join, Polyline, StrokePath};
pub use error::Error;

/// Crate result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Named timeline interval, measured in absolute composition frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    /// Marker name.
    pub name: String,
    /// Absolute starting frame.
    pub frame: f32,
    /// Duration in frames.
    pub duration: f32,
}

/// Parsed and validated animation, with a conservative per-frame geometry budget.
#[derive(Debug, Clone)]
pub struct Animation {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) rate: f32,
    pub(crate) first: f32,
    pub(crate) last: f32,
    pub(crate) layers: Vec<model::Layer>,
    pub(crate) markers: Vec<Marker>,
}

impl Animation {
    /// Parse Bodymovin v5 JSON, rejecting unsupported features and invalid input.
    ///
    /// # Errors
    /// Returns [`Error::TooLarge`] above 512 KiB, [`Error::Json`] for malformed
    /// JSON, [`Error::Invalid`] for invalid values, and [`Error::Unsupported`]
    /// for features outside this subset or resource budgets.
    pub fn parse(json: &str) -> Result<Self> {
        decode::parse::animation(json)
    }

    /// Canvas width and height in pixels.
    #[must_use]
    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    /// Timeline frames per second.
    #[must_use]
    pub fn frame_rate(&self) -> f32 {
        self.rate
    }

    /// First absolute timeline frame.
    #[must_use]
    pub fn first_frame(&self) -> f32 {
        self.first
    }

    /// Last absolute timeline frame (the Lottie out point).
    #[must_use]
    pub fn last_frame(&self) -> f32 {
        self.last
    }

    /// Timeline duration in seconds.
    #[must_use]
    pub fn duration_secs(&self) -> f32 {
        (self.last - self.first) / self.rate
    }

    /// Look up a named marker.
    #[must_use]
    pub fn marker(&self, name: &str) -> Option<Marker> {
        self.markers
            .iter()
            .find(|marker| marker.name == name)
            .cloned()
    }

    /// Evaluate an absolute frame, clamped to the timeline bounds.
    /// Non-finite frame arguments evaluate the first frame.
    #[must_use]
    pub fn frame_at(&self, frame: f32) -> Frame {
        eval::frame(self, frame)
    }
}
