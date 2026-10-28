//! On-disk formats for projects and pages, with versioned migrations.
//!
//! Every file carries `format` and `version`. Loading parses into `serde_json::Value`, runs the
//! migration chain from the file's version up to the current one, then deserializes. Each step
//! is a pure function `Value -> Value`, so old files keep loading forever and every step is
//! unit-testable on its own.

use crate::geom::Camera;
use crate::variables::{PAGE_TEMPLATE_KEY, PageModes};
use crate::model::annotation::Annotation;
use crate::model::{Color, Document, Node, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

pub const PROJECT_FORMAT: &str = "paper-rs-project";
pub const PAGE_FORMAT: &str = "paper-rs-page";
/// The single-file format of papercraft 0.1 (`.paper-rs`), version 1. It migrates to a page.
pub const LEGACY_FORMAT: &str = "paper-rs";

/// v1: pages only. v2: `design.json` is the source of truth for variables and pages carry
/// bindings, component keys and sources. An older app would write pages without those and never
/// update `design.json`, so it must refuse a v2 project (`TooNew`) instead of losing data.
/// v3: pages and versions are zstd-compressed (`pages/<id>.json.zst`); an older app would see
/// no page files and save empty pages over them.
pub const PROJECT_VERSION: u32 = 3;
/// Project migrations, indexed by source version: `PROJECT_STEPS[0]` turns v1 into v2. The
/// manifest itself does not change; the design system is built on load from the pages, and
/// pages are packed as they are saved.
const PROJECT_STEPS: &[fn(Value) -> Value] = &[|v| v, |v| v];
/// v1: legacy single file. v2: page file inside a project. v3: fill layers (`fills`).
pub const PAGE_VERSION: u32 = 3;

#[derive(Debug)]
pub enum FormatError {
    Io(std::io::Error),
    Json(serde_json::Error),
    WrongFormat(String),
    TooNew { version: u32, supported: u32 },
    Invalid(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::Io(e) => write!(f, "I/O error: {e}"),
            FormatError::Json(e) => write!(f, "malformed file: {e}"),
            FormatError::WrongFormat(s) => write!(f, "not a papercraft file (format {s:?})"),
            FormatError::TooNew { version, supported } => write!(
                f,
                "made by a newer PaperCraft (file version {version}, this app reads up to {supported}); update PaperCraft to open it"
            ),
            FormatError::Invalid(s) => write!(f, "invalid document: {s}"),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<std::io::Error> for FormatError {
    fn from(e: std::io::Error) -> Self {
        FormatError::Io(e)
    }
}

// ---------------------------------------------------------------------------------- project

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    /// Unix milliseconds.
    pub created: u64,
    pub modified: u64,
    pub pages: Vec<PageRef>,
    #[serde(default)]
    pub active_page: Option<String>,
    /// Page groups (folders): group name -> page ids, in page order.
    /// Old manifests have no groups and load with none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub page_groups: BTreeMap<String, Vec<String>>,
}

impl Manifest {
    pub fn new(name: &str) -> Manifest {
        let now = now_ms();
        let page = PageRef {
            id: new_id(),
            name: "Page 1".into(),
        };
        Manifest {
            format: PROJECT_FORMAT.into(),
            version: PROJECT_VERSION,
            id: new_id(),
            name: name.into(),
            created: now,
            modified: now,
            active_page: Some(page.id.clone()),
            pages: vec![page],
            page_groups: BTreeMap::new(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("manifest serializes")
    }

    pub fn from_json(s: &str) -> Result<Manifest, FormatError> {
        let v: Value = serde_json::from_str(s).map_err(FormatError::Json)?;
        let v = migrate(v, PROJECT_FORMAT, PROJECT_VERSION, &[], PROJECT_STEPS)?;
        serde_json::from_value(v).map_err(FormatError::Json)
    }
}

// ---------------------------------------------------------------------------------- page

/// One page: an infinite canvas with its own nodes and camera.
#[derive(Debug, Clone, PartialEq)]
pub struct PageData {
    pub id: String,
    pub name: String,
    pub doc: Document,
    pub camera: Option<Camera>,
}

#[derive(Serialize, Deserialize)]
struct PageFile {
    format: String,
    version: u32,
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    camera: Option<Camera>,
    next_id: u64,
    root: Vec<NodeId>,
    nodes: Vec<Node>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    variables: BTreeMap<String, Color>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    modes: BTreeMap<String, BTreeMap<String, Color>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    tokens: BTreeMap<String, String>,
    /// Active mode id per collection id.
    #[serde(default, rename = "collectionModes", skip_serializing_if = "BTreeMap::is_empty")]
    modes_by_collection: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    annotations: Vec<Annotation>,
    /// This page's annotation template (overrides the project default).
    #[serde(default, rename = "annotationTemplate", skip_serializing_if = "Option::is_none")]
    annotation_template: Option<String>,
}

/// Page migrations, indexed by source version: `PAGE_STEPS[0]` turns v1 into v2.
const PAGE_STEPS: &[fn(Value) -> Value] = &[page_v1_to_v2, page_v2_to_v3];
/// Formats accepted as older versions of a page.
const PAGE_ALIASES: &[&str] = &[LEGACY_FORMAT];

/// v1 (`.paper-rs` single file) → v2 page: new format tag and a page id.
fn page_v1_to_v2(mut v: Value) -> Value {
    v["format"] = json!(PAGE_FORMAT);
    if v.get("id").is_none() {
        v["id"] = json!(new_id());
    }
    if v.get("name").is_none() {
        v["name"] = json!("Page 1");
    }
    v
}

/// v2 → v3: `style.fill: "#rrggbb"` becomes `style.fills: [{type: solid, color}]`.
fn page_v2_to_v3(mut v: Value) -> Value {
    if let Some(nodes) = v.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        for n in nodes {
            let Some(style) = n.get_mut("style").and_then(|s| s.as_object_mut()) else {
                continue;
            };
            if let Some(fill) = style.remove("fill")
                && let Some(c) = fill.as_str()
            {
                style.insert("fills".into(), json!([{ "type": "solid", "color": c }]));
            }
        }
    }
    v
}

pub fn page_to_json(p: &PageData) -> String {
    page_to_json_modes(p, &PageModes::new())
}

/// A page file with its active mode per collection.
pub fn page_to_json_modes(p: &PageData, modes: &PageModes) -> String {
    let mut nodes: Vec<Node> = p.doc.nodes.values().cloned().collect();
    nodes.sort_by_key(|n| n.id);
    let f = PageFile {
        format: PAGE_FORMAT.into(),
        version: PAGE_VERSION,
        id: p.id.clone(),
        name: p.name.clone(),
        camera: p.camera,
        next_id: p.doc.next_id,
        root: p.doc.root.clone(),
        nodes,
        variables: p.doc.variables.clone(),
        modes: p.doc.modes.clone(),
        mode: p.doc.mode.clone(),
        tokens: p.doc.tokens.clone(),
        modes_by_collection: modes.iter().filter(|(k, _)| *k != PAGE_TEMPLATE_KEY).map(|(k, v)| (k.clone(), v.clone())).collect(),
        annotations: p.doc.annotations.clone(),
        annotation_template: modes.get(PAGE_TEMPLATE_KEY).cloned(),
    };
    serde_json::to_string(&f).expect("page serializes")
}

/// Parse a page file (or a legacy `.paper-rs` document) of any supported version.
pub fn page_from_json(s: &str) -> Result<PageData, FormatError> {
    page_from_json_modes(s).map(|(p, _)| p)
}

/// Like [`page_from_json`], plus the page's active mode per collection.
pub fn page_from_json_modes(s: &str) -> Result<(PageData, PageModes), FormatError> {
    // A current page is read straight into its types; going through a `Value` tree first
    // (needed only to migrate old files) doubles the parse time of a large page.
    let f: PageFile = if is_current_page(s) {
        serde_json::from_str(s).map_err(FormatError::Json)?
    } else {
        let v: Value = serde_json::from_str(s).map_err(FormatError::Json)?;
        let v = migrate(v, PAGE_FORMAT, PAGE_VERSION, PAGE_ALIASES, PAGE_STEPS)?;
        serde_json::from_value(v).map_err(FormatError::Json)?
    };
    page_from_file(f)
}

/// The file's format tag and version are the current ones (nothing to migrate).
fn is_current_page(s: &str) -> bool {
    #[derive(Deserialize)]
    struct Head {
        format: Option<String>,
        version: Option<u32>,
    }
    serde_json::from_str::<Head>(s)
        .is_ok_and(|h| h.format.as_deref() == Some(PAGE_FORMAT) && h.version == Some(PAGE_VERSION))
}

fn page_from_file(mut f: PageFile) -> Result<(PageData, PageModes), FormatError> {
    if let Some(t) = f.annotation_template.take() {
        f.modes_by_collection.insert(PAGE_TEMPLATE_KEY.into(), t);
    }
    let max_id = f.nodes.iter().map(|n| n.id.0).max().unwrap_or(0);
    let nodes: HashMap<NodeId, Node> = f.nodes.into_iter().map(|n| (n.id, n)).collect();
    let doc = Document {
        name: f.name.clone(),
        nodes,
        root: f.root,
        next_id: f.next_id.max(max_id + 1),
        variables: f.variables,
        modes: f.modes,
        mode: f.mode,
        tokens: f.tokens,
        annotations: f.annotations,
    };
    doc.validate().map_err(FormatError::Invalid)?;
    Ok((
        PageData {
            id: f.id,
            name: f.name,
            doc,
            camera: f.camera,
        },
        f.modes_by_collection,
    ))
}

/// Check the format tag and run the migration chain up to `current`.
fn migrate(
    mut v: Value,
    format: &str,
    current: u32,
    aliases: &[&str],
    steps: &[fn(Value) -> Value],
) -> Result<Value, FormatError> {
    let tag = v
        .get("format")
        .and_then(|f| f.as_str())
        .unwrap_or("")
        .to_string();
    if tag != format && !aliases.contains(&tag.as_str()) {
        return Err(FormatError::WrongFormat(tag));
    }
    let mut version = v.get("version").and_then(|x| x.as_u64()).unwrap_or(1) as u32;
    if version > current {
        return Err(FormatError::TooNew {
            version,
            supported: current,
        });
    }
    while version < current {
        let step = steps
            .get(version as usize - 1)
            .ok_or_else(|| FormatError::Invalid(format!("no migration from v{version}")))?;
        v = step(v);
        version += 1;
        v["version"] = json!(version);
    }
    Ok(v)
}

// ---------------------------------------------------------------------------------- helpers

#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Random 16-hex-digit identifier.
pub fn new_id() -> String {
    format!("{:016x}", fastrand::u64(..))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn page_roundtrip_is_deterministic() {
        let ed = demo::showcase();
        let p = PageData {
            id: "abc".into(),
            name: "Plan".into(),
            doc: Document {
                name: "Plan".into(),
                ..ed.doc.clone()
            },
            camera: Some(Camera::default()),
        };
        let s = page_to_json(&p);
        let back = page_from_json(&s).unwrap();
        assert_eq!(back, p);
        assert_eq!(page_to_json(&back), s);
    }

    fn page_with(doc: Document) -> PageData {
        PageData { id: "abc".into(), name: "Plan".into(), doc, camera: None }
    }

    #[test]
    fn annotations_roundtrip_and_old_pages_stay_byte_identical() {
        use crate::model::annotation::{Annotation, Anchor, Side};
        let ed = demo::showcase();
        let plain = page_to_json(&page_with(Document { name: "Plan".into(), ..ed.doc.clone() }));
        assert!(!plain.contains("annotations"), "empty list is not written");
        // An old file (no field) loads and saves to the same bytes.
        let old = page_from_json(&plain).unwrap();
        assert!(old.doc.annotations.is_empty());
        assert_eq!(page_to_json(&old), plain);

        let mut doc = Document { name: "Plan".into(), ..ed.doc.clone() };
        let mut a = Annotation::new(doc.root.first().copied(), "Rule", "business-rule");
        a.id = 77;
        a.body = "**bold**".into();
        a.anchor = Anchor { side: Side::Top, t: 0.5 };
        a.position = crate::geom::v2(12.0, -30.0);
        a.created = 1234;
        doc.annotations.push(a);
        let mut d = Annotation::new(None, "Gone", "data");
        d.id = 78;
        d.point = Some(crate::geom::v2(1.0, 2.0));
        d.detached = true;
        doc.annotations.push(d);
        let p = page_with(doc);
        let s = page_to_json(&p);
        let back = page_from_json(&s).unwrap();
        assert_eq!(back, p);
        assert_eq!(page_to_json(&back), s);
    }

    #[test]
    fn legacy_v1_file_migrates_to_page() {
        let ed = demo::showcase();
        let legacy = crate::io::file::to_string(&ed.doc, None);
        assert!(legacy.contains("\"version\": 1"));
        let p = page_from_json(&legacy).unwrap();
        assert_eq!(p.doc.nodes, ed.doc.nodes);
        assert_eq!(p.name, ed.doc.name);
        assert_eq!(p.id.len(), 16);
    }

    #[test]
    fn rejects_foreign_and_future_files() {
        assert!(matches!(
            page_from_json(r#"{"format":"x","version":1}"#),
            Err(FormatError::WrongFormat(_))
        ));
        assert!(matches!(
            page_from_json(r#"{"format":"paper-rs-page","version":99}"#),
            Err(FormatError::TooNew { version: 99, .. })
        ));
        assert!(matches!(
            Manifest::from_json(r#"{"format":"paper-rs-page","version":1}"#),
            Err(FormatError::WrongFormat(_))
        ));
        let broken = r#"{"format":"paper-rs-page","version":2,"id":"a","name":"x","next_id":1,"root":[5],"nodes":[]}"#;
        assert!(matches!(
            page_from_json(broken),
            Err(FormatError::Invalid(_))
        ));
    }

    #[test]
    fn v2_fill_migrates_to_fill_layers() {
        let v2 = r##"{"format":"paper-rs-page","version":2,"id":"a","name":"P","next_id":3,
            "root":[1,2],"nodes":[
            {"id":1,"name":"R","type":"rect","rect":{"x":0,"y":0,"w":5,"h":5},
             "style":{"fill":"#ff0000","stroke":null,"stroke_width":1,"corner_radius":2,"opacity":1}},
            {"id":2,"name":"E","type":"ellipse","rect":{"x":0,"y":0,"w":5,"h":5},
             "style":{"fill":null,"stroke":"#000000","stroke_width":1,"corner_radius":0,"opacity":1}}]}"##;
        let p = page_from_json(v2).unwrap();
        let r = p.doc.get(NodeId(1)).unwrap();
        assert_eq!(r.style.fill(), Some(crate::model::Color::rgb(255, 0, 0)));
        assert_eq!(r.style.corner_radius, 2.0);
        assert!(p.doc.get(NodeId(2)).unwrap().style.fills.is_empty());
        assert!(page_to_json(&p).contains("\"version\":3"));
    }

    #[test]
    fn v1_manifest_migrates_to_v2_and_v3_is_refused() {
        let v1 = r#"{"format":"paper-rs-project","version":1,"id":"a","name":"N","created":1,
            "modified":2,"pages":[{"id":"p","name":"Page 1"}]}"#;
        assert_eq!(Manifest::from_json(v1).unwrap().version, PROJECT_VERSION);
        assert_eq!(PROJECT_VERSION, 3);
        // What an older reader does with a newer file: TooNew.
        let err = Manifest::from_json(&v1.replace("\"version\":1", "\"version\":4")).unwrap_err();
        assert!(matches!(err, FormatError::TooNew { version: 4, supported: 3 }));
        assert!(err.to_string().contains("made by a newer PaperCraft"));
    }

    #[test]
    fn manifest_roundtrip() {
        let m = Manifest::new("Hello");
        assert_eq!(Manifest::from_json(&m.to_json()).unwrap(), m);
    }
}
