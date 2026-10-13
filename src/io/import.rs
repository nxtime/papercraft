//! Importing files into projects.

use crate::library::LibItem;
use crate::model::Node;
use crate::project::{PageData, Project, format};
use std::path::Path;

/// Read a file into pages for a new project. Returns a suggested project name.
pub fn import_as_pages(path: &Path) -> Result<(String, Vec<PageData>), String> {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Imported".into());
    if crate::vfs::is_dir(path) {
        let p = Project::open(path).map_err(|e| e.to_string())?;
        let pages = p.load_pages().map_err(|e| e.to_string())?;
        return Ok((p.manifest.name, pages));
    }
    let text = crate::vfs::read_to_string(path).map_err(|e| e.to_string())?;
    page_from_text(&stem, &text)
}

fn page_from_text(stem: &str, text: &str) -> Result<(String, Vec<PageData>), String> {
    let page = format::page_from_json(text).map_err(|e| e.to_string())?;
    let name = if page.name.is_empty() || page.name == "Untitled" {
        stem.to_string()
    } else {
        page.name.clone()
    };
    Ok((name, vec![page]))
}

/// What a dropped or picked file turned out to be.
#[derive(Debug)]
pub enum Imported {
    /// A PaperCraft document: pages for a new project, with a suggested name.
    Pages(String, Vec<PageData>),
    /// Loose layers from an SVG (`group` names the group they stay together in) or an
    /// Excalidraw scene.
    Scene {
        nodes: Vec<Node>,
        group: Option<String>,
    },
    /// Encoded raster image bytes, to be registered as an asset.
    Image(Vec<u8>),
    /// Shapes for the library (`.excalidrawlib`).
    Library(Vec<LibItem>),
    /// A project backup zip (see [`crate::io::backup`]); the caller unpacks it.
    Backup,
}

/// Lowercase extension of a file name, without the dot.
pub fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Classify and parse a file given only its name and bytes (the browser has no paths).
pub fn import_bytes(name: &str, bytes: &[u8]) -> Result<Imported, String> {
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Imported".into());
    let ext = extension(name);
    if ext == "zip" || super::backup::is_zip(bytes) {
        return Ok(Imported::Backup);
    }
    let text = || std::str::from_utf8(bytes).map_err(|_| format!("{name} is not a text file"));
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" => Ok(Imported::Image(bytes.to_vec())),
        "excalidrawlib" => crate::io::excalidraw::import_library(text()?).map(Imported::Library),
        "svg" => Ok(Imported::Scene {
            nodes: crate::io::svg_import::parse(text()?)?,
            group: Some(stem),
        }),
        "excalidraw" | "json" => {
            let doc = crate::io::excalidraw::import_scene(text()?)?;
            let nodes = doc
                .paint_order()
                .into_iter()
                .filter_map(|id| doc.get(id).cloned())
                .collect();
            Ok(Imported::Scene { nodes, group: None })
        }
        _ => page_from_text(&stem, text()?).map(|(n, p)| Imported::Pages(n, p)),
    }
}

/// Load the pages of a project folder or of a single document file (CLI export).
pub fn load_any(path: &Path) -> Result<Vec<PageData>, String> {
    import_as_pages(path).map(|(_, pages)| pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#;

    #[test]
    fn svg_becomes_a_named_scene() {
        let Ok(Imported::Scene { nodes, group }) = import_bytes("Logo.SVG", SVG.as_bytes()) else {
            panic!("svg should import as a scene");
        };
        assert!(!nodes.is_empty());
        assert_eq!(group.as_deref(), Some("Logo"));
    }

    #[test]
    fn images_pass_through_untouched() {
        let Ok(Imported::Image(b)) = import_bytes("a.png", b"bytes") else {
            panic!("png should import as an image");
        };
        assert_eq!(b, b"bytes");
    }

    #[test]
    fn zips_are_backups_whatever_their_name() {
        let zip = crate::io::zip::write(&[("a/project.json".into(), vec![])]);
        assert!(matches!(import_bytes("x.bin", &zip), Ok(Imported::Backup)));
    }

    #[test]
    fn papercraft_document_round_trips() {
        let page = PageData {
            id: "p".into(),
            name: "Untitled".into(),
            doc: Default::default(),
            camera: None,
        };
        let json = format::page_to_json(&page);
        let Ok(Imported::Pages(name, pages)) = import_bytes("My doc.papercraft", json.as_bytes())
        else {
            panic!("document should import as pages");
        };
        assert_eq!((name.as_str(), pages.len()), ("My doc", 1));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(import_bytes("x.papercraft", b"\xff\xfe").is_err());
        assert!(import_bytes("x.svg", b"not svg").is_err());
    }
}
