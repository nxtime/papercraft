//! The install dialog for an agent that is not found (OpenCode, Pi): the methods that work on this
//! machine, the exact command before anything runs, its output while it runs, and the command and
//! docs link to fall back on.

use crate::chat::install::{self, Host, Kind, Line, Method, Missing};
use crate::theme;
use crate::ui::kit::{self, ButtonKind};
use egui::RichText;
use std::sync::mpsc::Receiver;

const DIALOG_W: f32 = 480.0;
/// Output lines kept on screen; the rest scroll away.
const OUTPUT_MAX: usize = 400;
const OUTPUT_H: f32 = 160.0;

pub struct Install {
    pub agent: String,
    name: String,
    docs: &'static str,
    methods: Vec<Method>,
    missing: Vec<Missing>,
    pick: usize,
    run: Option<Receiver<Line>>,
    output: Vec<String>,
    done: Option<Result<(), String>>,
}

/// What the dialog asks of the chat.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Closed after a successful install: detect agents again.
    Installed,
    Closed,
    /// Pi needs a provider key: open Settings.
    Settings,
}

impl Install {
    pub fn new(agent: &str, host: &Host) -> Self {
        let def = crate::chat::agents::find(agent);
        let (methods, missing) = install::methods(agent, host);
        Self {
            agent: agent.to_owned(),
            name: def.map_or_else(|| agent.to_owned(), |d| d.name.to_owned()),
            docs: def.map_or("", |d| d.docs_url),
            methods,
            missing,
            pick: 0,
            run: None,
            output: vec![],
            done: None,
        }
    }

    fn method(&self) -> Option<&Method> {
        self.methods.get(self.pick)
    }

    /// Moves the running install's output into the dialog.
    fn drain(&mut self) {
        let Some(rx) = &self.run else { return };
        while let Ok(line) = rx.try_recv() {
            match line {
                Line::Out(l) => self.output.push(l),
                Line::Done(r) => self.done = Some(r),
            }
        }
        if self.output.len() > OUTPUT_MAX {
            self.output.drain(..self.output.len() - OUTPUT_MAX);
        }
        if self.done.is_some() {
            self.run = None;
        }
    }
}

/// Draws the dialog while `state` holds one. Nothing runs until "Install" is clicked.
pub fn dialog(ctx: &egui::Context, state: &mut Option<Install>) -> Option<Outcome> {
    let st = state.as_mut()?;
    st.drain();
    if st.run.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let p = theme::pal_from_ctx(ctx);
    let frame = egui::Frame::new().fill(p.sidebar).corner_radius(theme::r(24.0)).inner_margin(24.0);
    let resp = egui::Modal::new(egui::Id::new("chat-install")).frame(frame).show(ctx, |ui| {
        theme::modal_backing(ui, p.sidebar, theme::ru(24));
        ui.set_width(DIALOG_W.min(ui.ctx().content_rect().width() - 48.0));
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.label(RichText::new(format!("Install {}", st.name)).font(theme::semibold(18.0)));
        body(ui, st)
    });
    let close = resp.should_close() && st.run.is_none();
    let out = resp.inner.or_else(|| close.then_some(Outcome::Closed));
    if out.is_some() {
        *state = None;
    }
    out
}

fn body(ui: &mut egui::Ui, st: &mut Install) -> Option<Outcome> {
    let p = theme::pal(ui);
    let idle = st.run.is_none() && st.done.is_none();
    if st.methods.is_empty() {
        ui.label(RichText::new("No install method works on this machine yet.").color(p.secondary));
    } else if idle {
        ui.horizontal_wrapped(|ui| {
            for (i, m) in st.methods.iter().enumerate() {
                if kit::chip(ui, m.label, i == st.pick).clicked() {
                    st.pick = i;
                }
            }
        });
    }
    for m in &st.missing {
        ui.label(RichText::new(format!("{}: {}", m.label, m.reason)).size(theme::SECONDARY).color(p.tertiary));
    }
    let method = st.method().copied();
    if let Some(m) = method {
        ui.label(RichText::new("This runs:").size(theme::SECONDARY).color(p.secondary));
        ui.add(egui::Label::new(RichText::new(m.command).monospace()).selectable(true).wrap());
        if m.kind == Kind::Script {
            ui.label(RichText::new("It downloads a script from the agent's site and runs it.").size(theme::SECONDARY).color(p.warning));
        }
        if m.elevated {
            ui.label(RichText::new("It asks for a password or an admin shell: run it in a terminal.").size(theme::SECONDARY).color(p.warning));
        }
    }
    if !st.output.is_empty() {
        egui::ScrollArea::vertical().max_height(OUTPUT_H).stick_to_bottom(true).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(st.output.join("\n")).monospace().size(theme::SECONDARY)).selectable(true).wrap());
        });
    }
    match &st.done {
        Some(Ok(())) if st.agent == "pi" => drop(ui.label(RichText::new("Installed. Pi needs a key from one provider: add it in Settings, Agents.").color(p.success))),
        Some(Ok(())) => drop(ui.label(RichText::new("Installed.").color(p.success))),
        Some(Err(e)) => drop(ui.label(RichText::new(e).color(p.danger))),
        None => {}
    }
    if !st.docs.is_empty() && !matches!(st.done, Some(Ok(()))) {
        ui.hyperlink_to(RichText::new(format!("{} install docs", st.name)).size(theme::SECONDARY), st.docs);
    }
    ui.add_space(6.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if let Some(r) = &st.done {
            let next = match (r, st.agent.as_str()) {
                (Ok(()), "pi") => Some(("Add a key", Outcome::Settings)),
                (Ok(()), _) => Some(("Done", Outcome::Installed)),
                (Err(_), _) => None,
            };
            if let Some((label, out)) = next {
                return kit::button(ui, ButtonKind::Primary, None, label).clicked().then_some(out);
            }
        }
        if let Some(m) = method.filter(|m| idle && !m.elevated)
            && kit::button(ui, ButtonKind::Primary, None, "Install").clicked()
        {
            st.run = Some(install::run(&m));
        }
        if let Some(m) = method.filter(|_| st.run.is_none())
            && kit::button(ui, ButtonKind::Secondary, None, "Copy command").clicked()
        {
            ui.ctx().copy_text(m.command.to_owned());
        }
        let close = if st.run.is_some() { None } else { Some(if matches!(st.done, Some(Ok(()))) { Outcome::Installed } else { Outcome::Closed }) };
        match close {
            Some(c) if kit::button(ui, ButtonKind::Secondary, None, "Close").clicked() => Some(c),
            _ => None,
        }
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::install::Os;

    fn frame(ctx: &egui::Context, st: &mut Option<Install>) -> Vec<String> {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ctx| {
            dialog(ctx, st);
        });
        out.textures_delta.clear();
        out.shapes.iter().filter_map(|s| if let egui::epaint::Shape::Text(t) = &s.shape { Some(t.galley.text().to_owned()) } else { None }).collect()
    }

    #[test]
    fn the_command_shows_before_anything_runs() {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let host = Host { os: Os::Mac, distro: vec![], has: vec!["brew", "curl"], node: None };
        let mut st = Some(Install::new("opencode", &host));
        frame(&ctx, &mut st);
        let texts = frame(&ctx, &mut st);
        assert!(texts.iter().any(|t| t == "brew install anomalyco/tap/opencode"), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("npm: needs npm")), "a method that cannot run says why");
        assert!(st.as_ref().is_some_and(|s| s.run.is_none()), "nothing runs before Install is clicked");
    }

    #[test]
    fn a_failed_install_keeps_the_command_and_docs() {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let host = Host { os: Os::Mac, distro: vec![], has: vec!["brew"], node: None };
        let mut st = Some(Install::new("opencode", &host));
        st.as_mut().unwrap().done = Some(Err("`brew install anomalyco/tap/opencode` failed (exit status: 1)".into()));
        frame(&ctx, &mut st);
        let texts = frame(&ctx, &mut st);
        for want in ["brew install anomalyco/tap/opencode", "OpenCode install docs", "Copy command"] {
            assert!(texts.iter().any(|t| t.contains(want)), "{want} missing: {texts:?}");
        }
    }
}
