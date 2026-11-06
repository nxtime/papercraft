//! Named type styles on a page: define, change, delete and apply. Each call is one undo step.

use super::Editor;
use crate::model::{NodeId, TypeStyle};

impl Editor {
    /// Create or replace style `name`; every layer that follows it takes the new values.
    /// Returns how many layers changed.
    pub fn set_text_style(&mut self, name: &str, style: TypeStyle) -> usize {
        self.begin("Set text style");
        self.tx().snapshot_variables();
        let mut changed = Vec::new();
        for id in followers(&self.doc, name) {
            if let Some(n) = self.tx().node_mut(id)
                && style.apply(name, n)
            {
                changed.push(id);
            }
        }
        self.doc.text_styles.insert(name.to_string(), style);
        for id in &changed {
            self.fit_text(*id);
        }
        self.commit();
        changed.len()
    }

    /// Remove style `name`; layers that followed it keep their current look and stop following.
    /// None when there is no such style.
    pub fn delete_text_style(&mut self, name: &str) -> Option<usize> {
        if !self.doc.text_styles.contains_key(name) {
            return None;
        }
        self.begin("Delete text style");
        self.tx().snapshot_variables();
        let users = followers(&self.doc, name);
        for id in &users {
            if let Some(n) = self.tx().node_mut(*id) {
                n.text_style.style_ref = None;
            }
        }
        self.doc.text_styles.remove(name);
        self.commit();
        Some(users.len())
    }

    /// Make text layers follow style `name` (None: stop following, keeping the look). Returns
    /// the layers changed; non-text nodes are skipped.
    pub fn apply_text_style(&mut self, ids: &[NodeId], name: Option<&str>) -> Result<usize, String> {
        let style = match name {
            Some(n) => Some((n, self.doc.text_styles.get(n).cloned().ok_or_else(|| format!("no text style `{n}`"))?)),
            None => None,
        };
        self.begin("Apply text style");
        let mut changed = Vec::new();
        let mut tx = self.tx();
        for id in ids {
            let Some(n) = tx.node_mut(*id) else { continue };
            let ok = match &style {
                Some((name, s)) => s.apply(name, n),
                None => n.text_style.style_ref.take().is_some(),
            };
            if ok {
                changed.push(*id);
            }
        }
        for id in &changed {
            self.fit_text(*id);
        }
        self.commit();
        Ok(changed.len())
    }
}

fn followers(doc: &crate::model::Document, name: &str) -> Vec<NodeId> {
    let mut ids: Vec<NodeId> = doc
        .nodes
        .values()
        .filter(|n| n.text_style.style_ref.as_deref() == Some(name))
        .map(|n| n.id)
        .collect();
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::NodeKind;

    fn text_node(e: &mut Editor, s: &str) -> NodeId {
        let kind = NodeKind::Text { text: s.into(), font_size: 16.0, auto_width: true, align: Default::default() };
        e.add_node(crate::model::Node::new(NodeId(0), kind, Rect::new(0.0, 0.0, 50.0, 20.0)), None)
    }

    fn big() -> TypeStyle {
        TypeStyle { font_size: 40.0, font: None, bold: true, italic: false, line_height: Some(1.1), letter_spacing: 1.0 }
    }

    #[test]
    fn changing_a_style_updates_followers_in_one_undo_step() {
        let mut e = Editor::default();
        let (a, b) = (text_node(&mut e, "A"), text_node(&mut e, "B"));
        e.set_text_style("H", big());
        assert_eq!(e.apply_text_style(&[a], Some("H")), Ok(1));
        assert_eq!(e.doc.get(a).unwrap().kind.font_size(), Some(40.0));
        assert_eq!(e.doc.get(b).unwrap().kind.font_size(), Some(16.0));

        let bigger = TypeStyle { font_size: 60.0, ..big() };
        assert_eq!(e.set_text_style("H", bigger), 1);
        assert_eq!(e.doc.get(a).unwrap().kind.font_size(), Some(60.0));
        e.undo();
        assert_eq!(e.doc.get(a).unwrap().kind.font_size(), Some(40.0));
        assert_eq!(e.doc.text_styles["H"].font_size, 40.0);
        e.undo();
        e.undo();
        assert!(e.doc.text_styles.is_empty());
    }

    #[test]
    fn deleting_detaches_but_keeps_the_look_and_unknown_names_fail() {
        let mut e = Editor::default();
        let a = text_node(&mut e, "A");
        assert!(e.apply_text_style(&[a], Some("nope")).is_err());
        e.set_text_style("H", big());
        e.apply_text_style(&[a], Some("H")).unwrap();
        assert_eq!(e.delete_text_style("H"), Some(1));
        let n = e.doc.get(a).unwrap();
        assert_eq!((n.text_style.style_ref.clone(), n.kind.font_size()), (None, Some(40.0)));
        assert_eq!(e.delete_text_style("H"), None);
        e.undo();
        assert_eq!(e.doc.get(a).unwrap().text_style.style_ref.as_deref(), Some("H"));
        assert!(e.doc.text_styles.contains_key("H"));
    }
}
