//! Shared projects: the relay keeps a project's files on disk so several tabs (and devices) can
//! open and edit the same project.
//!
//! ```text
//! GET    /p/{token}/files        [{"path","size","mtime"}]
//! GET    /p/{token}/f/{path}     file bytes
//! PUT    /p/{token}/f/{path}     write (creates the project); answers the file's mtime
//! DELETE /p/{token}/f/{path}     remove
//! GET    /p/{token}/ws?client=<id>   pushes {"op","path","rev","client"} for other clients' changes
//! ```
//!
//! The token is the only credential, like the agent link: anyone holding it can read and edit.
//! A bad token or path is a plain 404.
//!
//! With `Data::library` the relay is a personal library instead: every project in the data
//! directory is listed, so a device needs no link to find them (the network is the boundary):
//!
//! ```text
//! GET    /library                JSON array of every project's token
//! DELETE /p/{token}              move the project to `.trash/` (kept on disk); later `files` answer 410
//! ```

use super::relay_proto;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::broadcast;

/// Biggest file a tab may upload (screenshots and assets are big).
const MAX_FILE_BYTES: usize = 64 << 20;
/// Changes buffered per project for a slow socket; a lagging tab resyncs from `/files`.
const EVENT_BACKLOG: usize = 256;
/// Header and query name a tab uses to identify itself, so it can skip its own echo.
const CLIENT_HEADER: &str = "x-client";

/// One change, as pushed to the other tabs.
#[derive(Serialize, Clone)]
struct Event {
    op: &'static str,
    path: String,
    rev: u64,
    client: String,
}

struct Project {
    rev: AtomicU64,
    tx: broadcast::Sender<Arc<Event>>,
}

/// Where deleted library projects go, inside the data directory.
const TRASH: &str = ".trash";

/// The `--data` directory, and whether it is a library (`--library`).
#[derive(Clone)]
pub struct Data {
    pub dir: PathBuf,
    pub library: bool,
}

/// The `--data` directory and the live state of the projects in it.
pub struct Store {
    root: PathBuf,
    library: bool,
    projects: Mutex<HashMap<String, Arc<Project>>>,
}

impl Store {
    pub fn new(data: Data) -> Arc<Store> {
        Arc::new(Store {
            root: data.dir,
            library: data.library,
            projects: Mutex::new(HashMap::new()),
        })
    }

    fn project(&self, token: &str) -> Arc<Project> {
        let mut map = self.projects.lock().unwrap_or_else(PoisonError::into_inner);
        map.entry(token.to_string())
            .or_insert_with(|| {
                Arc::new(Project {
                    rev: AtomicU64::new(0),
                    tx: broadcast::channel(EVENT_BACKLOG).0,
                })
            })
            .clone()
    }

    /// True when `token` was deleted: its project sits in the trash and is gone from the root.
    fn was_deleted(&self, token: &str) -> bool {
        let prefix = format!("{token}-");
        std::fs::read_dir(self.root.join(TRASH)).is_ok_and(|mut d| {
            d.any(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with(&prefix)))
        })
    }

    fn publish(&self, token: &str, op: &'static str, path: String, client: String) {
        let project = self.project(token);
        let rev = project.rev.fetch_add(1, Ordering::Relaxed) + 1;
        // No receiver is fine: nobody else has the project open.
        let _ = project.tx.send(Arc::new(Event {
            op,
            path,
            rev,
            client,
        }));
    }
}

/// `files/` relative path, or `None` for anything that could leave the project directory.
fn clean(path: &str) -> Option<PathBuf> {
    let p = FsPath::new(path);
    if path.is_empty() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(p.to_path_buf())
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

/// The project directory and cleaned file path for a request, or the 404 to answer.
fn target(store: &Store, token: &str, path: &str) -> Result<(PathBuf, PathBuf), Response> {
    if !relay_proto::valid_token(token) {
        return Err(not_found());
    }
    let rel = clean(path).ok_or_else(not_found)?;
    let dir = store.root.join(token);
    Ok((dir.join(&rel), rel))
}

fn client_of(headers: &HeaderMap) -> String {
    headers
        .get(CLIENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// Last-modified time in ns; clients compare it for equality only, to spot remote changes.
fn mtime_ns(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64)
}

fn walk(dir: &FsPath, base: &FsPath, out: &mut Vec<serde_json::Value>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, out);
        } else if let (Ok(rel), Ok(meta)) = (p.strip_prefix(base), e.metadata()) {
            // Temp files of an upload in flight are not part of the project.
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !rel.ends_with(".part") {
                out.push(serde_json::json!({ "path": rel, "size": meta.len(), "mtime": mtime_ns(&meta) }));
            }
        }
    }
}

async fn list(State(store): State<Arc<Store>>, Path(token): Path<String>) -> Response {
    if !relay_proto::valid_token(&token) {
        return not_found();
    }
    let dir = store.root.join(&token);
    if !dir.is_dir() {
        if store.was_deleted(&token) {
            return StatusCode::GONE.into_response();
        }
        return not_found();
    }
    let files = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        walk(&dir, &dir, &mut out);
        out
    })
    .await
    .unwrap_or_default();
    axum::Json(files).into_response()
}

/// Every project's token (library mode).
async fn library(State(store): State<Arc<Store>>) -> Response {
    let mut tokens: Vec<String> = std::fs::read_dir(&store.root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| relay_proto::valid_token(name))
        .collect();
    tokens.sort();
    axum::Json(tokens).into_response()
}

/// Delete a project: it moves to `.trash/<token>-<ms>` so nothing is lost on the server.
async fn delete_project(State(store): State<Arc<Store>>, Path(token): Path<String>) -> Response {
    if !relay_proto::valid_token(&token) {
        return not_found();
    }
    let dir = store.root.join(&token);
    if !dir.is_dir() {
        return not_found();
    }
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let trash = store.root.join(TRASH);
    let moved = tokio::fs::create_dir_all(&trash).await.is_ok()
        && tokio::fs::rename(&dir, trash.join(format!("{token}-{ms}")))
            .await
            .is_ok();
    if !moved {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    store
        .projects
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&token);
    StatusCode::NO_CONTENT.into_response()
}

async fn read(
    State(store): State<Arc<Store>>,
    Path((token, path)): Path<(String, String)>,
) -> Response {
    let (file, _) = match target(&store, &token, &path) {
        Ok(t) => t,
        Err(r) => return r,
    };
    match tokio::fs::read(&file).await {
        Ok(bytes) => bytes.into_response(),
        Err(_) => not_found(),
    }
}

async fn write(
    State(store): State<Arc<Store>>,
    Path((token, path)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (file, rel) = match target(&store, &token, &path) {
        Ok(t) => t,
        Err(r) => return r,
    };
    if let Some(parent) = file.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    // Write beside, then rename, so a reader never sees a half-written file.
    let tmp = file.with_extension("part");
    if tokio::fs::write(&tmp, &body).await.is_err() || tokio::fs::rename(&tmp, &file).await.is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    store.publish(
        &token,
        "put",
        rel.to_string_lossy().replace('\\', "/"),
        client_of(&headers),
    );
    // The new stamp, so the writer does not mistake its own upload for someone else's change.
    let stamp = tokio::fs::metadata(&file).await.map_or(0, |m| mtime_ns(&m));
    stamp.to_string().into_response()
}

async fn remove(
    State(store): State<Arc<Store>>,
    Path((token, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let (file, rel) = match target(&store, &token, &path) {
        Ok(t) => t,
        Err(r) => return r,
    };
    if tokio::fs::remove_file(&file).await.is_err() {
        return not_found();
    }
    store.publish(
        &token,
        "del",
        rel.to_string_lossy().replace('\\', "/"),
        client_of(&headers),
    );
    StatusCode::NO_CONTENT.into_response()
}

async fn socket(
    State(store): State<Arc<Store>>,
    Path(token): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !relay_proto::valid_token(&token) {
        return not_found();
    }
    let me = q.get("client").cloned().unwrap_or_default();
    let rx = store.project(&token).tx.subscribe();
    upgrade
        .on_upgrade(move |s| serve(s, rx, me))
        .into_response()
}

/// Push other clients' changes until the socket closes.
async fn serve(mut socket: WebSocket, mut rx: broadcast::Receiver<Arc<Event>>, me: String) {
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(ev) if ev.client != me || me.is_empty() => {
                    let Ok(json) = serde_json::to_string(&*ev) else { continue };
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                // Missed events: tell the tab to resync everything.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let resync = r#"{"op":"resync"}"#;
                    if socket.send(Message::Text(resync.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
            m = socket.recv() => match m {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

pub fn router(store: Arc<Store>) -> axum::Router {
    let mut app = axum::Router::new()
        .route("/p/{token}/files", get(list))
        .route("/p/{token}/f/{*path}", get(read).put(write).delete(remove))
        .route("/p/{token}/ws", get(socket));
    if store.library {
        app = app
            .route("/library", get(library))
            .route("/p/{token}", axum::routing::delete(delete_project));
    }
    app.layer(DefaultBodyLimit::max(MAX_FILE_BYTES))
        .with_state(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_project() {
        assert_eq!(clean("pages/a.json"), Some(PathBuf::from("pages/a.json")));
        for bad in ["", "../x", "a/../../x", "/etc/passwd", "./x"] {
            assert_eq!(clean(bad), None, "{bad:?}");
        }
    }
}
