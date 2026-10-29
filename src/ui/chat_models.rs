//! The model list of an ACP agent in the agent panel: grouped by provider, one line per model with
//! its price and context size, filtered by the panel's search.

use super::chat_agents::{Act, Fold, PanelIn, section};
use crate::chat::Backend;
use crate::chat::models::{AgentModel, FREE_AUTO};
use crate::theme;
use crate::ui::kit;
use egui::{Color32, FontId, Sense, Ui, pos2, vec2};

const ROW_H: f32 = 28.0;
const PAD: f32 = 8.0;
const CHECK_W: f32 = 18.0;
/// Agents with free models of their own: their list starts on the free ones.
const FREE_AGENTS: [&str; 2] = ["opencode", "pi"];

/// The model new chats on agent `id` start on: the one picked last, else "Free (auto)" for agents
/// with free models. ponytail: picking "Default" saves no model, so those agents come back to
/// "Free (auto)"; store the choice if someone wants their agent's own default kept.
pub fn default_model(prefs: &crate::ui::settings::ChatPrefs, id: &str) -> Option<String> {
    prefs.agents.get(id).and_then(|a| a.model.clone()).or_else(|| FREE_AGENTS.contains(&id).then(|| FREE_AUTO.to_owned()))
}

/// "Default" (the agent's own choice), "Free (auto)" for agents with free models, the chat's model
/// when the agent no longer lists it, then the catalog (free models only until `all` is set). A
/// click picks the model for this chat and for new chats on the agent.
pub fn list(ui: &mut Ui, i: &PanelIn, query: &str, all: &mut bool) -> Option<Act> {
    let b = i.backend;
    let catalog = b.agent.as_ref().and_then(|a| i.prefs.agents.get(a)).map_or(&[][..], |a| a.catalog.as_slice());
    let free_agent = b.agent.as_deref().is_some_and(|a| FREE_AGENTS.contains(&a)) && catalog.iter().any(AgentModel::is_free);
    let free_only = free_agent && !*all;
    let q = query.trim().to_lowercase();
    let hit = |m: &&AgentModel| (!free_only || m.is_free()) && (q.is_empty() || [&m.name, &m.id, &m.provider].iter().any(|s| s.to_lowercase().contains(&q)));
    let shown: Vec<&AgentModel> = catalog.iter().filter(hit).collect();
    let current = b.model.as_deref();
    let pick = |model: Option<&str>| Act::SetBackend(Backend { model: model.map(Into::into), effort: if model == current { b.effort.clone() } else { None }, ..b.clone() });
    let mut out = None;
    if let Some(model) = catalog.iter().find(|m| Some(m.id.as_str()) == current && !m.variants.is_empty()) {
        section(ui, "Effort", None, Fold::None);
        let options: Vec<(Option<&str>, &str)> = b.effort.is_none().then_some((None, "Agent choice")).into_iter()
            .chain(model.variants.iter().map(|v| (Some(v.as_str()), v.as_str())))
            .collect();
        let current = b.effort.as_deref().filter(|e| model.variants.iter().any(|v| v == e));
        if let Some(effort) = kit::select(ui, current, &options, ui.available_width()) {
            out = Some(Act::SetBackend(Backend { effort: effort.map(str::to_owned), ..b.clone() }));
        }
    }
    section(ui, "Model", Some(shown.len()), Fold::None);
    if free_agent && kit::chip(ui, "Free only", free_only).clicked() {
        *all = !*all;
    }
    if q.is_empty() && row(ui, "Default", "the agent's choice", current.is_none(), None).clicked() {
        out = Some(pick(None));
    }
    if free_agent && q.is_empty() && row(ui, "Free (auto)", "moves on when a model is busy", current == Some(FREE_AUTO), Some("Starts on the most capable free model and switches to the next one when it is rate limited or gone".into())).clicked() {
        out = Some(pick(Some(FREE_AUTO)));
    }
    if let Some(m) = current.filter(|m| q.is_empty() && *m != FREE_AUTO && !catalog.iter().any(|c| c.id == *m)) {
        row(ui, m, "", true, None);
    }
    let mut provider = None;
    for m in shown {
        if provider != Some(m.provider.as_str()) {
            provider = Some(m.provider.as_str());
            provider_label(ui, &m.provider);
        }
        let detail = [m.price(), m.context.map(|c| format!("{} ctx", crate::ui::chat_msgs::fmt_k(c))).unwrap_or_default()];
        let detail = detail.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("  \u{b7}  ");
        let image = match m.image {
            Some(true) => "\nReads images",
            Some(false) => "\nCannot read images",
            None => "",
        };
        if row(ui, &m.name, &detail, current == Some(m.id.as_str()), Some(format!("{}{image}", m.id))).clicked() {
            out = Some(pick(Some(&m.id)));
        }
    }
    if catalog.is_empty() {
        let p = theme::pal(ui);
        ui.label(egui::RichText::new("This agent lists its models when a session starts").size(theme::SECONDARY).color(p.tertiary));
    }
    out.filter(|_| !i.running)
}

fn provider_label(ui: &mut Ui, provider: &str) {
    let p = theme::pal(ui);
    ui.add_space(4.0);
    ui.add(egui::Label::new(egui::RichText::new(provider).size(theme::SECONDARY).color(p.tertiary)).truncate());
}

/// One line: a check when selected, the name (cut with "…" when long), the detail on the right.
fn row(ui: &mut Ui, name: &str, detail: &str, selected: bool, tip: Option<String>) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    if selected {
        theme::fill_selected(ui, rect, theme::radius_md(), p.field);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, theme::radius_md(), p.hover);
    }
    let font = FontId::proportional(theme::SECONDARY + 1.0);
    let cy = rect.center().y;
    if selected {
        let c = theme::accent();
        let pt = |x: f32, y: f32| pos2(rect.left() + PAD + x, cy - 8.0 + y);
        ui.painter().add(egui::Shape::line(vec![pt(1.0, 8.5), pt(4.5, 12.0), pt(11.0, 4.5)], egui::Stroke::new(1.6, c)));
    }
    let right = ui.painter().layout_no_wrap(detail.to_owned(), FontId::proportional(theme::SECONDARY), p.secondary);
    let right_w = right.size().x;
    ui.painter().galley(pos2(rect.right() - PAD - right_w, cy - right.size().y / 2.0), right, p.secondary);
    let x = rect.left() + PAD + CHECK_W;
    let room = (rect.right() - PAD - right_w - PAD - x).max(0.0);
    let mut job = egui::text::LayoutJob::single_section(name.to_owned(), egui::TextFormat::simple(font, p.text));
    job.wrap = egui::text::TextWrapping::truncate_at_width(room);
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, Color32::PLACEHOLDER);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    match tip {
        Some(t) => resp.on_hover_text(t),
        None => resp,
    }
}
