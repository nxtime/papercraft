//! Shrinks the font files a site ships: a subset with the characters the pages use, packed as
//! WOFF2, and the metrics of a matching system-font fallback so the swap does not shift layout.

use fontcull_klippa::{DEFAULT_LAYOUT_FEATURES, Plan, SubsetFlags, subset_font};
use fontcull_read_fonts::collections::IntSet;
use fontcull_skrifa::{FontRef, GlyphId, Tag};
use fontcull_write_fonts::types::NameId;
use std::collections::{BTreeSet, HashSet};

/// Always kept: the space, the no-break space, the ellipsis `text-overflow` draws with the
/// element's font and the hyphen browsers insert when they hyphenate.
const ALWAYS: [char; 4] = [' ', '\u{a0}', '\u{2026}', '-'];

/// Relative English letter frequencies (a-z) and the space, to weigh the average glyph width the
/// way Capsize does for `xWidthAvg`.
const WEIGHTS: [(char, f64); 27] = [
    ('a', 8.167),
    ('b', 1.492),
    ('c', 2.782),
    ('d', 4.253),
    ('e', 12.702),
    ('f', 2.228),
    ('g', 2.015),
    ('h', 6.094),
    ('i', 6.966),
    ('j', 0.153),
    ('k', 0.772),
    ('l', 4.025),
    ('m', 2.406),
    ('n', 6.749),
    ('o', 7.507),
    ('p', 1.929),
    ('q', 0.095),
    ('r', 5.987),
    ('s', 6.327),
    ('t', 9.056),
    ('u', 2.758),
    ('v', 0.978),
    ('w', 2.360),
    ('x', 0.150),
    ('y', 1.974),
    ('z', 0.074),
    (' ', 18.0),
];

/// Average width (em) of [`WEIGHTS`] in Arial and Courier New, the faces `local()` finds on
/// Windows, macOS and most Linux setups, measured with the same weights on their own files.
const ARIAL: SystemFont = SystemFont {
    name: "Arial",
    width: 0.446_97,
};
const COURIER: SystemFont = SystemFont {
    name: "Courier New",
    width: 0.600_10,
};

struct SystemFont {
    name: &'static str,
    width: f64,
}

/// A `@font-face` named `<family> Fallback` that draws a system font at the web font's metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct Fallback {
    pub family: String,
    pub local: &'static str,
    /// Percentages, as CSS wants them.
    pub size_adjust: f64,
    pub ascent: f64,
    pub descent: f64,
    pub gap: f64,
}

pub fn fallback_name(family: &str) -> String {
    format!("{family} Fallback")
}

/// The fallback face for `bytes`, from its hhea metrics and weighted average width. This is the
/// approach of Next.js `next/font`, Fontaine and Capsize: scale a system font to the web font's
/// average width (`size-adjust`), then divide its vertical metrics by that scale.
/// None when the font has no usable metrics.
pub fn fallback(family: &str, mono: bool, bytes: &[u8]) -> Option<Fallback> {
    let face = ttf_parser::Face::parse(bytes, 0).ok()?;
    let em = f64::from(face.units_per_em());
    let (mut sum, mut weight) = (0.0, 0.0);
    for (c, w) in WEIGHTS {
        let advance = face
            .glyph_index(c)
            .and_then(|g| face.glyph_hor_advance(g))?;
        sum += w * f64::from(advance) / em;
        weight += w;
    }
    let width = sum / weight;
    let sys = if mono { COURIER } else { ARIAL };
    if width <= 0.0 || em <= 0.0 {
        return None;
    }
    let scale = width / sys.width;
    let pct = |v: f64| (v * 1000.0).round() / 10.0;
    Some(Fallback {
        family: fallback_name(family),
        local: sys.name,
        size_adjust: pct(scale),
        ascent: pct(f64::from(face.ascender()) / em / scale),
        descent: pct(f64::from(-face.descender()) / em / scale),
        gap: pct(f64::from(face.line_gap()) / em / scale),
    })
}

/// The characters to keep for text that uses `chars`: those, their other case (CSS
/// `text-transform`) and [`ALWAYS`].
pub fn keep(chars: &BTreeSet<char>) -> HashSet<char> {
    let mut out: HashSet<char> = ALWAYS.into_iter().collect();
    for c in chars {
        out.insert(*c);
        out.extend(c.to_uppercase());
        out.extend(c.to_lowercase());
    }
    out
}

/// A font file ready to ship: `ext`/`format` describe `bytes`.
pub struct Packed {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
    pub format: &'static str,
    /// What had to be given up to keep the file valid.
    pub note: Option<&'static str>,
}

/// One klippa (the Rust port of HarfBuzz's subsetter, as vendored by `fontcull`) run, dropping
/// `drop` tables.
fn run_klippa(bytes: &[u8], chars: &HashSet<char>, drop: &[Tag]) -> Result<Vec<u8>, String> {
    let font = FontRef::new(bytes).map_err(|e| format!("{e:?}"))?;
    let unicodes: IntSet<u32> = chars.iter().map(|c| *c as u32).collect();
    let features: IntSet<Tag> = DEFAULT_LAYOUT_FEATURES.iter().copied().collect();
    let plan = Plan::new(
        &IntSet::<GlyphId>::empty(),
        &unicodes,
        &font,
        SubsetFlags::default(),
        &drop.iter().copied().collect(),
        &IntSet::<Tag>::all(),
        &features,
        &IntSet::<NameId>::empty(),
        &IntSet::<u16>::empty(),
    );
    subset_font(&font, &plan).map_err(|e| format!("{e:?}"))
}

/// Whether every lookup of GPOS and GSUB parses to its last subtable. klippa 0.1.2 writes an
/// unreadable GPOS for fonts with many kerning pairs (Inter), which browsers' sanitizers then
/// discard, so the check must be ours.
fn layout_is_sound(font: &[u8]) -> bool {
    let Ok(face) = ttf_parser::Face::parse(font, 0) else {
        return false;
    };
    let gpos = face.tables().gpos.is_none_or(|t| {
        (0..t.lookups.len()).all(|i| {
            t.lookups.get(i).is_some_and(|l| {
                (0..l.subtables.len()).all(|j| {
                    l.subtables
                        .get::<ttf_parser::gpos::PositioningSubtable>(j)
                        .is_some()
                })
            })
        })
    });
    let gsub = face.tables().gsub.is_none_or(|t| {
        (0..t.lookups.len()).all(|i| {
            t.lookups.get(i).is_some_and(|l| {
                (0..l.subtables.len()).all(|j| {
                    l.subtables
                        .get::<ttf_parser::gsub::SubstitutionSubtable>(j)
                        .is_some()
                })
            })
        })
    });
    gpos && gsub
}

/// `bytes` cut down to `chars`. Layout tables the subsetter breaks are dropped (kerning is lost,
/// the text still draws), and the second value says so.
fn subset(bytes: &[u8], chars: &HashSet<char>) -> Result<(Vec<u8>, Option<&'static str>), String> {
    let gpos = Tag::new(b"GPOS");
    for drop in [&[][..], &[gpos][..], &[gpos, Tag::new(b"GSUB")][..]] {
        let cut = run_klippa(bytes, chars, drop)?;
        if layout_is_sound(&cut) {
            return Ok((cut, note_static(drop)));
        }
    }
    Err("the subset's layout tables do not parse".into())
}

fn note_static(drop: &[Tag]) -> Option<&'static str> {
    match drop.len() {
        // The canvas does not kern either, so without GPOS the text keeps the design's widths.
        0 | 1 => None,
        // GSUB also carries the shaping some scripts cannot draw without.
        _ => Some("the font's kerning and ligatures could not be subset and were dropped"),
    }
}

/// `bytes` cut down to `chars`, as WOFF2 when it encodes (TTF/OTF otherwise). Err names why the
/// font could not be subset, so the caller can ship the whole file instead.
pub fn pack(bytes: &[u8], chars: &HashSet<char>, cff: bool) -> Result<Packed, String> {
    let (ttf, note) = subset(bytes, chars)?;
    // A subset that lost a character the original had would draw it in the fallback font.
    let (orig, cut) = (
        ttf_parser::Face::parse(bytes, 0),
        ttf_parser::Face::parse(&ttf, 0),
    );
    let (Ok(orig), Ok(cut)) = (orig, cut) else {
        return Err("the subset does not parse".into());
    };
    if let Some(c) = chars
        .iter()
        .find(|c| orig.glyph_index(**c).is_some() && cut.glyph_index(**c).is_none())
    {
        return Err(format!("the subset lost U+{:04X}", *c as u32));
    }
    Ok(
        match ttf2woff2::encode(&ttf, ttf2woff2::BrotliQuality::default()) {
            Ok(woff) => Packed {
                bytes: woff,
                ext: "woff2",
                format: "woff2",
                note,
            },
            Err(_) if cff => Packed {
                bytes: ttf,
                ext: "otf",
                format: "opentype",
                note,
            },
            Err(_) => Packed {
                bytes: ttf,
                ext: "ttf",
                format: "truetype",
                note,
            },
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{face_bytes, face_id};

    fn inter() -> &'static [u8] {
        face_bytes(face_id(false, false))
    }

    #[test]
    fn a_subset_is_far_smaller_keeps_every_used_character_and_other_cases() {
        let chars = keep(&"Olá ção".chars().collect());
        assert!(
            chars.contains(&'o') && chars.contains(&'Ç'),
            "other case is kept"
        );
        let (cut, _) = subset(inter(), &chars).unwrap();
        assert!(
            layout_is_sound(&cut),
            "a broken GPOS must be dropped, not shipped"
        );
        let face = ttf_parser::Face::parse(&cut, 0).expect("the subset parses");
        for c in &chars {
            assert!(face.glyph_index(*c).is_some(), "{c:?} has no cmap entry");
        }
        assert!(
            cut.len() * 10 < inter().len(),
            "{} of {}",
            cut.len(),
            inter().len()
        );
        let packed = pack(inter(), &chars, false).unwrap();
        assert_eq!((packed.ext, packed.format), ("woff2", "woff2"));
        assert!(packed.bytes.len() < cut.len() && packed.bytes.starts_with(b"wOF2"));
    }

    #[test]
    fn bytes_that_are_not_a_font_are_an_error_not_a_panic() {
        assert!(pack(b"nope", &keep(&BTreeSet::new()), false).is_err());
        assert!(fallback("X", false, b"nope").is_none());
    }

    #[test]
    fn the_fallback_scales_arial_to_the_font_and_divides_its_metrics_by_that_scale() {
        let f = fallback("Inter", false, inter()).unwrap();
        assert_eq!((f.family.as_str(), f.local), ("Inter Fallback", "Arial"));
        // Inter is a little wider than Arial and has 0.969 em ascent, -0.242 descent, no gap.
        assert!((100.0..115.0).contains(&f.size_adjust), "{f:?}");
        let scale = f.size_adjust / 100.0;
        assert!(
            (f.ascent - 96.9 / scale).abs() < 0.2 && (f.descent - 24.2 / scale).abs() < 0.2,
            "{f:?}"
        );
        assert_eq!(f.gap, 0.0);
        assert_eq!(
            fallback("Fira Code", true, inter()).unwrap().local,
            "Courier New"
        );
    }
}
