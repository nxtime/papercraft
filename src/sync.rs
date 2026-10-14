//! Keep a project folder in step with a shared copy on a relay (`mcp::store`), from the browser
//! and from the native app alike (everything goes through [`crate::vfs`]).
//!
//! A shared project has a `.share` file in its folder: the server, the share token, and the
//! state of the last sync. Every [`POLL`] the engine compares three views of each file, the
//! server's, the folder's and the last synced one:
//!
//! * changed in the folder: upload (or delete) it. The folder wins a simultaneous change, so
//!   whoever is typing keeps their work and the other side sees it on its next poll;
//! * changed on the server only: download (or delete) it, and flag [`Status::take_changed`] so
//!   the shell reloads the open project.
//!
//! Granularity is a whole file (one page), last write wins.

use crate::mcp::relay_proto;
use crate::vfs;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// File in a project folder that marks it as shared.
pub const SHARE_FILE: &str = ".share";
/// How often a shared project asks the server what changed.
const POLL: Duration = Duration::from_secs(2);
/// Files that stay on this machine: the sync state and the home-screen preview.
const LOCAL_ONLY: [&str; 2] = [SHARE_FILE, "thumbnail.png"];

/// What the last sync knew about one file.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    /// The server's modified time (compared for equality only).
    remote: u64,
    /// Hash of the file's bytes here.
    local: u64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Share {
    /// `https://host[:port]`, no trailing slash.
    pub server: String,
    pub token: String,
    #[serde(default)]
    synced: BTreeMap<String, Stamp>,
}

impl Share {
    pub fn new(server: &str) -> Share {
        Share {
            server: server.trim_end_matches('/').to_string(),
            token: new_token(),
            synced: BTreeMap::new(),
        }
    }

    pub fn load(dir: &Path) -> Option<Share> {
        let text = vfs::read_to_string(dir.join(SHARE_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
        vfs::write(dir.join(SHARE_FILE), json).map_err(|e| e.to_string())
    }

    /// What to send to a collaborator: opening it in the web app joins the project.
    pub fn link(&self) -> String {
        format!("{}/?share={}", self.server, self.token)
    }

    /// The share a link such as `https://host/?share=<token>` stands for.
    pub fn from_link(link: &str) -> Result<Share, String> {
        let link = link.trim();
        let (server, query) = link
            .split_once("?share=")
            .ok_or("not a share link (it should end in ?share=<token>)")?;
        let token: String = query
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if !relay_proto::valid_token(&token) {
            return Err("the share token in this link is not valid".into());
        }
        let server = server.trim_end_matches('/');
        if !server.starts_with("http://") && !server.starts_with("https://") {
            return Err("the link must start with http:// or https://".into());
        }
        Ok(Share {
            server: server.to_string(),
            token,
            synced: BTreeMap::new(),
        })
    }

    fn base(&self) -> String {
        format!("{}/p/{}", self.server, self.token)
    }
}

fn new_token() -> String {
    (0..relay_proto::MIN_TOKEN_LEN)
        .map(|_| char::from_digit(fastrand::u32(0..16), 16).unwrap_or('0'))
        .collect()
}

/// FNV-1a: a stable fingerprint of a file's bytes (the std hasher may change between releases,
/// and the value is stored in `.share`).
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A file path from the server, only if it stays inside the project folder.
fn safe_path(rel: &str) -> Option<PathBuf> {
    let p = Path::new(rel);
    let ok = !rel.is_empty()
        && p.components().all(|c| matches!(c, Component::Normal(_)))
        && !LOCAL_ONLY.contains(&rel);
    ok.then(|| p.to_path_buf())
}

/// Every file under `dir` that syncs, as relative `/` paths with their fingerprints.
fn scan(dir: &Path) -> BTreeMap<String, u64> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, u64>) {
        let Ok(entries) = vfs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                walk(base, &p, out);
            } else if let (Ok(rel), Ok(bytes)) = (p.strip_prefix(base), vfs::read(&p)) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if safe_path(&rel).is_some() {
                    out.insert(rel, fingerprint(&bytes));
                }
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

#[derive(Deserialize)]
struct RemoteFile {
    path: String,
    mtime: u64,
}

async fn list_remote(base: &str) -> Result<BTreeMap<String, u64>, String> {
    let (code, body) = net::request("GET", &format!("{base}/files"), None).await?;
    match code {
        200 => {}
        // Nothing uploaded yet: the first sync of a freshly shared project.
        404 => return Ok(BTreeMap::new()),
        c => return Err(format!("the server answered {c}")),
    }
    let files: Vec<RemoteFile> = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok(files
        .into_iter()
        .filter(|f| safe_path(&f.path).is_some())
        .map(|f| (f.path, f.mtime))
        .collect())
}

/// One round: reconcile the folder with the server. True when the server's version of some
/// file was applied here, so the open project may need reloading.
pub async fn sync_once(dir: &Path) -> Result<bool, String> {
    let mut share = Share::load(dir).ok_or("this project is not shared")?;
    let base = share.base();
    let remote = list_remote(&base).await?;
    let local = scan(dir);
    let paths: BTreeSet<&String> = remote
        .keys()
        .chain(local.keys())
        .chain(share.synced.keys())
        .collect();
    let (mut synced, mut applied) = (share.synced.clone(), false);
    for path in paths {
        let (l, r) = (local.get(path).copied(), remote.get(path).copied());
        let was = synced.get(path).copied();
        let url = format!("{base}/f/{path}");
        let file = dir.join(path);
        if l != was.map(|s| s.local) {
            let Some(hash) = l else {
                let (code, _) = net::request("DELETE", &url, None).await?;
                if code != 204 && code != 404 {
                    return Err(format!("could not delete {path}: server answered {code}"));
                }
                synced.remove(path);
                continue;
            };
            let bytes = vfs::read(&file).map_err(|e| format!("{path}: {e}"))?;
            let (code, body) = net::request("PUT", &url, Some(bytes)).await?;
            if code != 200 {
                return Err(format!("could not upload {path}: server answered {code}"));
            }
            let remote = String::from_utf8_lossy(&body).trim().parse().unwrap_or(0);
            synced.insert(
                path.clone(),
                Stamp {
                    remote,
                    local: hash,
                },
            );
        } else if r != was.map(|s| s.remote) {
            applied = true;
            let Some(stamp) = r else {
                vfs::remove_file(&file).ok();
                synced.remove(path);
                continue;
            };
            let (code, bytes) = net::request("GET", &url, None).await?;
            if code != 200 {
                return Err(format!("could not download {path}: server answered {code}"));
            }
            if let Some(parent) = file.parent() {
                vfs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            vfs::write(&file, &bytes).map_err(|e| format!("{path}: {e}"))?;
            synced.insert(
                path.clone(),
                Stamp {
                    remote: stamp,
                    local: fingerprint(&bytes),
                },
            );
        }
    }
    if synced != share.synced {
        share.synced = synced;
        share.save(dir)?;
    }
    Ok(applied)
}

/// What the shell shows and acts on for one shared project.
#[derive(Default)]
pub struct Status {
    /// What to send a collaborator.
    pub link: String,
    stop: AtomicBool,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    error: Option<String>,
    synced_once: bool,
    changed: bool,
}

impl Status {
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The server's copy changed files in the folder since the last call.
    pub fn take_changed(&self) -> bool {
        std::mem::take(&mut self.inner().changed)
    }

    pub fn label(&self) -> String {
        let i = self.inner();
        match (&i.error, i.synced_once) {
            (Some(e), _) => format!("Sync error: {e}"),
            (None, true) => "Synced".into(),
            (None, false) => "Connecting".into(),
        }
    }
}

async fn run(dir: PathBuf, status: Arc<Status>) {
    while !status.stop.load(Ordering::Relaxed) {
        let round = sync_once(&dir).await;
        {
            let mut i = status.inner();
            match round {
                Ok(changed) => {
                    i.error = None;
                    i.synced_once = true;
                    i.changed |= changed;
                }
                Err(e) => i.error = Some(e),
            }
        }
        net::sleep(POLL).await;
    }
}

/// The shared projects this process is keeping in sync.
#[derive(Default)]
pub struct Sync {
    running: HashMap<PathBuf, Arc<Status>>,
}

impl Sync {
    /// Start syncing the project in `dir` (it must have a `.share` file). A second call for the
    /// same folder does nothing.
    pub fn start(&mut self, dir: &Path) {
        if self.running.contains_key(dir) || Share::load(dir).is_none() {
            return;
        }
        let status = Arc::new(Status {
            link: Share::load(dir).map(|s| s.link()).unwrap_or_default(),
            ..Status::default()
        });
        net::spawn(run(dir.to_path_buf(), status.clone()));
        self.running.insert(dir.to_path_buf(), status);
    }

    /// Resume every shared project under the store root (at startup).
    pub fn start_all(&mut self, root: &Path) {
        let Ok(entries) = vfs::read_dir(root) else {
            return;
        };
        for e in entries.flatten() {
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                self.start(&e.path());
            }
        }
    }

    /// Make `dir` a shared project on `server`; returns the link to hand out.
    pub fn share(&mut self, dir: &Path, server: &str) -> Result<String, String> {
        if let Some(existing) = Share::load(dir) {
            return Ok(existing.link());
        }
        let share = Share::new(server);
        share.save(dir)?;
        self.start(dir);
        Ok(share.link())
    }

    /// Download the project behind `link` into a new folder `dir` and keep it in sync.
    pub fn join(&mut self, dir: &Path, link: &str) -> Result<(), String> {
        let share = Share::from_link(link)?;
        vfs::create_dir_all(dir).map_err(|e| e.to_string())?;
        share.save(dir)?;
        self.start(dir);
        Ok(())
    }

    pub fn status(&self, dir: &Path) -> Option<Arc<Status>> {
        self.running.get(dir).cloned()
    }

    pub fn dirs(&self) -> Vec<PathBuf> {
        self.running.keys().cloned().collect()
    }

    pub fn is_active(&self) -> bool {
        !self.running.is_empty()
    }

    /// Stop syncing `dir` and forget its share (the files stay).
    pub fn unshare(&mut self, dir: &Path) {
        if let Some(s) = self.running.remove(dir) {
            s.stop.store(true, Ordering::Relaxed);
        }
        vfs::remove_file(dir.join(SHARE_FILE)).ok();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod net {
    use std::sync::OnceLock;
    use std::time::Duration;

    pub async fn request(
        method: &str,
        url: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>), String> {
        static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
        let client = CLIENT.get_or_init(reqwest::Client::new);
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let mut req = client.request(method, url).timeout(Duration::from_secs(60));
        if let Some(b) = body {
            req = req.body(b);
        }
        let res = req.send().await.map_err(|e| e.to_string())?;
        let code = res.status().as_u16();
        let bytes = res.bytes().await.map_err(|e| e.to_string())?;
        Ok((code, bytes.to_vec()))
    }

    pub async fn sleep(d: Duration) {
        tokio::time::sleep(d).await;
    }

    /// Own thread and runtime: the editor's event loop has none.
    pub fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
        std::thread::Builder::new()
            .name("papercraft-sync".into())
            .spawn(move || {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    rt.block_on(fut);
                }
            })
            .ok();
    }
}

#[cfg(target_arch = "wasm32")]
mod net {
    use std::time::Duration;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    fn js(e: JsValue) -> String {
        e.as_string().unwrap_or_else(|| format!("{e:?}"))
    }

    pub async fn request(
        method: &str,
        url: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>), String> {
        let init = web_sys::RequestInit::new();
        init.set_method(method);
        if let Some(b) = body {
            init.set_body(&js_sys::Uint8Array::from(b.as_slice()));
        }
        let window = web_sys::window().ok_or("no window")?;
        let res = JsFuture::from(window.fetch_with_str_and_init(url, &init))
            .await
            .map_err(js)?;
        let res: web_sys::Response = res.dyn_into().map_err(js)?;
        let buf = JsFuture::from(res.array_buffer().map_err(js)?)
            .await
            .map_err(js)?;
        Ok((res.status(), js_sys::Uint8Array::new(&buf).to_vec()))
    }

    pub async fn sleep(d: Duration) {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                    &resolve,
                    d.as_millis() as i32,
                );
            }
        });
        let _ = JsFuture::from(promise).await;
    }

    pub fn spawn(fut: impl Future<Output = ()> + 'static) {
        wasm_bindgen_futures::spawn_local(fut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_paths_cannot_leave_the_project() {
        assert!(safe_path("pages/a.json").is_some());
        for bad in [
            "",
            "../x",
            "a/../../x",
            "/etc/passwd",
            ".share",
            "thumbnail.png",
        ] {
            assert!(safe_path(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn links_round_trip() {
        let s = Share::new("https://papercraft.example.com/");
        let back = Share::from_link(&s.link()).unwrap();
        assert_eq!(
            (back.server.as_str(), back.token.as_str()),
            (s.server.as_str(), s.token.as_str())
        );
        assert!(Share::from_link("https://x/?share=zz").is_err());
        assert!(Share::from_link("https://x/").is_err());
        assert!(Share::from_link("ftp://x/?share=00000000000000000000000000000000").is_err());
    }
}
