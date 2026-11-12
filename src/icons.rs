//! Open icon libraries on disk: `<icons dir>/<set>/<name>.svg`.
//!
//! The app never bundles icons: `papercraft-cli icons add <set>` fetches a set from the npm
//! registry into the icons dir (`$PAPERCRAFT_ICONS`, else `<data dir>/papercraft/icons`), and
//! `write_html` places them with `<x-paper-icon name="lucide:check">`. Each set keeps the
//! licence file of the package it came from next to the icons.
//!
//! The browser has no icons dir. There the web server carries each set as one JSON bundle
//! (`/icon-sets/<set>.json`, `{"license": "...", "icons": {name: svg}}`, built by
//! `deploy/icon-sets.mjs`); [`set_source`] points the page at it, and a set is fetched the first
//! time it is needed and kept in memory. Until it arrives, [`load`] says so instead of failing
//! as if the icon did not exist.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Sets held in memory (browser bundles), by id: icon name to SVG.
static BUNDLES: Mutex<BTreeMap<String, BTreeMap<String, String>>> = Mutex::new(BTreeMap::new());
/// Sets being fetched (`None`) or whose fetch failed (`Some(why)`).
static FETCHES: Mutex<BTreeMap<String, Option<String>>> = Mutex::new(BTreeMap::new());
/// Where the bundles are served from (the page's origin), once the browser knows it.
static SOURCE: Mutex<Option<String>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Keep a downloaded bundle in memory. The licence text stays in the JSON on the server, next
/// to the icons it covers; only the icons are needed here.
pub fn add_bundle(set: &str, json: &[u8]) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Bundle {
        icons: BTreeMap<String, String>,
    }
    spec(set).ok_or_else(|| format!("`{set}` is not an icon set"))?;
    let b: Bundle = serde_json::from_slice(json).map_err(|e| format!("icon set `{set}`: {e}"))?;
    lock(&BUNDLES).insert(set.to_string(), b.icons);
    lock(&FETCHES).remove(set);
    Ok(())
}

fn bundled(set: &str, name: &str) -> Option<String> {
    lock(&BUNDLES).get(set)?.get(name).cloned()
}

/// The SVG markup of `name` in `set`, from disk or from a loaded bundle.
pub fn svg(root: &Path, set: &str, name: &str) -> Option<String> {
    crate::vfs::read_to_string(root.join(set).join(format!("{name}.svg")))
        .ok()
        .or_else(|| bundled(set, name))
}

/// Point the page at the server that carries the icon bundles and start the default set, so
/// the common icons are ready before an agent asks for one.
pub fn set_source(base: &str) {
    *lock(&SOURCE) = Some(base.trim_end_matches('/').to_string());
    want("lucide");
}

/// Start fetching `set` if it is not in memory and not on its way. Only the browser has a
/// source to fetch from; elsewhere sets come from the icons dir.
pub fn want(set: &str) {
    if spec(set).is_none() || lock(&BUNDLES).contains_key(set) {
        return;
    }
    let Some(base) = lock(&SOURCE).clone() else {
        return;
    };
    if matches!(lock(&FETCHES).get(set), Some(None)) {
        return;
    }
    lock(&FETCHES).insert(set.to_string(), None);
    fetch(base, set.to_string());
}

#[cfg(target_arch = "wasm32")]
fn fetch(base: String, set: String) {
    crate::sync::net::spawn(async move {
        let url = format!("{base}/icon-sets/{set}.json");
        let got = match crate::sync::net::request("GET", &url, None).await {
            Ok((200, body)) => add_bundle(&set, &body),
            Ok((code, _)) => Err(format!("{url}: HTTP {code}")),
            Err(e) => Err(format!("{url}: {e}")),
        };
        if let Err(why) = got {
            lock(&FETCHES).insert(set, Some(why));
        }
    });
}

// shortcut: only the browser downloads; Android can add its branch once its TLS client lands.
#[cfg(not(target_arch = "wasm32"))]
fn fetch(_base: String, set: String) {
    lock(&FETCHES).insert(
        set,
        Some("downloading icon sets is not supported on this platform".into()),
    );
}

/// Why `set` (or any set, when `None`) cannot answer yet: still downloading, or failed to.
fn loading(set: Option<&str>) -> Option<String> {
    let f = lock(&FETCHES);
    let (id, state) = match set {
        Some(s) => (s, f.get(s)?),
        None => f.iter().next().map(|(k, v)| (k.as_str(), v))?,
    };
    Some(match state {
        None => format!("icon set `{id}` is still loading; retry this in a moment"),
        Some(why) => format!("icon set `{id}` could not be loaded: {why}"),
    })
}

/// The closest icon names to a reference that does not exist, as `set:name`.
pub fn closest(reference: &str) -> Vec<String> {
    let (set, name) = match reference.split_once(':') {
        Some((s, n)) => (Some(s), n),
        None => (None, reference),
    };
    let root = dir();
    let hits = search(&root, name, set, 5);
    if !hits.is_empty() {
        return hits;
    }
    // "check-mark" finds nothing as a whole; its words may.
    name.split(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .filter(|w| w.len() > 2)
        .map(|w| search(&root, w, set, 5))
        .find(|h| !h.is_empty())
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
const NO_SETS: &str = "no icon sets could be loaded from the server";
#[cfg(not(target_arch = "wasm32"))]
const NO_SETS: &str = "no icon sets installed; run `papercraft-cli icons add lucide` (see `papercraft-cli icons list`)";

/// A downloadable icon set: the npm package, the folder inside it, and its licence.
pub struct SetSpec {
    pub id: &'static str,
    pub package: &'static str,
    pub dir: &'static str,
    pub license: &'static str,
    pub about: &'static str,
}

pub const SETS: &[SetSpec] = &[
    SetSpec {
        id: "lucide",
        package: "lucide-static",
        dir: "icons",
        license: "ISC",
        about: "Lucide: ~1500 clean 24px stroke icons",
    },
    SetSpec {
        id: "tabler",
        package: "@tabler/icons",
        dir: "icons/outline",
        license: "MIT",
        about: "Tabler: ~5000 24px stroke icons",
    },
    SetSpec {
        id: "phosphor",
        package: "@phosphor-icons/core",
        dir: "assets/regular",
        license: "MIT",
        about: "Phosphor (regular weight): ~1500 filled-outline icons",
    },
    SetSpec {
        id: "heroicons",
        package: "heroicons",
        dir: "24/outline",
        license: "MIT",
        about: "Heroicons 24px outline: ~300 icons by the Tailwind team",
    },
    SetSpec {
        id: "material",
        package: "@material-symbols/svg-400",
        dir: "outlined",
        license: "Apache-2.0",
        about: "Material Symbols (outlined, weight 400): ~3000 icons",
    },
];

pub fn spec(id: &str) -> Option<&'static SetSpec> {
    SETS.iter().find(|s| s.id == id)
}

/// Where the sets live.
pub fn dir() -> PathBuf {
    std::env::var_os("PAPERCRAFT_ICONS")
        .map(PathBuf::from)
        .or_else(|| {
            let data = dirs::data_dir()?;
            let new = data.join("papercraft").join("icons");
            let legacy = data.join("paper-rs").join("icons");
            Some(
                if crate::vfs::is_dir(&legacy) && !crate::vfs::exists(&new) {
                    legacy
                } else {
                    new
                },
            )
        })
        .unwrap_or_else(|| PathBuf::from("icons"))
}

/// Installed sets with their icon counts.
pub fn installed(root: &Path) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = crate::vfs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| crate::vfs::is_dir(&e.path()))
        .map(|e| {
            let n = names(root, &e.file_name().to_string_lossy()).len();
            (e.file_name().to_string_lossy().into_owned(), n)
        })
        .collect();
    for (s, icons) in lock(&BUNDLES).iter() {
        if !out.iter().any(|(o, _)| o == s) {
            out.push((s.clone(), icons.len()));
        }
    }
    out.sort();
    out
}

/// Icon names (without `.svg`) in a set, sorted.
pub fn names(root: &Path, set: &str) -> Vec<String> {
    let mut v: Vec<String> = crate::vfs::read_dir(root.join(set))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "svg").then(|| p.file_stem()?.to_str().map(String::from))?
        })
        .collect();
    if let Some(b) = lock(&BUNDLES).get(set) {
        v.extend(b.keys().cloned());
    }
    v.sort();
    v.dedup();
    v
}

/// `set:name` for every icon whose name contains all words of `query` (all installed sets, or
/// just `set`). Exact names first, then prefix matches, then the rest, shortest first.
pub fn search(root: &Path, query: &str, set: Option<&str>, limit: usize) -> Vec<String> {
    let words: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let sets: Vec<String> = match set {
        Some(s) => vec![s.to_string()],
        None => installed(root).into_iter().map(|(s, _)| s).collect(),
    };
    let mut hits: Vec<(u8, usize, String)> = Vec::new();
    for s in sets {
        for n in names(root, &s) {
            let lower = n.to_lowercase();
            if !words.iter().all(|w| lower.contains(w.as_str())) {
                continue;
            }
            let q = words.join("-");
            let rank = if lower == q {
                0
            } else if lower.starts_with(&q) {
                1
            } else {
                2
            };
            hits.push((rank, n.len(), format!("{s}:{n}")));
        }
    }
    hits.sort();
    hits.into_iter().take(limit).map(|h| h.2).collect()
}

/// The SVG markup of `set:name` (`name` alone looks in every installed set, first match).
pub fn load(root: &Path, reference: &str) -> Result<String, String> {
    let (set, name) = match reference.split_once(':') {
        Some((s, n)) => (Some(s), n),
        None => (None, reference),
    };
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return Err(format!("`{reference}` is not an icon name"));
    }
    let sets: Vec<String> = match set {
        Some(s) if !s.contains(['/', '\\']) && !s.starts_with('.') => vec![s.to_string()],
        Some(s) => return Err(format!("`{s}` is not an icon set")),
        None => installed(root).into_iter().map(|(s, _)| s).collect(),
    };
    for s in &sets {
        if let Some(svg) = svg(root, s, name) {
            return Ok(svg);
        }
    }
    // An icon of a set that has not arrived yet is not a missing icon.
    if let Some(s) = set {
        want(s);
    }
    if let Some(msg) = loading(set) {
        return Err(msg);
    }
    if installed(root).is_empty() {
        return Err(NO_SETS.into());
    }
    let hint = search(root, name, set, 5);
    Err(format!(
        "icon `{reference}` not found{}",
        if hint.is_empty() {
            String::new()
        } else {
            format!("; similar: {}", hint.join(", "))
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_bundle_answers_load_names_and_search_like_a_folder() {
        let root = std::env::temp_dir().join("papercraft-no-such-icons");
        let json = br#"{"license":"MIT","icons":{"bolt":"<svg id=\"b\"/>","bolt-slash":"<svg/>"}}"#;
        assert!(add_bundle("nope", json).is_err());
        assert!(add_bundle("tabler", b"{").is_err());
        add_bundle("tabler", json).unwrap();
        assert_eq!(load(&root, "tabler:bolt").unwrap(), "<svg id=\"b\"/>");
        assert_eq!(names(&root, "tabler"), ["bolt", "bolt-slash"]);
        assert!(installed(&root).contains(&("tabler".into(), 2)));
        assert_eq!(
            search(&root, "bolt", Some("tabler"), 5),
            ["tabler:bolt", "tabler:bolt-slash"]
        );
        assert!(load(&root, "tabler:zap").unwrap_err().contains("not found"));
        lock(&BUNDLES).remove("tabler");
    }

    fn a_set_on_its_way_is_not_reported_as_a_missing_icon() {
        let root = std::env::temp_dir().join("papercraft-no-such-icons");
        lock(&FETCHES).insert("heroicons".into(), None);
        assert!(
            load(&root, "heroicons:home")
                .unwrap_err()
                .contains("still loading")
        );
        lock(&FETCHES).insert("heroicons".into(), Some("HTTP 404".into()));
        assert!(
            load(&root, "heroicons:home")
                .unwrap_err()
                .contains("could not be loaded")
        );
        lock(&FETCHES).remove("heroicons");
    }

    #[test]
    fn rejects_path_tricks_and_ranks_exact_names_first() {
        // One test: the bundle and fetch state is process-wide.
        a_bundle_answers_load_names_and_search_like_a_folder();
        a_set_on_its_way_is_not_reported_as_a_missing_icon();
        let root =
            std::env::temp_dir().join(format!("papercraft-icons-{:016x}", fastrand::u64(..)));
        let set = root.join("t");
        crate::vfs::create_dir_all(&set).unwrap();
        for n in ["check", "check-circle", "arrow-check", "x"] {
            crate::vfs::write(set.join(format!("{n}.svg")), "<svg/>").unwrap();
        }
        assert!(load(&root, "../etc/passwd").is_err());
        assert!(load(&root, "t:../x").is_err());
        assert!(load(&root, "..:x").is_err());
        assert_eq!(
            search(&root, "check", None, 10),
            ["t:check", "t:check-circle", "t:arrow-check"]
        );
        assert_eq!(load(&root, "t:x").unwrap(), "<svg/>");
        assert!(load(&root, "t:nope").unwrap_err().contains("not found"));
        assert!(
            load(&root.join("missing"), "x")
                .unwrap_err()
                .contains("no icon sets")
        );
        crate::vfs::remove_dir_all(root).ok();
    }
}
