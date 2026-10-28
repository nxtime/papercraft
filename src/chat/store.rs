//! Chat files in the project folder: `chats/<id>.json`, plus `chats/<id>.api.json` for the raw
//! Anthropic API messages (exact replay, thinking blocks included).

use super::{valid_id, Chat, ChatSummary, TMP_DIR};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
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

/// Reads chat `id`. Screenshots still inline (older chats) move to files, and the slimmer chat
/// is written back at once, so the move happens only once.
pub fn load(project: &Path, id: &str) -> Option<Chat> {
    if !valid_id(id) {
        return None;
    }
    let mut chat: Chat = serde_json::from_slice(&vfs::read(file_of(project, id)).ok()?).ok()?;
    let mut moved = false;
    for t in chat.messages.iter_mut().flat_map(|m| &mut m.tools) {
        if let Some(r) = &mut t.result {
            moved |= offload_images(project, id, &t.id, r);
        }
    }
    if moved && let Err(e) = save(project, &chat) {
        log::warn!("could not save chat {id}: {e}");
    }
    Some(chat)
}

/// How long agents' screenshots are kept; an older one shows no thumbnail in its chat.
pub const RESULT_IMAGES_KEEP: std::time::Duration = std::time::Duration::from_secs(14 * 24 * 60 * 60);
const RESULT_PREFIX: &str = "result-";

/// Deletes the agents' screenshots (not the person's own images) older than `keep` from every
/// chat's image folder of `project`, so long chats do not pile up on disk.
pub fn prune_result_images(project: &Path, keep: std::time::Duration) {
    let Ok(chats) = vfs::read_dir(project.join(TMP_DIR)) else { return };
    for folder in chats.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
        let Ok(files) = vfs::read_dir(folder.path()) else { continue };
        for f in files.flatten() {
            let old = f.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|age| age > keep));
            if old && f.file_name().to_string_lossy().starts_with(RESULT_PREFIX) && let Err(e) = vfs::remove_file(f.path()) {
                log::warn!("could not remove {}: {e}", f.path().display());
            }
        }
    }
}

/// Inline images shorter than this stay in the chat.
const OFFLOAD_MIN: usize = 4096;

/// An image block of a tool result: its mime type and base64 data (inline), or its file.
pub fn image_data(block: &Value) -> Option<(&str, ImageSrc<'_>)> {
    if block["type"] != "image" {
        return None;
    }
    let mime = block["source"]["media_type"].as_str().or_else(|| block["mimeType"].as_str()).unwrap_or("image/png");
    if let Some(path) = block["path"].as_str() {
        return Some((mime, ImageSrc::File(path)));
    }
    let data = block["source"]["data"].as_str().or_else(|| block["data"].as_str())?;
    Some((mime, ImageSrc::Base64(data)))
}

pub enum ImageSrc<'a> {
    Base64(&'a str),
    File(&'a str),
}

impl ImageSrc<'_> {
    pub fn bytes(&self) -> Option<Vec<u8>> {
        match self {
            ImageSrc::Base64(d) => STANDARD.decode(d.as_bytes()).ok(),
            ImageSrc::File(p) => vfs::read(p).ok(),
        }
    }
}

/// Moves the inline images of a tool result into the chat's image folder, leaving their path:
/// a long chat's screenshots would otherwise stay in memory and be rewritten with every save.
/// An image that cannot be written stays inline. True when any image moved.
pub fn offload_images(project: &Path, chat_id: &str, tool_id: &str, result: &mut Value) -> bool {
    let (Some(blocks), Some(folder)) = (result.as_array_mut(), tmp_of(project, chat_id)) else { return false };
    let mut moved = false;
    let name: String = tool_id.chars().filter(|c| c.is_ascii_alphanumeric() || ['-', '_'].contains(c)).collect();
    for (n, b) in blocks.iter_mut().enumerate() {
        let Some((mime, ImageSrc::Base64(data))) = image_data(b) else { continue };
        if data.len() < OFFLOAD_MIN {
            continue;
        }
        let Ok(bytes) = STANDARD.decode(data.as_bytes()) else { continue };
        let ext = mime.strip_prefix("image/").filter(|e| ["png", "jpeg", "webp", "gif"].contains(e)).unwrap_or("png");
        let path = folder.join(format!("{RESULT_PREFIX}{name}-{n}.{ext}"));
        let mime = mime.to_owned();
        // Named by tool id and position, so a file already there is this image.
        if !vfs::exists(&path)
            && let Err(e) = vfs::create_dir_all(&folder).and_then(|()| vfs::write(&path, &bytes))
        {
            log::warn!("could not save a chat image: {e}");
            continue;
        }
        *b = serde_json::json!({"type": "image", "mimeType": mime, "path": path.to_string_lossy()});
        moved = true;
    }
    moved
}

/// Every readable chat, newest updated first, without messages (`load` reads one whole).
/// Unreadable files are skipped.
pub fn list(project: &Path) -> Vec<ChatSummary> {
    let Ok(entries) = vfs::read_dir(dir(project)) else {
        return Vec::new();
    };
    let mut out: Vec<ChatSummary> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter(|e| !e.path().to_string_lossy().ends_with(API_SUFFIX))
        .filter_map(|e| serde_json::from_slice::<ChatSummary>(&vfs::read(e.path()).ok()?).ok())
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
                agent: None,
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
                session_cost: Some(0.5),
                usage: Some(Usage {
                    input: 10,
                    output: 4,
                    cache_read: 2,
                    cache_write: 0,
                    cost_usd: Some(0.5),
                }),
                at: 5,
            }],
            issues: [("t1".to_owned(), crate::chat::IssueOutcome::Filed { number: 7, url: "u".into(), existing: false })].into(),
        }
    }

    #[test]
    fn screenshots_move_out_of_the_chat_into_files() {
        let dir = temp_project();
        let png = vec![7u8; OFFLOAD_MIN];
        let mut c = chat("shots", 1);
        c.messages[0].tools[0].result = Some(serde_json::json!([{"type": "image", "mimeType": "image/png", "data": STANDARD.encode(&png)}, {"type": "text", "text": "ok"}]));
        save(&dir, &c).unwrap();
        let loaded = load(&dir, "shots").unwrap();
        let r = loaded.messages[0].tools[0].result.as_ref().unwrap();
        let Some((mime, ImageSrc::File(path))) = image_data(&r[0]) else { panic!("still inline: {r}") };
        let on_disk = std::fs::read_to_string(file_of(&dir, "shots")).unwrap();
        assert!(on_disk.contains(path) && !on_disk.contains(&STANDARD.encode(&png)), "the slimmer chat was written back");
        assert_eq!((mime, ImageSrc::File(path).bytes()), ("image/png", Some(png)));
        assert_eq!(r[1]["text"], "ok");
        remove_tmp(&dir, "shots");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn old_agent_screenshots_are_pruned_and_the_persons_images_kept() {
        let dir = temp_project();
        let folder = dir.join(TMP_DIR).join("c1");
        std::fs::create_dir_all(&folder).unwrap();
        let month_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 24 * 60 * 60);
        for name in ["result-t1-0.png", "result-t2-0.png", "1792910692197-crop.png"] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        for name in ["result-t1-0.png", "1792910692197-crop.png"] {
            std::fs::File::options().write(true).open(folder.join(name)).unwrap().set_modified(month_ago).unwrap();
        }
        prune_result_images(&dir, RESULT_IMAGES_KEEP);
        let mut left: Vec<String> = std::fs::read_dir(&folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["1792910692197-crop.png", "result-t2-0.png"]);
        std::fs::remove_dir_all(dir).ok();
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

