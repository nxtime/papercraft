//! Named type styles: the text properties a layer takes from "Heading / L" or "Body / M".

use super::node::{Node, NodeKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeStyle {
    pub font_size: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    #[serde(default)]
    pub letter_spacing: f64,
}

impl TypeStyle {
    /// Copy the properties onto a text layer and record the reference. A style change calls
    /// this again for every layer that follows it, so local edits of these fields are replaced;
    /// character spans (partial bold, colors) are kept. Returns false for non-text nodes.
    pub fn apply(&self, name: &str, n: &mut Node) -> bool {
        let NodeKind::Text { font_size, .. } = &mut n.kind else {
            return false;
        };
        *font_size = self.font_size;
        let ts = &mut n.text_style;
        ts.font = self.font.clone();
        ts.bold = self.bold;
        ts.italic = self.italic;
        ts.line_height = self.line_height;
        ts.letter_spacing = self.letter_spacing;
        ts.style_ref = Some(name.to_string());
        true
    }
}
