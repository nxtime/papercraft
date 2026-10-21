//! The project-level design system: collections of typed variables with modes, groups and aliases.
//! Pure model. `cache_for` derives the per-page `Document` fields the rest of the app reads.

use crate::design_system::sanitize_name;
use crate::model::Color;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const DEFAULT_MODE: &str = "Default";
pub const COLORS_COLLECTION: &str = "Colors";
pub const TOKENS_COLLECTION: &str = "Tokens";

/// Active mode id per collection id (missing = the collection's default mode).
pub type PageModes = BTreeMap<String, String>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DesignSystem {
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub libraries: Vec<EnabledLibrary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnabledLibrary {
    pub library_id: String,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub modes: Vec<Mode>,
    pub default_mode: String,
    #[serde(default)]
    pub variables: Vec<Variable>,
    /// `Some(library id)` = read-only copy from a library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mode {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Color,
    Number,
    String,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Value {
    Color(Color),
    Number(f64),
    String(String),
    Boolean(bool),
    /// Variable id.
    Alias(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    pub id: String,
    /// Slash path: `color/brand/primary`.
    pub name: String,
    pub kind: Kind,
    /// mode id -> value.
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    NotFound(String),
    ReadOnly(String),
    KindMismatch(String),
    Cycle(String),
    /// Two variables share this CSS name.
    Duplicate(String),
    /// Alias target(s) still referencing the variable (names).
    InUse(Vec<String>),
    /// No value in the mode and none in the default.
    Missing(String),
    LastMode,
    /// Empty name or empty `/` segment (`a//b`, `/a`, `a/`).
    InvalidName(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound(s) => write!(f, "not found: {s}"),
            Error::ReadOnly(s) => write!(f, "collection {s} is read-only"),
            Error::KindMismatch(s) => write!(f, "kind mismatch: {s}"),
            Error::Cycle(s) => write!(f, "alias cycle through {s}"),
            Error::Duplicate(s) => write!(f, "duplicate CSS name --{s}"),
            Error::InUse(v) => write!(f, "aliased by {}", v.join(", ")),
            Error::Missing(s) => write!(f, "no value for {s}"),
            Error::LastMode => write!(f, "a collection needs at least one mode"),
            Error::InvalidName(s) => write!(f, "invalid variable name {s:?} (empty group segment)"),
        }
    }
}

impl std::error::Error for Error {}

pub fn new_id() -> String {
    format!("{:08x}", fastrand::u32(..))
}

/// An id derived from `parts` (FNV-1a, 64 bit): the same input gives the same id on every
/// device. Migration uses it so one v1 project migrated on two devices gets one design system.
pub fn stable_id(parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in parts.iter().flat_map(|p| p.bytes().chain([0xff])) {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

impl Value {
    pub fn kind(&self) -> Option<Kind> {
        match self {
            Value::Color(_) => Some(Kind::Color),
            Value::Number(_) => Some(Kind::Number),
            Value::String(_) => Some(Kind::String),
            Value::Boolean(_) => Some(Kind::Boolean),
            Value::Alias(_) => None,
        }
    }
}

impl Kind {
    pub fn default_value(self) -> Value {
        match self {
            Kind::Color => Value::Color(Color::rgb(0, 0, 0)),
            Kind::Number => Value::Number(0.0),
            Kind::String => Value::String(String::new()),
            Kind::Boolean => Value::Boolean(false),
        }
    }
}

// ------------------------------------------------------------------------------ names, groups

/// `color/brand/primary` -> `color-brand-primary` (no leading `--`).
pub fn css_name(v: &Variable) -> String {
    sanitize_name(&v.name.replace('/', "-"))
}

/// `color/brand/primary` -> `color/brand`; no slash -> "".
pub fn group_of(name: &str) -> &str {
    name.rsplit_once('/').map_or("", |(g, _)| g)
}

/// Every group path (all prefixes), sorted and unique.
pub fn groups(c: &Collection) -> Vec<String> {
    let mut out = BTreeSet::new();
    for v in &c.variables {
        // Prefixes end at each `/` (a char boundary) and at the end; empty segments are skipped.
        let g = group_of(&v.name);
        let ends = g.match_indices('/').map(|(i, _)| i).chain([g.len()]);
        for end in ends {
            let p = &g[..end];
            if !p.is_empty() && !p.ends_with('/') && !p.split('/').any(str::is_empty) {
                out.insert(p.to_string());
            }
        }
    }
    out.into_iter().collect()
}

fn under(name: &str, group: &str) -> bool {
    name.strip_prefix(group).is_some_and(|r| r.starts_with('/'))
}

// ------------------------------------------------------------------------------ lookup

impl DesignSystem {
    pub fn collection(&self, id: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }

    fn local_mut(&mut self, id: &str) -> Result<&mut Collection, Error> {
        let c = self
            .collections
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| Error::NotFound(id.into()))?;
        if c.library.is_some() {
            return Err(Error::ReadOnly(c.name.clone()));
        }
        Ok(c)
    }

    /// The variable and its collection.
    pub fn find(&self, var_id: &str) -> Option<(&Collection, &Variable)> {
        self.collections
            .iter()
            .find_map(|c| c.variables.iter().find(|v| v.id == var_id).map(|v| (c, v)))
    }

    pub fn find_by_css(&self, css: &str) -> Option<(&Collection, &Variable)> {
        let css = sanitize_name(css);
        self.collections.iter().find_map(|c| {
            c.variables
                .iter()
                .find(|v| css_name(v) == css)
                .map(|v| (c, v))
        })
    }

    fn all(&self) -> impl Iterator<Item = (&Collection, &Variable)> {
        self.collections
            .iter()
            .flat_map(|c| c.variables.iter().map(move |v| (c, v)))
    }

    fn var_mut(&mut self, var_id: &str) -> Result<(&mut Collection, usize), Error> {
        let c = self
            .collections
            .iter_mut()
            .find(|c| c.variables.iter().any(|v| v.id == var_id))
            .ok_or_else(|| Error::NotFound(var_id.into()))?;
        if c.library.is_some() {
            return Err(Error::ReadOnly(c.name.clone()));
        }
        let i = c.variables.iter().position(|v| v.id == var_id).unwrap();
        Ok((c, i))
    }

    /// Names of variables that alias `var_id` in any mode.
    pub fn dependents(&self, var_id: &str) -> Vec<String> {
        self.all()
            .filter(|(_, v)| {
                v.values
                    .values()
                    .any(|x| matches!(x, Value::Alias(t) if t == var_id))
            })
            .map(|(_, v)| v.name.clone())
            .collect()
    }

    // -------------------------------------------------------------------------- validation

    /// Duplicate CSS names, dangling or mismatched aliases, alias cycles.
    pub fn validate(&self) -> Result<(), Error> {
        let mut seen = BTreeSet::new();
        for (_, v) in self.all() {
            if v.name.split('/').any(|s| s.trim().is_empty()) {
                return Err(Error::InvalidName(v.name.clone()));
            }
            let css = css_name(v);
            if !seen.insert(css.clone()) {
                return Err(Error::Duplicate(css));
            }
            for x in v.values.values() {
                let Value::Alias(t) = x else { continue };
                let (_, target) = self.find(t).ok_or_else(|| Error::NotFound(t.clone()))?;
                if target.kind != v.kind {
                    return Err(Error::KindMismatch(v.name.clone()));
                }
                if self.reaches(t, &v.id) {
                    return Err(Error::Cycle(v.name.clone()));
                }
            }
        }
        Ok(())
    }

    /// True when `from` is `to` or aliases it through any chain of modes.
    fn reaches(&self, from: &str, to: &str) -> bool {
        let mut stack = vec![from.to_string()];
        let mut seen = BTreeSet::new();
        while let Some(id) = stack.pop() {
            if id == to {
                return true;
            }
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some((_, v)) = self.find(&id) else {
                continue;
            };
            for x in v.values.values() {
                if let Value::Alias(t) = x {
                    stack.push(t.clone());
                }
            }
        }
        false
    }

    // -------------------------------------------------------------------------- edits

    pub fn add_collection(&mut self, name: &str) -> String {
        let mode = Mode {
            id: new_id(),
            name: DEFAULT_MODE.into(),
        };
        let id = new_id();
        self.collections.push(Collection {
            id: id.clone(),
            name: name.into(),
            default_mode: mode.id.clone(),
            modes: vec![mode],
            variables: Vec::new(),
            library: None,
        });
        id
    }

    pub fn add_variable(
        &mut self,
        collection_id: &str,
        name: &str,
        kind: Kind,
    ) -> Result<String, Error> {
        let c = self.local_mut(collection_id)?;
        let v = Variable {
            id: new_id(),
            name: name.into(),
            kind,
            values: c
                .modes
                .iter()
                .map(|m| (m.id.clone(), kind.default_value()))
                .collect(),
            description: String::new(),
            hidden: false,
        };
        let id = v.id.clone();
        c.variables.push(v);
        if let Err(e) = self.validate() {
            self.local_mut(collection_id)?.variables.pop();
            return Err(e);
        }
        Ok(id)
    }

    /// Removes the variable. Aliased and not `force`: refused with the dependents. With `force`,
    /// dependents keep the resolved literal. Returns the dependents either way.
    pub fn remove_variable(&mut self, var_id: &str, force: bool) -> Result<Vec<String>, Error> {
        self.var_mut(var_id)?;
        let deps = self.dependents(var_id);
        if !deps.is_empty() && !force {
            return Err(Error::InUse(deps));
        }
        // Each dependent mode keeps what the target resolved to in that mode (same collection:
        // the same mode; another collection: its default).
        let mut literals = Vec::new();
        for (c, v) in self.all() {
            for (mode, x) in &v.values {
                if matches!(x, Value::Alias(t) if t == var_id) {
                    let modes = PageModes::from([(c.id.clone(), mode.clone())]);
                    literals.push((v.id.clone(), mode.clone(), self.resolve(var_id, &modes).ok()));
                }
            }
        }
        for (vid, mode, literal) in literals {
            for c in &mut self.collections {
                for v in c.variables.iter_mut().filter(|v| v.id == vid) {
                    let lit = literal.clone().unwrap_or_else(|| v.kind.default_value());
                    v.values.insert(mode.clone(), lit);
                }
            }
        }
        let (c, i) = self.var_mut(var_id)?;
        c.variables.remove(i);
        Ok(deps)
    }

    pub fn rename_variable(&mut self, var_id: &str, name: &str) -> Result<(), Error> {
        let (c, i) = self.var_mut(var_id)?;
        let old = std::mem::replace(&mut c.variables[i].name, name.into());
        if let Err(e) = self.validate() {
            self.var_mut(var_id)?.0.variables[i].name = old;
            return Err(e);
        }
        Ok(())
    }

    /// Renames group `old` (and subgroups) to `new`; returns how many variables moved.
    pub fn rename_group(
        &mut self,
        collection_id: &str,
        old: &str,
        new: &str,
    ) -> Result<usize, Error> {
        let before = self.clone();
        let c = self.local_mut(collection_id)?;
        let mut n = 0;
        for v in &mut c.variables {
            if v.name == old || under(&v.name, old) {
                v.name = format!("{new}{}", &v.name[old.len()..]);
                n += 1;
            }
        }
        if let Err(e) = self.validate() {
            *self = before;
            return Err(e);
        }
        Ok(n)
    }

    /// Sets a literal or alias for one mode. Kind must match; alias must not make a cycle.
    pub fn set_value(&mut self, var_id: &str, mode_id: &str, value: Value) -> Result<(), Error> {
        let (c, i) = self.var_mut(var_id)?;
        let v = &mut c.variables[i];
        if value.kind().is_some_and(|k| k != v.kind) {
            return Err(Error::KindMismatch(v.name.clone()));
        }
        if !c.modes.iter().any(|m| m.id == mode_id) {
            return Err(Error::NotFound(mode_id.into()));
        }
        let old = c.variables[i].values.insert(mode_id.into(), value);
        if let Err(e) = self.validate() {
            let (c, i) = self.var_mut(var_id)?;
            match old {
                Some(o) => c.variables[i].values.insert(mode_id.into(), o),
                None => c.variables[i].values.remove(mode_id),
            };
            return Err(e);
        }
        Ok(())
    }

    /// New mode; each variable copies its default-mode value.
    pub fn add_mode(&mut self, collection_id: &str, name: &str) -> Result<String, Error> {
        let c = self.local_mut(collection_id)?;
        let id = new_id();
        for v in &mut c.variables {
            let seed = v
                .values
                .get(&c.default_mode)
                .cloned()
                .unwrap_or_else(|| v.kind.default_value());
            v.values.insert(id.clone(), seed);
        }
        c.modes.push(Mode {
            id: id.clone(),
            name: name.into(),
        });
        Ok(id)
    }

    /// Drops the mode and every value in it.
    pub fn remove_mode(&mut self, collection_id: &str, mode_id: &str) -> Result<(), Error> {
        let c = self.local_mut(collection_id)?;
        if c.modes.len() < 2 {
            return Err(Error::LastMode);
        }
        let i = c
            .modes
            .iter()
            .position(|m| m.id == mode_id)
            .ok_or_else(|| Error::NotFound(mode_id.into()))?;
        c.modes.remove(i);
        for v in &mut c.variables {
            v.values.remove(mode_id);
        }
        if c.default_mode == mode_id {
            c.default_mode = c.modes[0].id.clone();
        }
        Ok(())
    }

    // -------------------------------------------------------------------------- resolve

    fn active_mode<'a>(c: &'a Collection, modes: &'a PageModes) -> &'a str {
        modes
            .get(&c.id)
            .filter(|m| c.modes.iter().any(|x| &x.id == *m))
            .unwrap_or(&c.default_mode)
    }

    /// The literal value of a variable, following aliases. Each variable resolves in its own
    /// collection's active mode (or default).
    pub fn resolve(&self, var_id: &str, modes: &PageModes) -> Result<Value, Error> {
        let mut id = var_id.to_string();
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(id.clone()) {
                return Err(Error::Cycle(id));
            }
            let (c, v) = self.find(&id).ok_or_else(|| Error::NotFound(id.clone()))?;
            let value = v
                .values
                .get(Self::active_mode(c, modes))
                .or_else(|| v.values.get(&c.default_mode))
                .ok_or_else(|| Error::Missing(v.name.clone()))?;
            match value {
                Value::Alias(t) => id = t.clone(),
                lit => return Ok(lit.clone()),
            }
        }
    }

    // -------------------------------------------------------------------------- cache

    /// Today's `Document` fields (`variables`, `modes`, `mode`, `tokens`) for a page, keyed by CSS
    /// name. `modes`/`mode` describe the first collection with color variables when it has more
    /// than one mode.
    pub fn cache_for(&self, page_modes: &PageModes) -> Cache {
        let mut out = Cache::default();
        for (_, v) in self.all() {
            let Ok(value) = self.resolve(&v.id, page_modes) else {
                continue;
            };
            let css = css_name(v);
            match value {
                Value::Color(c) => {
                    out.variables.insert(css, c);
                }
                Value::Number(n) => {
                    out.tokens.insert(css, format!("{n}px"));
                }
                Value::String(s) => {
                    out.tokens.insert(css, s);
                }
                Value::Boolean(b) => {
                    out.bools.insert(css, b);
                }
                Value::Alias(_) => {}
            }
        }
        let Some(c) = self
            .collections
            .iter()
            .find(|c| c.variables.iter().any(|v| v.kind == Kind::Color))
            .filter(|c| c.modes.len() > 1)
        else {
            return out;
        };
        for m in &c.modes {
            let mut one = page_modes.clone();
            one.insert(c.id.clone(), m.id.clone());
            let colors = c
                .variables
                .iter()
                .filter(|v| v.kind == Kind::Color)
                .filter_map(|v| match self.resolve(&v.id, &one) {
                    Ok(Value::Color(col)) => Some((css_name(v), col)),
                    _ => None,
                })
                .collect();
            out.modes.insert(m.name.clone(), colors);
        }
        let active = Self::active_mode(c, page_modes);
        out.mode = c
            .modes
            .iter()
            .find(|m| m.id == active)
            .map(|m| m.name.clone());
        out
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cache {
    pub variables: BTreeMap<String, Color>,
    pub modes: BTreeMap<String, BTreeMap<String, Color>>,
    pub mode: Option<String>,
    pub tokens: BTreeMap<String, String>,
    /// Boolean variables by CSS name. Not a page field: they bind to `visible` and are never
    /// written into `tokens` (they are not CSS values).
    pub bools: BTreeMap<String, bool>,
}

// ------------------------------------------------------------------------------ migration

/// `16px` -> 16. A unitless value (a line-height such as `1.5`) stays a string, since numbers
/// come back out as px.
fn parse_number(s: &str) -> Option<f64> {
    s.trim().strip_suffix("px")?.trim().parse().ok()
}

/// Ids come from names (`stable_id`), so migrating the same project on two devices agrees.
pub fn migrate_from_document(doc: &crate::model::Document) -> DesignSystem {
    let mut ds = DesignSystem::default();
    let collection = |name: &str, modes: Vec<Mode>, default_mode: String| Collection {
        id: stable_id(&["collection", name]),
        name: name.into(),
        modes,
        default_mode,
        variables: Vec::new(),
        library: None,
    };
    let mode = |coll: &str, name: &str| Mode {
        id: stable_id(&["mode", coll, name]),
        name: name.into(),
    };
    let mut taken = BTreeSet::new();
    if !doc.variables.is_empty() || !doc.modes.is_empty() {
        let mut modes: Vec<Mode> = doc
            .modes
            .keys()
            .map(|n| mode(COLORS_COLLECTION, n))
            .collect();
        if modes.is_empty() {
            modes.push(mode(COLORS_COLLECTION, DEFAULT_MODE));
        }
        let active = doc
            .mode
            .as_ref()
            .and_then(|n| modes.iter().find(|m| &m.name == n))
            .unwrap_or(&modes[0])
            .id
            .clone();
        let mut c = collection(COLORS_COLLECTION, modes, active);
        let names: BTreeSet<&String> = doc
            .variables
            .keys()
            .chain(doc.modes.values().flat_map(|m| m.keys()))
            .collect();
        for name in names {
            let values = if doc.modes.is_empty() {
                doc.variables
                    .get(name)
                    .map(|col| (c.modes[0].id.clone(), Value::Color(*col)))
                    .into_iter()
                    .collect()
            } else {
                c.modes
                    .iter()
                    .filter_map(|m| {
                        let col = doc.modes[&m.name]
                            .get(name)
                            .or_else(|| doc.variables.get(name))?;
                        Some((m.id.clone(), Value::Color(*col)))
                    })
                    .collect()
            };
            let v = Variable {
                id: stable_id(&["var", COLORS_COLLECTION, name]),
                name: name.clone(),
                kind: Kind::Color,
                values,
                description: String::new(),
                hidden: false,
            };
            taken.insert(css_name(&v));
            c.variables.push(v);
        }
        ds.collections.push(c);
    }
    if !doc.tokens.is_empty() {
        let m = mode(TOKENS_COLLECTION, DEFAULT_MODE);
        let mut c = collection(TOKENS_COLLECTION, vec![m.clone()], m.id.clone());
        for (name, raw) in &doc.tokens {
            let (kind, value) = match parse_number(raw) {
                Some(n) => (Kind::Number, Value::Number(n)),
                None => (Kind::String, Value::String(raw.clone())),
            };
            // A color and a token may share a CSS name in an old page; the token yields.
            let mut name = name.clone();
            while taken.contains(&sanitize_name(&name.replace('/', "-"))) {
                name.push_str("-token");
            }
            let v = Variable {
                id: stable_id(&["var", TOKENS_COLLECTION, &name]),
                name,
                kind,
                values: BTreeMap::from([(m.id.clone(), value)]),
                description: String::new(),
                hidden: false,
            };
            taken.insert(css_name(&v));
            c.variables.push(v);
        }
        ds.collections.push(c);
    }
    ds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Document;

    fn red() -> Color {
        Color::rgb(255, 0, 0)
    }

    fn ds_with(names: &[&str]) -> (DesignSystem, String) {
        let mut ds = DesignSystem::default();
        let c = ds.add_collection("C");
        for n in names {
            ds.add_variable(&c, n, Kind::Color).unwrap();
        }
        (ds, c)
    }

    fn id_of(ds: &DesignSystem, name: &str) -> String {
        ds.all().find(|(_, v)| v.name == name).unwrap().1.id.clone()
    }

    #[test]
    fn groups_and_rename() {
        let (mut ds, c) = ds_with(&[
            "color/brand/primary",
            "color/brand/accent",
            "color/bg",
            "solo",
        ]);
        assert_eq!(groups(ds.collection(&c).unwrap()), ["color", "color/brand"]);
        assert_eq!(ds.rename_group(&c, "color/brand", "palette").unwrap(), 2);
        assert!(ds.find_by_css("palette-brand-primary").is_none());
        assert!(ds.find_by_css("palette-primary").is_some());
        assert!(ds.find_by_css("color-bg").is_some());
        assert_eq!(group_of("a/b/c"), "a/b");
        assert_eq!(group_of("solo"), "");
    }

    #[test]
    fn rename_group_duplicate_refused_and_reverted() {
        let (mut ds, c) = ds_with(&["a/x", "b/x"]);
        let before = ds.clone();
        assert!(matches!(
            ds.rename_group(&c, "a", "b"),
            Err(Error::Duplicate(_))
        ));
        assert_eq!(ds, before);
    }

    #[test]
    fn alias_chain_across_collections_per_mode() {
        let (mut ds, c1) = ds_with(&["base"]);
        let c2 = ds.add_collection("Theme");
        let dark = ds.add_mode(&c2, "Dark").unwrap();
        let light = ds.collection(&c2).unwrap().default_mode.clone();
        let base = id_of(&ds, "base");
        let m1 = ds.collection(&c1).unwrap().default_mode.clone();
        ds.set_value(&base, &m1, Value::Color(red())).unwrap();
        let mid = ds.add_variable(&c2, "mid", Kind::Color).unwrap();
        ds.set_value(&mid, &light, Value::Alias(base.clone()))
            .unwrap();
        ds.set_value(&mid, &dark, Value::Color(Color::rgb(0, 0, 255)))
            .unwrap();
        let top = ds.add_variable(&c1, "top", Kind::Color).unwrap();
        ds.set_value(&top, &m1, Value::Alias(mid.clone())).unwrap();
        assert_eq!(
            ds.resolve(&top, &PageModes::new()).unwrap(),
            Value::Color(red())
        );
        let modes = PageModes::from([(c2.clone(), dark)]);
        assert_eq!(
            ds.resolve(&top, &modes).unwrap(),
            Value::Color(Color::rgb(0, 0, 255))
        );
    }

    #[test]
    fn cycle_and_kind_refused() {
        let (mut ds, c) = ds_with(&["a", "b"]);
        let (a, b) = (id_of(&ds, "a"), id_of(&ds, "b"));
        let m = ds.collection(&c).unwrap().default_mode.clone();
        ds.set_value(&a, &m, Value::Alias(b.clone())).unwrap();
        assert!(matches!(
            ds.set_value(&b, &m, Value::Alias(a.clone())),
            Err(Error::Cycle(_))
        ));
        assert!(matches!(
            ds.set_value(&a, &m, Value::Alias(a.clone())),
            Err(Error::Cycle(_))
        ));
        assert!(matches!(
            ds.set_value(&b, &m, Value::Number(1.0)),
            Err(Error::KindMismatch(_))
        ));
        let n = ds.add_variable(&c, "n", Kind::Number).unwrap();
        assert!(matches!(
            ds.set_value(&a, &m, Value::Alias(n)),
            Err(Error::KindMismatch(_))
        ));
    }

    #[test]
    fn duplicate_css_refused() {
        let (mut ds, c) = ds_with(&["a/b"]);
        assert!(matches!(
            ds.add_variable(&c, "a-b", Kind::Number),
            Err(Error::Duplicate(_))
        ));
        let c2 = ds.add_collection("Other");
        assert!(matches!(
            ds.add_variable(&c2, "a/b", Kind::Color),
            Err(Error::Duplicate(_))
        ));
        assert_eq!(ds.all().count(), 1);
    }

    #[test]
    fn remove_aliased_reports_dependents() {
        let (mut ds, c) = ds_with(&["base", "user"]);
        let (base, user) = (id_of(&ds, "base"), id_of(&ds, "user"));
        let m = ds.collection(&c).unwrap().default_mode.clone();
        ds.set_value(&base, &m, Value::Color(red())).unwrap();
        ds.set_value(&user, &m, Value::Alias(base.clone())).unwrap();
        assert_eq!(
            ds.remove_variable(&base, false),
            Err(Error::InUse(vec!["user".into()]))
        );
        assert_eq!(
            ds.remove_variable(&base, true).unwrap(),
            vec!["user".to_string()]
        );
        assert_eq!(
            ds.resolve(&user, &PageModes::new()).unwrap(),
            Value::Color(red())
        );
        assert!(ds.find(&base).is_none());
    }

    #[test]
    fn modes_add_remove() {
        let (mut ds, c) = ds_with(&["a"]);
        let a = id_of(&ds, "a");
        let d = ds.add_mode(&c, "Dark").unwrap();
        assert!(ds.find(&a).unwrap().1.values.contains_key(&d));
        ds.remove_mode(&c, &d).unwrap();
        assert!(!ds.find(&a).unwrap().1.values.contains_key(&d));
        let only = ds.collection(&c).unwrap().default_mode.clone();
        assert_eq!(ds.remove_mode(&c, &only), Err(Error::LastMode));
    }

    #[test]
    fn library_collection_read_only() {
        let (mut ds, c) = ds_with(&["a"]);
        ds.collections[0].library = Some("lib".into());
        assert!(matches!(
            ds.add_variable(&c, "b", Kind::Color),
            Err(Error::ReadOnly(_))
        ));
    }

    #[test]
    fn cache_shape() {
        let mut ds = DesignSystem::default();
        let c = ds.add_collection("Colors");
        let light = ds.collection(&c).unwrap().default_mode.clone();
        let dark = ds.add_mode(&c, "Dark").unwrap();
        let bg = ds.add_variable(&c, "color/bg", Kind::Color).unwrap();
        ds.set_value(&bg, &light, Value::Color(Color::rgb(255, 255, 255)))
            .unwrap();
        ds.set_value(&bg, &dark, Value::Color(Color::rgb(0, 0, 0)))
            .unwrap();
        let t = ds.add_collection("Tokens");
        let sp = ds.add_variable(&t, "space/md", Kind::Number).unwrap();
        let tm = ds.collection(&t).unwrap().default_mode.clone();
        ds.set_value(&sp, &tm, Value::Number(16.0)).unwrap();
        let f = ds.add_variable(&t, "font", Kind::String).unwrap();
        ds.set_value(&f, &tm, Value::String("Inter".into()))
            .unwrap();
        let b = ds.add_variable(&t, "flag", Kind::Boolean).unwrap();
        ds.set_value(&b, &tm, Value::Boolean(true)).unwrap();

        let cache = ds.cache_for(&PageModes::from([(c.clone(), dark)]));
        assert_eq!(cache.variables["color-bg"], Color::rgb(0, 0, 0));
        assert_eq!(
            cache.modes["Default"]["color-bg"],
            Color::rgb(255, 255, 255)
        );
        assert_eq!(cache.modes["Dark"]["color-bg"], Color::rgb(0, 0, 0));
        assert_eq!(cache.mode.as_deref(), Some("Dark"));
        assert_eq!(cache.tokens["space-md"], "16px");
        assert_eq!(cache.tokens["font"], "Inter");
        assert!(!cache.tokens.contains_key("flag"), "booleans are not tokens");
        assert_eq!(cache.bools["flag"], true);
        assert_eq!(
            ds.cache_for(&PageModes::new()).mode.as_deref(),
            Some("Default")
        );
    }

    fn round_trip(doc: &Document) {
        let ds = migrate_from_document(doc);
        ds.validate().unwrap();
        let c = ds.cache_for(&PageModes::new());
        assert_eq!(c.variables, doc.variables);
        assert_eq!(c.modes, doc.modes);
        assert_eq!(c.mode, doc.mode);
        assert_eq!(c.tokens, doc.tokens);
    }

    #[test]
    fn migration_round_trip() {
        let mut doc = Document::default();
        round_trip(&doc);
        doc.variables.insert("bg".into(), Color::rgb(255, 255, 255));
        doc.tokens.insert("space-md".into(), "16px".into());
        doc.tokens
            .insert("shadow-sm".into(), "0 1px 2px rgba(0,0,0,.06)".into());
        round_trip(&doc);
        doc.modes.insert(
            "Light".into(),
            BTreeMap::from([("bg".into(), Color::rgb(255, 255, 255))]),
        );
        doc.modes.insert(
            "Dark".into(),
            BTreeMap::from([("bg".into(), Color::rgb(0, 0, 0))]),
        );
        doc.mode = Some("Light".into());
        round_trip(&doc);
        doc.mode = Some("Dark".into());
        doc.variables.insert("bg".into(), Color::rgb(0, 0, 0));
        round_trip(&doc);
        let ds = migrate_from_document(&doc);
        assert_eq!(ds.collections[0].name, COLORS_COLLECTION);
        assert_eq!(ds.collections[1].name, TOKENS_COLLECTION);
        assert_eq!(ds.collections[1].variables[1].kind, Kind::Number);
    }

    #[test]
    fn group_names_with_multibyte_and_empty_segments() {
        let mut ds = DesignSystem::default();
        let c = ds.add_collection("C");
        ds.add_variable(&c, "é/ü/x", Kind::Color).unwrap();
        assert_eq!(groups(ds.collection(&c).unwrap()), ["é", "é/ü"]);
        for bad in ["a//b", "/a", "a/", "", "é//x"] {
            assert!(
                matches!(ds.add_variable(&c, bad, Kind::Color), Err(Error::InvalidName(_))),
                "{bad:?}"
            );
        }
        // A stored bad name (hand-edited design.json) must not panic the group list.
        ds.collections[0].variables[0].name = "é//ü//x".into();
        assert_eq!(groups(ds.collection(&c).unwrap()), ["é"]);
    }

    #[test]
    fn force_remove_resolves_per_dependent_mode() {
        let mut ds = DesignSystem::default();
        let c = ds.add_collection("C");
        let light = ds.collection(&c).unwrap().default_mode.clone();
        let dark = ds.add_mode(&c, "Dark").unwrap();
        let base = ds.add_variable(&c, "base", Kind::Color).unwrap();
        let user = ds.add_variable(&c, "user", Kind::Color).unwrap();
        ds.set_value(&base, &light, Value::Color(red())).unwrap();
        ds.set_value(&base, &dark, Value::Color(Color::rgb(0, 0, 255))).unwrap();
        ds.set_value(&user, &light, Value::Alias(base.clone())).unwrap();
        ds.set_value(&user, &dark, Value::Alias(base.clone())).unwrap();
        ds.remove_variable(&base, true).unwrap();
        let v = &ds.find(&user).unwrap().1.values;
        assert_eq!(v[&light], Value::Color(red()));
        assert_eq!(v[&dark], Value::Color(Color::rgb(0, 0, 255)));
    }

    #[test]
    fn migration_ids_are_deterministic_and_dedupe_color_token_clash() {
        let mut doc = Document::default();
        doc.variables.insert("brand".into(), red());
        doc.tokens.insert("brand".into(), "4px".into());
        doc.tokens.insert("space".into(), "8px".into());
        let (a, b) = (migrate_from_document(&doc), migrate_from_document(&doc));
        assert_eq!(a, b, "same project, same ids on every device");
        a.validate().unwrap();
        assert!(a.find_by_css("brand-token").is_some());
        assert_eq!(a.find_by_css("brand").unwrap().1.kind, Kind::Color);
        assert_ne!(stable_id(&["a", "bc"]), stable_id(&["ab", "c"]));
    }
}
