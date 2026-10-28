//! Messages of the Chat column: user bubbles, assistant text, reasoning row, tool cards and task
//! groups, plan, approval, checkpoint and run cards, usage line, empty state. Drawing returns the
//! user's choices in [`Acts`]; `chat.rs` applies them (it owns the state and the editor).

use crate::theme::Surface as _;
use super::chat::Approval;
use super::chat_md;
use super::kit;
use crate::chat::{Chat, Message, Role, ToolCall, Usage, store};
use crate::runs::{Run, RunState};
use crate::theme::{self, Icon};
use egui::{Align, Color32, Id, Layout, RichText, Sense, Ui, vec2};
use serde_json::Value;

const ARGS_LIMIT: usize = 600;
const TARGET_CHARS: usize = 40;
const THUMB_MAX: u32 = 320;
const THUMB_H: f32 = 52.0;
/// Widest a result screenshot's thumbnail is shown.
const SHOT_THUMB_W_MAX: u32 = 120;
const LIGHTBOX_MAX: u32 = 2048;
/// The tool whose calls are drawn as a question card instead of a tool card.
const ASK_TOOL: &str = "ask_user";
const REFS_TOOL: &str = "fetch_reference_images";
const ISSUE_TOOL: &str = "report_issue";
/// Lines of an issue draft's body shown on its card.
/// Fields of an issue body shown before "Show all".
const ISSUE_PREVIEW_FIELDS: usize = 3;
const GEN_TOOL: &str = "generate_image";
const REF_THUMB_H: f32 = 96.0;
/// Opacity of a fetched image the agent did not keep.
const DIMMED: f32 = 0.35;
const CHIP_TALL: f32 = 28.0;
const OPTION_IMAGE_H: f32 = 72.0;
pub const ATTACHED_THUMB_H: f32 = 64.0;
const TODO_TOOL: &str = "TodoWrite";
const ROW_H: f32 = 22.0;
const BASE64_MIN: usize = 1000;
const KB: usize = 1024;
/// Prefixes of the context lines the composer puts in front of a prompt.
pub const CONTEXT_TAGS: [&str; 3] = ["[Selection] ", "[Attached] ", "[Research] "];
pub const SUGGESTIONS: [(Icon, &str); 3] = [
    (Icon::Frame, "Design a sign-in screen using this project's variables"),
    (Icon::Text, "Lint this page and fix warnings"),
    (Icon::Eye, "Make a dark mode"),
];
/// The Actions menu's intent lane: label and the prompt sent with a chip of the selection.
pub const ACTION_PROMPTS: [(&str, &str); 4] = [
    ("Optimize", "Tighten layout and spacing to the token scale, fix contrast to AA, keep the content. Prefer existing components."),
    ("Make consistent with DS", "Replace hand-made parts with instances of the listed components, bind every color and space to variables."),
    ("Write states", "Add the empty, loading and error states of the selection as frames next to it. Reuse its components and variables."),
    ("Variations \u{d7}3", "Make 3 variations of the selection side by side: compact, spacious and bold. Duplicate it to the right (offset by its width + 80), name them `<Frame> \u{b7} Variant A/B/C`, keep the content."),
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
    /// File this issue draft (its key and contents).
    pub send_issue: Option<(String, super::chat::IssueDraft)>,
    pub discard_issue: Option<String>,
}

pub struct View<'a> {
    pub running: bool,
    pub thinking: &'a str,
    pub thought_secs: Option<u64>,
    pub runs: &'a [Run],
    pub approvals: &'a [Approval],
    pub error: Option<&'a str>,
    /// The last turn was stopped by the user.
    pub stopped: bool,
    /// The last turn was stopped by the turn budget: the budget, in tokens read.
    pub budget: Option<u64>,
    /// Issue drafts being filed or filed, by `report_issue` call id.
    pub issues: &'a std::collections::HashMap<String, super::chat::IssueState>,
    pub signin: Option<&'a super::chat::chat_agents::SignIn>,
    /// The chat's run while it is being recorded, for the live count on the step group.
    pub live_run: Option<&'a Run>,
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
    let (v, unit) = match n {
        0..1000 => return n.to_string(),
        1000..1_000_000 => (n as f64 / 1e3, "k"),
        _ => (n as f64 / 1e6, "M"),
    };
    let s = format!("{v:.1}");
    format!("{}{unit}", s.strip_suffix(".0").unwrap_or(&s))
}

/// What one turn cost. Claude Code turns saved before `session_cost` hold the session's
/// running cost: the turn is the difference from the previous total (`session_total`).
fn turn_cost(m: &Message, claude_code: bool, session_total: &mut Option<f64>) -> Option<f64> {
    let cost = m.usage.as_ref().and_then(|u| u.cost_usd)?;
    let shown = match (claude_code && m.session_cost.is_none(), *session_total) {
        (true, Some(prev)) if cost >= prev => cost - prev,
        _ => cost,
    };
    *session_total = m.session_cost.or(Some(cost));
    Some(shown)
}

/// What the whole chat has cost so far (None when no turn reported a cost).
pub fn chat_cost(chat: &Chat) -> Option<f64> {
    let claude_code = chat.backend.kind == crate::chat::BackendKind::ClaudeCode;
    let mut session_total = None;
    let costs: Vec<f64> = turns(&chat.messages)
        .into_iter()
        .filter_map(|(start, end)| chat.messages[start + 1..end].iter().rev().find(|m| m.usage.is_some()))
        .filter_map(|m| turn_cost(m, claude_code, &mut session_total))
        .collect();
    (!costs.is_empty()).then(|| costs.iter().sum())
}

/// "12.4k in · 1.1k out · 9.8k cached · $0.06"
pub fn usage_line(u: &Usage) -> String {
    let mut parts = vec![format!("{} in", fmt_k(u.input)), format!("{} out", fmt_k(u.output))];
    if u.cache_read > 0 {
        parts.push(format!("{} cached", fmt_k(u.cache_read)));
    }
    // Cache writes cost more than plain input, often most of the bill: never hide them.
    if u.cache_write > 0 {
        parts.push(format!("{} cache write", fmt_k(u.cache_write)));
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

/// The first image block of a result.
fn image_block(result: &Value) -> Option<&Value> {
    result.as_array()?.iter().find(|b| store::image_data(b).is_some())
}

/// The bytes of a result's first image (inline, or the file it was moved to).
fn image_bytes(result: &Value) -> Option<Vec<u8>> {
    store::image_data(image_block(result)?)?.1.bytes()
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
        .corner_radius(theme::radius_lg())
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
    egui::Frame::new().fill(p.field).corner_radius(theme::radius_md()).inner_margin(egui::Margin::symmetric(10, 8)).show_surface(ui, |ui| {
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
        let bytes = image_bytes(result)?;
        let img = image::load_from_memory(&bytes).ok()?;
        let size = [img.width(), img.height()];
        // Twice the size it is shown at (Retina); the lightbox decodes the full image.
        let px = (2.0 * THUMB_H) as u32;
        let small = img.thumbnail(SHOT_THUMB_W_MAX * 2, px).to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw());
        let tex = ui.ctx().load_texture(format!("chat-shot-{tool_id}"), ci, egui::TextureOptions::LINEAR);
        Some(Thumb { tex, size, bytes: bytes.len() })
    })();
    ui.data_mut(|d| d.insert_temp(key, made.clone()));
    made
}

// ---------------------------------------------------------------------------------- attached images

/// An attached image as a thumbnail texture (fits 3 x `THUMB_MAX` by `THUMB_MAX`), decoded once
/// per `key`; the full image is decoded only for the lightbox.
fn image_tex(ui: &Ui, key: &str, bytes: impl FnOnce() -> Option<Vec<u8>>) -> Option<egui::TextureHandle> {
    let id = Id::new(("chat-image", key));
    if let Some(t) = ui.data(|d| d.get_temp::<Option<egui::TextureHandle>>(id)) {
        return t;
    }
    let made = (|| {
        let img = image::load_from_memory(&bytes()?).ok()?;
        Some(texture(ui.ctx(), &format!("chat-image-{key}"), img, 3 * THUMB_MAX, THUMB_MAX))
    })();
    ui.data_mut(|d| d.insert_temp(id, made.clone()));
    made
}

/// A thumbnail of an attached image, `h` tall; a click opens it in the lightbox. None when the
/// image cannot be read.
pub fn image_thumb(ui: &mut Ui, key: &str, h: f32, bytes: impl Fn() -> Option<Vec<u8>>) -> Option<egui::Response> {
    let tex = image_tex(ui, key, &bytes)?;
    let [w, th] = tex.size();
    let w = (h * w as f32 / th.max(1) as f32).clamp(h * 0.5, h * 3.0);
    let resp = ui
        .add(egui::Image::new(&tex).fit_to_exact_size(vec2(w, h)).corner_radius(theme::radius_md()).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::ZoomIn);
    if resp.clicked() {
        open_lightbox(ui.ctx(), key, bytes());
    }
    Some(resp)
}

/// `img` as a texture that fits `w` by `h` px.
fn texture(ctx: &egui::Context, name: &str, img: image::DynamicImage, w: u32, h: u32) -> egui::TextureHandle {
    let img = if img.width() > w || img.height() > h { img.thumbnail(w, h) } else { img };
    let rgba = img.to_rgba8();
    let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    ctx.load_texture(name, ci, egui::TextureOptions::LINEAR)
}

/// Shows the full image in the lightbox; its texture goes when the lightbox closes.
fn open_lightbox(ctx: &egui::Context, key: &str, bytes: Option<Vec<u8>>) {
    let Some(img) = bytes.and_then(|b| image::load_from_memory(&b).ok()) else { return };
    let tex = texture(ctx, &format!("chat-full-{key}"), img, LIGHTBOX_MAX, LIGHTBOX_MAX);
    ctx.data_mut(|d| d.insert_temp(Id::new("chat-lightbox"), Some(tex)));
}

/// The image the user clicked, full size over the window. Esc or a click closes it.
pub fn lightbox(ctx: &egui::Context) {
    let id = Id::new("chat-lightbox");
    let Some(tex) = ctx.data(|d| d.get_temp::<Option<egui::TextureHandle>>(id)).flatten() else { return };
    let max = ctx.content_rect().size() * 0.9;
    let size = tex.size_vec2();
    let size = size * (max.x / size.x).min(max.y / size.y).min(1.0);
    let modal = egui::Modal::new(id.with("modal")).frame(egui::Frame::NONE).show(ctx, |ui| {
        ui.add(egui::Image::new(&tex).fit_to_exact_size(size).corner_radius(theme::radius_md()).sense(Sense::click())).clicked()
    });
    if modal.inner || modal.should_close() {
        ctx.data_mut(|d| d.remove::<Option<egui::TextureHandle>>(id));
    }
}

// ---------------------------------------------------------------------------------- tool cards

fn tool_card(ui: &mut Ui, t: &ToolCall, state: ToolState, acts: &mut Acts) {
    let p = theme::pal(ui);
    let id = Id::new(("chat-tool", &t.id));
    let open = get_open(ui, id, false);
    let shot = t.result.as_ref().and_then(|r| thumb(ui, &t.id, r));
    let target = tool_target(&t.args);
    card(ui).show_surface(ui, |ui| {
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
                let img = ui.add(egui::Image::new(&s.tex).fit_to_exact_size(vec2(w.clamp(24.0, SHOT_THUMB_W_MAX as f32), h)).corner_radius(theme::radius_md()).sense(Sense::click()));
                if img.on_hover_cursor(egui::CursorIcon::ZoomIn).clicked() {
                    open_lightbox(ui.ctx(), &t.id, t.result.as_ref().and_then(image_bytes));
                }
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

fn tool_group(ui: &mut Ui, calls: &[&ToolCall], v: &View, acts: &mut Acts) {
    let (running, approvals) = (v.running, v.approvals);
    let p = theme::pal(ui);
    let states: Vec<ToolState> = calls.iter().map(|t| tool_state(t, running, approvals)).collect();
    let live = states.iter().any(|s| matches!(s, ToolState::Running | ToolState::Pending));
    let id = Id::new(("chat-group", &calls[0].id));
    let open = get_open(ui, id, live);
    let mut title = group_title(calls, live);
    if let Some(r) = v.live_run.filter(|r| live && r.touched() > 0) {
        let n = r.touched();
        title = format!("{title} \u{b7} {} change{}", super::grouped(n), if n == 1 { "" } else { "s" });
    }
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
                if *s == ToolState::Error || t.result.as_ref().and_then(image_block).is_some() {
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
        let mut images = Vec::new();
        for line in ctx {
            let (tag, rest) = CONTEXT_TAGS.iter().find_map(|t| line.strip_prefix(t).map(|r| (t.trim(), r))).unwrap_or(("", line));
            match tag {
                "[Attached]" => images.push(rest),
                "[Research]" => muted(ui, rest),
                _ => muted(ui, format!("{} {rest}", tag.trim_matches(['[', ']']))),
            }
        }
        if !images.is_empty() {
            ui.with_layout(Layout::right_to_left(Align::Min).with_main_wrap(true), |ui| {
                for path in images {
                    // Older API chats name the image without saving it: keep the name.
                    if image_thumb(ui, path, ATTACHED_THUMB_H, || crate::vfs::read(path).ok()).is_none() {
                        muted(ui, format!("Attached {path}"));
                    }
                }
            });
        }
        if prompt.is_empty() {
            return;
        }
        egui::Frame::new().fill(p.popover).corner_radius(theme::radius_panel()).inner_margin(egui::Margin::symmetric(12, 9)).show_surface(ui, |ui| {
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
    card(ui).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            kit::badge(ui, Icon::Pen, theme::accent(), 30.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(format!("Agent finished, {}", super::panels::count(changes, "change"))).font(theme::semibold(theme::BODY)));
                let secs = run.ended.unwrap_or(run.started).saturating_sub(run.started) / 1000;
                let by = if run.client.is_empty() { String::new() } else { format!("{} \u{b7} ", run.client) };
                muted(ui, format!("{by}Snapshot saved before run {} \u{b7} {secs} s", run.number));
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
    card(ui).show_surface(ui, |ui| {
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
    card(ui).show_surface(ui, |ui| {
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

/// One assistant message: its text, tool cards and reference galleries. Its questions are drawn
/// by [`list`] at the end of the turn. `offered`: the images the turn's questions offer.
fn assistant_segment(ui: &mut Ui, md_id: Id, m: &Message, v: &View, offered: &[String], acts: &mut Acts) {
    if !m.text.trim().is_empty() {
        chat_md::render(ui, md_id, &m.text, theme::BODY);
    }
    // A call waiting for approval is shown by its approval card instead.
    let waiting = |t: &ToolCall| v.approvals.iter().any(|a| a.id == t.id);
    let issues: Vec<&ToolCall> = m.tools.iter().filter(|t| t.name == ISSUE_TOOL && t.error.is_none()).collect();
    let tools: Vec<ToolCall> = m.tools.iter().filter(|t| !is_question(t) && !(t.name == ISSUE_TOOL && t.error.is_none())).cloned().collect();
    let (refs, tools): (Vec<ToolCall>, Vec<ToolCall>) = tools.into_iter().partition(|t| [REFS_TOOL, GEN_TOOL].contains(&t.name.as_str()) && t.result.is_some());
    // Finished calls stay folded in their group while the turn runs and only the live ones show
    // as cards, so the list does not grow and shrink (and the scroll jump) on every call.
    for run in group_tools(&tools, &waiting) {
        let live = |t: &&ToolCall| matches!(tool_state(t, v.running, v.approvals), ToolState::Running | ToolState::Pending);
        let (active, done): (Vec<&ToolCall>, Vec<&ToolCall>) = run.into_iter().partition(live);
        match done.as_slice() {
            [] => {}
            [t] => tool_card(ui, t, tool_state(t, v.running, v.approvals), acts),
            calls => tool_group(ui, calls, v, acts),
        }
        for t in active {
            tool_card(ui, t, tool_state(t, v.running, v.approvals), acts);
        }
    }
    for t in &refs {
        reference_gallery(ui, t, offered, acts);
    }
    for t in issues {
        issue_card(ui, t, v.issues.get(&t.id), acts);
    }
}

/// An issue the agent drafted about PaperCraft: what it says, and Send or Discard.
fn issue_card(ui: &mut Ui, t: &ToolCall, state: Option<&super::chat::IssueState>, acts: &mut Acts) {
    use super::chat::IssueState;
    let p = theme::pal(ui);
    let s = |k: &str| t.args[k].as_str().unwrap_or_default().to_owned();
    let draft = super::chat::IssueDraft { kind: s("kind"), title: s("title"), body: s("body") };
    card(ui).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            theme::paint_icon(ui.painter(), r, Icon::Note, theme::accent());
            ui.label(RichText::new("Issue for PaperCraft").font(theme::semibold(theme::BODY)));
            kit::chip_ink(ui, &draft.kind, None);
        });
        ui.label(RichText::new(&draft.title).font(theme::semibold(theme::BODY)));
        // `**Expected:** …` lines read as a field list: the label above its value (markdown).
        let fields = issue_fields(&draft.body);
        let more_id = Id::new(("issue-more", &t.id));
        let all = ui.data(|d| d.get_temp::<bool>(more_id)).unwrap_or(false);
        let shown = if all { fields.len() } else { fields.len().min(ISSUE_PREVIEW_FIELDS) };
        for (i, (label, value)) in fields.iter().take(shown).enumerate() {
            if !label.is_empty() {
                ui.label(RichText::new(label.to_uppercase()).size(theme::SECONDARY - 1.0).color(p.tertiary));
            }
            ui.scope(|ui| {
                ui.visuals_mut().override_text_color = Some(p.secondary);
                super::chat_md::render(ui, Id::new(("issue-field", &t.id, i)), value, theme::SECONDARY + 1.0);
            });
        }
        if fields.len() > ISSUE_PREVIEW_FIELDS && ui.link(RichText::new(if all { "Show less" } else { "Show all" }).size(theme::SECONDARY)).clicked() {
            ui.data_mut(|d| d.insert_temp(more_id, !all));
        }
        match state {
            Some(IssueState::Sending) => {
                ui.horizontal(|ui| {
                    spinner(ui, 12.0, theme::accent());
                    muted(ui, "Sending\u{2026}");
                });
            }
            Some(IssueState::Filed { number, url, existing }) => {
                ui.hyperlink_to(if *existing { format!("Already reported as #{number}") } else { format!("Filed as #{number}") }, url);
            }
            Some(IssueState::Discarded) => muted(ui, "Discarded"),
            failed => {
                if let Some(IssueState::Failed(e)) = failed {
                    ui.label(RichText::new(e).size(theme::SECONDARY).color(p.danger));
                }
                ui.horizontal(|ui| {
                    if kit::button(ui, kit::ButtonKind::Primary, None, if failed.is_some() { "Try again" } else { "Send issue" }).clicked() {
                        acts.send_issue = Some((t.id.clone(), draft.clone()));
                    }
                    if kit::chip(ui, "Discard", false).clicked() {
                        acts.discard_issue = Some(t.id.clone());
                    }
                });
            }
        }
    });
}

/// An issue body as `(label, value)` pairs: a `**Label:** value` line starts a field, other lines
/// continue it; text before the first label has an empty label.
fn issue_fields(body: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in body.lines() {
        let field = line.trim_start().strip_prefix("**").and_then(|r| r.split_once(":**"));
        match field {
            Some((label, rest)) => out.push((label.trim().to_owned(), rest.trim().to_owned())),
            None => match out.last_mut() {
                Some((_, v)) => {
                    v.push('\n');
                    v.push_str(line);
                }
                None => out.push((String::new(), line.to_owned())),
            },
        }
    }
    out.retain(|(l, v)| !l.is_empty() || !v.trim().is_empty());
    out
}

/// An `ask_user` call that went through: drawn as a question card, not a tool card.
fn is_question(t: &ToolCall) -> bool {
    t.name == ASK_TOOL && t.error.is_none()
}

/// `{path, source}` of each image a `fetch_reference_images` call saved.
fn reference_images(result: &Value) -> Vec<(String, String)> {
    let texts: Vec<&str> = match result {
        Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str()).collect(),
        Value::String(s) => vec![s.as_str()],
        _ => vec![],
    };
    texts
        .iter()
        .filter_map(|t| serde_json::from_str::<Value>(t).ok())
        .flat_map(|v| v["images"].as_array().cloned().unwrap_or_default())
        .filter_map(|i| Some((i["path"].as_str()?.to_owned(), i["source"].as_str().unwrap_or_default().to_owned())))
        .collect()
}

/// Image paths an `ask_user` call offers as options.
fn offered_images(args: &Value) -> Vec<String> {
    let options = args["questions"].as_array().into_iter().flatten().flat_map(|q| q["options"].as_array().cloned().unwrap_or_default());
    options.filter_map(|o| o["image"].as_str().map(str::to_owned)).collect()
}

/// The images the agent fetched from the web, as thumbnails (click: lightbox), with a way to
/// put them on the canvas. When the agent offered some as options, the others are dimmed.
fn reference_gallery(ui: &mut Ui, t: &ToolCall, offered: &[String], acts: &mut Acts) {
    let images = reference_images(t.result.as_ref().unwrap_or(&Value::Null));
    if images.is_empty() {
        return;
    }
    card(ui).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            theme::paint_icon(ui.painter(), r, Icon::Image, theme::accent());
            ui.label(RichText::new(if t.name == GEN_TOOL { "Generated" } else { "References" }).font(theme::semibold(theme::BODY)));
            let kept = images.iter().filter(|(p, _)| offered.contains(p)).count();
            if kept > 0 {
                muted(ui, format!("{kept} of {} kept", images.len()));
            } else {
                muted(ui, super::panels::count(images.len(), "image"));
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (path, source) in &images {
                let dim = !offered.is_empty() && !offered.contains(path);
                // Opacity on the row itself: a child scope would break the wrapping.
                let opacity = ui.opacity();
                if dim {
                    ui.set_opacity(opacity * DIMMED);
                }
                let r = image_thumb(ui, path, REF_THUMB_H, || crate::vfs::read(path).ok());
                ui.set_opacity(opacity);
                if let Some(r) = r {
                    r.on_hover_text(if dim { format!("Not used: {source}") } else { source.clone() });
                }
            }
        });
        if kit::chip(ui, "Put on canvas", false).on_hover_text("Place these images on a References page").clicked() {
            let paths: Vec<&str> = images.iter().map(|(p, _)| p.as_str()).collect();
            acts.send = Some(format!(
                "Put these reference images on a page named References, side by side at their own sizes with their source under each (write_html, <img src> with the path): {}",
                paths.join(", ")
            ));
        }
    });
}

/// The agent's `ask_user` questions as a card: a row of answers per question (a palette option
/// shows its swatches), an inline field for another answer, then Send.
///
/// One single-choice question answers on click (or its number key). Several questions, or
/// several picks, collect in egui memory until Send. Once answered (`answer` is the next user
/// message), the card shows what was picked and dims the rest.
fn question_card(ui: &mut Ui, t: &ToolCall, open: bool, answer: Option<&str>, acts: &mut Acts) {
    let Some(qs) = t.args["questions"].as_array() else { return };
    let id = Id::new(("chat-ask", &t.id));
    let other_id = id.with("other");
    let mut picks: Vec<Vec<usize>> = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| vec![Vec::new(); qs.len()]);
    let mut other: String = ui.data(|d| d.get_temp(other_id)).unwrap_or_default();
    let one_click = qs.len() == 1 && !qs[0]["multiple"].as_bool().unwrap_or(false);
    // What the answer message picked, per question (labels), when the card is closed.
    let answered: Option<Vec<Vec<String>>> = answer.filter(|_| !open).map(|a| qs.iter().map(|q| picked_in(a, q)).collect());
    let p = theme::pal(ui);
    // Number keys pick an option of a single open question while no field has focus.
    let key_pick = (open && qs.len() == 1 && ui.memory(|m| m.focused().is_none()))
        .then(|| ui.input(|i| (1..=9).find(|n| i.key_pressed(digit_key(*n)))))
        .flatten()
        .map(|n| n - 1);
    card(ui).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        for (qi, q) in qs.iter().enumerate() {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                theme::paint_icon(ui.painter(), r, Icon::Question, theme::accent());
                ui.add(egui::Label::new(RichText::new(q["question"].as_str().unwrap_or_default()).font(theme::semibold(theme::BODY))).wrap());
            });
            let multiple = q["multiple"].as_bool().unwrap_or(false);
            let options: Vec<&Value> = q["options"].as_array().into_iter().flatten().collect();
            ui.horizontal_wrapped(|ui| {
                for (oi, o) in options.iter().enumerate() {
                    let colors: Vec<Color32> = o["colors"].as_array().into_iter().flatten().filter_map(|c| crate::model::Color::from_hex(c.as_str()?)).map(super::c32).collect();
                    let label = o["label"].as_str().unwrap_or_default();
                    let on = match &answered {
                        Some(a) => a[qi].iter().any(|l| l == label),
                        None => picks[qi].contains(&oi),
                    };
                    let dim = answered.as_ref().is_some_and(|a| !a[qi].is_empty() && !on);
                    let hint = (open && qs.len() == 1 && oi < 9).then_some(oi + 1);
                    // A reference option shows its image; a double click opens it in the lightbox.
                    let tex = o["image"].as_str().and_then(|path| image_tex(ui, path, || crate::vfs::read(path).ok()));
                    // Dimmed in this row, not in a scope: a child ui in a wrapping row starts its
                    // own line, so the options would never wrap and would widen the whole list.
                    let opacity = ui.opacity();
                    if dim {
                        ui.multiply_opacity(ANSWER_DIM);
                    }
                    let mut r = match &tex {
                        Some(tex) => option_tile(ui, label, tex, on, oi == 0),
                        None => option_chip(ui, label, &colors, on, oi == 0, hint),
                    };
                    ui.set_opacity(opacity);
                    if let Some(path) = o["image"].as_str().filter(|_| tex.is_some() && r.double_clicked()) {
                        open_lightbox(ui.ctx(), path, crate::vfs::read(path).ok());
                    }
                    if let Some(d) = o["detail"].as_str() {
                        r = r.on_hover_text(d);
                    }
                    let picked = (r.clicked() || key_pick == Some(oi)) && open;
                    if !picked {
                        continue;
                    }
                    match (multiple, picks[qi].contains(&oi)) {
                        (true, true) => picks[qi].retain(|&x| x != oi),
                        (true, false) => picks[qi].push(oi),
                        (false, _) => picks[qi] = vec![oi],
                    }
                    if one_click {
                        acts.send = Some(answers_text(qs, &picks));
                    }
                }
            });
            // An answer in the message that is none of the options (typed, or Auto mode's note).
            if let (Some(a), Some(text)) = (&answered, answer)
                && a[qi].is_empty()
                && let Some(own) = own_answer(text, q)
            {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    theme::paint_icon(ui.painter(), r, Icon::ArrowRight, p.tertiary);
                    ui.add(egui::Label::new(RichText::new(own).size(theme::SECONDARY + 1.0).color(p.secondary)).wrap());
                });
            }
        }
        if answered.is_some() {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                let c = r.center();
                let s = egui::Stroke::new(1.6, p.success);
                ui.painter().line_segment([c + vec2(-4.0, 0.5), c + vec2(-1.0, 3.5)], s);
                ui.painter().line_segment([c + vec2(-1.0, 3.5), c + vec2(4.5, -3.0)], s);
                muted(ui, "Answered");
            });
        }
        if !open {
            return;
        }
        // Another answer, typed: Enter sends it (for the single question, or as a note).
        let typed = kit::icon_field(ui, Icon::Pencil, &mut other, if qs.len() == 1 { "Other answer\u{2026}" } else { "Add a note\u{2026}" });
        if typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !other.trim().is_empty() {
            let text = std::mem::take(&mut other);
            acts.send = Some(match qs.len() {
                1 => format!("Answers:\n- {} {}", qs[0]["question"].as_str().unwrap_or_default(), text.trim()),
                _ => format!("{}\n\nNote: {}", answers_text(qs, &picks), text.trim()),
            });
        }
        ui.horizontal(|ui| {
            if !one_click && kit::button(ui, kit::ButtonKind::Primary, Some(Icon::ArrowRight), "Send answers").clicked() {
                acts.send = Some(answers_text(qs, &picks));
            }
            if kit::chip(ui, "Use recommended", false).on_hover_text("The option the agent recommends (marked with a dot) for each question").clicked() {
                acts.send = Some("Go with the option you recommended for each question.".into());
            }
            if one_click {
                muted(ui, "Click an answer or press its number");
            }
        });
    });
    ui.data_mut(|d| {
        d.insert_temp(id, picks);
        d.insert_temp(other_id, other);
    });
}

/// Opacity of the answers not picked, once a question is answered.
const ANSWER_DIM: f32 = 0.45;

fn digit_key(n: usize) -> egui::Key {
    use egui::Key::*;
    [Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9][n - 1]
}

/// The text after `- {question} ` in an answers message, if the message has that line.
fn answer_line<'a>(answer: &'a str, q: &Value) -> Option<&'a str> {
    let question = q["question"].as_str()?;
    answer.lines().find_map(|l| l.strip_prefix("- ")?.strip_prefix(question)).map(str::trim)
}

/// Labels of `q`'s options that the answers message picked.
fn picked_in(answer: &str, q: &Value) -> Vec<String> {
    let Some(line) = answer_line(answer, q) else { return vec![] };
    let parts: Vec<&str> = line.split(", ").map(str::trim).collect();
    q["options"].as_array().into_iter().flatten().filter_map(|o| o["label"].as_str()).filter(|l| parts.contains(l)).map(str::to_owned).collect()
}

/// A typed answer to `q`: its line in the message when it is no option, else the whole
/// message when it is not an answers list at all ("go with your recommendation").
fn own_answer(answer: &str, q: &Value) -> Option<String> {
    match answer_line(answer, q) {
        Some("your call") => None,
        Some(line) => Some(line.to_owned()),
        None if !answer.starts_with("Answers:") => Some(answer.lines().next().unwrap_or_default().to_owned()),
        None => None,
    }
}

/// Auto mode's reply when the chat's last message ends on questions: every question answered
/// with its first (recommended) option.
pub(crate) fn auto_answers(messages: &[crate::chat::Message]) -> Option<String> {
    let last = messages.last().filter(|m| m.role == crate::chat::Role::Assistant)?;
    let t = last.tools.iter().rev().find(|t| is_question(t))?;
    let qs = t.args["questions"].as_array()?;
    let picks: Vec<Vec<usize>> = qs.iter().map(|q| if q["options"].as_array().is_some_and(|o| !o.is_empty()) { vec![0] } else { vec![] }).collect();
    Some(format!("{}\n\n(Auto mode: your recommended answers were taken. Go on.)", answers_text(qs, &picks)))
}

/// "Answers:" then one line per question; one left unpicked is the agent's call.
fn answers_text(qs: &[Value], picks: &[Vec<usize>]) -> String {
    let lines = qs.iter().zip(picks).map(|(q, pick)| {
        let labels: Vec<&str> = pick.iter().filter_map(|&i| q["options"][i]["label"].as_str()).collect();
        let answer = if labels.is_empty() { "your call".to_owned() } else { labels.join(", ") };
        format!("- {} {answer}", q["question"].as_str().unwrap_or_default())
    });
    std::iter::once("Answers:".to_owned()).chain(lines).collect::<Vec<_>>().join("\n")
}

/// Fill and outline of an answer: picked in the accent, else an outlined field (so it reads as
/// a button on the card).
fn option_frame(ui: &Ui, rect: egui::Rect, radius: u8, on: bool, hovered: bool) {
    let p = theme::pal(ui);
    let fill = if on { theme::accent().gamma_multiply(0.18) } else { theme::blend(p.field, p.hover.gamma_multiply(f32::from(u8::from(hovered)))) };
    let stroke = if on { egui::Stroke::new(1.0, theme::accent()) } else { egui::Stroke::new(1.0, p.secondary.gamma_multiply(OUTLINE_ALPHA)) };
    ui.painter().rect_filled(rect, egui::CornerRadius::same(radius), fill);
    ui.painter().rect_stroke(rect, egui::CornerRadius::same(radius), stroke, egui::StrokeKind::Inside);
}

/// Strength of an idle answer's outline: enough to read as a button on the card.
const OUTLINE_ALPHA: f32 = 0.45;

/// A dot before the recommended answer (the agent lists it first).
const RECOMMENDED_DOT: f32 = 6.0;

/// An answer chip: the recommended dot, swatches, then the label; the picked one in the accent.
fn option_chip(ui: &mut Ui, label: &str, colors: &[Color32], on: bool, recommended: bool, key: Option<usize>) -> egui::Response {
    const SWATCH: f32 = 12.0;
    const PAD: f32 = 10.0;
    const KEY_W: f32 = 18.0;
    let p = theme::pal(ui);
    let ink = if on { theme::accent() } else { p.text };
    let keyw = if key.is_some() { KEY_W + 6.0 } else { 0.0 };
    let dot = if recommended { RECOMMENDED_DOT + 6.0 } else { 0.0 };
    let sw = if colors.is_empty() { 0.0 } else { colors.len() as f32 * (SWATCH - 3.0) + 3.0 + 6.0 };
    let chrome = 2.0 * PAD + keyw + dot + sw;
    // A label longer than the row wraps (the chip grows taller): one chip wider than the column
    // would widen the whole message list.
    let wrap = (ui.max_rect().width() - chrome).max(PAD);
    let galley = ui.painter().layout(label.to_owned(), theme::semibold(theme::SECONDARY + 1.0), ink, wrap);
    let tall = (galley.size().y + PAD).max(CHIP_TALL);
    let (rect, resp) = ui.allocate_exact_size(vec2(chrome + galley.size().x, tall), Sense::click());
    option_frame(ui, rect, if tall > CHIP_TALL { theme::radius_lg() } else { 255 }, on, resp.hovered());
    // The number key that picks it, in a small cap.
    if let Some(k) = key {
        let cap = egui::Rect::from_center_size(egui::pos2(rect.left() + PAD - 2.0 + KEY_W / 2.0, rect.center().y), vec2(KEY_W, KEY_W));
        ui.painter().rect_filled(cap, theme::radius_md() / 2, p.hover);
        ui.painter().text(cap.center(), egui::Align2::CENTER_CENTER, k.to_string(), egui::FontId::proportional(theme::SECONDARY - 1.0), p.secondary);
    }
    let rect = egui::Rect::from_min_max(rect.min + vec2(keyw, 0.0), rect.max);
    if recommended {
        ui.painter().circle_filled(egui::pos2(rect.left() + PAD + RECOMMENDED_DOT / 2.0, rect.center().y), RECOMMENDED_DOT / 2.0, theme::accent());
    }
    let mut x = rect.left() + PAD + dot + SWATCH / 2.0;
    for c in colors {
        ui.painter().circle(egui::pos2(x, rect.center().y), SWATCH / 2.0, *c, egui::Stroke::new(1.0, p.separator));
        x += SWATCH - 3.0;
    }
    let tx = rect.left() + PAD + dot + sw;
    ui.painter().galley(egui::pos2(tx, rect.center().y - galley.size().y / 2.0), galley, ink);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if recommended { resp.on_hover_text("Recommended") } else { resp }
}

/// An answer that is a picture (a reference): the image, its label under it in up to two lines.
fn option_tile(ui: &mut Ui, label: &str, tex: &egui::TextureHandle, on: bool, recommended: bool) -> egui::Response {
    const PAD: f32 = 6.0;
    let p = theme::pal(ui);
    let [w, h] = tex.size();
    let img_w = (OPTION_IMAGE_H * w as f32 / h.max(1) as f32).clamp(OPTION_IMAGE_H * 1.2, OPTION_IMAGE_H * 2.0);
    let ink = if on { theme::accent() } else { p.text };
    let text = if recommended { format!("\u{2022} {label}") } else { label.to_owned() };
    let mut job = egui::text::LayoutJob::simple(text, theme::semibold(theme::SECONDARY), ink, img_w);
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    job.halign = Align::Center;
    let galley = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(vec2(img_w + 2.0 * PAD, OPTION_IMAGE_H + 3.0 * PAD + galley.size().y), Sense::click());
    option_frame(ui, rect, theme::radius_md(), on, resp.hovered());
    let img = egui::Rect::from_min_size(rect.min + vec2(PAD, PAD), vec2(img_w, OPTION_IMAGE_H));
    egui::Image::new(tex).corner_radius(theme::radius_md() - 2).paint_at(ui, img);
    ui.painter().galley(egui::pos2(rect.center().x, img.bottom() + PAD), galley, ink);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{label}\nDouble-click to view it larger"));
    if recommended { resp.on_hover_text("Recommended") } else { resp }
}

/// The scrolling message list of `chat`.
pub fn list(ui: &mut Ui, chat: &Chat, v: &View, acts: &mut Acts) {
    // Thumbnails are cached per image while a chat is open; another chat frees them all.
    let shown = Id::new("chat-thumbs-of");
    if ui.data(|d| d.get_temp::<String>(shown)).as_deref() != Some(chat.id.as_str()) {
        ui.data_mut(|d| {
            d.remove_by_type::<Option<Thumb>>();
            d.remove_by_type::<Option<egui::TextureHandle>>();
            d.insert_temp(shown, chat.id.clone());
        });
    }
    let jump_id = Id::new("chat-jump");
    let jump = ui.data_mut(|d| d.remove_temp::<bool>(jump_id)).unwrap_or(false);
    let out = egui::ScrollArea::vertical().id_salt(("chat-list", &chat.id)).auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        let msgs = &chat.messages;
        let all = turns(msgs);
        let claude_code = chat.backend.kind == crate::chat::BackendKind::ClaudeCode;
        let mut session_total: Option<f64> = None;
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
            let questions: Vec<&ToolCall> = reply.iter().flat_map(|m| &m.tools).filter(|t| is_question(t)).collect();
            // Images the agent offered as options are the ones it kept; the rest are dimmed.
            let offered: Vec<String> = questions.iter().flat_map(|t| offered_images(&t.args)).collect();
            for (k, m) in reply.iter().enumerate() {
                assistant_segment(ui, Id::new(("chat-md", &chat.id, start + 1 + k)), m, v, &offered, acts);
            }
            // Questions close the turn, after the summary the agent writes once the call returns.
            for t in questions {
                let answer = all.get(n + 1).map(|&(next, _)| msgs[next].text.as_str());
                question_card(ui, t, is_last && !v.running, answer, acts);
            }
            let text = reply.iter().map(|m| m.text.as_str()).collect::<Vec<_>>().join("\n\n");
            if !text.is_empty() && !(is_last && v.running) {
                actions_row(ui, Id::new(("chat-actions", &chat.id, start)), &text, is_last, acts);
            }
            if let Some(m) = reply.iter().rev().find(|m| m.usage.is_some()) {
                let mut u = m.usage.clone().unwrap_or_default();
                u.cost_usd = turn_cost(m, claude_code, &mut session_total);
                ui.label(RichText::new(usage_line(&u)).size(theme::SECONDARY).color(theme::pal(ui).tertiary));
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
            error_row(ui, e, v.signin, acts);
        } else if let Some(b) = v.budget.filter(|_| v.stopped && !v.running) {
            muted(ui, format!("Stopped at the turn budget ({} tokens read). Send a message to continue, or change the budget in Settings \u{2192} Chat.", fmt_k(b)));
        } else if v.stopped && !v.running {
            muted(ui, "Stopped");
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

/// A card with a title, a hint and Retry: the sign-in state of an agent.
pub fn notice_card(ui: &mut Ui, title: &str, hint: &str, acts: &mut Acts) {
    let p = theme::pal(ui);
    card(ui).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).font(theme::semibold(theme::BODY)).color(p.warning));
        ui.add(egui::Label::new(RichText::new(hint).size(theme::SECONDARY).color(p.secondary)).wrap());
        if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Restore), "Retry").clicked() {
            acts.retry_last = true;
        }
    });
}

/// The error card with Retry; an agent that is signed out gets its sign-in card instead.
pub fn error_row(ui: &mut Ui, e: &str, signin: Option<&super::chat::chat_agents::SignIn>, acts: &mut Acts) {
    if let Some(s) = signin {
        return super::chat::chat_agents::signin_card(ui, s, acts);
    }
    let p = theme::pal(ui);
    card(ui).show_surface(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(RichText::new(e).color(p.danger)).wrap());
        if kit::button(ui, kit::ButtonKind::Secondary, Some(Icon::Restore), "Retry").clicked() {
            acts.retry_last = true;
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
            ui.painter().rect_filled(r.rect, theme::radius_lg(), p.hover);
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

    /// How wide `f` draws in a 400 px column.
    fn drawn_width(f: impl Fn(&mut Ui)) -> f32 {
        let ctx = egui::Context::default();
        super::super::setup_style(&ctx);
        let mut w = 0.0;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            w = ui.allocate_ui(vec2(400.0, 2000.0), |ui| f(ui)).response.rect.width();
        })
        .textures_delta
        .clear();
        w
    }

    #[test]
    fn question_options_wrap_inside_the_column() {
        let options: Vec<Value> = ["Sending in progress state", "Make the queue row a component", "Make the sheet a component", "Darken the overlay"]
            .iter()
            .map(|l| json!({"label": l}))
            .collect();
        let q = call("q", "AskUserQuestion", json!({"questions": [{"question": "Next step?", "options": options}]}));
        assert!(drawn_width(|ui| question_card(ui, &q, true, None, &mut Acts::default())) <= 400.0, "open");
        let answer = "Answers:\n- Next step? Make the sheet a component";
        assert!(drawn_width(|ui| question_card(ui, &q, false, Some(answer), &mut Acts::default())) <= 400.0, "answered, the rest dimmed");
    }

    #[test]
    fn an_option_longer_than_the_column_wraps() {
        let long = "Motorista: permissions (location, camera, notifications), vehicle, 3-card tour, and a long tail";
        let q = call("q", "ask_user", json!({"questions": [{"question": "What goes into each first-access wizard?", "options": [{"label": long}, {"label": "No"}]}]}));
        assert!(drawn_width(|ui| question_card(ui, &q, true, None, &mut Acts::default())) <= 400.0);
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
        assert_eq!(image_block(&img).and_then(store::image_data).map(|i| i.0), Some("image/png"));
        assert_eq!(image_block(&json!([{"type": "text", "text": "x"}])), None);
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

    #[test]
    fn auto_mode_takes_the_first_option_of_the_last_questions() {
        use crate::chat::{Message, Role, ToolCall};
        let ask = ToolCall { id: "a".into(), name: ASK_TOOL.into(), args: serde_json::json!({"questions": [
            {"question": "Density?", "options": [{"label": "Compact"}, {"label": "Touch"}]},
            {"question": "Next?", "options": [{"label": "Mobile layout"}]}]}), ..Default::default() };
        let mut msgs = vec![Message { role: Role::Assistant, tools: vec![ask], ..Default::default() }];
        let a = auto_answers(&msgs).unwrap();
        assert!(a.contains("Density? Compact") && a.contains("Next? Mobile layout"), "{a}");
        msgs.push(Message { role: Role::User, text: "hi".into(), ..Default::default() });
        assert_eq!(auto_answers(&msgs), None, "already answered");
        assert_eq!(auto_answers(&[Message { role: Role::Assistant, text: "done".into(), ..Default::default() }]), None);
    }

    #[test]
    fn answers_message_marks_picked_options() {
        let q = serde_json::json!({"question": "Theme?", "options": [{"label": "Dark"}, {"label": "Light"}, {"label": "Both"}]});
        let msg = "Answers:\n- Theme? Dark, Both";
        assert_eq!(picked_in(msg, &q), ["Dark", "Both"]);
        assert_eq!(own_answer(msg, &q), Some("Dark, Both".into()));
        assert_eq!(picked_in("Answers:\n- Theme? something warmer", &q), Vec::<String>::new());
        assert_eq!(own_answer("Answers:\n- Theme? your call", &q), None);
        assert_eq!(own_answer("Go with the option you recommended for each question.", &q).as_deref(), Some("Go with the option you recommended for each question."));
    }

    #[test]
    fn issue_body_reads_as_labelled_fields() {
        let f = issue_fields("**Expected:** a\n**Actual:** b\nmore\n**Repro:** `x`");
        assert_eq!(f, [("Expected".into(), "a".into()), ("Actual".into(), "b\nmore".into()), ("Repro".into(), "`x`".into())]);
        assert_eq!(issue_fields("plain"), [(String::new(), "plain".into())]);
    }

    #[test]
    fn answers_name_each_pick_and_leave_the_rest_to_the_agent() {
        let qs = [
            json!({"question": "Frame size?", "options": [{"label": "Desktop 1440x900"}, {"label": "Phone 390x844"}]}),
            json!({"question": "Palette?", "options": [{"label": "Ember", "colors": ["#ff5a1f"]}, {"label": "Tide"}], "multiple": true}),
            json!({"question": "Density?", "options": [{"label": "Compact"}, {"label": "Comfortable"}]}),
        ];
        let text = answers_text(&qs, &[vec![1], vec![0, 1], vec![]]);
        assert_eq!(text, "Answers:\n- Frame size? Phone 390x844\n- Palette? Ember, Tide\n- Density? your call");
    }

    #[test]
    fn fetched_references_list_their_saved_paths() {
        let mcp = json!([{"type": "image", "data": "x"}, {"type": "text", "text": "{\"images\":[{\"path\":\"/p/a.png\",\"source\":\"https://x/a\"}],\"skipped\":[]}"}]);
        assert_eq!(reference_images(&mcp), [("/p/a.png".to_owned(), "https://x/a".to_owned())]);
        assert!(reference_images(&json!("not json")).is_empty());
    }

    #[test]
    fn a_chat_total_adds_each_turns_share() {
        use crate::chat::{Backend, BackendKind, Usage};
        let turn = |cost: f64, session: Option<f64>| Message { role: Role::Assistant, usage: Some(Usage { cost_usd: Some(cost), ..Default::default() }), session_cost: session, ..Default::default() };
        let user = || Message { role: Role::User, text: "go".into(), ..Default::default() };
        // Two legacy turns holding the running total (1.00, then 1.50), then one with its share.
        let chat = Chat { backend: Backend { kind: BackendKind::ClaudeCode, ..Default::default() }, messages: vec![user(), turn(1.0, None), user(), turn(1.5, None), user(), turn(0.25, Some(1.75))], ..Default::default() };
        assert!((chat_cost(&chat).unwrap() - 1.75).abs() < 1e-9);
        assert_eq!(chat_cost(&Chat::default()), None);
    }
}



