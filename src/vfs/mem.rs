//! In-memory tree persisted to IndexedDB (browser).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
let dbp;
function db() {
  return (dbp ??= new Promise((res, rej) => {
    const r = indexedDB.open('papercraft', 1);
    r.onupgradeneeded = () => r.result.createObjectStore('files');
    r.onsuccess = () => res(r.result);
    r.onerror = () => rej(r.error);
  }));
}
function store(d, mode) { return d.transaction('files', mode).objectStore('files'); }
export async function idb_load_all() {
  const d = await db();
  return new Promise((res, rej) => {
    const out = [];
    const rq = store(d, 'readonly').openCursor();
    rq.onsuccess = () => {
      const c = rq.result;
      if (c) { out.push([c.key, c.value]); c.continue(); } else res(out);
    };
    rq.onerror = () => rej(rq.error);
  });
}
export async function idb_put(key, bytes) {
  // `bytes` views wasm memory, which Rust reuses once this call returns: copy before awaiting.
  bytes = bytes.slice();
  const d = await db();
  return new Promise((res, rej) => {
    const rq = store(d, 'readwrite').put(bytes, key);
    rq.onsuccess = () => res();
    rq.onerror = () => rej(rq.error);
  });
}
export async function idb_delete(key) {
  const d = await db();
  return new Promise((res, rej) => {
    const rq = store(d, 'readwrite').delete(key);
    rq.onsuccess = () => res();
    rq.onerror = () => rej(rq.error);
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn idb_load_all() -> std::result::Result<JsValue, JsValue>;
    fn idb_put(key: &str, bytes: &[u8]) -> js_sys::Promise;
    fn idb_delete(key: &str) -> js_sys::Promise;
}

#[derive(Default)]
struct Tree {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
}

thread_local! {
    static TREE: RefCell<Tree> = RefCell::new(Tree::default());
}

fn key(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

thread_local! {
    /// The latest IndexedDB write that failed (quota, private mode, blocked database). The files
    /// stay in memory, so the shell shows this and the user can still download a backup.
    static PERSIST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Take the latest failed IndexedDB write, if any.
pub fn take_persist_error() -> Option<String> {
    PERSIST_ERROR.with(|e| e.borrow_mut().take())
}

/// A failed write is waiting to be shown.
pub fn has_persist_error() -> bool {
    PERSIST_ERROR.with(|e| e.borrow().is_some())
}

fn persist(p: &Path, what: &'static str, promise: js_sys::Promise) {
    let name = p.display().to_string();
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = wasm_bindgen_futures::JsFuture::from(promise).await {
            let why = e
                .as_string()
                .or_else(|| {
                    js_sys::Reflect::get(&e, &"message".into())
                        .ok()?
                        .as_string()
                })
                .unwrap_or_else(|| "storage error".into());
            PERSIST_ERROR.with(|slot| {
                *slot.borrow_mut() =
                    Some(format!("Could not {what} {name} in browser storage: {why}"));
            });
        }
    });
}

fn persist_put(p: &Path, data: &[u8]) {
    persist(p, "save", idb_put(&key(p), data));
}

fn persist_delete(p: &Path) {
    persist(p, "delete", idb_delete(&key(p)));
}

fn not_found(p: &Path) -> Error {
    Error::new(ErrorKind::NotFound, format!("{}: not found", p.display()))
}

/// Read everything back from IndexedDB. Call once before the app starts.
pub async fn load() -> std::result::Result<(), String> {
    let rows = idb_load_all().await.map_err(|e| format!("{e:?}"))?;
    TREE.with(|t| {
        let mut t = t.borrow_mut();
        for row in js_sys::Array::from(&rows).iter() {
            let row = js_sys::Array::from(&row);
            let (Some(k), v) = (row.get(0).as_string(), row.get(1)) else {
                continue;
            };
            let p = PathBuf::from(k);
            for a in p.ancestors().skip(1) {
                t.dirs.insert(a.to_path_buf());
            }
            t.files.insert(p, js_sys::Uint8Array::new(&v).to_vec());
        }
    });
    Ok(())
}

pub fn is_file(p: &Path) -> bool {
    TREE.with(|t| t.borrow().files.contains_key(p))
}

pub fn is_dir(p: &Path) -> bool {
    TREE.with(|t| {
        let t = t.borrow();
        t.dirs.contains(p) || t.files.keys().any(|f| f.starts_with(p) && f != p)
    })
}

pub fn exists(p: &Path) -> bool {
    is_file(p) || is_dir(p)
}

pub fn create_dir_all(p: impl AsRef<Path>) -> Result<()> {
    TREE.with(|t| {
        let mut t = t.borrow_mut();
        for a in p.as_ref().ancestors() {
            if !a.as_os_str().is_empty() {
                t.dirs.insert(a.to_path_buf());
            }
        }
    });
    Ok(())
}

pub fn read(p: impl AsRef<Path>) -> Result<Vec<u8>> {
    let p = p.as_ref();
    TREE.with(|t| t.borrow().files.get(p).cloned())
        .ok_or_else(|| not_found(p))
}

pub fn read_to_string(p: impl AsRef<Path>) -> Result<String> {
    String::from_utf8(read(p)?).map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

pub fn write(p: impl AsRef<Path>, data: impl AsRef<[u8]>) -> Result<()> {
    let (p, data) = (p.as_ref(), data.as_ref());
    if let Some(dir) = p.parent() {
        create_dir_all(dir)?;
    }
    TREE.with(|t| t.borrow_mut().files.insert(p.to_path_buf(), data.to_vec()));
    persist_put(p, data);
    Ok(())
}

pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<u64> {
    let data = read(from)?;
    write(to, &data)?;
    Ok(data.len() as u64)
}

pub fn remove_file(p: impl AsRef<Path>) -> Result<()> {
    let p = p.as_ref();
    TREE.with(|t| t.borrow_mut().files.remove(p))
        .ok_or_else(|| not_found(p))?;
    persist_delete(p);
    Ok(())
}

pub fn remove_dir_all(p: impl AsRef<Path>) -> Result<()> {
    let p = p.as_ref();
    if !exists(p) {
        return Err(not_found(p));
    }
    let gone: Vec<PathBuf> = TREE.with(|t| {
        let mut t = t.borrow_mut();
        t.dirs.retain(|d| !d.starts_with(p));
        let gone: Vec<PathBuf> = t
            .files
            .keys()
            .filter(|f| f.starts_with(p))
            .cloned()
            .collect();
        for f in &gone {
            t.files.remove(f);
        }
        gone
    });
    for f in gone {
        persist_delete(&f);
    }
    Ok(())
}

/// Moves a file, or a directory with everything under it.
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    if !exists(from) {
        return Err(not_found(from));
    }
    if let Some(dir) = to.parent() {
        create_dir_all(dir)?;
    }
    let moved: Vec<(PathBuf, PathBuf, Vec<u8>)> = TREE.with(|t| {
        let mut t = t.borrow_mut();
        let dirs: Vec<PathBuf> = t
            .dirs
            .iter()
            .filter(|d| d.starts_with(from))
            .cloned()
            .collect();
        for d in dirs {
            t.dirs.remove(&d);
            if let Ok(rest) = d.strip_prefix(from) {
                t.dirs.insert(to.join(rest));
            }
        }
        let files: Vec<PathBuf> = t
            .files
            .keys()
            .filter(|f| f.starts_with(from))
            .cloned()
            .collect();
        files
            .into_iter()
            .filter_map(|f| {
                let data = t.files.remove(&f)?;
                let dest = to.join(f.strip_prefix(from).ok()?);
                t.files.insert(dest.clone(), data.clone());
                Some((f, dest, data))
            })
            .collect()
    });
    for (old, new, data) in moved {
        persist_put(&new, &data);
        persist_delete(&old);
    }
    Ok(())
}

pub struct Metadata {
    len: u64,
}

impl Metadata {
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn modified(&self) -> Result<SystemTime> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "no mtime in the browser",
        ))
    }
}

pub fn metadata(p: impl AsRef<Path>) -> Result<Metadata> {
    let p = p.as_ref();
    match TREE.with(|t| t.borrow().files.get(p).map(|f| f.len() as u64)) {
        Some(len) => Ok(Metadata { len }),
        None if is_dir(p) => Ok(Metadata { len: 0 }),
        None => Err(not_found(p)),
    }
}

#[derive(Clone, Copy)]
pub struct FileType {
    dir: bool,
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.dir
    }
    pub fn is_file(&self) -> bool {
        !self.dir
    }
}

pub struct DirEntry {
    path: PathBuf,
    dir: bool,
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
    pub fn file_name(&self) -> OsString {
        self.path.file_name().unwrap_or_default().to_owned()
    }
    pub fn file_type(&self) -> Result<FileType> {
        Ok(FileType { dir: self.dir })
    }
    pub fn metadata(&self) -> Result<Metadata> {
        metadata(&self.path)
    }
}

pub struct ReadDir(std::vec::IntoIter<DirEntry>);

impl Iterator for ReadDir {
    type Item = Result<DirEntry>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Ok)
    }
}

pub fn read_dir(p: impl AsRef<Path>) -> Result<ReadDir> {
    let p = p.as_ref();
    if !is_dir(p) {
        return Err(not_found(p));
    }
    let mut seen: BTreeMap<PathBuf, bool> = BTreeMap::new();
    TREE.with(|t| {
        let t = t.borrow();
        for d in &t.dirs {
            if d.parent() == Some(p) {
                seen.insert(d.clone(), true);
            }
        }
        for f in t.files.keys() {
            let Ok(rest) = f.strip_prefix(p) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else { continue };
            let child = p.join(first);
            let is_dir = parts.next().is_some();
            seen.entry(child)
                .and_modify(|d| *d |= is_dir)
                .or_insert(is_dir);
        }
    });
    Ok(ReadDir(
        seen.into_iter()
            .map(|(path, dir)| DirEntry { path, dir })
            .collect::<Vec<_>>()
            .into_iter(),
    ))
}
