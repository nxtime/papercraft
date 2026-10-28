//! Pixel-grid helpers for thin icon strokes: a line centred between two device pixels is drawn
//! as a soft double-width smear, which reads as aliasing. Snapping the stroke to whole device
//! pixels and its centre to the grid keeps horizontal and vertical glyphs sharp.

use egui::{Pos2, Stroke};

/// A stroke of about `width` points, rounded to a whole number of device pixels (at least one),
/// and the number of device pixels it covers.
pub fn stroke(ppp: f32, width: f32, color: egui::Color32) -> (Stroke, u32) {
    let px = (width * ppp).round().max(1.0);
    (Stroke::new(px / ppp, color), px as u32)
}

/// Snaps `v` (points) so a line `px` device pixels wide centred on it covers whole pixels: odd
/// widths are centred on a pixel's middle, even widths on a pixel edge.
pub fn snap(v: f32, ppp: f32, px: u32) -> f32 {
    let offset = if px % 2 == 1 { 0.5 } else { 0.0 };
    ((v * ppp - offset).round() + offset) / ppp
}

pub fn snap_pos(p: Pos2, ppp: f32, px: u32) -> Pos2 {
    Pos2::new(snap(p.x, ppp, px), snap(p.y, ppp, px))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_covers_whole_device_pixels() {
        let (s, px) = stroke(1.5, 1.25, egui::Color32::WHITE);
        assert_eq!(px, 2);
        assert!((s.width * 1.5 - 2.0).abs() < 1e-5);
        assert_eq!(stroke(1.0, 0.2, egui::Color32::WHITE).1, 1, "never thinner than a pixel");
    }

    #[test]
    fn odd_widths_centre_on_pixel_middles_even_on_edges() {
        let on_grid = |v: f32, ppp: f32, off: f32| ((v * ppp - off) - (v * ppp - off).round()).abs() < 1e-4;
        assert!(on_grid(snap(10.3, 1.0, 1), 1.0, 0.5));
        assert!(on_grid(snap(10.3, 1.5, 2), 1.5, 0.0));
        assert_eq!(snap(10.0, 2.0, 2), 10.0);
    }
}
