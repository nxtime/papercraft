//! OpenDesign design-system packages: a folder with `manifest.json`, `tokens.css` and
//! `DESIGN.md`.
//!
//! The wire format is reimplemented from the published description of the OpenDesign
//! design-system project format (github.com/nexu-io/open-design, Apache-2.0). No file of that
//! project is copied; only the format is followed. See `docs/design-system-packages.md`.
//!
//! A package maps onto the PaperCraft design system like this:
//!
//! | Package | PaperCraft |
//! |---|---|
//! | `:root { --accent: #cba6f7 }` | a variable `accent` |
//! | `:root { --spacing-md: 16px }` | a token `spacing-md` |
//! | `[data-theme="Dark"] { --accent: #000 }` | the mode `Dark` |
//! | a value that is not a hex color | a token (PaperCraft colors are hex only) |

use crate::model::{Color, Document};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// The `schemaVersion` of the packages we read and write.
pub const SCHEMA_VERSION: &str = "od-design-system-project/v1";
/// The file names of a package.
pub const MANIFEST_FILE: &str = "manifest.json";
pub const TOKENS_FILE: &str = "tokens.css";
pub const DESIGN_FILE: &str = "DESIGN.md";
/// The `importMode` values the format allows.
const IMPORT_MODES: [&str; 3] = ["normalized", "hybrid", "verbatim"];
/// The `source.type` values the format allows.
const SOURCE_TYPES: [&str; 4] = ["bundled", "local", "github", "shadcn"];

// ------------------------------------------------------------------------------ manifest

/// The `manifest.json` of a design-system package.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: Source,
    pub files: Files,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components_manifest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub craft: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonts: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_files: Option<Value>,
}

/// Where a package came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    #[serde(rename = "type")]
    pub kind: String,
    /// Everything else the manifest says about the source.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Source {
    /// A package that lives on this machine.
    pub fn local() -> Source {
        Source {
            kind: "local".into(),
            extra: Map::new(),
        }
    }
}

/// The files of a package, relative to its folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Files {
    pub design: String,
    pub tokens: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_tokens: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailwind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<String>,
}

/// What an exported manifest says about the package.
#[derive(Debug, Clone)]
pub struct Meta {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: Option<String>,
    pub source: Source,
}

impl Meta {
    /// The meta block of a page document exported into `dir`: the id is the folder slug.
    pub fn for_page(dir: &Path, doc: &Document) -> Meta {
        let folder = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Meta {
            id: slug(&folder),
            name: doc.name.clone(),
            category: "design-system".into(),
            description: None,
            source: Source::local(),
        }
    }

    /// The manifest this meta describes.
    pub fn manifest(&self) -> Manifest {
        Manifest {
            schema_version: SCHEMA_VERSION.into(),
            id: self.id.clone(),
            name: self.name.clone(),
            category: self.category.clone(),
            description: self.description.clone(),
            source: self.source.clone(),
            files: Files {
                design: DESIGN_FILE.into(),
                tokens: TOKENS_FILE.into(),
                design_tokens: None,
                tailwind: None,
                components: None,
            },
            assets_dir: None,
            preview_dir: None,
            usage: None,
            components_manifest: None,
            import_mode: None,
            craft: None,
            fonts: None,
            preview: None,
            source_files: None,
        }
    }
}

/// Read a `manifest.json`. Malformed input is an error, never a panic.
pub fn parse_manifest(raw: &str) -> Result<Manifest, String> {
    let m: Manifest =
        serde_json::from_str(raw).map_err(|e| format!("malformed {MANIFEST_FILE}: {e}"))?;
    if m.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported schemaVersion `{}` (expected `{SCHEMA_VERSION}`)",
            m.schema_version
        ));
    }
    if !valid_id(&m.id) {
        return Err(format!(
            "manifest id `{}` must be a slug: lowercase letters, digits and dashes",
            m.id
        ));
    }
    if !SOURCE_TYPES.contains(&m.source.kind.as_str()) {
        return Err(format!(
            "unknown source type `{}` (bundled, local, github or shadcn)",
            m.source.kind
        ));
    }
    if let Some(mode) = &m.import_mode
        && !IMPORT_MODES.contains(&mode.as_str())
    {
        return Err(format!(
            "unknown importMode `{mode}` (normalized, hybrid or verbatim)"
        ));
    }
    Ok(m)
}

/// An id the format allows: a slug of lowercase letters, digits and dashes.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

// ------------------------------------------------------------------------------ tokens

/// The values of a `tokens.css` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tokens {
    /// The colors of the `:root` block: the variables of the design system.
    pub variables: BTreeMap<String, Color>,
    /// The other values of the `:root` block: spacing, radius, shadows, ...
    pub tokens: BTreeMap<String, String>,
    /// The colors of the `[data-theme="..."]` blocks: one map per mode.
    pub modes: BTreeMap<String, BTreeMap<String, Color>>,
}

/// What a `tokens.css` block is about.
enum Selector {
    Root,
    Mode(String),
}

/// Read a `tokens.css` file: the `:root` block gives the variables and the tokens, every
/// `[data-theme="name"]` block gives the values of that mode.
///
/// Only hex colors become variables; every other value (a size, a shadow, an `oklch()` color)
/// is a token, because PaperCraft colors are hex only. A non-color value inside a mode block
/// cannot be stored per mode, so it becomes a plain token.
pub fn parse_tokens_css(src: &str) -> Tokens {
    let mut out = Tokens::default();
    let blocks = blocks(src);
    // The root block first: it decides which names are colors.
    for body in blocks
        .iter()
        .filter(|(s, _)| matches!(selector_kind(s), Some(Selector::Root)))
        .map(|(_, body)| body)
    {
        for (name, value) in declarations(body) {
            match hex_color(&value) {
                Some(c) => {
                    out.variables.insert(name, c);
                }
                None => {
                    out.tokens.insert(name, value);
                }
            }
        }
    }
    for (mode, body) in blocks.iter().filter_map(|(s, b)| match selector_kind(s) {
        Some(Selector::Mode(m)) => Some((m, b)),
        _ => None,
    }) {
        let mut colors = BTreeMap::new();
        let mut others = Vec::new();
        for (name, value) in declarations(body) {
            match hex_color(&value) {
                Some(c) => {
                    colors.insert(name, c);
                }
                None => others.push((name, value)),
            }
        }
        out.modes.insert(sanitize_name(&mode), colors);
        for (name, value) in others {
            if !out.variables.contains_key(&name) {
                out.tokens.entry(name).or_insert(value);
            }
        }
    }
    // A name is either a color or a token, never both.
    out.tokens
        .retain(|name, _| !out.variables.contains_key(name));
    out
}

/// The page's variables, modes and tokens as a `tokens.css` file.
pub fn tokens_css(doc: &Document) -> String {
    let mut out = String::from(":root {\n");
    for (name, color) in &doc.variables {
        out.push_str(&format!(
            "  --{}: {};\n",
            sanitize_name(name),
            color.to_hex()
        ));
    }
    for (name, value) in &doc.tokens {
        out.push_str(&format!(
            "  --{}: {};\n",
            sanitize_name(name),
            clean_value(value)
        ));
    }
    out.push_str("}\n");
    for (mode, vars) in &doc.modes {
        out.push_str(&format!("[data-theme=\"{}\"] {{\n", mode_name(mode)));
        for (name, color) in vars {
            out.push_str(&format!(
                "  --{}: {};\n",
                sanitize_name(name),
                color.to_hex()
            ));
        }
        out.push_str("}\n");
    }
    out
}

/// A CSS-safe custom-property name: letters, digits, `-` and `_`.
pub fn sanitize_name(name: &str) -> String {
    let name = name.trim().trim_start_matches("--");
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    out
}

/// A folder slug: lowercase letters, digits and dashes.
pub fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// The color of a `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` value.
fn hex_color(value: &str) -> Option<Color> {
    let h = value.trim().strip_prefix('#')?;
    let h = match h.len() {
        3 | 4 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 | 8 => h.to_string(),
        _ => return None,
    };
    Color::from_hex(&format!("#{h}"))
}

/// A token value that fits on one line of CSS.
fn clean_value(value: &str) -> String {
    value
        .chars()
        .map(|c| if c == ';' || c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The blocks of a CSS file as (selector, body) pairs.
fn blocks(src: &str) -> Vec<(String, String)> {
    let src = strip_comments(src);
    let mut out = Vec::new();
    collect_blocks(&src, &mut out);
    out
}

fn collect_blocks(src: &str, out: &mut Vec<(String, String)>) {
    let bytes = src.as_bytes();
    let mut selector = String::new();
    let mut i = 0;
    while i < src.len() {
        match bytes[i] {
            b'{' => {
                let Some(close) = matching_brace(src, i) else {
                    break;
                };
                let body = &src[i + 1..close];
                out.push((selector.trim().to_string(), body.to_string()));
                collect_blocks(body, out);
                i = close + 1;
                selector.clear();
            }
            b'}' => {
                selector.clear();
                i += 1;
            }
            _ => {
                let c = src[i..].chars().next().unwrap_or(' ');
                selector.push(c);
                i += c.len_utf8();
            }
        }
    }
}

/// The index of the `}` that closes the `{` at `open`.
fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in src.as_bytes()[open..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The source without its `/* ... */` comments.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The `--name: value` declarations of a CSS block.
fn declarations(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in body.split(';') {
        let Some((name, value)) = part.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if !name.starts_with("--") {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let name = sanitize_name(name);
        if name.is_empty() {
            continue;
        }
        out.push((name, value.to_string()));
    }
    out
}

/// What a selector is about: the root block or a mode.
fn selector_kind(selector: &str) -> Option<Selector> {
    for part in selector.split(',') {
        let part = part.trim();
        if part == ":root" {
            return Some(Selector::Root);
        }
        if let Some(name) = theme_name(part) {
            return Some(Selector::Mode(name));
        }
    }
    None
}

/// The mode name of a `[data-theme="dark"]` selector.
fn theme_name(selector: &str) -> Option<String> {
    let rest = selector.trim().strip_prefix("[data-theme")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let body = &rest[1..];
        let end = body.find(quote)?;
        Some(body[..end].to_string())
    } else {
        let end = rest.find(']')?;
        Some(rest[..end].to_string())
    }
}

/// The name of a mode, safe to write in a selector.
fn mode_name(mode: &str) -> String {
    sanitize_name(mode)
}

// ------------------------------------------------------------------------------ packages

/// A design-system package read from disk.
#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: Manifest,
    /// The values of `tokens.css`.
    pub tokens: Tokens,
    /// The prose of `DESIGN.md`, when the package has one.
    pub design: Option<String>,
}

/// Read a design-system package from `dir`: `manifest.json`, `tokens.css` and, when the
/// manifest names one, `DESIGN.md`.
#[cfg(not(target_arch = "wasm32"))]
pub fn import_package(dir: &Path) -> Result<Package, String> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let manifest = parse_manifest(&raw)?;
    // The id of a package is the slug of its folder.
    let folder = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if !folder.is_empty() && slug(&folder) != manifest.id {
        return Err(format!(
            "manifest id `{}` does not match the folder `{}` (expected `{}`)",
            manifest.id,
            folder,
            slug(&folder)
        ));
    }
    let tokens_name = file_name(&manifest.files.tokens)?;
    let tokens_path = dir.join(&tokens_name);
    let css = std::fs::read_to_string(&tokens_path)
        .map_err(|e| format!("{}: {e}", tokens_path.display()))?;
    let design_name = file_name(&manifest.files.design)?;
    let design_path = dir.join(&design_name);
    let design = std::fs::read_to_string(&design_path).ok();
    Ok(Package {
        manifest,
        tokens: parse_tokens_css(&css),
        design,
    })
}

/// Write a design-system package to `dir`: `manifest.json`, `tokens.css` and `DESIGN.md`.
/// Returns the files written.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_package(dir: &Path, doc: &Document) -> Result<Vec<std::path::PathBuf>, String> {
    export_package_with(dir, doc, &Meta::for_page(dir, doc))
}

/// Write a design-system package to `dir` with the given meta data. Returns the files written.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_package_with(
    dir: &Path,
    doc: &Document,
    meta: &Meta,
) -> Result<Vec<std::path::PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let manifest = serde_json::to_string_pretty(&meta.manifest())
        .map_err(|e| format!("{MANIFEST_FILE}: {e}"))?;
    let mut written = Vec::new();
    for (name, body) in [
        (MANIFEST_FILE, manifest),
        (TOKENS_FILE, tokens_css(doc)),
        (DESIGN_FILE, design_md(doc)),
    ] {
        let path = dir.join(name);
        crate::io::atomic::write(&path, body.as_bytes())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

/// A short prose description of a design system, for `DESIGN.md`.
#[cfg(not(target_arch = "wasm32"))]
fn design_md(doc: &Document) -> String {
    let mut out = String::from("# Design system\n\nExported from PaperCraft.\n");
    if !doc.variables.is_empty() {
        out.push_str("\n## Variables\n\n| Name | Value |\n|---|---|\n");
        for (name, color) in &doc.variables {
            out.push_str(&format!("| `{name}` | `{}` |\n", color.to_hex()));
        }
    }
    if !doc.modes.is_empty() {
        out.push_str("\n## Modes\n\n");
        for (mode, vars) in &doc.modes {
            out.push_str(&format!("* `{mode}`: {} variable(s)\n", vars.len()));
        }
    }
    if !doc.tokens.is_empty() {
        out.push_str("\n## Tokens\n\n| Name | Value |\n|---|---|\n");
        for (name, value) in &doc.tokens {
            out.push_str(&format!("| `{name}` | `{value}` |\n"));
        }
    }
    out
}

/// A file name of a package: inside the folder, never a path.
#[cfg(not(target_arch = "wasm32"))]
fn file_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("the manifest names an empty file".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!(
            "the manifest names `{name}`, which is outside the package folder"
        ));
    }
    Ok(name.to_string())
}
