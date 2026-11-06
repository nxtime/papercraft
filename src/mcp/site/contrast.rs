//! Text that misses WCAG AA contrast. The colours are the designer's, so the export only says so.

use super::model::Frame;
use crate::mcp::lint::text_contrast;
use crate::model::{Document, NodeKind};

/// One warning per visible text layer of the frames below AA, in every locale's document.
/// `labels` name the locales; they are left out of the message with a single locale.
pub fn warnings(docs: &[Document], frames: &[Frame], labels: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        for f in frames {
            for id in doc.subtree(f.id) {
                let Some(n) = doc.get(id).filter(|n| n.visible) else {
                    continue;
                };
                if !matches!(n.kind, NodeKind::Text { .. }) {
                    continue;
                }
                let Some((fg, bg, ratio, need)) = text_contrast(doc, id) else {
                    continue;
                };
                if ratio < need {
                    let mode = if docs.len() > 1 {
                        format!(" ({})", labels[i])
                    } else {
                        String::new()
                    };
                    out.push(format!(
                        "{} / {}{mode}: text {} on {} is {ratio:.2}:1, below WCAG AA ({need}:1); change a colour in the design",
                        f.name,
                        n.name,
                        fg.to_hex(),
                        bg.to_hex()
                    ));
                }
            }
        }
    }
    out
}
