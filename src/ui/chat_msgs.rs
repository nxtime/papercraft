//! Messages of the Chat column: user bubbles, assistant text, reasoning row, tool cards and task
//! groups, plan, approval, checkpoint and run cards, usage line, empty state. Drawing returns the
//! user's choices in [`Acts`]; `chat.rs` applies them (it owns the state and the editor).

use super::chat::Approval;
use super::chat_md;
use super::kit;
use crate::chat::{Chat, Message, Role, ToolCall, Usage};
use crate::runs::{Run, RunState};
use crate::theme::{self, Icon};
use egui::{Align, Color32, Id, Layout, RichText, Sense, Ui, vec2};
use serde_json::Value;

const ARGS_LIMIT: usize = 600;
const TARGET_CHARS: usize = 40;
const THUMB_MAX: u32 = 320;
const THUMB_H: f32 = 52.0;
const TODO_TOOL: &str = "TodoWrite";
const ROW_H: f32 = 22.0;
const BASE64_MIN: usize = 1000;
const KB: usize = 1024;
/// Prefixes of the context lines the composer puts in front of a prompt.
pub const CONTEXT_TAGS: [&str; 2] = ["[Selection] ", "[Attached] "];
pub const SUGGESTIONS: [(Icon, &str); 3] = [
    (Icon::Frame, "Design a sign-in screen using this project's variables"),
    (Icon::Text, "Lint this page and fix warnings"),
    (Icon::Eye, "Make a dark mode"),
];
const AFTER_RUN: [(&str, &str); 3] = [
    ("Lint this", "Lint this page and fix warnings"),
    ("Make a dark mode", "Make a dark mode"),
    ("Review changes", "Review the changes you just made and point out anything off"),
];

/// What the user chose while the list was drawn.
#[derive(Default)]
pub struct Acts {
    /// Send the last user message again.
    pub retry_last: bool,
    /// Send this follow-up (a failed tool call is retried by asking the agent).
    pub send: Option<String>,
    pub restore: Option<String>,
    pub review: Option<String>,
    pub revert: Option<String>,
    pub accept: Option<String>,
    pub approve: Option<(String, bool)>,
}

pub struct View<'a> {
    pub running: bool,
    pub thinking: &'a str,
    pub thought_secs: Option<u64>,
    pub runs: &'a [Run],
    pub approvals: &'a [Approval],
    pub error: Option<&'a str>,
}

// ---------------------------------------------------------------------------------- pure helpers

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    Active,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    pub text: String,
    pub status: TodoStatus,
}

/// Items of a `TodoWrite` call (`todos: [{content, status, activeForm}]`).
pub fn parse_todos(args: &Value) -> Vec<Todo> {
    let Some(items) = args["todos"].as_array() else { return Vec::new() };
    items
        .iter()
        .filter_map(|t| {
            let status = match t["status"].as_str().unwrap_or("pending") {
                "completed" => TodoStatus::Done,
                "in_progress" => TodoStatus::Active,
                _ => TodoStatus::Pending,
            };
            let text = t["content"].as_str().or_else(|| t["activeForm"].as_str())?;
            Some(Todo { text: text.into(), status })
        })
        .collect()
}

/// The newest `TodoWrite` list of the chat (each call replaces the whole list).
pub fn latest_plan(chat: &Chat) -> Vec<Todo> {
    chat.messages
        .iter()
        .flat_map(|m| &m.tools)
        .rev()
        .find(|t| t.name == TODO_TOOL)
        .map(|t| parse_todos(&t.args))
        .unwrap_or_default()
}

/// `12400` -> "12.4k", `12000` -> "12k", `900` -> "900".
pub fn fmt_k(n: u64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    let s = format!("{:.1}", n as f64 / 1000.0);
    format!("{}k", s.strip_suffix(".0").unwrap_or(&s))
}

/// "12.4k in · 1.1k out · 9.8k cached · $0.06"
pub fn usage_line(u: &Usage) -> String {
    let mut parts = vec![format!("{} in", fmt_k(u.input)), format!("{} out", fmt_k(u.output))];
    if u.cache_read > 0 {
        parts.push(format!("{} cached", fmt_k(u.cache_read)));
    }
    if let Some(c) = u.cost_usd {
        parts.push(format!("${c:.2}"));
    }
    parts.join(" \u{b7} ")
}

/// Verb for a card header: the design verb, else the bare tool name.
pub fn tool_verb(name: &str) -> String {
    match crate::ui::agent_verb(name) {
        "Working" => name.to_owned(),
        v => v.to_owned(),
    }
}

/// What the call acts on: a name argument, else the node id (or a path / command for CLI tools).
pub fn tool_target(args: &Value) -> Option<String> {
    let s = ["name", "nodeName", "pageName", "file_path", "path", "command", "pattern", "nodeId", "targetNodeId"]
        .iter()
        .find_map(|k| args[*k].as_str().filter(|s| !s.is_empty()))
        .or_else(|| args["nodeIds"][0].as_str())?;
    Some(if s.chars().count() > TARGET_CHARS {
        format!("{}\u{2026}", s.chars().take(TARGET_CHARS).collect::<String>())
    } else {
        s.to_owned()
    })
}

/// The node a call points at, for the canvas to follow.
pub fn follow_node(args: &Value) -> Option<String> {
    ["nodeId", "targetNodeId", "parentId"]
        .iter()
        .find_map(|k| args[*k].as_str().filter(|s| s.contains(':')))
        .map(str::to_owned)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Pending,
    Running,
    Done,
    Error,
    /// No result and the turn is over (stopped).
    Stopped,
}

pub fn tool_state(t: &ToolCall, turn_running: bool, approvals: &[Approval]) -> ToolState {
    match (&t.error, &t.result) {
        (Some(_), _) => ToolState::Error,
        (None, Some(_)) => ToolState::Done,
        _ if approvals.iter().any(|a| a.id == t.id) => ToolState::Pending,
        _ if turn_running => ToolState::Running,
        _ => ToolState::Stopped,
    }
}

impl ToolState {
    fn label(self) -> &'static str {
        match self {
            ToolState::Pending => "Waiting",
            ToolState::Running => "Running",
            ToolState::Done => "Done",
            ToolState::Error => "Failed",
            ToolState::Stopped => "Stopped",
        }
    }
}

/// Splits a message's calls into display items: a run of two or more consecutive calls is one
/// group, a lone call stays a card. `TodoWrite` is shown as the plan, not as a card.
pub fn group_tools<'a>(tools: &'a [ToolCall], skip: &dyn Fn(&ToolCall) -> bool) -> Vec<Vec<&'a ToolCall>> {
    // A message holds consecutive calls only (text starts a new message), so one group per message.
    let shown: Vec<&ToolCall> = tools.iter().filter(|t| t.name != TODO_TOOL && !skip(t)).collect();
    if shown.len() < 2 { shown.into_iter().map(|t| vec![t]).collect() } else { vec![shown] }
}

pub fn group_title(calls: &[&ToolCall], running: bool) -> String {
    let n = calls.len();
    let steps = format!("{n} step{}", if n == 1 { "" } else { "s" });
    if running {
        return format!("Working \u{b7} {steps}");
    }
    let writes: Vec<&&ToolCall> = calls.iter().filter(|t| ["create_artboard", "write_html"].contains(&t.name.as_str())).collect();
    // Nothing was drawn when every write failed: say so instead of "Built".
    let verb = if writes.iter().all(|t| t.error.is_some()) { "Tried" } else { "Built" };
    let head = match writes.first() {
        Some(t) => match tool_target(&t.args).filter(|_| t.name == "create_artboard") {
            Some(target) => format!("{verb} {target}"),
            None => format!("{verb} the layout"),
        },
        None => tool_verb(&calls[0].name),
    };
    format!("{head} \u{b7} {steps}")
}

/// Indices `(first, one past last)` of each user message's turn: the user message at `first` and
/// the assistant messages after it.
pub fn turns(msgs: &[Message]) -> Vec<(usize, usize)> {
    let starts: Vec<usize> = (0..msgs.len()).filter(|&i| msgs[i].role == Role::User).collect();
    starts.iter().enumerate().map(|(k, &s)| (s, starts.get(k + 1).copied().unwrap_or(msgs.len()))).collect()
}

/// The runs a turn made: those its assistant messages name (`Message.run_id`, set when the
/// run finished during the turn), oldest first. Never matched by time.
pub fn runs_of<'a>(reply: &[Message], runs: &'a [Run]) -> Vec<&'a Run> {
    let mut v: Vec<&Run> = reply.iter().filter_map(|m| m.run_id.as_deref()).filter_map(|id| runs.iter().find(|r| r.id == id)).collect();
    v.sort_by_key(|r| r.started);
    v
}

/// `"[Selection] Card (a1b2c3:14)\n\nprompt"` -> context lines and the prompt.
pub fn split_context(text: &str) -> (Vec<&str>, &str) {
    let mut ctx = Vec::new();
    let mut rest = text;
    while let Some(line) = rest.lines().next().filter(|l| CONTEXT_TAGS.iter().any(|t| l.starts_with(t))) {
        ctx.push(line);
        rest = rest[line.len()..].trim_start_matches('\n');
    }
    (ctx, rest)
}

fn is_base64ish(s: &str) -> bool {
    s.len() >= BASE64_MIN && !s.contains(char::is_whitespace)
}

/// The value with base64 payloads replaced by a size note, for display.
pub fn elide(v: &Value) -> Value {
    match v {
        Value::String(s) if is_base64ish(s) => Value::String(format!("<base64, {} KB>", s.len() * 3 / 4 / KB)),
        Value::Array(a) => Value::Array(a.iter().map(elide).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), elide(v))).collect()),
        other => other.clone(),
    }
}

/// Pretty JSON, cut at `limit` characters; the flag says it was cut.
pub fn pretty(v: &Value, limit: usize) -> (String, bool) {
    let s = serde_json::to_string_pretty(&elide(v)).unwrap_or_default();
    if s.chars().count() <= limit {
        return (s, false);
    }
    (format!("{}\u{2026}", s.chars().take(limit).collect::<String>()), true)
}

/// Text of a tool result: its text blocks (parsed as JSON when that works), else the value.
fn result_text(v: &Value) -> Value {
    let Some(blocks) = v.as_array() else { return v.clone() };
    let texts: Vec<&str> = blocks.iter().filter_map(|b| b["text"].as_str()).collect();
    if texts.is_empty() {
        return elide(v);
    }
    let joined = texts.join("\n");
    serde_json::from_str(&joined).unwrap_or(Value::String(joined))
}

/// The first image block of a result: (mime, base64 data).
pub fn image_of(result: &Value) -> Option<(String, String)> {
    result.as_array()?.iter().find(|b| b["type"] == "image").and_then(|b| {
        let data = b["source"]["data"].as_str().or_else(|| b["data"].as_str())?;
        let mime = b["source"]["media_type"].as_str().or_else(|| b["mimeType"].as_str()).unwrap_or("image/png");
        Some((mime.to_owned(), data.to_owned()))
    })
}

/// Title and detail line of an approval card.
pub fn approval_text(a: &Approval) -> (String, String) {
    let n = a.args["nodeIds"].as_array().map_or(0, Vec::len);
    let title = match a.name.as_str() {
        "delete_pages" => "Delete pages?".to_owned(),
        "delete_nodes" => format!("Delete {n} layers?"),
        "disable_library" => "Disable library?".to_owned(),
        "import_design_system" => "Import design system?".to_owned(),
        other => format!("Run {other}?"),
    };
    let target = tool_target(&a.args).map(|t| format!(" \u{b7} {t}")).unwrap_or_default();
    (title, format!("{}{target}. Reverting the run restores it.", a.name))
}

// ---------------------------------------------------------------------------------- small widgets

fn get_open(ui: &Ui, id: Id, default: bool) -> bool {
    ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(default)
}

fn flip(ui: &Ui, id: Id, now: bool) {
    ui.data_mut(|d| d.insert_temp(id, !now));
}

/// A row with `left` content, `right` content (added right to left) and a click target over all.
fn header(ui: &mut Ui, id: Id, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) -> bool {
    let w = ui.available_width();
    let r = ui.allocate_ui_with_layout(vec2(w, ROW_H), Layout::right_to_left(Align::Center), |ui| {
        right(ui);
        ui.with_layout(Layout::left_to_right(Align::Center), left);
    });
    ui.interact(r.response.rect, id, Sense::click()).clicked()
}

fn chevron(ui: &mut Ui, open: bool) {
    let p = theme::pal(ui);
    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    theme::paint_icon(ui.painter(), r, if open { Icon::ChevronDown } else { Icon::ChevronRight }, p.secondary);
}

/// A rotating three-quarter arc (egui's own spinner reads as a dot at this size).
fn spinner(ui: &mut Ui, size: f32, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let start = t * std::f32::consts::TAU * 1.1;
    let radius = size / 2.0 - 1.5;
    let pts: Vec<egui::Pos2> = (0..=16)
        .map(|i| {
            let a = start + i as f32 / 16.0 * std::f32::consts::PI * 1.5;
            r.center() + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    ui.painter().add(egui::Shape::line(pts, egui::Stroke::new(1.6, color)));
    ui.request_repaint();
}

fn mark(ui: &mut Ui, state: ToolState) {
    let p = theme::pal(ui);
    if state == ToolState::Running {
        spinner(ui, 14.0, theme::accent());
        return;
    }
    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    let (c, ink) = (r.center(), r.width() / 2.0 - 1.0);
    let st = |col: Color32| egui::Stroke::new(1.6, col);
    let pt = |x: f32, y: f32| c + vec2(x, y) * ink / 5.0;
    match state {
        ToolState::Done => {
            ui.painter().line_segment([pt(-3.0, 0.5), pt(-1.0, 2.8)], st(p.success));
            ui.painter().line_segment([pt(-1.0, 2.8), pt(3.5, -2.5)], st(p.success));
        }
        ToolState::Error => {
            ui.painter().circle_stroke(c, ink, st(p.danger));
        }
        ToolState::Pending => {
            ui.painter().circle_stroke(c, ink, st(p.warning));
        }
        _ => {
            ui.painter().circle_stroke(c, ink, st(p.tertiary));
        }
    }
    if state == ToolState::Error {
        ui.painter().line_segment([pt(0.0, -2.2), pt(0.0, 0.6)], st(p.danger));
        ui.painter().circle_filled(pt(0.0, 2.4), 0.9, p.danger);
    }
}

fn card(ui: &Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(theme::pal(ui).popover)
        .corner_radius(theme::RADIUS_LG)
        .inner_margin(egui::Margin::symmetric(12, 8))
}

fn muted(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).size(theme::SECONDARY).color(theme::pal(ui).secondary));
}

/// `v` as shown: JSON pretty-printed and cut at `ARGS_LIMIT` characters, strings cut the same.
/// The flag says it was cut.
fn preview(v: &Value, all: bool) -> (String, bool) {
    let limit = if all { usize::MAX } else { ARGS_LIMIT };
    match v {
        Value::String(s) if s.chars().count() > limit => (format!("{}\u{2026}", s.chars().take(limit).collect::<String>()), true),
        Value::String(s) => (s.clone(), false),
        other => pretty(other, limit),
    }
}

/// A monospace block with a label and a "Show all" toggle when `text` was cut. `value` is only
/// called when the preview is not cached; `settled` says the call is over, so the preview cannot
/// change and is kept per tool id instead of being parsed and printed every frame.
fn code_section(ui: &mut Ui, id: Id, label: &str, value: impl FnOnce() -> Value, error: bool, settled: bool) {
    let p = theme::pal(ui);
    let all_id = id.with("all");
    let all = get_open(ui, all_id, false);
    let key = id.with(("preview", all));
    let cached = settled.then(|| ui.data(|d| d.get_temp::<(String, bool)>(key))).flatten();
    let (text, cut) = cached.unwrap_or_else(|| {
        let made = preview(&value(), all);
        if settled {
            ui.data_mut(|d| d.insert_temp(key, made.clone()));
        }
        made
    });
    egui::Frame::new().fill(p.field).corner_radius(theme::RADIUS_MD).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let ink = if error { p.danger } else { p.secondary };
        ui.label(RichText::new(label).size(theme::SECONDARY).color(ink));
        ui.add(egui::Label::new(RichText::new(text).monospace().size(theme::SECONDARY).color(p.text)).wrap());
        if cut || all {
            let t = if all { "Show less" } else { "Show all" };
            if ui.add(egui::Label::new(RichText::new(t).size(theme::SECONDARY).color(theme::accent())).sense(Sense::click())).clicked() {
                flip(ui, all_id, all);
            }
        }
    });
}

#[derive(Clone)]
struct Thumb {
    tex: egui::TextureHandle,
    size: [u32; 2],
    bytes: usize,
}

/// The decoded screenshot of a tool result, as a texture cached by tool id.
fn thumb(ui: &Ui, tool_id: &str, result: &Value) -> Option<Thumb> {
    let key = Id::new(("chat-shot", tool_id));
    if let Some(t) = ui.data(|d| d.get_temp::<Option<Thumb>>(key)) {
        return t;
    }
    let made = (|| {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let (_, data) = image_of(result)?;
        let bytes = STANDARD.decode(data.as_bytes()).ok()?;
        let img = image::load_from_memory(&bytes).ok()?;
        let size = [img.width(), img.height()];
        let small = img.thumbnail(THUMB_MAX, THUMB_MAX).to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw());
        let tex = ui.ctx().load_texture(format!("chat-shot-{tool_id}"), ci, egui::TextureOptions::LINEAR);
        Some(Thumb { tex, size, bytes: bytes.len() })
    })();
    ui.data_mut(|d| d.insert_temp(key, made.clone()));
    made
}

// ---------------------------------------------------------------------------------- tool cards

fn tool_card(ui: &mut Ui, t: &ToolCall, state: ToolState, acts: &mut Acts) {
    let p = theme::pal(ui);
    let id = Id::new(("chat-tool", &t.id));
    let open = get_open(ui, id, false);
    let shot = t.result.as_ref().and_then(|r| thumb(ui, &t.id, r));
    let target = tool_target(&t.args);
    card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        let ink = match state {
            ToolState::Error => p.danger,
            ToolState::Running => theme::accent(),
            ToolState::Pending => p.warning,
            _ => p.secondary,
        };
        let clicked = header(
            ui,
            id.with("hdr"),
            |ui| {
                mark(ui, state);
                ui.add(egui::Label::new(RichText::new(tool_verb(&t.name)).font(theme::semibold(theme::BODY))).truncate());
                if let Some(target) = &target {
                    ui.add(egui::Label::new(RichText::new(target).color(p.secondary)).truncate());
                }
            },
            |ui| {
                chevron(ui, open);
                ui.label(RichText::new(state.label()).size(theme::SECONDARY).color(ink));
            },
        );
        if clicked {
            flip(ui, id, open);
        }
        if let Some(s) = &shot {
            ui.horizontal(|ui| {
                let h = THUMB_H;
                let w = h * s.size[0] as f32 / s.size[1].max(1) as f32;
                ui.add(egui::Image::new(&s.tex).fit_to_exact_size(vec2(w.clamp(24.0, 120.0), h)).corner_radius(theme::RADIUS_MD));
                ui.vertical(|ui| {
                    muted(ui, format!("{} x {}", s.size[0], s.size[1]));
                    muted(ui, format!("Result image, {} KB", (s.bytes / KB).max(1)));
                });
            });
        }
        if let Some(e) = &t.error {
            code_section(ui, id.with("err"), "error", || Value::String(e.clone()), true, true);
            if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Restore), "Retry").clicked() {
                acts.send = Some(format!("The {} call failed: {e}\nPlease fix the cause and retry it.", t.name));
            }
        }
        if open {
            let settled = t.result.is_some() || t.error.is_some();
            if !t.args.is_null() {
                code_section(ui, id.with("args"), "args", || t.args.clone(), false, settled);
            }
            if let Some(r) = &t.result {
                code_section(ui, id.with("res"), "result", || result_text(r), false, true);
            }
        }
    });
}

/// One line of a task group.
fn step_row(ui: &mut Ui, t: &ToolCall, state: ToolState) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        mark(ui, state);
        let label = match tool_target(&t.args) {
            Some(target) => format!("{} {target}", tool_verb(&t.name)),
            None => tool_verb(&t.name),
        };
        ui.add(egui::Label::new(RichText::new(label).size(theme::SECONDARY + 1.0).color(p.secondary)).truncate());
        if let Some(ms) = t.ms.filter(|_| state == ToolState::Done) {
            ui.label(RichText::new(format!("{:.1}s", ms as f32 / 1000.0)).size(theme::SECONDARY).color(p.tertiary));
        }
    });
}

fn tool_group(ui: &mut Ui, calls: &[&ToolCall], running: bool, approvals: &[Approval], acts: &mut Acts) {
    let p = theme::pal(ui);
    let states: Vec<ToolState> = calls.iter().map(|t| tool_state(t, running, approvals)).collect();
    let live = states.iter().any(|s| matches!(s, ToolState::Running | ToolState::Pending));
    let id = Id::new(("chat-group", &calls[0].id));
    let open = get_open(ui, id, live);
    let title = group_title(calls, live);
    if header(
        ui,
        id.with("hdr"),
        |ui| {
            chevron(ui, open);
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            theme::paint_icon(ui.painter(), r, Icon::Group, p.secondary);
            ui.label(RichText::new(title).font(theme::semibold(theme::SECONDARY + 1.0)));
        },
        |_| {},
    ) {
        flip(ui, id, open);
    }
    if !open {
        return;
    }
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for (t, s) in calls.iter().zip(&states) {
                if *s == ToolState::Error || t.result.as_ref().and_then(image_of).is_some() {
                    tool_card(ui, t, *s, acts);
                } else {
                    step_row(ui, t, *s);
                }
            }
        });
    });
}

fn reasoning_row(ui: &mut Ui, text: &str, running: bool, secs: Option<u64>) {
    let p = theme::pal(ui);
    let id = Id::new("chat-reasoning");
    let open = get_open(ui, id, false);
    let label = match (running, secs) {
        (false, Some(s)) => format!("Thought for {s}s"),
        _ => "Thinking\u{2026}".to_owned(),
    };
    if header(
        ui,
        id.with("hdr"),
        |ui| {
            chevron(ui, open);
            if running {
                spinner(ui, 12.0, p.secondary);
            }
            ui.label(RichText::new(label).font(theme::semibold(theme::SECONDARY + 1.0)).color(p.secondary));
        },
        |_| {},
    ) {
        flip(ui, id, open);
    }
    if open {
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.add(egui::Label::new(RichText::new(text).size(theme::SECONDARY + 1.0).color(p.secondary)).wrap());
        });
    }
}

// ---------------------------------------------------------------------------------- cards

fn user_bubble(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    let (ctx, prompt) = split_context(text);
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        for line in ctx {
            let (tag, rest) = CONTEXT_TAGS.iter().find_map(|t| line.strip_prefix(t).map(|r| (t.trim(), r))).unwrap_or(("", line));
            muted(ui, format!("{} {rest}", tag.trim_matches(['[', ']'])));
        }
        if prompt.is_empty() {
            return;
        }
        egui::Frame::new().fill(p.popover).corner_radius(theme::RADIUS_PANEL).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
            ui.set_max_width(ui.available_width() * 0.88);
            ui.add(egui::Label::new(prompt).wrap());
        });
    });
}

fn checkpoint(ui: &mut Ui, run: &Run, acts: &mut Acts) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
        theme::paint_icon(ui.painter(), r, Icon::Restore, p.secondary);
        ui.label(RichText::new(format!("Checkpoint \u{b7} before run {}", run.number)).size(theme::SECONDARY).color(p.secondary));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if run.state == RunState::Reverted {
                kit::chip_ink(ui, "Reverted", None);
            } else if kit::chip(ui, "Restore to here", false).on_hover_text("Revert this run and every run after it").clicked() {
                acts.restore = Some(run.id.clone());
            }
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(line.x_range(), line.center().y, egui::Stroke::new(1.0, p.separator));
        });
    });
}

fn run_card(ui: &mut Ui, run: &Run, acts: &mut Acts) {
    let p = theme::pal(ui);
    let changes = run.added + run.changed + run.removed;
    card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            kit::badge(ui, Icon::Pen, theme::accent(), 30.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(format!("Agent finished, {}", super::panels::count(changes, "change"))).font(theme::semibold(theme::BODY)));
                let secs = run.ended.unwrap_or(run.started).saturating_sub(run.started) / 1000;
                muted(ui, format!("Snapshot saved before run {} \u{b7} {secs} s", run.number));
            });
        });
        ui.add_space(4.0);
        match run.state {
            RunState::Review => {
                ui.horizontal(|ui| {
                    if kit::button(ui, kit::ButtonKind::Primary, Some(Icon::Eye), "Review").clicked() {
                        acts.review = Some(run.id.clone());
                    }
                    if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Restore), "Revert").clicked() {
                        acts.revert = Some(run.id.clone());
                    }
                    if kit::button(ui, kit::ButtonKind::Secondary, None, "Accept").clicked() {
                        acts.accept = Some(run.id.clone());
                    }
                });
            }
            RunState::Accepted => {
                kit::chip_ink(ui, "Accepted", Some(p.success));
            }
            RunState::Reverted => {
                kit::chip_ink(ui, "Reverted", None);
            }
            RunState::Running => {}
        }
    });
}

pub fn approval_card(ui: &mut Ui, a: &Approval, acts: &mut Acts) {
    let p = theme::pal(ui);
    let (title, detail) = approval_text(a);
    card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(title).font(theme::semibold(theme::BODY)));
                ui.add(egui::Label::new(RichText::new(detail).size(theme::SECONDARY).color(p.secondary)).wrap());
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if kit::button(ui, kit::ButtonKind::Secondary, None, "Deny").clicked() {
                acts.approve = Some((a.id.clone(), false));
            }
            if kit::button(ui, kit::ButtonKind::Danger, Some(Icon::Trash), "Approve").clicked() {
                acts.approve = Some((a.id.clone(), true));
            }
            muted(ui, "Waiting for you");
        });
    });
}

/// The pinned plan above the composer.
pub fn plan_card(ui: &mut Ui, todos: &[Todo]) {
    let p = theme::pal(ui);
    let id = Id::new("chat-plan");
    let open = get_open(ui, id, true);
    let done = todos.iter().filter(|t| t.status == TodoStatus::Done).count();
    card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 4.0;
        if header(
            ui,
            id.with("hdr"),
            |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                theme::paint_icon(ui.painter(), r, Icon::Group, theme::accent());
                ui.label(RichText::new("Plan").font(theme::semibold(theme::SECONDARY + 1.0)));
                muted(ui, format!("{done} of {} done", todos.len()));
            },
            |ui| chevron(ui, open),
        ) {
            flip(ui, id, open);
        }
        if !open {
            return;
        }
        for t in todos {
            ui.horizontal(|ui| {
                let state = match t.status {
                    TodoStatus::Done => ToolState::Done,
                    TodoStatus::Active => ToolState::Running,
                    TodoStatus::Pending => ToolState::Stopped,
                };
                mark(ui, state);
                let text = RichText::new(&t.text).size(theme::SECONDARY + 1.0);
                ui.add(egui::Label::new(if t.status == TodoStatus::Done { text.strikethrough().color(p.tertiary) } else { text.color(p.text) }).truncate());
            });
        }
    });
}

fn actions_row(ui: &mut Ui, id: Id, text: &str, can_retry: bool, acts: &mut Acts) {
    let show = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let r = ui
        .scope(|ui| {
            ui.set_height(ROW_H);
            if !show {
                ui.set_invisible();
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if kit::lottie_button(ui, "copy", "Copy", ROW_H).clicked() {
                    ui.ctx().copy_text(text.to_owned());
                }
                if can_retry && kit::lottie_button(ui, "refresh-cw", "Retry", ROW_H).clicked() {
                    acts.retry_last = true;
                }
            });
        })
        .response;
    let hover = ui.rect_contains_pointer(r.rect.expand2(vec2(400.0, 60.0)));
    ui.data_mut(|d| d.insert_temp(id, hover));
}

// ---------------------------------------------------------------------------------- list

fn assistant_segment(ui: &mut Ui, md_id: Id, m: &Message, v: &View, acts: &mut Acts) {
    if !m.text.trim().is_empty() {
        chat_md::render(ui, md_id, &m.text, theme::BODY);
    }
    // While a call is live every call shows as a card; a settled turn folds into one group.
    let live = m.tools.iter().any(|t| matches!(tool_state(t, v.running, v.approvals), ToolState::Running | ToolState::Pending));
    // A call waiting for approval is shown by its approval card instead.
    let waiting = |t: &ToolCall| v.approvals.iter().any(|a| a.id == t.id);
    for run in group_tools(&m.tools, &waiting) {
        match run.as_slice() {
            calls if live || calls.len() == 1 => {
                for t in calls {
                    tool_card(ui, t, tool_state(t, v.running, v.approvals), acts);
                }
            }
            calls => tool_group(ui, calls, v.running, v.approvals, acts),
        }
    }
}

/// The scrolling message list of `chat`.
pub fn list(ui: &mut Ui, chat: &Chat, v: &View, acts: &mut Acts) {
    let jump_id = Id::new("chat-jump");
    let jump = ui.data_mut(|d| d.remove_temp::<bool>(jump_id)).unwrap_or(false);
    let out = egui::ScrollArea::vertical().id_salt(("chat-list", &chat.id)).auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        let msgs = &chat.messages;
        let all = turns(msgs);
        let last_turn = all.len().saturating_sub(1);
        for (n, &(start, end)) in all.iter().enumerate() {
            let runs = runs_of(&msgs[start + 1..end], v.runs);
            if let Some(r) = runs.first() {
                checkpoint(ui, r, acts);
            }
            user_bubble(ui, &msgs[start].text);
            let is_last = n == last_turn;
            if is_last && (!v.thinking.is_empty() || v.thought_secs.is_some()) {
                reasoning_row(ui, v.thinking, v.running, v.thought_secs);
            }
            let reply = &msgs[start + 1..end];
            for (k, m) in reply.iter().enumerate() {
                assistant_segment(ui, Id::new(("chat-md", &chat.id, start + 1 + k)), m, v, acts);
            }
            let text = reply.iter().map(|m| m.text.as_str()).collect::<Vec<_>>().join("\n\n");
            if !text.is_empty() && !(is_last && v.running) {
                actions_row(ui, Id::new(("chat-actions", &chat.id, start)), &text, is_last, acts);
            }
            if let Some(u) = reply.iter().rev().find_map(|m| m.usage.as_ref()) {
                ui.label(RichText::new(usage_line(u)).size(theme::SECONDARY).color(theme::pal(ui).tertiary));
            }
            for r in runs.iter().filter(|r| r.state != RunState::Running) {
                run_card(ui, r, acts);
            }
            if is_last && !v.running && !runs.is_empty() && v.error.is_none() {
                ui.horizontal_wrapped(|ui| {
                    for (label, prompt) in AFTER_RUN {
                        if kit::chip(ui, label, false).clicked() {
                            acts.send = Some(prompt.into());
                        }
                    }
                });
            }
        }
        for a in v.approvals {
            approval_card(ui, a, acts);
        }
        if v.running && v.approvals.is_empty() && chat.messages.last().is_none_or(|m| m.tools.iter().all(|t| t.result.is_some() || t.error.is_some())) {
            ui.horizontal(|ui| {
                spinner(ui, 12.0, theme::accent());
                muted(ui, "Working\u{2026}");
            });
        }
        if let Some(e) = v.error {
            let p = theme::pal(ui);
            card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(e).color(p.danger)).wrap());
                if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Restore), "Retry").clicked() {
                    acts.retry_last = true;
                }
            });
        }
        ui.add_space(4.0);
        if jump {
            ui.scroll_to_cursor(Some(Align::BOTTOM));
        }
    });
    let at_bottom = out.state.offset.y + out.inner_rect.height() >= out.content_size.y - 4.0;
    if at_bottom {
        return;
    }
    egui::Area::new(Id::new("chat-jump-pill"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(out.inner_rect.center_bottom() - vec2(0.0, 10.0))
        .show(ui.ctx(), |ui| {
            if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::ArrowDown), "Jump to latest").clicked() {
                ui.data_mut(|d| d.insert_temp(jump_id, true));
                ui.request_repaint();
            }
        });
}

/// Suggestions, shown for a chat without messages.
pub fn empty(ui: &mut Ui, acts: &mut Acts) {
    let p = theme::pal(ui);
    ui.add_space(ui.available_height() * 0.12);
    kit::badge(ui, Icon::Star, theme::accent(), 40.0);
    ui.add_space(6.0);
    ui.label(RichText::new("Design by conversation").font(theme::semibold(18.0)));
    ui.add(egui::Label::new(
        RichText::new("Describe a screen, a fix or a review. The agent draws on the canvas and you stay in control with Review and Revert.")
            .color(p.secondary),
    ));
    ui.add_space(14.0);
    ui.label(RichText::new("TRY").font(theme::semibold(theme::SECONDARY)).color(p.tertiary));
    for (icon, text) in SUGGESTIONS {
        let w = ui.available_width();
        let r = card(ui)
            .show(ui, |ui| {
                ui.set_width(w - 24.0);
                ui.horizontal(|ui| {
                    let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    theme::paint_icon(ui.painter(), ir, icon, theme::accent());
                    ui.add(egui::Label::new(text).wrap());
                });
            })
            .response;
        let click = ui.interact(r.rect, Id::new(("chat-suggest", text)), Sense::click());
        if click.hovered() {
            ui.painter().rect_filled(r.rect, theme::RADIUS_LG, p.hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if click.clicked() {
            acts.send = Some(text.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, name: &str, args: Value) -> ToolCall {
        ToolCall { id: id.into(), name: name.into(), args, ..Default::default() }
    }

    #[test]
    fn usage_line_matches_the_mockup() {
        let u = Usage { input: 12_400, output: 1_100, cache_read: 9_800, cache_write: 0, cost_usd: Some(0.06) };
        assert_eq!(usage_line(&u), "12.4k in \u{b7} 1.1k out \u{b7} 9.8k cached \u{b7} $0.06");
        let u = Usage { input: 900, output: 12_000, ..Default::default() };
        assert_eq!(usage_line(&u), "900 in \u{b7} 12k out");
    }

    #[test]
    fn todo_write_parses_and_the_latest_wins() {
        let a = json!({"todos": [
            {"content": "Create artboard", "status": "completed", "activeForm": "Creating"},
            {"content": "Style title", "status": "in_progress"},
            {"content": "Lint", "status": "pending"},
        ]});
        let t = parse_todos(&a);
        assert_eq!(t.iter().map(|t| t.status).collect::<Vec<_>>(), [TodoStatus::Done, TodoStatus::Active, TodoStatus::Pending]);
        assert!(parse_todos(&json!({})).is_empty());
        let mut chat = Chat::default();
        chat.messages.push(Message {
            role: Role::Assistant,
            tools: vec![call("1", "TodoWrite", a), call("2", "TodoWrite", json!({"todos": [{"content": "Only", "status": "completed"}]}))],
            ..Default::default()
        });
        assert_eq!(latest_plan(&chat).len(), 1);
    }

    #[test]
    fn consecutive_calls_group_and_todo_write_is_hidden() {
        let one = [call("1", "create_artboard", json!({"name": "Card"}))];
        assert_eq!(group_tools(&one, &|_| false).len(), 1);
        let many = [
            call("1", "create_artboard", json!({"name": "Card"})),
            call("2", "TodoWrite", json!({})),
            call("3", "update_styles", json!({})),
            call("4", "lint_design", json!({})),
        ];
        let g = group_tools(&many, &|_| false);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].len(), 3);
        assert_eq!(group_title(&g[0], false), "Built Card \u{b7} 3 steps");
        assert_eq!(group_title(&g[0], true), "Working \u{b7} 3 steps");
        assert_eq!(group_tools(&many, &|t| t.id == "1" || t.id == "4").len(), 1, "skipped calls leave a lone card");
    }

    #[test]
    fn targets_and_follow_nodes() {
        assert_eq!(tool_target(&json!({"name": "Hero", "nodeId": "a:1"})).as_deref(), Some("Hero"));
        assert_eq!(tool_target(&json!({"nodeId": "a:1"})).as_deref(), Some("a:1"));
        assert_eq!(tool_target(&json!({"nodeIds": ["a:2"]})).as_deref(), Some("a:2"));
        assert_eq!(tool_target(&json!({})), None);
        assert_eq!(follow_node(&json!({"parentId": "a:9"})).as_deref(), Some("a:9"));
        assert_eq!(follow_node(&json!({"nodeId": "nope"})), None);
        assert_eq!(tool_verb("write_html"), "Drawing layout");
        assert_eq!(tool_verb("Bash"), "Bash");
    }

    #[test]
    fn context_lines_split_from_the_prompt() {
        let (ctx, rest) = split_context("[Selection] Card (a:1)\n[Attached] /tmp/x.png\n\nMake it blue");
        assert_eq!(ctx.len(), 2);
        assert_eq!(rest, "Make it blue");
        assert_eq!(split_context("plain"), (vec![], "plain"));
    }

    #[test]
    fn turns_own_only_the_runs_they_name() {
        let m = |role, at| Message { role, at, ..Default::default() };
        let msgs = [m(Role::User, 10), m(Role::Assistant, 11), m(Role::Assistant, 12), m(Role::User, 100), m(Role::Assistant, 101)];
        assert_eq!(turns(&msgs), vec![(0, 3), (3, 5)]);
        let run = |id: &str, started| Run { id: id.into(), started, ..Default::default() };
        // "b" started inside the first turn's time span but belongs to another chat or agent.
        let runs = [run("b", 11), run("a", 20)];
        let mut named = m(Role::Assistant, 12);
        named.run_id = Some("a".into());
        let reply = [m(Role::Assistant, 11), named];
        assert_eq!(runs_of(&reply, &runs).iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["a"]);
        assert!(runs_of(&msgs[1..3], &runs).is_empty(), "no run_id, no card");
    }

    #[test]
    fn a_turn_whose_writes_all_failed_is_not_titled_built() {
        let mut bad = call("1", "create_artboard", json!({"name": "Card"}));
        bad.error = Some("no project".into());
        let mut ok = call("3", "create_artboard", json!({"name": "Card"}));
        ok.result = Some(json!([]));
        let lint = call("2", "lint_design", json!({}));
        assert_eq!(group_title(&[&bad, &lint], false), "Tried Card \u{b7} 2 steps");
        assert_eq!(group_title(&[&bad, &ok, &lint], false), "Built Card \u{b7} 3 steps");
    }

    #[test]
    fn previews_are_cut_and_only_cached_once_settled() {
        let big = json!({"html": "x ".repeat(600)});
        let (short, cut) = preview(&big, false);
        assert!(cut && short.chars().count() <= ARGS_LIMIT + 1);
        let (full, cut) = preview(&big, true);
        assert!(!cut && full.chars().count() > ARGS_LIMIT);
        assert_eq!(preview(&Value::String("a".repeat(ARGS_LIMIT + 5)), false).1, true);
        let ctx = egui::Context::default();
        let calls = std::cell::Cell::new(0);
        for settled in [true, true, false, false] {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                code_section(ui, Id::new(("t", settled)), "args", || {
                    calls.set(calls.get() + 1);
                    big.clone()
                }, false, settled);
            });
            out.textures_delta.clear();
        }
        // Settled: built once. Unsettled: built each frame.
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn long_json_is_cut_and_base64_is_elided() {
        let big = json!({"html": "x ".repeat(600)});
        let (s, cut) = pretty(&big, ARGS_LIMIT);
        assert!(cut && s.chars().count() <= ARGS_LIMIT + 1);
        assert!(!pretty(&json!({"a": 1}), ARGS_LIMIT).1);
        let img = json!([{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "A".repeat(4000)}}]);
        assert!(elide(&img).to_string().contains("<base64, 2 KB>"));
        assert_eq!(image_of(&img).map(|i| i.0).as_deref(), Some("image/png"));
        assert_eq!(image_of(&json!([{"type": "text", "text": "x"}])), None);
    }

    #[test]
    fn tool_states() {
        let a = [Approval { id: "p".into(), name: "delete_pages".into(), args: json!({}) }];
        let mut t = call("p", "delete_pages", json!({}));
        assert_eq!(tool_state(&t, true, &a), ToolState::Pending);
        assert_eq!(tool_state(&t, true, &[]), ToolState::Running);
        assert_eq!(tool_state(&t, false, &[]), ToolState::Stopped);
        t.error = Some("x".into());
        assert_eq!(tool_state(&t, true, &[]), ToolState::Error);
        t.error = None;
        t.result = Some(json!([]));
        assert_eq!(tool_state(&t, true, &[]), ToolState::Done);
    }
}
