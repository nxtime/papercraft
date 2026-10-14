//! The projects home screen: a recents grid with thumbnails, plus the trash.

use crate::project::{Entry, TrashEntry};
use crate::theme::{self, Icon};
use egui::{Color32, RichText, Ui};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub enum HomeAction {
    New,
    Import,
    Open(PathBuf),
    Rename(PathBuf, String),
    Duplicate(PathBuf),
    Trash(PathBuf),
    Restore(PathBuf),
    DeleteForever(PathBuf),
    EmptyTrash,
    /// Show the projects folder in the system file manager.
    RevealRoot,
    /// Put a project on this server (a relay) so others can open and edit it.
    Share(PathBuf, String),
    /// Open the shared project behind a share link.
    Join(String),
    /// Stop syncing a project; it stays here as a local copy.
    Unshare(PathBuf),
}

/// The small window opened by "Share…" and "Join shared project…".
#[derive(Debug, Clone)]
pub struct ShareDialog {
    /// The project to share, or None to join.
    pub dir: Option<PathBuf>,
    /// The server to share on, or the link to join.
    pub text: String,
}

/// What the sync engine reports for one shared project.
#[derive(Debug, Clone)]
pub struct ShareInfo {
    pub status: String,
    pub link: String,
}

#[derive(Default)]
pub struct HomeView {
    pub entries: Vec<Entry>,
    pub trash: Vec<TrashEntry>,
    pub show_trash: bool,
    pub renaming: Option<(PathBuf, String)>,
    /// Requests for the app, drained after each frame.
    pub actions: Vec<HomeAction>,
    /// "Now" for relative timestamps (unix ms); fixed in screenshots.
    pub now_ms: u64,
    pub root: PathBuf,
    pub dialog: Option<ShareDialog>,
    /// Shared projects by folder, filled by the shell every frame.
    pub shared: HashMap<PathBuf, ShareInfo>,
    /// Where a new share goes unless the user types another server.
    pub default_server: String,
    /// The server keeps a library: every project is shared, so no share controls.
    pub library: bool,
    thumbs: HashMap<PathBuf, (Option<SystemTime>, egui::TextureHandle)>,
}

const CARD_W: f32 = 232.0;
const THUMB_H: f32 = 148.0;
/// `HomeAction::RevealRoot` opens the folder natively; a browser has none, so the same action
/// downloads a zip of every project there.
const REVEAL_LABEL: &str = if cfg!(target_arch = "wasm32") {
    "Download backup"
} else {
    "Open projects folder"
};

impl HomeView {
    /// Re-read the project list and trash from the store (keeps cached thumbnails).
    pub fn refresh(&mut self, store: &crate::project::Store, now_ms: u64) {
        self.root = store.root.clone();
        self.entries = store.list();
        self.trash = store.list_trash();
        self.now_ms = now_ms;
    }

    pub fn load(store: &crate::project::Store, now_ms: u64) -> HomeView {
        let mut hv = HomeView::default();
        hv.refresh(store, now_ms);
        hv
    }

    fn thumb(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
        let mtime = crate::vfs::metadata(path).and_then(|m| m.modified()).ok();
        if let Some((t, h)) = self.thumbs.get(path)
            && *t == mtime
        {
            return Some(h.clone());
        }
        let img = image::load_from_memory(&crate::vfs::read(path).ok()?)
            .ok()?
            .to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        let h = ctx.load_texture(path.to_string_lossy(), ci, egui::TextureOptions::LINEAR);
        self.thumbs.insert(path.to_path_buf(), (mtime, h.clone()));
        Some(h)
    }
}

/// "Edited 5 minutes ago" style relative time.
pub fn relative_time(then_ms: u64, now_ms: u64) -> String {
    let s = now_ms.saturating_sub(then_ms) / 1000;
    match s {
        0..=59 => "just now".into(),
        60..=3599 => plural(s / 60, "minute"),
        3600..=86_399 => plural(s / 3600, "hour"),
        86_400..=2_591_999 => plural(s / 86_400, "day"),
        _ => plural(s / 2_592_000, "month"),
    }
}

fn plural(n: u64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{n} {unit}s ago")
    }
}

pub fn draw(ui: &mut Ui, hv: &mut HomeView, native_chrome: bool, translucent: bool) {
    let p = theme::pal(ui);
    egui::Panel::left("home-sidebar")
        .resizable(false)
        .exact_size(200.0)
        .frame(
            egui::Frame::NONE
                .fill(theme::sidebar_fill(translucent, p))
                .inner_margin(egui::Margin::symmetric(10, 8)),
        )
        .show(ui, |ui| {
            if native_chrome {
                ui.add_space(theme::TOOLBAR_H - 8.0);
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("PaperCraft")
                    .font(theme::semibold(theme::SECONDARY))
                    .color(p.secondary),
            );
            ui.add_space(4.0);
            if sidebar_item(ui, Icon::Grid, "Recents", None, !hv.show_trash) {
                hv.show_trash = false;
            }
            let n = hv.trash.len();
            if sidebar_item(
                ui,
                Icon::Trash,
                "Trash",
                (n > 0).then(|| n.to_string()),
                hv.show_trash,
            ) {
                hv.show_trash = true;
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(4.0);
                let path = RichText::new(hv.root.display().to_string())
                    .size(theme::SECONDARY - 1.0)
                    .color(p.secondary);
                ui.label(path).on_hover_text("Projects folder");
                if sidebar_item(ui, Icon::Page, REVEAL_LABEL, None, false) {
                    hv.actions.push(HomeAction::RevealRoot);
                }
            });
        });
    egui::Panel::top("home-toolbar")
        .exact_size(theme::TOOLBAR_H + 8.0)
        .frame(
            egui::Frame::NONE
                .fill(p.window)
                .inner_margin(egui::Margin::symmetric(20, 0)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(
                    RichText::new(if hv.show_trash { "Trash" } else { "Recents" })
                        .font(theme::semibold(17.0)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if hv.show_trash {
                        if ui
                            .add_enabled(!hv.trash.is_empty(), egui::Button::new("Empty Trash"))
                            .clicked()
                        {
                            hv.actions.push(HomeAction::EmptyTrash);
                        }
                    } else {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("New Project").color(Color32::WHITE),
                                )
                                .fill(theme::accent()),
                            )
                            .clicked()
                        {
                            hv.actions.push(HomeAction::New);
                        }
                        if ui.button("Import…").clicked() {
                            hv.actions.push(HomeAction::Import);
                        }
                        if !hv.library && ui.button("Join shared…").clicked() {
                            hv.dialog = Some(ShareDialog {
                                dir: None,
                                text: String::new(),
                            });
                        }
                    }
                });
            });
            let r = ui.max_rect();
            ui.painter().hline(
                r.x_range(),
                r.bottom() - 0.5,
                egui::Stroke::new(1.0, p.separator),
            );
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(p.window).inner_margin(20))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    if hv.show_trash {
                        trash_grid(ui, hv);
                    } else {
                        recents_grid(ui, hv);
                    }
                });
        });
    share_dialog(ui, hv);
}

fn share_dialog(ui: &mut Ui, hv: &mut HomeView) {
    let Some(mut d) = hv.dialog.take() else {
        return;
    };
    let (mut keep, mut submit) = (true, false);
    let (title, hint, button) = match d.dir {
        Some(_) => (
            "Share project",
            "Server, e.g. https://papercraft.example.com",
            "Share",
        ),
        None => ("Join shared project", "Paste the share link", "Join"),
    };
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut keep)
        .show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut d.text)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY),
            );
            if d.dir.is_some() {
                ui.label(
                    RichText::new("Anyone with the link can open and edit the project.")
                        .size(theme::SECONDARY)
                        .color(theme::pal(ui).secondary),
                );
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button(button).clicked() || enter {
                submit = true;
            }
        });
    if submit && !d.text.trim().is_empty() {
        let text = d.text.trim().to_string();
        hv.actions.push(match d.dir {
            Some(dir) => HomeAction::Share(dir, text),
            None => HomeAction::Join(text),
        });
    } else if keep {
        hv.dialog = Some(d);
    }
}

fn sidebar_item(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    badge: Option<String>,
    selected: bool,
) -> bool {
    let p = theme::pal(ui);
    let (row, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    if selected {
        ui.painter().rect_filled(row, 6.0, p.pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(row, 6.0, p.hover);
    }
    let ir = egui::Rect::from_center_size(
        egui::pos2(row.left() + 16.0, row.center().y),
        egui::vec2(15.0, 15.0),
    );
    theme::paint_icon(ui.painter(), ir, icon, theme::accent());
    ui.painter().text(
        egui::pos2(row.left() + 32.0, row.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme::BODY),
        p.text,
    );
    if let Some(b) = badge {
        ui.painter().text(
            egui::pos2(row.right() - 8.0, row.center().y),
            egui::Align2::RIGHT_CENTER,
            b,
            egui::FontId::proportional(theme::SECONDARY),
            p.secondary,
        );
    }
    resp.clicked()
}

fn columns(ui: &Ui) -> usize {
    ((ui.available_width() + 20.0) / (CARD_W + 20.0))
        .floor()
        .max(1.0) as usize
}

fn recents_grid(ui: &mut Ui, hv: &mut HomeView) {
    let p = theme::pal(ui);
    if hv.entries.is_empty() {
        ui.add_space(80.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No projects yet").font(theme::semibold(17.0)));
            ui.add_space(4.0);
            ui.label(
                RichText::new("Projects are folders in your Documents, saved as you work.")
                    .color(p.secondary),
            );
            ui.add_space(12.0);
            if ui
                .add(
                    egui::Button::new(RichText::new("New Project").color(Color32::WHITE))
                        .fill(theme::accent()),
                )
                .clicked()
            {
                hv.actions.push(HomeAction::New);
            }
        });
        return;
    }
    let cols = columns(ui);
    let entries = hv.entries.clone();
    for row in entries.chunks(cols) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            for e in row {
                project_card(ui, hv, e);
            }
        });
        ui.add_space(16.0);
    }
}

fn card_frame(
    ui: &mut Ui,
    hv: &mut HomeView,
    thumb: Option<&Path>,
    id: egui::Id,
) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(CARD_W, THUMB_H), egui::Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        8.0,
        if ui.visuals().dark_mode {
            Color32::from_rgb(0x1f, 0x1f, 0x21)
        } else {
            Color32::from_rgb(0xee, 0xee, 0xf0)
        },
    );
    if let Some(t) = thumb.and_then(|t| hv.thumb(ui.ctx(), t)) {
        // Fit the thumbnail inside the card, centered.
        let [tw, th] = t.size();
        let s = ((rect.width() - 16.0) / tw as f32)
            .min((rect.height() - 16.0) / th as f32)
            .min(1.0);
        let r =
            egui::Rect::from_center_size(rect.center(), egui::vec2(tw as f32 * s, th as f32 * s));
        ui.painter().image(
            t.id(),
            r,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        theme::paint_icon(
            ui.painter(),
            egui::Rect::from_center_size(rect.center(), egui::vec2(28.0, 28.0)),
            Icon::Page,
            p.tertiary,
        );
    }
    ui.painter().rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(
            if hovered { 2.0 } else { 1.0 },
            if hovered {
                theme::accent()
            } else {
                p.separator
            },
        ),
        egui::StrokeKind::Inside,
    );
    let _ = id;
    resp
}

fn project_card(ui: &mut Ui, hv: &mut HomeView, e: &Entry) {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.set_width(CARD_W);
        ui.spacing_mut().item_spacing.y = 2.0;
        let id = egui::Id::new(("card", &e.dir));
        let resp = card_frame(ui, hv, e.thumbnail.as_deref(), id);
        if resp.double_clicked() {
            hv.actions.push(HomeAction::Open(e.dir.clone()));
        }
        resp.context_menu(|ui| card_menu(ui, hv, e));
        ui.add_space(6.0);
        let renaming = hv.renaming.as_ref().is_some_and(|(d, _)| *d == e.dir);
        if renaming {
            let (_, name) = hv.renaming.as_mut().unwrap();
            let r = ui.add(
                egui::TextEdit::singleline(name)
                    .margin(crate::theme::INPUT_MARGIN)
                    .desired_width(CARD_W),
            );
            r.request_focus();
            if r.lost_focus() {
                let (dir, name) = hv.renaming.take().unwrap();
                if !ui.input(|i| i.key_pressed(egui::Key::Escape)) && name.trim() != e.manifest.name
                {
                    hv.actions.push(HomeAction::Rename(dir, name));
                }
            }
        } else {
            ui.horizontal(|ui| {
                let name = ui.add(
                    egui::Label::new(
                        RichText::new(&e.manifest.name).font(theme::semibold(theme::BODY)),
                    )
                    .truncate()
                    .sense(egui::Sense::click()),
                );
                if name.double_clicked() {
                    hv.renaming = Some((e.dir.clone(), e.manifest.name.clone()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let more =
                        theme::icon_button(ui, Icon::More, 20.0, false).on_hover_text("More");
                    egui::Popup::menu(&more).show(|ui| card_menu(ui, hv, e));
                });
            });
        }
        let pages = e.manifest.pages.len();
        ui.label(
            RichText::new(format!(
                "Edited {} · {} page{}",
                relative_time(e.manifest.modified, hv.now_ms),
                pages,
                if pages == 1 { "" } else { "s" }
            ))
            .size(theme::SECONDARY)
            .color(p.secondary),
        );
        if let Some(s) = hv.shared.get(&e.dir) {
            let prefix = if hv.library { "" } else { "Shared · " };
            ui.label(
                RichText::new(format!("{prefix}{}", s.status))
                    .size(theme::SECONDARY)
                    .color(p.secondary),
            );
        }
    });
}

fn card_menu(ui: &mut Ui, hv: &mut HomeView, e: &Entry) {
    match hv.shared.get(&e.dir).map(|s| s.link.clone()) {
        _ if hv.library => {}
        Some(link) => {
            if ui.button("Copy share link").clicked() {
                ui.ctx().copy_text(link);
                ui.close();
            }
            if ui.button("Stop sharing").clicked() {
                hv.actions.push(HomeAction::Unshare(e.dir.clone()));
                ui.close();
            }
        }
        None => {
            if ui.button("Share…").clicked() {
                hv.dialog = Some(ShareDialog {
                    dir: Some(e.dir.clone()),
                    text: hv.default_server.clone(),
                });
                ui.close();
            }
        }
    }
    if !hv.library {
        ui.separator();
    }
    if ui.button("Open").clicked() {
        hv.actions.push(HomeAction::Open(e.dir.clone()));
        ui.close();
    }
    if ui.button("Rename").clicked() {
        hv.renaming = Some((e.dir.clone(), e.manifest.name.clone()));
        ui.close();
    }
    if ui.button("Duplicate").clicked() {
        hv.actions.push(HomeAction::Duplicate(e.dir.clone()));
        ui.close();
    }
    ui.separator();
    if ui.button("Move to Trash").clicked() {
        hv.actions.push(HomeAction::Trash(e.dir.clone()));
        ui.close();
    }
}

fn trash_grid(ui: &mut Ui, hv: &mut HomeView) {
    let p = theme::pal(ui);
    if hv.trash.is_empty() {
        ui.add_space(80.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Trash is empty").font(theme::semibold(17.0)));
            ui.label(
                RichText::new("Deleted projects stay here until you empty the trash.")
                    .color(p.secondary),
            );
        });
        return;
    }
    let cols = columns(ui);
    let entries = hv.trash.clone();
    for row in entries.chunks(cols) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            for e in row {
                ui.vertical(|ui| {
                    ui.set_width(CARD_W);
                    let thumb = e.dir.join(crate::project::THUMBNAIL);
                    let thumb = crate::vfs::is_file(&thumb).then_some(thumb);
                    let resp = card_frame(ui, hv, thumb.as_deref(), egui::Id::new(&e.dir));
                    resp.context_menu(|ui| {
                        if ui.button("Restore").clicked() {
                            hv.actions.push(HomeAction::Restore(e.dir.clone()));
                            ui.close();
                        }
                        if ui.button("Delete Permanently").clicked() {
                            hv.actions.push(HomeAction::DeleteForever(e.dir.clone()));
                            ui.close();
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&e.manifest.name).font(theme::semibold(theme::BODY)),
                            )
                            .truncate(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::icon_button(ui, Icon::Restore, 20.0, false)
                                .on_hover_text("Restore")
                                .clicked()
                            {
                                hv.actions.push(HomeAction::Restore(e.dir.clone()));
                            }
                        });
                    });
                    ui.label(
                        RichText::new(format!("Deleted {}", relative_time(e.deleted, hv.now_ms)))
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                });
            }
        });
        ui.add_space(16.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_times() {
        let now = 10_000_000_000;
        assert_eq!(relative_time(now - 5_000, now), "just now");
        assert_eq!(relative_time(now - 60_000, now), "1 minute ago");
        assert_eq!(relative_time(now - 3 * 3_600_000, now), "3 hours ago");
        assert_eq!(relative_time(now - 2 * 86_400_000, now), "2 days ago");
        assert_eq!(relative_time(now + 1, now), "just now");
    }
}
