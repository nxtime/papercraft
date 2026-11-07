//! Breakpoint preview: a frame shown at another width on a throwaway copy of the document, using
//! its constraints and auto layout. The real document and its undo history are never touched.

use crate::geom::Rect;
use crate::history::{Recording, Tx};
use crate::model::{Document, NodeId, NodeKind, Sizing};

/// Widths offered for every frame, as `[mobile, tablet, desktop]`.
pub const PRESETS: [u32; 3] = [375, 768, 1280];

/// The preview window: which frame, at which width, and the last render.
#[derive(Debug, Clone)]
pub struct Preview {
    pub frame: NodeId,
    pub width: u32,
    /// Bumped on every new render so the UI knows to rebuild its texture.
    pub serial: u64,
    pub image: Option<image::RgbaImage>,
}

/// Labelled widths for `frame`: the presets, then the widths of its `Name @variant` siblings
/// (the frames `export_site` merges into one page).
pub fn choices(doc: &Document, frame: NodeId) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = PRESETS.iter().map(|w| (w.to_string(), *w)).collect();
    let Some(n) = doc.get(frame) else { return out };
    let base = n.name.split(" @").next().unwrap_or("").trim().to_lowercase();
    let siblings = match n.parent.and_then(|p| doc.get(p)) {
        Some(p) => p.children.clone(),
        None => doc.root.clone(),
    };
    for s in siblings {
        let Some(v) = doc.get(s) else { continue };
        let Some((b, suffix)) = v.name.split_once(" @") else { continue };
        let w = v.rect.w.round() as u32;
        if b.trim().to_lowercase() == base && w > 0 && !out.iter().any(|c| c.1 == w) {
            out.push((format!("{} ({w})", suffix.trim()), w));
        }
    }
    out
}

/// A copy of `doc` where `frame` is `width` wide and its content has followed. Returns the copy
/// and the frame's id in it, or `None` when `frame` is not a frame.
pub fn at_width(doc: &Document, frame: NodeId, width: f64) -> Option<(Document, NodeId)> {
    let old = doc.get(frame).filter(|n| matches!(n.kind, NodeKind::Frame { .. }))?.rect;
    let mut copy = doc.clone();
    let mut rec = Recording::new("breakpoint preview", &copy);
    let mut tx = Tx::new(&mut copy, &mut rec);
    let new = Rect::new(old.x, old.y, width.max(1.0), old.h);
    if let Some(n) = tx.node_mut(frame) {
        n.rect = new;
        n.layout_child.sizing[0] = Sizing::Fixed;
    }
    crate::layout::resize_children(&mut tx, frame, old, new);
    crate::layout::layout_frame(&mut tx, frame);
    Some((copy, frame))
}

impl super::Editor {
    /// Open the preview for the screen that holds the selection.
    pub fn open_breakpoint_preview(&mut self) {
        // The outermost frame is the screen; its nested frames are parts of it.
        let frame = self.selection.iter().find_map(|id| {
            std::iter::once(*id)
                .chain(self.doc.ancestors(*id))
                .filter(|a| self.doc.get(*a).is_some_and(|n| matches!(n.kind, NodeKind::Frame { .. })))
                .next_back()
        });
        let Some(frame) = frame else {
            self.status = "Select a frame to preview at other widths".into();
            return;
        };
        let width = self.doc.get(frame).map_or(PRESETS[0], |n| n.rect.w.round() as u32);
        self.breakpoint = Some(Preview { frame, width, serial: 0, image: None });
        self.requests.push(super::AppRequest::PreviewBreakpoint);
    }

    /// Render the preview frame at its width into `breakpoint.image`.
    pub fn render_breakpoint(&mut self, gpu: &crate::render::gpu::Headless) {
        let Some(p) = &self.breakpoint else { return };
        let (frame, width) = (p.frame, p.width);
        let Some((copy, id)) = at_width(&self.doc, frame, f64::from(width)) else {
            self.status = "The previewed frame is gone".into();
            self.breakpoint = None;
            return;
        };
        match crate::io::png::render(gpu, &copy, Some(&[id]), 1.0, Some(crate::model::Color::WHITE)) {
            Ok(img) => {
                if let Some(p) = &mut self.breakpoint {
                    p.serial += 1;
                    p.image = Some(img);
                }
            }
            Err(e) => self.status = e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Constraint, Constraints, Node};

    fn frame(doc: &mut Document, name: &str, w: f64) -> NodeId {
        let id = doc.alloc_id();
        let mut n = Node::new(id, NodeKind::Frame { clip: false }, Rect::new(0.0, 0.0, w, 200.0));
        n.name = name.into();
        doc.nodes.insert(id, n);
        doc.root.push(id);
        id
    }

    #[test]
    fn narrowing_a_frame_moves_pinned_content_and_leaves_the_document_alone() {
        let mut doc = Document::default();
        let f = frame(&mut doc, "Home", 1280.0);
        let k = doc.alloc_id();
        let mut kid = Node::new(k, NodeKind::Rect, Rect::new(1200.0, 10.0, 60.0, 20.0));
        kid.layout_child.constraints = Constraints { h: Constraint::Max, v: Constraint::Min };
        kid.parent = Some(f);
        doc.nodes.insert(k, kid);
        doc.nodes.get_mut(&f).unwrap().children.push(k);
        let before = doc.clone();

        let (copy, id) = at_width(&doc, f, 375.0).unwrap();

        assert_eq!(copy.get(id).unwrap().rect.w, 375.0);
        assert_eq!(copy.get(k).unwrap().rect.x, 295.0, "pinned to the right edge");
        assert_eq!(doc.get(f).unwrap().rect, before.get(f).unwrap().rect);
        assert_eq!(doc.get(k).unwrap().rect, before.get(k).unwrap().rect);
        assert_eq!(doc.next_id, before.next_id);
    }

    #[test]
    fn choices_add_the_widths_of_variant_frames_and_skip_duplicates() {
        let mut doc = Document::default();
        let home = frame(&mut doc, "Home", 1280.0);
        frame(&mut doc, "Home @mobile", 390.0);
        frame(&mut doc, "Home @768", 768.0);
        frame(&mut doc, "About @mobile", 300.0);
        let c = choices(&doc, home);
        let widths: Vec<u32> = c.iter().map(|x| x.1).collect();
        assert_eq!(widths, vec![375, 768, 1280, 390]);
        assert_eq!(c[3].0, "mobile (390)");
    }

    #[test]
    fn only_frames_can_be_previewed() {
        let mut doc = Document::default();
        let id = doc.alloc_id();
        doc.nodes.insert(id, Node::new(id, NodeKind::Rect, Rect::new(0.0, 0.0, 10.0, 10.0)));
        doc.root.push(id);
        assert!(at_width(&doc, id, 375.0).is_none());
    }
}
