//! A small markdown renderer for chat replies: paragraphs, headings, bullet and numbered lists,
//! fenced code blocks (with a copy button), bold, italic, inline code and links. Unclosed markers
//! stay literal, so half-streamed text never flickers into a different shape.

use crate::theme::Surface as _;
use crate::theme;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, FontFamily, FontId, Id, RichText, Ui};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Para(Vec<Span>),
    Heading(Vec<Span>),
    /// `start` is Some for numbered lists.
    List { start: Option<usize>, items: Vec<Vec<Span>> },
    Code { lang: String, text: String },
}

#[cfg(test)]
fn plain(text: &str) -> Span {
    Span { text: text.into(), ..Default::default() }
}

/// Position of `pat` in `chars` at or after `from`.
fn find(chars: &[char], from: usize, pat: &str) -> Option<usize> {
    let p: Vec<char> = pat.chars().collect();
    (from..chars.len().saturating_sub(p.len() - 1)).find(|&i| chars[i..i + p.len()] == p[..])
}

pub fn inline(s: &str) -> Vec<Span> {
    let c: Vec<char> = s.chars().collect();
    let (mut out, mut buf) = (Vec::<Span>::new(), String::new());
    let (mut bold, mut italic) = (false, false);
    let mut i = 0;
    // Once a `[` finds no `](...)` after it, no later `[` can: stop scanning (keeps this linear).
    let mut no_more_links = false;
    let flush = |out: &mut Vec<Span>, buf: &mut String, bold, italic| {
        if !buf.is_empty() {
            out.push(Span { text: std::mem::take(buf), bold, italic, ..Default::default() });
        }
    };
    while i < c.len() {
        let rest_has = |pat: &str| find(&c, i + pat.chars().count(), pat).is_some();
        match c[i] {
            '`' if rest_has("`") => {
                flush(&mut out, &mut buf, bold, italic);
                let end = find(&c, i + 1, "`").expect("checked");
                out.push(Span { text: c[i + 1..end].iter().collect(), code: true, ..Default::default() });
                i = end + 1;
            }
            '*' if c.get(i + 1) == Some(&'*') && (bold || rest_has("**")) => {
                flush(&mut out, &mut buf, bold, italic);
                bold = !bold;
                i += 2;
            }
            '*' if c.get(i + 1) != Some(&'*') && (italic || rest_has("*")) => {
                flush(&mut out, &mut buf, bold, italic);
                italic = !italic;
                i += 1;
            }
            '[' => {
                let link = if no_more_links { None } else { find(&c, i + 1, "](").and_then(|m| Some((m, find(&c, m + 2, ")")?))) };
                no_more_links |= link.is_none();
                match link {
                    Some((m, end)) => {
                        flush(&mut out, &mut buf, bold, italic);
                        out.push(Span {
                            text: c[i + 1..m].iter().collect(),
                            bold,
                            italic,
                            link: Some(c[m + 2..end].iter().collect()),
                            ..Default::default()
                        });
                        i = end + 1;
                    }
                    None => {
                        buf.push('[');
                        i += 1;
                    }
                }
            }
            ch => {
                buf.push(ch);
                i += 1;
            }
        }
    }
    flush(&mut out, &mut buf, bold, italic);
    out
}

/// `- x`, `* x`, `+ x` -> (None, "x"); `3. x` -> (Some(3), "x").
fn list_item(line: &str) -> Option<(Option<usize>, &str)> {
    let t = line.trim_start();
    if let Some(r) = ["- ", "* ", "+ "].iter().find_map(|m| t.strip_prefix(m)) {
        return Some((None, r));
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    let r = t[digits..].strip_prefix(". ")?;
    (digits > 0).then(|| (t[..digits].parse().ok(), r))
}

pub fn parse(src: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut lines = src.lines();
    let end_para = |out: &mut Vec<Block>, para: &mut Vec<&str>| {
        if !para.is_empty() {
            out.push(Block::Para(inline(&para.join(" "))));
            para.clear();
        }
    };
    while let Some(line) = lines.next() {
        let t = line.trim_start();
        if let Some(lang) = t.strip_prefix("```") {
            end_para(&mut out, &mut para);
            let mut body: Vec<&str> = Vec::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                body.push(l);
            }
            out.push(Block::Code { lang: lang.trim().into(), text: body.join("\n") });
        } else if t.is_empty() {
            end_para(&mut out, &mut para);
        } else if t.starts_with('#') && t.trim_start_matches('#').starts_with(' ') {
            end_para(&mut out, &mut para);
            out.push(Block::Heading(inline(t.trim_start_matches('#').trim())));
        } else if let Some((start, text)) = list_item(line) {
            end_para(&mut out, &mut para);
            match out.last_mut() {
                Some(Block::List { start: s, items }) if s.is_some() == start.is_some() => items.push(inline(text)),
                _ => out.push(Block::List { start, items: vec![inline(text)] }),
            }
        } else {
            para.push(t);
        }
    }
    end_para(&mut out, &mut para);
    out
}

fn job(ui: &Ui, spans: &[Span], size: f32, heading: bool) -> LayoutJob {
    let p = theme::pal(ui);
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    for s in spans {
        let font = match (s.code, s.bold || heading) {
            (true, _) => FontId::new(size - 1.0, FontFamily::Monospace),
            (false, true) => theme::semibold(size),
            _ => FontId::proportional(size),
        };
        let mut f = TextFormat { font_id: font, color: p.text, italics: s.italic, ..Default::default() };
        if s.code {
            f.background = p.field;
            f.color = theme::accent();
        }
        job.append(&s.text, 0.0, f);
    }
    job
}

/// A run of spans. Links make it a row of separate widgets (so each can be clicked).
fn spans(ui: &mut Ui, spans: &[Span], size: f32, heading: bool) {
    if !spans.iter().any(|s| s.link.is_some()) {
        ui.add(egui::Label::new(job(ui, spans, size, heading)).selectable(true));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for s in spans {
            match &s.link {
                Some(url) => {
                    ui.hyperlink_to(RichText::new(&s.text).size(size).color(theme::accent()), url);
                }
                None => {
                    ui.add(egui::Label::new(job(ui, std::slice::from_ref(s), size, heading)).selectable(true));
                }
            }
        }
    });
}

fn code_block(ui: &mut Ui, lang: &str, text: &str) {
    let p = theme::pal(ui);
    egui::Frame::new().fill(p.field).corner_radius(theme::radius_lg()).inner_margin(egui::Margin::symmetric(12, 10)).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let label = if lang.is_empty() { "code" } else { lang };
            ui.label(RichText::new(label).font(theme::semibold(theme::SECONDARY)).color(p.secondary));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if super::kit::lottie_button(ui, "copy", "Copy code", 22.0).clicked() {
                    ui.ctx().copy_text(text.to_owned());
                }
            });
        });
        ui.add(egui::Label::new(RichText::new(text).font(FontId::new(theme::SECONDARY + 1.0, FontFamily::Monospace)).color(p.text)).wrap().selectable(true));
    });
}

/// Parsed blocks of one message, kept while its text does not change.
#[derive(Clone)]
struct Parsed {
    len: usize,
    hash: u64,
    blocks: Arc<Vec<Block>>,
}

fn hash_of(src: &str) -> u64 {
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

/// Draws `src` as markdown in `ui`. `id` names the message: its parse is cached under it and
/// redone only when the text changes (streaming), not every frame.
pub fn render(ui: &mut Ui, id: Id, src: &str, size: f32) {
    let p = theme::pal(ui);
    ui.spacing_mut().item_spacing.y = 6.0;
    let hash = hash_of(src);
    let parsed = match ui.data(|d| d.get_temp::<Parsed>(id)).filter(|c| c.len == src.len() && c.hash == hash) {
        Some(c) => c.blocks,
        None => {
            let blocks = Arc::new(parse(src));
            ui.data_mut(|d| d.insert_temp(id, Parsed { len: src.len(), hash, blocks: blocks.clone() }));
            blocks
        }
    };
    for block in parsed.iter() {
        match block {
            Block::Para(s) => spans(ui, s, size, false),
            Block::Heading(s) => spans(ui, s, size + 1.0, true),
            Block::Code { lang, text } => code_block(ui, lang, text),
            Block::List { start, items } => {
                ui.spacing_mut().item_spacing.y = 3.0;
                for (n, item) in items.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        let mark = start.map_or_else(|| "\u{2022}".to_owned(), |s| format!("{}.", s + n));
                        ui.add_sized([16.0, 0.0], egui::Label::new(RichText::new(mark).size(size).color(p.secondary)));
                        ui.vertical(|ui| spans(ui, item, size, false));
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_styles() {
        let s = inline("a **b** *c* `d` [e](http://x)");
        let kinds: Vec<_> = s.iter().map(|s| (s.text.as_str(), s.bold, s.italic, s.code, s.link.is_some())).collect();
        assert_eq!(
            kinds,
            vec![
                ("a ", false, false, false, false),
                ("b", true, false, false, false),
                (" ", false, false, false, false),
                ("c", false, true, false, false),
                (" ", false, false, false, false),
                ("d", false, false, true, false),
                (" ", false, false, false, false),
                ("e", false, false, false, true),
            ]
        );
        assert_eq!(s[7].link.as_deref(), Some("http://x"));
    }

    #[test]
    fn unclosed_markers_stay_literal() {
        assert_eq!(inline("2 * 3 and `half"), vec![plain("2 * 3 and `half")]);
        assert_eq!(inline("see [docs]("), vec![plain("see [docs](")]);
        assert_eq!(inline("**bol"), vec![plain("**bol")]);
    }

    #[test]
    fn many_unmatched_brackets_parse_in_linear_time() {
        let t = std::time::Instant::now();
        let s = inline(&"[x ".repeat(60_000));
        assert_eq!(s.iter().map(|s| s.text.len()).sum::<usize>(), 180_000);
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
        // The first `[` pairs with the first `](...)` after it.
        assert_eq!(inline("[a [b](u)").iter().filter(|s| s.link.is_some()).count(), 1);
    }

    #[test]
    fn lists_and_paragraphs() {
        let b = parse("Intro line\ncontinues\n\n- one\n- two `x`\n\n1. a\n2. b");
        assert_eq!(b.len(), 3);
        assert_eq!(b[0], Block::Para(inline("Intro line continues")));
        assert!(matches!(&b[1], Block::List { start: None, items } if items.len() == 2));
        assert!(matches!(&b[2], Block::List { start: Some(1), items } if items.len() == 2));
    }

    #[test]
    fn code_fence_keeps_text_and_lang_and_survives_streaming() {
        let b = parse("Before\n```html\n<div>\n  hi\n</div>\n```\nAfter");
        assert_eq!(b[1], Block::Code { lang: "html".into(), text: "<div>\n  hi\n</div>".into() });
        assert_eq!(b.len(), 3);
        // Unterminated fence: the rest is code.
        let b = parse("```\nlet x = 1;");
        assert_eq!(b, vec![Block::Code { lang: String::new(), text: "let x = 1;".into() }]);
    }

    #[test]
    fn heading_and_render_smoke() {
        assert!(matches!(parse("## Title")[0], Block::Heading(_)));
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let src = "# H\ntext **b** [l](http://x)\n- a\n1. b\n```rs\nfn main() {}\n```";
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| render(ui, Id::new("md"), src, 13.0));
        out.textures_delta.clear();
    }
}
