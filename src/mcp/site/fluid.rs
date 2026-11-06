//! What lets a page narrower than its design reflow instead of clipping: fixed widths that give
//! way, rows that wrap, big type that scales, pictures that keep their proportions. Only layers
//! in an auto-layout flow qualify: absolutely positioned ones cannot reflow (the lint warns).

use super::style::Decls;
use crate::mcp::codegen;
use crate::mcp::lint::DESKTOP_MIN_W;
use crate::model::{Direction, Document, Node, NodeKind, Sizing};

/// Type from this size up shrinks with the window; body text keeps its size.
const FLUID_TEXT_PX: f64 = 40.0;
/// Slack for the rounding of rects.
const FIT_SLACK_PX: f64 = 0.5;
/// A fixed-width child this wide is a card or column, not an icon or a button.
const COLUMN_MIN_PX: f64 = 120.0;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The extra declarations of `n`: at the designed width and wider each one is a no-op.
pub fn decls(doc: &Document, n: &Node, replaced: bool) -> Decls {
    let mut out = Decls::new();
    let Some(parent) = codegen::parent_layout(doc, n) else {
        return out;
    };
    if n.layout_child.absolute {
        return out;
    }
    let fixed_w = n.layout_child.sizing[0] == Sizing::Fixed
        && !matches!(
            n.kind,
            NodeKind::Text {
                auto_width: true,
                ..
            }
        );
    // A layer wider than its parent's content box overflows on purpose (a bleed, a strip in a
    // clipped frame): shrinking it would change the design.
    let room = n
        .parent
        .and_then(|p| doc.get(p))
        .map_or(0.0, |p| p.rect.w - parent.padding[1] - parent.padding[3]);
    if fixed_w && n.layout_child.max_size[0].is_none() && n.rect.w <= room + FIT_SLACK_PX {
        out.push(("max-width".into(), "100%".into()));
    }
    if replaced
        && fixed_w
        && n.rect.w <= room + FIT_SLACK_PX
        && n.layout_child.sizing[1] == Sizing::Fixed
        && n.rect.h >= 1.0
    {
        out.push(("height".into(), "auto".into()));
        out.push((
            "aspect-ratio".into(),
            format!("{} / {}", r2(n.rect.w), r2(n.rect.h)),
        ));
    }
    if wraps(doc, n) {
        out.push(("flex-wrap".into(), "wrap".into()));
    }
    if let NodeKind::Text { font_size, .. } = &n.kind
        && *font_size >= FLUID_TEXT_PX
    {
        out.push((
            "font-size".into(),
            fluid_size(*font_size, full_size_width(doc, n)),
        ));
        if let Some(lh) = n.text_style.line_height {
            // A pixel line height would stay as tall as the big type it was drawn for.
            out.push(("line-height".into(), format!("{}", r2(lh))));
        }
    }
    out
}

/// The Tailwind classes of `decls`, for the properties `tw_classes` has no hook for.
pub fn tailwind(decls: &Decls) -> Vec<String> {
    decls
        .iter()
        .filter_map(|(k, v)| match k.as_str() {
            "max-width" => Some("max-w-full".into()),
            "height" => Some("h-auto".into()),
            "flex-wrap" => Some("flex-wrap".into()),
            "aspect-ratio" => Some(format!("aspect-[{}]", v.replace(' ', ""))),
            "font-size" => Some(format!("text-[length:{v}]").replace(' ', "")),
            "line-height" => Some(format!("leading-[{v}]")),
            _ => None,
        })
        .collect()
}

/// The window width from which type keeps its designed size: the page frame's own width, at most
/// a desktop's, so a phone design is not shrunk on the phone it was drawn for.
fn full_size_width(doc: &Document, n: &Node) -> f64 {
    let page = doc
        .ancestors(n.id)
        .last()
        .and_then(|p| doc.get(*p))
        .unwrap_or(n);
    page.rect.w.min(DESKTOP_MIN_W)
}

/// `clamp(half, designed size at `full` px, designed size)`.
pub fn fluid_size(px: f64, full: f64) -> String {
    format!(
        "clamp({}px, {}vw, {}px)",
        r2(px / 2.0),
        r2(px / full * 100.0),
        r2(px)
    )
}

/// A row of two or more fixed-width cards or columns may wrap. Rows with a child that fills the
/// rest, icon + label rows and rows that already wrap stay as designed: they fit at any width
/// the design does, and wrapping them would only risk a break from font differences.
fn wraps(doc: &Document, n: &Node) -> bool {
    let Some(l) = n
        .layout
        .as_ref()
        .filter(|l| !l.wrap && l.direction == Direction::Horizontal)
    else {
        return false;
    };
    let kids: Vec<&Node> = n
        .children
        .iter()
        .filter_map(|c| doc.get(*c))
        .filter(|c| !c.layout_child.absolute)
        .collect();
    let columns = kids
        .iter()
        .filter(|c| {
            c.layout_child.sizing[0] == Sizing::Fixed
                && !matches!(c.kind, NodeKind::Text { .. })
                && c.rect.w >= COLUMN_MIN_PX
        })
        .count();
    let gap = if l.gap_auto { 0.0 } else { l.gap };
    let sum: f64 = kids.iter().map(|c| c.rect.w).sum::<f64>() + gap * (kids.len() as f64 - 1.0);
    // A carousel that overflows its row by design must not stack.
    let fits = sum <= n.rect.w - l.padding[1] - l.padding[3] + FIT_SLACK_PX;
    fits && columns >= 2
        && kids
            .iter()
            .all(|c| c.layout_child.sizing[0] != Sizing::Fill)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fluid_size_is_the_designed_one_from_a_desktop_up() {
        assert_eq!(fluid_size(56.0, 1024.0), "clamp(28px, 5.47vw, 56px)");
        assert_eq!(fluid_size(80.0, 1024.0), "clamp(40px, 7.81vw, 80px)");
    }

    #[test]
    fn tailwind_names_the_fluid_declarations() {
        let d: Decls = vec![
            ("max-width".into(), "100%".into()),
            ("aspect-ratio".into(), "300 / 200".into()),
            ("font-size".into(), fluid_size(56.0, 1024.0)),
        ];
        assert_eq!(
            tailwind(&d),
            [
                "max-w-full",
                "aspect-[300/200]",
                "text-[length:clamp(28px,5.47vw,56px)]"
            ]
        );
    }
}
