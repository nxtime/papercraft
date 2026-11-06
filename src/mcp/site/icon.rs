//! The site icon: the layer named Favicon, written as the page icon.

use super::media::Media;
use crate::mcp::codegen;
use crate::model::{Document, NodeKind, Paint};

/// The path of the icon file for the first visible layer named `favicon` (any case) on the page:
/// the picture itself when it is one image, else the layer drawn as SVG. None when there is no
/// such layer or it cannot be drawn.
pub fn favicon(media: &mut Media, doc: &Document) -> Option<String> {
    let id = doc.root.iter().flat_map(|r| doc.subtree(*r)).find(|id| {
        doc.get(*id)
            .is_some_and(|n| n.visible && n.name.trim().eq_ignore_ascii_case("favicon"))
    })?;
    let n = doc.get(id)?;
    if let [Paint::Image { asset, .. }] = n.style.fills.as_slice()
        && !matches!(n.kind, NodeKind::Text { .. })
    {
        return media.image(asset, 0.0);
    }
    let svg = codegen::inline_svg(doc, id);
    svg.contains("<svg").then(|| media.svg(&svg))
}
