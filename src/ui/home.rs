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
    #[cfg(not(target_arch = "wasm32"))]
    pub show_updates: bool,
    pub renaming: Option<(PathBuf, String)>,
    /// Requests for the app, drained after each frame.
    pub actions: Vec<HomeAction>,
    pending_removal: Option<HomeAction>,
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

pub fn draw(ui: &mut Ui, hv: &mut HomeView, _native_chrome: bool, translucent: bool) {
    let p = theme::pal(ui);
    let floating = super::is_floating(translucent);
    let gap = super::PANEL_GAP;
    if floating && theme::is_acrylic() {
        // No canvas under the home screen: the glow covers the whole window.
        let screen = ui.ctx().content_rect();
        ui.painter().add(theme::backdrop_mesh(p, screen, screen));
    }
    let backdrop = |l: i8, r: i8, t: i8, b: i8| {
        egui::Frame::NONE
            .fill(theme::backdrop_fill(p))
            .inner_margin(egui::Margin {
                left: l,
                right: r,
                top: t,
                bottom: b,
            })
    };
    egui::Panel::left("home-sidebar")
        .resizable(false)
        .exact_size(252.0 + 2.0 * gap)
        .frame(if floating {
            backdrop(gap as i8, gap as i8, gap as i8, gap as i8)
        } else {
            egui::Frame::NONE
                .fill(theme::sidebar_fill(translucent, p))
                .inner_margin(egui::Margin::symmetric(10, 8))
        })
        .show(ui, |ui| {
            let body = |ui: &mut Ui| {
                ui.horizontal(|ui| {
                    super::kit::brand_mark(ui, 30.0);
                    ui.label(RichText::new("PaperCraft").font(theme::semibold(17.0)));
                });
                ui.add_space(14.0);
                if super::kit::button_wide(
                    ui,
                    super::kit::ButtonKind::Primary,
                    Some(Icon::Plus),
                    "New design",
                )
                .clicked()
                {
                    hv.actions.push(HomeAction::New);
                }
                ui.add_space(10.0);
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
                        .color(p.tertiary);
                    ui.label(path).on_hover_text("Projects folder");
                    #[cfg(not(target_arch = "wasm32"))]
                    if sidebar_item(ui, Icon::Restore, "Check for updates", None, false) {
                        hv.show_updates = true;
                    }
                    if sidebar_item(ui, Icon::Page, REVEAL_LABEL, None, false) {
                        hv.actions.push(HomeAction::RevealRoot);
                    }
                });
            };
            if floating {
                super::floating_panel(ui, p.sidebar, egui::Margin::symmetric(14, 14), body);
            } else {
                let mut body = body;
                body(ui);
            }
        });
    egui::CentralPanel::default()
        .frame(if floating {
            backdrop(0, gap as i8, gap as i8, gap as i8)
        } else {
            egui::Frame::NONE.fill(p.window).inner_margin(20)
        })
        .show(ui, |ui| {
            let body = |ui: &mut Ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            RichText::new(if hv.show_trash {
                                "Trash"
                            } else {
                                "Recent designs"
                            })
                            .font(theme::semibold(26.0)),
                        );
                        ui.label(
                            RichText::new(if hv.show_trash {
                                "Deleted projects stay here until you empty the trash."
                            } else {
                                "Pick up where you left off."
                            })
                            .color(p.secondary),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if hv.show_trash {
                            ui.add_enabled_ui(!hv.trash.is_empty(), |ui| {
                                if super::kit::button(
                                    ui,
                                    super::kit::ButtonKind::Danger,
                                    Some(Icon::Trash),
                                    "Empty Trash",
                                )
                                .clicked()
                                {
                                    hv.pending_removal = Some(HomeAction::EmptyTrash);
                                }
                            });
                        } else {
                            if !hv.library
                                && super::kit::button(
                                    ui,
                                    super::kit::ButtonKind::Secondary,
                                    None,
                                    "Join shared…",
                                )
                                .clicked()
                            {
                                hv.dialog = Some(ShareDialog {
                                    dir: None,
                                    text: String::new(),
                                });
                            }
                            if super::kit::button(
                                ui,
                                super::kit::ButtonKind::Secondary,
                                None,
                                "Import…",
                            )
                            .clicked()
                            {
                                hv.actions.push(HomeAction::Import);
                            }
                        }
                    });
                });
                ui.add_space(18.0);
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        if hv.show_trash {
                            trash_grid(ui, hv);
                        } else {
                            recents_grid(ui, hv);
                        }
                    });
            };
            if floating {
                super::floating_panel(ui, p.sidebar, egui::Margin::symmetric(28, 24), body);
            } else {
                let mut body = body;
                body(ui);
            }
        });
    share_dialog(ui, hv);
    removal_dialog(ui.ctx(), hv);
}

fn removal_dialog(ctx: &egui::Context, hv: &mut HomeView) {
    let Some(action) = hv.pending_removal.take() else {
        return;
    };
    let (title, message, button) = match &action {
        HomeAction::Trash(path) => {
            let Some(entry) = hv.entries.iter().find(|e| e.dir == *path) else {
                return;
            };
            (
                "Move project to Trash?",
                format!(
                    "Move “{}” to Trash? You can restore it later.",
                    entry.manifest.name
                ),
                "Move to Trash",
            )
        }
        HomeAction::DeleteForever(path) => {
            let Some(entry) = hv.trash.iter().find(|e| e.dir == *path) else {
                return;
            };
            (
                "Delete project permanently?",
                format!(
                    "Permanently delete “{}” and all its pages? This cannot be undone.",
                    entry.manifest.name
                ),
                "Delete permanently",
            )
        }
        HomeAction::EmptyTrash => (
            "Empty Trash?",
            format!(
                "Permanently delete all {} projects in Trash? This cannot be undone.",
                hv.trash.len()
            ),
            "Empty Trash",
        ),
        _ => unreachable!(),
    };
    match super::confirm::dialog(ctx, "home-removal", title, &message, button) {
        Some(true) => hv.actions.push(action),
        Some(false) => {}
        None => hv.pending_removal = Some(action),
    }
}

/// Width of the share / join dialog.
const SHARE_DIALOG_W: f32 = 380.0;

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
    let frame = egui::Frame::new()
        .fill(theme::pal_from_ctx(ui.ctx()).sidebar)
        .corner_radius(crate::theme::r(24.0))
        .inner_margin(24.0);
    let response = egui::Modal::new(egui::Id::new("share-dialog"))
        .frame(frame)
        .show(ui.ctx(), |ui| {
            theme::modal_backing(ui, theme::pal(ui).sidebar, theme::ru(24));
            ui.set_width(SHARE_DIALOG_W.min(ui.ctx().content_rect().width() - 48.0));
            ui.spacing_mut().item_spacing.y = 10.0;
            super::kit::badge(ui, Icon::Export, theme::accent(), 48.0);
            ui.label(RichText::new(title).font(theme::semibold(18.0)));
            let r = crate::theme::lit(ui, |ui| ui.add(
                egui::TextEdit::singleline(&mut d.text)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY)
                    .margin(theme::INPUT_MARGIN),
            ));
            if d.dir.is_some() {
                ui.label(
                    RichText::new("Anyone with the link can open and edit the project.")
                        .size(theme::SECONDARY)
                        .color(theme::pal(ui).secondary),
                );
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if super::kit::button(ui, super::kit::ButtonKind::Primary, None, button).clicked()
                    || enter
                {
                    submit = true;
                }
                if super::kit::button(ui, super::kit::ButtonKind::Secondary, None, "Cancel")
                    .clicked()
                {
                    keep = false;
                }
            });
        });
    if response.should_close() {
        keep = false;
    }
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
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::click());
    let hover = super::motion::hover(ui.ctx(), resp.id.with("nav"), &resp);
    if selected {
        theme::fill_selected(ui, row, theme::radius_lg(), p.field);
    } else if hover > 0.0 {
        ui.painter()
            .rect_filled(row, theme::radius_lg(), p.hover.gamma_multiply(hover));
    }
    let ir = egui::Rect::from_center_size(
        egui::pos2(row.left() + 20.0, row.center().y),
        egui::vec2(18.0, 18.0),
    );
    theme::paint_icon(
        ui.painter(),
        ir,
        icon,
        if selected {
            theme::accent()
        } else {
            p.secondary
        },
    );
    ui.painter().text(
        egui::pos2(row.left() + 40.0, row.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        if selected {
            theme::semibold(theme::BODY + 1.0)
        } else {
            egui::FontId::proportional(theme::BODY + 1.0)
        },
        p.text,
    );
    if let Some(b) = badge {
        ui.painter().text(
            egui::pos2(row.right() - 12.0, row.center().y),
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
    if hv.entries.is_empty() {
        ui.add_space(60.0);
        let mut new = false;
        super::kit::empty_state(
            ui,
            Icon::Page,
            theme::accent(),
            "No designs yet",
            if cfg!(target_arch = "wasm32") {
                "Projects are kept in this browser, saved as you work."
            } else {
                "Projects are folders in your Documents, saved as you work."
            },
            |ui| {
                new = super::kit::button(
                    ui,
                    super::kit::ButtonKind::Primary,
                    Some(Icon::Plus),
                    "New design",
                )
                .clicked();
            },
        );
        if new {
            hv.actions.push(HomeAction::New);
        }
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
    let hover = super::motion::hover(ui.ctx(), resp.id.with("card"), &resp);
    ui.painter().rect_filled(
        rect,
        theme::radius_panel(),
        theme::blend(p.field, theme::accent().gamma_multiply(0.12 * hover)),
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
        resp.context_menu(|ui| {
            crate::theme::popup_backing(ui);
            card_menu(ui, hv, e)
        });
        ui.add_space(6.0);
        let renaming = hv.renaming.as_ref().is_some_and(|(d, _)| *d == e.dir);
        if renaming {
            let (_, name) = hv.renaming.as_mut().unwrap();
            let r = crate::theme::lit(ui, |ui| ui.add(
                egui::TextEdit::singleline(name)
                    .margin(crate::theme::INPUT_MARGIN)
                    .desired_width(CARD_W),
            ));
            // Check before taking focus back: `lost_focus` reads the focus as it is now, so
            // requesting it first would hide Enter, Escape and a click elsewhere.
            if r.lost_focus() {
                let (dir, name) = hv.renaming.take().unwrap();
                if !ui.input(|i| i.key_pressed(egui::Key::Escape)) && name.trim() != e.manifest.name
                {
                    hv.actions.push(HomeAction::Rename(dir, name));
                }
            } else {
                r.request_focus();
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
                    egui::Popup::menu(&more).gap(crate::theme::POPUP_GAP).show(|ui| {
            crate::theme::popup_backing(ui);
            card_menu(ui, hv, e)
        });
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
            if super::icon_menu_item(ui, Icon::Export, "Copy share link").clicked() {
                ui.ctx().copy_text(link);
                ui.close();
            }
            if super::icon_menu_item(ui, Icon::Wrap, "Stop sharing").clicked() {
                hv.actions.push(HomeAction::Unshare(e.dir.clone()));
                ui.close();
            }
        }
        None => {
            if super::icon_menu_item(ui, Icon::Export, "Share…").clicked() {
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
    if super::icon_menu_item(ui, Icon::ArrowRight, "Open").clicked() {
        hv.actions.push(HomeAction::Open(e.dir.clone()));
        ui.close();
    }
    if super::icon_menu_item(ui, Icon::Text, "Rename").clicked() {
        hv.renaming = Some((e.dir.clone(), e.manifest.name.clone()));
        ui.close();
    }
    if super::icon_menu_item(ui, Icon::Group, "Duplicate").clicked() {
        hv.actions.push(HomeAction::Duplicate(e.dir.clone()));
        ui.close();
    }
    ui.separator();
    if super::icon_menu_item(ui, Icon::Trash, "Move to Trash").clicked() {
        hv.pending_removal = Some(HomeAction::Trash(e.dir.clone()));
        ui.close();
    }
}

fn trash_grid(ui: &mut Ui, hv: &mut HomeView) {
    let p = theme::pal(ui);
    if hv.trash.is_empty() {
        ui.add_space(60.0);
        super::kit::empty_state(
            ui,
            Icon::Trash,
            p.secondary,
            "Trash is empty",
            "Deleted projects stay here until you empty the trash.",
            |_| {},
        );
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
                        crate::theme::popup_backing(ui);
                        if super::icon_menu_item(ui, crate::theme::Icon::Restore, "Restore")
                            .clicked()
                        {
                            hv.actions.push(HomeAction::Restore(e.dir.clone()));
                            ui.close();
                        }
                        if super::icon_menu_item(
                            ui,
                            crate::theme::Icon::Trash,
                            "Delete Permanently",
                        )
                        .clicked()
                        {
                            hv.pending_removal = Some(HomeAction::DeleteForever(e.dir.clone()));
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
