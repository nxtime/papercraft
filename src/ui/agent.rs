//! The browser build's "Agent" menu: a link a remote AI agent can use to drive this tab through
//! the relay (see `mcp::relay`). The shell (`web/relay.rs`) owns the connection and mirrors its
//! state here; this module only draws and records what the user clicked.

use super::theme;
use egui::{Color32, RichText};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkState {
    #[default]
    Offline,
    Connecting,
    Connected,
}

impl LinkState {
    pub fn label(self) -> &'static str {
        match self {
            LinkState::Offline => "Offline",
            LinkState::Connecting => "Connecting",
            LinkState::Connected => "Connected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAction {
    /// Replace the token (the old link stops working) and reconnect.
    Regenerate,
}

#[derive(Debug, Default)]
pub struct AgentUi {
    /// The full agent URL, `https://<host>/s/<token>/mcp`.
    pub url: String,
    pub state: LinkState,
    pub open: bool,
    pub actions: Vec<AgentAction>,
}

const CONNECTED_DOT: Color32 = Color32::from_rgb(0x34, 0xc7, 0x59);
const CONNECTING_DOT: Color32 = Color32::from_rgb(0xff, 0xb3, 0x40);
const DOT_RADIUS: f32 = 4.0;

/// Shell command that registers the link with Claude Code.
pub fn claude_command(url: &str) -> String {
    format!("claude mcp add --transport http papercraft {url}")
}

/// Generic MCP client configuration for the link.
pub fn json_config(url: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": { "papercraft": { "type": "http", "url": url } }
    }))
    .expect("config serializes")
}

impl AgentUi {
    /// The top-level "Agent" menu.
    pub fn menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Agent", |ui| {
            if ui.button("Connect…").clicked() {
                self.open = true;
                ui.close();
            }
        });
    }

    /// A status dot for the toolbar; nothing while offline.
    pub fn dot(&self, ui: &mut egui::Ui) {
        let color = match self.state {
            LinkState::Offline => return,
            LinkState::Connecting => CONNECTING_DOT,
            LinkState::Connected => CONNECTED_DOT,
        };
        let (rect, resp) = ui.allocate_exact_size(
            egui::Vec2::splat(DOT_RADIUS * 2.0 + 4.0),
            egui::Sense::hover(),
        );
        ui.painter().circle_filled(rect.center(), DOT_RADIUS, color);
        resp.on_hover_text(format!("Agent link: {}", self.state.label()));
    }

    /// The "Connect…" window.
    pub fn window(&mut self, ctx: &egui::Context) {
        let mut open = self.open;
        egui::Window::new("Connect an AI agent")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .show(ctx, |ui| self.contents(ui));
        self.open = open && self.open;
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui);
        ui.label(
            RichText::new(
                "Give this link to an MCP-capable agent. Anyone who has it can edit this tab, \
                 so treat it like a password.",
            )
            .size(theme::SECONDARY)
            .color(p.secondary),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Status:");
            let color = match self.state {
                LinkState::Connected => CONNECTED_DOT,
                LinkState::Connecting => CONNECTING_DOT,
                LinkState::Offline => p.secondary,
            };
            ui.label(RichText::new(self.state.label()).color(color));
        });
        ui.add_space(6.0);
        self.copy_row(ui, "Agent URL", &self.url.clone(), false);
        self.copy_row(ui, "Claude Code", &claude_command(&self.url), false);
        self.copy_row(ui, "MCP config (JSON)", &json_config(&self.url), true);
        ui.add_space(6.0);
        if ui.button("Regenerate link").clicked() {
            self.actions.push(AgentAction::Regenerate);
        }
    }

    fn copy_row(&self, ui: &mut egui::Ui, label: &str, text: &str, multiline: bool) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(label);
            if ui.button("Copy").clicked() {
                ui.ctx().copy_text(text.to_string());
            }
        });
        let mut shown = text;
        let edit = if multiline {
            egui::TextEdit::multiline(&mut shown).desired_rows(5)
        } else {
            egui::TextEdit::singleline(&mut shown)
        };
        ui.add(
            edit.desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_carry_the_url() {
        let url = "https://h.example/s/abc/mcp";
        assert_eq!(
            claude_command(url),
            "claude mcp add --transport http papercraft https://h.example/s/abc/mcp"
        );
        let v: serde_json::Value = serde_json::from_str(&json_config(url)).unwrap();
        assert_eq!(v["mcpServers"]["papercraft"]["url"], url);
        assert_eq!(v["mcpServers"]["papercraft"]["type"], "http");
    }
}
