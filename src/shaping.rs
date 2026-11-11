//! OpenType shaping (HarfRust) and the Unicode bidi algorithm for [`crate::text`].
//!
//! Shaping happens in font units and is cached per (face, direction, text), so a layout that
//! only moves or rescales text does not shape again.

use crate::text::{face_bytes, FaceId};
use harfrust::{
    BufferFlags, Direction, FontRef, ShapeOptions, Shaper, ShaperData, Tag, UnicodeBuffer,
};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// One shaped glyph, in font units, in visual (left to right) order.
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    pub gid: u16,
    /// Absolute char index of the first char of the cluster this glyph belongs to.
    pub cluster: usize,
    pub advance: f64,
    pub dx: f64,
    /// Up is positive.
    pub dy: f64,
}

type CacheKey = (FaceId, bool, bool, Vec<char>);

const CACHE_LIMIT: usize = 4096;

fn cache() -> &'static Mutex<HashMap<CacheKey, std::sync::Arc<Vec<ShapedGlyph>>>> {
    static C: OnceLock<Mutex<HashMap<CacheKey, std::sync::Arc<Vec<ShapedGlyph>>>>> =
        OnceLock::new();
    C.get_or_init(Default::default)
}

/// Faces never go away (see `fonts`), so their shaper data is leaked once and shared.
fn shaper_data(face: FaceId) -> Option<(FontRef<'static>, &'static ShaperData)> {
    static D: OnceLock<Mutex<HashMap<FaceId, Option<&'static ShaperData>>>> = OnceLock::new();
    let font = FontRef::new(face_bytes(face)).ok()?;
    let mut map = D.get_or_init(Default::default).lock().unwrap();
    let data = (*map
        .entry(face)
        .or_insert_with(|| Some(Box::leak(Box::new(ShaperData::new(&font))))))?;
    Some((font, data))
}

/// Shape `chars` (cluster values start at `first`). `rtl` runs come back in visual order.
/// Without `ligatures` (letter-spaced text) `liga` and `clig` are off, as in CSS.
pub fn shape(
    face: FaceId,
    rtl: bool,
    ligatures: bool,
    chars: &[char],
    first: usize,
) -> Vec<ShapedGlyph> {
    let key: CacheKey = (face, rtl, ligatures, chars.to_vec());
    if let Some(hit) = cache().lock().unwrap().get(&key) {
        return hit
            .iter()
            .map(|g| ShapedGlyph {
                cluster: g.cluster + first,
                ..*g
            })
            .collect();
    }
    let out = std::sync::Arc::new(shape_uncached(face, rtl, ligatures, chars));
    let mut c = cache().lock().unwrap();
    if c.len() >= CACHE_LIMIT {
        c.clear();
    }
    c.insert(key, out.clone());
    out.iter()
        .map(|g| ShapedGlyph {
            cluster: g.cluster + first,
            ..*g
        })
        .collect()
}

fn shape_uncached(face: FaceId, rtl: bool, ligatures: bool, chars: &[char]) -> Vec<ShapedGlyph> {
    let Some((font, data)) = shaper_data(face) else {
        return Vec::new();
    };
    let shaper: Shaper = data.shaper(&font).build();
    let mut buf = UnicodeBuffer::new();
    for (i, c) in chars.iter().enumerate() {
        buf.add(*c, i as u32);
    }
    buf.set_direction(if rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    buf.set_flags(BufferFlags::BEGINNING_OF_TEXT | BufferFlags::END_OF_TEXT);
    buf.guess_segment_properties();
    let off = [
        harfrust::Feature::new(Tag::new(b"liga"), 0, ..),
        harfrust::Feature::new(Tag::new(b"clig"), 0, ..),
    ];
    let features: &[harfrust::Feature] = if ligatures { &[] } else { &off };
    let shaped = shaper.shape(buf, ShapeOptions::new().features(features));
    shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(i, p)| ShapedGlyph {
            gid: i.glyph_id as u16,
            cluster: i.cluster as usize,
            advance: p.x_advance as f64,
            dx: p.x_offset as f64,
            dy: p.y_offset as f64,
        })
        .collect()
}

/// Whether `c` can make a paragraph right to left or flip its neighbours' order.
fn may_be_rtl(c: char) -> bool {
    matches!(c,
        '\u{0590}'..='\u{08FF}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2066}'..='\u{2069}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'
        | '\u{10800}'..='\u{10FFF}' | '\u{1E800}'..='\u{1EFFF}')
}

/// Embedding level of each char of one paragraph (no line breaks). Odd = right to left.
pub fn levels(chars: &[char]) -> Vec<u8> {
    if !chars.iter().copied().any(may_be_rtl) {
        return vec![0; chars.len()];
    }
    let text: String = chars.iter().collect();
    let info = unicode_bidi::BidiInfo::new(&text, None);
    text.char_indices()
        .map(|(b, _)| info.levels[b].number())
        .collect()
}

/// Visual order of logical runs given their levels (rule L2): reverse every maximal sequence at
/// or above each level, from the highest down to the lowest odd one.
pub fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let Some(&max) = levels.iter().max() else {
        return order;
    };
    let Some(min_odd) = levels.iter().copied().filter(|l| l % 2 == 1).min() else {
        return order;
    };
    for level in (min_odd..=max).rev() {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= level {
                let start = i;
                while i < order.len() && levels[order[i]] >= level {
                    i += 1;
                }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }
    order
}

/// A stretch of one paragraph that shapes as a unit: one bidi level, face and size.
#[derive(Debug, Clone, Copy)]
pub struct Run {
    pub a: usize,
    pub b: usize,
    pub level: u8,
    pub face: FaceId,
    pub size: f64,
}

impl Run {
    pub fn rtl(&self) -> bool {
        self.level % 2 == 1
    }
}

/// Split `[start, end)` into runs; `size[i]` and `face[i]` are per char, `levels` per char.
pub fn runs_in(start: usize, end: usize, levels: &[u8], face: &[FaceId], size: &[f64]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for i in start..end {
        let (l, f, z) = (levels[i], face[i], size[i]);
        match out.last_mut() {
            Some(r) if r.level == l && r.face == f && r.size == z => r.b = i + 1,
            _ => out.push(Run {
                a: i,
                b: i + 1,
                level: l,
                face: f,
                size: z,
            }),
        }
    }
    out
}

/// A cluster of source chars drawn as one unit (a ligature, a base with its marks): the chars
/// `[a, b)` span `[x0, x1]` of the run, which is 0 at the run's left edge.
#[derive(Debug, Clone, Copy)]
pub struct Cluster {
    pub a: usize,
    pub b: usize,
    pub x0: f64,
    pub x1: f64,
}

impl Cluster {
    /// Extent of the `k`-th char of the cluster: a ligature is split evenly, and an RTL run
    /// counts its chars from the right.
    pub fn char_span(&self, k: usize, rtl: bool) -> (f64, f64) {
        let w = (self.x1 - self.x0) / (self.b - self.a) as f64;
        match rtl {
            false => (self.x0 + k as f64 * w, self.x0 + (k + 1) as f64 * w),
            true => (self.x1 - (k + 1) as f64 * w, self.x1 - k as f64 * w),
        }
    }
}

/// The run's glyphs scaled to `r.size`, with `spacing` added after every char.
pub fn shape_px(r: &Run, chars: &[char], spacing: f64) -> Vec<ShapedGlyph> {
    let upem = crate::text::face_of(r.face).units_per_em() as f64;
    let k = r.size / upem;
    let mut glyphs: Vec<ShapedGlyph> =
        shape(r.face, r.rtl(), spacing == 0.0, &chars[r.a..r.b], r.a)
            .into_iter()
            .map(|g| ShapedGlyph {
                advance: g.advance * k,
                dx: g.dx * k,
                dy: g.dy * k,
                ..g
            })
            .collect();
    if spacing != 0.0 {
        let mut last_of_cluster = Vec::new();
        for (i, g) in glyphs.iter().enumerate() {
            match last_of_cluster.last_mut() {
                Some((c, last)) if *c == g.cluster => *last = i,
                _ => last_of_cluster.push((g.cluster, i)),
            }
        }
        let clusters = clusters(r, &glyphs);
        for (c, i) in last_of_cluster {
            let n = clusters
                .iter()
                .find(|cl| cl.a <= c && c < cl.b)
                .map_or(1, |cl| cl.b - cl.a);
            glyphs[i].advance += spacing * n as f64;
        }
    }
    glyphs
}

/// The run's clusters in logical order; chars no glyph names belong to the cluster before them.
pub fn clusters(r: &Run, glyphs: &[ShapedGlyph]) -> Vec<Cluster> {
    let mut found: Vec<(usize, f64, f64)> = Vec::new();
    let mut x = 0.0;
    for g in glyphs {
        match found.iter_mut().find(|f| f.0 == g.cluster) {
            Some(f) => f.2 = x + g.advance,
            None => found.push((g.cluster, x, x + g.advance)),
        }
        x += g.advance;
    }
    found.sort_by_key(|f| f.0);
    let ends: Vec<usize> = found.iter().skip(1).map(|f| f.0).chain([r.b]).collect();
    found
        .iter()
        .zip(ends)
        .enumerate()
        .map(|(i, (f, b))| Cluster {
            a: if i == 0 { r.a } else { f.0 },
            b,
            x0: f.1,
            x1: f.2,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::geom::v2;
    use crate::model::{TextAlign, TextStyle};
    use crate::text::{layout_rich, TextLayout};

    const ARABIC_FONT: &[u8] = include_bytes!("../tests/fixtures/fonts/NotoSansArabic-subset.ttf");
    const GEIST: &[u8] = include_bytes!("../assets/fonts/Geist-Regular.ttf");
    const BA: &str = "\u{628}\u{628}\u{628}";

    fn lay(font: &[u8], text: &str, max_width: Option<f64>, spacing: f64) -> TextLayout {
        let family = crate::fonts::register(font).unwrap();
        let ts = TextStyle {
            font: Some(family),
            letter_spacing: spacing,
            ..TextStyle::default()
        };
        layout_rich(
            text,
            20.0,
            crate::model::Color::INK,
            &ts,
            max_width,
            TextAlign::Left,
        )
    }

    #[test]
    fn ligature_is_one_glyph_with_a_caret_inside() {
        let l = lay(GEIST, "fi", None, 0.0);
        assert_eq!(l.glyphs.len(), 1);
        let (mid, end) = (l.lines[0].carets[1], l.lines[0].carets[2]);
        assert!((mid - end / 2.0).abs() < 1e-6, "{mid} {end}");
        assert_eq!(l.hit(v2(mid + 0.1, 5.0)), 1);
        // Letter-spacing switches ligatures off, as in CSS.
        assert_eq!(lay(GEIST, "fi", None, 1.0).glyphs.len(), 2);
    }

    #[test]
    fn arabic_joins_and_runs_right_to_left() {
        let l = lay(ARABIC_FONT, BA, None, 0.0);
        // The font splits each letter into a base and its dot, so compare the glyphs per char.
        let gids_of = |l: &TextLayout, ch: usize| -> Vec<u16> {
            let mut v: Vec<u16> = l
                .glyphs
                .iter()
                .filter(|g| g.char_index == ch)
                .map(|g| g.gid)
                .collect();
            v.sort();
            v
        };
        let isolated = gids_of(&lay(ARABIC_FONT, "\u{628}", None, 0.0), 0);
        let (first, medial, last) = (gids_of(&l, 0), gids_of(&l, 1), gids_of(&l, 2));
        assert!(
            first != isolated && medial != isolated && first != medial,
            "joining picks a form per position"
        );
        assert_ne!(medial, last);
        let spans = &l.lines[0].spans;
        assert!(
            spans[0].0 >= spans[1].1 - 1e-9 && spans[1].0 >= spans[2].1 - 1e-9,
            "{spans:?}"
        );
        // Glyphs are placed left to right, so the last char comes first.
        let mut order: Vec<usize> = l.glyphs.iter().map(|g| g.char_index).collect();
        order.dedup();
        assert_eq!(order, [2, 1, 0]);
    }

    #[test]
    fn mixed_direction_line_orders_runs_by_bidi() {
        let l = lay(ARABIC_FONT, "ab \u{628}\u{628} cd", None, 0.0);
        let s = &l.lines[0].spans;
        // a b _ [ب ب] _ c d, the Arabic pair reversed in place.
        assert!(s[1].1 <= s[2].0 + 1e-9 && s[2].1 <= s[4].0 + 1e-9);
        assert!(s[4].1 <= s[3].0 + 1e-9 && s[3].1 <= s[5].0 + 1e-9);
        assert!(s[5].1 <= s[6].0 + 1e-9 && s[6].1 <= s[7].0 + 1e-9);
    }

    #[test]
    fn caret_and_hit_agree_on_rtl() {
        let l = lay(ARABIC_FONT, BA, None, 0.0);
        let carets = &l.lines[0].carets;
        assert!(carets[0] > carets[1] && carets[1] > carets[2] && carets[2] > carets[3]);
        for i in 0..=3 {
            let (x, top, h) = l.caret(i);
            assert_eq!(l.hit(v2(x, top + h * 0.5)), i);
        }
    }

    #[test]
    fn selection_in_rtl_text_highlights_the_right_chars() {
        let l = lay(ARABIC_FONT, BA, None, 0.0);
        let rects = l.selection_rects(0, 1);
        assert_eq!(rects.len(), 1);
        let (x, _, w, _) = rects[0];
        assert!(
            (x + w - l.width).abs() < 1e-6,
            "the first char sits at the right edge"
        );
        // Chars 0 and 2 of a mixed line are two separate stretches.
        let mixed = lay(ARABIC_FONT, "a\u{628}b", None, 0.0);
        assert_eq!(mixed.selection_rects(0, 3).len(), 1);
        assert_eq!(mixed.selection_rects(0, 1).len(), 1);
    }

    #[test]
    fn rtl_text_wraps_by_its_shaped_width() {
        let text = [BA; 6].join(" ");
        let l = lay(ARABIC_FONT, &text, Some(60.0), 0.0);
        assert!(l.lines.len() > 1);
        assert!(
            l.lines.iter().all(|line| line.width <= 60.0 + 1e-6),
            "{:?}",
            l.lines.iter().map(|x| x.width).collect::<Vec<_>>()
        );
    }

    #[test]
    fn visual_order_reverses_odd_levels() {
        assert_eq!(super::visual_order(&[0, 1, 1, 0]), [0, 2, 1, 3]);
        assert_eq!(super::visual_order(&[0, 1, 2, 1]), [0, 3, 2, 1]);
        assert_eq!(super::visual_order(&[0, 0]), [0, 1]);
    }
}
