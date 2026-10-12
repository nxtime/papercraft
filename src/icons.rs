//! Open icon libraries on disk: `<icons dir>/<set>/<name>.svg`.
//!
//! The app never bundles or downloads icons: `paper-rs-cli icons add <set>` fetches a set from
//! the npm registry into the icons dir (`$PAPER_RS_ICONS`, else `<data dir>/paper-rs/icons`), and
//! `write_html` places them with `<x-paper-icon name="lucide:check">`. Each set keeps the
//! licence file of the package it came from next to the icons.

use std::path::{Path, PathBuf};

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
    std::env::var_os("PAPER_RS_ICONS")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("paper-rs").join("icons")))
        .unwrap_or_else(|| PathBuf::from("icons"))
}

/// Installed sets with their icon counts.
pub fn installed(root: &Path) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let n = names(root, &e.file_name().to_string_lossy()).len();
            (e.file_name().to_string_lossy().into_owned(), n)
        })
        .collect();
    out.sort();
    out
}

/// Icon names (without `.svg`) in a set, sorted.
pub fn names(root: &Path, set: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root.join(set))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "svg").then(|| p.file_stem()?.to_str().map(String::from))?
        })
        .collect();
    v.sort();
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
        if let Ok(svg) = std::fs::read_to_string(root.join(s).join(format!("{name}.svg"))) {
            return Ok(svg);
        }
    }
    if installed(root).is_empty() {
        return Err(
            "no icon sets installed; run `paper-rs-cli icons add lucide` (see `paper-rs-cli icons list`)"
                .into(),
        );
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

    #[test]
    fn rejects_path_tricks_and_ranks_exact_names_first() {
        let root = std::env::temp_dir().join(format!("paper-rs-icons-{:016x}", fastrand::u64(..)));
        let set = root.join("t");
        std::fs::create_dir_all(&set).unwrap();
        for n in ["check", "check-circle", "arrow-check", "x"] {
            std::fs::write(set.join(format!("{n}.svg")), "<svg/>").unwrap();
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
        std::fs::remove_dir_all(root).ok();
    }
}
