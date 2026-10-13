//! Whole-library backups as a zip of the store folder, so a backup made in the browser (where
//! there is no folder to open) can be restored there or anywhere else. Restoring brings back
//! the projects only; the trash and the shape library are left as they are.

use super::zip::{self, Entry};
use crate::project::TRASH_DIR;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

fn collect(dir: &Path, prefix: &str, out: &mut Vec<Entry>) -> std::io::Result<()> {
    let mut entries: Vec<_> = crate::vfs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = format!("{prefix}{}", e.file_name().to_string_lossy());
        if e.file_type()?.is_dir() {
            collect(&e.path(), &format!("{name}/"), out)?;
        } else {
            out.push((name, crate::vfs::read(e.path())?));
        }
    }
    Ok(())
}

/// A zip of the whole store folder: every project, the trash and the shape library.
pub fn pack(root: &Path) -> Result<Vec<u8>, String> {
    let mut files = Vec::new();
    collect(root, "", &mut files).map_err(|e| e.to_string())?;
    if files.is_empty() {
        return Err("there are no projects to back up".into());
    }
    Ok(zip::write(&files))
}

/// Whether `bytes` look like a zip (the import path then treats the file as a backup).
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

/// A relative path made only of normal components, so an archive cannot write outside `root`.
fn safe_relative(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    p.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then(|| p.to_path_buf())
}

/// Restore the projects in a backup next to the existing ones (never overwriting: a folder
/// that already exists gets a numbered sibling). Returns the folders created.
pub fn unpack(bytes: &[u8], root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = zip::read(bytes)?;
    let mut tops: Vec<String> = Vec::new();
    for (name, _) in &entries {
        let Some(rel) = safe_relative(name) else {
            return Err(format!("unsafe path in the backup: {name}"));
        };
        let mut parts = rel.components();
        let (Some(top), Some(_)) = (parts.next(), parts.next()) else {
            continue; // loose file at the top level
        };
        let top = top.as_os_str().to_string_lossy().into_owned();
        if top != TRASH_DIR && !tops.contains(&top) {
            tops.push(top);
        }
    }
    let mut taken: HashSet<String> = HashSet::new();
    let mut created = Vec::new();
    for top in tops {
        let has_project = entries
            .iter()
            .any(|(n, _)| n == &format!("{top}/{}", crate::project::MANIFEST));
        if !has_project {
            continue;
        }
        let mut target = top.clone();
        let mut i = 2;
        while taken.contains(&target) || crate::vfs::exists(&root.join(&target)) {
            target = format!("{top} {i}");
            i += 1;
        }
        taken.insert(target.clone());
        for (name, data) in &entries {
            let Some(rest) = name.strip_prefix(&format!("{top}/")) else {
                continue;
            };
            let path = root.join(&target).join(rest);
            if let Some(dir) = path.parent() {
                crate::vfs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            crate::vfs::write(&path, data).map_err(|e| e.to_string())?;
        }
        created.push(root.join(target));
    }
    if created.is_empty() {
        return Err("the zip holds no PaperCraft projects".into());
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Store;

    fn store() -> Store {
        Store::open(
            std::env::temp_dir().join(format!("papercraft-backup-{:016x}", fastrand::u64(..))),
        )
        .unwrap()
    }

    #[test]
    fn backup_round_trips_into_another_store() {
        let a = store();
        a.create("One").unwrap();
        a.create("Two").unwrap();
        let zip = pack(&a.root).unwrap();
        assert!(is_zip(&zip));
        let b = store();
        let made = unpack(&zip, &b.root).unwrap();
        assert_eq!(made.len(), 2);
        let mut names: Vec<_> = b.list().into_iter().map(|e| e.manifest.name).collect();
        names.sort();
        assert_eq!(names, ["One", "Two"]);
        // Restoring again never overwrites.
        unpack(&zip, &b.root).unwrap();
        assert_eq!(b.list().len(), 4);
    }

    #[test]
    fn refuses_path_traversal() {
        let zip = zip::write(&[("../evil/project.json".into(), b"{}".to_vec())]);
        assert!(unpack(&zip, &store().root).is_err());
    }

    #[test]
    fn empty_store_has_nothing_to_back_up() {
        assert!(pack(&store().root).is_err());
    }
}
