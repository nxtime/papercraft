use crate::{
    theme,
    update::{self, Release},
};
use std::{path::PathBuf, sync::mpsc, time::Duration};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const WINDOW_WIDTH: f32 = 440.0;

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Checking,
    Current,
    Available(Release),
    Downloading {
        received: u64,
        total: u64,
    },
    Ready(PathBuf),
    Failed(String),
}

enum Message {
    Checked(Result<Option<Release>, String>),
    Progress(u64, u64),
    Downloaded(Result<PathBuf, String>),
}

#[derive(Default)]
pub struct UpdaterUi {
    pub open: bool,
    state: State,
    events: Option<mpsc::Receiver<Message>>,
}

impl UpdaterUi {
    pub fn check(&mut self) {
        if self.events.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.events = Some(rx);
        self.state = State::Checking;
        std::thread::spawn(move || {
            let result = runtime().and_then(|rt| rt.block_on(update::check()));
            let _ = tx.send(Message::Checked(result));
        });
    }

    fn download(&mut self, release: &Release) {
        let Some(asset) = release.asset.clone() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.events = Some(rx);
        self.state = State::Downloading {
            received: 0,
            total: asset.size,
        };
        std::thread::spawn(move || {
            let result = runtime().and_then(|rt| {
                rt.block_on(update::download(&asset, |received, total| {
                    let _ = tx.send(Message::Progress(received, total));
                }))
            });
            let _ = tx.send(Message::Downloaded(result));
        });
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if self.open && matches!(self.state, State::Idle) {
            self.check();
        }
        let mut finished = false;
        if let Some(rx) = &self.events {
            for event in rx.try_iter() {
                match event {
                    Message::Checked(result) => {
                        self.state = match result {
                            Ok(Some(release)) => State::Available(release),
                            Ok(None) => State::Current,
                            Err(error) => State::Failed(error),
                        };
                        finished = true;
                    }
                    Message::Progress(received, total) => {
                        self.state = State::Downloading { received, total }
                    }
                    Message::Downloaded(result) => {
                        self.state = match result {
                            Ok(path) => State::Ready(path),
                            Err(error) => State::Failed(error),
                        };
                        finished = true;
                    }
                }
            }
        }
        if finished {
            self.events = None;
        }
        if self.events.is_some() {
            ctx.request_repaint_after(POLL_INTERVAL);
        }
        let t = super::motion::progress(
            ctx,
            "updater-open",
            self.open,
            super::motion::FAST,
            super::motion::ease_out_cubic,
        );
        if !self.open && t <= 0.0 {
            return;
        }
        let live = self.open;
        let mut close = false;
        let mut check = false;
        let mut download = None;
        let mut installer = None;
        let frame = egui::Frame::new()
            .fill(theme::pal_from_ctx(ctx).sidebar)
            .corner_radius(crate::theme::r(24.0))
            .inner_margin(24.0);
        let modal = egui::Modal::new(egui::Id::new("updater-modal"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.multiply_opacity(t);
                theme::modal_backing(ui, theme::pal(ui).sidebar, theme::ru(24));
                ui.set_width(WINDOW_WIDTH);
                ui.spacing_mut().item_spacing.y = 10.0;
                let p = theme::pal(ui);
                let version = env!("CARGO_PKG_VERSION");
                let (icon, tint, title, sub) = match &self.state {
                    State::Idle | State::Checking => (theme::Icon::Restore, theme::accent(), "Checking for updates…".to_string(), "This only takes a moment.".to_string()),
                    State::Current => (theme::Icon::Star, theme::accent(), "PaperCraft is up to date".to_string(), format!("You have version {version}.")),
                    State::Available(r) => (theme::Icon::Export, theme::accent(), format!("PaperCraft {} is ready", r.version), format!("You have {version}.")),
                    State::Downloading { .. } => (theme::Icon::Export, theme::accent(), "Downloading the update…".to_string(), String::new()),
                    State::Ready(_) => (theme::Icon::Export, theme::accent(), "Update downloaded".to_string(), "Save your work before installing.".to_string()),
                    State::Failed(_) => (theme::Icon::Question, ui.visuals().error_fg_color, "Could not check for updates".to_string(), "Your work is safe.".to_string()),
                };
                ui.horizontal(|ui| {
                    super::kit::badge(ui, icon, tint, 48.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(egui::RichText::new(title).font(theme::semibold(18.0)));
                        if !sub.is_empty() {
                            ui.label(egui::RichText::new(sub).color(p.secondary));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if super::kit::close_button(ui).clicked() {
                            close = true;
                        }
                    });
                });
                match &self.state {
                    State::Idle | State::Checking => {
                        ui.add(egui::Spinner::new());
                    }
                    State::Current => {}
                    State::Available(release) => {
                        if !release.notes.trim().is_empty() {
                            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| { ui.label(&release.notes); });
                        }
                        ui.hyperlink_to("View release", &release.url);
                        if release.asset.is_none() {
                            ui.label(egui::RichText::new("No installer is published for this platform yet. Open the release page or update from your local checkout.").color(p.secondary));
                        }
                    }
                    State::Downloading { received, total } => {
                        let progress = if *total > 0 { *received as f32 / *total as f32 } else { 0.0 };
                        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, crate::theme::r(4.0), p.field);
                        let mut fill = r;
                        fill.set_width(r.width() * progress.clamp(0.0, 1.0));
                        ui.painter().rect_filled(fill, crate::theme::r(4.0), theme::accent());
                        ui.label(egui::RichText::new(format!("Downloaded {received} of {total} bytes")).size(theme::SECONDARY).color(p.secondary));
                    }
                    State::Ready(_) => {}
                    State::Failed(error) => {
                        ui.label(egui::RichText::new(error).color(ui.visuals().error_fg_color));
                    }
                }
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match &self.state {
                        State::Available(release) if release.asset.is_some() => {
                            if super::kit::button(ui, super::kit::ButtonKind::Primary, None, "Download update").clicked() {
                                download = Some(release.clone());
                            }
                        }
                        State::Ready(path) => {
                            if super::kit::button(ui, super::kit::ButtonKind::Primary, None, "Open installer").clicked() {
                                installer = Some(path.clone());
                            }
                        }
                        _ => {}
                    }
                    if self.events.is_none() && super::kit::button(ui, super::kit::ButtonKind::Secondary, None, "Check again").clicked() {
                        check = true;
                    }
                });
            });
        if live && modal.should_close() {
            close = true;
        }
        self.open = live && !close;
        if check {
            self.check();
        }
        if let Some(release) = download {
            self.download(&release);
        }
        if let Some(path) = installer
            && let Err(error) = open_installer(&path)
        {
            self.state = State::Failed(error);
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Could not start updater: {e}"))
}

fn open_installer(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("msiexec");
        command.arg("/i");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open installer: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_background_check_updates_idle_ui() {
        let (tx, rx) = mpsc::channel();
        let mut updater = UpdaterUi {
            state: State::Checking,
            events: Some(rx),
            ..Default::default()
        };
        tx.send(Message::Checked(Ok(None))).unwrap();
        updater.show(&egui::Context::default());
        assert!(matches!(updater.state, State::Current));
        assert!(updater.events.is_none());
    }

    #[test]
    fn unopened_updater_does_not_start_network_requests() {
        let mut updater = UpdaterUi::default();
        updater.show(&egui::Context::default());
        assert!(updater.events.is_none());
        assert!(matches!(updater.state, State::Idle));
    }

    #[test]
    fn updater_visuals() {
        let Ok(gpu) = crate::render::gpu::Headless::new() else {
            return;
        };
        let cases = [
            ("papercraft-updater-current.png", State::Current),
            ("papercraft-updater-available.png", State::Available(Release {
                version: "0.4.0".into(),
                notes: "Page folders, safer deletion, and restored design themes.".into(),
                url: "https://git.marcospaulo.dev.br/masi/papercraft/releases/tag/v0.4.0".into(),
                asset: Some(update::Asset { name: "papercraft-0.4.0-macos-aarch64.dmg".into(), url: "https://git.marcospaulo.dev.br/masi/papercraft/releases/download/v0.4.0/papercraft-0.4.0-macos-aarch64.dmg".into(), size: 1234 }),
            })),
        ];
        for (name, state) in cases {
            let mut ed = crate::editor::Editor::default();
            let mut st = crate::ui::UiState {
                updater: UpdaterUi {
                    open: true,
                    state,
                    events: None,
                },
                ..Default::default()
            };
            let image =
                crate::render::frame::capture(&gpu, &mut ed, &mut st, [1000.0, 700.0], 1.0, |_| {});
            image.save(std::env::temp_dir().join(name)).unwrap();
        }
    }
}
