//! Handoff points: a named snapshot of every page, taken when screens go to development, so a
//! later design edit can be reported against what the developers received. Kept in the project
//! folder under `handoffs/`, one file per name (marking a name again moves it).

use crate::model::Document;
use crate::project::packed;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DIR: &str = "handoffs";

#[derive(Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub name: String,
    pub doc: Document,
}

#[derive(Serialize, Deserialize)]
pub struct Handoff {
    pub name: String,
    /// Unix milliseconds.
    pub saved: u64,
    pub pages: Vec<Page>,
}

/// A handoff file without its pages: listing must not build every document.
#[derive(Deserialize)]
pub struct Header {
    pub name: String,
    pub saved: u64,
}

fn dir(project: &Path) -> PathBuf {
    project.join(DIR)
}

/// The file stem of a name: letters and digits, the rest as `-`.
fn stem(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "handoff".into() } else { s }
}

pub fn save(project: &Path, h: &Handoff) -> std::io::Result<()> {
    crate::vfs::create_dir_all(dir(project))?;
    let json = serde_json::to_string(h).map_err(std::io::Error::other)?;
    packed::write(&packed::path(&dir(project), &stem(&h.name)), &json)
}

/// Every handoff point, newest first.
pub fn list(project: &Path) -> Vec<Header> {
    let Ok(entries) = crate::vfs::read_dir(dir(project)) else { return Vec::new() };
    let mut out: Vec<Header> = entries
        .flatten()
        .filter_map(|e| {
            let id = packed::stem(e.file_name().to_str()?)?.to_owned();
            serde_json::from_str(&packed::read(&packed::path(&dir(project), &id)).ok()?).ok()
        })
        .collect();
    out.sort_by_key(|h| std::cmp::Reverse(h.saved));
    out
}

pub fn load(project: &Path, name: &str) -> Result<Handoff, String> {
    let json = packed::read(&packed::path(&dir(project), &stem(name))).map_err(|_| {
        let known: Vec<String> = list(project).into_iter().map(|h| h.name).collect();
        format!("no handoff point named `{name}` (known: {})", if known.is_empty() { "none, mark one with mark_handoff".into() } else { known.join(", ") })
    })?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_marked_again_moves_and_lists_once() {
        let dir = std::env::temp_dir().join(format!("papercraft-handoffs-{:016x}", fastrand::u64(..)));
        let h = |saved| Handoff { name: "Design v1".into(), saved, pages: vec![Page { id: "p".into(), name: "P".into(), doc: Document::default() }] };
        save(&dir, &h(1)).unwrap();
        save(&dir, &h(2)).unwrap();
        let all = list(&dir);
        assert_eq!((all.len(), all[0].saved), (1, 2));
        assert_eq!(load(&dir, "design v1").unwrap().pages.len(), 1, "names match by their stem");
        assert!(load(&dir, "v2").err().is_some_and(|e| e.contains("Design v1")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
