//! Undoable annotation edits: each call is one history step.

use super::Editor;
use crate::history::Tx;
use crate::model::annotation::{Annotation, AnnotationCategory, AnnotationTemplate, builtin_templates};
use crate::variables::{Error, PAGE_TEMPLATE_KEY};

impl Tx<'_> {
    /// Add an annotation (its id is allocated here). Returns the id.
    pub fn add_annotation(&mut self, mut a: Annotation) -> u64 {
        self.snapshot_annotations();
        a.id = self.doc.alloc_id().0;
        if a.created == 0 {
            a.created = crate::project::format::now_ms();
        }
        let id = a.id;
        self.doc.annotations.push(a);
        id
    }

    /// Add annotation copies (see `annotation::copies_for`) under fresh ids.
    pub fn add_annotation_copies(&mut self, copies: Vec<Annotation>) {
        for a in copies {
            self.add_annotation(a);
        }
    }

    /// Change an annotation in place; `id` and `target` stay as they were unless `f` sets them.
    pub fn update_annotation(&mut self, id: u64, f: impl FnOnce(&mut Annotation)) -> bool {
        if !self.doc.annotations.iter().any(|a| a.id == id) {
            return false;
        }
        self.snapshot_annotations();
        let a = self.doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
        f(a);
        a.id = id;
        true
    }

    pub fn delete_annotation(&mut self, id: u64) -> bool {
        if !self.doc.annotations.iter().any(|a| a.id == id) {
            return false;
        }
        self.snapshot_annotations();
        self.doc.annotations.retain(|a| a.id != id);
        true
    }
}

impl Editor {
    pub fn add_annotation(&mut self, a: Annotation) -> u64 {
        self.edit("Add annotation", |tx| tx.add_annotation(a))
    }

    /// Update label, body, category, position or anchor of one annotation.
    pub fn update_annotation(&mut self, id: u64, f: impl FnOnce(&mut Annotation)) -> bool {
        self.edit("Edit annotation", |tx| tx.update_annotation(id, f))
    }

    pub fn delete_annotation(&mut self, id: u64) -> bool {
        self.edit("Delete annotation", |tx| tx.delete_annotation(id))
    }

    /// Page `page`'s own template override, if set.
    pub fn page_template_id(&self, page: usize) -> Option<&str> {
        self.pages[page].modes.get(PAGE_TEMPLATE_KEY).map(String::as_str)
    }

    /// The template in effect on page `page`: its override, else the project default.
    pub fn active_template(&self, page: usize) -> AnnotationTemplate {
        self.design.active_template(self.page_template_id(page))
    }

    /// Set the project's default template (`None` = Product spec). Undoable.
    pub fn set_default_template(&mut self, id: Option<&str>) -> Result<(), Error> {
        if let Some(id) = id
            && !self.design.templates().iter().any(|t| t.id == id)
        {
            return Err(Error::NotFound(id.into()));
        }
        self.edit_design("Set default annotation template", |d| {
            d.annotation_template = id.map(String::from);
            Ok(())
        })
        .map(|_| ())
    }

    /// Override the template on one page (`None` = follow the project default). Undoable.
    pub fn set_page_template(&mut self, page: usize, id: Option<&str>) -> Result<(), Error> {
        if let Some(id) = id
            && !self.design.templates().iter().any(|t| t.id == id)
        {
            return Err(Error::NotFound(id.into()));
        }
        self.set_page_template_on(page, id);
        Ok(())
    }

    /// Add a template, or replace the one with the same id. Undoable.
    pub fn upsert_template(&mut self, t: AnnotationTemplate) -> Result<(), Error> {
        self.edit_design("Edit annotation template", |d| {
            let mut all = d.templates();
            match all.iter_mut().find(|x| x.id == t.id) {
                Some(x) => *x = t,
                None => all.push(t),
            }
            d.annotation_templates = all;
            Ok(())
        })
        .map(|_| ())
    }

    /// Delete a template (the last one stays). A default pointing at it goes back to Product spec.
    pub fn delete_template(&mut self, id: &str) -> Result<(), Error> {
        self.edit_design("Delete annotation template", |d| {
            let mut all = d.templates();
            if all.len() < 2 {
                return Err(Error::LastMode);
            }
            let n = all.len();
            all.retain(|t| t.id != id);
            if all.len() == n {
                return Err(Error::NotFound(id.into()));
            }
            d.annotation_templates = all;
            if d.annotation_template.as_deref() == Some(id) {
                d.annotation_template = None;
            }
            Ok(())
        })
        .map(|_| ())
    }

    /// Copy a template under a fresh id ("<name> copy"). Returns the new id. Undoable.
    pub fn duplicate_template(&mut self, id: &str) -> Result<String, Error> {
        self.edit_design("Duplicate annotation template", |d| {
            let mut all = d.templates();
            let src = all.iter().find(|t| t.id == id).ok_or_else(|| Error::NotFound(id.into()))?;
            let mut copy = src.clone();
            let mut n = 1;
            while all.iter().any(|t| t.id == copy.id) {
                n += 1;
                copy.id = format!("{id}-{n}");
            }
            copy.name = format!("{} copy", src.name);
            let new = copy.id.clone();
            all.push(copy);
            d.annotation_templates = all;
            Ok(new)
        })
        .map(|(id, _)| id)
    }

    /// Save an edited template together with the category list it was edited against. A changed
    /// category key is renamed in every template. One undo step per page. Duplicate keys are refused.
    pub fn save_template(&mut self, t: AnnotationTemplate, cats: Vec<AnnotationCategory>) -> Result<(), Error> {
        self.edit_design("Edit annotation template", |d| {
            let old = d.categories();
            for (i, c) in cats.iter().enumerate() {
                if cats[..i].iter().any(|o| o.key().eq_ignore_ascii_case(c.key())) {
                    return Err(Error::Duplicate(c.key().into()));
                }
            }
            let renames: Vec<(String, String)> = cats
                .iter()
                .filter_map(|c| old.iter().find(|o| o.id == c.id).map(|o| (o.key().to_string(), c.key().to_string())))
                .filter(|(a, b)| a != b)
                .collect();
            let rename = |t: &mut AnnotationTemplate| {
                for tc in &mut t.categories {
                    if let Some((_, to)) = renames.iter().find(|(from, _)| *from == tc.key) {
                        tc.key.clone_from(to);
                    }
                }
            };
            let mut all = d.templates();
            all.iter_mut().for_each(rename);
            let mut t = t;
            rename(&mut t);
            match all.iter_mut().find(|x| x.id == t.id) {
                Some(x) => *x = t,
                None => all.push(t),
            }
            d.annotation_templates = all;
            d.annotation_categories = cats;
            Ok(())
        })
        .map(|_| ())
    }

    /// Put a built-in template back to its original content. Errors for a custom template.
    pub fn reset_template(&mut self, id: &str) -> Result<(), Error> {
        let original = builtin_templates()
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| Error::NotFound(id.into()))?;
        self.upsert_template(original)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Rect, V2, v2};
    use crate::model::annotation::{self, Anchor, Side};
    use crate::model::{Node, NodeId, NodeKind};

    fn frame(e: &mut Editor, r: Rect) -> NodeId {
        e.add_node(Node::new(NodeId(0), NodeKind::Frame { clip: true }, r), None)
    }

    fn child(e: &mut Editor, parent: NodeId, r: Rect) -> NodeId {
        e.add_node(Node::new(NodeId(0), NodeKind::Rect, r), Some(parent))
    }

    #[test]
    fn add_update_delete_undo_one_step_each() {
        let mut e = Editor::default();
        let f = frame(&mut e, Rect::new(0.0, 0.0, 100.0, 100.0));
        let base = e.doc.clone();
        let mut a = Annotation::new(Some(f), "Label", "data");
        a.position = v2(120.0, 0.0);
        let id = e.add_annotation(a);
        let added = e.doc.clone();
        assert_eq!(e.doc.annotations.len(), 1);
        assert!(e.update_annotation(id, |a| {
            a.label = "New".into();
            a.body = "text".into();
            a.category = "content".into();
            a.position = v2(5.0, 6.0);
            a.anchor = Anchor { side: Side::Left, t: 0.25 };
        }));
        let updated = e.doc.clone();
        assert_eq!(updated.annotations[0].label, "New");
        assert!(e.delete_annotation(id));
        assert!(e.doc.annotations.is_empty());
        assert!(!e.delete_annotation(id), "missing id is a no-op");
        e.undo();
        assert_eq!(e.doc, updated);
        e.undo();
        assert_eq!(e.doc, added);
        e.undo();
        assert_eq!(e.doc, base);
        e.redo();
        assert_eq!(e.doc, added);
    }

    #[test]
    fn move_and_resize_target_keep_anchor_and_card() {
        let mut e = Editor::default();
        let f = frame(&mut e, Rect::new(100.0, 100.0, 400.0, 400.0));
        let c = child(&mut e, f, Rect::new(10.0, 20.0, 100.0, 50.0));
        let mut a = Annotation::new(Some(c), "x", "data");
        a.anchor = Anchor { side: Side::Right, t: 0.5 };
        a.position = v2(150.0, -10.0);
        let id = e.add_annotation(a);
        let a0 = e.doc.annotations[0].clone();
        assert_eq!(annotation::anchor_world(&e.doc, &a0), v2(210.0, 145.0));
        assert_eq!(annotation::card_world(&e.doc, &a0), v2(260.0, 110.0));
        e.edit("move", |tx| tx.node_mut(c).unwrap().rect = Rect::new(30.0, 40.0, 200.0, 100.0));
        let a1 = e.doc.annotations[0].clone();
        assert_eq!(a1, a0, "annotation itself untouched");
        assert_eq!(annotation::anchor_world(&e.doc, &a1), v2(330.0, 190.0));
        assert_eq!(annotation::card_world(&e.doc, &a1), v2(280.0, 130.0));
        e.edit("move frame", |tx| tx.node_mut(f).unwrap().rect.x += 1000.0);
        assert_eq!(annotation::anchor_world(&e.doc, &e.doc.annotations[0]), v2(1330.0, 190.0));
        let _ = id;
    }

    #[test]
    fn deleting_target_detaches_and_undo_reattaches() {
        let mut e = Editor::default();
        let f = frame(&mut e, Rect::new(100.0, 100.0, 400.0, 400.0));
        let c = child(&mut e, f, Rect::new(10.0, 20.0, 100.0, 50.0));
        let mut a = Annotation::new(Some(c), "x", "data");
        a.anchor = Anchor { side: Side::Bottom, t: 1.0 };
        a.position = v2(150.0, 0.0);
        e.add_annotation(a);
        let before = e.doc.clone();
        let (anchor, card) = {
            let a = &e.doc.annotations[0];
            (annotation::anchor_world(&e.doc, a), annotation::card_world(&e.doc, a))
        };
        e.edit("Delete", |tx| tx.remove(f));
        let a = &e.doc.annotations[0];
        assert!(a.detached && a.target.is_none());
        assert_eq!(a.point, Some(anchor));
        assert_eq!(annotation::anchor_world(&e.doc, a), anchor);
        assert_eq!(annotation::card_world(&e.doc, a), card);
        e.undo();
        assert_eq!(e.doc, before);
    }

    #[test]
    fn auto_position_sits_right_of_frame_and_stacks() {
        let mut e = Editor::default();
        let f = frame(&mut e, Rect::new(100.0, 100.0, 400.0, 400.0));
        let c = child(&mut e, f, Rect::new(10.0, 20.0, 100.0, 50.0));
        let p1 = annotation::auto_position(&e.doc, c);
        let mut a = Annotation::new(Some(c), "1", "rule");
        a.position = p1;
        e.add_annotation(a);
        let w1 = annotation::card_world(&e.doc, &e.doc.annotations[0]);
        assert_eq!(w1, v2(524.0, 120.0));
        let p2 = annotation::auto_position(&e.doc, c);
        let w2 = v2(110.0, 120.0) + p2;
        assert_eq!(w2.x, w1.x);
        assert!(w2.y >= w1.y + annotation::CARD_H, "stacked below, no overlap");
    }

    #[test]
    fn numbering_is_per_frame_in_reading_order() {
        let mut e = Editor::default();
        let f1 = frame(&mut e, Rect::new(0.0, 0.0, 300.0, 300.0));
        let f2 = frame(&mut e, Rect::new(500.0, 0.0, 300.0, 300.0));
        let low = child(&mut e, f1, Rect::new(10.0, 200.0, 20.0, 20.0));
        let right = child(&mut e, f1, Rect::new(200.0, 10.0, 20.0, 20.0));
        let left = child(&mut e, f1, Rect::new(10.0, 10.0, 20.0, 20.0));
        let other = child(&mut e, f2, Rect::new(10.0, 10.0, 20.0, 20.0));
        let mut ids = Vec::new();
        for t in [Some(low), Some(right), Some(left), Some(other), None] {
            ids.push(e.add_annotation(Annotation::new(t, "n", "data")));
        }
        let n = annotation::numbers(&e.doc);
        assert_eq!(n[&ids[2]], (Some(f1), 1));
        assert_eq!(n[&ids[1]], (Some(f1), 2));
        assert_eq!(n[&ids[0]], (Some(f1), 3));
        assert_eq!(n[&ids[3]], (Some(f2), 1));
        assert_eq!(n[&ids[4]], (None, 1));
    }

    #[test]
    fn paste_of_a_frame_copies_its_annotations() {
        let mut e = Editor::default();
        let f = frame(&mut e, Rect::new(0.0, 0.0, 200.0, 200.0));
        let c = child(&mut e, f, Rect::new(10.0, 10.0, 20.0, 20.0));
        e.add_annotation(Annotation::new(Some(c), "on child", "data"));
        e.add_annotation(Annotation::new(Some(f), "on frame", "content"));
        e.add_annotation(Annotation::new(None, "loose", "content"));
        e.select(vec![f]);
        e.copy_internal();
        e.paste(V2::ZERO);
        assert_eq!(e.doc.annotations.len(), 5);
        let copy = e.selection[0];
        let copy_child = e.doc.get(copy).unwrap().children[0];
        let mut ids: Vec<u64> = e.doc.annotations.iter().map(|a| a.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 5, "fresh ids");
        let on = |t| e.doc.annotations.iter().filter(|a| a.target == Some(t)).count();
        assert_eq!((on(f), on(c), on(copy), on(copy_child)), (1, 1, 1, 1));
        e.undo();
        assert_eq!(e.doc.annotations.len(), 3);
    }

    #[test]
    fn restore_snapshot_restores_annotations_in_one_undo() {
        let mut e = Editor::default();
        let f = frame(&mut e, Rect::new(0.0, 0.0, 100.0, 100.0));
        e.add_annotation(Annotation::new(Some(f), "old", "data"));
        let snap = e.doc.clone();
        e.delete_annotation(snap.annotations[0].id);
        e.add_annotation(Annotation::new(Some(f), "agent", "rule"));
        let changed = e.doc.clone();
        e.restore_snapshot(&snap);
        assert_eq!(e.doc.annotations, snap.annotations, "target kept, not detached");
        e.undo();
        assert_eq!(e.doc.annotations, changed.annotations);
    }

    #[test]
    fn revert_to_snapshot_without_annotations_removes_them_and_undo_restores() {
        let mut e = Editor::default();
        let f = frame(&mut e, Rect::new(0.0, 0.0, 100.0, 100.0));
        let snap = e.doc.clone();
        e.add_annotation(Annotation::new(Some(f), "agent", "rule"));
        e.restore_snapshot(&snap);
        assert!(e.doc.annotations.is_empty());
        e.undo();
        assert_eq!(e.doc.annotations.len(), 1);
        assert_eq!(e.doc.annotations[0].target, Some(f));
    }

    #[test]
    fn save_template_renames_category_key_everywhere_in_one_undo() {
        let mut e = Editor::default();
        let mut cats = e.design.categories();
        cats[0].key = "policy".into();
        let mut t = e.design.template_by_ref("product-spec").unwrap();
        t.name = "Spec".into();
        let before = e.design.clone();
        let undo_len = e.history.undo_len();
        e.save_template(t, cats.clone()).unwrap();
        assert_eq!(e.history.undo_len(), undo_len + 1);
        for t in e.design.templates() {
            assert!(t.categories.iter().all(|c| c.key != "rule"), "{}", t.name);
        }
        assert!(e.active_template(0).enabled("policy").is_some());
        assert!(e.design.category_by_key("policy").is_some());
        let mut dup = cats.clone();
        dup[1].key = "POLICY".into();
        let t = e.design.template_by_ref("minimal").unwrap();
        let now = e.design.clone();
        assert!(e.save_template(t, dup).is_err());
        assert_eq!(e.design, now);
        e.undo();
        assert_eq!(e.design, before);
    }
}
