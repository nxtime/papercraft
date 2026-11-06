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
    /// `ready` is an unpacked bundle (a folder) or an installer (a file).
    Ready { version: String, ready: PathBuf },
    Failed(String),
}

enum Message {
    Checked(Result<Option<Release>, String>),
    Progress(u64, u64),
    Downloaded(Result<(String, PathBuf), String>),
}

#[derive(Default)]
pub struct UpdaterUi {
    pub open: bool,
    state: State,
    events: Option<mpsc::Receiver<Message>>,
    /// A check nobody asked for: it stays out of sight until an update is ready.
    quiet: bool,
    /// Set by "Restart to update"; the app acts on it once it has closed.
    pub restart: Option<Restart>,
}

/// What happens as the app closes to update.
#[derive(Clone, Debug, PartialEq)]
pub enum Restart {
    /// The files are already replaced: start again.
    Relaunch,
    /// Run this installer, then start again.
    Install(PathBuf),
}

/// Whether updates download on their own. Stored as `0` or `1`, on by default.
pub fn auto_enabled() -> bool {
    pref_path().and_then(|p| std::fs::read_to_string(p).ok()).is_none_or(|s| s.trim() != "0")
}

pub fn set_auto_enabled(on: bool) {
    let Some(path) = pref_path() else { return };
    if let Err(e) = std::fs::write(path, if on { "1" } else { "0" }) {
        log::warn!("could not save the update setting: {e}");
    }
}

fn pref_path() -> Option<PathBuf> {
    Some(crate::project::config_dir()?.join("auto-update"))
}

impl UpdaterUi {
    /// Looks for an update at launch and, if there is one, downloads it without asking.
    pub fn check_quietly(&mut self) {
        self.check();
        self.quiet = self.events.is_some();
    }

    pub fn check(&mut self) {
        self.quiet = false;
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
        let Some(asset) = release.package().cloned() else {
            return;
        };
        let version = release.version.clone();
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
            let result = result.and_then(|path| update::unpack(&path)).map(|ready| (version, ready));
            let _ = tx.send(Message::Downloaded(result));
        });
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if self.open && matches!(self.state, State::Idle) {
            self.check();
        }
        let mut finished = false;
        let mut fetch = None;
        if let Some(rx) = &self.events {
            for event in rx.try_iter() {
                match event {
                    Message::Checked(result) => {
                        self.state = match result {
                            Ok(Some(release)) => {
                                if self.quiet && release.package().is_some() {
                                    fetch = Some(release.clone());
                                }
                                State::Available(release)
                            }
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
                            Ok((version, ready)) => {
                                self.open = true;
                                State::Ready { version, ready }
                            }
                            Err(error) => State::Failed(error),
                        };
                        finished = true;
                    }
                }
            }
        }
        if finished {
            self.events = None;
            // Nobody is waiting on a quiet check, so a failure just waits for the next launch.
            if self.quiet && !self.open && fetch.is_none() {
                if let State::Failed(error) = &self.state {
                    log::warn!("update check failed: {error}");
                }
                self.state = State::Idle;
                self.quiet = false;
            }
        }
        if let Some(release) = fetch {
            self.download(&release);
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
        let mut restart = None;
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
                    State::Ready { version: new, .. } => (theme::Icon::Export, theme::accent(), format!("PaperCraft {new} is ready to install"), "Restart to finish. Your work is saved first.".to_string()),
                    State::Failed(_) => (theme::Icon::Question, ui.visuals().error_fg_color, "Could not update".to_string(), "Your work is safe.".to_string()),
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
                        if release.package().is_none() {
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
                        let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
                        ui.label(egui::RichText::new(format!("{:.0} of {:.0} MB", mb(*received), mb(*total))).size(theme::SECONDARY).color(p.secondary));
                    }
                    State::Ready { ready, .. } if !ready.is_dir() && cfg!(windows) => {
                        ui.label(egui::RichText::new("Windows will ask for permission to install.").size(theme::SECONDARY).color(p.secondary));
                    }
                    State::Ready { .. } => {}
                    State::Failed(error) => {
                        ui.label(egui::RichText::new(error).color(ui.visuals().error_fg_color));
                    }
                }
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match &self.state {
                        State::Available(release) if release.package().is_some() => {
                            if super::kit::button(ui, super::kit::ButtonKind::Primary, None, "Download update").clicked() {
                                download = Some(release.clone());
                            }
                        }
                        State::Ready { ready, .. } if cfg!(windows) => {
                            if super::kit::button(ui, super::kit::ButtonKind::Primary, None, "Restart to update").clicked() {
                                restart = Some(ready.clone());
                            }
                            if super::kit::button(ui, super::kit::ButtonKind::Secondary, None, "Later").clicked() {
                                close = true;
                            }
                        }
                        State::Ready { ready, .. } => {
                            if super::kit::button(ui, super::kit::ButtonKind::Primary, None, "Open installer").clicked() {
                                installer = Some(ready.clone());
                            }
                        }
                        _ => {}
                    }
                    if self.events.is_none() && !matches!(self.state, State::Ready { .. }) && super::kit::button(ui, super::kit::ButtonKind::Secondary, None, "Check again").clicked() {
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
        if let Some(ready) = restart {
            match prepare_restart(&ready) {
                Ok(next) => self.restart = Some(next),
                Err(error) => self.state = State::Failed(error),
            }
        }
    }
}

/// Swaps the files now, not as the app closes, so a failure can still be shown.
fn prepare_restart(ready: &std::path::Path) -> Result<Restart, String> {
    if !ready.is_dir() {
        return Ok(Restart::Install(ready.to_path_buf()));
    }
    let exe = std::env::current_exe().map_err(|e| format!("could not find the app: {e}"))?;
    let dir = exe.parent().ok_or("the app has no folder")?;
    update::install(ready, dir)?;
    Ok(Restart::Relaunch)
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
    fn a_failed_quiet_check_stays_out_of_sight() {
        let (tx, rx) = mpsc::channel();
        let mut updater = UpdaterUi {
            state: State::Checking,
            events: Some(rx),
            quiet: true,
            ..Default::default()
        };
        tx.send(Message::Checked(Err("offline".into()))).unwrap();
        updater.show(&egui::Context::default());
        assert!(!updater.open);
        assert!(matches!(updater.state, State::Idle));
    }

    #[test]
    fn a_finished_download_opens_the_prompt() {
        let (tx, rx) = mpsc::channel();
        let mut updater = UpdaterUi {
            state: State::Downloading { received: 0, total: 1 },
            events: Some(rx),
            quiet: true,
            ..Default::default()
        };
        tx.send(Message::Downloaded(Ok(("9.9.9".into(), PathBuf::from("x.msi"))))).unwrap();
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| updater.show(ui.ctx())).textures_delta.clear();
        assert!(updater.open);
        assert!(matches!(updater.state, State::Ready { .. }));
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
                url: "https://github.com/nxtime/papercraft/releases/tag/v0.4.0".into(),
                asset: Some(update::Asset { name: "papercraft-0.4.0-macos-aarch64.dmg".into(), url: "https://github.com/nxtime/papercraft/releases/download/v0.4.0/papercraft-0.4.0-macos-aarch64.dmg".into(), size: 1234, digest: String::new() }),
                portable: None,
            })),
        ];
        for (name, state) in cases {
            let mut ed = crate::editor::Editor::default();
            let mut st = crate::ui::UiState {
                updater: UpdaterUi {
                    open: true,
                    state,
                    ..Default::default()
                },
                ..Default::default()
            };
            let image =
                crate::render::frame::capture(&gpu, &mut ed, &mut st, [1000.0, 700.0], 1.0, |_| {});
            image.save(std::env::temp_dir().join(name)).unwrap();
        }
    }
}
