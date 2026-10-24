//! PNG export through the offscreen GPU renderer.

use super::svg::export_bounds;
use crate::geom::{Camera, Rect, V2, v2};
use crate::model::{Color, Document, NodeId};
use crate::render::gpu::Headless;
use crate::render::scene::{self, View};

/// Largest exported edge, in pixels.
pub const MAX_EDGE: f64 = 16384.0;

/// What the GPU has to draw for a PNG: the frame, its pixel size and the clear colour.
struct Plan {
    frame: scene::CanvasFrame,
    size: [u32; 2],
    clear: [f64; 4],
    transparent: bool,
}

fn plan(
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
    clip: Option<Rect>,
) -> Result<Plan, String> {
    let roots: Vec<NodeId> = roots
        .map(|r| r.to_vec())
        .unwrap_or_else(|| doc.root.clone());
    let b = export_bounds(doc, &roots).ok_or("nothing to export")?;
    let b = match clip {
        Some(c) => b.intersection(&c).ok_or("nothing to export")?,
        None => b,
    };
    let scale = scale.min(MAX_EDGE / b.w.max(b.h)).max(1e-3);
    let size = [
        (b.w * scale).ceil().max(1.0) as u32,
        (b.h * scale).ceil().max(1.0) as u32,
    ];
    // Tessellate at physical resolution: the camera zoom carries the scale.
    let v = View {
        doc,
        camera: Camera {
            pan: b.min(),
            zoom: scale,
        },
        viewport: v2(size[0] as f64, size[1] as f64),
        offset: V2::ZERO,
    };
    let frame = scene::export_frame(doc, Some(&roots), v.camera, v.viewport, None);
    let clear = match background {
        Some(c) => {
            let f = c.to_f32(1.0);
            [f[0] as f64, f[1] as f64, f[2] as f64, f[3] as f64]
        }
        None => [0.0; 4],
    };
    Ok(Plan {
        frame,
        size,
        clear,
        transparent: background.is_none(),
    })
}

/// Render `roots` (or all top-level nodes) at `scale` px per unit.
pub fn render(
    gpu: &Headless,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
) -> Result<image::RgbaImage, String> {
    render_clip(gpu, doc, roots, scale, background, None)
}

/// [`render`] limited to `clip` (world units): only the part of the roots inside it is drawn.
pub fn render_clip(
    gpu: &Headless,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
    clip: Option<Rect>,
) -> Result<image::RgbaImage, String> {
    let p = plan(doc, roots, scale, background, clip)?;
    let mut img = gpu.render_frame(&p.frame, p.size, 1.0, p.clear);
    if p.transparent {
        unpremultiply_edges(&mut img);
    }
    Ok(img)
}

/// [`render`] for the browser, where the GPU readback cannot block.
pub async fn render_async(
    gpu: &Headless,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
) -> Result<image::RgbaImage, String> {
    let p = plan(doc, roots, scale, background, None)?;
    let mut img = gpu
        .render_frame_async(&p.frame, p.size, 1.0, p.clear)
        .await?;
    if p.transparent {
        unpremultiply_edges(&mut img);
    }
    Ok(img)
}

/// The GPU blends into a transparent target with straight-alpha blending, which leaves color
/// channels multiplied by coverage at anti-aliased edges. Undo that so PNGs composite cleanly.
fn unpremultiply_edges(img: &mut image::RgbaImage) {
    for p in img.pixels_mut() {
        let a = p.0[3];
        if a > 0 && a < 255 {
            for c in 0..3 {
                p.0[c] = ((p.0[c] as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            }
        }
    }
}

pub fn export(
    path: &std::path::Path,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
) -> Result<(u32, u32), String> {
    let gpu = Headless::new()?;
    let img = render(&gpu, doc, roots, scale, background)?;
    img.save(path).map_err(|e| e.to_string())?;
    Ok(img.dimensions())
}
