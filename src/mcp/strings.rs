//! Languages as variable modes: `get_strings` reads a page's text for translation and
//! `set_strings` extracts it into a string collection, fills a language, or fills a pseudo-locale.
//! A language is a mode of the collection, never a copy of the page; text layers follow it through
//! the `text` binding, so `set_variable_mode` switches and re-fits them.

use super::ToolResult;
use super::exec::{Args, page_arg, resolve, text};
use crate::editor::Editor;
use crate::model::{Document, NodeId};
use crate::variables::{self, Collection, DesignSystem, Kind, Value as DsVal, Variable};
use serde_json::{Map, Value, json};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

const DEFAULT_COLLECTION: &str = "Strings";
const FIELD: &str = "text";
/// A pseudo-locale is this much longer than its base text, like a typical translation.
const PSEUDO_GROWTH_PERCENT: usize = 35;
const KEY_SLUG_MAX: usize = 32;

/// A text layer in scope and the names from the scope's frame down to it.
struct TextLayer {
    id: NodeId,
    path: Vec<String>,
}

fn layers_under(doc: &Document, id: NodeId, path: &mut Vec<String>, out: &mut Vec<TextLayer>) {
    let Some(n) = doc.get(id) else { return };
    path.push(n.name.clone());
    if n.kind.text().is_some_and(|t| !t.trim().is_empty()) {
        out.push(TextLayer { id, path: path.clone() });
    }
    for c in &n.children {
        layers_under(doc, *c, path, out);
    }
    path.pop();
}

/// The page and its text layers in document order, from `frame` or from every top layer.
fn scope(ed: &Editor, a: &Args) -> Result<(usize, Vec<TextLayer>), String> {
    let def = page_arg(ed, a)?;
    let (page, roots) = match a.str("frame").filter(|f| !f.is_empty()) {
        Some(f) => {
            let (page, id) = resolve(ed, f, def)?;
            (page, vec![id])
        }
        None => (def, ed.page_doc(def).root.clone()),
    };
    let doc = ed.page_doc(page);
    let mut out = Vec::new();
    for r in roots {
        layers_under(doc, r, &mut Vec::new(), &mut out);
    }
    Ok((page, out))
}

fn slug(s: &str) -> String {
    let dashed: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let parts: Vec<&str> = dashed.split('-').filter(|p| !p.is_empty()).collect();
    let joined = parts.join("-");
    let cut: String = joined.chars().take(KEY_SLUG_MAX).collect();
    let cut = cut.trim_end_matches('-');
    if cut.is_empty() { "text".into() } else { cut.into() }
}

fn local_collection<'a>(ds: &'a DesignSystem, key: &str) -> Result<&'a Collection, String> {
    ds.collections
        .iter()
        .find(|c| c.library.is_none() && (c.name == key || c.id == key))
        .ok_or_else(|| {
            let names: Vec<&str> = ds.collections.iter().filter(|c| c.library.is_none()).map(|c| c.name.as_str()).collect();
            format!("no collection `{key}` (collections: {})", names.join(", "))
        })
}

fn string_of(v: &Variable, mode: &str) -> Option<String> {
    match v.values.get(mode) {
        Some(DsVal::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn mode_name<'a>(c: &'a Collection, id: &'a str) -> &'a str {
    c.modes.iter().find(|m| m.id == id).map_or(id, |m| m.name.as_str())
}

pub(super) fn get_strings(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, layers) = scope(ed, a)?;
    let ds = &ed.design;
    let only = a.str("collection").filter(|c| !c.is_empty()).map(|c| local_collection(ds, c).map(|c| c.id.clone())).transpose()?;
    let doc = ed.page_doc(page);
    let mut bound: Vec<(&Collection, &Variable)> = Vec::new();
    let mut unbound = Map::new();
    for l in &layers {
        let Some(n) = doc.get(l.id) else { continue };
        match n.bindings.get(FIELD).and_then(|css| ds.find_by_css(css)) {
            Some((c, v)) if v.kind == Kind::String => {
                let fits = only.as_ref().is_none_or(|o| *o == c.id);
                if fits && !bound.iter().any(|(_, b)| b.id == v.id) {
                    bound.push((c, v));
                }
            }
            _ if only.is_none() => {
                unbound.insert(super::exec::node_key(ed, page, l.id), json!(n.kind.text()));
            }
            _ => {}
        }
    }
    if let Some(mode) = a.str("missing").filter(|m| !m.is_empty()) {
        return missing_only(&bound, mode);
    }
    let mut strings = Map::new();
    let mut missing: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (c, v) in &bound {
        let per_mode: Map<String, Value> = c
            .modes
            .iter()
            .filter_map(|m| string_of(v, &m.id).map(|s| (m.name.clone(), json!(s))))
            .collect();
        strings.insert(v.name.clone(), Value::Object(per_mode));
        for m in c.modes.iter().filter(|m| string_of(v, &m.id).is_none()) {
            missing.entry(m.name.clone()).or_default().push(&v.name);
        }
    }
    let mut collections: Vec<&str> = bound.iter().map(|(c, _)| c.name.as_str()).collect();
    collections.sort_unstable();
    collections.dedup();
    text(json!({"collections": collections, "strings": strings, "unbound": unbound, "missing": missing}))
}

fn missing_only(bound: &[(&Collection, &Variable)], mode: &str) -> ToolResult {
    let mut missing = Map::new();
    let mut base = None;
    for (c, v) in bound {
        let Some(m) = c.modes.iter().find(|m| m.name == mode || m.id == mode) else { continue };
        base.get_or_insert_with(|| mode_name(c, &c.default_mode).to_string());
        if string_of(v, &m.id).is_none() {
            missing.insert(v.name.clone(), json!(string_of(v, &c.default_mode).unwrap_or_default()));
        }
    }
    let Some(base) = base else {
        return Err(format!("no mode `{mode}` in the bound collections; create it with set_strings"));
    };
    text(json!({"mode": mode, "base": base, "missing": missing}))
}

pub(super) fn set_strings(ed: &mut Editor, a: &Args) -> ToolResult {
    if a.bool("extract") == Some(true) {
        return extract(ed, a);
    }
    let collection = a.str("collection").filter(|c| !c.is_empty()).ok_or("`collection` is required (or pass extract: true)")?;
    let mode = a.str("mode").filter(|m| !m.is_empty()).ok_or("`mode` is required")?;
    let c = local_collection(&ed.design, collection)?;
    let writes: Vec<(String, String)> = if a.bool("pseudo") == Some(true) {
        // A pseudo-locale of the base mode would overwrite the text it is made from.
        if c.modes.iter().any(|m| m.id == c.default_mode && (m.name == mode || m.id == mode)) {
            return Err(format!("`{mode}` is the base mode of `{}`; pass another mode for the pseudo-locale", c.name));
        }
        pseudo_writes(c)
    } else {
        let strings = a.get("strings").and_then(Value::as_object).ok_or("pass `strings` {key: text}, `pseudo` true or `extract` true")?;
        checked_strings(c, strings)?
    };
    let cid = c.id.clone();
    let count = writes.len();
    ed.edit_design("Set strings", |ds| {
        let mid = ensure_mode(ds, &cid, mode)?;
        for (vid, text) in writes {
            ds.set_value(&vid, &mid, DsVal::String(text))?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    text(json!({"collection": collection, "mode": mode, "set": count}))
}

/// `(variable id, text)` for every key, or an error naming all the bad keys and applying nothing.
fn checked_strings(c: &Collection, strings: &Map<String, Value>) -> Result<Vec<(String, String)>, String> {
    let (mut unknown, mut not_string, mut not_text) = (Vec::new(), Vec::new(), Vec::new());
    let mut out = Vec::new();
    for (key, val) in strings {
        let css = crate::design_system::sanitize_name(&key.replace('/', "-"));
        let Some(v) = c.variables.iter().find(|v| v.name == *key || variables::css_name(v) == css) else {
            unknown.push(key.as_str());
            continue;
        };
        match val.as_str() {
            _ if v.kind != Kind::String => not_string.push(key.as_str()),
            Some(t) => out.push((v.id.clone(), t.to_string())),
            None => not_text.push(key.as_str()),
        }
    }
    let mut problems = Vec::new();
    for (what, keys) in [("unknown keys", &unknown), ("not string variables", &not_string), ("values that are not text", &not_text)] {
        if !keys.is_empty() {
            problems.push(format!("{what}: {}", keys.join(", ")));
        }
    }
    if problems.is_empty() {
        return Ok(out);
    }
    Err(format!("nothing applied; collection `{}` has {}", c.name, problems.join("; ")))
}

fn pseudo_writes(c: &Collection) -> Vec<(String, String)> {
    c.variables
        .iter()
        .filter(|v| v.kind == Kind::String)
        .filter_map(|v| string_of(v, &c.default_mode).filter(|s| !s.is_empty()).map(|s| (v.id.clone(), pseudo(&s))))
        .collect()
}

/// A mode by name or id, created when missing. A new mode starts with no value of its own (not
/// the copies `add_mode` seeds), so `get_strings` can tell what is still untranslated; a variable
/// without a value in the active mode shows its base text.
fn ensure_mode(ds: &mut DesignSystem, cid: &str, name: &str) -> Result<String, variables::Error> {
    let c = ds.collection(cid).ok_or_else(|| variables::Error::NotFound(cid.into()))?;
    if let Some(m) = c.modes.iter().find(|m| m.name == name || m.id == name) {
        return Ok(m.id.clone());
    }
    let id = ds.add_mode(cid, name)?;
    if let Some(c) = ds.collections.iter_mut().find(|c| c.id == cid) {
        for v in c.variables.iter_mut().filter(|v| v.kind == Kind::String) {
            v.values.remove(&id);
        }
    }
    Ok(id)
}

fn accent(c: char) -> char {
    match c {
        'a' => 'á', 'e' => 'é', 'i' => 'í', 'o' => 'ó', 'u' => 'ú', 'y' => 'ý', 'c' => 'ç', 'n' => 'ñ',
        'A' => 'Á', 'E' => 'É', 'I' => 'Í', 'O' => 'Ó', 'U' => 'Ú', 'C' => 'Ç', 'N' => 'Ñ',
        other => other,
    }
}

/// `[Ñéw ñéw ...]`: accented, bracketed, with a tail of the text's own letters for the extra length.
fn pseudo(base: &str) -> String {
    let accented: String = base.chars().map(accent).collect();
    let extra = (base.chars().count() * PSEUDO_GROWTH_PERCENT).div_ceil(100);
    let tail: String = accented.chars().cycle().take(extra).collect();
    format!("[{accented} {tail}]")
}

/// One unbound text layer to extract: its key in the collection and the text it holds.
struct Planned {
    id: NodeId,
    key: String,
    css: String,
    text: String,
}

fn extract(ed: &mut Editor, a: &Args) -> ToolResult {
    let (page, layers) = scope(ed, a)?;
    let doc = ed.page_doc(page);
    let todo: Vec<(NodeId, Vec<String>, String)> = layers
        .into_iter()
        .filter_map(|l| {
            let n = doc.get(l.id)?;
            let t = n.kind.text()?;
            (!n.bindings.contains_key(FIELD)).then(|| (l.id, l.path, t.to_string()))
        })
        .collect();
    let name = a.str("collection").filter(|c| !c.is_empty()).unwrap_or(DEFAULT_COLLECTION).to_string();
    if todo.is_empty() {
        return text(json!({"collection": name, "strings": {}}));
    }
    let base_mode = a.str("mode").filter(|m| !m.is_empty()).unwrap_or(variables::DEFAULT_MODE).to_string();
    let plan: RefCell<Vec<Planned>> = RefCell::new(Vec::new());
    let calls = Cell::new(0usize);
    // The bindings go in the same step as the variables: `edit_design_each` visits every page once,
    // in order, so the visit counter says when the page being extracted is the one in hand.
    let bind = |tx: &mut crate::history::Tx| {
        let visit = calls.replace(calls.get() + 1);
        if visit != page {
            return;
        }
        for p in plan.borrow().iter() {
            if let Some(n) = tx.node_mut(p.id) {
                n.bindings.insert(FIELD.into(), p.css.clone());
            }
        }
    };
    let (collection, _) = ed
        .edit_design_each(
            "Extract strings",
            |ds| {
                let existing = ds.collections.iter().find(|c| c.library.is_none() && (c.name == name || c.id == name)).map(|c| c.id.clone());
                let cid = match existing {
                    Some(id) => id,
                    None => {
                        let id = ds.add_collection(&name);
                        if let Some(c) = ds.collections.iter_mut().find(|c| c.id == id) {
                            c.modes[0].name = base_mode.clone();
                        }
                        id
                    }
                };
                let base = ds.collection(&cid).map(|c| c.default_mode.clone()).ok_or_else(|| variables::Error::NotFound(cid.clone()))?;
                for (id, path, text) in &todo {
                    let key = unique_key(ds, &path.iter().map(|p| slug(p)).collect::<Vec<_>>().join("/"));
                    let vid = ds.add_variable(&cid, &key, Kind::String)?;
                    ds.set_value(&vid, &base, DsVal::String(text.clone()))?;
                    // Other modes start untranslated and show the base text (see `ensure_mode`).
                    if let Some(c) = ds.collections.iter_mut().find(|c| c.id == cid)
                        && let Some(v) = c.variables.iter_mut().find(|v| v.id == vid)
                    {
                        v.values.retain(|m, _| *m == base);
                    }
                    let css = ds.find(&vid).map(|(_, v)| variables::css_name(v)).ok_or_else(|| variables::Error::NotFound(vid.clone()))?;
                    plan.borrow_mut().push(Planned { id: *id, key, css, text: text.clone() });
                }
                Ok(ds.collection(&cid).map(|c| c.name.clone()).unwrap_or_default())
            },
            &bind,
        )
        .map_err(|e| e.to_string())?;
    let strings: Map<String, Value> = plan.borrow().iter().map(|p| (p.key.clone(), json!(p.text))).collect();
    text(json!({"collection": collection, "strings": strings}))
}

/// `base`, or `base-2`, `base-3`... when a variable with that CSS name already exists.
fn unique_key(ds: &DesignSystem, base: &str) -> String {
    let taken = |name: &str| ds.find_by_css(&crate::design_system::sanitize_name(&name.replace('/', "-"))).is_some();
    if !taken(base) {
        return base.into();
    }
    (2..).map(|i| format!("{base}-{i}")).find(|k| !taken(k)).unwrap_or_else(|| base.into())
}
