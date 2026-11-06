//! `export_site`: frames to a website folder, written in one call. `tree` turns the frames into
//! a framework-neutral element tree and style model (semantic tags, merged classes, locale
//! messages); a writer per framework prints it, and `write` puts the files on disk safely. The
//! result is a short summary, so an agent never has to read the output back.

mod angular;
mod astro;
mod contrast;
mod fluid;
mod fontsub;
mod host;
mod html;
mod icon;
mod markup;
mod media;
mod model;
mod react;
mod semantic;
mod style;
mod tree;
mod vue;

use super::exec::{self, Host};
use super::{Out, ToolResult};
use crate::editor::Editor;
use crate::io::backup::safe_relative;
use crate::model::{Document, NodeId};
use model::{Frame, Framework, Locale, Options, Site, Styling, is_lang_tag};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use style::unique;

const MANIFEST: &str = ".papercraft-site.json";
const GENERATOR: &str = "papercraft export_site";
/// Bumped when the layout of the folder changes.
const FORMAT: u32 = 1;
/// Longest file name stem made from a frame name.
const STEM_CHARS: usize = 60;
/// Folder names a language or mode cannot take: they hold the site's own files.
const RESERVED_FOLDERS: [&str; 2] = ["assets", "src"];

pub fn export(host: &mut dyn Host, a: &Value) -> ToolResult {
    let options = Options::parse(a)?;
    let path = a
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty());
    let dir = Path::new(path.ok_or("`path` is required: the folder to write the site into")?)
        .to_path_buf();
    exec::out_folder(&dir, MANIFEST, is_site_manifest)?;
    let base_url = match a
        .get("base_url")
        .and_then(Value::as_str)
        .map(|b| b.trim().trim_end_matches('/'))
        .filter(|b| !b.is_empty())
    {
        Some(b) if b.starts_with("http://") || b.starts_with("https://") => Some(b.to_owned()),
        Some(b) => {
            return Err(format!(
                "`base_url` must start with http:// or https:// (got `{b}`)"
            ));
        }
        None => None,
    };
    let text = |k: &str| {
        a.get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let lang = match text("lang") {
        Some(l) if is_lang_tag(l) => Some(l.to_owned()),
        Some(l) => {
            return Err(format!(
                "`lang` must be a language tag such as `en` or `pt-BR` (got `{l}`)"
            ));
        }
        None => None,
    };
    let project = host.project().map(|p| p.manifest.name.clone());

    let ed = host.editor();
    let page = match text("pageId") {
        Some(p) => exec::find_page(ed, p)?,
        None => ed.active_page,
    };
    let ids = pick_frames(ed, page, a)?;
    let (locales, docs) = variants(ed, page, a, lang)?;
    let frames = name_frames(&docs[0], ids);
    let titles = localized(a, "title", &locales)?;
    let descriptions = localized(a, "description", &locales)?;

    let built = tree::build(tree::Params {
        docs: &docs,
        frames: &frames,
        locales: &locales,
        tailwind: options.styling == Styling::Tailwind,
        title: &titles,
        description: &descriptions,
    });
    let site = Site {
        options,
        site_title: titles[0]
            .clone()
            .or(project)
            .unwrap_or_else(|| "Site".into()),
        base_url,
        locales,
        pages: built.pages,
        messages: built.messages,
        assets: built.media.files,
        style: built.style,
        warnings: built.warnings,
    };
    let mut output = match options.framework {
        Framework::Html => html::write(&site)?,
        Framework::React => react::write(&site)?,
        Framework::Vue => vue::write(&site)?,
        Framework::Angular => angular::write(&site)?,
        Framework::Astro => astro::write(&site)?,
    };
    host::add(&options, &mut output.files);
    let mut warnings = site.warnings.clone();
    warnings.append(&mut output.warnings);

    let pages = match options.framework {
        Framework::Html => output.files.keys().filter(|k| k.ends_with(".html")).count(),
        _ => site.pages.len() * site.locales.len(),
    };
    let bytes = output.files.values().map(Vec::len).sum::<usize>();
    let total = output.files.len();
    let (written, removed, kept) = write(&dir, output.files)?;
    warnings.extend(kept.iter().map(|k| format!("`{k}` was edited after the last export, so it was kept; delete it by hand if it is unwanted")));
    let mut out = json!({
        "dir": dir, "framework": chosen(a, "framework", "html"), "render": chosen(a, "render", "static"), "styling": chosen(a, "styling", "css"),
        "pages": pages, "files": total, "bytes": bytes, "written": written, "removed": removed,
        "links": {"resolved": built.resolved, "unresolved": built.unresolved},
        "urls": if site.base_url.is_some() { "absolute (sitemap.xml written)" } else { "relative (pass base_url for absolute hreflang and canonical URLs and a sitemap)" },
        "next_steps": output.next_steps,
        "warnings": warnings,
    });
    if site.locales.iter().any(|l| l.folder.is_some()) {
        out["modes"] = json!(
            site.locales
                .iter()
                .map(|l| l.folder.clone().unwrap_or_default())
                .collect::<Vec<_>>()
        );
    }
    Ok(vec![Out::Text(
        serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?,
    )])
}

/// A text option per locale: a string for all of them, or an object keyed by mode name. A
/// language without an entry takes the base language's, else the first entry in key order
/// (the arguments arrive sorted, so "first given" is not recoverable).
fn localized(a: &Value, key: &str, locales: &[Locale]) -> Result<Vec<Option<String>>, String> {
    let clean = |s: &str| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
    let wrong = || {
        format!(
            "`{key}` must be a string, or an object of strings keyed by mode name such as {{\"en\": \"...\", \"pt-BR\": \"...\"}}"
        )
    };
    match a.get(key) {
        None | Some(Value::Null) => Ok(vec![None; locales.len()]),
        Some(Value::String(s)) => Ok(vec![clean(s); locales.len()]),
        Some(Value::Object(map)) => {
            let mut entries = BTreeMap::new();
            for (k, v) in map {
                let v = v.as_str().ok_or_else(wrong)?;
                entries.insert(k.as_str(), clean(v));
            }
            // A misspelt language would silently get the base language's text.
            if locales.len() > 1
                && let Some(k) = map.keys().find(|k| !locales.iter().any(|l| l.label == **k))
            {
                let names: Vec<&str> = locales.iter().map(|l| l.label.as_str()).collect();
                return Err(format!(
                    "`{key}` names the language `{k}`, which this export does not have; its languages are {}",
                    names.join(", ")
                ));
            }
            let first = locales
                .first()
                .and_then(|l| entries.get(l.label.as_str()))
                .cloned()
                .flatten()
                .or_else(|| entries.values().flatten().next().cloned());
            Ok(locales
                .iter()
                .map(|l| {
                    entries
                        .get(l.label.as_str())
                        .cloned()
                        .flatten()
                        .or_else(|| first.clone())
                })
                .collect())
        }
        Some(_) => Err(wrong()),
    }
}

/// An option as the caller wrote it (already validated by `Options::parse`), or its default.
fn chosen(a: &Value, key: &str, default: &str) -> String {
    a.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .to_owned()
}

/// The frames to export: the ones named, else the page's declared screens, else its top-level
/// frames (components excluded).
fn pick_frames(ed: &Editor, page: usize, a: &Value) -> Result<Vec<NodeId>, String> {
    let doc = ed.page_doc(page);
    let mut ids = Vec::new();
    if let Some(keys) = a.get("frames").and_then(Value::as_array) {
        for k in keys {
            let k = k.as_str().ok_or("`frames` must be a list of frame ids")?;
            let (p, id) = exec::resolve(ed, k, page)?;
            if p != page || !doc.get(id).is_some_and(|n| n.is_frame()) {
                return Err(format!("`{k}` must be a frame on the exported page"));
            }
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    } else {
        ids = crate::editor::screens::declared(doc);
        if ids.is_empty() {
            ids = doc
                .root
                .iter()
                .copied()
                .filter(|id| doc.get(*id).is_some_and(|n| n.is_frame() && !n.component))
                .collect();
        }
    }
    if ids.is_empty() {
        return Err("no frames to export: the page has no screens or top-level frames".into());
    }
    Ok(ids)
}

/// File and component names from frame names: the first page is `index`, the rest slugs, made
/// unique.
fn name_frames(doc: &Document, ids: Vec<NodeId>) -> Vec<Frame> {
    let mut taken = BTreeSet::from(["index".to_owned()]);
    let mut components = BTreeSet::new();
    ids.into_iter()
        .enumerate()
        .map(|(i, id)| {
            let name = doc.get(id).map(|n| n.name.clone()).unwrap_or_default();
            let stem = if i == 0 {
                "index".to_owned()
            } else {
                unique(&mut taken, slug(&name))
            };
            let component = component_name(&name, &mut components);
            Frame {
                id,
                name,
                stem,
                component,
            }
        })
        .collect()
}

/// PascalCase ASCII ending in `Page`: a valid identifier in TypeScript, Vue, Angular and Astro,
/// never a keyword, and unique even on a case-insensitive disk.
fn component_name(name: &str, taken: &mut BTreeSet<String>) -> String {
    let words: String = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w[..1].to_ascii_uppercase() + &w[1..])
        .collect();
    let words = match words.chars().next() {
        None => "Screen".to_owned(),
        Some(c) if c.is_ascii_digit() => format!("Screen{words}"),
        _ => words,
    };
    let base = format!("{words}Page");
    let (mut candidate, mut n) = (base.clone(), 2);
    while !taken.insert(candidate.to_lowercase()) {
        candidate = format!("{base}{n}");
        n += 1;
    }
    candidate
}

/// The locales and, for each, the document as it is or in one mode, rendered without touching
/// the editor (no switch to undo, nothing on the history). Without `modes` there is one locale.
fn variants(
    ed: &mut Editor,
    page: usize,
    a: &Value,
    lang: Option<String>,
) -> Result<(Vec<Locale>, Vec<Document>), String> {
    let Some(spec) = a.get("modes").filter(|m| !m.is_null()) else {
        let locale = Locale {
            tag: lang,
            hreflang: false,
            folder: None,
            label: String::new(),
            scope: String::new(),
            class: None,
        };
        return Ok((vec![locale], vec![ed.page_doc(page).clone()]));
    };
    let coll = spec["collection"]
        .as_str()
        .ok_or("`modes` needs a `collection` (name or id) and a `modes` list")?;
    let names: Vec<&str> = spec["modes"]
        .as_array()
        .map(|m| m.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if names.is_empty() {
        return Err("`modes.modes` must list at least one mode name".into());
    }
    let c = ed
        .design
        .collections
        .iter()
        .find(|c| c.name == coll || c.id == coll)
        .ok_or_else(|| format!("no variable collection `{coll}`"))?
        .clone();
    let mut taken: BTreeSet<String> = RESERVED_FOLDERS.iter().map(|s| (*s).to_owned()).collect();
    let (mut locales, mut docs) = (Vec::new(), Vec::new());
    for name in names {
        let m = c
            .modes
            .iter()
            .find(|m| m.name == name || m.id == name)
            .ok_or_else(|| format!("collection `{coll}` has no mode `{name}`"))?;
        let is_lang = is_lang_tag(&m.name);
        let folder = unique(
            &mut taken,
            if is_lang {
                m.name.clone()
            } else {
                slug(&m.name)
            },
        );
        // A language is told apart by `:lang()`; any other mode by a class on `<html>`.
        let (scope, class) = match (locales.is_empty(), is_lang) {
            (true, _) => (String::new(), None),
            (false, true) => (format!(":root:lang({})", m.name), None),
            (false, false) => (
                format!(":root.mode-{folder}"),
                Some(format!("mode-{folder}")),
            ),
        };
        let tag = if is_lang {
            Some(m.name.clone())
        } else {
            lang.clone()
        };
        locales.push(Locale {
            tag,
            hreflang: is_lang,
            folder: Some(folder),
            label: m.name.clone(),
            scope,
            class,
        });
        docs.push(ed.page_doc_in(page, &c.id, &m.id));
    }
    Ok((locales, docs))
}

/// A URL-safe file name stem: lower case letters and digits, runs of anything else as one `-`.
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_matches('-').chars().take(STEM_CHARS).collect();
    if out.is_empty() {
        "page".into()
    } else if is_device_name(&out) {
        // Windows opens `con.html` as the console, not a file.
        format!("{out}-page")
    } else {
        out
    }
}

fn is_device_name(stem: &str) -> bool {
    let s = stem.to_ascii_lowercase();
    matches!(s.as_str(), "con" | "prn" | "aux" | "nul")
        || (s.len() == 4
            && (s.starts_with("com") || s.starts_with("lpt"))
            && matches!(s.as_bytes()[3], b'1'..=b'9'))
}

/// Whether `bytes` is a manifest this exporter wrote.
fn is_site_manifest(bytes: &[u8]) -> bool {
    let Ok(m) = serde_json::from_slice::<Value>(bytes) else {
        return false;
    };
    m["generator"] == GENERATOR
        && m["format"] == FORMAT
        && m["files"]
            .as_object()
            .is_some_and(|f| f.values().all(|e| e["hash"].is_string()))
}

/// Writes the files whose bytes differ, then the manifest last (so a failed export never lists a
/// file that is missing), and removes files the previous manifest listed that this export no
/// longer has, unless their content changed since (the person edited them). Returns (written,
/// removed, kept).
fn write(
    dir: &Path,
    files: BTreeMap<String, Vec<u8>>,
) -> Result<(usize, usize, Vec<String>), String> {
    let old = std::fs::read(dir.join(MANIFEST))
        .ok()
        .filter(|b| is_site_manifest(b))
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(Value::Null);
    let entries: BTreeMap<&String, Value> = files
        .iter()
        .map(|(k, v)| (k, json!({"hash": crate::assets::hash(v)})))
        .collect();
    let manifest = serde_json::to_vec_pretty(
        &json!({"generator": GENERATOR, "format": FORMAT, "files": entries}),
    )
    .map_err(|e| e.to_string())?;
    let mut written = 0;
    let manifest_entry = (MANIFEST.to_owned(), manifest);
    for (rel, bytes) in files
        .iter()
        .chain(std::iter::once((&manifest_entry.0, &manifest_entry.1)))
    {
        let path =
            dir.join(safe_relative(rel).ok_or_else(|| format!("`{rel}` would leave the folder"))?);
        if std::fs::read(&path).is_ok_and(|b| &b == bytes) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        crate::io::atomic::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        written += 1;
    }
    let (mut removed, mut kept) = (0, Vec::new());
    for (rel, entry) in old["files"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| !files.contains_key(*k))
    {
        // Read back from disk: only plain relative paths inside the folder are trusted.
        let Some(safe) = safe_relative(rel) else {
            continue;
        };
        let path = dir.join(safe);
        let Ok(current) = std::fs::read(&path) else {
            continue;
        };
        if entry["hash"].as_str() != Some(crate::assets::hash(&current).as_str()) {
            kept.push(rel.clone());
        } else if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok((written, removed, kept))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_device_names_get_a_suffix_in_any_case() {
        for bad in ["Con", "NUL", "aux", "Prn", "Com1", "lpt9"] {
            assert_eq!(slug(bad), format!("{}-page", bad.to_lowercase()));
        }
        assert_eq!(slug("Com10"), "com10");
        assert_eq!(slug("Contact"), "contact");
    }

    #[test]
    fn unique_compares_case_insensitively() {
        let mut taken = BTreeSet::new();
        assert_eq!(unique(&mut taken, "pt-BR".into()), "pt-BR");
        assert_eq!(unique(&mut taken, "pt-br".into()), "pt-br-2");
    }

    #[test]
    fn component_names_are_unique_ascii_identifiers() {
        let mut taken = BTreeSet::new();
        let names: Vec<String> = [
            "Home screen",
            "home-screen",
            "3 Cards",
            "\u{1F600}",
            "caf\u{e9} menu",
        ]
        .iter()
        .map(|n| component_name(n, &mut taken))
        .collect();
        assert_eq!(
            names,
            [
                "HomeScreenPage",
                "HomeScreenPage2",
                "Screen3CardsPage",
                "ScreenPage",
                "CafMenuPage"
            ]
        );
        assert!(
            names
                .iter()
                .all(|n| n.chars().all(|c| c.is_ascii_alphanumeric())
                    && !n.starts_with(|c: char| c.is_ascii_digit()))
        );
    }

    fn scratch() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pc-site-unit-{:016x}", fastrand::u64(..)));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_failed_write_leaves_no_manifest_listing_missing_files() {
        let dir = scratch();
        let files = BTreeMap::from([
            ("a.html".to_owned(), b"a".to_vec()),
            ("zz/../bad.html".to_owned(), b"b".to_vec()),
        ]);
        assert!(write(&dir, files).is_err());
        assert!(!dir.join(MANIFEST).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stale_page_edited_by_hand_survives_a_reexport() {
        let dir = scratch();
        write(
            &dir,
            BTreeMap::from([
                ("a.html".to_owned(), b"a".to_vec()),
                ("b.html".to_owned(), b"b".to_vec()),
                ("c.html".to_owned(), b"c".to_vec()),
            ]),
        )
        .unwrap();
        std::fs::write(dir.join("b.html"), "mine").unwrap();
        let (_, removed, kept) =
            write(&dir, BTreeMap::from([("a.html".to_owned(), b"a".to_vec())])).unwrap();
        assert_eq!((removed, kept), (1, vec!["b.html".to_owned()]));
        assert_eq!(std::fs::read_to_string(dir.join("b.html")).unwrap(), "mine");
        assert!(!dir.join("c.html").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
