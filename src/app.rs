//! The windowed application: winit event loop, wgpu surface, egui integration, file dialogs.

use crate::editor::{AppRequest, Button, Editor, InputEvent, Key, Modifiers};
use crate::geom::{V2, v2};
use crate::io::{file, png};
use crate::project::Store;
use crate::render::frame::{self, Painter};
use crate::render::gpu::Headless;
use crate::session::Session;
use crate::theme;
use crate::ui::home::HomeAction;
use crate::ui::{self, UiState};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WKey, NamedKey};
use winit::window::{Window, WindowId};

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    painter: Painter,
    egui: egui_winit::State,
    adapter_info: wgpu::AdapterInfo,
}

pub struct App {
    gfx: Option<Gfx>,
    pub ed: Editor,
    session: Session,
    st: UiState,
    ctx: egui::Context,
    start: Instant,
    canvas: egui::Rect,
    captured: bool,
    /// Title bar close button pressed; handled like `CloseRequested` in `about_to_wait`.
    close_requested: bool,
    title: String,
    /// When egui asked for a delayed repaint (tooltips, caret blink, animations).
    next_repaint: Option<Instant>,
    #[cfg(target_os = "macos")]
    menu: Option<crate::macos::NativeMenu>,
    /// Tool calls from the MCP server, and the server itself (stops when dropped).
    mcp: Option<(
        std::sync::mpsc::Receiver<crate::mcp::Call>,
        crate::mcp::server::Running,
        crate::mcp::server::Dispatch,
    )>,
    /// Groups the agent's editing calls into reviewable runs.
    runs: crate::runs::Tracker,
}

/// Events delivered to the winit loop from other sources.
#[derive(Debug)]
pub enum UserEvent {
    #[cfg(target_os = "macos")]
    Menu(muda::MenuId),
    /// MCP tool calls are waiting in `App::mcp`.
    Mcp,
}

/// What to show at startup.
pub enum Start {
    /// The projects home screen.
    Home,
    /// Open this project folder.
    Project(PathBuf),
    /// A scratch editor that is not backed by a project (demo, tests).
    Scratch(Box<Editor>),
}

impl App {
    pub fn new(start: Start, store: Store) -> App {
        let ctx = egui::Context::default();
        theme::set_material(theme::Material::load());
        theme::set_roundness(theme::Roundness::load());
        theme::set_ui_font(theme::load_ui_font());
        theme::set_accent_choice(theme::load_accent_choice());
        theme::apply_choice(&ctx, theme::Choice::load());
        theme::apply_ui_scale(&ctx, theme::load_ui_scale(), false);
        let library = crate::library::Library::load(&crate::library::Library::path_in(&store.root));
        let mut app = App {
            gfx: None,
            ed: Editor::default(),
            session: Session::new(store),
            st: UiState::default(),
            ctx,
            start: Instant::now(),
            canvas: egui::Rect::NOTHING,
            captured: false,
            close_requested: false,
            title: String::new(),
            next_repaint: None,
            #[cfg(target_os = "macos")]
            menu: None,
            mcp: None,
            runs: Default::default(),
        };
        app.st.library.library = library;
        app.st.perf.prefs = crate::ui::perf::PerfPrefs::load();
        app.ed.clipboard_io = crate::io::clipboard::System::new()
            .map(|c| Box::new(c) as Box<dyn crate::io::clipboard::Provider>);
        match start {
            Start::Home => app.show_home(),
            Start::Project(dir) => {
                if let Err(e) = app.session.open(&mut app.ed, &dir) {
                    app.error(format!("Could not open {}: {e}", dir.display()));
                    app.show_home();
                } else {
                    app.st.project_name = app.session.project_name();
                    app.st.project_dir = app.session.project.as_ref().map(|p| p.dir.clone());
                }
            }
            Start::Scratch(ed) => app.ed = *ed,
        }
        app
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn to_canvas(&self, p: V2) -> V2 {
        p - v2(self.canvas.min.x as f64, self.canvas.min.y as f64)
    }

    fn over_canvas(&self, p: V2) -> bool {
        self.canvas.contains(egui::pos2(p.x as f32, p.y as f32)) && !self.ctx.is_pointer_over_egui()
    }

    /// Before leaving a scratch (project-less) document. Projects autosave, so they never ask.
    fn confirm_discard(&mut self) -> bool {
        if self.session.project.is_some() || self.st.home.is_some() || !self.ed.is_dirty() {
            return true;
        }
        let r = rfd::MessageDialog::new()
            .set_title("Unsaved changes")
            .set_description(format!("Save changes to \"{}\" first?", self.ed.doc.name))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        match r {
            rfd::MessageDialogResult::Yes => self.save(false),
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    fn error(&mut self, msg: String) {
        log::error!("{msg}");
        self.ed.status = msg.clone();
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("PaperCraft")
            .set_description(msg)
            .show();
    }

    /// Save a scratch document to a legacy file (only used when no project is open).
    fn save(&mut self, _save_as: bool) -> bool {
        if self.session.project.is_some() {
            return self.save_project(true);
        }
        let Some(p) = rfd::FileDialog::new()
            .add_filter("PaperCraft document", &[file::EXTENSION])
            .set_file_name(format!("{}.{}", self.ed.doc.name, file::EXTENSION))
            .save_file()
        else {
            return false;
        };
        let path = with_ext(p, file::EXTENSION);
        match file::save(&path, &self.ed.doc, Some(&self.ed.camera)) {
            Ok(()) => {
                self.ed.mark_saved();
                self.ed.status = format!("Saved {}", path.display());
                self.ed.file_path = Some(path);
                true
            }
            Err(e) => {
                self.error(format!("Could not save: {e}"));
                false
            }
        }
    }

    /// A GPU handle for offscreen work (thumbnails, PNG export) on the window's device.
    fn offscreen(&self) -> Option<Headless> {
        let g = self.gfx.as_ref()?;
        Some(Headless {
            device: g.device.clone(),
            queue: g.queue.clone(),
            adapter_info: g.adapter_info.clone(),
        })
    }

    /// Save the open project; `full` also writes the camera and refreshes the thumbnail.
    fn save_project(&mut self, full: bool) -> bool {
        self.st.save_state = String::new();
        let r = if full {
            self.session.save_all(&mut self.ed)
        } else {
            self.session.save(&mut self.ed).map(|_| ())
        };
        if let Err(e) = r {
            self.st.save_state = "Not saved".into();
            self.session.defer();
            self.error(format!("Could not save project: {e}"));
            return false;
        }
        self.st.save_state = "Saved".into();
        if self.session.thumbnail_due(Instant::now(), full)
            && let Some(gpu) = self.offscreen()
            && let Err(e) = self.session.write_thumbnail(&gpu, &self.ed)
        {
            log::warn!("thumbnail: {e}");
        }
        true
    }

    /// Autosave when due. Called after every frame and on timer wake-ups.
    fn tick_autosave(&mut self) {
        if self.session.autosave_due(&self.ed, Instant::now()) {
            self.save_project(false);
        }
    }

    fn default_server(&self) -> String {
        self.session.server_hint()
    }

    /// Look at the shared projects' sync engines (about once a second).
    fn tick_sync(&mut self) {
        if self.session.sync_poll(&mut self.ed, Instant::now()) {
            if self.st.home.is_some() {
                self.show_home();
            }
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
        }
    }

    fn show_home(&mut self) {
        let mut hv = self.st.home.take().unwrap_or_default();
        hv.refresh(&self.session.store, crate::project::now_ms());
        hv.shared = self.session.shared_info();
        hv.default_server = self.default_server();
        hv.library = self.session.sync.library_active();
        self.st.home = Some(hv);
        self.st.project_name.clear();
        self.st.save_state.clear();
    }

    fn open_project(&mut self, dir: &std::path::Path) {
        if self.session.project.is_some() {
            self.save_project(true);
        }
        let counts = self.runs.counts(&self.ed);
        match self.session.open(&mut self.ed, dir) {
            Ok(()) => {
                self.runs.end_counted(crate::project::format::now_ms(), counts, None);
                self.st.home = None;
                self.st.project_name = self.session.project_name();
                self.st.project_dir = self.session.project.as_ref().map(|p| p.dir.clone());
                self.st.save_state.clear();
                self.ed.status.clear();
            }
            Err(e) => self.error(format!("Could not open {}: {e}", dir.display())),
        }
    }

    fn new_project(&mut self) {
        if self.session.project.is_some() {
            self.save_project(true);
        }
        let counts = self.runs.counts(&self.ed);
        match self.session.create(&mut self.ed, "Untitled") {
            Ok(()) => {
                self.runs.end_counted(crate::project::format::now_ms(), counts, None);
                self.st.home = None;
                self.st.project_name = self.session.project_name();
                self.st.project_dir = self.session.project.as_ref().map(|p| p.dir.clone());
                self.st.save_state.clear();
            }
            Err(e) => self.error(format!("Could not create a project: {e}")),
        }
    }

    fn go_home(&mut self) {
        if self.session.project.is_some() {
            self.save_project(true);
            let counts = self.runs.counts(&self.ed);
            self.runs.end_counted(crate::project::format::now_ms(), counts, None);
            if let Err(e) = self.session.close(&mut self.ed) {
                self.error(format!("Could not save project: {e}"));
            }
        } else if !self.confirm_discard() {
            return;
        }
        self.show_home();
    }

    /// Import a file into a new project and open it.
    fn import_dialog(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .add_filter(
                "Supported files",
                &[file::EXTENSION, file::LEGACY_EXTENSION, "excalidraw", "svg"],
            )
            .add_filter(
                "PaperCraft document",
                &[file::EXTENSION, file::LEGACY_EXTENSION],
            )
            .add_filter("Excalidraw scene", &["excalidraw"])
            .add_filter("SVG", &["svg"])
            .pick_file()
        else {
            return;
        };
        self.import_file(&p);
    }

    fn import_file(&mut self, p: &std::path::Path) {
        match crate::io::import::import_as_pages(p) {
            Ok((name, pages)) => match self.session.store.create_with_pages(&name, pages) {
                Ok(proj) => {
                    let dir = proj.dir.clone();
                    self.open_project(&dir);
                    self.ed.status = format!("Imported {}", p.display());
                }
                Err(e) => self.error(format!("Could not create a project: {e}")),
            },
            Err(e) => self.error(format!("Could not import {}: {e}", p.display())),
        }
    }

    fn handle_home_actions(&mut self) {
        let Some(hv) = &mut self.st.home else { return };
        let actions = std::mem::take(&mut hv.actions);
        if actions.is_empty() {
            return;
        }
        let store = self.session.store.clone();
        for a in actions {
            let r: Result<(), String> = match a {
                HomeAction::New => {
                    self.new_project();
                    Ok(())
                }
                HomeAction::Import => {
                    self.import_dialog();
                    Ok(())
                }
                HomeAction::Open(dir) => {
                    self.open_project(&dir);
                    Ok(())
                }
                HomeAction::Rename(dir, name) => store
                    .rename(&dir, &name)
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                HomeAction::Duplicate(dir) => {
                    store.duplicate(&dir).map(|_| ()).map_err(|e| e.to_string())
                }
                HomeAction::Trash(dir) => {
                    // A trashed project leaves the share (and a library); restoring it brings
                    // back a local copy.
                    self.session.sync.drop_project(&dir);
                    store.trash(&dir).map(|_| ()).map_err(|e| e.to_string())
                }
                HomeAction::Share(dir, server) => self.session.share_project(&dir, &server),
                HomeAction::Join(link) => self.session.join_shared(&link),
                HomeAction::Unshare(dir) => {
                    self.session.sync.unshare(&dir);
                    Ok(())
                }
                HomeAction::Restore(dir) => {
                    store.restore(&dir).map(|_| ()).map_err(|e| e.to_string())
                }
                HomeAction::DeleteForever(dir) => {
                    store.delete_forever(&dir).map_err(|e| e.to_string())
                }
                HomeAction::EmptyTrash => store.empty_trash().map_err(|e| e.to_string()),
                HomeAction::RevealRoot => {
                    let opener = if cfg!(windows) {
                        "explorer"
                    } else if cfg!(target_os = "macos") {
                        "open"
                    } else {
                        "xdg-open"
                    };
                    // explorer exits non-zero even on success, so only spawn failure counts.
                    std::process::Command::new(opener)
                        .arg(&store.root)
                        .spawn()
                        .map(|_| ())
                        .map_err(|e| format!("cannot open {}: {e}", store.root.display()))
                }
            };
            if let Err(e) = r {
                self.error(e);
            }
        }
        if self.st.home.is_some() {
            self.show_home();
        }
    }

    fn place_image_file(&mut self, p: &std::path::Path, at: V2) {
        match std::fs::read(p)
            .map_err(|e| e.to_string())
            .and_then(|b| crate::assets::register(&b))
        {
            Ok(id) => {
                self.ed.insert_image(&id, at);
                self.ed.status = format!("Placed {}", p.display());
            }
            Err(e) => self.error(format!("Could not place {}: {e}", p.display())),
        }
    }

    /// Export through a save (or folder) dialog.
    fn export(&mut self, spec: crate::io::export::Spec) {
        use crate::io::export;
        self.ed.finish_loading();
        let names: Vec<String> = self.ed.pages.iter().map(|p| p.name.clone()).collect();
        let pages: Vec<(&str, &crate::model::Document)> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), self.ed.page_doc(i)))
            .collect();
        let sel = self.ed.top_selection();
        let items = export::items(&spec, &pages, self.ed.active_page, &sel);
        if items.is_empty() {
            self.ed.status = "Nothing to export".into();
            return;
        }
        let ext = spec.format.ext();
        let out = if spec.single_file(items.len()) {
            let base = if items.len() == 1 {
                items[0].name.clone()
            } else {
                self.st.project_name.clone()
            };
            rfd::FileDialog::new()
                .add_filter(ext.to_uppercase(), &[ext])
                .set_file_name(format!("{}.{ext}", crate::project::folder_name(&base)))
                .save_file()
                .map(|p| with_ext(p, ext))
        } else {
            rfd::FileDialog::new()
                .set_title(format!(
                    "Choose a folder for {} {} files",
                    items.len(),
                    ext.to_uppercase()
                ))
                .pick_folder()
        };
        let Some(out) = out else { return };
        let gpu = self.offscreen();
        match export::write(&spec, &items, &out, gpu.as_ref()) {
            Ok(files) => {
                self.ed.status = if files.len() == 1 {
                    format!("Exported {}", files[0].display())
                } else {
                    format!("Exported {} files to {}", files.len(), out.display())
                }
            }
            Err(e) => self.error(format!("Export failed: {e}")),
        }
    }

    /// Bring a file into the current page: SVG, Excalidraw scene/library, image, or a
    /// papercraft page. Without an open project, a new project is created from it.
    fn import_into_page(&mut self, p: &std::path::Path, at: Option<V2>) {
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let at = at.unwrap_or_else(|| self.ed.camera.screen_to_world(self.ed.viewport * 0.5));
        let text = || std::fs::read_to_string(p).map_err(|e| e.to_string());
        let result: Result<String, String> = match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" => {
                self.place_image_file(p, at);
                return;
            }
            "excalidrawlib" => text()
                .and_then(|t| crate::io::excalidraw::import_library(&t))
                .map(|items| {
                    let n = items.len();
                    self.st.library.library.items.extend(items);
                    self.st.library.dirty = true;
                    self.st.show_library = true;
                    format!("Added {n} item(s) to the library")
                }),
            "svg" | "excalidraw" | "json" => text().and_then(|t| {
                let nodes: Vec<crate::model::Node> = if ext == "svg" {
                    crate::io::svg_import::parse(&t)?
                } else {
                    let doc = crate::io::excalidraw::import_scene(&t)?;
                    doc.paint_order()
                        .into_iter()
                        .filter_map(|id| doc.get(id).cloned())
                        .collect()
                };
                let (roots, world) = crate::io::clipboard::roots_of(&nodes);
                let off = crate::io::clipboard::center_offset(&world, at);
                let label = format!(
                    "Import {}",
                    p.file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default()
                );
                let ids = if ext == "svg" {
                    // Keep an SVG together as one group.
                    let name = p
                        .file_stem()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or("SVG".into());
                    let b = crate::io::clipboard::bounds(&world).unwrap_or_default();
                    vec![self.ed.edit(&label, |tx| {
                        crate::io::svg_import::insert_group(tx, nodes, &name, b.min() + off)
                    })]
                } else {
                    self.ed
                        .insert_subtrees(&label, &nodes, &roots, &world, off, false)
                };
                self.ed.selection = ids;
                Ok(format!("Imported {}", p.display()))
            }),
            _ => {
                if self.session.project.is_none() {
                    self.import_file(p);
                    return;
                }
                // A papercraft page or project: add its pages to this project.
                crate::io::import::import_as_pages(p).map(|(_, pages)| {
                    let n = pages.len();
                    for pg in pages {
                        let i = self.ed.add_page();
                        self.ed.rename_page(i, &pg.name);
                        let nodes: Vec<crate::model::Node> = pg
                            .doc
                            .paint_order()
                            .into_iter()
                            .filter_map(|id| pg.doc.get(id).cloned())
                            .collect();
                        let (roots, world) = crate::io::clipboard::roots_of(&nodes);
                        self.ed.insert_subtrees(
                            "Import page",
                            &nodes,
                            &roots,
                            &world,
                            V2::ZERO,
                            false,
                        );
                    }
                    format!("Imported {n} page(s)")
                })
            }
        };
        match result {
            Ok(msg) => self.ed.status = msg,
            Err(e) => self.error(format!("Could not import {}: {e}", p.display())),
        }
    }

    fn export_roots(&self) -> Option<Vec<crate::model::NodeId>> {
        let sel = self.ed.top_selection();
        (!sel.is_empty()).then_some(sel)
    }

    fn handle_requests(&mut self) {
        for req in std::mem::take(&mut self.ed.requests) {
            match req {
                AppRequest::Home => self.go_home(),
                AppRequest::ImageFill(ids, i) => {
                    let Some(p) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                        .pick_file()
                    else {
                        continue;
                    };
                    match std::fs::read(&p)
                        .map_err(|e| e.to_string())
                        .and_then(|b| crate::assets::register(&b))
                    {
                        Ok(asset) => {
                            ui::inspector::apply(&mut self.ed, "fills", &ids, |n| {
                                let paint = crate::model::Paint::Image {
                                    asset: asset.clone(),
                                    mode: crate::model::ImageMode::Fill,
                                    opacity: 1.0,
                                };
                                match n.style.fills.get_mut(i) {
                                    Some(slot) => *slot = paint,
                                    None => n.style.fills.push(paint),
                                }
                            });
                        }
                        Err(e) => self.error(format!("Could not load {}: {e}", p.display())),
                    }
                }
                AppRequest::PlaceImage => {
                    let Some(p) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                        .pick_file()
                    else {
                        continue;
                    };
                    let at = self.ed.camera.screen_to_world(self.ed.viewport * 0.5);
                    self.place_image_file(&p, at);
                }
                AppRequest::New => self.new_project(),
                AppRequest::Open => {
                    if self.session.project.is_some() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter(
                                "Supported files",
                                &[
                                    "svg",
                                    "excalidraw",
                                    "excalidrawlib",
                                    "json",
                                    "png",
                                    "jpg",
                                    "jpeg",
                                    "gif",
                                    "webp",
                                    file::EXTENSION,
                                    file::LEGACY_EXTENSION,
                                ],
                            )
                            .pick_file()
                        {
                            self.import_into_page(&p, None);
                        }
                    } else {
                        self.import_dialog();
                    }
                }
                AppRequest::Save => {
                    self.save(false);
                }
                AppRequest::SaveAs => {
                    if let Some(dir) = self.session.project.as_ref().map(|p| p.dir.clone()) {
                        self.save_project(true);
                        match self.session.store.duplicate(&dir) {
                            Ok(copy) => self.open_project(&copy),
                            Err(e) => self.error(format!("Could not duplicate: {e}")),
                        }
                    } else {
                        self.save(true);
                    }
                }
                AppRequest::ExportFormat(f) => {
                    let spec = crate::io::export::Spec {
                        format: f,
                        ..self.st.export
                    };
                    self.export(spec);
                }
                AppRequest::Export(spec) => self.export(spec),
                AppRequest::CopyAsPng => {
                    let roots = self.export_roots();
                    let Some(gpu) = self.offscreen() else {
                        continue;
                    };
                    match png::render(&gpu, &self.ed.doc, roots.as_deref(), 2.0, None) {
                        Ok(img) => {
                            if let Some(io) = &mut self.ed.clipboard_io {
                                io.write_image(&img);
                                self.ed.status =
                                    format!("Copied {}×{} PNG", img.width(), img.height());
                            }
                        }
                        Err(e) => self.ed.status = e,
                    }
                }
            }
        }
    }

    fn try_exit(&mut self, event_loop: &ActiveEventLoop) {
        if self.session.project.is_some() {
            self.save_project(true);
        }
        if self.confirm_discard() {
            event_loop.exit();
        }
    }

    fn handle_win_cmd(&mut self) {
        let Some(cmd) = self.st.win_cmd.take() else {
            return;
        };
        let Some(g) = self.gfx.as_ref() else { return };
        match cmd {
            ui::WinCmd::Drag => _ = g.window.drag_window(),
            ui::WinCmd::Minimize => g.window.set_minimized(true),
            ui::WinCmd::ToggleMaximize => g.window.set_maximized(!g.window.is_maximized()),
            ui::WinCmd::Close => self.close_requested = true,
        }
    }

    fn redraw(&mut self) {
        let now = self.now();
        self.ed.tick(now);
        if let Some((act, keys)) = self.st.pending_action.take() {
            let mut runs = std::mem::take(&mut self.runs);
            crate::ui::actions::run(self, &mut runs, act, &keys, crate::project::format::now_ms());
            self.runs = runs;
        }
        let dir = self
            .session
            .project
            .as_ref()
            .filter(|_| self.st.home.is_none());
        self.runs.tick(
            crate::project::format::now_ms(),
            dir.map(|p| p.dir.as_path()),
            &self.ed,
        );
        if self.st.chat.take_finish_run() {
            // A chat turn that edited is over: close its run now, not after the 30 s idle timeout.
            self.runs.finish(crate::project::format::now_ms(), &self.ed);
        }
        self.st.sync_runs(&mut self.runs, &self.ed);
        self.st.chat.tools = self.tool_dispatch().map(crate::chat::events::ToolDispatch);
        if self.st.chat.gpu.is_none() {
            self.st.chat.gpu = self.offscreen();
        }
        let Some(g) = self.gfx.as_mut() else { return };
        let work = Instant::now();
        let input = g.egui.take_egui_input(&g.window);
        let mut l = frame::layout(&self.ctx, input, &mut self.ed, &mut self.st);
        self.canvas = l.canvas;
        let repaint_delay = l
            .output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        let platform = std::mem::take(&mut l.output.platform_output);
        g.egui.handle_platform_output(&g.window, platform);

        let frame = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Occluded => {
                // macOS reports Occluded until the window is on screen (and while it is
                // hidden). Keep egui's texture state in sync, but don't spin:
                // WindowEvent::Occluded(false) triggers a redraw.
                log::debug!("surface occluded; skipping frame");
                g.painter
                    .skip_frame(&g.device, &g.queue, &mut l.output.textures_delta);
                return;
            }
            other => {
                log::warn!("surface unavailable: {other:?}");
                g.surface.configure(&g.device, &g.config);
                g.painter
                    .skip_frame(&g.device, &g.queue, &mut l.output.textures_delta);
                g.window.request_redraw();
                return;
            }
        };
        // AppKit re-lays out the titlebar (first show, fullscreen, resize) and puts the
        // traffic lights back; re-inset them (a no-op when already in place).
        #[cfg(target_os = "macos")]
        {
            crate::macos::hide_traffic_lights(&g.window);
        }
        let view = frame.texture.create_view(&Default::default());
        let size = [g.config.width, g.config.height];
        g.painter.paint(
            &g.device,
            &g.queue,
            &view,
            size,
            &self.ctx,
            l,
            &self.ed,
            !self.st.show_grid_off,
        );
        self.st.perf.record(self.start.elapsed().as_secs_f64(), work.elapsed().as_secs_f32() * 1000.0);
        g.window.pre_present_notify();
        g.queue.present(frame);

        let title = if self.st.home.is_some() {
            "PaperCraft".to_string()
        } else if self.session.project.is_some() {
            format!(
                "{} — {} — PaperCraft",
                self.st.project_name,
                self.ed.page_name()
            )
        } else {
            format!(
                "{}{} — PaperCraft",
                self.ed.doc.name,
                if self.ed.is_dirty() { " •" } else { "" }
            )
        };
        if title != self.title {
            g.window.set_title(&title);
            self.title = title;
        }
        let pending = !self.ed.requests.is_empty()
            || self.st.home.as_ref().is_some_and(|h| !h.actions.is_empty());
        self.handle_requests();
        self.handle_win_cmd();
        self.handle_home_actions();
        self.tick_autosave();
        if self.st.library.dirty {
            self.st.library.dirty = false;
            if let Err(e) = self.st.library.library.save() {
                self.error(format!("Could not save the library: {e}"));
            }
        }
        self.sync_menu();
        // Repaint only when egui wants it: immediately (an animation is running or state just
        // changed), after a delay (tooltip, caret blink), or never (idle; input wakes us).
        let Some(g) = &self.gfx else { return };
        if log::log_enabled!(log::Level::Trace) {
            log::trace!(
                "frame: repaint in {repaint_delay:?}, causes {:?}",
                self.ctx.repaint_causes()
            );
        }
        // The laser trail fades out frame by frame, then the loop goes idle again.
        let animating = self.ed.animating(now);
        // Pages are still being read in the background: look again soon to show them.
        let repaint_delay = if self.ed.loading() {
            repaint_delay.min(Duration::from_millis(30))
        } else {
            repaint_delay
        };
        if repaint_delay.is_zero() || pending || animating || !self.ed.requests.is_empty() {
            g.window.request_redraw();
            self.next_repaint = None;
        } else if repaint_delay < Duration::from_secs(3600) {
            self.next_repaint = Some(Instant::now() + repaint_delay);
        } else {
            self.next_repaint = None;
        }
    }

    /// Keep the native menu's check marks in step with the editor.
    fn sync_menu(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(m) = &self.menu {
            let set = |item: &muda::CheckMenuItem, v: bool| {
                if item.is_checked() != v {
                    item.set_checked(v);
                }
            };
            set(&m.snapping, self.ed.snapping);
            set(&m.grid, !self.st.show_grid_off);
            set(&m.help, self.st.show_help);
            for (c, item) in &m.themes {
                set(item, *c == theme::choice());
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn menu_command(&mut self, event_loop: &ActiveEventLoop, cmd: crate::macos::MenuCmd) {
        use crate::editor::shortcuts::Action as A;
        use crate::macos::MenuCmd;
        match cmd {
            MenuCmd::Quit => {
                if self.confirm_discard() {
                    event_loop.exit();
                }
            }
            MenuCmd::ToggleGrid => self.st.show_grid_off = !self.st.show_grid_off,
            MenuCmd::ToggleHelp => self.st.show_help = !self.st.show_help,
            MenuCmd::CheckUpdates => {
                self.st.updater.open = true;
                self.st.updater.check();
            }
            MenuCmd::Theme(c) => {
                theme::apply_choice(&self.ctx, c);
                c.save();
            }
            // Menu key equivalents never reach egui, so while a text field is focused, hand
            // the standard editing commands to it instead of the canvas.
            MenuCmd::Run(a @ (A::Undo | A::Redo | A::Cut | A::Copy | A::Paste | A::SelectAll))
                if self.ctx.egui_wants_keyboard_input() =>
            {
                let Some(g) = self.gfx.as_mut() else { return };
                let key = |key, shift: bool| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: if shift {
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
                    } else {
                        egui::Modifiers::COMMAND
                    },
                };
                let ev = match a {
                    A::Cut => egui::Event::Cut,
                    A::Copy => egui::Event::Copy,
                    // An image-only clipboard has no text: an empty paste still tells the chat
                    // composer to look for an image.
                    A::Paste => egui::Event::Paste(g.egui.clipboard_text().unwrap_or_default()),
                    A::Undo => key(egui::Key::Z, false),
                    A::Redo => key(egui::Key::Z, true),
                    _ => key(egui::Key::A, false),
                };
                g.egui.egui_input_mut().events.push(ev);
            }
            MenuCmd::Run(a) => self.ed.run(a),
        }
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

fn with_ext(p: PathBuf, ext: &str) -> PathBuf {
    if p.extension().is_some_and(|e| e == ext) {
        p
    } else {
        p.with_extension(ext)
    }
}

fn map_key(k: &WKey) -> Option<Key> {
    Some(match k {
        WKey::Named(NamedKey::Escape) => Key::Escape,
        WKey::Named(NamedKey::Enter) => Key::Enter,
        WKey::Named(NamedKey::Backspace) => Key::Backspace,
        WKey::Named(NamedKey::Delete) => Key::Delete,
        WKey::Named(NamedKey::Tab) => Key::Tab,
        WKey::Named(NamedKey::Space) => Key::Space,
        WKey::Named(NamedKey::ArrowLeft) => Key::Left,
        WKey::Named(NamedKey::ArrowRight) => Key::Right,
        WKey::Named(NamedKey::ArrowUp) => Key::Up,
        WKey::Named(NamedKey::ArrowDown) => Key::Down,
        WKey::Named(NamedKey::Home) => Key::Home,
        WKey::Named(NamedKey::End) => Key::End,
        WKey::Named(NamedKey::PageUp) => Key::PageUp,
        WKey::Named(NamedKey::PageDown) => Key::PageDown,
        WKey::Character(s) => Key::Char(s.chars().next()?),
        _ => return None,
    })
}

fn app_icon() -> Option<winit::window::Icon> {
    let png = include_bytes!("../assets/app-icon/papercraft-macos-512.png");
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let img = image::imageops::resize(&img, 256, 256, image::imageops::FilterType::Lanczos3);
    winit::window::Icon::from_rgba(img.into_raw(), 256, 256).ok()
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let icon = app_icon();
        let attrs = Window::default_attributes()
            .with_title("PaperCraft")
            .with_window_icon(icon.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(1440.0, 900.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(720.0, 480.0));
        // PAPERCRAFT_OPAQUE=1 turns off vibrancy (and window transparency) on macOS.
        #[cfg(target_os = "macos")]
        let translucent = std::env::var_os("PAPERCRAFT_OPAQUE").is_none();
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            // Unified, title-less toolbar look: content runs under a transparent titlebar.
            attrs
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true)
                .with_title_hidden(true)
                .with_transparent(translucent)
        };
        #[cfg(windows)]
        let attrs = {
            use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
            // No native frame, but keep the system shadow and Windows 11's rounded corners.
            attrs
                .with_decorations(false)
                .with_undecorated_shadow(true)
                .with_corner_preference(CornerPreference::Round)
                .with_taskbar_icon(icon)
        };
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        self.st.custom_titlebar = cfg!(windows);
        window.set_ime_allowed(true);
        #[cfg(target_os = "macos")]
        {
            self.st.translucent_sidebar = translucent && crate::macos::apply_vibrancy(&window);
            self.st.native_chrome = true;
            crate::macos::hide_traffic_lights(&window);
            self.menu = Some(crate::macos::install_menu());
        }
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no compatible GPU adapter");
        let adapter_info = adapter.get_info();
        log::info!("adapter: {adapter_info:?}");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("PaperCraft"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("create device");
        let caps = surface.get_capabilities(&adapter);
        log::info!("surface alpha modes: {:?}", caps.alpha_modes);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: pick_alpha_mode(&caps.alpha_modes, self.st.translucent_sidebar),
            view_formats: vec![],
            color_space: Default::default(),
        };
        surface.configure(&device, &config);
        let mut painter = Painter::new(&device, format);
        painter.transparent = self.st.translucent_sidebar;
        let egui = egui_winit::State::new(
            self.ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            // Start in the system appearance; ThemeChanged keeps it in sync afterwards.
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        window.request_redraw();
        self.st.perf.gpu = format!("{} ({:?})", adapter_info.name, adapter_info.backend);
        self.gfx = Some(Gfx {
            window,
            surface,
            device,
            queue,
            config,
            painter,
            egui,
            adapter_info,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(g) = self.gfx.as_mut() else { return };
        let resp = g.egui.on_window_event(&g.window, &event);
        if !matches!(event, WindowEvent::RedrawRequested) {
            log::trace!("event {event:?} -> egui repaint {}", resp.repaint);
        }
        let scale = g.window.scale_factor();
        let mut redraw = resp.repaint;
        let now = self.now();
        match event {
            WindowEvent::CloseRequested => self.try_exit(event_loop),
            WindowEvent::Resized(s) => {
                let g = self.gfx.as_mut().unwrap();
                g.config.width = s.width.max(1);
                g.config.height = s.height.max(1);
                g.surface.configure(&g.device, &g.config);
                #[cfg(target_os = "macos")]
                {
                    crate::macos::hide_traffic_lights(&g.window);
                }
                redraw = true;
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // Moved between Retina and non-Retina displays. egui-winit has already picked
                // up the new pixels-per-point; resize the surface to the new physical size
                // (winit follows with a Resized, but don't rely on it).
                let g = self.gfx.as_mut().unwrap();
                let s = g.window.inner_size();
                g.config.width = s.width.max(1);
                g.config.height = s.height.max(1);
                g.surface.configure(&g.device, &g.config);
                redraw = true;
            }
            WindowEvent::ThemeChanged(_) => {
                // Light/dark switch: egui-winit swaps the style; refresh the accent too.
                ui::setup_style(&self.ctx);
                redraw = true;
            }
            WindowEvent::Occluded(false) => redraw = true,
            WindowEvent::RedrawRequested => {
                // egui-winit reports `repaint: true` for RedrawRequested itself; honouring it
                // re-requested a redraw every frame (a 100% CPU busy loop). Whether another
                // frame is needed is decided inside redraw() from egui's repaint delay.
                self.redraw();
                return;
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.ed.handle(InputEvent::Modifiers(Modifiers {
                    shift: s.shift_key(),
                    ctrl: s.control_key(),
                    alt: s.alt_key(),
                    logo: s.super_key(),
                }));
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = v2(position.x / scale, position.y / scale);
                if self.captured || self.over_canvas(p) {
                    self.ed.handle(InputEvent::PointerMove {
                        pos: self.to_canvas(p),
                        pressure: None,
                        time_ms: now,
                    });
                    redraw = true;
                } else if self.ed.hover.take().is_some() {
                    redraw = true;
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => Button::Primary,
                    MouseButton::Middle => Button::Middle,
                    MouseButton::Right => Button::Secondary,
                    _ => return,
                };
                let p = self
                    .ctx
                    .input(|i| i.pointer.latest_pos())
                    .map(|p| v2(p.x as f64, p.y as f64));
                let Some(p) = p else { return };
                match state {
                    ElementState::Pressed if self.over_canvas(p) => {
                        // Clicking the canvas takes keyboard focus away from text fields.
                        self.ctx.memory_mut(|m| {
                            if let Some(f) = m.focused() {
                                m.surrender_focus(f)
                            }
                        });
                        self.captured = true;
                        self.ed.handle(InputEvent::PointerDown {
                            pos: self.to_canvas(p),
                            button: b,
                            pressure: None,
                            time_ms: now,
                        });
                    }
                    ElementState::Released if self.captured => {
                        self.captured = false;
                        self.ed.handle(InputEvent::PointerUp {
                            pos: self.to_canvas(p),
                            button: b,
                            time_ms: now,
                        });
                    }
                    _ => {}
                }
                redraw = true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let p = self
                    .ctx
                    .input(|i| i.pointer.latest_pos())
                    .map(|p| v2(p.x as f64, p.y as f64));
                if let Some(p) = p.filter(|p| self.over_canvas(*p)) {
                    let d = match delta {
                        MouseScrollDelta::LineDelta(x, y) => v2(x as f64 * 40.0, y as f64 * 40.0),
                        MouseScrollDelta::PixelDelta(pp) => v2(pp.x / scale, pp.y / scale),
                    };
                    let d = if self.ed.mods.shift && d.x == 0.0 {
                        v2(d.y, 0.0)
                    } else {
                        d
                    };
                    self.ed.handle(InputEvent::Scroll {
                        delta: -d,
                        pos: self.to_canvas(p),
                    });
                    redraw = true;
                }
            }
            WindowEvent::PinchGesture { delta, .. } => {
                let p = self
                    .ctx
                    .input(|i| i.pointer.latest_pos())
                    .map(|p| v2(p.x as f64, p.y as f64));
                if let Some(p) = p.filter(|p| self.over_canvas(*p)) {
                    // `delta` is the change in magnification; exp() keeps pinch in and out
                    // symmetric, like MKMapView / Preview.
                    self.ed.handle(InputEvent::Zoom {
                        factor: delta.exp(),
                        pos: self.to_canvas(p),
                    });
                    redraw = true;
                }
            }
            WindowEvent::DoubleTapGesture { .. } => {
                // Trackpad smart zoom (two-finger double tap): toggle fit / 100%.
                let a = if (self.ed.camera.zoom - 1.0).abs() < 1e-3 {
                    crate::editor::shortcuts::Action::ZoomFit
                } else {
                    crate::editor::shortcuts::Action::Zoom100
                };
                self.ed.run(a);
                redraw = true;
            }
            WindowEvent::Touch(t) => {
                // Pens and touch screens: forward pressure for ink.
                let p = v2(t.location.x / scale, t.location.y / scale);
                let pressure = t.force.map(|f| f.normalized());
                let pos = self.to_canvas(p);
                match t.phase {
                    TouchPhase::Started if self.over_canvas(p) => {
                        self.captured = true;
                        self.ed.handle(InputEvent::PointerDown {
                            pos,
                            button: Button::Primary,
                            pressure,
                            time_ms: now,
                        });
                    }
                    TouchPhase::Moved if self.captured => self.ed.handle(InputEvent::PointerMove {
                        pos,
                        pressure,
                        time_ms: now,
                    }),
                    TouchPhase::Ended | TouchPhase::Cancelled if self.captured => {
                        self.captured = false;
                        self.ed.handle(InputEvent::PointerUp {
                            pos,
                            button: Button::Primary,
                            time_ms: now,
                        });
                    }
                    _ => {}
                }
                redraw = true;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.ctx.egui_wants_keyboard_input() {
                    let command = self.ed.mods.command();
                    let g = self.gfx.as_mut().unwrap();
                    // egui-winit drops Cmd/Ctrl+V when the clipboard holds no text (an image):
                    // send an empty paste so the chat composer can pick the image up.
                    let input = g.egui.egui_input_mut();
                    if event.state == ElementState::Pressed
                        && command
                        && event.physical_key == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV)
                        && !input.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                    {
                        input.events.push(egui::Event::Paste(String::new()));
                    }
                    g.window.request_redraw();
                    return;
                }
                let physical = match &event.physical_key {
                    // Shortcuts should work regardless of the active keyboard layout's shift state.
                    winit::keyboard::PhysicalKey::Code(c) => physical_char(*c).map(Key::Char),
                    _ => None,
                };
                // Alt changes the logical character on macOS (⌥A types "å"), so chords with
                // Alt (and with ⌘/Ctrl) resolve by physical key instead.
                let chord = self.ed.mods.alt || self.ed.mods.command();
                let key = if chord && self.ed.text_edit.is_none() {
                    physical.clone().or_else(|| map_key(&event.logical_key))
                } else {
                    map_key(&event.logical_key).or(physical)
                };
                if let Some(key) = key {
                    match event.state {
                        ElementState::Pressed => {
                            self.ed.handle(InputEvent::KeyDown {
                                key,
                                repeat: event.repeat,
                            });
                            if let Some(t) = &event.text
                                && self.ed.text_edit.is_some()
                                && !self.ed.mods.command()
                            {
                                self.ed.handle(InputEvent::Text(t.to_string()));
                            }
                        }
                        ElementState::Released => self.ed.handle(InputEvent::KeyUp { key }),
                    }
                }
                redraw = true;
            }
            WindowEvent::Ime(winit::event::Ime::Commit(s)) => {
                if !self.ctx.egui_wants_keyboard_input() {
                    self.ed.handle(InputEvent::Text(s));
                }
                redraw = true;
            }
            WindowEvent::DroppedFile(p) => {
                let ext = p
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if matches!(
                    ext.as_str(),
                    "png"
                        | "jpg"
                        | "jpeg"
                        | "gif"
                        | "webp"
                        | "svg"
                        | "excalidraw"
                        | "excalidrawlib"
                ) && self.st.home.is_none()
                {
                    let at = self.ed.camera.screen_to_world(self.ed.pointer);
                    self.import_into_page(&p, Some(at));
                } else if p
                    .extension()
                    .is_some_and(|e| e == file::EXTENSION || e == file::LEGACY_EXTENSION)
                    && self.confirm_discard()
                {
                    match file::load(&p) {
                        Ok((doc, cam)) => self.ed.load(doc, cam, Some(p)),
                        Err(e) => self.error(format!("Could not open {}: {e}", p.display())),
                    }
                }
                redraw = true;
            }
            _ => {}
        }
        if redraw && let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            #[cfg(target_os = "macos")]
            UserEvent::Menu(id) => {
                if let Some(cmd) = self.menu.as_ref().and_then(|m| m.command(&id)) {
                    self.menu_command(_event_loop, cmd);
                }
            }
            UserEvent::Mcp => self.run_mcp_calls(),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if std::mem::take(&mut self.close_requested) {
            self.try_exit(event_loop);
        }
        // Autosave runs off a timer so it also fires while the window sits idle.
        if self
            .session
            .next_deadline()
            .is_some_and(|d| Instant::now() >= d)
        {
            self.tick_autosave();
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
        }
        self.tick_sync();
        let wake = [
            self.next_repaint,
            self.session.next_deadline(),
            self.session.sync_deadline(),
        ]
        .into_iter()
        .flatten()
        .min();
        match wake {
            Some(t) if Instant::now() >= t => {
                if self.next_repaint.is_some_and(|r| r <= t) {
                    self.next_repaint = None;
                }
                if let Some(g) = &self.gfx {
                    g.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Some(t) => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// A non-opaque mode when the window shows vibrancy through transparent pixels.
fn pick_alpha_mode(
    modes: &[wgpu::CompositeAlphaMode],
    translucent: bool,
) -> wgpu::CompositeAlphaMode {
    use wgpu::CompositeAlphaMode as A;
    if translucent {
        for m in [A::PreMultiplied, A::PostMultiplied, A::Inherit] {
            if modes.contains(&m) {
                return m;
            }
        }
        log::warn!("no transparent surface alpha mode in {modes:?}");
    }
    if modes.contains(&A::Opaque) {
        A::Opaque
    } else {
        modes[0]
    }
}

fn physical_char(c: winit::keyboard::KeyCode) -> Option<char> {
    use winit::keyboard::KeyCode::*;
    Some(match c {
        BracketLeft => '[',
        BracketRight => ']',
        Equal => '=',
        Minus => '-',
        Quote => '\'',
        Slash => '/',
        Backslash => '\\',
        Comma => ',',
        Period => '.',
        Digit0 => '0',
        Digit1 => '1',
        Digit2 => '2',
        Digit3 => '3',
        Digit4 => '4',
        Digit5 => '5',
        Digit6 => '6',
        Digit7 => '7',
        Digit8 => '8',
        Digit9 => '9',
        KeyA => 'a',
        KeyB => 'b',
        KeyC => 'c',
        KeyD => 'd',
        KeyE => 'e',
        KeyF => 'f',
        KeyG => 'g',
        KeyH => 'h',
        KeyI => 'i',
        KeyJ => 'j',
        KeyK => 'k',
        KeyL => 'l',
        KeyM => 'm',
        KeyN => 'n',
        KeyO => 'o',
        KeyP => 'p',
        KeyQ => 'q',
        KeyR => 'r',
        KeyS => 's',
        KeyT => 't',
        KeyU => 'u',
        KeyV => 'v',
        KeyW => 'w',
        KeyX => 'x',
        KeyY => 'y',
        KeyZ => 'z',
        _ => return None,
    })
}

impl App {
    /// Sends a tool call to the UI thread like an MCP request (runs, undo and the agent log
    /// behave the same); for the chat's API backend (`TurnConfig::tools`).
    pub fn tool_dispatch(&self) -> Option<crate::mcp::server::Dispatch> {
        self.mcp.as_ref().map(|(_, _, d)| d.clone())
    }

    /// Run the MCP tool calls that arrived since the last wake-up, then repaint.
    fn run_mcp_calls(&mut self) {
        let calls: Vec<crate::mcp::Call> = match &self.mcp {
            Some((rx, _, _)) => rx.try_iter().collect(),
            None => return,
        };
        if calls.is_empty() {
            return;
        }
        for c in calls {
            // Network and Codex work runs off the editor thread; it only reads the project folder.
            // The helper calls back into this app through papercraft-cli, so it must not hold the UI thread.
            if [crate::mcp::refs::TOOL, crate::mcp::imagegen::TOOL, crate::mcp::delegate::TOOL].contains(&c.name.as_str()) {
                let dir = self.session.project.as_ref().map(|p| p.dir.clone());
                let chat = (c.client.name == crate::mcp::CHAT_CLIENT).then(|| c.client.session.clone());
                let provider = self.st.settings.chat.image_provider();
                let helper = self.st.settings.chat.helper_command.clone();
                std::thread::spawn(move || {
                    let r = if c.name == crate::mcp::delegate::TOOL {
                        crate::mcp::delegate::run(&helper, chat.as_deref(), &c.args)
                    } else if c.name == crate::mcp::refs::TOOL {
                        crate::mcp::refs::fetch(dir.as_deref(), chat.as_deref(), &c.args)
                    } else {
                        crate::mcp::imagegen::generate(dir.as_deref(), chat.as_deref(), &provider, &c.args)
                    };
                    let _ = c.reply.send(crate::mcp::steer::attach(&c.client, r));
                });
                continue;
            }
            let mut runs = std::mem::take(&mut self.runs);
            let r = runs.call(self, &c.client, &c.name, &c.args, crate::project::format::now_ms());
            let touched = runs.take_touched();
            if r.is_ok() {
                let who = runs.label(&c.client);
                self.st.mark_agent(&self.ed, who, &c.name, &c.args, &touched);
            }
            self.runs = runs;
            self.st.log_agent(&c.name, r.as_ref().err().cloned());
            if let Err(e) = &r {
                log::info!("mcp {}: {e}", c.name);
            }
            let _ = c.reply.send(crate::mcp::steer::attach(&c.client, r));
        }
        self.st.project_name = self.session.project_name();
        self.st.project_dir = self.session.project.as_ref().map(|p| p.dir.clone());
        self.ed.revision += 1;
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

impl crate::mcp::exec::Host for App {
    fn editor(&mut self) -> &mut Editor {
        &mut self.ed
    }
    fn store(&self) -> &Store {
        &self.session.store
    }
    fn project(&self) -> Option<&crate::project::Project> {
        self.session
            .project
            .as_ref()
            .filter(|_| self.st.home.is_none())
    }
    fn canvas_open(&self) -> bool {
        self.st.home.is_none()
    }
    fn open_project(&mut self, dir: &std::path::Path) -> Result<(), String> {
        // No modal dialogs here: the agent gets the error instead.
        if self.session.project.is_some() {
            self.session
                .save_all(&mut self.ed)
                .map_err(|e| format!("could not save the open project: {e}"))?;
        }
        self.session
            .open(&mut self.ed, dir)
            .map_err(|e| format!("could not open {}: {e}", dir.display()))?;
        self.st.home = None;
        self.st.project_name = self.session.project_name();
        self.st.project_dir = self.session.project.as_ref().map(|p| p.dir.clone());
        self.st.save_state.clear();
        self.ed.status.clear();
        Ok(())
    }
    fn save(&mut self) -> Result<(), String> {
        if self.session.project.is_some() {
            self.session
                .save_all(&mut self.ed)
                .map_err(|e| e.to_string())?;
            self.st.save_state = "Saved".into();
        }
        Ok(())
    }
    fn rename_project(&mut self, name: &str) -> Result<std::path::PathBuf, String> {
        let dir = self.session.rename_open(&mut self.ed, name)?;
        self.st.project_name = self.session.project_name();
        Ok(dir)
    }
    fn gpu(&mut self) -> Option<Headless> {
        self.offscreen().or_else(|| Headless::new().ok())
    }
}

/// Start the MCP server unless `PAPERCRAFT_MCP=0`. Calls wake the event loop through `proxy`.
fn start_mcp(
    proxy: winit::event_loop::EventLoopProxy<UserEvent>,
) -> Option<(
    std::sync::mpsc::Receiver<crate::mcp::Call>,
    crate::mcp::server::Running,
    crate::mcp::server::Dispatch,
)> {
    if std::env::var("PAPERCRAFT_MCP").is_ok_and(|v| matches!(v.as_str(), "0" | "off" | "false")) {
        return None;
    }
    let (tx, rx) = std::sync::mpsc::channel::<crate::mcp::Call>();
    let tx = std::sync::Mutex::new(tx);
    let dispatch: crate::mcp::server::Dispatch = std::sync::Arc::new(move |call| {
        tx.lock().unwrap().send(call).is_ok() && proxy.send_event(UserEvent::Mcp).is_ok()
    });
    match crate::mcp::server::spawn(crate::mcp::port(), dispatch.clone()) {
        Ok(running) => {
            log::info!("MCP server listening on {}", running.url());
            Some((rx, running, dispatch))
        }
        Err(e) => {
            log::warn!("MCP server not started: {e}");
            None
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn prefer_x11(kernel_release: &str, display: Option<&std::ffi::OsStr>) -> bool {
    kernel_release.to_ascii_lowercase().contains("microsoft")
        && display.is_some_and(|value| !value.is_empty())
}

pub fn run(start: Start) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    crate::chat::path::init();
    let store = Store::open(Store::default_root())
        .map_err(|e| format!("cannot create the projects folder: {e}"))?;
    // Tells papercraft-cli the app is up (or starting), so it waits for the server instead of
    // writing the projects offline. Released by the OS when the process ends.
    let _running = crate::cli::hold_app_lock(&store.root);
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "linux")]
    {
        use winit::platform::x11::EventLoopBuilderExtX11;
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        // WSLg retains Wayland decoration shadows after resize (microsoft/wslg#914).
        if prefer_x11(&kernel, std::env::var_os("DISPLAY").as_deref()) {
            builder.with_x11();
        }
    }
    let event_loop = builder
        .build()
        .map_err(|e| format!("cannot open a window: {e}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    #[cfg(target_os = "macos")]
    {
        let proxy = event_loop.create_proxy();
        muda::MenuEvent::set_event_handler(Some(move |e: muda::MenuEvent| {
            let _ = proxy.send_event(UserEvent::Menu(e.id));
        }));
    }
    let mut app = App::new(start, store);
    app.mcp = start_mcp(event_loop.create_proxy());
    app.st.agent_endpoint = app.mcp.as_ref().map(|(_, r, _)| r.url()).unwrap_or_default();
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::prefer_x11;
    use std::ffi::OsStr;

    #[test]
    fn wsl_prefers_x11_when_a_display_is_available() {
        for kernel in ["6.6.87.2-microsoft-standard-WSL2", "4.4.0-19041-Microsoft"] {
            assert!(prefer_x11(kernel, Some(OsStr::new(":0"))));
        }
    }

    #[test]
    fn native_linux_keeps_default_backend_selection() {
        assert!(!prefer_x11("6.8.0-ubuntu", Some(OsStr::new(":0"))));
    }

    #[test]
    fn wsl_without_x11_keeps_default_backend_selection() {
        assert!(!prefer_x11("6.6.87.2-microsoft-standard-WSL2", None));
        assert!(!prefer_x11(
            "6.6.87.2-microsoft-standard-WSL2",
            Some(OsStr::new(""))
        ));
    }
}
