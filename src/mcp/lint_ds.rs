//! `lint_design {fix}` applying a design system to existing work: literal spacing, radius and
//! font-size numbers follow the number variable with the same value, and text that matches a
//! named text style follows the style.

use crate::model::{AutoLayout, Document, Node, NodeKind};
use std::collections::BTreeMap;

/// A field bound to a number variable (CSS name) with the note for the report.
pub struct Binding {
    pub field: &'static str,
    pub token: String,
    pub note: String,
}

const SPACING_HINTS: [&str; 5] = ["space", "spacing", "gap", "pad", "inset"];
const RADIUS_HINTS: [&str; 3] = ["radius", "round", "corner"];
const FONT_HINTS: [&str; 4] = ["font", "text", "type", "fs"];

fn px_tokens(tokens: &BTreeMap<String, String>) -> Vec<(&str, f64)> {
    tokens
        .iter()
        .filter_map(|(k, v)| Some((k.as_str(), v.trim().strip_suffix("px")?.trim().parse::<f64>().ok()?)))
        .filter(|(_, v)| v.is_finite())
        .collect()
}

const HINT_GROUPS: [&[&str]; 3] = [&SPACING_HINTS, &RADIUS_HINTS, &FONT_HINTS];

fn fits(name: &str, hints: &[&str]) -> bool {
    let name = name.to_lowercase();
    hints.iter().any(|h| name.contains(h))
}

/// The variable holding `value`: one whose name fits the field, else the only one with that value
/// whose name points at no kind of field, so a gap never follows `radius-sm` because both are 8px.
fn pick<'a>(tokens: &[(&'a str, f64)], value: f64, hints: &[&str]) -> Option<&'a str> {
    let same: Vec<&str> = tokens.iter().filter(|(_, v)| (v - value).abs() < 1e-6).map(|(k, _)| *k).collect();
    if let Some(k) = same.iter().find(|k| fits(k, hints)) {
        return Some(k);
    }
    let neutral: Vec<&str> = same.into_iter().filter(|k| !HINT_GROUPS.iter().any(|g| fits(k, g))).collect();
    (neutral.len() == 1).then(|| neutral[0])
}

/// Number variables for the literal gap, padding, radius and font size of `n` (its layout as
/// it will be after the grid snap is `layout`). Fields that already follow a variable are kept.
pub fn bind_numbers(doc: &Document, n: &Node, layout: Option<&AutoLayout>, font_size: Option<f64>) -> Vec<Binding> {
    let tokens = px_tokens(&doc.tokens);
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut add = |field: &'static str, value: f64, hints: &[&str], label: &str| {
        if value <= 0.0 || n.bindings.contains_key(field) {
            return;
        }
        if let Some(t) = pick(&tokens, value, hints) {
            out.push(Binding { field, token: t.to_owned(), note: format!("{label} {value}px bound to --{t}") });
        }
    };
    if let Some(l) = layout {
        add("gap", l.gap, &SPACING_HINTS, "gap");
        let p = l.padding;
        let own = |f: &str| n.bindings.keys().any(|k| k.starts_with(f));
        if p.iter().all(|v| *v == p[0]) && !own("padding") {
            add("padding", p[0], &SPACING_HINTS, "padding");
        } else if !n.bindings.contains_key("padding") {
            for (field, v) in ["padding-top", "padding-right", "padding-bottom", "padding-left"].into_iter().zip(p) {
                add(field, v, &SPACING_HINTS, field);
            }
        }
    }
    if n.is_frame() || matches!(n.kind, NodeKind::Rect) {
        add("radius", n.style.corner_radius, &RADIUS_HINTS, "radius");
    }
    if let Some(fs) = font_size {
        add("font-size", fs, &FONT_HINTS, "font size");
    }
    out
}

/// The text style whose properties `n` already has exactly, when it follows none.
pub fn matching_text_style(doc: &Document, n: &Node) -> Option<String> {
    let NodeKind::Text { font_size, .. } = &n.kind else { return None };
    let ts = &n.text_style;
    if ts.style_ref.is_some() {
        return None;
    }
    doc.text_styles
        .iter()
        .find(|(_, s)| {
            (s.font_size - font_size).abs() < 0.01
                && s.font == ts.font
                && s.bold == ts.bold
                && s.italic == ts.italic
                && s.line_height == ts.line_height
                && (s.letter_spacing - ts.letter_spacing).abs() < 1e-6
        })
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_variable_of_another_kind_is_not_borrowed() {
        let only_radius = [("radius-sm", 8.0)];
        assert_eq!(pick(&only_radius, 8.0, &SPACING_HINTS), None, "a gap never follows radius-sm");
        assert_eq!(pick(&only_radius, 8.0, &RADIUS_HINTS), Some("radius-sm"));
        let neutral = [("radius-sm", 8.0), ("size-2", 8.0)];
        assert_eq!(pick(&neutral, 8.0, &SPACING_HINTS), Some("size-2"), "a neutral name still fits");
        let both = [("radius-sm", 8.0), ("space-2", 8.0)];
        assert_eq!(pick(&both, 8.0, &SPACING_HINTS), Some("space-2"));
    }
}
