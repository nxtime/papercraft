//! The files a site needs besides its pages: images (shrunk when far larger than shown), vector
//! drawings and the font faces its text uses. Everything is content-hashed, so a re-export
//! writes the same names and the same bytes.

use super::fontsub::{self, Fallback};
use super::style::FontFace;
use crate::model::{Document, NodeId, NodeKind, TextStyle};
use crate::text::{BUILT_IN_FACES, face_bytes, face_id};
use crate::{assets, fonts};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Cursor;

/// Pixels per displayed pixel kept in an image: 2x covers high-density screens.
const DENSITY: f64 = 2.0;
const JPEG_QUALITY: u8 = 85;
pub const INTER: &str = "Inter";

#[derive(Default)]
pub struct Media {
    /// Site-root relative path to bytes.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Image ids that are not loaded.
    pub missing: BTreeSet<String>,
    shrunk: HashMap<(String, u32), String>,
}

impl Media {
    fn put(&mut self, folder: &str, bytes: Vec<u8>, ext: &str) -> String {
        let path = format!("{folder}/{}.{ext}", assets::hash(&bytes));
        self.files.entry(path.clone()).or_insert(bytes);
        path
    }

    /// The path of image `id` for a picture shown `width` px wide: the original, or a copy
    /// scaled down to twice the width when it has more pixels than that. None when not loaded.
    pub fn image(&mut self, id: &str, width: f64) -> Option<String> {
        let Some(asset) = assets::get(id) else {
            self.missing.insert(id.to_owned());
            return None;
        };
        let target = (width * DENSITY).ceil();
        if width > 0.0
            && f64::from(asset.width) > target
            && let Some(path) = self.shrink(&asset, target as u32)
        {
            return Some(path);
        }
        Some(self.original(&asset))
    }

    /// The path of image `id` at its own size (a CSS background).
    pub fn background(&mut self, id: &str) -> Option<String> {
        let Some(asset) = assets::get(id) else {
            self.missing.insert(id.to_owned());
            return None;
        };
        Some(self.original(&asset))
    }

    fn original(&mut self, asset: &assets::ImageAsset) -> String {
        let path = format!("assets/{}.{}", asset.id, asset.ext);
        self.files
            .entry(path.clone())
            .or_insert_with(|| asset.bytes.clone());
        path
    }

    fn shrink(&mut self, asset: &assets::ImageAsset, width: u32) -> Option<String> {
        let key = (asset.id.clone(), width);
        if let Some(p) = self.shrunk.get(&key) {
            return Some(p.clone());
        }
        let bytes = scaled(asset, width)?;
        let path = self.put("assets", bytes, asset.ext);
        self.shrunk.insert(key, path.clone());
        Some(path)
    }

    /// A vector drawing as `assets/<hash>.svg`.
    pub fn svg(&mut self, svg: &str) -> String {
        // The exporter writes an inline `<svg>`; as a file it needs its namespace.
        let svg = if svg.contains("xmlns=") {
            svg.to_owned()
        } else {
            svg.replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1)
        };
        self.put("assets", svg.into_bytes(), "svg")
    }
}

/// The image re-encoded in its own format at `width`, if that is smaller than the original
/// (gif is left alone: re-encoding loses frames).
fn scaled(asset: &assets::ImageAsset, width: u32) -> Option<Vec<u8>> {
    use image::{DynamicImage, ImageFormat, codecs::jpeg::JpegEncoder, imageops::FilterType};
    let format = match asset.ext {
        "png" => ImageFormat::Png,
        "jpg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        _ => return None,
    };
    let height = ((f64::from(asset.height) * f64::from(width) / f64::from(asset.width)).round()
        as u32)
        .max(1);
    let small = image::imageops::resize(&asset.rgba()?, width, height, FilterType::Lanczos3);
    let mut out = Cursor::new(Vec::new());
    if format == ImageFormat::Jpeg {
        // JPEG has no alpha channel.
        let rgb = DynamicImage::ImageRgba8(small).to_rgb8();
        JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .encode_image(&rgb)
            .ok()?;
    } else {
        DynamicImage::ImageRgba8(small)
            .write_to(&mut out, format)
            .ok()?;
    }
    let bytes = out.into_inner();
    (bytes.len() < asset.bytes.len()).then_some(bytes)
}

/// The family a text layer is drawn in: its installed family, else Inter (which also stands in
/// for a family that is not installed, so the page matches the canvas).
pub fn family(style: &TextStyle) -> String {
    match style.font.as_deref() {
        Some(f) if !f.eq_ignore_ascii_case(INTER) => {
            fonts::styles(f).map_or_else(|| INTER.to_owned(), |(name, _)| name)
        }
        _ => INTER.to_owned(),
    }
}

/// Font families named by text that are not installed.
pub fn missing_families(doc: &Document, roots: &[NodeId]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for n in roots
        .iter()
        .flat_map(|r| doc.subtree(*r))
        .filter_map(|id| doc.get(id))
    {
        if let (NodeKind::Text { .. }, Some(f)) = (&n.kind, n.text_style.font.as_deref())
            && !f.eq_ignore_ascii_case(INTER)
            && fonts::styles(f).is_none()
        {
            out.insert(f.to_owned());
        }
    }
    out
}

/// What the text uses a face for.
#[derive(Default)]
pub struct FaceUse {
    /// Characters in the first document, to pick the body face.
    pub chars: usize,
    /// Every character of every document (language), to subset the face to.
    pub glyphs: BTreeSet<char>,
}

/// `(family, bold, italic)` of the faces the text uses, and what they are used for.
pub fn font_uses(docs: &[Document], roots: &[NodeId]) -> BTreeMap<(String, bool, bool), FaceUse> {
    let mut uses: BTreeMap<(String, bool, bool), FaceUse> = BTreeMap::new();
    for (i, doc) in docs.iter().enumerate() {
        for n in roots
            .iter()
            .flat_map(|r| doc.subtree(*r))
            .filter_map(|id| doc.get(id))
        {
            let NodeKind::Text { text, .. } = &n.kind else {
                continue;
            };
            let fam = family(&n.text_style);
            let mut keys = vec![(fam.clone(), n.text_style.bold, n.text_style.italic)];
            for s in &n.text_style.spans {
                keys.push((
                    fam.clone(),
                    s.bold.unwrap_or(n.text_style.bold),
                    s.italic.unwrap_or(n.text_style.italic),
                ));
            }
            for (k, key) in keys.into_iter().enumerate() {
                let e = uses.entry(key).or_default();
                e.glyphs.extend(text.chars());
                if k == 0 && i == 0 {
                    e.chars += text.chars().count();
                }
            }
        }
    }
    uses
}

/// A `@font-face` per face in `uses` (its file subset to the characters used, in `media`), the
/// index of the face with the most characters (the body text, worth preloading), a fallback face
/// per family and what went wrong on the way.
pub fn faces(
    media: &mut Media,
    uses: &BTreeMap<(String, bool, bool), FaceUse>,
) -> (Vec<FontFace>, Option<usize>, Vec<Fallback>, Vec<String>) {
    let mut out = Vec::new();
    let mut fallbacks: Vec<Fallback> = Vec::new();
    let mut warnings = Vec::new();
    let mut best: Option<(usize, usize)> = None;
    for ((family, bold, italic), used) in uses {
        let id = if family == INTER {
            face_id(*bold, *italic)
        } else {
            fonts::style_face(Some(family), *bold, *italic)
        };
        let bytes = if id < BUILT_IN_FACES {
            face_bytes(id)
        } else {
            fonts::loaded(id).bytes
        };
        let cff = ttf_parser::Face::parse(bytes, 0).is_ok_and(|f| f.tables().cff.is_some());
        let keep = fontsub::keep(&used.glyphs);
        let packed = fontsub::pack(bytes, &keep, cff).unwrap_or_else(|why| {
            warnings.push(format!(
                "font `{family}` could not be subset ({why}); the whole file is shipped"
            ));
            let (ext, format) = if cff {
                ("otf", "opentype")
            } else {
                ("ttf", "truetype")
            };
            fontsub::Packed {
                bytes: bytes.to_vec(),
                ext,
                format,
                note: None,
            }
        });
        if let Some(note) = packed.note {
            warnings.push(format!("font `{family}`: {note}"));
        }
        if !fallbacks
            .iter()
            .any(|f| f.family == fontsub::fallback_name(family))
        {
            let lower = family.to_lowercase();
            let mono = ["mono", "code", "courier", "consolas"]
                .iter()
                .any(|k| lower.contains(k));
            fallbacks.extend(fontsub::fallback(family, mono, bytes));
        }
        let path = media.put("assets/fonts", packed.bytes, packed.ext);
        if used.chars > 0 && best.is_none_or(|(_, c)| used.chars > c) {
            best = Some((out.len(), used.chars));
        }
        out.push(FontFace {
            family: family.clone(),
            weight: if *bold { 700 } else { 400 },
            italic: *italic,
            path,
            format: packed.format,
        });
    }
    (out, best.map(|b| b.0), fallbacks, warnings)
}

/// The CSS `font-family` value for a family: quoted, with a generic fallback so text shows
/// while the face loads.
pub fn font_stack(family: &str) -> String {
    let lower = family.to_lowercase();
    let generic = if ["mono", "code", "courier", "consolas"]
        .iter()
        .any(|k| lower.contains(k))
    {
        "ui-monospace, monospace"
    } else {
        "system-ui, sans-serif"
    };
    format!(
        "{}, {}, {generic}",
        crate::mcp::codegen::css_value("fontFamily", family),
        crate::mcp::codegen::css_string(&fontsub::fallback_name(family))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> String {
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x * 7 % 256) as u8, (y * 13 % 256) as u8, 90, 255])
        });
        assets::register_rgba(&img)
    }

    #[test]
    fn an_image_far_larger_than_shown_is_scaled_to_twice_its_size_and_the_rest_are_kept() {
        let id = png(400, 200);
        let mut m = Media::default();
        let small = m.image(&id, 100.0).unwrap();
        let (w, h) = {
            let img = image::load_from_memory(&m.files[&small]).unwrap();
            (img.width(), img.height())
        };
        assert_eq!((w, h), (200, 100));
        assert!(small.ends_with(".png") && small != format!("assets/{id}.png"));
        assert_eq!(m.image(&id, 100.0).unwrap(), small, "same size, same file");
        assert_eq!(
            m.image(&id, 300.0).unwrap(),
            format!("assets/{id}.png"),
            "under 2x is not resized"
        );
        assert!(m.image("nope", 10.0).is_none() && m.missing.contains("nope"));
    }

    #[test]
    fn a_family_that_is_not_installed_is_drawn_in_inter_and_stacks_have_fallbacks() {
        let style = TextStyle {
            font: Some("No Such Font".into()),
            ..TextStyle::default()
        };
        assert_eq!(family(&style), "Inter");
        assert_eq!(
            font_stack("Inter"),
            "\"Inter\", \"Inter Fallback\", system-ui, sans-serif"
        );
        assert!(font_stack("Fira Code").ends_with("ui-monospace, monospace"));
    }
}
