//! Chat files in the project folder: `chats/<id>.json`, plus `chats/<id>.api.json` for the raw
//! Anthropic API messages (exact replay, thinking blocks included).

use super::{valid_id, Chat, TMP_DIR};
use crate::vfs;
use std::path::{Path, PathBuf};

const DIR: &str = "chats";
const API_SUFFIX: &str = ".api.json";

pub use crate::project::format::new_id;

fn dir(project: &Path) -> PathBuf {
    project.join(DIR)
}

fn file_of(project: &Path, id: &str) -> PathBuf {
    dir(project).join(format!("{id}.json"))
}

pub fn save(project: &Path, chat: &Chat) -> std::io::Result<()> {
    if !valid_id(&chat.id) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid chat id"));
    }
    vfs::create_dir_all(dir(project))?;
    let bytes = serde_json::to_vec(chat).map_err(std::io::Error::other)?;
    vfs::write(file_of(project, &chat.id), bytes)
}

pub fn load(project: &Path, id: &str) -> Option<Chat> {
    if !valid_id(id) {
        return None;
    }
    serde_json::from_slice(&vfs::read(file_of(project, id)).ok()?).ok()
}

/// Every readable chat, newest updated first. Unreadable files are skipped.
pub fn list(project: &Path) -> Vec<Chat> {
    let Ok(entries) = vfs::read_dir(dir(project)) else {
        return Vec::new();
    };
    let mut out: Vec<Chat> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter(|e| !e.path().to_string_lossy().ends_with(API_SUFFIX))
        .filter_map(|e| serde_json::from_slice::<Chat>(&vfs::read(e.path()).ok()?).ok())
        .filter(|c| valid_id(&c.id))
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.updated));
    out
}

/// Removes the chat and its API replay file, if there is one.
pub fn delete(project: &Path, id: &str) {
    if !valid_id(id) {
        return;
    }
    let _ = vfs::remove_file(file_of(project, id));
    let _ = vfs::remove_file(dir(project).join(format!("{id}{API_SUFFIX}")));
}

/// Folder of the images saved for chat `id`; None when `id` is not a plain name.
pub fn tmp_of(project: &Path, id: &str) -> Option<PathBuf> {
    valid_id(id).then(|| project.join(TMP_DIR).join(id))
}

/// Removes the chat's saved images, only when the folder really lies inside
/// `<project>/.papercraft-chat-tmp` (a symlink pointing elsewhere is left alone).
pub fn remove_tmp(project: &Path, id: &str) {
    let Some(folder) = tmp_of(project, id) else { return };
    let (Ok(root), Ok(real)) = (std::fs::canonicalize(project.join(TMP_DIR)), std::fs::canonicalize(&folder)) else { return };
    if real != root && real.starts_with(&root) {
        let _ = vfs::remove_dir_all(real);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{Backend, BackendKind, Message, Role, ToolCall, Usage};

    fn temp_project() -> PathBuf {
        std::env::temp_dir().join(format!("papercraft-chat-{:016x}", fastrand::u64(..)))
    }

    fn chat(id: &str, updated: u64) -> Chat {
        Chat {
            id: id.into(),
            title: format!("Chat {id}"),
            created: 1,
            updated,
            backend: Backend {
                kind: BackendKind::Codex,
                model: Some("gpt-6.1-sol".into()),
                effort: None,
                endpoint: None,
                cli_session: Some("s1".into()),
            },
            messages: vec![Message {
                role: Role::Assistant,
                text: "Done".into(),
                tools: vec![ToolCall {
                    id: "t1".into(),
                    name: "create_artboard".into(),
                    args: serde_json::json!({ "w": 390 }),
                    result: Some(serde_json::json!({ "ok": true })),
                    error: None,
                    ms: Some(12),
                }],
                run_id: Some("r1".into()),
                context: Some(7),
                usage: Some(Usage {
                    input: 10,
                    output: 4,
                    cache_read: 2,
                    cache_write: 0,
                    cost_usd: Some(0.5),
                }),
                at: 5,
            }],
        }
    }

    #[test]
    fn chat_round_trips() {
        let dir = temp_project();
        let c = chat("aaaa", 3);
        save(&dir, &c).unwrap();
        assert_eq!(load(&dir, "aaaa"), Some(c));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn list_orders_newest_first_and_skips_unreadable() {
        let dir = temp_project();
        save(&dir, &chat("old", 1)).unwrap();
        save(&dir, &chat("new", 9)).unwrap();
        vfs::write(dir.join(DIR).join("broken.json"), b"{not json").unwrap();
        let ids: Vec<String> = list(&dir).into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["new", "old"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn delete_removes_chat_and_api_file() {
        let dir = temp_project();
        save(&dir, &chat("a", 1)).unwrap();
        vfs::write(dir.join(DIR).join("a.api.json"), b"[]").unwrap();
        delete(&dir, "a");
        assert!(!dir.join(DIR).join("a.json").exists());
        assert!(!dir.join(DIR).join("a.api.json").exists());
        assert!(list(&dir).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn hostile_ids_never_reach_the_file_system() {
        let dir = temp_project();
        let outside = dir.join("victim");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(dir.join(TMP_DIR)).unwrap();
        for id in ["..", "../victim", "a/b", ".", "", "a.b", "..\\victim"] {
            assert!(!valid_id(id), "{id}");
            assert!(tmp_of(&dir, id).is_none());
            remove_tmp(&dir, id);
            delete(&dir, id);
            assert!(load(&dir, id).is_none());
            assert!(save(&dir, &chat(id, 1)).is_err());
        }
        assert!(outside.exists());
        assert!(dir.join(TMP_DIR).exists(), "the tmp root itself survives");
        // A file on disk whose content claims a hostile id is not listed.
        std::fs::create_dir_all(dir.join(DIR)).unwrap();
        vfs::write(dir.join(DIR).join("x.json"), br#"{"id":"../victim"}"#).unwrap();
        assert!(list(&dir).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn remove_tmp_ignores_a_symlink_out_of_the_tmp_dir() {
        let dir = temp_project();
        let outside = dir.join("victim");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep"), b"x").unwrap();
        std::fs::create_dir_all(dir.join(TMP_DIR)).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join(TMP_DIR).join("abc")).unwrap();
        remove_tmp(&dir, "abc");
        assert!(outside.join("keep").exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn project_without_chats_dir_lists_empty() {
        let dir = temp_project();
        std::fs::create_dir_all(&dir).unwrap();
        assert!(list(&dir).is_empty());
        assert!(load(&dir, "x").is_none());
        std::fs::remove_dir_all(dir).ok();
    }
}
