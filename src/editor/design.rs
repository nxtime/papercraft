//! The one write path for the project design system. Every writer (Design tab, confirm dialog,
//! MCP tools) goes through [`Editor::edit_design`] or [`Editor::set_page_mode_on`]; nothing writes
//! a page's variable cache directly any more.
//!
//! Undo: each page gets its own step holding its cache (`VarState`) and re-bound fills, and every
//! one of those steps carries the `DesignSystem` before/after. Undoing or redoing any of them
//! moves `Editor::design` to the step's other side, but only when the design is still the step's
//! starting point (nothing newer replaced it; a stale step on another page then never drops a
//! later variable edit). Either way every page's cache, fills and bindings are then rebuilt from
//! `Editor::design` without pushing history, so design and caches cannot disagree.

use super::Editor;
use crate::model::{Color, Node, NodeId, NodeKind};
use crate::variables::{
    self, COLORS_COLLECTION, DEFAULT_MODE, DesignSystem, Error, Kind, PageModes, TOKENS_COLLECTION,
    Value,
};
use std::collections::BTreeMap;

/// Fills whose color changed, per variable CSS name, summed over every page.
pub type Fills = BTreeMap<String, usize>;

fn add_fills(into: &mut Fills, from: Fills) {
    for (k, n) in from {
        *into.entry(k).or_insert(0) += n;
    }
}

impl Editor {
    /// Apply `f` to a copy of the design system, validate it, then rebuild every page's cache
    /// and bound fields, one undo step per page. Nothing changes when `f` or validation fails.
    pub fn edit_design<R>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut DesignSystem) -> Result<R, Error>,
    ) -> Result<(R, Fills), Error> {
        self.edit_design_with(label, f, None, None)
    }

    /// [`Self::edit_design`], plus `each`: run on every page inside its own step before the
    /// caches and bindings are re-applied. It runs even when the design itself does not change.
    pub fn edit_design_each<R>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut DesignSystem) -> Result<R, Error>,
        each: &dyn Fn(&mut crate::history::Tx),
    ) -> Result<(R, Fills), Error> {
        self.edit_design_with(label, f, None, Some(each))
    }

    /// [`Self::edit_design`], plus `on_active`: extra work done inside the active page's own step
    /// (a restored version replaces the page content in the same step as its variables).
    pub(super) fn edit_design_with<R>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut DesignSystem) -> Result<R, Error>,
        mut on_active: Option<OnActive<'_>>,
        pre: Option<&dyn Fn(&mut crate::history::Tx)>,
    ) -> Result<(R, Fills), Error> {
        self.commit();
        self.finish_loading();
        // A project that never went through the session (no design.json yet) starts from what
        // its first page already holds.
        if self.design.collections.is_empty() {
            let old = std::mem::take(&mut self.design);
            self.design = variables::migrate_from_document(self.page_doc(0));
            self.design.annotation_categories = old.annotation_categories;
            self.design.annotation_templates = old.annotation_templates;
            self.design.annotation_template = old.annotation_template;
        }
        let before = self.design.clone();
        let mut next = before.clone();
        let r = f(&mut next)?;
        next.validate()?;
        let active = self.active_page;
        let active_modes = on_active
            .as_ref()
            .and_then(|o| o.mode.as_deref())
            .and_then(|name| find_mode(&next, name))
            .map(|(c, m)| {
                let mut modes = self.pages[active].modes.clone();
                modes.insert(c, m);
                modes
            })
            .filter(|m| *m != self.pages[active].modes);
        // A restore always goes on: its page content (and any page whose cache lags a design
        // just built from page 0) must be applied even when the design itself did not change.
        if next == before && active_modes.is_none() && on_active.is_none() && pre.is_none() {
            return Ok((r, Fills::new()));
        }
        // Variables whose CSS name changed (rename, group rename): bound fields follow.
        let renames = css_renames(&before, &next);
        let mut fills = Fills::new();
        for i in 0..self.pages.len() {
            let old = self.pages[i].modes.clone();
            let modes = match (&active_modes, i == active) {
                (Some(m), true) => m.clone(),
                _ => old.clone(),
            };
            let cache = next.cache_for(&modes);
            let carry = Some((before.clone(), next.clone()));
            let page_modes = (modes != old).then(|| (old, modes.clone()));
            let extra = if i == active { on_active.take() } else { None };
            let n = self.with_page(i, |ed| {
                ed.rebuild_cache(
                    label,
                    &cache,
                    &renames,
                    pre,
                    extra.map(|o| o.apply),
                    |rec| {
                        rec.design = carry;
                        rec.page_modes = page_modes;
                    },
                    true,
                )
            });
            self.pages[i].modes = modes;
            add_fills(&mut fills, n);
        }
        self.design = next;
        Ok((r, fills))
    }

    /// Rebuild every page's cache, bound fills and bindings from `Editor::design` after an undo
    /// or redo, with no history of their own. The pages are marked unsaved so the files follow.
    fn rebuild_all_silently(&mut self, renames: &BTreeMap<String, String>) {
        self.finish_loading();
        for i in 0..self.pages.len() {
            let cache = self.design.cache_for(&self.pages[i].modes);
            self.with_page(i, |ed| ed.rebuild_cache("", &cache, renames, None, None, |_| {}, false));
            self.pages[i].saved_gen = u64::MAX;
        }
    }

    fn rebuild_cache(
        &mut self,
        label: &str,
        cache: &variables::Cache,
        renames: &BTreeMap<String, String>,
        pre: Option<&dyn Fn(&mut crate::history::Tx)>,
        extra: Option<Box<dyn FnOnce(&mut crate::history::Tx) + '_>>,
        record: impl FnOnce(&mut crate::history::Recording),
        keep: bool,
    ) -> Fills {
        self.begin(label);
        // Runs before the cache is applied, so what it syncs or detaches is re-bound below.
        if let Some(p) = pre {
            p(&mut self.tx());
        }
        let n = self.tx().set_cache(cache, renames);
        let (fit, mut geometry) = apply_bindings(&mut self.tx(), cache, renames);
        geometry |= pre.is_some();
        for id in fit {
            self.fit_text(id);
        }
        if let Some(x) = extra {
            x(&mut self.tx());
            geometry = true;
        }
        if let Some((rec, _)) = self.rec.as_mut() {
            record(rec);
        }
        // Bound sizes, gaps and paddings move layout; fixup re-runs it for the touched frames.
        // Colors alone never do, so existing positions stay as they are.
        if geometry {
            self.fixup();
        }
        match self.rec.take() {
            Some((rec, key)) if keep => {
                if self.log_touched(&rec) && self.history.commit(&self.doc, rec, key) {
                    self.revision += 1;
                }
            }
            // Silent rebuild (after undo/redo): the document changes, history does not.
            _ => self.revision += 1,
        }
        n
    }

    /// A copy of page `page` with its variables resolved in mode `mode` (by name), for a preview
    /// that leaves the document, its history and the canvas as they are.
    // ponytail: colours and bindings only; a mode that changes bound sizes previews them without
    // re-running layout (fit_text/fixup need the editor).
    pub fn page_doc_in_mode(&mut self, page: usize, mode: &str) -> Result<crate::model::Document, String> {
        let (cid, mid) = find_mode(&self.design, mode).ok_or(format!("no mode `{mode}`"))?;
        self.ensure_loaded(page);
        let mut modes = self.pages[page].modes.clone();
        modes.insert(cid, mid);
        let cache = self.design.cache_for(&modes);
        let mut doc = self.page_doc(page).clone();
        let mut rec = crate::history::Recording::new("Preview mode", &doc);
        let mut tx = crate::history::Tx::new(&mut doc, &mut rec);
        tx.set_cache(&cache, &BTreeMap::new());
        apply_bindings(&mut tx, &cache, &BTreeMap::new());
        Ok(doc)
    }

    /// Switch the active page to `mode_id` of `collection_id`: that page's modes, cache and bound
    /// fills only, one undo step. Returns the fills that changed.
    pub fn set_page_mode(&mut self, collection_id: &str, mode_id: &str) -> Result<usize, Error> {
        self.set_page_mode_on(self.active_page, collection_id, mode_id)
    }

    /// [`Self::set_page_mode`] for page `page` (the MCP `set_variable_mode` tool switches all).
    pub fn set_page_mode_on(
        &mut self,
        page: usize,
        collection_id: &str,
        mode_id: &str,
    ) -> Result<usize, Error> {
        let c = self
            .design
            .collection(collection_id)
            .ok_or_else(|| Error::NotFound(collection_id.into()))?;
        if !c.modes.iter().any(|m| m.id == mode_id) {
            return Err(Error::NotFound(mode_id.into()));
        }
        self.ensure_loaded(page);
        let old = self.pages[page].modes.clone();
        let mut modes = old.clone();
        modes.insert(collection_id.into(), mode_id.into());
        if modes == old {
            return Ok(0);
        }
        let cache = self.design.cache_for(&modes);
        let n = self.with_page(page, |ed| {
            ed.rebuild_cache("Set variable mode", &cache, &BTreeMap::new(), None, None, |rec| {
                rec.page_modes = Some((old, modes.clone()));
            }, true)
        });
        self.pages[page].modes = modes;
        Ok(n.values().sum())
    }

    /// Set (or clear, with `None`) page `page`'s annotation template override. One undo step.
    pub fn set_page_template_on(&mut self, page: usize, template: Option<&str>) {
        self.ensure_loaded(page);
        let old = self.pages[page].modes.clone();
        let mut modes = old.clone();
        match template {
            Some(t) => modes.insert(variables::PAGE_TEMPLATE_KEY.into(), t.into()),
            None => modes.remove(variables::PAGE_TEMPLATE_KEY),
        };
        if modes == old {
            return;
        }
        let cache = self.design.cache_for(&modes);
        self.with_page(page, |ed| {
            ed.rebuild_cache("Set annotation template", &cache, &BTreeMap::new(), None, None, |rec| {
                rec.page_modes = Some((old, modes.clone()));
            }, true)
        });
        self.pages[page].modes = modes;
    }

    /// Bind `field` of `ids` to variable `css` (`None` detaches: the current value stays, the
    /// binding goes). The value applies at once. One undo step. On an instance the field becomes
    /// an override.
    pub fn bind(&mut self, ids: &[NodeId], field: &str, css: Option<&str>) -> Result<(), String> {
        let Some(kind) = field_kind(field) else {
            return Err(format!("unknown bindable field '{field}'"));
        };
        let mut want = None;
        if let Some(css) = css {
            let (_, v) = self
                .design
                .find_by_css(css)
                .ok_or_else(|| format!("no variable '{css}'"))?;
            if v.kind != kind {
                return Err(format!(
                    "'{field}' needs a {kind:?} variable, '{css}' is {:?}",
                    v.kind
                ));
            }
            want = Some(variables::css_name(v));
        }
        for id in ids {
            let n = self.doc.get(*id).ok_or("no such layer")?;
            let ok = match field {
                "gap" | "padding" | "padding-top" | "padding-right" | "padding-bottom"
                | "padding-left" => n.layout.is_some(),
                "font-size" | "line-height" | "text-color" => n.kind.font_size().is_some(),
                _ => true,
            };
            if want.is_some() && !ok {
                return Err(format!("'{field}' does not apply to layer {}", n.name));
            }
        }
        let cache = self.design.cache_for(&self.pages[self.active_page].modes);
        self.begin("Bind variable");
        self.mark_overrides(ids, field);
        let mut tx = self.tx();
        for id in ids {
            let Some(n) = tx.node_mut(*id) else { continue };
            match &want {
                Some(w) => {
                    n.bindings.insert(field.into(), w.clone());
                    apply_field(n, field, w, &cache);
                }
                None => {
                    n.bindings.remove(field);
                }
            }
        }
        let fit: Vec<NodeId> = ids.to_vec();
        for id in fit {
            self.fit_text(id);
        }
        self.commit();
        Ok(())
    }

    /// Put back the project-level state an undone or redone step carried.
    pub(super) fn restore_project_state(&mut self, u: crate::history::Undone) {
        if let Some(m) = u.page_modes {
            self.pages[self.active_page].modes = m;
        }
        let Some(target) = u.design else { return };
        // Move the design only from the step's own starting point; a stale step (the design
        // changed again elsewhere) leaves it alone, so a later variable edit is never dropped.
        let renames = if u.design_from.as_ref() == Some(&self.design) {
            let r = css_renames(&self.design, &target);
            self.design = target;
            r
        } else {
            BTreeMap::new()
        };
        if let Some(dir) = &self.project_dir {
            self.library_docs = crate::libraries::load_enabled_docs(dir, &self.design.libraries);
        }
        self.rebuild_all_silently(&renames);
    }
}

/// Extra work for the active page's step of a design edit.
pub(super) struct OnActive<'a> {
    /// Mode name to make active on the page (looked up in the edited design).
    pub mode: Option<String>,
    pub apply: Box<dyn FnOnce(&mut crate::history::Tx) + 'a>,
}

/// Variables whose CSS name differs between two designs, old name to new, matched by id.
fn css_renames(before: &DesignSystem, next: &DesignSystem) -> BTreeMap<String, String> {
    before
        .collections
        .iter()
        .flat_map(|c| &c.variables)
        .filter_map(|v| {
            let (_, n) = next.find(&v.id)?;
            let (old, new) = (variables::css_name(v), variables::css_name(n));
            (old != new).then_some((old, new))
        })
        .collect()
}

// ------------------------------------------------------------------ field bindings

/// The variable kind a bindable field takes, or None for an unknown field.
fn field_kind(field: &str) -> Option<Kind> {
    Some(match field {
        "stroke" | "text-color" => Kind::Color,
        "width" | "height" | "gap" | "padding" | "padding-top" | "padding-right"
        | "padding-bottom" | "padding-left" | "radius" | "font-size" | "line-height"
        | "opacity" => Kind::Number,
        "visible" => Kind::Boolean,
        _ => return None,
    })
}

/// Number variables below this bind as a line-height multiplier, from here up as pixels.
const LINE_HEIGHT_PX_FROM: f64 = 4.0;

/// `16px` -> 16 (also a bare number, as opacity and line height are not lengths).
fn token_number(cache: &variables::Cache, name: &str) -> Option<f64> {
    let t = cache.tokens.get(name)?.trim();
    t.strip_suffix("px").unwrap_or(t).trim().parse().ok()
}

/// Set `field` of `n` from variable `name` in `cache`. An unresolved variable leaves the value.
fn apply_field(n: &mut Node, field: &str, name: &str, cache: &variables::Cache) {
    match field {
        "stroke" => {
            if let Some(c) = cache.variables.get(name) {
                n.style.stroke = Some(*c);
            }
        }
        "text-color" => {
            if let Some(c) = cache.variables.get(name) {
                n.style.set_fill(Some(*c));
                for sp in &mut n.text_style.spans {
                    sp.color = None;
                }
            }
        }
        "visible" => {
            if let Some(b) = cache.bools.get(name) {
                n.visible = *b;
            }
        }
        _ => {
            let Some(v) = token_number(cache, name) else {
                return;
            };
            match field {
                "width" => n.rect.w = v.max(0.0),
                "height" => n.rect.h = v.max(0.0),
                "gap" => {
                    if let Some(l) = &mut n.layout {
                        l.gap = v;
                    }
                }
                "padding" => {
                    if let Some(l) = &mut n.layout {
                        l.padding = [v; 4];
                    }
                }
                "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
                    let i = ["top", "right", "bottom", "left"]
                        .iter()
                        .position(|s| field.ends_with(s))
                        .unwrap();
                    if let Some(l) = &mut n.layout {
                        l.padding[i] = v;
                    }
                }
                "radius" => {
                    n.style.corner_radius = v;
                    n.style.radii = None;
                }
                "font-size" => {
                    if let NodeKind::Text { font_size, .. } | NodeKind::Sticky { font_size, .. } =
                        &mut n.kind
                    {
                        *font_size = v;
                    }
                }
                // A multiplier of the font size below LINE_HEIGHT_PX_FROM (1.5), else pixels (24).
                "line-height" => {
                    if let Some(size) = n.kind.font_size().filter(|s| *s > 0.0) {
                        n.text_style.line_height =
                            Some(if v < LINE_HEIGHT_PX_FROM { v } else { v / size });
                    }
                }
                "opacity" => n.style.opacity = v.clamp(0.0, 1.0),
                _ => {}
            }
        }
    }
}

/// Re-apply every node's bindings from `cache`, after `renames` (old CSS name to new) moved
/// them. Returns text layers whose size may have changed, and whether any size, layout or
/// visibility changed.
fn apply_bindings(
    tx: &mut crate::history::Tx,
    cache: &variables::Cache,
    renames: &BTreeMap<String, String>,
) -> (Vec<NodeId>, bool) {
    let ids: Vec<NodeId> = tx
        .doc()
        .nodes
        .values()
        .filter(|n| !n.bindings.is_empty())
        .map(|n| n.id)
        .collect();
    let mut fit = Vec::new();
    let mut geometry = false;
    for id in ids {
        let orig = tx.doc().get(id).unwrap();
        let mut n = orig.clone();
        for (field, name) in n.bindings.clone() {
            let name = renames.get(&name).cloned().unwrap_or(name);
            apply_field(&mut n, &field, &name, cache);
            n.bindings.insert(field, name);
        }
        if n != *orig {
            geometry |= n.rect != orig.rect
                || n.layout != orig.layout
                || n.visible != orig.visible
                || n.kind != orig.kind
                || n.text_style != orig.text_style;
            if n.kind.font_size().is_some() {
                fit.push(id);
            }
            *tx.node_mut(id).unwrap() = n;
        }
    }
    (fit, geometry)
}

// ------------------------------------------------------------------ by-name helpers
// The Design tab and the MCP tools speak CSS names and mode names; these map them onto the model.

fn is_local_colors(c: &variables::Collection) -> bool {
    c.library.is_none() && c.variables.iter().any(|v| v.kind == Kind::Color)
}

/// Where color variables land: the first local collection that has some, else the local one named
/// `Colors`, else a new `Colors`.
pub fn color_collection(ds: &mut DesignSystem) -> String {
    let found = ds
        .collections
        .iter()
        .find(|c| is_local_colors(c))
        .or_else(|| {
            ds.collections
                .iter()
                .find(|c| c.library.is_none() && c.name == COLORS_COLLECTION)
        });
    match found {
        Some(c) => c.id.clone(),
        None => ds.add_collection(COLORS_COLLECTION),
    }
}

fn tokens_collection(ds: &mut DesignSystem) -> String {
    let found = ds
        .collections
        .iter()
        .find(|c| c.library.is_none() && c.name == TOKENS_COLLECTION);
    match found {
        Some(c) => c.id.clone(),
        None => ds.add_collection(TOKENS_COLLECTION),
    }
}

/// The id of mode `name` in the collection, creating it. A collection that still has only its
/// unnamed `Default` mode adopts the name, so the first mode defined is the active one.
fn mode_id(ds: &mut DesignSystem, cid: &str, name: &str) -> Result<String, Error> {
    let c = ds
        .collection(cid)
        .ok_or_else(|| Error::NotFound(cid.into()))?;
    if let Some(m) = c.modes.iter().find(|m| m.name == name) {
        return Ok(m.id.clone());
    }
    if c.modes.len() == 1 && c.modes[0].name == DEFAULT_MODE {
        let id = c.modes[0].id.clone();
        if let Some(c) = ds.collections.iter_mut().find(|c| c.id == cid) {
            c.modes[0].name = name.into();
        }
        return Ok(id);
    }
    ds.add_mode(cid, name)
}

/// `(collection id, mode id)` for a mode name; collections with colors win.
pub fn find_mode(ds: &DesignSystem, name: &str) -> Option<(String, String)> {
    let by_name = |c: &&variables::Collection| c.modes.iter().any(|m| m.name == name);
    let c = ds
        .collections
        .iter()
        .filter(by_name)
        .find(|c| is_local_colors(c))
        .or_else(|| ds.collections.iter().find(by_name))?;
    let m = c.modes.iter().find(|m| m.name == name)?;
    Some((c.id.clone(), m.id.clone()))
}

/// Define or change a color variable by CSS name. `mode` names the mode to write (created when
/// missing); without it the value goes to the mode `active` (the page's modes) has selected. A new
/// variable starts with `color` in every mode.
pub fn set_color(
    ds: &mut DesignSystem,
    active: &PageModes,
    name: &str,
    mode: Option<&str>,
    color: Color,
) -> Result<(), Error> {
    let (cid, vid, fresh) = match ds.find_by_css(name) {
        Some((c, v)) if v.kind == Kind::Color => (c.id.clone(), v.id.clone(), false),
        Some(_) => return Err(Error::KindMismatch(name.into())),
        None => {
            let cid = color_collection(ds);
            let vid = ds.add_variable(&cid, name, Kind::Color)?;
            (cid, vid, true)
        }
    };
    let mid = match mode {
        Some(m) => mode_id(ds, &cid, m)?,
        None => {
            let c = ds.collection(&cid).expect("collection exists");
            active
                .get(&cid)
                .filter(|m| c.modes.iter().any(|x| &x.id == *m))
                .unwrap_or(&c.default_mode)
                .clone()
        }
    };
    if fresh {
        let modes: Vec<String> = ds
            .collection(&cid)
            .unwrap()
            .modes
            .iter()
            .map(|m| m.id.clone())
            .collect();
        for m in modes {
            ds.set_value(&vid, &m, Value::Color(color))?;
        }
    }
    ds.set_value(&vid, &mid, Value::Color(color))
}

/// Define or change a non-color token by CSS name: `16px` is a Number, anything else a String.
/// It lands in the variable's own collection, or in `Tokens` (created) when new. An existing
/// variable changes in its default mode only; see [`set_token_in`].
pub fn set_token(ds: &mut DesignSystem, name: &str, value: &str) -> Result<(), Error> {
    set_token_in(ds, &PageModes::new(), name, value, None)
}

/// [`set_token`] for a named `mode` or, without one, the mode `active` selects. A new variable (or
/// one whose kind changes) gets the value in every mode; an existing one keeps the values of its
/// other modes.
pub fn set_token_in(
    ds: &mut DesignSystem,
    active: &PageModes,
    name: &str,
    value: &str,
    mode: Option<&str>,
) -> Result<(), Error> {
    let (kind, v) = match value
        .trim()
        .strip_suffix("px")
        .and_then(|n| n.trim().parse::<f64>().ok())
    {
        Some(n) => (Kind::Number, Value::Number(n)),
        None => (Kind::String, Value::String(value.into())),
    };
    let mut cid = None;
    if let Some((c, old)) = ds.find_by_css(name) {
        if old.kind == Kind::Color {
            return Err(Error::KindMismatch(name.into()));
        }
        cid = Some(c.id.clone());
        if old.kind != kind {
            let id = old.id.clone();
            ds.remove_variable(&id, true)?;
        }
    }
    let cid = cid.unwrap_or_else(|| tokens_collection(ds));
    let (vid, fresh) = match ds.find_by_css(name) {
        Some((_, old)) => (old.id.clone(), false),
        None => (ds.add_variable(&cid, name, kind)?, true),
    };
    if !fresh {
        let c = ds.collection(&cid).expect("collection exists");
        let mid = match mode {
            Some(m) => mode_id(ds, &cid, m)?,
            None => active
                .get(&cid)
                .filter(|m| c.modes.iter().any(|x| &x.id == *m))
                .unwrap_or(&c.default_mode)
                .clone(),
        };
        return ds.set_value(&vid, &mid, v);
    }
    let modes: Vec<String> = ds
        .collection(&cid)
        .unwrap()
        .modes
        .iter()
        .map(|m| m.id.clone())
        .collect();
    for m in modes {
        ds.set_value(&vid, &m, v.clone())?;
    }
    Ok(())
}

/// Delete a variable or token by CSS name. Aliases to it keep the resolved value.
pub fn remove_by_css(ds: &mut DesignSystem, name: &str) -> Result<(), Error> {
    let id = ds
        .find_by_css(name)
        .ok_or_else(|| Error::NotFound(name.into()))?
        .1
        .id
        .clone();
    ds.remove_variable(&id, true).map(|_| ())
}

pub fn rename_by_css(ds: &mut DesignSystem, old: &str, new: &str) -> Result<(), Error> {
    let id = ds
        .find_by_css(old)
        .ok_or_else(|| Error::NotFound(old.into()))?
        .1
        .id
        .clone();
    ds.rename_variable(&id, new)
}

/// Remove the mode called `name` from the collection that holds it.
pub fn remove_mode_by_name(ds: &mut DesignSystem, name: &str) -> Result<(), Error> {
    let (cid, mid) = find_mode(ds, name).ok_or_else(|| Error::NotFound(name.into()))?;
    ds.remove_mode(&cid, &mid)
}

/// Add a color mode called `name`, seeded with `values` (CSS name to color; the rest copies the
/// default mode).
pub fn add_color_mode(
    ds: &mut DesignSystem,
    name: &str,
    values: &BTreeMap<String, Color>,
) -> Result<(), Error> {
    let cid = color_collection(ds);
    let mid = ds.add_mode(&cid, name)?;
    for (css, color) in values {
        let Some((c, v)) = ds.find_by_css(css) else {
            continue;
        };
        if c.id == cid && v.kind == Kind::Color {
            let vid = v.id.clone();
            ds.set_value(&vid, &mid, Value::Color(*color))?;
        }
    }
    Ok(())
}

/// Replace the local `Colors` / `Tokens` collections the package defines (design-system package
/// import): `variables`, per-mode colors and `tokens`. Local collections the package does not
/// define stay, and so do library collections. Aliases from the kept ones to a variable that
/// goes away keep its resolved value instead of dangling.
pub fn replace_system(
    ds: &mut DesignSystem,
    variables: &BTreeMap<String, Color>,
    modes: &BTreeMap<String, BTreeMap<String, Color>>,
    tokens: &BTreeMap<String, String>,
) {
    let mut tmp = crate::model::Document {
        variables: variables.clone(),
        modes: modes.clone(),
        tokens: tokens.clone(),
        ..Default::default()
    };
    // The package's `:root` colors are the base theme: with modes they become the active
    // `Default` mode, as before when no mode was live.
    if !modes.is_empty() {
        tmp.modes
            .entry(DEFAULT_MODE.into())
            .or_insert_with(|| variables.clone());
        tmp.mode = Some(DEFAULT_MODE.into());
    }
    let fresh = variables::migrate_from_document(&tmp);
    let replaced = |c: &variables::Collection| {
        c.library.is_none() && fresh.collections.iter().any(|f| f.name == c.name)
    };
    // Variables with the same id and kind come back from the package, so aliases to them hold.
    let gone: Vec<String> = ds
        .collections
        .iter()
        .filter(|c| replaced(c))
        .flat_map(|c| &c.variables)
        .filter(|v| !fresh.find(&v.id).is_some_and(|(_, f)| f.kind == v.kind))
        .map(|v| v.id.clone())
        .collect();
    for id in gone {
        let _ = ds.remove_variable(&id, true);
    }
    ds.collections.retain(|c| !replaced(c));
    ds.collections.splice(0..0, fresh.collections);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Node, NodeId, NodeKind, Paint};

    /// Two pages, each with a rect following `--accent`; "Light"/"Dark" modes. Page 0 is active.
    fn two_pages() -> (Editor, [NodeId; 2]) {
        let mut e = Editor::default();
        e.add_page_in_background(None);
        let mut ids = [NodeId(0); 2];
        for (page, slot) in ids.iter_mut().enumerate() {
            e.with_page(page, |e| {
                let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0., 0., 10., 10.));
                n.style.fills = vec![Paint::solid_var(Color::BLACK, "accent")];
                *slot = e.add_node(n, None);
            });
        }
        let none = PageModes::new();
        e.edit_design("Setup", |ds| {
            set_color(ds, &none, "accent", Some("Light"), Color::rgb(255, 0, 0))?;
            set_color(ds, &none, "accent", Some("Dark"), Color::rgb(0, 0, 255))
        })
        .unwrap();
        (e, ids)
    }

    fn fill(e: &Editor, page: usize, id: NodeId) -> (Color, Option<String>) {
        match e.page_doc(page).get(id).unwrap().style.fills[0].clone() {
            Paint::Solid { color, var } => (color, var),
            _ => unreachable!(),
        }
    }

    #[test]
    fn color_change_updates_every_page_cache_and_bound_fill() {
        let (mut e, ids) = two_pages();
        for page in 0..2 {
            assert_eq!(fill(&e, page, ids[page]).0, Color::rgb(255, 0, 0));
        }
        let green = Color::rgb(0, 255, 0);
        let (_, fills) = e
            .edit_design("Change", |ds| {
                set_color(ds, &PageModes::new(), "accent", None, green)
            })
            .unwrap();
        assert_eq!(fills["accent"], 2);
        for page in 0..2 {
            assert_eq!(e.page_doc(page).variables["accent"], green);
            assert_eq!(e.page_doc(page).modes["Light"]["accent"], green);
            assert_eq!(fill(&e, page, ids[page]), (green, Some("accent".into())));
        }
    }

    #[test]
    fn removing_a_variable_affects_every_page() {
        let (mut e, ids) = two_pages();
        e.edit_design("Remove", |ds| remove_by_css(ds, "accent"))
            .unwrap();
        for page in 0..2 {
            assert!(!e.page_doc(page).variables.contains_key("accent"));
            // The fill keeps its color but stops following.
            assert_eq!(fill(&e, page, ids[page]), (Color::rgb(255, 0, 0), None));
        }
    }

    #[test]
    fn rename_moves_bound_fills_on_every_page() {
        let (mut e, ids) = two_pages();
        e.edit_design("Rename", |ds| rename_by_css(ds, "accent", "brand"))
            .unwrap();
        for page in 0..2 {
            assert_eq!(fill(&e, page, ids[page]).1.as_deref(), Some("brand"));
        }
    }

    #[test]
    fn mode_switch_touches_only_its_page() {
        let (mut e, ids) = two_pages();
        let (cid, dark) = find_mode(&e.design, "Dark").unwrap();
        let before = e.page_doc(1).clone();
        assert_eq!(e.set_page_mode(&cid, &dark).unwrap(), 1);
        assert_eq!(e.page_doc(0).mode.as_deref(), Some("Dark"));
        assert_eq!(fill(&e, 0, ids[0]).0, Color::rgb(0, 0, 255));
        assert_eq!(*e.page_doc(1), before, "the other page is untouched");
        assert!(e.pages[1].modes.is_empty());
        // Undo restores the page's modes and cache.
        e.undo();
        assert!(e.pages[0].modes.is_empty());
        assert_eq!(e.doc.mode.as_deref(), Some("Light"));
        assert_eq!(fill(&e, 0, ids[0]).0, Color::rgb(255, 0, 0));
    }

    #[test]
    fn undo_restores_design_and_caches() {
        let (mut e, ids) = two_pages();
        let design = e.design.clone();
        let blue = Color::rgb(1, 2, 3);
        e.edit_design("Change", |ds| {
            set_color(ds, &PageModes::new(), "accent", None, blue)
        })
        .unwrap();
        e.edit_design("Add", |ds| {
            set_color(ds, &PageModes::new(), "extra", None, blue)
        })
        .unwrap();
        e.undo();
        e.undo();
        assert_eq!(e.design, design);
        assert_eq!(e.doc.variables["accent"], Color::rgb(255, 0, 0));
        assert!(!e.doc.variables.contains_key("extra"));
        assert_eq!(fill(&e, 0, ids[0]).0, Color::rgb(255, 0, 0));
        // The other page's own step undoes its cache.
        e.switch_page(1);
        e.undo();
        e.undo();
        assert_eq!(fill(&e, 1, ids[1]).0, Color::rgb(255, 0, 0));
        assert_eq!(e.doc.variables["accent"], Color::rgb(255, 0, 0));
        e.switch_page(0);
        e.redo();
        assert_eq!(e.design.find_by_css("accent").map(|_| ()), Some(()));
        assert_eq!(e.doc.variables["accent"], blue);
    }

    #[test]
    fn restored_variable_values_survive_the_next_design_edit() {
        let (mut e, _) = two_pages();
        let snap = e.doc.clone();
        let blue = Color::rgb(1, 2, 3);
        e.edit_design("Change", |ds| set_color(ds, &PageModes::new(), "accent", None, blue))
            .unwrap();
        e.restore_snapshot(&snap);
        assert_eq!(accent(&e, 0), Color::rgb(255, 0, 0));
        assert_eq!(accent(&e, 1), Color::rgb(255, 0, 0));
        e.edit_design("Other", |ds| set_color(ds, &PageModes::new(), "extra", None, blue))
            .unwrap();
        assert_eq!(accent(&e, 0), Color::rgb(255, 0, 0), "the design holds the restored value");
        e.undo(); // the extra edit
        e.undo(); // the restore: back to the changed value, in one step
        assert_eq!(accent(&e, 0), blue);
    }

    #[test]
    fn duplicated_page_mains_get_fresh_component_keys() {
        let (mut e, ids) = two_pages();
        e.selection = vec![ids[0]];
        e.create_component();
        let main = e.selection[0];
        let key = e.doc.get(main).unwrap().component_key.clone();
        assert!(key.is_some());
        e.duplicate_page(0);
        let copy = e.doc.nodes.values().find(|n| n.component).unwrap();
        assert!(copy.component_key.is_some());
        assert_ne!(copy.component_key, key);
    }

    fn accent(e: &Editor, page: usize) -> Color {
        e.page_doc(page).variables["accent"]
    }

    #[test]
    fn undo_on_another_page_restores_design_and_every_cache() {
        let (mut e, ids) = two_pages();
        let design = e.design.clone();
        let blue = Color::rgb(1, 2, 3);
        e.edit_design("Change", |ds| set_color(ds, &PageModes::new(), "accent", None, blue))
            .unwrap();
        e.switch_page(1);
        e.undo();
        assert_eq!(e.design, design, "any page's step moves the design back");
        for page in 0..2 {
            assert_eq!(accent(&e, page), Color::rgb(255, 0, 0));
            assert_eq!(fill(&e, page, ids[page]).0, Color::rgb(255, 0, 0));
        }
        e.redo();
        assert_eq!(e.design.find_by_css("accent").map(|_| ()), Some(()));
        for page in 0..2 {
            assert_eq!(accent(&e, page), blue);
            assert_eq!(fill(&e, page, ids[page]).0, blue);
        }
    }

    #[test]
    fn undo_never_drops_a_later_variable_edit() {
        let (mut e, ids) = two_pages();
        let blue = Color::rgb(1, 2, 3);
        e.edit_design("A", |ds| set_color(ds, &PageModes::new(), "accent", None, blue))
            .unwrap();
        e.switch_page(1);
        e.edit_design("B", |ds| set_color(ds, &PageModes::new(), "extra", None, blue))
            .unwrap();
        // Undo on A first meets B's step (the later edit), so A's own edit survives it.
        e.switch_page(0);
        e.undo();
        assert!(e.design.find_by_css("extra").is_none());
        assert_eq!(accent(&e, 1), blue);
        assert_eq!(fill(&e, 1, ids[1]).0, blue);
        e.undo();
        assert_eq!(accent(&e, 1), Color::rgb(255, 0, 0));

        // A step made stale by undoing elsewhere changes no design: the later edit stays.
        e.redo();
        e.redo();
        e.switch_page(1);
        e.undo();
        e.undo();
        let after_b = e.design.clone();
        e.edit_design("C", |ds| set_color(ds, &PageModes::new(), "late", None, blue))
            .unwrap();
        e.switch_page(0);
        e.undo(); // C's step on A: design == C's result, undone
        assert_eq!(e.design, after_b);
        e.undo(); // B's stale step on A: design is not its start; nothing is dropped
        assert!(e.design.find_by_css("late").is_none());
        for page in 0..2 {
            assert_eq!(accent(&e, page), e.design.cache_for(&e.pages[page].modes).variables["accent"]);
        }
    }

    #[test]
    fn set_token_writes_one_mode_when_the_variable_exists() {
        let (mut e, _) = gap_pages();
        e.edit_design("Num", |ds| {
            let (c, v) = ds.find_by_css("space-md").map(|(c, v)| (c.id.clone(), v.id.clone())).unwrap();
            let m = ds.add_mode(&c, "Dense")?;
            ds.set_value(&v, &m, Value::Number(8.0))
        })
        .unwrap();
        e.edit_design("Set", |ds| set_token(ds, "space-md", "20px")).unwrap();
        let (c, v) = e.design.find_by_css("space-md").unwrap();
        let dense = c.modes.iter().find(|m| m.name == "Dense").unwrap().id.clone();
        assert_eq!(v.values[&dense], Value::Number(8.0), "other modes keep their value");
        assert_eq!(v.values[&c.default_mode], Value::Number(20.0));
        // Named mode.
        let mut ds = e.design.clone();
        set_token_in(&mut ds, &PageModes::new(), "space-md", "9px", Some("Dense")).unwrap();
        assert_eq!(ds.find_by_css("space-md").unwrap().1.values[&dense], Value::Number(9.0));
        // A new token still lands in every mode.
        let mut ds = e.design.clone();
        set_token(&mut ds, "fresh", "1px").unwrap();
        assert_eq!(ds.find_by_css("fresh").unwrap().1.values.len(), 2);
    }

    #[test]
    fn replace_system_keeps_other_collections_and_detaches_dangling_aliases() {
        let mut ds = DesignSystem::default();
        let colors = ds.add_collection(COLORS_COLLECTION);
        let bg = ds.add_variable(&colors, "bg", Kind::Color).unwrap();
        let mine = ds.add_collection("Brand");
        let x = ds.add_variable(&mine, "brand-x", Kind::Color).unwrap();
        let (mc, cm) = (
            ds.collection(&mine).unwrap().default_mode.clone(),
            ds.collection(&colors).unwrap().default_mode.clone(),
        );
        ds.set_value(&bg, &cm, Value::Color(Color::rgb(9, 9, 9))).unwrap();
        ds.set_value(&x, &mc, Value::Alias(bg)).unwrap();
        let pkg = BTreeMap::from([("other".to_string(), Color::rgb(1, 1, 1))]);
        replace_system(&mut ds, &pkg, &BTreeMap::new(), &BTreeMap::new());
        ds.validate().unwrap();
        assert!(ds.collections.iter().any(|c| c.name == "Brand"), "untouched collection stays");
        assert!(ds.find_by_css("bg").is_none());
        assert!(ds.find_by_css("other").is_some());
        assert_eq!(
            ds.resolve(&x, &PageModes::new()).unwrap(),
            Value::Color(Color::rgb(9, 9, 9)),
            "the alias keeps the resolved value"
        );
    }

    #[test]
    fn opacity_clamps_line_height_multiplier_or_pixels_and_booleans_stay_out_of_tokens() {
        let mut n = Node::new(
            NodeId(1),
            NodeKind::Text { text: "t".into(), font_size: 20.0, auto_width: true, align: Default::default() },
            Rect::new(0., 0., 10., 10.),
        );
        let mut cache = variables::Cache::default();
        for (k, v) in [("half", "0.5px"), ("big", "7px"), ("mult", "1.5px"), ("px", "30px")] {
            cache.tokens.insert(k.into(), v.into());
        }
        apply_field(&mut n, "opacity", "half", &cache);
        assert_eq!(n.style.opacity, 0.5);
        apply_field(&mut n, "opacity", "big", &cache);
        assert_eq!(n.style.opacity, 1.0);
        apply_field(&mut n, "line-height", "mult", &cache);
        assert_eq!(n.text_style.line_height, Some(1.5));
        apply_field(&mut n, "line-height", "px", &cache);
        assert_eq!(n.text_style.line_height, Some(1.5), "30px / 20px font size");
        let (mut e, ids) = two_pages();
        e.edit_design("Flag", |ds| {
            let c = color_collection(ds);
            let v = ds.add_variable(&c, "show", Kind::Boolean)?;
            let m = ds.collection(&c).unwrap().default_mode.clone();
            ds.set_value(&v, &m, Value::Boolean(false))
        })
        .unwrap();
        assert!(!e.doc.tokens.contains_key("show"));
        e.bind(&[ids[0]], "visible", Some("show")).unwrap();
        assert!(!e.doc.get(ids[0]).unwrap().visible);
    }

    /// Per page: a horizontal frame (gap 10) with two 10x10 rects; the second rect's x shows
    /// the gap. Tokens `space-md` = 16px.
    fn gap_pages() -> (Editor, [NodeId; 2]) {
        let (mut e, _) = two_pages();
        let mut frames = [NodeId(0); 2];
        for (page, slot) in frames.iter_mut().enumerate() {
            e.with_page(page, |e| {
                let mut f = Node::new(NodeId(0), NodeKind::Frame { clip: false }, Rect::new(0., 0., 100., 20.));
                f.layout = Some(crate::model::AutoLayout {
                    gap: 10.0,
                    padding: [0.0; 4],
                    ..Default::default()
                });
                let f = e.add_node(f, None);
                for _ in 0..2 {
                    e.add_node(Node::new(NodeId(0), NodeKind::Rect, Rect::new(0., 0., 10., 10.)), Some(f));
                }
                *slot = f;
            });
        }
        e.edit_design("Tokens", |ds| set_token(ds, "space-md", "16px")).unwrap();
        (e, frames)
    }

    fn second_x(e: &Editor, page: usize, f: NodeId) -> f64 {
        let k = e.page_doc(page).get(f).unwrap().children[1];
        e.page_doc(page).get(k).unwrap().rect.x
    }

    fn gap_of(e: &Editor, page: usize, f: NodeId) -> f64 {
        e.page_doc(page).get(f).unwrap().layout.unwrap().gap
    }

    #[test]
    fn bound_gap_follows_token_on_every_page_and_relayouts() {
        let (mut e, f) = gap_pages();
        e.selection = vec![f[0]];
        e.bind(&[f[0]], "gap", Some("space-md")).unwrap();
        e.switch_page(1);
        e.bind(&[f[1]], "gap", Some("space-md")).unwrap();
        e.switch_page(0);
        for p in 0..2 {
            assert_eq!(gap_of(&e, p, f[p]), 16.0);
            assert_eq!(second_x(&e, p, f[p]), 26.0);
        }
        e.edit_design("Change", |ds| set_token(ds, "space-md", "24px")).unwrap();
        for p in 0..2 {
            assert_eq!(gap_of(&e, p, f[p]), 24.0);
            assert_eq!(second_x(&e, p, f[p]), 34.0);
        }
    }

    #[test]
    fn mode_switch_updates_only_its_pages_bound_fields() {
        let (mut e, f) = gap_pages();
        e.edit_design("Num", |ds| {
            set_token(ds, "alpha", "1px")?;
            let (c, v) = ds.find_by_css("alpha").map(|(c, v)| (c.id.clone(), v.id.clone())).unwrap();
            let m = ds.add_mode(&c, "Dim")?;
            ds.set_value(&v, &m, Value::Number(0.5))
        })
        .unwrap();
        e.bind(&[f[0]], "opacity", Some("alpha")).unwrap();
        e.switch_page(1);
        e.bind(&[f[1]], "opacity", Some("alpha")).unwrap();
        e.switch_page(0);
        let (cid, dim) = find_mode(&e.design, "Dim").unwrap();
        e.set_page_mode(&cid, &dim).unwrap();
        assert_eq!(e.page_doc(0).get(f[0]).unwrap().style.opacity, 0.5);
        assert_eq!(e.page_doc(1).get(f[1]).unwrap().style.opacity, 1.0);
    }

    #[test]
    fn detach_keeps_value_and_kind_mismatch_is_refused() {
        let (mut e, f) = gap_pages();
        e.bind(&[f[0]], "gap", Some("space-md")).unwrap();
        e.bind(&[f[0]], "gap", None).unwrap();
        assert!(e.doc.get(f[0]).unwrap().bindings.is_empty());
        e.edit_design("Change", |ds| set_token(ds, "space-md", "24px")).unwrap();
        assert_eq!(gap_of(&e, 0, f[0]), 16.0);
        assert!(e.bind(&[f[0]], "gap", Some("accent")).unwrap_err().contains("Number"));
        assert!(e.bind(&[f[0]], "stroke", Some("space-md")).is_err());
        assert!(e.bind(&[f[0]], "visible", Some("space-md")).is_err());
        assert!(e.doc.get(f[0]).unwrap().bindings.is_empty());
    }

    #[test]
    fn color_and_visible_fields_bind() {
        let (mut e, ids) = two_pages();
        e.bind(&[ids[0]], "stroke", Some("accent")).unwrap();
        assert_eq!(e.doc.get(ids[0]).unwrap().style.stroke, Some(Color::rgb(255, 0, 0)));
        e.edit_design("Flag", |ds| {
            let c = color_collection(ds);
            let v = ds.add_variable(&c, "show", Kind::Boolean)?;
            let m = ds.collection(&c).unwrap().default_mode.clone();
            ds.set_value(&v, &m, Value::Boolean(false))
        })
        .unwrap();
        e.bind(&[ids[0]], "visible", Some("show")).unwrap();
        assert!(!e.doc.get(ids[0]).unwrap().visible);
        e.undo();
        assert!(e.doc.get(ids[0]).unwrap().visible);
    }

    #[test]
    fn instance_inherits_binding_and_override_drops_it() {
        let (mut e, f) = gap_pages();
        e.selection = vec![f[0]];
        e.create_component();
        e.bind(&[f[0]], "gap", Some("space-md")).unwrap();
        let i = e.create_instance(f[0], crate::geom::v2(300.0, 0.0)).unwrap();
        assert_eq!(e.doc.get(i).unwrap().bindings["gap"], "space-md");
        e.edit_design("Change", |ds| set_token(ds, "space-md", "24px")).unwrap();
        assert_eq!(e.doc.get(i).unwrap().layout.unwrap().gap, 24.0);
        e.bind(&[i], "gap", Some("space-md")).unwrap();
        e.mark_overrides(&[i], "gap");
        e.commit();
        assert!(e.doc.get(i).unwrap().bindings.is_empty());
        assert_eq!(e.doc.get(f[0]).unwrap().bindings["gap"], "space-md");
    }

    #[test]
    fn rename_moves_bindings() {
        let (mut e, f) = gap_pages();
        e.bind(&[f[0]], "gap", Some("space-md")).unwrap();
        e.edit_design("Rename", |ds| rename_by_css(ds, "space-md", "space-m")).unwrap();
        assert_eq!(e.doc.get(f[0]).unwrap().bindings["gap"], "space-m");
        e.edit_design("Change", |ds| set_token(ds, "space-m", "30px")).unwrap();
        assert_eq!(gap_of(&e, 0, f[0]), 30.0);
    }

    #[test]
    fn refused_edit_changes_nothing() {
        let (mut e, _) = two_pages();
        let design = e.design.clone();
        let r = e.edit_design("Bad", |ds| set_token(ds, "accent", "4px"));
        assert!(r.is_err());
        assert_eq!(e.design, design);
    }
}
