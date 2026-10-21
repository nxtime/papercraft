//! Saved versions of a page: a named snapshot of its document, kept in the project folder under
//! `versions/` so it survives restarts and travels with the project.

use crate::model::Document;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DIR: &str = "versions";

#[derive(Serialize, Deserialize)]
struct File {
    name: String,
    /// Taken by the app (before a restore), not named by the user.
    #[serde(default)]
    auto: bool,
    saved: u64,
    doc: Document,
}

/// One saved version, without its document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// File stem, unique per project.
    pub id: String,
    pub name: String,
    pub auto: bool,
    /// Unix milliseconds.
    pub saved: u64,
}

fn dir(project: &Path) -> PathBuf {
    project.join(DIR)
}

fn file_of(project: &Path, id: &str) -> PathBuf {
    dir(project).join(format!("{id}.json"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Saves `doc` as a version of page `page_id`. Returns the new version.
pub fn save(
    project: &Path,
    page_id: &str,
    name: &str,
    auto: bool,
    doc: &Document,
) -> std::io::Result<Version> {
    std::fs::create_dir_all(dir(project))?;
    let saved = now_ms();
    let id = format!("{page_id}__{saved}");
    let name = name.trim();
    let name = if name.is_empty() {
        "Untitled version".to_string()
    } else {
        name.to_string()
    };
    let file = File {
        name: name.clone(),
        auto,
        saved,
        doc: doc.clone(),
    };
    let bytes = serde_json::to_vec(&file).map_err(std::io::Error::other)?;
    std::fs::write(file_of(project, &id), bytes)?;
    Ok(Version {
        id,
        name,
        auto,
        saved,
    })
}

/// The saved versions of page `page_id`, newest first.
pub fn list(project: &Path, page_id: &str) -> Vec<Version> {
    let Ok(entries) = std::fs::read_dir(dir(project)) else {
        return Vec::new();
    };
    let prefix = format!("{page_id}__");
    let mut out: Vec<Version> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let id = path.file_stem()?.to_str()?.to_owned();
            if !id.starts_with(&prefix) || path.extension()? != "json" {
                return None;
            }
            let f: File = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
            Some(Version {
                id,
                name: f.name,
                auto: f.auto,
                saved: f.saved,
            })
        })
        .collect();
    out.sort_by_key(|v| std::cmp::Reverse(v.saved));
    out
}

/// The document of a saved version.
pub fn load(project: &Path, id: &str) -> Option<Document> {
    let f: File = serde_json::from_slice(&std::fs::read(file_of(project, id)).ok()?).ok()?;
    Some(f.doc)
}

pub fn delete(project: &Path, id: &str) {
    let _ = std::fs::remove_file(file_of(project, id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_round_trip_and_list_newest_first() {
        let dir =
            std::env::temp_dir().join(format!("papercraft-versions-{:016x}", fastrand::u64(..)));
        let doc = Document::default();
        let a = save(&dir, "p1", "First", false, &doc).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = save(&dir, "p1", "", true, &doc).unwrap();
        save(&dir, "p2", "Other page", false, &doc).unwrap();
        let listed = list(&dir, "p1");
        assert_eq!(
            listed.iter().map(|v| v.id.clone()).collect::<Vec<_>>(),
            vec![b.id.clone(), a.id.clone()]
        );
        assert_eq!(listed[0].name, "Untitled version");
        assert!(listed[0].auto && !listed[1].auto);
        assert!(load(&dir, &a.id).is_some());
        delete(&dir, &a.id);
        assert_eq!(list(&dir, "p1").len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }
}
