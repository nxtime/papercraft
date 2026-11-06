//! Fonts people add on top of the bundled Inter.
//!
//! Faces are registered process-wide and never dropped, like image assets, because glyph caches
//! and queued draws keep their ids. A project stores its faces as `assets/fonts/<hash>.<ext>`, so
//! it opens the same anywhere; text naming a family that is not installed renders in Inter.

use crate::text::{BUILT_IN_FACES, FaceId, face_id};
use std::path::Path;
use std::sync::{OnceLock, RwLock};

const FONTS_DIR: &str = "fonts";
/// Larger than any Latin family with hinting, and small enough to refuse a wrong download.
pub const MAX_FONT_BYTES: usize = 32 << 20;
const REGULAR_WEIGHT: i32 = 400;
const BOLD_WEIGHT: i32 = 700;
/// Chars cached per face, see `text::CACHED_CHARS`.
pub type AdvanceTable = Vec<(FaceId, u16, f64)>;

pub struct LoadedFace {
    hash: String,
    pub bytes: &'static [u8],
    pub face: ttf_parser::Face<'static>,
    weight: i32,
    /// The Inter face of the same style, for characters this one lacks.
    pub fallback: FaceId,
    pub advances: OnceLock<AdvanceTable>,
}

impl LoadedFace {
    fn ext(&self) -> &'static str {
        if self.face.tables().cff.is_some() { "otf" } else { "ttf" }
    }
}

struct Family {
    name: String,
    /// Indexed like a built-in `FaceId`: bit 0 = bold, bit 1 = italic.
    slots: [Option<FaceId>; 4],
}

#[derive(Default)]
struct Registry {
    faces: Vec<&'static LoadedFace>,
    families: Vec<Family>,
}

impl Registry {
    fn family(&self, name: &str) -> Option<&Family> {
        self.families.iter().find(|f| f.name.eq_ignore_ascii_case(name))
    }

    fn loaded(&self, id: FaceId) -> &'static LoadedFace {
        self.faces[usize::from(id - BUILT_IN_FACES)]
    }
}

fn registry() -> &'static RwLock<Registry> {
    static R: OnceLock<RwLock<Registry>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// A face registered by [`register`]; ids below [`BUILT_IN_FACES`] are Inter's.
pub fn loaded(id: FaceId) -> &'static LoadedFace {
    registry().read().unwrap().loaded(id)
}

fn family_name(face: &ttf_parser::Face) -> Option<String> {
    use ttf_parser::name_id::{FAMILY, TYPOGRAPHIC_FAMILY};
    let named = |id| face.names().into_iter().find(|n| n.name_id == id && n.is_unicode()).and_then(|n| n.to_string());
    named(TYPOGRAPHIC_FAMILY).or_else(|| named(FAMILY)).filter(|n| !n.trim().is_empty())
}

/// Registers a TrueType or OpenType file and returns its family's name.
pub fn register(bytes: &[u8]) -> Result<String, String> {
    let hash = crate::assets::hash(bytes);
    let mut reg = registry().write().unwrap();
    let known = (0..reg.faces.len()).find(|&i| reg.faces[i].hash == hash).map(|i| i as FaceId + BUILT_IN_FACES);
    if let Some(id) = known {
        let name = reg.families.iter().find(|f| f.slots.contains(&Some(id))).map(|f| f.name.clone());
        return name.ok_or_else(|| "the font was replaced by another of its style".to_owned());
    }
    if bytes.len() > MAX_FONT_BYTES {
        return Err(format!("the font is over {} MB", MAX_FONT_BYTES >> 20));
    }
    let probe = ttf_parser::Face::parse(bytes, 0).map_err(|_| "not a TrueType or OpenType font (.ttf or .otf); WOFF and archives must be unpacked first".to_owned())?;
    let tables = probe.tables();
    if tables.cmap.is_none() || (tables.glyf.is_none() && tables.cff.is_none()) {
        return Err("the font has no outlines or character map".into());
    }
    let family = family_name(&probe).ok_or("the font does not name its family")?;
    if family.eq_ignore_ascii_case("inter") {
        return Err("Inter is built in".into());
    }
    let (bold, italic) = (probe.is_bold() || probe.weight().to_number() >= 600, probe.is_italic() || probe.is_oblique());
    let weight = i32::from(probe.weight().to_number());

    // Leaked on purpose: a face lives as long as the process, see the module comment.
    let bytes: &'static [u8] = Box::leak(bytes.to_vec().into_boxed_slice());
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|e| e.to_string())?;
    let fallback = face_id(bold, italic);
    let id = reg.faces.len() as FaceId + BUILT_IN_FACES;
    reg.faces.push(Box::leak(Box::new(LoadedFace { hash, bytes, face, weight, fallback, advances: OnceLock::new() })));

    let index = match reg.families.iter().position(|f| f.name.eq_ignore_ascii_case(&family)) {
        Some(i) => i,
        None => {
            reg.families.push(Family { name: family.clone(), slots: [None; 4] });
            reg.families.len() - 1
        }
    };
    let slot = usize::from(fallback);
    let target = if bold { BOLD_WEIGHT } else { REGULAR_WEIGHT };
    let held = reg.families[index].slots[slot].map(|old| (reg.loaded(old).weight - target).abs());
    // Of a family's many weights, the slot keeps the one nearest what it stands for.
    if held.is_none_or(|held| (weight - target).abs() < held) {
        reg.families[index].slots[slot] = Some(id);
    }
    Ok(reg.families[index].name.clone())
}

/// The face to draw with: the family's nearest style, or Inter's when it is not installed.
pub fn style_face(family: Option<&str>, bold: bool, italic: bool) -> FaceId {
    let inter = face_id(bold, italic);
    let Some(name) = family else { return inter };
    let reg = registry().read().unwrap();
    let Some(f) = reg.family(name) else { return inter };
    let slot = usize::from(inter);
    [slot, slot & 2, slot & 1, 0].iter().find_map(|&s| f.slots[s]).or_else(|| f.slots.iter().flatten().next().copied()).unwrap_or(inter)
}

/// Installed family names, sorted.
pub fn families() -> Vec<String> {
    let mut names: Vec<String> = registry().read().unwrap().families.iter().map(|f| f.name.clone()).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// A family's installed name, however `name` is spelled, with `(bold, italic)` for each style.
pub fn styles(name: &str) -> Option<(String, Vec<(bool, bool)>)> {
    let reg = registry().read().unwrap();
    let f = reg.family(name)?;
    let styles = (0..4).filter(|&s| f.slots[s].is_some()).map(|s| (s & 1 != 0, s & 2 != 0)).collect();
    Some((f.name.clone(), styles))
}

/// What a CSS `font-family` list asks for, its first family that can be drawn: `Some(None)` is
/// Inter, and `None` means none can, so the layer keeps its font.
pub fn pick(css: &str) -> Option<Option<String>> {
    css.split(',').map(|f| f.trim().trim_matches(['"', '\''])).find_map(|f| {
        if f.eq_ignore_ascii_case("inter") {
            return Some(None);
        }
        styles(f).map(|(name, _)| Some(name))
    })
}

/// Registers a font and keeps it in the project, when there is one.
pub fn install(bytes: &[u8], assets_dir: Option<&Path>) -> Result<String, String> {
    let family = register(bytes)?;
    if let Some(dir) = assets_dir {
        save_to_dir(dir, std::slice::from_ref(&family)).map_err(|e| format!("could not save the font: {e}"))?;
    }
    Ok(family)
}

/// Registers the fonts in a project's `assets/` folder.
pub fn load_dir(assets_dir: &Path) {
    let Ok(rd) = crate::vfs::read_dir(assets_dir.join(FONTS_DIR)) else {
        return;
    };
    for e in rd.flatten() {
        let loaded = crate::vfs::read(e.path()).map_err(|e| e.to_string()).and_then(|b| register(&b));
        if let Err(err) = loaded {
            log::warn!("{}: {err}", e.path().display());
        }
    }
}

/// Writes these families into a project's `assets/` folder (skipping files already there).
pub fn save_to_dir(assets_dir: &Path, families: &[String]) -> std::io::Result<()> {
    let dir = assets_dir.join(FONTS_DIR);
    for name in families {
        let faces: Vec<&'static LoadedFace> = {
            let reg = registry().read().unwrap();
            let Some(f) = reg.family(name) else { continue };
            f.slots.iter().flatten().map(|id| reg.loaded(*id)).collect()
        };
        for face in faces {
            let path = dir.join(format!("{}.{}", face.hash, face.ext()));
            if !crate::vfs::exists(&path) {
                crate::vfs::create_dir_all(&dir)?;
                crate::io::atomic::write(&path, face.bytes)?;
            }
        }
    }
    Ok(())
}

/// Font families a document's text names.
pub fn used_by(doc: &crate::model::Document) -> Vec<String> {
    let mut out: Vec<String> = doc.nodes.values().filter_map(|n| n.text_style.font.clone()).collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Color, TextAlign, TextStyle};

    static VARELA: &[u8] = include_bytes!("../assets/fonts/VarelaRound-Regular.ttf");

    fn styled(font: &str) -> TextStyle {
        TextStyle { font: Some(font.into()), ..TextStyle::default() }
    }

    #[test]
    fn registering_rejects_what_is_not_a_font_and_keeps_one_copy() {
        assert!(register(b"<html>not a font</html>").is_err());
        assert_eq!(register(crate::theme::INTER_REGULAR), Err("Inter is built in".into()));
        let family = register(VARELA).unwrap();
        assert_eq!(family, "Varela Round");
        assert_eq!(register(VARELA).unwrap(), family);
        assert_eq!(families().iter().filter(|f| **f == family).count(), 1);
    }

    #[test]
    fn text_lays_out_in_its_family_and_falls_back_per_character() {
        let family = register(VARELA).unwrap();
        let wide = |font: &TextStyle| crate::text::layout_rich("Hamburg", 20.0, Color::INK, font, None, TextAlign::Left);
        let custom = wide(&styled(&family));
        assert!(custom.glyphs.iter().all(|g| g.face >= BUILT_IN_FACES));
        assert_ne!(custom.width, wide(&TextStyle::default()).width);

        let missing = crate::text::layout_rich("Ω", 20.0, Color::INK, &styled(&family), None, TextAlign::Left);
        let glyph_lacking = crate::text::face_of(custom.glyphs[0].face).glyph_index('Ω').is_none();
        assert_eq!(missing.glyphs[0].face < BUILT_IN_FACES, glyph_lacking);
    }

    #[test]
    fn a_family_without_a_style_uses_its_nearest_and_an_unknown_one_inter() {
        let family = register(VARELA).unwrap();
        assert_eq!(style_face(Some(&family), true, true), style_face(Some(&family), false, false));
        assert_eq!(style_face(Some("No Such Font"), true, false), face_id(true, false));
        assert!(style_face(Some("varela round"), false, false) >= BUILT_IN_FACES);
    }

    #[test]
    fn css_lists_take_the_first_family_that_can_be_drawn() {
        let family = register(VARELA).unwrap();
        assert_eq!(pick("'Nope', \"varela round\", serif"), Some(Some(family)));
        assert_eq!(pick("Nope, Inter"), Some(None));
        assert_eq!(pick("system-ui, sans-serif"), None);
    }

    #[test]
    fn a_project_keeps_its_fonts_in_its_assets_folder() {
        let family = register(VARELA).unwrap();
        let dir = std::env::temp_dir().join(format!("papercraft-fonts-{}", std::process::id()));
        install(VARELA, Some(&dir)).unwrap();
        save_to_dir(&dir, std::slice::from_ref(&family)).unwrap();
        let files: Vec<_> = std::fs::read_dir(dir.join(FONTS_DIR)).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        assert_eq!(std::fs::read(files[0].path()).unwrap(), VARELA);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
