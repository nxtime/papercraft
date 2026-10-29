//! Code generation (Tailwind JSX, HTML+CSS, design token formats).

use super::css;
use crate::model::{
    Align, AutoLayout, Direction, Document, Node, NodeId, NodeKind, Paint, Sizing, TextAlign,
};
use crate::variables::{DesignSystem, Kind, PageModes, Value as DsValue};
use serde_json::{Map, Value, json};

fn parent_layout(doc: &Document, n: &Node) -> Option<AutoLayout> {
    n.parent.and_then(|p| doc.get(p)).and_then(|p| p.layout)
}

/// `(set, name)` of an icon frame: a frame called `Icon / <set>:<name>` whose children are all vector
/// geometry (what `<x-paper-icon>` and the icon tools create).
fn icon_of(doc: &Document, n: &Node) -> Option<(String, String)> {
    if !matches!(n.kind, NodeKind::Frame { .. }) || n.children.is_empty() {
        return None;
    }
    let (set, name) = n.name.strip_prefix("Icon / ")?.split_once(':')?;
    fn only_geometry(doc: &Document, id: NodeId) -> bool {
        doc.get(id).is_some_and(|c| match c.kind {
            NodeKind::Text { .. } | NodeKind::Sticky { .. } => false,
            NodeKind::Frame { .. } | NodeKind::Group => {
                !c.children.is_empty() && c.children.iter().all(|k| only_geometry(doc, *k))
            }
            _ => true,
        })
    }
    let geometry = n.children.iter().all(|c| only_geometry(doc, *c));
    geometry.then(|| (set.to_string(), name.to_string()))
}

/// The tint of an icon: the stroke (or solid fill) of its first path, and the document variable that has
/// the same value, if any (icon strokes do not follow variables, so the match is a best guess).
fn icon_tint(doc: &Document, n: &Node) -> (Option<String>, Option<String>) {
    fn first_geometry<'a>(doc: &'a Document, n: &'a Node) -> Option<&'a Node> {
        n.children.iter().filter_map(|c| doc.get(*c)).find_map(|c| {
            if matches!(c.kind, NodeKind::Frame { .. } | NodeKind::Group) {
                first_geometry(doc, c)
            } else {
                Some(c)
            }
        })
    }
    let first = first_geometry(doc, n);
    let hex = first.and_then(|c| {
        c.style
            .stroke
            .filter(|_| c.style.stroke_width > 0.0)
            .map(|s| s.to_hex())
            .or_else(|| match c.style.fills.last() {
                Some(Paint::Solid { color, .. }) => Some(color.to_hex()),
                _ => None,
            })
    });
    let var = hex.as_ref().and_then(|h| {
        doc.variables
            .iter()
            .find(|(_, c)| &c.to_hex() == h)
            .map(|(k, _)| format!("--{k}"))
    });
    (hex, var)
}

/// Inline SVG of exactly this node (and its descendants), without the XML prolog. The export is in
/// world coordinates; the result is shifted so its `viewBox` starts at 0 0 and can be placed anywhere.
fn inline_svg(doc: &Document, id: NodeId) -> String {
    let svg = crate::io::svg::export(doc, Some(&[id]), None)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with("<?xml"))
        .collect::<Vec<_>>()
        .join("");
    normalize_svg_origin(&svg)
}

fn normalize_svg_origin(svg: &str) -> String {
    let Some(vb) = svg.find("viewBox=\"") else {
        return svg.to_string();
    };
    let start = vb + "viewBox=\"".len();
    let Some(len) = svg[start..].find('"') else {
        return svg.to_string();
    };
    let nums: Vec<f64> = svg[start..start + len]
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    let [x, y, w, h] = nums[..] else {
        return svg.to_string();
    };
    if x == 0.0 && y == 0.0 {
        return svg.to_string();
    }
    let Some(open_end) = svg.find('>') else {
        return svg.to_string();
    };
    let Some(close) = svg.rfind("</svg>") else {
        return svg.to_string();
    };
    let head = format!(
        "{}viewBox=\"0 0 {} {}\"{}",
        &svg[..vb],
        px_num(w),
        px_num(h),
        &svg[start + len + 1..=open_end]
    );
    format!(
        "{head}<g transform=\"translate({} {})\">{}</g></svg>",
        px_num(-x),
        px_num(-y),
        &svg[open_end + 1..close]
    )
}

/// The `--token` whose value is `px` pixels, among tokens whose name starts with one of `prefixes`.
fn token_for(doc: &Document, px: f64, prefixes: &[&str]) -> Option<String> {
    let want = format!("{}px", px_num(px));
    doc.tokens
        .iter()
        .find(|(k, v)| v.trim() == want && prefixes.iter().any(|p| k.starts_with(p)))
        .map(|(k, _)| format!("--{k}"))
}

fn px_num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.trunc()).abs() < 1e-9 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn tw_space(prefix: &str, px: f64) -> String {
    let r = (px * 100.0).round() / 100.0;
    let scale = match (r * 10.0).round() as i64 {
        0 => Some("0"),
        20 => Some("0.5"),
        40 => Some("1"),
        60 => Some("1.5"),
        80 => Some("2"),
        100 => Some("2.5"),
        120 => Some("3"),
        140 => Some("3.5"),
        160 => Some("4"),
        200 => Some("5"),
        240 => Some("6"),
        280 => Some("7"),
        320 => Some("8"),
        360 => Some("9"),
        400 => Some("10"),
        440 => Some("11"),
        480 => Some("12"),
        560 => Some("14"),
        640 => Some("16"),
        _ => None,
    };
    match scale {
        Some(s) => format!("{prefix}-{s}"),
        None => format!("{prefix}-[{}px]", px_num(px)),
    }
}

fn tw_padding(p: [f64; 4], classes: &mut Vec<String>) {
    let [pt, pr, pb, pl] = p;
    if pt == 0.0 && pr == 0.0 && pb == 0.0 && pl == 0.0 {
        return;
    }
    if (pt - pr).abs() < 1e-6 && (pr - pb).abs() < 1e-6 && (pb - pl).abs() < 1e-6 {
        classes.push(tw_space("p", pt));
    } else if (pt - pb).abs() < 1e-6 && (pr - pl).abs() < 1e-6 {
        if pt > 0.0 {
            classes.push(tw_space("py", pt));
        }
        if pr > 0.0 {
            classes.push(tw_space("px", pr));
        }
    } else {
        if pt > 0.0 {
            classes.push(tw_space("pt", pt));
        }
        if pr > 0.0 {
            classes.push(tw_space("pr", pr));
        }
        if pb > 0.0 {
            classes.push(tw_space("pb", pb));
        }
        if pl > 0.0 {
            classes.push(tw_space("pl", pl));
        }
    }
}

fn tw_radius(r: f64) -> Option<String> {
    if r <= 0.0 {
        return None;
    }
    if r >= 999.0 {
        return Some("rounded-full".into());
    }
    let rounded = match r.round() as i64 {
        4 if (r - 4.0).abs() < 1e-6 => "rounded",
        6 if (r - 6.0).abs() < 1e-6 => "rounded-md",
        8 if (r - 8.0).abs() < 1e-6 => "rounded-lg",
        12 if (r - 12.0).abs() < 1e-6 => "rounded-xl",
        16 if (r - 16.0).abs() < 1e-6 => "rounded-2xl",
        _ => return Some(format!("rounded-[{}px]", px_num(r))),
    };
    Some(rounded.into())
}

fn tw_font_size(s: f64) -> String {
    let named = match s.round() as i64 {
        12 if (s - 12.0).abs() < 1e-6 => Some("text-xs"),
        14 if (s - 14.0).abs() < 1e-6 => Some("text-sm"),
        16 if (s - 16.0).abs() < 1e-6 => Some("text-base"),
        18 if (s - 18.0).abs() < 1e-6 => Some("text-lg"),
        20 if (s - 20.0).abs() < 1e-6 => Some("text-xl"),
        24 if (s - 24.0).abs() < 1e-6 => Some("text-2xl"),
        32 if (s - 32.0).abs() < 1e-6 => Some("text-3xl"),
        _ => None,
    };
    match named {
        Some(n) => n.into(),
        None => format!("text-[{}px]", px_num(s)),
    }
}

fn tw_classes(doc: &Document, n: &Node, is_root: bool) -> String {
    let mut c: Vec<String> = Vec::new();
    let pl = parent_layout(doc, n);
    let in_flow = pl.is_some() && !n.layout_child.absolute;
    if !n.visible {
        c.push("hidden".into());
    }
    if !in_flow && !is_root {
        c.push("absolute".into());
        c.push(format!("left-[{}px]", px_num(n.rect.x)));
        c.push(format!("top-[{}px]", px_num(n.rect.y)));
    }
    let is_auto_text = matches!(
        n.kind,
        NodeKind::Text {
            auto_width: true,
            ..
        }
    );
    let is_text = matches!(n.kind, NodeKind::Text { .. });
    match n.layout_child.sizing[0] {
        Sizing::Fill => {
            if pl.is_some_and(|l| l.direction == Direction::Horizontal) && in_flow {
                c.push("flex-1".into());
            }
            c.push("w-full".into());
        }
        Sizing::Hug => {
            if !is_auto_text {
                c.push("w-fit".into());
            }
        }
        Sizing::Fixed => {
            if !is_auto_text {
                c.push(format!("w-[{}px]", px_num(n.rect.w)));
            }
        }
    }
    match n.layout_child.sizing[1] {
        Sizing::Fill => {
            if pl.is_some_and(|l| l.direction == Direction::Vertical) && in_flow {
                c.push("flex-1".into());
            }
            c.push("h-full".into());
        }
        Sizing::Hug => {
            if !is_text {
                c.push("h-fit".into());
            }
        }
        Sizing::Fixed => {
            if !is_text {
                c.push(format!("h-[{}px]", px_num(n.rect.h)));
            }
        }
    }
    if let Some(l) = &n.layout {
        c.push("flex".into());
        c.push(match l.direction {
            Direction::Horizontal => "flex-row".into(),
            Direction::Vertical => "flex-col".into(),
        });
        if l.wrap {
            c.push("flex-wrap".into());
        }
        if l.gap_auto {
            c.push("justify-between".into());
        } else {
            c.push(match l.align_main {
                Align::Start => "justify-start".into(),
                Align::Center => "justify-center".into(),
                Align::End => "justify-end".into(),
            });
            if l.gap > 0.0 {
                c.push(tw_space("gap", l.gap));
            }
        }
        c.push(match l.align_cross {
            Align::Start => "items-start".into(),
            Align::Center => "items-center".into(),
            Align::End => "items-end".into(),
        });
        tw_padding(l.padding, &mut c);
    }
    if matches!(n.kind, NodeKind::Frame { clip: true }) {
        c.push(if n.scroll { "overflow-y-auto" } else { "overflow-hidden" }.into());
    }
    match &n.kind {
        NodeKind::Text {
            font_size, align, ..
        } => {
            c.push(tw_font_size(*font_size));
            if n.text_style.bold {
                c.push("font-bold".into());
            }
            if n.text_style.italic {
                c.push("italic".into());
            }
            c.push(match align {
                TextAlign::Left => "text-left".into(),
                TextAlign::Center => "text-center".into(),
                TextAlign::Right => "text-right".into(),
            });
            if let Some(Paint::Solid { color, var }) = n.style.fills.last() {
                match var {
                    Some(v) => c.push(format!("text-[var(--{v})]")),
                    None => c.push(format!("text-[{}]", color.to_hex())),
                }
            }
        }
        _ => {
            if let Some(Paint::Solid { color, var }) = n.style.fills.last() {
                match var {
                    Some(v) => c.push(format!("bg-[var(--{v})]")),
                    None => c.push(format!("bg-[{}]", color.to_hex())),
                }
            }
        }
    }
    if let Some(stroke) = n.style.stroke
        && n.style.stroke_width > 0.0
    {
        if (n.style.stroke_width - 1.0).abs() < 1e-6 {
            c.push("border".into());
        } else {
            c.push(format!("border-[{}px]", px_num(n.style.stroke_width)));
        }
        c.push(format!("border-[{}]", stroke.to_hex()));
    }
    if matches!(n.kind, NodeKind::Ellipse) {
        c.push("rounded-full".into());
    } else if let Some(r) = tw_radius(n.style.corner_radius) {
        c.push(r);
    }
    if let Some(sh) = n.style.shadow {
        c.push(format!(
            "shadow-[{}px_{}px_{}px_{}px_{}]",
            px_num(sh.x),
            px_num(sh.y),
            px_num(sh.blur),
            px_num(sh.spread),
            sh.color.to_hex()
        ));
    }
    if n.style.opacity < 1.0 {
        let op = (n.style.opacity * 1000.0).round() / 1000.0;
        c.push(format!("opacity-[{op}]"));
    }
    c.join(" ")
}

fn jsx_text(t: &str) -> String {
    if t.chars()
        .all(|c| !matches!(c, '{' | '}' | '<' | '>' | '\n' | '"' | '\\'))
    {
        t.to_string()
    } else {
        format!("{{{}}}", serde_json::to_string(t).unwrap_or_default())
    }
}

fn instance_comp_attr(doc: &Document, n: &Node) -> String {
    n.instance
        .as_ref()
        .and_then(|l| doc.get(l.main))
        .filter(|m| m.component)
        .map(|m| format!(" data-component=\"{}\"", m.name.replace('"', "'")))
        .unwrap_or_default()
}

/// The code link of a node: its own (a main component) or its main component's (an instance).
pub(crate) fn code_link<'a>(doc: &'a Document, n: &'a Node) -> Option<&'a crate::model::CodeLink> {
    n.code.as_ref().or_else(|| n.instance.as_ref().and_then(|l| doc.get(l.target().0)).and_then(|m| m.code.as_ref()))
}

/// A code link template with `{Name}` replaced by the instance's property `Name`, else the text
/// of the layer named `Name` under `id` (overrides included). An unknown name stays as written.
fn fill_template(doc: &Document, id: NodeId, template: &str, escape: fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        let Some(j) = rest[i..].find('}').map(|j| i + j) else { break };
        let name = &rest[i + 1..j];
        // A property value of the instance first (`{Status}`), else a layer's text (`{Label}`).
        let text = (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || " _-".contains(c)))
            .then(|| {
                doc.get(id).and_then(|n| n.prop_values.get(name).cloned()).or_else(|| doc.subtree(id).into_iter().filter_map(|d| doc.get(d)).find(|d| d.name == name).and_then(|d| d.kind.text().map(|t| t.to_string())))
            })
            .flatten();
        out.push_str(&rest[..i]);
        match text {
            Some(t) => out.push_str(&escape(&t)),
            None => out.push_str(&rest[i..=j]),
        }
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    out
}

/// A linked component's JSX, filled in from the instance (None when it has no React link).
pub(crate) fn linked_react(doc: &Document, id: NodeId) -> Option<String> {
    let n = doc.get(id)?;
    code_link(doc, n).filter(|l| !l.react.is_empty()).map(|l| fill_template(doc, id, &l.react, jsx_text))
}

/// The distinct import lines of the linked components under `root`, in order of first use.
pub(crate) fn code_imports(doc: &Document, root: NodeId) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in doc.subtree(root) {
        if let Some(link) = doc.get(id).and_then(|n| code_link(doc, n))
            && !link.import.is_empty()
            && !out.contains(&link.import)
        {
            out.push(link.import.clone());
        }
    }
    out
}

/// Each line of a (possibly multi-line) snippet, indented.
pub(crate) fn push_indented(out: &mut String, ind: &str, snippet: &str) {
    for line in snippet.lines() {
        out.push_str(&format!("{ind}{line}\n"));
    }
}

fn tailwind_node(doc: &Document, id: NodeId, depth: usize, is_root: bool, out: &mut String) {
    let Some(n) = doc.get(id) else { return };
    let ind = "  ".repeat(depth);
    // A linked component is written as its library usage, not rebuilt from its layers.
    if let Some(link) = code_link(doc, n).filter(|l| !l.react.is_empty()) {
        push_indented(out, &ind, &fill_template(doc, id, &link.react, jsx_text));
        return;
    }
    let cls = tw_classes(doc, n, is_root);
    let name = n.name.replace('"', "'");
    let comp_attr = instance_comp_attr(doc, n);
    if let Some((set, icon)) = icon_of(doc, n) {
        out.push_str(&format!(
            "{ind}<div data-name=\"{name}\" data-icon=\"{set}:{icon}\"{comp_attr} className=\"{cls}\">{}</div>\n",
            inline_svg(doc, id)
        ));
        return;
    }
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Group => {
            if n.children.is_empty() {
                out.push_str(&format!(
                    "{ind}<div data-name=\"{name}\"{comp_attr} className=\"{cls}\" />\n"
                ));
            } else {
                out.push_str(&format!(
                    "{ind}<div data-name=\"{name}\"{comp_attr} className=\"{cls}\">\n"
                ));
                for c in &n.children {
                    tailwind_node(doc, *c, depth + 1, false, out);
                }
                out.push_str(&format!("{ind}</div>\n"));
            }
        }
        NodeKind::Text { text, .. } => {
            out.push_str(&format!(
                "{ind}<span{comp_attr} className=\"{cls}\">{}</span>\n",
                jsx_text(text)
            ));
        }
        NodeKind::Rect | NodeKind::Ellipse => {
            let img = n.style.fills.iter().rev().find_map(|p| match p {
                Paint::Image { asset, .. } => Some(asset.clone()),
                _ => None,
            });
            match img {
                Some(asset) => {
                    let src = format!("paper-asset://{asset}");
                    out.push_str(&format!(
                        "{ind}<img src=\"{src}\" alt=\"{name}\"{comp_attr} className=\"{cls} object-cover\" />\n"
                    ));
                }
                None => out.push_str(&format!(
                    "{ind}<div data-name=\"{name}\"{comp_attr} className=\"{cls}\" />\n"
                )),
            }
        }
        NodeKind::Sticky { text, .. } => {
            out.push_str(&format!(
                "{ind}<div data-name=\"{name}\"{comp_attr} className=\"{cls} p-4\">{}</div>\n",
                jsx_text(text)
            ));
        }
        _ => {
            let svg = inline_svg(doc, id);
            out.push_str(&format!(
                "{ind}<div data-name=\"{name}\"{comp_attr} className=\"{cls}\">{svg}</div>\n"
            ));
        }
    }
}

/// Serialize `root` and its descendants into clean React JSX with Tailwind CSS utility classes.
pub fn to_tailwind_jsx(doc: &Document, root: NodeId) -> String {
    let mut out = String::new();
    for import in code_imports(doc, root) {
        out.push_str(&format!("{import}\n"));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    tailwind_node(doc, root, 0, true, &mut out);
    out
}

fn camel_to_kebab(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn escape_html(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn html_node(
    doc: &Document,
    id: NodeId,
    depth: usize,
    is_root: bool,
    styles: &mut String,
    markup: &mut String,
) {
    let Some(n) = doc.get(id) else { return };
    let cls = format!("node-{}", id.0);
    let mut st = css::computed(n, parent_layout(doc, n).as_ref(), n.parent.is_none());
    if is_root {
        for k in ["position", "left", "top"] {
            st.remove(k);
        }
    }
    if matches!(n.kind, NodeKind::Ellipse) {
        st.insert("borderRadius".into(), json!("50%"));
    }
    styles.push_str(&format!(".{cls} {{\n"));
    for (k, v) in &st {
        if let Some(val) = v.as_str() {
            styles.push_str(&format!("  {}: {val};\n", camel_to_kebab(k)));
        }
    }
    styles.push_str("}\n");

    let ind = "  ".repeat(depth);
    let name = n.name.replace('"', "'");
    let comp_attr = instance_comp_attr(doc, n);
    if let Some((set, icon)) = icon_of(doc, n) {
        markup.push_str(&format!(
            "{ind}<div class=\"{cls}\" data-name=\"{name}\" data-icon=\"{set}:{icon}\"{comp_attr}>{}</div>\n",
            inline_svg(doc, id)
        ));
        return;
    }
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Group => {
            if n.children.is_empty() {
                markup.push_str(&format!(
                    "{ind}<div class=\"{cls}\" data-name=\"{name}\"{comp_attr}></div>\n"
                ));
            } else {
                markup.push_str(&format!(
                    "{ind}<div class=\"{cls}\" data-name=\"{name}\"{comp_attr}>\n"
                ));
                for c in &n.children {
                    html_node(doc, *c, depth + 1, false, styles, markup);
                }
                markup.push_str(&format!("{ind}</div>\n"));
            }
        }
        NodeKind::Text { text, .. } => {
            markup.push_str(&format!(
                "{ind}<span class=\"{cls}\"{comp_attr}>{}</span>\n",
                escape_html(text)
            ));
        }
        NodeKind::Sticky { text, .. } => {
            markup.push_str(&format!(
                "{ind}<div class=\"{cls}\" data-name=\"{name}\"{comp_attr}>{}</div>\n",
                escape_html(text)
            ));
        }
        _ => {
            markup.push_str(&format!(
                "{ind}<div class=\"{cls}\" data-name=\"{name}\"{comp_attr}>{}</div>\n",
                inline_svg(doc, id)
            ));
        }
    }
}

/// Generate a self-contained `<style>...</style>` + HTML block for `root`.
pub fn to_html_css(doc: &Document, root: NodeId) -> String {
    let mut css_out = String::new();
    css_out.push_str(":root {\n");
    for (k, c) in &doc.variables {
        css_out.push_str(&format!("  --{k}: {};\n", c.to_hex()));
    }
    for (k, v) in &doc.tokens {
        css_out.push_str(&format!("  --{k}: {v};\n"));
    }
    css_out.push_str("}\n");
    for (mode, vars) in &doc.modes {
        css_out.push_str(&format!("[data-mode=\"{mode}\"] {{\n"));
        for (k, c) in vars {
            css_out.push_str(&format!("  --{k}: {};\n", c.to_hex()));
        }
        css_out.push_str("}\n");
    }
    let mut markup = String::new();
    html_node(doc, root, 0, true, &mut css_out, &mut markup);
    format!("<style>\n{css_out}</style>\n{markup}")
}

/// Variables and tokens as `:root` custom properties plus one `[data-theme]` block per mode.
pub fn export_variables_css(doc: &Document) -> String {
    let body: String = doc
        .variables
        .iter()
        .map(|(k, c)| format!("  --{k}: {};\n", c.to_hex()))
        .chain(doc.tokens.iter().map(|(k, v)| format!("  --{k}: {v};\n")))
        .collect();
    let mut out = format!(":root {{\n{body}}}\n");
    for (mode, vars) in &doc.modes {
        out.push_str(&format!("[data-theme=\"{mode}\"] {{\n"));
        for (k, c) in vars {
            out.push_str(&format!("  --{k}: {};\n", c.to_hex()));
        }
        out.push_str("}\n");
    }
    out
}

/// Export document variables and tokens as a Tailwind v4 `@theme` block plus mode overrides.
pub fn export_variables_tailwind(doc: &Document) -> String {
    let mut out = String::from("@theme {\n");
    for (k, c) in &doc.variables {
        out.push_str(&format!("  --color-{k}: {};\n", c.to_hex()));
    }
    for (k, v) in &doc.tokens {
        out.push_str(&format!("  --{k}: {v};\n"));
    }
    out.push_str("}\n");
    for (mode, vars) in &doc.modes {
        out.push_str(&format!("[data-theme=\"{mode}\"] {{\n"));
        for (k, c) in vars {
            out.push_str(&format!("  --color-{k}: {};\n", c.to_hex()));
        }
        out.push_str("}\n");
    }
    out
}

fn is_dimension_token(v: &str) -> bool {
    let v = v.trim();
    if v == "0" {
        return true;
    }
    for unit in ["px", "rem", "em", "%", "pt", "vh", "vw"] {
        if let Some(num) = v.strip_suffix(unit)
            && num.trim().parse::<f64>().is_ok_and(|x| x.is_finite())
        {
            return true;
        }
    }
    false
}

/// Export document variables and tokens in W3C Design Tokens Community Group (DTCG) JSON format.
pub fn export_variables_dtcg(doc: &Document) -> Value {
    let mut out = Map::new();
    for (k, c) in &doc.variables {
        let mut entry = Map::new();
        entry.insert("$type".into(), json!("color"));
        entry.insert("$value".into(), json!(c.to_hex()));
        if !doc.modes.is_empty() {
            let mut modes = Map::new();
            for (mode, vars) in &doc.modes {
                if let Some(mc) = vars.get(k) {
                    modes.insert(mode.clone(), json!(mc.to_hex()));
                }
            }
            if !modes.is_empty() {
                entry.insert("$extensions".into(), json!({ "modes": modes }));
            }
        }
        out.insert(k.clone(), Value::Object(entry));
    }
    for (k, v) in &doc.tokens {
        let ty = if is_dimension_token(v) {
            "dimension"
        } else {
            "string"
        };
        out.insert(k.clone(), json!({ "$type": ty, "$value": v }));
    }
    Value::Object(out)
}

/// The project design system as DTCG JSON: groups (`/` in a name) nest, aliases are `{a.b}`
/// references, extra modes sit in `$extensions.modes`. `$value` follows the page's `modes`.
pub fn export_dtcg_system(ds: &DesignSystem, modes: &PageModes) -> Value {
    let reference = |id: &str| {
        let name = ds.find(id).map_or("", |(_, v)| v.name.as_str());
        format!("{{{}}}", name.replace('/', "."))
    };
    let literal = |v: &DsValue, kind: Kind| match v {
        DsValue::Color(c) => json!(c.to_hex()),
        DsValue::Number(n) if kind == Kind::Number => json!(format!("{n}px")),
        DsValue::Number(n) => json!(n),
        DsValue::String(s) => json!(s),
        DsValue::Boolean(b) => json!(b),
        DsValue::Alias(id) => json!(reference(id)),
    };
    let mut out = Map::new();
    for c in &ds.collections {
        for v in &c.variables {
            let active = modes.get(&c.id).unwrap_or(&c.default_mode);
            let Some(raw) = v.values.get(active).or_else(|| v.values.get(&c.default_mode)) else {
                continue;
            };
            let ty = match (v.kind, raw) {
                (Kind::Color, _) => "color",
                (Kind::Number, _) => "dimension",
                (Kind::Boolean, _) => "boolean",
                (Kind::String, DsValue::String(s)) if is_dimension_token(s) => "dimension",
                (Kind::String, _) => "string",
            };
            let mut entry = Map::new();
            entry.insert("$type".into(), json!(ty));
            entry.insert("$value".into(), literal(raw, v.kind));
            if !v.description.is_empty() {
                entry.insert("$description".into(), json!(v.description));
            }
            if c.modes.len() > 1 {
                let per: Map<String, Value> = c
                    .modes
                    .iter()
                    .filter_map(|m| Some((m.name.clone(), literal(v.values.get(&m.id)?, v.kind))))
                    .collect();
                entry.insert("$extensions".into(), json!({ "modes": per }));
            }
            insert_nested(&mut out, &v.name, entry);
        }
    }
    Value::Object(out)
}

/// Put `entry` at the `/` path of `name`; when a path segment is already a token, the entry
/// goes in flat under its full name instead.
fn insert_nested(root: &mut Map<String, Value>, name: &str, entry: Map<String, Value>) {
    let segs: Vec<&str> = name.split('/').filter(|s| !s.is_empty()).collect();
    let Some((leaf, groups)) = segs.split_last() else {
        return;
    };
    // A group that is already a token, or a leaf already taken, sends the entry in flat.
    let mut probe = Some(&*root);
    for g in groups {
        probe = match probe.and_then(|m| m.get(*g)) {
            None => None,
            Some(Value::Object(m)) if !m.contains_key("$value") => Some(m),
            Some(_) => {
                root.insert(name.replace('/', "-"), Value::Object(entry));
                return;
            }
        };
    }
    if probe.is_some_and(|m| m.contains_key(*leaf)) {
        root.insert(name.replace('/', "-"), Value::Object(entry));
        return;
    }
    let mut cur = root;
    for g in groups {
        cur = cur
            .entry(g.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("checked above");
    }
    cur.insert(leaf.to_string(), Value::Object(entry));
}

fn pascal_ident(name: &str) -> String {
    let mut out = String::new();
    let mut cap = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if cap {
                out.push(c.to_ascii_uppercase());
                cap = false;
            } else {
                out.push(c);
            }
        } else {
            cap = true;
        }
    }
    if out.is_empty() {
        return "Component".into();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("Component{out}")
    } else {
        out
    }
}

fn kebab_ident(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("node-{trimmed}").trim_end_matches('-').to_string()
    } else {
        trimmed
    }
}

fn upper_snake_ident(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "TOKEN".into()
    } else if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("TOKEN_{trimmed}")
    } else {
        trimmed
    }
}

/// Generate a complete, self-contained React TypeScript component (`export function <PascalName>()`)
/// using `to_tailwind_jsx` for the body and preceded by a comment block listing `:root` variables and tokens.
pub fn to_react_tsx(doc: &Document, root: NodeId) -> String {
    let mut out = String::from("/*\n:root {\n");
    for (k, c) in &doc.variables {
        out.push_str(&format!("  --{k}: {};\n", c.to_hex()));
    }
    for (k, v) in &doc.tokens {
        out.push_str(&format!("  --{k}: {v};\n"));
    }
    out.push_str("}\n*/\n\n");

    let comp_name = doc
        .get(root)
        .map(|n| pascal_ident(&n.name))
        .unwrap_or_else(|| "Component".into());
    for import in code_imports(doc, root) {
        out.push_str(&format!("{import}\n"));
    }
    let mut jsx = String::new();
    tailwind_node(doc, root, 0, true, &mut jsx);
    let body: String = jsx.lines().map(|line| format!("    {line}\n")).collect();
    out.push_str(&format!(
        "\nexport function {comp_name}() {{\n  return (\n{body}  );\n}}\n"
    ));
    out
}

fn escape_ng_template(t: &str) -> String {
    escape_html(t)
        .replace('`', "\\`")
        .replace("${", "\\${")
        .replace('{', "&#123;")
        .replace('}', "&#125;")
}

fn angular_node(
    doc: &Document,
    id: NodeId,
    depth: usize,
    is_root: bool,
    styles: &mut String,
    markup: &mut String,
) {
    let Some(n) = doc.get(id) else { return };
    let cls = format!("node-{}", id.0);
    let mut st = css::computed(n, parent_layout(doc, n).as_ref(), n.parent.is_none());
    if is_root {
        for k in ["position", "left", "top"] {
            st.remove(k);
        }
    }
    if matches!(n.kind, NodeKind::Ellipse) {
        st.insert("borderRadius".into(), json!("50%"));
    }
    styles.push_str(&format!("    .{cls} {{\n"));
    for (k, v) in &st {
        if let Some(val) = v.as_str() {
            styles.push_str(&format!("      {}: {val};\n", camel_to_kebab(k)));
        }
    }
    styles.push_str("    }\n");

    let ind = "  ".repeat(depth + 2);
    let name = n.name.replace('"', "'");
    let comp_attr = instance_comp_attr(doc, n);
    let attrs = format!(
        "class=\"{cls}\" data-node=\"{}\" data-name=\"{name}\"{comp_attr}",
        id.0
    );
    if let Some((set, icon)) = icon_of(doc, n) {
        markup.push_str(&format!(
            "{ind}<div {attrs} data-icon=\"{set}:{icon}\">{}</div>\n",
            escape_ng_template(&inline_svg(doc, id))
        ));
        return;
    }
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Group => {
            if n.children.is_empty() {
                markup.push_str(&format!("{ind}<div {attrs}></div>\n"));
            } else {
                markup.push_str(&format!("{ind}<div {attrs}>\n"));
                for c in &n.children {
                    angular_node(doc, *c, depth + 1, false, styles, markup);
                }
                markup.push_str(&format!("{ind}</div>\n"));
            }
        }
        NodeKind::Text { text, .. } => {
            markup.push_str(&format!(
                "{ind}<span {attrs}>{}</span>\n",
                escape_ng_template(text)
            ));
        }
        NodeKind::Sticky { text, .. } => {
            markup.push_str(&format!(
                "{ind}<div {attrs}>{}</div>\n",
                escape_ng_template(text)
            ));
        }
        _ => {
            markup.push_str(&format!(
                "{ind}<div {attrs}>{}</div>\n",
                escape_ng_template(&inline_svg(doc, id))
            ));
        }
    }
}

/// Generate a modern Angular v17+ Standalone Component TypeScript file.
pub fn to_angular(doc: &Document, root: NodeId) -> String {
    let raw_name = doc.get(root).map(|n| n.name.as_str()).unwrap_or("view");
    let kebab = kebab_ident(raw_name);
    let pascal = pascal_ident(raw_name);
    let class_name = if pascal.ends_with("Component") {
        pascal
    } else {
        format!("{pascal}Component")
    };

    let mut styles = String::from("    :host {\n      display: block;\n");
    for (k, c) in &doc.variables {
        styles.push_str(&format!("      --{k}: {};\n", c.to_hex()));
    }
    for (k, v) in &doc.tokens {
        styles.push_str(&format!("      --{k}: {v};\n"));
    }
    styles.push_str("    }\n");
    for (mode, vars) in &doc.modes {
        styles.push_str(&format!("    :host([data-mode=\"{mode}\"]) {{\n"));
        for (k, c) in vars {
            styles.push_str(&format!("      --{k}: {};\n", c.to_hex()));
        }
        styles.push_str("    }\n");
    }

    let mut markup = String::new();
    angular_node(doc, root, 0, true, &mut styles, &mut markup);

    format!(
        "import {{ Component, ChangeDetectionStrategy }} from '@angular/core';\n\n\
@Component({{\n\
  selector: 'app-{kebab}',\n\
  standalone: true,\n\
  changeDetection: ChangeDetectionStrategy.OnPush,\n\
  template: `\n\
{markup}  `,\n\
  styles: [`\n\
{styles}  `],\n\
}})\n\
export class {class_name} {{}}\n"
    )
}

fn f32_lit(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if !r.is_finite() {
        return "0.0".into();
    }
    if (r - r.trunc()).abs() < 1e-9 {
        format!("{:.1}", r)
    } else {
        format!("{r}")
    }
}

fn egui_color_expr(doc: &Document, color: crate::model::Color, var: Option<&str>) -> String {
    if let Some(v) = var
        && doc.variables.contains_key(v)
    {
        return format!("tokens::{}", upper_snake_ident(v));
    }
    format!(
        "egui::Color32::from_rgba_unmultiplied({}, {}, {}, {})",
        color.r, color.g, color.b, color.a
    )
}

fn egui_fill_expr(doc: &Document, n: &Node) -> Option<String> {
    n.style.fills.last().and_then(|p| match p {
        Paint::Solid { color, var } => Some(egui_color_expr(doc, *color, var.as_deref())),
        _ => None,
    })
}

fn egui_node(doc: &Document, id: NodeId, depth: usize, is_root: bool, out: &mut String) {
    let Some(n) = doc.get(id) else { return };
    if !n.visible {
        return;
    }
    let ind = "    ".repeat(depth);
    // A linked component is the widget call it stands for.
    if let Some(link) = code_link(doc, n).filter(|l| !l.egui.is_empty()) {
        push_indented(out, &ind, &fill_template(doc, id, &link.egui, |t| format!("{t:?}")));
        return;
    }
    if let Some((set, icon)) = icon_of(doc, n) {
        let (hex, var) = icon_tint(doc, n);
        let tint = hex
            .as_deref()
            .and_then(|h| crate::model::Color::from_hex(h))
            .map_or_else(
                || "egui::Color32::WHITE".to_string(),
                |c| egui_color_expr(doc, c, var.as_deref().map(|v| v.trim_start_matches("--"))),
            );
        out.push_str(&format!(
            "{ind}// icon `{set}:{icon}` (stroke icon, {w}x{h}); paint it with your icon renderer, tinted by the colour below\n",
            w = f32_lit(n.rect.w),
            h = f32_lit(n.rect.h)
        ));
        out.push_str(&format!(
            "{ind}let (rect, _) = ui.allocate_exact_size(egui::vec2({} as f32, {} as f32), egui::Sense::hover());\n",
            f32_lit(n.rect.w),
            f32_lit(n.rect.h)
        ));
        out.push_str(&format!(
            "{ind}let _icon_tint = {tint}; let _icon_rect = rect;\n"
        ));
        return;
    }
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Group => {
            let mut frame_chain = String::from("egui::Frame::new()");
            if let Some(fill) = egui_fill_expr(doc, n) {
                frame_chain.push_str(&format!(".fill({fill})"));
            }
            if n.style.corner_radius > 0.0 {
                frame_chain.push_str(&format!(
                    ".corner_radius({} as f32)",
                    f32_lit(n.style.corner_radius)
                ));
            }
            if let Some(stroke_col) = n.style.stroke
                && n.style.stroke_width > 0.0
            {
                let sc = egui_color_expr(doc, stroke_col, None);
                frame_chain.push_str(&format!(
                    ".stroke(egui::Stroke::new({} as f32, {sc}))",
                    f32_lit(n.style.stroke_width)
                ));
            }
            if let Some(l) = &n.layout {
                let [pt, pr, pb, pl] = l.padding;
                if pt != 0.0 || pr != 0.0 || pb != 0.0 || pl != 0.0 {
                    frame_chain.push_str(&format!(
                        ".inner_margin(egui::Margin {{ top: {}, right: {}, bottom: {}, left: {} }})",
                        pt.round() as i8,
                        pr.round() as i8,
                        pb.round() as i8,
                        pl.round() as i8,
                    ));
                }
            }
            out.push_str(&format!("{ind}{frame_chain}.show(ui, |ui| {{\n"));
            let inner = "    ".repeat(depth + 1);
            if is_root || n.layout_child.sizing[0] == Sizing::Fixed {
                out.push_str(&format!(
                    "{inner}ui.set_min_width({} as f32);\n",
                    f32_lit(n.rect.w)
                ));
            }
            if is_root || n.layout_child.sizing[1] == Sizing::Fixed {
                out.push_str(&format!(
                    "{inner}ui.set_min_height({} as f32);\n",
                    f32_lit(n.rect.h)
                ));
            }
            if let Some(l) = &n.layout {
                out.push_str(&format!(
                    "{inner}ui.spacing_mut().item_spacing = egui::vec2({} as f32, {} as f32);\n",
                    f32_lit(l.gap),
                    f32_lit(l.cross_gap)
                ));
                let flow_fn = match (l.direction, l.wrap) {
                    (Direction::Horizontal, true) => "horizontal_wrapped",
                    (Direction::Horizontal, false) => "horizontal",
                    (Direction::Vertical, _) => "vertical",
                };
                out.push_str(&format!("{inner}ui.{flow_fn}(|ui| {{\n"));
                for c in &n.children {
                    egui_node(doc, *c, depth + 2, false, out);
                }
                out.push_str(&format!("{inner}}});\n"));
            } else {
                for c in &n.children {
                    egui_node(doc, *c, depth + 1, false, out);
                }
            }
            out.push_str(&format!("{ind}}});\n"));
        }
        NodeKind::Text {
            text, font_size, ..
        }
        | NodeKind::Sticky {
            text, font_size, ..
        } => {
            let mut rich = format!(
                "egui::RichText::new({:?}).size({} as f32)",
                text,
                f32_lit(*font_size)
            );
            if let Some(fill) = egui_fill_expr(doc, n) {
                rich.push_str(&format!(".color({fill})"));
            }
            if n.text_style.bold {
                rich.push_str(".strong()");
            }
            if n.text_style.italic {
                rich.push_str(".italics()");
            }
            out.push_str(&format!("{ind}ui.label({rich});\n"));
        }
        NodeKind::Ellipse => {
            let fill =
                egui_fill_expr(doc, n).unwrap_or_else(|| "egui::Color32::TRANSPARENT".into());
            out.push_str(&format!(
                "{ind}let (rect, _) = ui.allocate_exact_size(egui::vec2({} as f32, {} as f32), egui::Sense::hover());\n",
                f32_lit(n.rect.w),
                f32_lit(n.rect.h)
            ));
            out.push_str(&format!(
                "{ind}ui.painter().circle_filled(rect.center(), rect.width().min(rect.height()) * 0.5, {fill});\n"
            ));
        }
        _ => {
            let fill =
                egui_fill_expr(doc, n).unwrap_or_else(|| "egui::Color32::TRANSPARENT".into());
            out.push_str(&format!(
                "{ind}let (rect, _) = ui.allocate_exact_size(egui::vec2({} as f32, {} as f32), egui::Sense::hover());\n",
                f32_lit(n.rect.w),
                f32_lit(n.rect.h)
            ));
            out.push_str(&format!(
                "{ind}ui.painter().rect_filled(rect, {} as f32, {fill});\n",
                f32_lit(n.style.corner_radius)
            ));
        }
    }
}

/// Generate idiomatic Rust `egui 0.36` code that renders the `root` subtree.
pub fn to_egui(doc: &Document, root: NodeId) -> String {
    let mut out = String::from("pub mod tokens {\n");
    for (k, c) in &doc.variables {
        let name = upper_snake_ident(k);
        out.push_str(&format!(
            "    pub const {name}: egui::Color32 = egui::Color32::from_rgba_unmultiplied({}, {}, {}, {});\n",
            c.r, c.g, c.b, c.a
        ));
    }
    for (k, v) in &doc.tokens {
        if let Some(px) = css::length(v, 16.0) {
            let name = upper_snake_ident(k);
            out.push_str(&format!("    pub const {name}: f32 = {};\n", f32_lit(px)));
        }
    }
    out.push_str("}\n\n");
    out.push_str("pub fn render(ui: &mut egui::Ui) {\n");
    egui_node(doc, root, 1, true, &mut out);
    out.push_str("}\n");
    out
}

pub(crate) fn annotation_comments(
    doc: &Document,
    root: NodeId,
    categories: &[crate::model::annotation::AnnotationCategory],
) -> String {
    let ids = doc.subtree(root);
    let numbers = crate::model::annotation::numbers(doc);
    let mut notes: Vec<_> = doc
        .annotations
        .iter()
        .filter(|a| a.target.is_some_and(|id| ids.contains(&id)))
        .collect();
    notes.sort_by_key(|a| (numbers[&a.id].0.map(|id| id.0), numbers[&a.id].1));
    let mut out = String::new();
    for note in notes {
        let category = categories
            .iter()
            .find(|c| c.id == note.category)
            .map(|c| c.name.as_str())
            .unwrap_or(&note.category);
        out.push_str(&format!(
            "// {} {category}: {}\n",
            numbers[&note.id].1, note.label
        ));
        for line in note.body.lines() {
            out.push_str(&format!("// {line}\n"));
        }
    }
    out
}

/// Generate code for `root` in the requested `target` format (`tailwind`/`jsx`, `react`/`tsx`,
/// `angular`/`ng`, `html`/`css`, or `egui`/`rust`).
pub fn code_for_target(doc: &Document, root: NodeId, target: &str) -> Result<String, String> {
    code_for_target_with_categories(
        doc,
        root,
        target,
        &crate::model::annotation::default_categories(),
    )
}

pub(crate) fn code_for_target_with_categories(
    doc: &Document,
    root: NodeId,
    target: &str,
    categories: &[crate::model::annotation::AnnotationCategory],
) -> Result<String, String> {
    let code = match target.trim().to_ascii_lowercase().as_str() {
        "tailwind" | "jsx" => Ok(to_tailwind_jsx(doc, root)),
        "react" | "tsx" => Ok(to_react_tsx(doc, root)),
        "angular" | "ng" => Ok(to_angular(doc, root)),
        "html" | "css" => Ok(to_html_css(doc, root)),
        "egui" | "rust" => Ok(to_egui(doc, root)),
        other => Err(format!(
            "unknown target `{other}` (expected react, tailwind, angular, html, or egui)"
        )),
    }?;
    Ok(format!(
        "{}{code}",
        annotation_comments(doc, root, categories)
    ))
}

fn sizing_name(s: Sizing) -> &'static str {
    match s {
        Sizing::Fixed => "fixed",
        Sizing::Hug => "hug",
        Sizing::Fill => "fill",
    }
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Start => "start",
        Align::Center => "center",
        Align::End => "end",
    }
}

fn text_align_name(a: TextAlign) -> &'static str {
    match a {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

/// One fill layer as JSON: solid colours keep their variable, gradients keep their stops, images their asset.
fn paint_json(doc: &Document, p: &Paint) -> Value {
    let _ = doc;
    match p {
        Paint::Solid { color, var } => {
            json!({"type": "solid", "color": color.to_hex(), "var": var.as_ref().map(|v| format!("--{v}"))})
        }
        Paint::Linear { stops, from, to } => json!({
            "type": "linear",
            "from": [from.x, from.y],
            "to": [to.x, to.y],
            "stops": stops.iter().map(|s| json!({"pos": s.pos, "color": s.color.to_hex()})).collect::<Vec<_>>(),
        }),
        Paint::Radial {
            stops,
            center,
            radius,
        } => json!({
            "type": "radial",
            "center": [center.x, center.y],
            "radius": [radius.x, radius.y],
            "stops": stops.iter().map(|s| json!({"pos": s.pos, "color": s.color.to_hex()})).collect::<Vec<_>>(),
        }),
        Paint::Image { asset, opacity, .. } => {
            json!({"type": "image", "asset": asset, "opacity": opacity})
        }
    }
}

fn build_spec_tree(
    doc: &Document,
    id: NodeId,
    is_root: bool,
    components: &mut Vec<Value>,
    rules: &mut Vec<String>,
    flow_count: &mut usize,
) -> Option<Value> {
    let n = doc.get(id)?;
    if !n.visible {
        return None;
    }
    let pl = parent_layout(doc, n);
    let in_flow = is_root || (pl.is_some() && !n.layout_child.absolute);
    if !is_root && in_flow {
        *flow_count += 1;
    }
    // A linked component is one library call: no rules for the layers that draw it (the
    // "MUST use the <kit> component" rule covers it).
    if !is_root && let Some(link) = code_link(doc, n) {
        return Some(json!({
            "id": n.id.0, "name": n.name, "kind": "library",
            "library": {"kit": link.kit, "import": link.import, "react": link.react, "egui": link.egui,
                "usage": fill_template(doc, id, &link.react, jsx_text), "eguiUsage": fill_template(doc, id, &link.egui, |t| format!("{t:?}"))},
            "bounds": {"width": n.rect.w, "height": n.rect.h},
            "sizing": {"horizontal": "fixed", "vertical": "fixed", "inFlow": in_flow},
        }));
    }

    let mut instance_of: Option<Value> = None;
    if let Some(link) = &n.instance
        && let Some(main) = doc.get(link.target().0)
        && main.component
    {
        instance_of = Some(
            json!({"component": main.name, "componentId": main.id.0, "overrides": link.overrides}),
        );
        components.push(json!({
            "nodeId": n.id.0,
            "instanceName": n.name,
            "componentName": main.name,
            "overrides": link.overrides,
        }));
        rules.push(format!(
            "MUST preserve component instance `{}` (#{}) referencing `{}`",
            n.name, n.id.0, main.name
        ));
    }

    let (fill_hex, fill_var) = match n.style.fills.last() {
        Some(Paint::Solid { color, var }) => {
            (Some(color.to_hex()), var.as_ref().map(|v| format!("--{v}")))
        }
        _ => (None, None),
    };

    if let Some(Paint::Solid {
        color,
        var: Some(v),
    }) = n.style.fills.last()
    {
        rules.push(format!(
            "MUST use token `--{v}` (or `tokens::{}`) instead of hardcoded hex `{}` on `{}` (#{})",
            upper_snake_ident(v),
            color.to_hex(),
            n.name,
            n.id.0
        ));
    }

    let auto_layout = n.layout.map(|l| {
        let dir = match l.direction {
            Direction::Horizontal => "row",
            Direction::Vertical => "column",
        };
        rules.push(format!(
            "MUST use {dir} auto-layout on `{}` (#{}) with gap {}px and padding [{}px, {}px, {}px, {}px]",
            n.name,
            n.id.0,
            px_num(l.gap),
            px_num(l.padding[0]),
            px_num(l.padding[1]),
            px_num(l.padding[2]),
            px_num(l.padding[3])
        ));
        if let Some(t) = token_for(doc, l.gap, &["space"]).filter(|_| l.gap > 0.0) {
            rules.push(format!(
                "SHOULD use spacing token `{t}` for the {}px gap on `{}` (#{})",
                px_num(l.gap),
                n.name,
                n.id.0
            ));
        }
        let pad_tokens: Vec<Value> = l
            .padding
            .iter()
            .map(|p| json!(token_for(doc, *p, &["space"])))
            .collect();
        json!({
            "direction": dir,
            "wrap": l.wrap,
            "gap": l.gap,
            "gapToken": token_for(doc, l.gap, &["space"]),
            "crossGap": l.cross_gap,
            "paddingTokens": pad_tokens,
            "padding": l.padding,
            "alignMain": if l.gap_auto { "space-between" } else { align_name(l.align_main) },
            "alignCross": align_name(l.align_cross),
        })
    });

    let text_info = match &n.kind {
        NodeKind::Text {
            text,
            font_size,
            align,
            ..
        } => {
            if !text.is_empty() {
                rules.push(format!(
                    "MUST include exact text string {:?} for `{}` (#{})",
                    text, n.name, n.id.0
                ));
            }
            Some(json!({
                "content": text,
                "fontSize": font_size,
                "fontFamily": "Inter",
                "fontWeight": if n.text_style.bold { 700 } else { 400 },
                "lineHeight": n.text_style.line_height.map(|m| m * font_size),
                "letterSpacing": n.text_style.letter_spacing,
                "autoWidth": matches!(n.kind, NodeKind::Text { auto_width: true, .. }),
                "bold": n.text_style.bold,
                "italic": n.text_style.italic,
                "align": text_align_name(*align),
                "fontSizeToken": token_for(doc, *font_size, &["text"]),
            }))
        }
        NodeKind::Sticky { text, font_size } => {
            if !text.is_empty() {
                rules.push(format!(
                    "MUST include exact text string {:?} for `{}` (#{})",
                    text, n.name, n.id.0
                ));
            }
            Some(json!({
                "content": text,
                "fontSize": font_size,
                "bold": n.text_style.bold,
                "italic": n.text_style.italic,
                "align": "left",
            }))
        }
        _ => None,
    };

    let icon = icon_of(doc, n);
    let children: Vec<Value> = if icon.is_some() {
        Vec::new()
    } else {
        n.children
            .iter()
            .filter_map(|c| build_spec_tree(doc, *c, false, components, rules, flow_count))
            .collect()
    };

    let mut obj = Map::new();
    obj.insert("id".into(), json!(n.id.0));
    obj.insert("name".into(), json!(n.name));
    obj.insert(
        "kind".into(),
        json!(if icon.is_some() {
            "icon".to_string()
        } else {
            n.kind.type_name().to_ascii_lowercase()
        }),
    );
    if let Some((set, name)) = &icon {
        let (hex, var) = icon_tint(doc, n);
        rules.push(format!(
            "MUST render icon `{set}:{name}` at {}x{}px on `{}` (#{}) as a stroke icon tinted {}",
            px_num(n.rect.w),
            px_num(n.rect.h),
            n.name,
            n.id.0,
            var.as_deref()
                .map(|v| format!("`var({v})`"))
                .or_else(|| hex.clone())
                .unwrap_or_else(|| "the surrounding text colour".into()),
        ));
        obj.insert(
            "icon".into(),
            json!({
                "set": set,
                "name": name,
                "size": [n.rect.w, n.rect.h],
                "color": hex,
                "colorVar": var,
                "note": "Icon strokes are not bound to variables; colorVar is the variable with the same value.",
            }),
        );
    }
    if let Some(io) = instance_of {
        obj.insert("instanceOf".into(), io);
    }
    if matches!(
        n.kind,
        NodeKind::Path { .. } | NodeKind::Line { .. } | NodeKind::Connector { .. }
    ) {
        let svg = inline_svg(doc, n.id);
        // The SVG is a little larger than the node's box (stroke caps, rounding); report where it sits.
        let attr = |name: &str| {
            let key = format!("{name}=\"");
            let at = svg.find(&key)? + key.len();
            svg[at..at + svg[at..].find('"')?].parse::<f64>().ok()
        };
        if let (Some(w), Some(h)) = (attr("width"), attr("height")) {
            obj.insert(
                "svgBounds".into(),
                json!({
                    "x": n.rect.x - (w - n.rect.w) / 2.0,
                    "y": n.rect.y - (h - n.rect.h) / 2.0,
                    "width": w,
                    "height": h,
                }),
            );
        }
        obj.insert("svg".into(), json!(svg));
    }
    obj.insert(
        "bounds".into(),
        json!({
            "x": n.rect.x,
            "y": n.rect.y,
            "width": n.rect.w,
            "height": n.rect.h,
        }),
    );
    // Text boxes size themselves: an auto-width text hugs its content on both axes, a wrapping text
    // hugs its height. The layout `sizing` of a text node only says how it sits in its parent.
    let (h_sizing, v_sizing) = match n.kind {
        NodeKind::Text {
            auto_width: true, ..
        } => ("hug", "hug"),
        NodeKind::Text { .. } => (sizing_name(n.layout_child.sizing[0]), "hug"),
        _ => (
            sizing_name(n.layout_child.sizing[0]),
            sizing_name(n.layout_child.sizing[1]),
        ),
    };
    let mut sizing = json!({
        "horizontal": h_sizing,
        "vertical": v_sizing,
        "inFlow": in_flow,
    });
    if let Some(m) = sizing.as_object_mut() {
        let lc = n.layout_child;
        for (key, v) in [
            ("minWidth", lc.min_size[0]),
            ("minHeight", lc.min_size[1]),
            ("maxWidth", lc.max_size[0]),
            ("maxHeight", lc.max_size[1]),
        ] {
            if let Some(v) = v {
                m.insert(key.into(), json!(v));
            }
        }
        if let Some(a) = lc.align_self {
            m.insert("alignSelf".into(), json!(align_name(a)));
        }
    }
    obj.insert("sizing".into(), sizing);
    if let Some(al) = auto_layout {
        obj.insert("autoLayout".into(), al);
    }
    let stroke = n.style.stroke.filter(|_| n.style.stroke_width > 0.0);
    let mut style = json!({
        "fill": fill_hex,
        "fillVar": fill_var,
        "fills": n.style.fills.iter().map(|f| paint_json(doc, f)).collect::<Vec<_>>(),
        "stroke": stroke.map(|c| c.to_hex()),
        // A bound stroke (`bind_variable` field `stroke`) is written as its variable, like fills.
        "strokeVar": stroke.and(n.bindings.get("stroke")).map(|v| format!("--{}", v.trim_start_matches("--"))),
        "strokeWidth": stroke.map(|_| n.style.stroke_width),
        "cornerRadius": n.style.corner_radius,
        "cornerRadiusToken": token_for(doc, n.style.corner_radius, &["radius"]),
        "opacity": n.style.opacity,
    });
    if let Some(m) = style.as_object_mut() {
        if let Some(r) = n.style.radii {
            m.insert("radii".into(), json!(r));
        }
        if let Some(sd) = n.style.stroke_sides {
            m.insert("strokeSides".into(), json!(sd));
        }
        if stroke.is_some() {
            m.insert(
                "strokeCap".into(),
                json!(format!("{:?}", n.style.cap).to_lowercase()),
            );
            m.insert(
                "strokeJoin".into(),
                json!(format!("{:?}", n.style.join).to_lowercase()),
            );
        }
        if let NodeKind::Frame { clip } = n.kind {
            m.insert("clip".into(), json!(clip));
        }
        if n.scroll {
            // How far the content reaches past the frame: code needs it for `overflow: auto`.
            m.insert("scroll".into(), json!(crate::editor::scroll_range(doc, n.id).round()));
        }
        if let Some(sh) = n.style.shadow {
            m.insert(
                "shadow".into(),
                json!({"x": sh.x, "y": sh.y, "blur": sh.blur, "spread": sh.spread}),
            );
        }
        if n.style.blend != Default::default() {
            m.insert(
                "blend".into(),
                json!(format!("{:?}", n.style.blend).to_lowercase()),
            );
        }
    }
    obj.insert("style".into(), style);
    if let Some(ti) = text_info {
        obj.insert("text".into(), ti);
    }
    obj.insert("children".into(), Value::Array(children));
    Some(Value::Object(obj))
}

/// Build the deterministic AI Layout & Enforcement Spec JSON for `root` and `target`.
pub fn to_ai_spec(doc: &Document, root: NodeId, target: &str) -> Result<Value, String> {
    to_ai_spec_with_categories(doc, root, target, &crate::model::annotation::default_categories())
}

pub(crate) fn to_ai_spec_with_categories(
    doc: &Document, root: NodeId, target: &str,
    categories: &[crate::model::annotation::AnnotationCategory],
) -> Result<Value, String> {
    let Some(root_node) = doc.get(root) else {
        return Err(format!("no node #{id}", id = root.0));
    };
    let reference_code = code_for_target_with_categories(doc, root, target, categories)?;

    let mut components = Vec::new();
    let mut rules = Vec::new();
    let mut flow_count = 0usize;
    let layout_tree = build_spec_tree(
        doc,
        root,
        true,
        &mut components,
        &mut rules,
        &mut flow_count,
    )
    .unwrap_or(Value::Null);

    rules.insert(
        0,
        format!(
            "MUST NOT use `position: absolute` (or Tailwind `absolute`) on in-flow children ({flow_count} flow nodes under root)"
        ),
    );
    let library = library_components(doc, root);
    for l in &library {
        let usage = if matches!(target.trim().to_ascii_lowercase().as_str(), "egui" | "rust") { &l["egui"] } else { &l["react"] };
        if let Some(u) = usage.as_str().filter(|u| !u.is_empty()) {
            rules.push(format!(
                "MUST use the {} component for `{}` ({} uses: `{}`), not rebuild it from its layers",
                l["kit"].as_str().unwrap_or(""),
                l["name"].as_str().unwrap_or(""),
                l["usedBy"].as_array().map_or(0, Vec::len),
                u.lines().next().unwrap_or(u)
            ));
        }
    }

    let colors: Map<String, Value> = doc
        .variables
        .iter()
        .map(|(k, c)| (k.clone(), json!(c.to_hex())))
        .collect();
    let modes: Vec<String> = doc.modes.keys().cloned().collect();

    let mut definitions: Vec<Value> = Vec::new();
    let mut seen: Vec<NodeId> = Vec::new();
    for c in components.clone() {
        let Some(inst) = c["nodeId"]
            .as_u64()
            .map(|i| NodeId(i as _))
            .and_then(|i| doc.get(i))
        else {
            continue;
        };
        let Some(main) = inst.instance.as_ref().map(|l| l.target().0) else {
            continue;
        };
        if seen.contains(&main) {
            continue;
        }
        seen.push(main);
        let (mut nested, mut nested_rules, mut nested_flow) = (Vec::new(), Vec::new(), 0usize);
        if let Some(tree) = build_spec_tree(
            doc,
            main,
            true,
            &mut nested,
            &mut nested_rules,
            &mut nested_flow,
        ) {
            definitions.push(json!({"name": doc.get(main).map(|m| m.name.clone()), "nodeId": main.0, "layoutTree": tree}));
        }
    }

    Ok(json!({
        "target": target,
        "rootId": root.0,
        "name": root_node.name,
        "tokens": {
            "colors": colors,
            "modes": modes,
            "dimensions": doc.tokens,
        },
        "components": components,
        "componentDefinitions": definitions,
        "libraryComponents": library,
        "layoutTree": layout_tree,
        "enforcementRules": rules,
        "annotations": annotation_comments(doc, root, categories).lines().collect::<Vec<_>>(),
        "referenceCode": reference_code,
    }))
}

/// The linked components used under `root`: one entry per main component, with the layers
/// that use it. Nested layers of a linked component are not looked into.
fn library_components(doc: &Document, root: NodeId) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(n) = doc.get(id) else { continue };
        let Some(link) = code_link(doc, n) else {
            stack.extend(n.children.iter().rev());
            continue;
        };
        let main = n.instance.as_ref().map_or(id, |l| l.target().0);
        let name = doc.get(main).map_or_else(|| n.name.clone(), |m| m.name.clone());
        match out.iter_mut().find(|e| e["mainId"] == json!(main.0)) {
            Some(e) => e["usedBy"].as_array_mut().unwrap().push(json!(id.0)),
            None => out.push(json!({"name": name, "mainId": main.0, "kit": link.kit, "import": link.import, "react": link.react, "egui": link.egui, "usedBy": [id.0]})),
        }
    }
    out
}

fn format_hierarchy_md(node: &Value, depth: usize, out: &mut String) {
    let ind = "  ".repeat(depth);
    let id = node["id"].as_u64().unwrap_or(0);
    let name = node["name"].as_str().unwrap_or("Node");
    let kind = node["kind"].as_str().unwrap_or("node");
    let w = node["bounds"]["width"].as_f64().unwrap_or(0.0);
    let h = node["bounds"]["height"].as_f64().unwrap_or(0.0);
    let sh = node["sizing"]["horizontal"].as_str().unwrap_or("fixed");
    let sv = node["sizing"]["vertical"].as_str().unwrap_or("fixed");
    let in_flow = node["sizing"]["inFlow"].as_bool().unwrap_or(true);

    let mut details = vec![format!(
        "size={}x{} (h:{sh}, v:{sv}, {})",
        px_num(w),
        px_num(h),
        if in_flow { "in-flow" } else { "absolute" }
    )];

    if let Some(al) = node.get("autoLayout") {
        let dir = al["direction"].as_str().unwrap_or("row");
        let gap = al["gap"].as_f64().unwrap_or(0.0);
        let pad = al["padding"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|v| px_num(v.as_f64().unwrap_or(0.0)))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        details.push(format!("flex={dir} gap={}px pad=[{pad}]", px_num(gap)));
    }
    if let Some(fv) = node["style"]["fillVar"].as_str() {
        details.push(format!("fill=var({fv})"));
    } else if let Some(fh) = node["style"]["fill"].as_str() {
        details.push(format!("fill={fh}"));
    }
    if let Some(txt) = node.get("text")
        && let Some(content) = txt["content"].as_str()
    {
        details.push(format!("text={content:?}"));
        if let Some(w) = txt["fontWeight"].as_u64() {
            details.push(format!("weight={w}"));
        }
        if let Some(lh) = txt["lineHeight"].as_f64() {
            details.push(format!("line-height={}px", px_num(lh)));
        }
    }
    if let Some(lib) = node.get("library") {
        let usage = lib["usage"].as_str().filter(|u| !u.is_empty()).or(lib["eguiUsage"].as_str()).unwrap_or("");
        details.push(format!("use `{}`", usage.lines().next().unwrap_or(usage)));
    }
    if let Some(ic) = node.get("icon") {
        details.push(format!(
            "icon={}:{} tint={}",
            ic["set"].as_str().unwrap_or(""),
            ic["name"].as_str().unwrap_or(""),
            ic["colorVar"]
                .as_str()
                .or(ic["color"].as_str())
                .unwrap_or("currentColor")
        ));
    }

    out.push_str(&format!(
        "{ind}- **`{name}`** (`#{id}`, {kind}): {}\n",
        details.join(" | ")
    ));

    if let Some(children) = node["children"].as_array() {
        for c in children {
            format_hierarchy_md(c, depth + 1, out);
        }
    }
}

fn lang_is_egui(target: &str) -> bool {
    matches!(target.trim().to_ascii_lowercase().as_str(), "egui" | "rust")
}

/// Format the AI Layout & Enforcement Spec as a structured Markdown document.
pub fn to_ai_spec_markdown(doc: &Document, root: NodeId, target: &str) -> Result<String, String> {
    to_ai_spec_markdown_with_categories(doc, root, target, &crate::model::annotation::default_categories())
}

pub(crate) fn to_ai_spec_markdown_with_categories(
    doc: &Document, root: NodeId, target: &str,
    categories: &[crate::model::annotation::AnnotationCategory],
) -> Result<String, String> {
    let spec = to_ai_spec_with_categories(doc, root, target, categories)?;
    let name = spec["name"].as_str().unwrap_or("Frame");
    let mut md = format!("# AI UI Implementation Spec: {name} ({target})\n\n");

    md.push_str("## 1. Mandatory Enforcement Rules\n\n");
    if let Some(rules) = spec["enforcementRules"].as_array() {
        for r in rules {
            if let Some(s) = r.as_str() {
                md.push_str(&format!("- [ ] {s}\n"));
            }
        }
    }
    md.push('\n');

    md.push_str("## 2. Design Tokens & Variables\n\n");
    if doc.variables.is_empty() && doc.tokens.is_empty() {
        md.push_str("_No document tokens defined._\n\n");
    } else {
        for (k, c) in &doc.variables {
            md.push_str(&format!("- `--{k}`: `{}`\n", c.to_hex()));
        }
        for (k, v) in &doc.tokens {
            md.push_str(&format!("- `--{k}`: `{v}`\n"));
        }
        if !doc.modes.is_empty() {
            let modes: Vec<&str> = doc.modes.keys().map(String::as_str).collect();
            md.push_str(&format!("- Modes: `{}`\n", modes.join("`, `")));
        }
        md.push('\n');
    }

    // Sections are numbered as they appear: the library one only shows when there is one.
    let mut section = 2;
    let mut next = || {
        section += 1;
        section
    };
    if let Some(lib) = spec["libraryComponents"].as_array().filter(|l| !l.is_empty()) {
        let egui = lang_is_egui(target);
        md.push_str(&format!("## {}. Library Components\n\nUse these components; the layers under them are only how they look.\n\n", next()));
        for l in lib {
            let usage = if egui { l["egui"].as_str() } else { l["react"].as_str() }.unwrap_or("");
            md.push_str(&format!("- **`{}`** ({}), used {}×", l["name"].as_str().unwrap_or(""), l["kit"].as_str().unwrap_or(""), l["usedBy"].as_array().map_or(0, Vec::len)));
            if !egui && let Some(i) = l["import"].as_str().filter(|i| !i.is_empty()) {
                md.push_str(&format!(": `{i}`"));
            }
            md.push_str(&format!("\n  ```{}\n  {}\n  ```\n", if egui { "rust" } else { "tsx" }, usage.replace('\n', "\n  ")));
        }
        md.push('\n');
    }

    md.push_str(&format!("## {}. Structured Layout Hierarchy\n\n", next()));
    format_hierarchy_md(&spec["layoutTree"], 0, &mut md);
    md.push('\n');

    let lang_tag = match target.trim().to_ascii_lowercase().as_str() {
        "react" | "tsx" | "tailwind" | "jsx" => "tsx",
        "angular" | "ng" => "ts",
        "egui" | "rust" => "rust",
        "html" | "css" => "html",
        _ => "txt",
    };
    let ref_code = spec["referenceCode"].as_str().unwrap_or("");
    md.push_str(&format!(
        "## {}. Reference Implementation ({target})\n\n```{lang_tag}\n{}\n```\n",
        next(),
        ref_code.trim_end()
    ));

    Ok(md)
}

/// Format the AI spec as compact text for an implementer: notes, the tokens the subtree uses,
/// kit components, component definitions and one line per node. No reference code, no
/// per-node enforcement prose (`verify_code` checks those). About 12x fewer tokens than JSON.
pub fn to_ai_spec_compact(doc: &Document, root: NodeId, target: &str) -> Result<String, String> {
    to_ai_spec_compact_with_categories(doc, root, target, &crate::model::annotation::default_categories())
}

pub(crate) fn to_ai_spec_compact_with_categories(
    doc: &Document, root: NodeId, target: &str,
    categories: &[crate::model::annotation::AnnotationCategory],
) -> Result<String, String> {
    let spec = to_ai_spec_with_categories(doc, root, target, categories)?;
    Ok(format_compact(&spec))
}

fn format_compact(spec: &Value) -> String {
    let mut out = format!("# {}\n\n", spec["name"].as_str().unwrap_or("Frame"));
    if let Some(notes) = spec["annotations"].as_array().filter(|a| !a.is_empty()) {
        out.push_str("## Notes\n");
        for n in notes {
            out.push_str(n.as_str().unwrap_or("").trim_start_matches('/').trim());
            out.push('\n');
        }
        out.push('\n');
    }
    let mut used = std::collections::BTreeSet::new();
    collect_tokens(&spec["layoutTree"], &mut used);
    for d in spec["componentDefinitions"].as_array().into_iter().flatten() {
        collect_tokens(&d["layoutTree"], &mut used);
    }
    if !used.is_empty() {
        out.push_str("## Tokens used\n");
        for u in &used {
            let key = u.trim_start_matches("--");
            let v = spec["tokens"]["colors"][key].as_str().or(spec["tokens"]["dimensions"][key].as_str()).unwrap_or("?");
            out.push_str(&format!("{u}: {v}\n"));
        }
        out.push('\n');
    }
    if let Some(lib) = spec["libraryComponents"].as_array().filter(|l| !l.is_empty()) {
        out.push_str("## Kit components\n");
        for l in lib {
            out.push_str(&format!("{} ({}): {}\n", l["name"].as_str().unwrap_or(""), l["kit"].as_str().unwrap_or(""), l["import"].as_str().unwrap_or("")));
        }
        out.push('\n');
    }
    if let Some(defs) = spec["componentDefinitions"].as_array().filter(|d| !d.is_empty()) {
        out.push_str("## Component definitions\n");
        for d in defs {
            out.push_str(&format!("### {}\n", d["name"].as_str().unwrap_or("")));
            compact_node(&d["layoutTree"], 0, &mut out);
            out.push('\n');
        }
    }
    out.push_str("## Tree\nFormat: Kind \"layer\" WxH layout style. hug/fill = sizing. Tokens as --name. scroll+N = content scrolls, N px past the bottom. Text: \"content\" size/lineHeight weight color.\n\n");
    compact_node(&spec["layoutTree"], 0, &mut out);
    out
}

fn collect_tokens(n: &Value, used: &mut std::collections::BTreeSet<String>) {
    let mut add = |v: &Value| {
        if let Some(s) = v.as_str() {
            used.insert(s.to_owned());
        }
    };
    add(&n["autoLayout"]["gapToken"]);
    for t in n["autoLayout"]["paddingTokens"].as_array().into_iter().flatten() {
        add(t);
    }
    for f in n["style"]["fills"].as_array().into_iter().flatten() {
        add(&f["var"]);
    }
    add(&n["style"]["fillVar"]);
    add(&n["style"]["strokeVar"]);
    add(&n["style"]["cornerRadiusToken"]);
    add(&n["text"]["fontSizeToken"]);
    add(&n["icon"]["colorVar"]);
    for c in n["children"].as_array().into_iter().flatten() {
        collect_tokens(c, used);
    }
}

/// A token name when bound, else the raw value.
fn tok(var: &Value, value: &Value) -> String {
    if let Some(v) = var.as_str() {
        return v.to_owned();
    }
    match value {
        Value::Number(n) => px_num(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => s.clone(),
        _ => "0".into(),
    }
}

fn compact_size(n: &Value) -> String {
    let axis = |mode: &Value, px: &Value| match mode.as_str() {
        Some("hug") => "hug".to_owned(),
        Some("fill") => "fill".to_owned(),
        _ => px_num(px.as_f64().unwrap_or(0.0)),
    };
    format!(
        "{}×{}",
        axis(&n["sizing"]["horizontal"], &n["bounds"]["width"]),
        axis(&n["sizing"]["vertical"], &n["bounds"]["height"])
    )
}

fn compact_layout(al: &Value) -> Vec<String> {
    if al.is_null() {
        return Vec::new();
    }
    let mut parts = vec![if al["direction"] == "row" { "row" } else { "col" }.to_owned()];
    if al["wrap"] == true {
        parts.push("wrap".into());
    }
    let gap = tok(&al["gapToken"], &al["gap"]);
    if gap != "0" {
        parts.push(format!("gap={gap}"));
    }
    let pad: Vec<&Value> = al["padding"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    if pad.iter().any(|v| v.as_f64().unwrap_or(0.0) != 0.0) {
        let toks = al["paddingTokens"].as_array();
        let parts_pad: Vec<String> = pad.iter().enumerate().map(|(i, v)| tok(toks.and_then(|t| t.get(i)).unwrap_or(&Value::Null), v)).collect();
        parts.push(format!("pad=[{}]", parts_pad.join(",")));
    }
    if let Some(m) = al["alignMain"].as_str().filter(|m| *m != "start") {
        parts.push(format!("main={m}"));
    }
    if let Some(c) = al["alignCross"].as_str().filter(|c| *c != "start") {
        parts.push(format!("cross={c}"));
    }
    parts
}

fn compact_style(st: &Value) -> Vec<String> {
    let mut parts = Vec::new();
    for f in st["fills"].as_array().into_iter().flatten() {
        if f["type"] == "solid" {
            parts.push(format!("bg={}", tok(&f["var"], &f["color"])));
        } else if let Some(t) = f["type"].as_str() {
            parts.push(format!("bg={t}"));
        }
    }
    if !st["stroke"].is_null() {
        parts.push(format!("border={}/{}", tok(&st["strokeVar"], &st["stroke"]), px_num(st["strokeWidth"].as_f64().unwrap_or(1.0))));
    }
    if st["cornerRadius"].as_f64().unwrap_or(0.0) > 0.0 {
        parts.push(format!("r={}", tok(&st["cornerRadiusToken"], &st["cornerRadius"])));
    }
    if let Some(o) = st["opacity"].as_f64().filter(|o| *o != 1.0) {
        parts.push(format!("op={o}"));
    }
    if st["clip"] == true {
        parts.push("clip".into());
    }
    if let Some(more) = st["scroll"].as_f64() {
        parts.push(format!("scroll+{}", px_num(more)));
    }
    parts
}

fn compact_node(n: &Value, depth: usize, out: &mut String) {
    let ind = "  ".repeat(depth);
    let name = n["name"].as_str().unwrap_or("");
    let kind = n["kind"].as_str().unwrap_or("node");
    let line = match kind {
        "text" => {
            let t = &n["text"];
            let mut bits = vec![
                format!("Text {name:?}"),
                format!("{:?}", t["content"].as_str().unwrap_or("")),
                format!("{}/{}", tok(&t["fontSizeToken"], &t["fontSize"]), px_num(t["lineHeight"].as_f64().unwrap_or(0.0))),
                t["fontWeight"].as_u64().unwrap_or(400).to_string(),
            ];
            if let Some(a) = t["align"].as_str().filter(|a| *a != "left") {
                bits.push(a.to_owned());
            }
            bits.push(format!("color={}", tok(&n["style"]["fillVar"], &n["style"]["fill"])));
            if t["autoWidth"] != true {
                bits.push(format!("w={}", px_num(n["bounds"]["width"].as_f64().unwrap_or(0.0))));
            }
            bits.join(" ")
        }
        "icon" => {
            let i = &n["icon"];
            format!(
                "Icon {}:{} {} color={}",
                i["set"].as_str().unwrap_or(""),
                i["name"].as_str().unwrap_or(""),
                px_num(i["size"][0].as_f64().unwrap_or(0.0)),
                tok(&i["colorVar"], &i["color"])
            )
        }
        "library" => {
            let l = &n["library"];
            let usage = l["usage"].as_str().filter(|u| !u.is_empty()).or(l["react"].as_str()).unwrap_or("");
            format!("Kit {} {}", l["kit"].as_str().unwrap_or(""), usage.lines().next().unwrap_or(""))
        }
        _ if !n["instanceOf"].is_null() => {
            let of = n["instanceOf"]["component"].as_str().unwrap_or("");
            format!("Instance {name:?} of {of:?} {}", compact_size(n))
        }
        "vector" => format!("Vector {name:?} {}", compact_size(n)),
        _ => {
            let mut bits = vec![format!("{}{} {name:?}", kind[..1].to_ascii_uppercase(), &kind[1..]), compact_size(n)];
            bits.extend(compact_layout(&n["autoLayout"]));
            bits.extend(compact_style(&n["style"]));
            bits.join(" ")
        }
    };
    out.push_str(&format!("{ind}{line}\n"));
    if matches!(kind, "icon" | "library") || !n["instanceOf"].is_null() {
        return;
    }
    for c in n["children"].as_array().into_iter().flatten() {
        compact_node(c, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{Color, InstanceLink};

    fn sample_card_doc() -> (Document, NodeId) {
        let mut doc = Document::default();
        doc.variables
            .insert("surface".into(), Color::rgb(0x1e, 0x1e, 0x2e));
        doc.tokens.insert("spacing-md".into(), "16px".into());
        let comp_id = doc.alloc_id();
        let mut comp = Node::new(
            comp_id,
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 200.0, 80.0),
        );
        comp.name = "Card".into();
        comp.component = true;
        doc.nodes.insert(comp_id, comp);

        let root = doc.alloc_id();
        let mut frame = Node::new(
            root,
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 320.0, 120.0),
        );
        frame.name = "Card Instance".into();
        frame.instance = Some(InstanceLink {
            main: comp_id,
            overrides: Vec::new(),
            source: None,
            swap: None,
        });
        frame.layout = Some(AutoLayout {
            direction: Direction::Vertical,
            wrap: false,
            gap: 16.0,
            gap_auto: false,
            cross_gap: 0.0,
            padding: [12.0, 24.0, 12.0, 24.0],
            align_main: Align::Center,
            align_cross: Align::Start,
        });
        frame.style.fills = vec![Paint::solid_var(Color::rgb(0x1e, 0x1e, 0x2e), "surface")];
        frame.style.corner_radius = 12.0;

        let txt_id = doc.alloc_id();
        let mut txt = Node::new(
            txt_id,
            NodeKind::Text {
                text: "Hello".into(),
                font_size: 20.0,
                auto_width: true,
                align: TextAlign::Center,
            },
            Rect::new(0.0, 0.0, 60.0, 24.0),
        );
        txt.parent = Some(root);
        txt.text_style.bold = true;
        txt.style.fills = vec![Paint::solid(Color::WHITE)];
        frame.children.push(txt_id);

        doc.nodes.insert(root, frame);
        doc.nodes.insert(txt_id, txt);
        doc.root.push(root);
        (doc, root)
    }

    #[test]
    fn tailwind_jsx_maps_layout_and_styles() {
        let (doc, root) = sample_card_doc();
        let jsx = to_tailwind_jsx(&doc, root);
        assert!(jsx.contains("data-component=\"Card\""), "{jsx}");
        assert!(
            jsx.contains("flex flex-col justify-center gap-4 items-start py-3 px-6"),
            "{jsx}"
        );
        assert!(jsx.contains("bg-[var(--surface)]"), "{jsx}");
        assert!(jsx.contains("rounded-xl"), "{jsx}");
        assert!(
            jsx.contains("text-xl font-bold text-center text-[#ffffff]"),
            "{jsx}"
        );
    }

    #[test]
    fn html_css_includes_variables_and_rules() {
        let mut doc = Document::default();
        doc.variables
            .insert("accent".into(), Color::rgb(0x3b, 0x82, 0xf6));
        doc.tokens.insert("spacing-md".into(), "16px".into());
        let id = doc.alloc_id();
        let mut n = Node::new(
            id,
            NodeKind::Frame { clip: false },
            Rect::new(0.0, 0.0, 100.0, 50.0),
        );
        n.name = "Box".into();
        n.style.fills = vec![Paint::solid_var(Color::rgb(0x3b, 0x82, 0xf6), "accent")];
        doc.nodes.insert(id, n);
        doc.root.push(id);

        let html = to_html_css(&doc, id);
        assert!(html.contains("<style>"), "{html}");
        assert!(html.contains("--accent: #3b82f6;"), "{html}");
        assert!(html.contains("--spacing-md: 16px;"), "{html}");
        assert!(html.contains("background-color: var(--accent);"), "{html}");
        assert!(
            html.contains("<div class=\"node-1\" data-name=\"Box\"></div>"),
            "{html}"
        );
    }

    #[test]
    fn tailwind_and_dtcg_token_exports() {
        let mut doc = Document::default();
        doc.variables
            .insert("primary".into(), Color::rgb(0x11, 0x22, 0x33));
        let mut dark = std::collections::BTreeMap::new();
        dark.insert("primary".into(), Color::rgb(0xee, 0xdd, 0xcc));
        doc.modes.insert("Dark".into(), dark);
        doc.tokens.insert("radius-sm".into(), "6px".into());

        let tw = export_variables_tailwind(&doc);
        assert!(tw.contains("@theme {\n  --color-primary: #112233;\n  --radius-sm: 6px;\n}"));
        assert!(tw.contains("[data-theme=\"Dark\"] {\n  --color-primary: #eeddcc;\n}"));

        let dtcg = export_variables_dtcg(&doc);
        assert_eq!(dtcg["primary"]["$type"], "color");
        assert_eq!(dtcg["primary"]["$value"], "#112233");
        assert_eq!(dtcg["primary"]["$extensions"]["modes"]["Dark"], "#eeddcc");
        assert_eq!(dtcg["radius-sm"]["$type"], "dimension");
        assert_eq!(dtcg["radius-sm"]["$value"], "6px");
    }

    #[test]
    fn react_tsx_wraps_tailwind_and_tokens() {
        let (doc, root) = sample_card_doc();
        let tsx = to_react_tsx(&doc, root);
        assert!(tsx.contains("--surface: #1e1e2e;"), "{tsx}");
        assert!(tsx.contains("--spacing-md: 16px;"), "{tsx}");
        assert!(tsx.contains("export function CardInstance()"), "{tsx}");
        assert!(tsx.contains("bg-[var(--surface)]"), "{tsx}");
    }

    #[test]
    fn angular_standalone_component_generation() {
        let (doc, root) = sample_card_doc();
        let ng = to_angular(&doc, root);
        assert!(
            ng.contains("import { Component, ChangeDetectionStrategy } from '@angular/core';"),
            "{ng}"
        );
        assert!(ng.contains("selector: 'app-card-instance'"), "{ng}");
        assert!(ng.contains("standalone: true"), "{ng}");
        assert!(ng.contains("--surface: #1e1e2e;"), "{ng}");
        assert!(ng.contains("background-color: var(--surface);"), "{ng}");
        assert!(ng.contains("data-component=\"Card\""), "{ng}");
        assert!(ng.contains("export class CardInstanceComponent {}"), "{ng}");
    }

    #[test]
    fn egui_rust_generation() {
        let (doc, root) = sample_card_doc();
        let rs = to_egui(&doc, root);
        assert!(rs.contains("pub mod tokens"), "{rs}");
        assert!(
            rs.contains(
                "pub const SURFACE: egui::Color32 = egui::Color32::from_rgba_unmultiplied(30, 30, 46, 255);"
            ),
            "{rs}"
        );
        assert!(rs.contains("pub const SPACING_MD: f32 = 16.0;"), "{rs}");
        assert!(rs.contains("pub fn render(ui: &mut egui::Ui)"), "{rs}");
        assert!(rs.contains(".fill(tokens::SURFACE)"), "{rs}");
        assert!(rs.contains("ui.vertical(|ui|"), "{rs}");
        assert!(
            rs.contains("egui::RichText::new(\"Hello\").size(20.0 as f32)"),
            "{rs}"
        );
    }

    #[test]
    fn ai_spec_and_markdown_include_rules_and_reference_code() {
        let (doc, root) = sample_card_doc();
        let spec = to_ai_spec(&doc, root, "angular").unwrap();
        assert_eq!(spec["target"], "angular");
        assert_eq!(spec["rootId"], root.0);
        assert_eq!(spec["tokens"]["colors"]["surface"], "#1e1e2e");
        assert_eq!(spec["components"][0]["componentName"], "Card");
        assert_eq!(spec["layoutTree"]["autoLayout"]["direction"], "column");
        let rules = spec["enforcementRules"].as_array().unwrap();
        assert!(
            rules
                .iter()
                .any(|r| r.as_str().unwrap_or("").contains("--surface")),
            "{rules:?}"
        );
        assert!(
            rules
                .iter()
                .any(|r| r.as_str().unwrap_or("").contains("\"Hello\"")),
            "{rules:?}"
        );

        let md = to_ai_spec_markdown(&doc, root, "angular").unwrap();
        assert!(
            md.contains("# AI UI Implementation Spec: Card Instance (angular)"),
            "{md}"
        );
        assert!(md.contains("## 1. Mandatory Enforcement Rules"), "{md}");
        assert!(md.contains("## 2. Design Tokens & Variables"), "{md}");
        assert!(md.contains("## 3. Structured Layout Hierarchy"), "{md}");
        assert!(
            md.contains("## 4. Reference Implementation (angular)"),
            "{md}"
        );
    }

    #[test]
    fn linked_components_export_as_library_code() {
        let mut doc = Document::default();
        let frame = |doc: &mut Document, name: &str| {
            let id = doc.alloc_id();
            let mut n = Node::new(id, NodeKind::Frame { clip: false }, Rect::new(0.0, 0.0, 90.0, 36.0));
            n.name = name.into();
            doc.nodes.insert(id, n);
            id
        };
        let label = |doc: &mut Document, parent: NodeId, text: &str| {
            let id = doc.alloc_id();
            let mut t = Node::new(id, NodeKind::Text { text: text.into(), font_size: 14.0, auto_width: true, align: TextAlign::Left }, Rect::new(0.0, 0.0, 50.0, 20.0));
            t.name = "Label".into();
            t.parent = Some(parent);
            doc.nodes.insert(id, t);
            doc.nodes.get_mut(&parent).unwrap().children.push(id);
        };
        let main = frame(&mut doc, "Button / Outline");
        label(&mut doc, main, "Button");
        {
            let m = doc.nodes.get_mut(&main).unwrap();
            m.component = true;
            m.code = Some(crate::model::CodeLink {
                kit: "shadcn".into(),
                import: r#"import { Button } from "@/components/ui/button""#.into(),
                react: r#"<Button variant="outline">{Label}</Button>"#.into(),
                egui: "ui.button({Label});".into(),
            });
        }
        let screen = frame(&mut doc, "Screen");
        doc.root.push(screen);
        for text in ["Save", "Cancel"] {
            let inst = frame(&mut doc, "Button / Outline");
            label(&mut doc, inst, text);
            let n = doc.nodes.get_mut(&inst).unwrap();
            n.instance = Some(InstanceLink { main, overrides: Vec::new(), source: None, swap: None });
            n.parent = Some(screen);
            doc.nodes.get_mut(&screen).unwrap().children.push(inst);
        }
        let tsx = to_react_tsx(&doc, screen);
        assert_eq!(tsx.matches("import { Button }").count(), 1, "{tsx}");
        assert!(tsx.contains(r#"<Button variant="outline">Save</Button>"#) && tsx.contains(r#"<Button variant="outline">Cancel</Button>"#), "{tsx}");
        assert!(!tsx.contains("data-name=\"Label\""), "the layers under a linked component are not rebuilt: {tsx}");
        let rs = to_egui(&doc, screen);
        assert!(rs.contains(r#"ui.button("Save");"#), "{rs}");
        let spec = to_ai_spec(&doc, screen, "react").unwrap();
        assert_eq!(spec["libraryComponents"][0]["usedBy"].as_array().unwrap().len(), 2);
        assert!(spec["enforcementRules"].as_array().unwrap().iter().any(|r| r.as_str().unwrap().contains("MUST use the shadcn component")));
    }
}
