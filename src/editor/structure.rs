//! Structural editing: groups, components and instances, auto layout, alignment and the
//! post-edit fixup pass that keeps derived state (group bounds, layout, instances) in sync.

use super::Editor;
use crate::geom::{Rect, V2, v2};
use crate::history::Tx;
use crate::layout;
use crate::model::{
    AutoLayout, Direction, Document, InstanceLink, Node, NodeId, NodeKind, Sizing, Source, Swap,
};
use std::collections::{BTreeSet, HashSet};

/// Property groups an instance can override (and that sync copies from the main component).
pub const OVERRIDE_KEYS: [&str; 11] = [
    "fills", "stroke", "radius", "opacity", "text", "font", "visible", "name", "size", "position",
    "layout",
];

/// Map an inspector field name to the override key it changes.
pub fn override_key(field: &str) -> &'static str {
    match field {
        "x" | "y" | "position" => "position",
        "width" | "height" | "size" => "size",
        "fill" | "fills" | "color" | "text-color" => "fills",
        "stroke" | "stroke width" | "stroke align" | "dash" | "cap" | "join" => "stroke",
        "radius" | "radii" => "radius",
        "opacity" | "blend" => "opacity",
        "text" => "text",
        "font size" | "font" | "bold" | "italic" | "align" | "line height" | "letter spacing"
        | "font-size" | "line-height" => "font",
        "visible" => "visible",
        "name" => "name",
        _ => "layout",
    }
}

/// Copy the properties of `main` onto `inst` except overridden ones.
fn copy_props(main: &Node, inst: &mut Node, overrides: &[String], is_root: bool) {
    let ov = |k: &str| overrides.iter().any(|o| o == k);
    let s = &main.style;
    if !ov("fills") {
        inst.style.fills = s.fills.clone();
    }
    if !ov("stroke") {
        inst.style.stroke = s.stroke;
        inst.style.stroke_width = s.stroke_width;
        inst.style.stroke_align = s.stroke_align;
        inst.style.dash = s.dash.clone();
        inst.style.cap = s.cap;
        inst.style.join = s.join;
    }
    if !ov("radius") {
        inst.style.corner_radius = s.corner_radius;
        inst.style.radii = s.radii;
    }
    if !ov("opacity") {
        inst.style.opacity = s.opacity;
        inst.style.blend = s.blend;
        inst.style.shadow = s.shadow;
    }
    inst.style.rough = s.rough;
    match (&main.kind, &mut inst.kind) {
        (
            NodeKind::Text {
                text: mt,
                font_size: mf,
                auto_width: ma,
                align: mal,
            },
            NodeKind::Text {
                text,
                font_size,
                auto_width,
                align,
            },
        ) => {
            if !ov("text") {
                *text = mt.clone();
            }
            if !ov("font") {
                *font_size = *mf;
                *auto_width = *ma;
                *align = *mal;
            }
        }
        (mk, ik) if !matches!(mk, NodeKind::Text { .. }) => *ik = mk.clone(),
        _ => {}
    }
    if !ov("font") {
        inst.text_style = main.text_style.clone();
    }
    if !ov("visible") {
        inst.visible = main.visible;
    }
    if !ov("name") && !is_root {
        inst.name = main.name.clone();
    }
    if !ov("layout") {
        inst.layout = main.layout;
        // How a root sits in its own parent (absolute, fill/hug) is the instance's, not the main's.
        if !is_root {
            inst.layout_child = main.layout_child;
        }
    }
    if !is_root && !ov("position") {
        inst.rect.x = main.rect.x;
        inst.rect.y = main.rect.y;
    }
    if !ov("size") {
        inst.rect.w = main.rect.w;
        inst.rect.h = main.rect.h;
        // Its own text gets its own box: the main's was measured for the main's words.
        if ov("text")
            && let Some(l) = crate::text::layout_node(inst)
        {
            if matches!(inst.kind, NodeKind::Text { auto_width: true, .. }) {
                inst.rect.w = l.width.max(1.0);
            }
            inst.rect.h = l.height;
        }
    }
    // Bindings follow the main, field by field, except in overridden groups.
    let fields: BTreeSet<String> = main.bindings.keys().chain(inst.bindings.keys()).cloned().collect();
    for f in fields {
        if ov(override_key(&f)) {
            continue;
        }
        match main.bindings.get(&f) {
            Some(v) => inst.bindings.insert(f, v.clone()),
            None => inst.bindings.remove(&f),
        };
    }
}

/// Copy a subtree as a new instance subtree under `parent` (ids are fresh). The main is on
/// the same page.
pub fn instantiate(
    tx: &mut Tx,
    main: NodeId,
    parent: Option<NodeId>,
    index: Option<usize>,
    root_rect: Option<Rect>,
) -> NodeId {
    instantiate_from(tx, None, None, main, parent, index, root_rect)
}

/// Like [`instantiate`] for a main in another document `src` (`None` = this one), recorded
/// as `source` on every node of the instance.
pub fn instantiate_from(
    tx: &mut Tx,
    src: Option<&Document>,
    source: Option<Source>,
    main: NodeId,
    parent: Option<NodeId>,
    index: Option<usize>,
    root_rect: Option<Rect>,
) -> NodeId {
    let node = match src {
        Some(d) => d.get(main),
        None => tx.doc().get(main),
    };
    let src_node = node.unwrap().clone();
    let id = tx.alloc_id();
    let mut n = src_node.clone();
    n.id = id;
    n.children = Vec::new();
    n.component = false;
    n.component_key = None;
    n.comments.clear();
    n.instance = Some(InstanceLink {
        main,
        overrides: Vec::new(),
        source: source.clone(),
        swap: None,
    });
    if let Some(r) = root_rect {
        n.rect = r;
    }
    tx.insert(n, parent, index);
    for c in src_node.children {
        instantiate_from(tx, src, source.clone(), c, Some(id), None, None);
    }
    id
}

/// Mark property groups of an instance node as overridden, so edits to the main component
/// stop overwriting them. Does nothing for nodes that aren't part of an instance.
pub fn mark_overrides(tx: &mut Tx, id: NodeId, keys: &[&str]) {
    let Some(link) = tx.doc().get(id).and_then(|n| n.instance.as_ref()) else {
        return;
    };
    let missing: Vec<String> = keys
        .iter()
        .filter(|k| !link.overrides.iter().any(|o| o == *k))
        .map(|k| k.to_string())
        .collect();
    if missing.is_empty() {
        return;
    }
    if let Some(link) = tx.node_mut(id).and_then(|n| n.instance.as_mut()) {
        link.overrides.extend(missing);
    }
}

/// Re-sync the instance subtree at `inst` from `ext` (a library snapshot), keeping overrides.
pub fn sync_library_instance(tx: &mut Tx, inst: NodeId, ext: &Document) {
    sync_node(tx, inst, true, Some(ext));
}

/// Bring one instance subtree in line with its main counterpart. `ext` is the document of
/// the main's page when the link has a `source`; without it such an instance is left alone.
/// A swapped nested instance syncs from the component it was swapped to (`ext` is then that
/// component's page) and keeps its own place, like a root.
pub(super) fn sync_node(tx: &mut Tx, inst: NodeId, is_root: bool, ext: Option<&Document>) {
    let Some(n) = tx.doc().get(inst) else { return };
    let Some(link) = n.instance.clone() else {
        return;
    };
    let (main_id, source) = match &link.swap {
        Some(s) => (s.main, s.source.clone()),
        None => (link.main, link.source.clone()),
    };
    let is_root = is_root || link.swap.is_some();
    let ext = ext.filter(|_| source.is_some());
    let main = match ext {
        None => tx.doc().get(main_id),
        Some(d) => d.get(main_id),
    };
    if source.is_some() && ext.is_none() {
        return;
    }
    let Some(main) = main.cloned() else {
        return;
    };
    let mut updated = n.clone();
    copy_props(&main, &mut updated, &link.overrides, is_root);
    if updated != *n {
        *tx.node_mut(inst).unwrap() = updated;
    }
    // Children: match by link; create missing, remove stale, follow main's order.
    let kids: Vec<NodeId> = tx.doc().get(inst).unwrap().children.clone();
    let mut by_main: Vec<(NodeId, Option<Source>, NodeId)> = kids
        .iter()
        .filter_map(|k| {
            tx.doc()
                .get(*k)
                .and_then(|c| c.instance.as_ref())
                .map(|l| (l.main, l.source.clone(), *k))
        })
        .collect();
    for k in &kids {
        let stale = tx
            .doc()
            .get(*k)
            .and_then(|c| c.instance.as_ref())
            .is_none_or(|l| !main.children.contains(&l.main) || l.source != source);
        if stale {
            tx.remove(*k);
            by_main.retain(|(_, _, x)| x != k);
        }
    }
    for (i, mc) in main.children.iter().enumerate() {
        match by_main.iter().find(|(m, s, _)| m == mc && *s == source) {
            Some((_, _, k)) => {
                let k = *k;
                if tx.doc().children(Some(inst)).iter().position(|x| *x == k) != Some(i) {
                    tx.move_to_index(k, i);
                }
                // A kid swapped to a component on another page syncs from there (cross-page flush).
                let kid_ext = match tx.doc().get(k).and_then(|c| c.instance.as_ref()).and_then(|l| l.swap.as_ref()) {
                    Some(s) if s.source != source => None,
                    _ => ext,
                };
                sync_node(tx, k, false, kid_ext);
            }
            None => {
                instantiate_from(tx, ext, source.clone(), *mc, Some(inst), Some(i), None);
            }
        }
    }
}

impl Editor {
    /// Keep derived structure consistent after edits in the open transaction: group bounds,
    /// auto layout, and instances of edited components. Runs inside the transaction, so it
    /// undoes together with the edit that caused it.
    pub fn fixup(&mut self) {
        self.touched_mains.clear();
        let Some((rec, _)) = &self.rec else { return };
        let touched: Vec<NodeId> = rec.touched().to_vec();
        if touched.is_empty() {
            return;
        }
        let mut seeds: HashSet<NodeId> = HashSet::new();
        for id in &touched {
            if self.doc.nodes.contains_key(id) {
                seeds.insert(*id);
            }
            // Parents of removed or moved nodes.
            if let Some(Some(b)) = rec.before(*id)
                && let Some(p) = b.parent
                && self.doc.nodes.contains_key(&p)
            {
                seeds.insert(p);
            }
            if let Some(p) = self.doc.get(*id).and_then(|n| n.parent) {
                seeds.insert(p);
            }
        }
        let mut cands: HashSet<NodeId> = seeds.clone();
        for s in &seeds {
            cands.extend(self.doc.ancestors(*s));
        }
        let depth = |e: &Editor, id: NodeId| e.doc.ancestors(id).len();

        // 1. Groups, deepest first.
        let mut groups: Vec<NodeId> = cands
            .iter()
            .copied()
            .filter(|id| self.doc.get(*id).is_some_and(|n| n.is_group()))
            .collect();
        groups.sort_by_key(|g| std::cmp::Reverse(depth(self, *g)));
        // 2. Components touched (self or ancestor is a main component).
        let comps: Vec<NodeId> = cands
            .iter()
            .copied()
            .filter(|id| self.doc.get(*id).is_some_and(|n| n.component))
            .collect();
        self.touched_mains = comps.clone();
        {
            let mut tx = self.tx();
            for g in &groups {
                layout::normalize_group(&mut tx, *g);
            }
            for c in &comps {
                let instances: Vec<NodeId> = tx
                    .doc()
                    .nodes
                    .values()
                    .filter(|n| {
                        n.instance.as_ref().is_some_and(|l| match &l.swap {
                            Some(s) => s.main == *c && s.source.is_none(),
                            None => l.main == *c && l.source.is_none(),
                        })
                    })
                    .map(|n| n.id)
                    .collect();
                for i in instances {
                    sync_node(&mut tx, i, true, None);
                    cands.insert(i);
                }
            }
        }
        // 3. Auto layout from the outermost auto-layout ancestor of each candidate.
        let mut roots: BTreeSet<(usize, NodeId)> = BTreeSet::new();
        for id in cands.iter().copied() {
            let Some(n) = self.doc.get(id) else { continue };
            if n.layout.is_none()
                && n.parent
                    .and_then(|p| self.doc.get(p))
                    .is_none_or(|p| p.layout.is_none())
            {
                continue;
            }
            let mut top = if n.layout.is_some() {
                Some(id)
            } else {
                n.parent
            };
            let mut cur = top;
            while let Some(c) = cur {
                let cn = self.doc.get(c).unwrap();
                if cn.layout.is_some() {
                    top = Some(c);
                }
                // Climb while this frame sits in an auto-layout parent's flow.
                match cn.parent.and_then(|p| self.doc.get(p)) {
                    Some(p) if p.layout.is_some() && !cn.layout_child.absolute => cur = Some(p.id),
                    _ => break,
                }
            }
            if let Some(t) = top
                && !self.layout_suspended.contains(&t)
            {
                roots.insert((depth(self, t), t));
            }
        }
        if !roots.is_empty() {
            let mut tx = self.tx();
            for (_, r) in roots {
                layout::layout_frame(&mut tx, r);
            }
        }
        // 4. Arrows bound to anything that moved follow it.
        let touched: HashSet<NodeId> = self
            .rec
            .as_ref()
            .map(|(r, _)| r.touched().iter().copied().collect())
            .unwrap_or_default();
        self.reroute_connectors(&touched);
    }

    /// Record that `prop` was explicitly changed on nodes inside instances.
    pub fn mark_overrides(&mut self, ids: &[NodeId], field: &str) {
        let key = override_key(field);
        let mut tx = self.tx();
        for id in ids {
            let Some(n) = tx.doc().get(*id) else { continue };
            let Some(l) = &n.instance else { continue };
            let push = !l.overrides.iter().any(|o| o == key);
            let unbind = n.bindings.keys().any(|f| override_key(f) == key);
            if !push && !unbind {
                continue;
            }
            let n = tx.node_mut(*id).unwrap();
            // An overridden field stops following its variable, on this instance only.
            n.bindings.retain(|f, _| override_key(f) != key);
            if push && let Some(l) = &mut n.instance {
                l.overrides.push(key.into());
            }
        }
    }

    // ---------------------------------------------------------------------- groups

    /// Group the selection (Cmd+G). Selected layers must share a parent.
    pub fn group_selection(&mut self) {
        let ids = self.top_selection();
        if ids.is_empty() {
            return;
        }
        let parent = self.doc.get(ids[0]).and_then(|n| n.parent);
        if ids
            .iter()
            .any(|id| self.doc.get(*id).and_then(|n| n.parent) != parent)
        {
            self.status = "Selection must share a parent".into();
            return;
        }
        let Some(b) = self.selection_bounds() else {
            return;
        };
        let index = ids
            .iter()
            .filter_map(|id| self.doc.children(parent).iter().position(|x| x == id))
            .max()
            .map(|i| i + 1);
        let g = self.edit("Group", |tx| {
            let gid = tx.alloc_id();
            let o = tx.doc().parent_origin(parent);
            let g = Node::new(gid, NodeKind::Group, b.translate(V2::ZERO - o));
            tx.insert(g, parent, index);
            let mut ordered = ids.clone();
            ordered.sort_by_key(|id| tx.doc().children(parent).iter().position(|x| x == id));
            for id in ordered {
                tx.reparent(id, Some(gid), None);
            }
            gid
        });
        self.selection = vec![g];
    }

    /// Ungroup selected groups (Cmd+Shift+G); frames lose their frame too, like Figma.
    pub fn ungroup_selection(&mut self) {
        let ids: Vec<NodeId> = self
            .top_selection()
            .into_iter()
            .filter(|id| {
                self.doc
                    .get(*id)
                    .is_some_and(|n| n.is_container() && !n.children.is_empty())
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut freed = Vec::new();
        self.edit("Ungroup", |tx| {
            for g in ids {
                let n = tx.doc().get(g).unwrap().clone();
                let idx = tx
                    .doc()
                    .children(n.parent)
                    .iter()
                    .position(|x| *x == g)
                    .unwrap_or(0);
                for (k, c) in n.children.iter().enumerate() {
                    tx.reparent(*c, n.parent, Some(idx + 1 + k));
                    freed.push(*c);
                }
                tx.remove(g);
            }
        });
        self.selection = freed;
    }

    // ---------------------------------------------------------------------- components

    /// Make the selection a main component (Cmd+Alt+K). A single frame is converted in
    /// place; anything else is wrapped in a new component frame.
    pub fn create_component(&mut self) {
        let ids = self.top_selection();
        if ids.is_empty() {
            return;
        }
        if let [id] = ids[..]
            && self
                .doc
                .get(id)
                .is_some_and(|n| n.is_frame() && n.instance.is_none())
        {
            self.edit("Create component", |tx| {
                let n = tx.node_mut(id).unwrap();
                n.component = true;
                n.component_key.get_or_insert_with(crate::variables::new_id);
            });
            return;
        }
        self.wrap_in_frame();
        if let [f] = self.selection[..] {
            let n = self.doc.nodes.values().filter(|n| n.component).count();
            self.edit("Create component", |tx| {
                let node = tx.node_mut(f).unwrap();
                node.component = true;
                node.component_key.get_or_insert_with(crate::variables::new_id);
                node.name = format!("Component {}", n + 1);
            });
        }
    }

    /// Place an instance of a main component at world position `at` (top-left).
    pub fn create_instance(&mut self, component: NodeId, at: V2) -> Option<NodeId> {
        let main = self.doc.get(component)?;
        if !main.component {
            return None;
        }
        let size = (main.rect.w, main.rect.h);
        let id = self.edit("Create instance", |tx| {
            instantiate(
                tx,
                component,
                None,
                None,
                Some(Rect::new(at.x, at.y, size.0, size.1)),
            )
        });
        self.selection = vec![id];
        Some(id)
    }

    /// Duplicate the selected component as an instance to its right (Figma: Alt-drag).
    pub fn instance_of_selection(&mut self) {
        let Some(&c) = self.selection.first() else {
            return;
        };
        let Some(r) = self.doc.world_rect(c) else {
            return;
        };
        if self.doc.get(c).is_some_and(|n| n.component) {
            self.create_instance(c, v2(r.right() + 40.0, r.y));
        }
    }

    /// Detach selected instances: they become plain frames (Cmd+Alt+B).
    pub fn detach_instance(&mut self) {
        let roots: Vec<NodeId> = self
            .top_selection()
            .into_iter()
            .filter(|id| self.is_instance_root(*id) || self.is_main_missing(*id))
            .collect();
        if roots.is_empty() {
            return;
        }
        self.edit("Detach instance", |tx| {
            for r in roots {
                for id in tx.doc().subtree(r) {
                    tx.node_mut(id).unwrap().instance = None;
                }
            }
        });
    }

    /// Clear overrides on the selected instance nodes and resync from the main component.
    pub fn reset_overrides(&mut self) {
        self.finish_loading();
        let ids = self.top_selection();
        let roots: Vec<NodeId> = ids
            .iter()
            .filter_map(|id| {
                std::iter::once(*id)
                    .chain(self.doc.ancestors(*id))
                    .find(|a| self.is_instance_root(*a))
            })
            .collect();
        let exts: Vec<Option<Document>> = roots.iter().map(|r| self.source_doc(*r)).collect();
        self.edit("Reset overrides", |tx| {
            for r in &ids {
                for id in tx.doc().subtree(*r) {
                    if let Some(l) = &mut tx.node_mut(id).unwrap().instance {
                        l.overrides.clear();
                    }
                }
            }
            for (r, ext) in roots.into_iter().zip(&exts) {
                sync_node(tx, r, true, ext.as_ref());
            }
        });
    }

    /// Index of the page the document being edited belongs to (differs from `active_page`
    /// inside `with_page`).
    pub fn edit_page_index(&self) -> usize {
        self.edit_page.unwrap_or(self.active_page)
    }

    /// The page and node a link points at, when that node exists.
    fn link_main(&self, l: &InstanceLink) -> Option<(usize, &Node)> {
        let (main, source) = l.target();
        let page = match source {
            None => self.edit_page_index(),
            Some(Source::Page(p)) => self.pages.iter().position(|s| s.id == *p)?,
            Some(Source::Library(_)) => return None,
        };
        let doc = if page == self.edit_page_index() {
            &self.doc
        } else {
            &self.pages[page].stash.as_ref()?.doc
        };
        doc.get(main).map(|n| (page, n))
    }

    /// The main of `l` is on a page that is still being read, so it cannot be looked at yet.
    fn link_pending(&self, l: &InstanceLink) -> bool {
        matches!(l.target().1, Some(Source::Page(p)) if self.pages.iter().any(|s| s.id == *p && s.pending))
    }

    /// A clone of the document holding the main of instance node `id`, when it is another page.
    pub(super) fn source_doc(&self, id: NodeId) -> Option<Document> {
        let l = self.doc.get(id)?.instance.as_ref()?;
        let Source::Page(_) = l.target().1.as_ref()? else {
            return match l.target().1.as_ref()? {
                Source::Library(id) => self.library_docs.get(id).cloned(),
                Source::Page(_) => None,
            };
        };
        let (page, _) = self.link_main(l)?;
        Some(self.page_doc(page).clone())
    }

    fn library_main_ok(&self, lib: &str, main: NodeId) -> bool {
        self.library_docs.get(lib).and_then(|d| d.get(main)).is_some_and(|m| m.component)
    }

    pub fn is_instance_root(&self, id: NodeId) -> bool {
        self.doc.get(id).is_some_and(|n| {
            n.instance.as_ref().is_some_and(|l| match &l.source {
                // A library instance whose library or component is gone still counts (detach, warning).
                Some(Source::Library(lib)) => self.library_main_ok(lib, l.main) || self.is_main_missing(id),
                _ => self.link_pending(l) || self.link_main(l).is_some_and(|(_, m)| m.component),
            })
        })
    }

    /// An instance root whose main component no longer exists.
    pub fn is_main_missing(&self, id: NodeId) -> bool {
        self.doc.get(id).is_some_and(|n| {
            let Some(l) = &n.instance else { return false };
            let root = n
                .parent
                .and_then(|p| self.doc.get(p))
                .is_none_or(|p| p.instance.is_none());
            root && match &l.source {
                Some(Source::Library(lib)) => !self.library_main_ok(lib, l.main),
                _ => !self.link_pending(l) && self.link_main(l).is_none_or(|(_, m)| !m.component),
            }
        })
    }

    /// Instance roots on page `page` whose main component is gone (deleted, or its page).
    /// They keep their content; the UI offers Detach or Restore. Not for use inside `with_page`.
    pub fn missing_mains(&self, page: usize) -> Vec<NodeId> {
        let doc = self.page_doc(page);
        let mut out: Vec<NodeId> = doc
            .nodes
            .values()
            .filter(|n| {
                let Some(l) = &n.instance else { return false };
                let root = n
                    .parent
                    .and_then(|p| doc.get(p))
                    .is_none_or(|p| p.instance.is_none());
                let found = match &l.source {
                    None => doc.get(l.main),
                    Some(Source::Page(p)) => {
                        let src = self.pages.iter().position(|s| s.id == *p);
                        // Still being read: its mains cannot be told apart from missing ones.
                        if src.is_some_and(|i| self.pages[i].pending) {
                            return false;
                        }
                        src.and_then(|i| self.page_doc(i).get(l.main))
                    }
                    Some(Source::Library(lib)) => return root && !self.library_main_ok(lib, l.main),
                };
                root && found.is_none_or(|m| !m.component)
            })
            .map(|n| n.id)
            .collect();
        out.sort();
        out
    }

    /// The main component an instance root points to, when it is on the same page.
    pub fn instance_main_root(&self, id: NodeId) -> Option<NodeId> {
        let l = self.doc.get(id)?.instance.as_ref()?;
        if l.source.is_some() {
            return None;
        }
        self.link_main(l).filter(|(_, m)| m.component).map(|(_, m)| m.id)
    }

    /// Select the main component of the selected instance.
    pub fn go_to_main(&mut self) {
        let Some(l) = self
            .selection
            .first()
            .and_then(|id| self.doc.get(*id))
            .and_then(|n| n.instance.clone())
        else {
            return;
        };
        let Some((page, m)) = self.link_main(&l).filter(|(_, m)| m.component).map(|(p, m)| (p, m.id))
        else {
            return;
        };
        self.switch_page(page);
        self.selection = vec![m];
        if let Some(r) = self.doc.world_rect(m) {
            self.camera
                .fit(&r, self.viewport, 80.0, self.camera.zoom.max(1.0));
            self.revision += 1;
        }
    }

    /// Swap an instance root to point at `new_main` component, preserving overrides on children
    /// matched by name and kind.
    pub fn swap_instance(&mut self, inst: NodeId, new_main: NodeId) -> bool {
        if !self.is_instance_root(inst)
            || !self.doc.get(new_main).is_some_and(|m| m.component)
            || inst == new_main
            || self.doc.is_ancestor(inst, new_main)
        {
            return false;
        }
        self.edit("Swap instance", |tx| swap_instance_tx(tx, None, None, inst, new_main));
        true
    }

    // ---------------------------------------------------------------------- auto layout

    /// Add auto layout (Shift+A): converts a selected frame, or wraps the selection in a new
    /// auto-layout frame whose direction, gap and padding are inferred from the arrangement.
    pub fn add_auto_layout(&mut self) {
        let ids = self.top_selection();
        if ids.is_empty() {
            return;
        }
        if let [id] = ids[..]
            && self.doc.get(id).is_some_and(|n| n.is_frame())
        {
            if self.doc.get(id).unwrap().layout.is_some() {
                return;
            }
            let kids = self.doc.get(id).unwrap().children.clone();
            let rects: Vec<Rect> = kids
                .iter()
                .map(|k| self.doc.get(*k).unwrap().rect)
                .collect();
            let l = infer_layout(
                &rects,
                self.doc.get(id).unwrap().rect.w,
                self.doc.get(id).unwrap().rect.h,
            );
            self.edit("Add auto layout", |tx| {
                // Order children along the inferred axis.
                let mut order = kids.clone();
                order.sort_by(|a, b| {
                    let (ra, rb) = (
                        tx.doc().get(*a).unwrap().rect,
                        tx.doc().get(*b).unwrap().rect,
                    );
                    if l.direction == Direction::Horizontal {
                        ra.x.total_cmp(&rb.x)
                    } else {
                        ra.y.total_cmp(&rb.y)
                    }
                });
                for (i, k) in order.iter().enumerate() {
                    tx.move_to_index(*k, i);
                }
                let n = tx.node_mut(id).unwrap();
                n.layout = Some(l);
                n.layout_child.sizing = [Sizing::Hug; 2];
            });
            self.edit("Add auto layout", |_| {});
            return;
        }
        let rects: Vec<Rect> = ids.iter().filter_map(|i| self.doc.world_rect(*i)).collect();
        self.wrap_in_frame();
        let Some(&f) = self.selection.first() else {
            return;
        };
        let fr = self.doc.world_rect(f).unwrap();
        let rel: Vec<Rect> = rects
            .iter()
            .map(|r| r.translate(V2::ZERO - fr.min()))
            .collect();
        let mut l = infer_layout(&rel, fr.w, fr.h);
        l.padding = [0.0; 4];
        let kids = self.doc.get(f).unwrap().children.clone();
        self.edit("Add auto layout", |tx| {
            let mut order = kids.clone();
            order.sort_by(|a, b| {
                let (ra, rb) = (
                    tx.doc().get(*a).unwrap().rect,
                    tx.doc().get(*b).unwrap().rect,
                );
                if l.direction == Direction::Horizontal {
                    ra.x.total_cmp(&rb.x)
                } else {
                    ra.y.total_cmp(&rb.y)
                }
            });
            for (i, k) in order.iter().enumerate() {
                tx.move_to_index(*k, i);
            }
            let n = tx.node_mut(f).unwrap();
            n.layout = Some(l);
            n.layout_child.sizing = [Sizing::Hug; 2];
            n.name = "Auto layout".into();
        });
    }

    pub fn remove_auto_layout(&mut self) {
        let ids: Vec<NodeId> = self
            .top_selection()
            .into_iter()
            .filter(|id| self.doc.get(*id).is_some_and(|n| n.layout.is_some()))
            .collect();
        self.edit("Remove auto layout", |tx| {
            for id in ids {
                let n = tx.node_mut(id).unwrap();
                n.layout = None;
                n.layout_child.sizing = [Sizing::Fixed; 2];
            }
        });
    }

    // ---------------------------------------------------------------------- align

    /// Align the selection: to its bounds, or a single layer to its parent frame.
    /// `axis` 0 = x, 1 = y; `to` 0 = start, 0.5 = center, 1 = end.
    pub fn align(&mut self, axis: usize, to: f64) {
        let ids: Vec<NodeId> = self
            .top_selection()
            .into_iter()
            .filter(|id| !self.doc.is_effectively_locked(*id))
            .collect();
        let target = if ids.len() == 1 {
            match self.doc.get(ids[0]).and_then(|n| n.parent) {
                Some(p) => self.doc.world_rect(p),
                None => return,
            }
        } else {
            self.selection_bounds()
        };
        let Some(t) = target else { return };
        let moves: Vec<(NodeId, V2)> = ids
            .iter()
            .filter_map(|id| {
                let r = self.doc.world_rect(*id)?;
                let d = if axis == 0 {
                    v2(t.x + (t.w - r.w) * to - r.x, 0.0)
                } else {
                    v2(0.0, t.y + (t.h - r.h) * to - r.y)
                };
                Some((*id, d))
            })
            .collect();
        self.edit("Align", |tx| {
            for (id, d) in moves {
                let n = tx.node_mut(id).unwrap();
                n.rect = n.rect.translate(d);
            }
        });
    }

    /// Equalize the gaps between 3+ selected layers along an axis.
    pub fn distribute(&mut self, axis: usize) {
        let mut items: Vec<(NodeId, Rect)> = self
            .top_selection()
            .into_iter()
            .filter_map(|id| Some((id, self.doc.world_rect(id)?)))
            .collect();
        if items.len() < 3 {
            return;
        }
        let key = |r: &Rect| if axis == 0 { r.x } else { r.y };
        let size = |r: &Rect| if axis == 0 { r.w } else { r.h };
        items.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
        let first = key(&items[0].1);
        let last = items.last().unwrap();
        let span = key(&last.1) + size(&last.1) - first;
        let total: f64 = items.iter().map(|(_, r)| size(r)).sum();
        let gap = (span - total) / (items.len() - 1) as f64;
        let mut cursor = first;
        let mut moves = Vec::new();
        for (id, r) in &items {
            let d = cursor - key(r);
            moves.push((*id, if axis == 0 { v2(d, 0.0) } else { v2(0.0, d) }));
            cursor += size(r) + gap;
        }
        self.edit("Distribute", |tx| {
            for (id, d) in moves {
                let n = tx.node_mut(id).unwrap();
                n.rect = n.rect.translate(d);
            }
        });
    }
}

/// Infer auto-layout settings from children's current arrangement (frame-local rects).
pub fn infer_layout(rects: &[Rect], fw: f64, fh: f64) -> AutoLayout {
    let mut l = AutoLayout::default();
    if rects.is_empty() {
        return l;
    }
    let u = Rect::union_all(rects).unwrap();
    let horizontal = u.w / fw.max(1.0) >= u.h / fh.max(1.0) && rects.len() > 1 && {
        let spread_x = u.w - rects.iter().map(|r| r.w).fold(0.0, f64::max);
        let spread_y = u.h - rects.iter().map(|r| r.h).fold(0.0, f64::max);
        spread_x >= spread_y
    };
    l.direction = if horizontal {
        Direction::Horizontal
    } else {
        Direction::Vertical
    };
    let mut sorted = rects.to_vec();
    if horizontal {
        sorted.sort_by(|a, b| a.x.total_cmp(&b.x));
    } else {
        sorted.sort_by(|a, b| a.y.total_cmp(&b.y));
    }
    let gaps: Vec<f64> = sorted
        .windows(2)
        .map(|w| {
            if horizontal {
                w[1].x - w[0].right()
            } else {
                w[1].y - w[0].bottom()
            }
        })
        .collect();
    if !gaps.is_empty() {
        l.gap = (gaps.iter().sum::<f64>() / gaps.len() as f64)
            .round()
            .max(0.0);
    }
    l.padding = [
        u.y.max(0.0).round(),
        (fw - u.right()).max(0.0).round(),
        (fh - u.bottom()).max(0.0).round(),
        u.x.max(0.0).round(),
    ];
    l
}

/// Swap `inst` to `new_main` inside an open transaction, matching child overrides by name/kind.
pub fn swap_instance_tx(
    tx: &mut Tx,
    ext: Option<&Document>,
    source: Option<Source>,
    inst: NodeId,
    new_main: NodeId,
) -> bool {
    let src = ext.unwrap_or(tx.doc());
    let Some(new_comp) = src.get(new_main).cloned() else {
        return false;
    };
    if !new_comp.component {
        return false;
    }
    let mut target_by_name: Vec<(String, &'static str, NodeId)> = Vec::new();
    for cid in src.subtree(new_main).into_iter().skip(1) {
        if let Some(cn) = src.get(cid) {
            target_by_name.push((cn.name.clone(), cn.kind.type_name(), cid));
        }
    }
    let mut used: HashSet<NodeId> = HashSet::new();
    let inst_children: Vec<NodeId> = tx.doc().subtree(inst).into_iter().skip(1).collect();
    for k in inst_children {
        let Some(kn) = tx.doc().get(k).cloned() else {
            continue;
        };
        let matched = target_by_name
            .iter()
            .find(|(name, kind, tid)| {
                !used.contains(tid) && *name == kn.name && *kind == kn.kind.type_name()
            })
            .map(|(_, _, tid)| *tid);
        if let Some(tid) = matched {
            used.insert(tid);
            if let Some(link) = tx.node_mut(k).and_then(|n| n.instance.as_mut()) {
                link.main = tid;
            }
        }
        // Unmatched nodes are removed by the sync below; they must not keep the old source.
        if let Some(link) = tx.node_mut(k).and_then(|n| n.instance.as_mut()) {
            link.source = source.clone();
        }
    }
    if let Some(root) = tx.node_mut(inst) {
        if let Some(link) = &mut root.instance {
            link.main = new_main;
            link.source = source;
        }
        if root
            .instance
            .as_ref()
            .is_none_or(|l| !l.overrides.iter().any(|o| o == "name"))
        {
            root.name = new_comp.name;
        }
    }
    sync_node(tx, inst, true, ext);
    true
}

/// Swap a nested instance (one inside an instance) to another component: it keeps its place in
/// the outer instance, so later syncs of the outer main keep the swap.
pub fn swap_nested_tx(tx: &mut Tx, ext: Option<&Document>, source: Option<Source>, inst: NodeId, new_main: NodeId) -> bool {
    let src = ext.unwrap_or(tx.doc());
    if !src.get(new_main).is_some_and(|c| c.component) {
        return false;
    }
    let name = src.get(new_main).unwrap().name.clone();
    for k in tx.doc().children(Some(inst)).to_vec() {
        tx.remove(k);
    }
    let Some(n) = tx.node_mut(inst) else { return false };
    let Some(link) = n.instance.as_mut() else { return false };
    link.swap = Some(Swap { main: new_main, source });
    link.overrides.retain(|o| o == "name");
    if !link.overrides.iter().any(|o| o == "name") {
        n.name = name;
    }
    sync_node(tx, inst, true, ext);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Paint;

    /// Two pages; a component on page 0 with a text child, an instance of it on page 1.
    fn setup() -> (Editor, NodeId, NodeId) {
        let mut e = Editor::default();
        e.add_page_in_background(None);
        let comp = e.add_node(
            Node::new(NodeId(0), NodeKind::Frame { clip: false }, Rect::new(0., 0., 50., 50.)),
            None,
        );
        e.selection = vec![comp];
        e.create_component();
        let src = e.doc.clone();
        let source = Some(Source::Page(e.pages[0].id.clone()));
        let inst = e.with_page(1, |e| {
            e.edit("i", |tx| instantiate_from(tx, Some(&src), source, comp, None, None, None))
        });
        (e, comp, inst)
    }

    fn red(e: &Editor, page: usize, id: NodeId) -> bool {
        matches!(e.page_doc(page).get(id).unwrap().style.fills.first(),
            Some(Paint::Solid { color, .. }) if color.r == 255 && color.b == 0)
    }

    #[test]
    fn main_edit_updates_other_page_with_undo_per_page() {
        let (mut e, comp, inst) = setup();
        assert!(e.doc.get(comp).unwrap().component_key.is_some());
        assert!(e.page_doc(1).get(inst).unwrap().component_key.is_none());
        e.edit("red", |tx| {
            tx.node_mut(comp).unwrap().style.fills = vec![Paint::solid(crate::model::Color::rgb(255, 0, 0))];
        });
        assert!(red(&e, 1, inst), "page B follows the main");
        e.undo();
        assert!(!red(&e, 0, comp), "A restored");
        assert!(red(&e, 1, inst), "B untouched by undo on A");
        e.with_page(1, |e| e.undo());
        assert!(!red(&e, 1, inst), "undo on B restores B");
    }

    #[test]
    fn swap_and_main_edit_keep_how_the_root_sits_in_its_parent() {
        let (mut e, comp, inst) = setup();
        e.edit("absolute main", |tx| tx.node_mut(comp).unwrap().layout_child.absolute = true);
        let src = e.doc.clone();
        let source = Some(Source::Page(e.pages[0].id.clone()));
        let other = e.add_node(
            Node::new(NodeId(0), NodeKind::Frame { clip: false }, Rect::new(0., 0., 40., 40.)),
            None,
        );
        e.selection = vec![other];
        e.create_component();
        let src2 = e.doc.clone();
        e.with_page(1, |e| {
            e.edit("in flow", |tx| {
                let n = tx.node_mut(inst).unwrap();
                n.layout_child.absolute = false;
                n.layout_child.sizing = [Sizing::Fill, Sizing::Hug];
            });
            e.edit("sync", |tx| sync_node(tx, inst, true, Some(&src)));
            e.edit("swap", |tx| swap_instance_tx(tx, Some(&src2), source, inst, other));
            let lc = e.doc.get(inst).unwrap().layout_child;
            assert!(!lc.absolute, "root stays in auto layout");
            assert_eq!(lc.sizing, [Sizing::Fill, Sizing::Hug]);
        });
    }

    #[test]
    fn deleting_main_keeps_instance_and_lists_it() {
        let (mut e, comp, inst) = setup();
        assert!(e.missing_mains(1).is_empty());
        e.edit("del", |tx| tx.remove(comp));
        assert!(e.page_doc(1).get(inst).is_some());
        assert_eq!(e.missing_mains(1), vec![inst]);
    }

    #[test]
    fn sync_replaces_child_links_with_a_different_source() {
        let mut e = Editor::default();
        let comp = e.add_node(
            Node::new(NodeId(0), NodeKind::Frame { clip: false }, Rect::new(0., 0., 50., 50.)),
            None,
        );
        let kid = e.add_node(
            Node::new(NodeId(0), NodeKind::Frame { clip: false }, Rect::new(0., 0., 5., 5.)),
            Some(comp),
        );
        e.selection = vec![comp];
        e.create_component();
        let inst = e.edit("i", |tx| instantiate(tx, comp, None, None, None));
        let old_kid = e.doc.get(inst).unwrap().children[0];
        e.edit("stale", |tx| {
            tx.node_mut(old_kid).unwrap().instance.as_mut().unwrap().source = Some(Source::Page("gone".into()));
        });
        e.edit("sync", |tx| sync_node(tx, inst, true, None));
        let kids = e.doc.get(inst).unwrap().children.clone();
        assert_eq!(kids.len(), 1);
        assert_ne!(kids[0], old_kid, "stale-source child replaced");
        let l = e.doc.get(kids[0]).unwrap().instance.clone().unwrap();
        assert_eq!((l.main, l.source), (kid, None));
    }
}
