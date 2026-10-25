//! AI design-to-code enforcer: verifies generated Web (React, Angular, HTML+CSS) and Rust (`egui`)
//! code against a PaperCraft frame's layout, tokens, components, and text content.

use crate::model::{Direction, Document, NodeId, NodeKind, Paint};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Violation {
    pub rule: &'static str,
    pub severity: &'static str, // "error" or "warning"
    pub node_id: u64,
    pub node_name: String,
    pub expected: String,
    pub found: String,
    pub fix: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EnforcementReport {
    pub passed: bool,
    pub score: u32, // 0..=100
    pub target: String,
    pub errors: usize,
    pub warnings: usize,
    pub checked_nodes: usize,
    pub violations: Vec<Violation>,
}

fn normalize_target(target: &str) -> Result<&'static str, String> {
    match target.trim().to_ascii_lowercase().as_str() {
        "react" | "tsx" => Ok("react"),
        "tailwind" | "jsx" => Ok("tailwind"),
        "angular" | "ng" => Ok("angular"),
        "html" | "css" => Ok("html"),
        "egui" | "rust" => Ok("egui"),
        other => Err(format!(
            "unknown enforcement target {other:?}; expected react, tailwind, angular, html, or egui"
        )),
    }
}

fn is_visible_in_subtree(doc: &Document, root: NodeId, id: NodeId) -> bool {
    let mut cur = Some(id);
    while let Some(c) = cur {
        let Some(n) = doc.get(c) else { return false };
        if !n.visible {
            return false;
        }
        if c == root {
            return true;
        }
        cur = n.parent;
    }
    false
}

fn visible_subtree(doc: &Document, root: NodeId) -> Vec<NodeId> {
    doc.subtree(root)
        .into_iter()
        .filter(|&id| is_visible_in_subtree(doc, root, id))
        .collect()
}

fn escape_html(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn text_snippet_present(code: &str, snippet: &str) -> bool {
    if code.contains(snippet) || code.contains(&escape_html(snippet)) {
        return true;
    }
    if let Ok(json_escaped) = serde_json::to_string(snippet) {
        let inner = json_escaped.trim_matches('"');
        if code.contains(inner) {
            return true;
        }
    }
    false
}

/// Strip CSS variable declaration blocks (`:root { ... }`, `@theme { ... }`, `:host { ... }`,
/// `[data-mode...] { ... }`, `[data-theme...] { ... }`) so hex literals inside token definitions
/// are not flagged as hardcoded style values.
fn strip_token_declaration_blocks(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    while i < code.len() {
        let rest = &code[i..];
        // Byte-safe prefix test: slicing `rest[..n]` panics when byte n falls inside a
        // multibyte char (a `ó` in "Rótulo" took the app down on a real screen's code).
        let starts_decl = [":root", "@theme", ":host", "[data-mode", "[data-theme"]
            .iter()
            .any(|p| rest.get(..p.len()).is_some_and(|head| head.eq_ignore_ascii_case(p)));
        if starts_decl && let Some(open_rel) = rest.find('{') {
            let mut depth = 0usize;
            let mut end_idx = None;
            for (rel, ch) in rest[open_rel..].char_indices() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end_idx = Some(i + open_rel + rel + ch.len_utf8());
                        break;
                    }
                }
            }
            if let Some(next_i) = end_idx {
                i = next_i;
                continue;
            }
        }
        if let Some(ch) = rest.chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

fn web_var_referenced(code: &str, var_name: &str) -> bool {
    let outside = strip_token_declaration_blocks(code);
    let css_var = format!("--{var_name}");
    let tw_color_var = format!("--color-{var_name}");
    outside.contains(&css_var) || outside.contains(&tw_color_var)
}

fn count_web_absolute(code: &str) -> usize {
    let lower = code.to_ascii_lowercase();
    let mut count = 0usize;

    // Count CSS `position: absolute` / `position:absolute`
    let mut rest = lower.as_str();
    while let Some(pos) = rest.find("position") {
        let after = &rest[pos + "position".len()..];
        let after_trim = after.trim_start();
        if let Some(after_colon) = after_trim.strip_prefix(':') {
            let val = after_colon.trim_start();
            if val.starts_with("absolute") {
                let next_ch = val.as_bytes().get("absolute".len()).copied();
                if !next_ch.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
                    count += 1;
                }
            }
        }
        rest = after;
    }

    // Count standalone Tailwind `absolute` class tokens inside class/className strings
    for attr in ["classname=", "class="] {
        let mut scan = lower.as_str();
        while let Some(idx) = scan.find(attr) {
            let after_attr = &scan[idx + attr.len()..].trim_start();
            let quote = after_attr.as_bytes().first().copied();
            if let Some(q) = quote
                && (q == b'"' || q == b'\'' || q == b'`')
            {
                let inner = &after_attr[1..];
                if let Some(end) = inner.find(q as char) {
                    let class_str = &inner[..end];
                    for token in class_str.split_whitespace() {
                        let base = token.rsplit(':').next().unwrap_or(token);
                        if base == "absolute" {
                            count += 1;
                        }
                    }
                    scan = &inner[end + 1..];
                    continue;
                }
            }
            scan = after_attr;
        }
    }

    count
}

fn px_num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.trunc()).abs() < 1e-9 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn tw_gap_scale(px: f64) -> Option<&'static str> {
    let r = (px * 100.0).round() / 100.0;
    match (r * 10.0).round() as i64 {
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
    }
}

fn web_gap_represented(doc: &Document, code: &str, gap: f64) -> bool {
    let num = px_num(gap);
    let px_str = format!("{num}px");
    if code.contains(&px_str) {
        return true;
    }
    if let Some(scale) = tw_gap_scale(gap) {
        for prefix in ["gap-", "gap-x-", "gap-y-", "space-x-", "space-y-"] {
            let pat = format!("{prefix}{scale}");
            if code.contains(&pat) {
                return true;
            }
        }
    }
    let rem = gap / 16.0;
    let rem_str = format!("{}rem", px_num(rem));
    if code.contains(&rem_str) {
        return true;
    }
    // Check document spacing tokens matching `{num}px`
    for (k, v) in &doc.tokens {
        if v.trim() == px_str && code.contains(&format!("--{k}")) {
            return true;
        }
    }
    if code.contains("var(--gap") || code.contains("var(--space") || code.contains("var(--spacing")
    {
        return true;
    }
    false
}

fn egui_gap_represented(code: &str, gap: f64) -> bool {
    let num = px_num(gap);
    code.contains("item_spacing")
        || code.contains(&format!("add_space({num})"))
        || code.contains(&format!("add_space({num}.0)"))
}

fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = String::new();
                    out.push(first.to_ascii_uppercase());
                    out.push_str(&chars.as_str().to_ascii_lowercase());
                    out
                }
                None => String::new(),
            }
        })
        .collect()
}

fn to_kebab_case(s: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if !c.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
        } else if c.is_ascii_uppercase() && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
            cur.push(c.to_ascii_lowercase());
        } else {
            cur.push(c.to_ascii_lowercase());
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words.join("-")
}

fn component_referenced(code: &str, comp_name: &str, is_egui: bool) -> bool {
    let trimmed = comp_name.trim();
    if trimmed.is_empty() {
        return true;
    }
    let pascal = to_pascal_case(trimmed);
    let kebab = to_kebab_case(trimmed);
    let snake = kebab.replace('-', "_");

    if code.contains(trimmed)
        || (!pascal.is_empty() && code.contains(&pascal))
        || (!kebab.is_empty() && code.contains(&kebab))
    {
        return true;
    }
    if is_egui && !snake.is_empty() && code.to_ascii_lowercase().contains(&snake) {
        return true;
    }
    false
}

pub fn verify_code(
    doc: &Document,
    root: NodeId,
    target: &str,
    code: &str,
) -> Result<EnforcementReport, String> {
    let Some(root_node) = doc.get(root) else {
        return Err(format!("missing node {root}"));
    };
    let norm_target = normalize_target(target)?;
    let is_egui = norm_target == "egui";

    // Annotation comments describe rules, not rendered content or layout.
    let source = code.lines().filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>().join("\n");
    let code = source.as_str();
    let visible_ids = visible_subtree(doc, root);

    let mut violations: Vec<Violation> = Vec::new();
    let outside_decls = strip_token_declaration_blocks(code);
    let outside_lower = outside_decls.to_ascii_lowercase();

    // 1 & 2 & 5 & 6: Walk visible nodes
    let mut out_of_flow_nodes = 0usize;
    let mut has_any_auto_layout = false;
    let mut needs_horizontal = false;
    let mut needs_vertical = false;

    for &id in &visible_ids {
        let Some(n) = doc.get(id) else { continue };

        if id != root {
            let parent_has_layout = n
                .parent
                .and_then(|p| doc.get(p))
                .is_some_and(|p| p.layout.is_some());
            if n.layout_child.absolute || !parent_has_layout {
                out_of_flow_nodes += 1;
            }
        }

        if let Some(l) = &n.layout {
            has_any_auto_layout = true;
            if !n.children.is_empty() {
                match l.direction {
                    Direction::Horizontal => needs_horizontal = true,
                    Direction::Vertical => needs_vertical = true,
                }
            }
        }

        // Rule 1: missing-text (error)
        if let NodeKind::Text { text, .. } = &n.kind {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                for line in trimmed.lines() {
                    let line = line.trim();
                    if !line.is_empty() && !text_snippet_present(code, line) {
                        violations.push(Violation {
                            rule: "missing-text",
                            severity: "error",
                            node_id: id.0,
                            node_name: n.name.clone(),
                            expected: line.to_string(),
                            found: "missing in generated code".into(),
                            fix: format!("Include exact text {:?} from layer {:?}", line, n.name),
                        });
                    }
                }
            }
        }

        // Rule 2: hardcoded-color (error)
        for fill in &n.style.fills {
            if let Paint::Solid {
                color,
                var: Some(var_name),
            } = fill
            {
                let hex = color.to_hex().to_ascii_lowercase();
                let upper_var = var_name.to_ascii_uppercase().replace('-', "_");
                if is_egui {
                    let has_token = code.contains(&upper_var) || code.contains(var_name.as_str());
                    if !has_token {
                        let rgb_tuple = format!("{}, {}, {}", color.r, color.g, color.b);
                        let found = if code.contains(&rgb_tuple) || outside_lower.contains(&hex) {
                            format!("raw color ({rgb_tuple}) / {hex}")
                        } else {
                            format!("missing token {upper_var}")
                        };
                        violations.push(Violation {
                            rule: "hardcoded-color",
                            severity: "error",
                            node_id: id.0,
                            node_name: n.name.clone(),
                            expected: format!("tokens::{upper_var}"),
                            found,
                            fix: format!(
                                "Use design token constant `tokens::{upper_var}` instead of raw color {hex}"
                            ),
                        });
                    }
                } else {
                    let has_var = web_var_referenced(code, var_name);
                    let has_raw_hex = outside_lower.contains(&hex);
                    if !has_var {
                        violations.push(Violation {
                            rule: "hardcoded-color",
                            severity: "error",
                            node_id: id.0,
                            node_name: n.name.clone(),
                            expected: format!("var(--{var_name})"),
                            found: if has_raw_hex {
                                hex.clone()
                            } else {
                                format!("missing --{var_name}")
                            },
                            fix: format!(
                                "Use `var(--{var_name})` (or `bg-[var(--{var_name})]` / `text-[var(--{var_name})]`) instead of hardcoded {hex}"
                            ),
                        });
                    }
                }
            }
        }

        // Rule 5: missing-component (warning)
        if let Some(inst) = n.instance.as_ref()
            && let Some(main_comp) = doc.get(inst.main)
            && main_comp.component
            && !component_referenced(code, &main_comp.name, is_egui)
        {
            let pascal = to_pascal_case(&main_comp.name);
            let kebab = to_kebab_case(&main_comp.name);
            violations.push(Violation {
                rule: "missing-component",
                severity: "warning",
                node_id: id.0,
                node_name: n.name.clone(),
                expected: main_comp.name.clone(),
                found: "inlined / unnamed markup".into(),
                fix: if is_egui {
                    format!(
                        "Reference component `{}` (e.g. `{}` widget/helper)",
                        main_comp.name,
                        kebab.replace('-', "_")
                    )
                } else {
                    format!(
                        "Instantiate component `<{pascal} />`, `<app-{kebab}>`, or `data-component=\"{}\"`",
                        main_comp.name
                    )
                },
            });
        }

        // Rule 6: spacing-drift (warning)
        if let Some(l) = &n.layout
            && !n.children.is_empty()
            && l.gap > 0.0
            && !l.gap_auto
        {
            let ok = if is_egui {
                egui_gap_represented(code, l.gap)
            } else {
                web_gap_represented(doc, code, l.gap)
            };
            if !ok {
                let gap_str = px_num(l.gap);
                violations.push(Violation {
                    rule: "spacing-drift",
                    severity: "warning",
                    node_id: id.0,
                    node_name: n.name.clone(),
                    expected: format!("{gap_str}px gap"),
                    found: "gap not found in code".into(),
                    fix: if is_egui {
                        format!(
                            "Set `item_spacing` or call `ui.add_space({gap_str}.0)` on container {:?}",
                            n.name
                        )
                    } else {
                        let tw_hint = tw_gap_scale(l.gap)
                            .map(|s| format!("`gap-{s}` or "))
                            .unwrap_or_default();
                        format!(
                            "Apply {tw_hint}`gap: {gap_str}px` on container {:?}",
                            n.name
                        )
                    },
                });
            }
        }
    }

    // Rule 3: forbidden-absolute (error)
    if !is_egui {
        let code_abs = count_web_absolute(code);
        if code_abs > out_of_flow_nodes {
            violations.push(Violation {
                rule: "forbidden-absolute",
                severity: "error",
                node_id: root.0,
                node_name: root_node.name.clone(),
                expected: format!("at most {out_of_flow_nodes} absolute node(s)"),
                found: format!("{code_abs} absolute positioning occurrence(s)"),
                fix: "Flow children in auto-layout frames must use Flexbox/Grid, never position: absolute".into(),
            });
        }
    }

    // Rule 4: missing-layout (error)
    if has_any_auto_layout {
        if is_egui {
            let has_grid = code.contains("egui::Grid") || code.contains("Grid::new");
            let has_horiz = code.contains("ui.horizontal") || code.contains(".horizontal(");
            let has_vert = code.contains("ui.vertical") || code.contains(".vertical(");
            let any_layout = has_grid || has_horiz || has_vert;
            let dir_ok = has_grid
                || ((!needs_horizontal || has_horiz)
                    && (!needs_vertical || has_vert)
                    && any_layout);
            if !dir_ok {
                violations.push(Violation {
                    rule: "missing-layout",
                    severity: "error",
                    node_id: root.0,
                    node_name: root_node.name.clone(),
                    expected: if needs_horizontal && needs_vertical {
                        "ui.horizontal + ui.vertical (or egui::Grid)".into()
                    } else if needs_horizontal {
                        "ui.horizontal (or egui::Grid)".into()
                    } else {
                        "ui.vertical (or egui::Grid)".into()
                    },
                    found: "no matching egui layout container".into(),
                    fix: "Use `ui.horizontal(|ui| ...)` / `ui.vertical(|ui| ...)` or `egui::Grid` matching the frame's auto-layout direction".into(),
                });
            }
        } else {
            let lower = code.to_ascii_lowercase();
            let has_flex_or_grid = lower.contains("flex")
                || lower.contains("display: flex")
                || lower.contains("display:flex")
                || lower.contains("grid");
            if !has_flex_or_grid {
                violations.push(Violation {
                    rule: "missing-layout",
                    severity: "error",
                    node_id: root.0,
                    node_name: root_node.name.clone(),
                    expected: "flex or grid layout".into(),
                    found: "no flex/grid layout found".into(),
                    fix: "Use Flexbox (`flex`, `display: flex`) or CSS Grid (`grid`, `display: grid`) for auto-layout frames".into(),
                });
            }
        }
    }

    let errors = violations.iter().filter(|v| v.severity == "error").count();
    let warnings = violations
        .iter()
        .filter(|v| v.severity == "warning")
        .count();
    let score = 100u32.saturating_sub((errors * 15 + warnings * 5) as u32);
    let passed = errors == 0;

    Ok(EnforcementReport {
        passed,
        score,
        target: norm_target.to_string(),
        errors,
        warnings,
        checked_nodes: visible_ids.len(),
        violations,
    })
}

pub fn to_json(r: &EnforcementReport) -> serde_json::Value {
    serde_json::to_value(r).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{
        Align, AutoLayout, Color, Direction, InstanceLink, Node, NodeKind, Paint, TextAlign,
    };

    fn sample_doc() -> (Document, NodeId) {
        let mut doc = Document::default();
        let surface = Color::rgb(0x1e, 0x1e, 0x2e);
        doc.variables.insert("surface".into(), surface);

        let root = doc.alloc_id();
        let mut frame = Node::new(
            root,
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 320.0, 120.0),
        );
        frame.name = "Action Card".into();
        frame.layout = Some(AutoLayout {
            direction: Direction::Vertical,
            wrap: false,
            gap: 16.0,
            gap_auto: false,
            cross_gap: 0.0,
            padding: [16.0; 4],
            align_main: Align::Center,
            align_cross: Align::Center,
        });
        frame.style.fills = vec![Paint::solid_var(surface, "surface")];

        let txt_id = doc.alloc_id();
        let mut txt = Node::new(
            txt_id,
            NodeKind::Text {
                text: "Continue".into(),
                font_size: 16.0,
                auto_width: true,
                align: TextAlign::Center,
            },
            Rect::new(0.0, 0.0, 80.0, 20.0),
        );
        txt.name = "CTA Label".into();
        txt.parent = Some(root);
        txt.style.fills = vec![Paint::solid(Color::WHITE)];
        frame.children.push(txt_id);

        doc.nodes.insert(root, frame);
        doc.nodes.insert(txt_id, txt);
        doc.root.push(root);
        (doc, root)
    }

    #[test]
    fn valid_tailwind_angular_and_egui_pass() {
        let (doc, root) = sample_doc();

        let tw_code = r#"
            <div className="flex flex-col gap-4 p-4 bg-[var(--surface)]">
              <span className="text-base text-white">Continue</span>
            </div>
        "#;
        let tw_report = verify_code(&doc, root, "tailwind", tw_code).unwrap();
        assert!(tw_report.passed, "{tw_report:?}");
        assert_eq!(tw_report.errors, 0);
        assert!(tw_report.score >= 95);

        let ng_code = r#"
            @Component({
              selector: 'app-action-card',
              template: `<div class="card"><span>Continue</span></div>`,
              styles: [`
                :host { --surface: #1e1e2e; }
                .card { display: flex; flex-direction: column; gap: 16px; background: var(--surface); }
              `]
            })
            export class ActionCardComponent {}
        "#;
        let ng_report = verify_code(&doc, root, "angular", ng_code).unwrap();
        assert!(ng_report.passed, "{ng_report:?}");
        assert_eq!(ng_report.errors, 0);
        assert!(ng_report.score >= 95);

        let egui_code = r#"
            egui::Frame::none().fill(tokens::SURFACE).show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(16.0, 16.0);
                ui.vertical(|ui| {
                    ui.label("Continue");
                });
            });
        "#;
        let egui_report = verify_code(&doc, root, "egui", egui_code).unwrap();
        assert!(egui_report.passed, "{egui_report:?}");
        assert_eq!(egui_report.errors, 0);
        assert!(egui_report.score >= 95);
    }

    #[test]
    fn broken_code_reports_missing_text_hardcoded_color_and_forbidden_absolute() {
        let (doc, root) = sample_doc();

        let bad_code = r#"
            <div className="flex flex-col gap-4 bg-[#1e1e2e]">
              <span className="absolute left-0 top-0">Wrong Label</span>
            </div>
        "#;
        let report = verify_code(&doc, root, "react", bad_code).unwrap();
        assert!(!report.passed);
        assert!(report.errors >= 3);
        let rules: Vec<&str> = report.violations.iter().map(|v| v.rule).collect();
        assert!(rules.contains(&"missing-text"), "{rules:?}");
        assert!(rules.contains(&"hardcoded-color"), "{rules:?}");
        assert!(rules.contains(&"forbidden-absolute"), "{rules:?}");

        let json_val = to_json(&report);
        assert_eq!(json_val["passed"], false);
    }

    #[test]
    fn detects_missing_layout_component_and_spacing_drift() {
        let (mut doc, root) = sample_doc();
        let comp_id = doc.alloc_id();
        let mut comp = Node::new(
            comp_id,
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 100.0, 40.0),
        );
        comp.name = "PrimaryButton".into();
        comp.component = true;
        doc.nodes.insert(comp_id, comp);
        doc.nodes.get_mut(&root).unwrap().instance = Some(InstanceLink {
            main: comp_id,
            overrides: Vec::new(),
            source: None,
            swap: None,
        });

        let drift_code = r#"
            <div className="bg-[var(--surface)]">
              <span>Continue</span>
            </div>
        "#;
        let report = verify_code(&doc, root, "tailwind", drift_code).unwrap();
        assert!(!report.passed);
        let rules: Vec<&str> = report.violations.iter().map(|v| v.rule).collect();
        assert!(rules.contains(&"missing-layout"), "{rules:?}");
        assert!(rules.contains(&"missing-component"), "{rules:?}");
        assert!(rules.contains(&"spacing-drift"), "{rules:?}");
    }
}
