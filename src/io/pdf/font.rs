//! Text as real PDF text: each Inter face a text uses is embedded once per file (a Type0 font
//! over a CIDFontType2, glyph ids as character codes) with a ToUnicode map, so a reader can
//! select, search, copy and edit it in any language the font covers.

use super::{Objects, num};
use crate::text::{self, FaceId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// The faces a file uses: the object id reserved for each and the glyphs drawn with it.
#[derive(Default)]
pub(super) struct Fonts {
    used: BTreeMap<FaceId, (usize, BTreeSet<u16>)>,
}

/// PDF glyph space: 1000 units to the em.
const EM: f64 = 1000.0;

impl Fonts {
    /// The resource name of `face` (`/F<face>`), its object reserved on first use.
    pub(super) fn name(&mut self, objs: &mut Objects, face: FaceId, gids: impl IntoIterator<Item = u16>) -> (String, usize) {
        let (id, set) = self.used.entry(face).or_insert_with(|| (objs.reserve(), BTreeSet::new()));
        set.extend(gids);
        (format!("F{face}"), *id)
    }

    /// Writes every font used, once all pages are drawn.
    pub(super) fn finish(self, objs: &mut Objects) {
        for (face, (id, gids)) in self.used {
            let f = text::face_of(face);
            let bytes = text::face_bytes(face);
            let k = EM / f64::from(f.units_per_em());
            let name = f
                .names()
                .into_iter()
                .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
                .and_then(|n| n.to_string())
                .unwrap_or_else(|| "Inter".into());
            let file = objs.stream(&format!("/Length1 {}", bytes.len()), bytes);
            let b = f.global_bounding_box();
            let descriptor = objs.add(
                format!(
                    "<< /Type /FontDescriptor /FontName /{name} /Flags {} /FontBBox [{} {} {} {}] /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {file} 0 R >>",
                    if f.is_italic() { 32 | 64 } else { 32 },
                    num(f64::from(b.x_min) * k),
                    num(f64::from(b.y_min) * k),
                    num(f64::from(b.x_max) * k),
                    num(f64::from(b.y_max) * k),
                    num(f.italic_angle().into()),
                    num(f64::from(f.ascender()) * k),
                    num(f64::from(f.descender()) * k),
                    num(f64::from(f.capital_height().unwrap_or(f.ascender())) * k),
                )
                .into_bytes(),
            );
            let widths: String = gids
                .iter()
                .map(|g| format!("{g} [{}]", num(f64::from(f.glyph_hor_advance(ttf_parser::GlyphId(*g)).unwrap_or(0)) * k)))
                .collect::<Vec<_>>()
                .join(" ");
            let cid = objs.add(
                format!(
                    "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity /W [{widths}] >>"
                )
                .into_bytes(),
            );
            let to_unicode = objs.stream("", to_unicode(f, &gids).as_bytes());
            objs.set(
                id,
                format!("<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {to_unicode} 0 R >>")
                    .into_bytes(),
            );
        }
    }
}

/// Glyph advance in points at `size`.
pub(super) fn advance(face: FaceId, gid: u16, size: f64) -> f64 {
    let f = text::face_of(face);
    f64::from(f.glyph_hor_advance(ttf_parser::GlyphId(gid)).unwrap_or(0)) * size / f64::from(f.units_per_em())
}

/// The character each glyph stands for, read back from the font's own character map (the
/// lowest code point when several share a glyph).
fn chars_of(f: &ttf_parser::Face, gids: &BTreeSet<u16>) -> BTreeMap<u16, char> {
    let mut out = BTreeMap::new();
    for t in f.tables().cmap.into_iter().flat_map(|c| c.subtables).filter(|t| t.is_unicode()) {
        t.codepoints(|cp| {
            let (Some(g), Some(c)) = (t.glyph_index(cp), char::from_u32(cp)) else { return };
            if gids.contains(&g.0) && out.get(&g.0).is_none_or(|old: &char| c < *old) {
                out.insert(g.0, c);
            }
        });
    }
    out
}

fn to_unicode(f: &ttf_parser::Face, gids: &BTreeSet<u16>) -> String {
    let map = chars_of(f, gids);
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    // A bfchar block holds at most 100 entries.
    let entries: Vec<(u16, char)> = map.into_iter().collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (g, c) in chunk {
            let utf16: String = c.encode_utf16(&mut [0; 2]).iter().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(s, "<{g:04X}> <{utf16}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_in_any_language_the_font_covers_maps_back_to_its_characters() {
        let f = text::face_of(0);
        let s = "Ação, saída! Привет Ωμέγα";
        let gids: BTreeSet<u16> = s.chars().filter_map(|c| f.glyph_index(c)).map(|g| g.0).collect();
        let back: BTreeSet<char> = chars_of(f, &gids).into_values().collect();
        assert!(s.chars().all(|c| back.contains(&c)), "{back:?}");
        assert!(to_unicode(f, &gids).contains(&format!("<{:04X}>", 'ç' as u32)));
    }
}
