//! `lint_design`: checks a design for the mistakes agents make most, with a concrete fix for each.
//!
//! The rules are conventions, not law: WCAG 2.x contrast, a 4px spacing grid, a type scale, 44px
//! hit areas for buttons and tabs, real layer names, and colors that should be variables. Each
//! issue says which node, what is wrong, and what to change.

use crate::model::{Color, Document, Node, NodeId, Paint};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub rule: &'static str,
    pub severity: &'static str,
    pub node: NodeId,
    pub message: String,
    pub fix: String,
}

/// Sizes the type-scale rule accepts by default.
pub const DEFAULT_SCALE: [f64; 9] = [12.0, 14.0, 16.0, 20.0, 24.0, 32.0, 40.0, 48.0, 64.0];

/// WCAG relative luminance of an sRGB color.
fn luminance(c: Color) -> f64 {
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio, 1..=21.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `over` composited on an opaque `base`.
fn blend(base: Color, over: Color) -> Color {
    let a = over.a as f64 / 255.0;
    let mix = |b: u8, o: u8| (b as f64 * (1.0 - a) + o as f64 * a).round() as u8;
    Color::rgb(
        mix(base.r, over.r),
        mix(base.g, over.g),
        mix(base.b, over.b),
    )
}

fn solid(n: &Node) -> Option<(Color, Option<&str>)> {
    match n.style.fills.last() {
        Some(Paint::Solid { color, var }) if color.a > 0 => Some((*color, var.as_deref())),
        _ => None,
    }
}

/// The color text sits on: the nearest opaque fill up the tree (translucent ones blend over
/// what is behind them); white when nothing is filled.
fn backdrop(doc: &Document, id: NodeId) -> Color {
    let mut layers: Vec<Color> = Vec::new();
    let mut cur = doc.get(id).and_then(|n| n.parent);
    while let Some(p) = cur {
        let Some(n) = doc.get(p) else { break };
        if n.visible
            && let Some((c, _)) = solid(n)
        {
            layers.push(c);
            if c.a == 255 {
                break;
            }
        }
        cur = n.parent;
    }
    layers
        .into_iter()
        .rev()
        .fold(Color::rgb(255, 255, 255), blend)
}

fn is_default_name(name: &str) -> bool {
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ');
    matches!(
        base,
        "Frame"
            | "Rectangle"
            | "Ellipse"
            | "Group"
            | "Path"
            | "Line"
            | "Arrow"
            | "Layer"
            | "Vector"
            | "Polygon"
            | "Star"
            | "Diamond"
            | "Triangle"
            | "Image"
            | "Icon"
    ) && name.len() < 14
}

fn off_grid(v: f64) -> bool {
    v > 0.0 && (v / 4.0 - (v / 4.0).round()).abs() > 1e-6
}

/// Lint the subtrees under `roots` (instances are skipped: their component is linted instead).
pub fn lint(doc: &Document, roots: &[NodeId], scale: &[f64]) -> Vec<Issue> {
    let mut out: Vec<Issue> = Vec::new();
    let mut ids: Vec<NodeId> = Vec::new();
    for r in roots {
        ids.extend(doc.subtree(*r));
    }
    let vars: BTreeMap<String, Color> = doc.variables.clone();
    // Colors used literally, with the first node using each, for the variables report.
    let mut literal: BTreeMap<String, (usize, NodeId)> = BTreeMap::new();
    for id in ids {
        let Some(n) = doc.get(id) else { continue };
        if !n.visible || n.instance.is_some() {
            continue;
        }
        let mut add = |rule: &'static str, severity: &'static str, message: String, fix: String| {
            out.push(Issue {
                rule,
                severity,
                node: id,
                message,
                fix,
            });
        };
        // ---- text
        if let (Some(size), Some(_)) = (n.kind.font_size(), n.kind.text()) {
            if let Some((fg, _)) = solid(n) {
                let bg = backdrop(doc, id);
                let fg = blend(bg, fg);
                let ratio = contrast(fg, bg);
                let large = size >= 24.0 || (size >= 18.66 && n.text_style.bold);
                let need = if large { 3.0 } else { 4.5 };
                if ratio < need {
                    add(
                        "contrast",
                        "error",
                        format!(
                            "text {} on {} is {ratio:.1}:1 (needs {need}:1)",
                            fg.to_hex(),
                            bg.to_hex()
                        ),
                        "use a lighter or darker text color, or change the background".into(),
                    );
                }
            }
            if size < 12.0 {
                add(
                    "text-size",
                    "warning",
                    format!("{size}px text is hard to read"),
                    "use 12px or more (14px+ for body copy)".into(),
                );
            } else if !scale.is_empty() && !scale.iter().any(|s| (s - size).abs() < 0.01) {
                let near = scale
                    .iter()
                    .min_by(|a, b| (*a - size).abs().total_cmp(&(*b - size).abs()))
                    .copied()
                    .unwrap_or(size);
                add(
                    "type-scale",
                    "info",
                    format!("{size}px is not on the type scale"),
                    format!("use {near}px (scale: {})", join(scale)),
                );
            }
        }
        // ---- spacing on the 4px grid
        if let Some(l) = &n.layout {
            let mut bad: Vec<String> = Vec::new();
            if off_grid(l.gap) {
                bad.push(format!("gap {}", l.gap));
            }
            for (side, p) in ["top", "right", "bottom", "left"].iter().zip(l.padding) {
                if off_grid(p) {
                    bad.push(format!("padding-{side} {p}"));
                }
            }
            if !bad.is_empty() {
                add(
                    "spacing-grid",
                    "warning",
                    format!("off the 4px grid: {}", bad.join(", ")),
                    "use multiples of 4 (4, 8, 12, 16, 24, 32, 48)".into(),
                );
            }
        }
        // ---- hit areas
        let lower = n.name.to_lowercase();
        let tap = lower.starts_with("button")
            || lower.starts_with("tab /")
            || lower.starts_with("tab bar /");
        if tap && n.is_frame() && n.rect.h < 44.0 && n.rect.h > 0.0 && !lower.contains("icon") {
            add(
                "touch-target",
                "warning",
                format!("`{}` is {:.0}px tall", n.name, n.rect.h),
                "make interactive targets at least 44px tall (padding, not just text size)".into(),
            );
        }
        // ---- names
        if !n.component
            && n.kind.text().is_none()
            && is_default_name(&n.name)
            && !doc.children(Some(id)).is_empty()
        {
            add(
                "layer-name",
                "warning",
                format!("`{}` is a default name", n.name),
                "name it for what it is (layer-name=\"Task Row\"), `Component / Variant` for parts"
                    .into(),
            );
        }
        // ---- colors that should be variables
        if let Some((c, var)) = solid(n)
            && var.is_none()
            && c.a == 255
        {
            if let Some((name, _)) = vars.iter().find(|(_, v)| **v == c) {
                add(
                    "hard-coded-color",
                    "warning",
                    format!("{} is the value of --{name}", c.to_hex()),
                    format!("write var(--{name}) so a theme change reaches it"),
                );
            } else if !vars.is_empty() {
                let e = literal.entry(c.to_hex()).or_insert((0, id));
                e.0 += 1;
            }
        }
    }
    for (hex, (count, first)) in literal {
        out.push(Issue {
            rule: "hard-coded-color",
            severity: "info",
            node: first,
            message: format!("{hex} is used {count}× and is not a variable"),
            fix: "add it with set_variables, or use an existing variable".into(),
        });
    }
    out
}

fn join(v: &[f64]) -> String {
    v.iter()
        .map(|s| format!("{s}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast(Color::rgb(0, 0, 0), Color::rgb(255, 255, 255)) - 21.0).abs() < 0.01);
        assert!(
            (contrast(Color::rgb(119, 119, 119), Color::rgb(255, 255, 255)) - 4.48).abs() < 0.05
        );
        assert!(off_grid(13.0) && !off_grid(12.0) && !off_grid(0.0));
        assert!(is_default_name("Frame 12") && !is_default_name("Task Row"));
    }
}
