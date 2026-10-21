use super::UiState;
use crate::{editor::Editor, theme};

const DIALOG_WIDTH: f32 = 420.0;

pub enum Removal {
    Page { id: String, name: String },
    Group(String),
    Mode(String),
    Variable(String),
    Token(String),
}

pub struct PendingRemoval {
    project_stamp: u64,
    action: Removal,
}

pub fn request(st: &mut UiState, ed: &Editor, action: Removal) {
    st.pending_removal = Some(PendingRemoval {
        project_stamp: ed.project_stamp,
        action,
    });
}

pub(super) fn dialog(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    message: &str,
    button: &str,
) -> Option<bool> {
    let frame = egui::Frame::new()
        .fill(theme::pal_from_ctx(ctx).sidebar)
        .corner_radius(24.0)
        .inner_margin(24.0);
    let response = egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .show(ctx, |ui| {
            ui.multiply_opacity(super::motion::appear(
                ctx,
                ("dialog-appear", id),
                super::motion::MEDIUM,
                super::motion::ease_out_cubic,
            ));
            ui.set_width(DIALOG_WIDTH.min(ui.ctx().content_rect().width() - 48.0));
            ui.spacing_mut().item_spacing.y = 10.0;
            super::kit::badge(ui, theme::Icon::Trash, ui.visuals().error_fg_color, 48.0);
            ui.label(egui::RichText::new(title).font(theme::semibold(18.0)));
            ui.label(egui::RichText::new(message).color(theme::pal(ui).secondary));
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if super::kit::button(ui, super::kit::ButtonKind::Danger, None, button).clicked() {
                    return Some(true);
                }
                let cancel =
                    super::kit::button(ui, super::kit::ButtonKind::Secondary, None, "Cancel");
                if cancel.clicked() {
                    return Some(false);
                }
                None
            })
            .inner
        });
    response
        .inner
        .or_else(|| response.should_close().then_some(false))
}

/// What the last dialog showed, kept so it can fade out after it is answered.
#[derive(Clone)]
struct Ghost {
    title: String,
    message: String,
    button: String,
    since: f64,
}

fn ghost_id() -> egui::Id {
    egui::Id::new("confirm-dialog-ghost")
}

/// Draws an answered dialog, inert, fading out over `motion::FAST`.
fn paint_ghost(ctx: &egui::Context) {
    let Some(g) = ctx.data(|d| d.get_temp::<Ghost>(ghost_id())) else {
        return;
    };
    let t = 1.0 - ((ctx.input(|i| i.time) - g.since) as f32 / super::motion::FAST).clamp(0.0, 1.0);
    if t <= 0.0 || !super::motion::enabled(ctx) {
        ctx.data_mut(|d| d.remove::<Ghost>(ghost_id()));
        return;
    }
    ctx.request_repaint();
    let frame = egui::Frame::new()
        .fill(theme::pal_from_ctx(ctx).sidebar)
        .corner_radius(24.0)
        .inner_margin(24.0);
    egui::Modal::new(egui::Id::new("editor-removal-ghost"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.multiply_opacity(t);
            ui.disable();
            ui.set_width(DIALOG_WIDTH.min(ui.ctx().content_rect().width() - 48.0));
            ui.spacing_mut().item_spacing.y = 10.0;
            super::kit::badge(ui, theme::Icon::Trash, ui.visuals().error_fg_color, 48.0);
            ui.label(egui::RichText::new(&g.title).font(theme::semibold(18.0)));
            ui.label(egui::RichText::new(&g.message).color(theme::pal(ui).secondary));
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                super::kit::button(ui, super::kit::ButtonKind::Danger, None, &g.button);
                super::kit::button(ui, super::kit::ButtonKind::Secondary, None, "Cancel");
            });
        });
}

pub(super) fn show(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    paint_ghost(ctx);
    let Some(pending) = st.pending_removal.take() else {
        return;
    };
    if ed.project_stamp != pending.project_stamp {
        return;
    }
    let (title, message, button) = match &pending.action {
        Removal::Page { name, .. } => (
            "Delete page?",
            format!("Delete “{name}” and all its layers? This cannot be undone."),
            "Delete",
        ),
        Removal::Group(name) => (
            "Remove page group?",
            format!("Remove “{name}”? Its pages will be kept outside the group."),
            "Remove",
        ),
        Removal::Mode(name) => (
            "Delete mode?",
            format!(
                "Delete “{name}” from every page? If it is active, the next available mode will be selected."
            ),
            "Delete",
        ),
        Removal::Variable(name) => (
            "Delete variable?",
            format!(
                "Delete “--{name}” from every page and mode? Linked fills will keep their current color."
            ),
            "Delete",
        ),
        Removal::Token(name) => (
            "Delete token?",
            format!("Delete “--{name}” from every page?"),
            "Delete",
        ),
    };
    let answer = dialog(ctx, "editor-removal", title, &message, button);
    if answer.is_some() {
        let ghost = Ghost {
            title: title.to_owned(),
            message: message.clone(),
            button: button.to_owned(),
            since: ctx.input(|i| i.time),
        };
        ctx.data_mut(|d| d.insert_temp(ghost_id(), ghost));
        ctx.request_repaint();
    }
    match answer {
        None => st.pending_removal = Some(pending),
        Some(false) => {}
        Some(true) => match pending.action {
            Removal::Page { id, .. } => {
                if let Some(i) = ed.pages.iter().position(|page| page.id == id) {
                    ed.delete_page(i);
                    st.pages.clear_interaction();
                }
            }
            Removal::Group(name) => {
                ed.remove_page_group(&name);
            }
            action => {
                // One design-system edit: every page loses it, not just the active one.
                let r = ed.edit_design("Remove design token", |ds| match &action {
                    Removal::Mode(name) => crate::editor::design::remove_mode_by_name(ds, name),
                    Removal::Variable(name) | Removal::Token(name) => {
                        crate::editor::design::remove_by_css(ds, name)
                    }
                    _ => unreachable!(),
                });
                if let Err(e) = r {
                    ed.status = e.to_string();
                }
            }
        },
    }
}
