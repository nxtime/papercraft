//! Browser shell: a canvas with a WebGPU/WebGL surface, DOM events translated into egui input
//! and editor input. The native counterpart is `app.rs`. Files live in the in-memory/IndexedDB
//! `vfs`; there are no dialogs, so imports are file pickers / drops (`files.rs`), exports are
//! downloads, and an AI agent reaches the tab through the relay (`relay.rs`).
//!
//! GPU readback cannot block in a browser, so everything that needs pixels (agent screenshots,
//! PNG export, thumbnails, "copy as PNG") renders offscreen on the window's own device and awaits
//! `Headless::read_async`: the executor answers `get_screenshot` with a
//! `mcp::exec::NEEDS_RENDER` marker and the shell replies when the render finishes.

mod dom;
mod files;
mod relay;

use crate::editor::shortcuts::Action;
use crate::editor::{AppRequest, Button, Editor, InputEvent, Key, Modifiers};
use crate::geom::{V2, v2};
use crate::mcp::ToolResult;
use crate::mcp::exec::{self, Host, ShotRequest};
use crate::mcp::relay_proto::Reply;
use crate::project::{Project, Store};
use crate::render::frame::{self, Painter};
use crate::render::gpu::Headless;
use crate::session::Session;
use crate::theme;
use crate::ui::UiState;
use crate::ui::agent::{AgentAction, AgentUi};
use crate::ui::home::{HomeAction, HomeView};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, KeyboardEvent, PointerEvent, WheelEvent};

/// One DOM wheel notch in pixels (browsers report 3 lines ≈ 120px in pixel mode already).
/// Where the projects live inside the browser's virtual file system.
const WEB_ROOT: &str = "/papercraft";

const LINE_PX: f64 = 40.0;
/// Largest ctrl+wheel delta that still zooms smoothly (mouse notches report ±100).
const MAX_PINCH_DELTA: f64 = 30.0;
const PINCH_ZOOM_PER_PX: f64 = 0.01;
/// How long a status message stays in the corner, and an error toast on screen.
const STATUS_VISIBLE_MS: f64 = 6_000.0;
const ERROR_VISIBLE_MS: f64 = 8_000.0;

struct Web {
    canvas: HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    painter: Painter,
    ctx: egui::Context,
    ed: Editor,
    st: UiState,
    session: Session,
    events: Vec<egui::Event>,
    mods: Modifiers,
    pointer: Option<egui::Pos2>,
    captured: bool,
    canvas_rect: egui::Rect,
    start_ms: f64,
    /// Another frame is needed (input arrived, or egui asked for a repaint).
    dirty: bool,
    repaint_at: Option<f64>,
    adapter_info: wgpu::AdapterInfo,
    /// This shell, for work that finishes after the current event (readbacks, file reads).
    me: Weak<RefCell<Web>>,
    relay: relay::Relay,
    /// Groups the agent's editing calls into reviewable runs.
    runs: crate::runs::Tracker,
    /// What the editor's clipboard provider reads and writes (see `dom::Mirror`).
    mirror: dom::SharedMirror,
    toast_until: Option<f64>,
    /// The status text last seen and when it first appeared, to let it fade.
    status_seen: (String, f64),
}

type Shared = Rc<RefCell<Web>>;

fn window() -> web_sys::Window {
    web_sys::window().expect("no window")
}

fn now_ms() -> f64 {
    window().performance().map_or(0.0, |p| p.now())
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    // fastrand has no entropy source on wasm32-unknown-unknown; without this every session
    // would mint the same project ids.
    fastrand::seed(js_sys::Date::now() as u64 ^ (js_sys::Math::random() * u64::MAX as f64) as u64);
    let _ = console_log::init_with_level(log::Level::Info);
    let canvas = window()
        .document()
        .and_then(|d| d.get_element_by_id("papercraft"))
        .and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok())
        .expect("index.html needs <canvas id=\"papercraft\">");
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = crate::vfs::load().await {
            log::error!("papercraft web: cannot load saved projects: {e}");
        }
        match Web::new(canvas).await {
            Ok(web) => run(Rc::new(RefCell::new(web))),
            Err(e) => log::error!("papercraft web: {e}"),
        }
    });
}

impl Web {
    async fn new(canvas: HtmlCanvasElement) -> Result<Web, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| format!("create surface: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no compatible GPU adapter: {e}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("PaperCraft"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("create device: {e}"))?;
        let adapter_info = adapter.get_info();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: Default::default(),
        };
        let ctx = egui::Context::default();
        theme::apply_choice(&ctx, theme::Choice::load());
        let demo = window().location().hash().is_ok_and(|h| h == "#demo");
        let store = Store::open(WEB_ROOT).map_err(|e| format!("open store: {e}"))?;
        let mut st = UiState::default();
        if !demo {
            st.home = Some(HomeView::load(&store, crate::project::now_ms()));
        }
        st.library.library =
            crate::library::Library::load(&crate::library::Library::path_in(&store.root));
        let relay = relay::Relay::new();
        st.agent_link = Some(AgentUi {
            url: relay.agent_url(),
            ..Default::default()
        });
        let mirror = dom::SharedMirror::default();
        let mut ed = if demo {
            crate::demo::showcase()
        } else {
            Editor::default()
        };
        ed.clipboard_io = Some(Box::new(dom::WebClipboard(mirror.clone())));
        let mut session = Session::new(store);
        // A share link (`/?share=<token>`) opens the shared project; joining twice is a no-op.
        let loc = window().location();
        if let (Ok(search), Ok(origin)) = (loc.search(), loc.origin())
            && let Some(token) = search
                .trim_start_matches('?')
                .split('&')
                .find_map(|kv| kv.strip_prefix("share="))
            && let Err(e) = session.join_shared(&format!("{origin}/?share={token}"))
        {
            log::warn!("share link: {e}");
        }
        // A relay started with --library keeps every project for every device that opens it.
        if !demo && let Ok(origin) = loc.origin() {
            session.follow_library(&origin);
        }
        Ok(Web {
            painter: Painter::new(&device, format),
            canvas,
            surface,
            device,
            queue,
            config,
            ctx,
            ed,
            st,
            session,
            events: vec![],
            mods: Modifiers::NONE,
            pointer: None,
            captured: false,
            canvas_rect: egui::Rect::NOTHING,
            start_ms: now_ms(),
            dirty: true,
            repaint_at: None,
            adapter_info,
            me: Weak::new(),
            relay,
            runs: Default::default(),
            mirror,
            toast_until: None,
            status_seen: (String::new(), 0.0),
        })
    }

    fn ppp(&self) -> f32 {
        window().device_pixel_ratio() as f32
    }

    fn over_canvas(&self, p: egui::Pos2) -> bool {
        self.canvas_rect.contains(p) && !self.ctx.is_pointer_over_egui()
    }

    fn to_canvas(&self, p: egui::Pos2) -> V2 {
        v2(
            (p.x - self.canvas_rect.min.x) as f64,
            (p.y - self.canvas_rect.min.y) as f64,
        )
    }

    fn set_mods(&mut self, ctrl: bool, shift: bool, alt: bool, meta: bool) {
        // Cmd and Ctrl both mean "command" in the browser (`Modifiers::command` is Ctrl here).
        self.mods = Modifiers {
            shift,
            ctrl: ctrl || meta,
            alt,
            logo: false,
        };
        self.ed.handle(InputEvent::Modifiers(self.mods));
    }

    fn egui_mods(&self) -> egui::Modifiers {
        egui::Modifiers {
            alt: self.mods.alt,
            ctrl: self.mods.ctrl,
            shift: self.mods.shift,
            mac_cmd: false,
            command: self.mods.ctrl,
        }
    }

    fn pointer_event(&mut self, e: &PointerEvent, pressed: Option<bool>) {
        let p = egui::pos2(e.offset_x() as f32, e.offset_y() as f32);
        self.set_mods(e.ctrl_key(), e.shift_key(), e.alt_key(), e.meta_key());
        self.pointer = Some(p);
        self.dirty = true;
        let pressure = (e.pointer_type() == "pen").then(|| e.pressure() as f64);
        let t = now_ms() - self.start_ms;
        self.events.push(egui::Event::PointerMoved(p));
        let Some(down) = pressed else {
            if self.captured || self.over_canvas(p) {
                self.ed.handle(InputEvent::PointerMove {
                    pos: self.to_canvas(p),
                    pressure,
                    time_ms: t,
                });
            } else {
                self.ed.hover = None;
            }
            return;
        };
        let (button, egui_button) = match e.button() {
            0 => (Button::Primary, egui::PointerButton::Primary),
            1 => (Button::Middle, egui::PointerButton::Middle),
            2 => (Button::Secondary, egui::PointerButton::Secondary),
            _ => return,
        };
        self.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui_button,
            pressed: down,
            modifiers: self.egui_mods(),
        });
        if down && self.over_canvas(p) {
            // Clicking the canvas takes keyboard focus away from text fields.
            self.ctx.memory_mut(|m| {
                if let Some(f) = m.focused() {
                    m.surrender_focus(f)
                }
            });
            self.captured = true;
            let _ = self.canvas.set_pointer_capture(e.pointer_id());
            self.ed.handle(InputEvent::PointerDown {
                pos: self.to_canvas(p),
                button,
                pressure,
                time_ms: t,
            });
        } else if !down && self.captured {
            self.captured = false;
            self.ed.handle(InputEvent::PointerUp {
                pos: self.to_canvas(p),
                button,
                time_ms: t,
            });
        }
    }

    fn wheel_event(&mut self, e: &WheelEvent) {
        let p = egui::pos2(e.offset_x() as f32, e.offset_y() as f32);
        self.set_mods(e.ctrl_key(), e.shift_key(), e.alt_key(), e.meta_key());
        self.pointer = Some(p);
        self.dirty = true;
        let unit = if e.delta_mode() == 1 { LINE_PX } else { 1.0 };
        let (dx, dy) = (e.delta_x() * unit, e.delta_y() * unit);
        if !self.over_canvas(p) {
            self.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(-dx as f32, -dy as f32),
                phase: egui::TouchPhase::Move,
                modifiers: self.egui_mods(),
            });
            return;
        }
        let pos = self.to_canvas(p);
        if e.ctrl_key() || e.meta_key() {
            // Trackpad pinch arrives as ctrl+wheel with small deltas.
            let d = dy.clamp(-MAX_PINCH_DELTA, MAX_PINCH_DELTA);
            self.ed.handle(InputEvent::Zoom {
                factor: (-d * PINCH_ZOOM_PER_PX).exp(),
                pos,
            });
        } else {
            let d = if e.shift_key() && dx == 0.0 {
                v2(dy, 0.0)
            } else {
                v2(dx, dy)
            };
            self.ed.handle(InputEvent::Scroll { delta: d, pos });
        }
    }

    /// Returns true when the browser should not handle the key itself.
    fn key_event(&mut self, e: &KeyboardEvent, down: bool) -> bool {
        self.set_mods(e.ctrl_key(), e.shift_key(), e.alt_key(), e.meta_key());
        self.dirty = true;
        let name = e.key();
        let typing = name.chars().count() == 1 && !self.mods.ctrl && !self.mods.alt;
        let egui_key = egui::Key::from_name(if name == " " { "Space" } else { &name });
        if let Some(key) = egui_key {
            self.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: down,
                repeat: e.repeat(),
                modifiers: self.egui_mods(),
            });
        }
        if down && typing {
            self.events.push(egui::Event::Text(name.clone()));
        }
        // Cmd/Ctrl+V must reach the browser so it fires `paste` (the only way to read the
        // clipboard); copy and cut do too while a text field has focus.
        let code = e.code();
        let wants_text = self.ctx.egui_wants_keyboard_input();
        if self.mods.ctrl
            && !self.mods.alt
            && (code == "KeyV" || (wants_text && ["KeyC", "KeyX"].contains(&code.as_str())))
        {
            return false;
        }
        if wants_text {
            return true;
        }
        let chord = self.mods.alt || self.mods.ctrl;
        let key = if chord && self.ed.text_edit.is_none() {
            code_char(&e.code()).or_else(|| map_key(&name))
        } else {
            map_key(&name).or_else(|| code_char(&e.code()))
        };
        let Some(key) = key else { return false };
        if down {
            self.ed.handle(InputEvent::KeyDown {
                key,
                repeat: e.repeat(),
            });
            if typing && self.ed.text_edit.is_some() {
                self.ed.handle(InputEvent::Text(name));
            }
        } else {
            self.ed.handle(InputEvent::KeyUp { key });
        }
        true
    }

    /// Show a failure to the user: a toast (visible on the home screen too) and the status bar.
    fn error(&mut self, msg: String) {
        log::error!("{msg}");
        self.toast_until = Some(now_ms() + ERROR_VISIBLE_MS);
        self.st.toast = Some((msg.clone(), true));
        self.ed.status = msg;
        self.dirty = true;
    }

    /// The window's device as an offscreen renderer (screenshots, export, thumbnails).
    fn gpu_handle(&self) -> Headless {
        Headless {
            device: self.device.clone(),
            queue: self.queue.clone(),
            adapter_info: self.adapter_info.clone(),
        }
    }

    /// Run the tool calls the relay queued, on the main thread at the start of the frame.
    fn pump_calls(&mut self) {
        while let Some(call) = self.relay.next_call() {
            self.dirty = true;
            let mut runs = std::mem::take(&mut self.runs);
            let result = runs.call(
                self,
                &call.name,
                &call.args,
                crate::project::format::now_ms(),
            );
            self.runs = runs;
            self.st.project_name = self.session.project_name();
            self.st.project_dir = self.session.project.as_ref().map(|p| p.dir.clone());
            self.ed.revision += 1;
            let failed = result
                .as_ref()
                .err()
                .filter(|e| ShotRequest::from_marker(e).is_none())
                .cloned();
            self.st.log_agent(&call.name, failed);
            match result {
                Err(e) => match ShotRequest::from_marker(&e) {
                    Some(req) => self.start_screenshot(call.id, req),
                    None => self.relay.send(&Reply::err(call.id, e)),
                },
                Ok(out) => self.relay.send(&Reply::ok(call.id, out)),
            }
        }
    }

    /// Render a screenshot offscreen and answer call `id` when the readback lands.
    fn start_screenshot(&mut self, id: u64, req: ShotRequest) {
        let doc = self.ed.page_doc(req.page).clone();
        let gpu = self.gpu_handle();
        self.finish(
            async move { exec::screenshot_async(&gpu, &doc, &req).await },
            move |w, r: ToolResult| {
                let reply = match r {
                    Ok(out) => Reply::ok(id, out),
                    Err(e) => Reply::err(id, e),
                };
                w.relay.send(&reply);
            },
        );
    }

    fn save_project(&mut self, full: bool) {
        self.st.save_state = String::new();
        let r = if full {
            self.session.save_all(&mut self.ed)
        } else {
            self.session.save(&mut self.ed).map(|_| ())
        };
        match r {
            Ok(()) => self.st.save_state = "Saved".into(),
            Err(e) => {
                self.st.save_state = "Not saved".into();
                self.session.defer();
                self.error(format!("Could not save project: {e}"));
                return;
            }
        }
        if self.session.thumbnail_due(web_time::Instant::now(), full) {
            self.start_thumbnail();
        }
    }

    /// The server this page came from: where "Share…" puts a project unless told otherwise.
    fn default_server(&self) -> String {
        window().location().origin().unwrap_or_default()
    }

    /// Look at the shared projects' sync engines (about once a second).
    fn tick_sync(&mut self, now: f64) {
        if self
            .session
            .sync_poll(&mut self.ed, web_time::Instant::now())
        {
            if self.st.home.is_some() {
                self.show_home();
            }
            self.dirty = true;
        }
        if self.session.sync_deadline().is_some() {
            self.wake_at(now + 1000.0);
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

    fn enter_project(&mut self, r: Result<(), crate::project::format::FormatError>) {
        match r {
            Ok(()) => {
                self.st.home = None;
                self.st.project_name = self.session.project_name();
                self.st.save_state.clear();
                self.ed.status.clear();
            }
            Err(e) => self.error(format!("Could not open the project: {e}")),
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
        }
        self.show_home();
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
                    let counts = self.runs.counts(&self.ed);
                    let r = self.session.create(&mut self.ed, "Untitled");
                    if r.is_ok() {
                        self.runs.end_counted(crate::project::format::now_ms(), counts, None);
                    }
                    self.enter_project(r);
                    Ok(())
                }
                HomeAction::Open(dir) => {
                    let counts = self.runs.counts(&self.ed);
                    let r = self.session.open(&mut self.ed, &dir);
                    if r.is_ok() {
                        self.runs.end_counted(crate::project::format::now_ms(), counts, None);
                    }
                    self.enter_project(r);
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
                HomeAction::Import => {
                    self.request_import();
                    Ok(())
                }
                HomeAction::RevealRoot => {
                    self.download_backup();
                    Ok(())
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

    fn handle_requests(&mut self) {
        for req in std::mem::take(&mut self.ed.requests) {
            match req {
                AppRequest::Home => self.go_home(),
                AppRequest::ImageFill(ids, i) => self.request_image_fill(ids, i),
                AppRequest::PlaceImage => self.request_place_image(),
                AppRequest::New => {
                    if self.session.project.is_some() {
                        self.save_project(true);
                    }
                    let counts = self.runs.counts(&self.ed);
                    let r = self.session.create(&mut self.ed, "Untitled");
                    if r.is_ok() {
                        self.runs.end_counted(crate::project::format::now_ms(), counts, None);
                    }
                    self.enter_project(r);
                }
                AppRequest::Open => self.request_import(),
                AppRequest::Save => self.save_project(false),
                AppRequest::SaveAs => {
                    let Some(dir) = self.session.project.as_ref().map(|p| p.dir.clone()) else {
                        continue;
                    };
                    self.save_project(true);
                    match self.session.store.duplicate(&dir) {
                        Ok(copy) => {
                            let counts = self.runs.counts(&self.ed);
                            let r = self.session.open(&mut self.ed, &copy);
                            if r.is_ok() {
                                self.runs.end_counted(crate::project::format::now_ms(), counts, None);
                            }
                            self.enter_project(r);
                        }
                        Err(e) => self.error(format!("Could not duplicate: {e}")),
                    }
                }
                AppRequest::ExportFormat(f) => self.export_format(f),
                AppRequest::Export(spec) => self.export(spec),
                AppRequest::CopyAsPng => self.copy_as_png(),
            }
        }
    }

    /// Ask for a frame at `t` (ms) unless an earlier one is already planned.
    fn wake_at(&mut self, t: f64) {
        self.repaint_at = Some(self.repaint_at.map_or(t, |r| r.min(t)));
    }

    /// Fade out the toast and the status text, surface storage failures, and plan the wake-ups.
    fn tick_notices(&mut self, now: f64) {
        if let Some(e) = crate::vfs::take_persist_error() {
            self.st.save_state = "Not saved".into();
            self.error(e);
        }
        if self.toast_until.is_some_and(|t| now >= t) {
            self.toast_until = None;
            self.st.toast = None;
        }
        if self.ed.status != self.status_seen.0 {
            self.status_seen = (self.ed.status.clone(), now);
        }
        if !self.ed.status.is_empty() && now - self.status_seen.1 >= STATUS_VISIBLE_MS {
            self.ed.status.clear();
            self.status_seen.0.clear();
        }
        if let Some(t) = self.toast_until {
            self.wake_at(t);
        }
        if !self.ed.status.is_empty() {
            self.wake_at(self.status_seen.1 + STATUS_VISIBLE_MS);
        }
    }

    fn handle_agent_actions(&mut self, now: f64) {
        let Some(a) = &mut self.st.agent_link else {
            return;
        };
        let state = self.relay.state();
        if a.state != state {
            a.state = state;
            self.dirty = true;
        }
        for act in std::mem::take(&mut a.actions) {
            match act {
                AgentAction::Regenerate => {
                    self.relay.regenerate(now);
                    a.url = self.relay.agent_url();
                    self.dirty = true;
                }
            }
        }
    }

    fn frame(&mut self) {
        let now = now_ms();
        self.pump_calls();
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
        self.dirty |= self.st.sync_runs(&mut self.runs);
        if self.relay.tick(now) {
            self.dirty = true;
        }
        let due = self.repaint_at.is_some_and(|t| now >= t);
        if !self.dirty && !due && !crate::vfs::has_persist_error() {
            return;
        }
        self.dirty = false;
        self.repaint_at = None;
        self.tick_notices(now);
        let ppp = self.ppp();
        let w = window();
        let css = egui::vec2(
            w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
            w.inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0) as f32,
        );
        let px = [
            (css.x * ppp).round().max(1.0) as u32,
            (css.y * ppp).round().max(1.0) as u32,
        ];
        if [self.config.width, self.config.height] != px {
            self.canvas.set_width(px[0]);
            self.canvas.set_height(px[1]);
            self.config.width = px[0];
            self.config.height = px[1];
            self.surface.configure(&self.device, &self.config);
        }
        self.ctx.set_pixels_per_point(ppp);
        self.ed.tick(now - self.start_ms);
        let dark = w
            .match_media("(prefers-color-scheme: dark)")
            .ok()
            .flatten()
            .is_some_and(|m| m.matches());
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, css)),
            time: Some((now - self.start_ms) / 1000.0),
            events: std::mem::take(&mut self.events),
            system_theme: Some(if dark {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            }),
            ..Default::default()
        };
        self.st.project_dir = self.session.project.as_ref().map(|p| p.dir.clone());
        self.st.agent_endpoint = self.relay.agent_url();
        // The library probe answers after the home screen first shows; keep its controls in step.
        let library = self.session.sync.library_active();
        let server = self.default_server();
        if let Some(h) = &mut self.st.home {
            h.library = library;
            h.default_server = server;
        }
        let mut l = frame::layout(&self.ctx, input, &mut self.ed, &mut self.st);
        self.canvas_rect = l.canvas;
        let busy = !self.ed.requests.is_empty()
            || self.st.home.as_ref().is_some_and(|h| !h.actions.is_empty());
        self.handle_requests();
        self.handle_home_actions();
        self.handle_agent_actions(now);
        self.tick_sync(now);
        if self.st.library.dirty {
            self.st.library.dirty = false;
            if let Err(e) = self.st.library.library.save() {
                self.error(format!("Could not save the library: {e}"));
            }
        }
        if self
            .session
            .autosave_due(&self.ed, web_time::Instant::now())
        {
            self.save_project(false);
        }
        if busy || self.session.next_deadline().is_some() {
            self.dirty = true;
        }
        let delay = l
            .output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        if delay.is_zero() {
            self.dirty = true;
        } else if delay != std::time::Duration::MAX {
            self.wake_at(now + delay.as_secs_f64() * 1000.0);
        }
        if let Some(text) = l
            .output
            .platform_output
            .commands
            .iter()
            .find_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
        {
            let _ = window().navigator().clipboard().write_text(&text);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                self.painter
                    .skip_frame(&self.device, &self.queue, &mut l.output.textures_delta);
                self.surface.configure(&self.device, &self.config);
                self.dirty = true;
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        self.painter.paint(
            &self.device,
            &self.queue,
            &view,
            px,
            &self.ctx,
            l,
            &self.ed,
            !self.st.show_grid_off,
        );
        self.queue.present(frame);
    }
}

/// Mirrors `impl Host for App` (`app.rs`): the same executor drives both shells.
impl Host for Web {
    fn editor(&mut self) -> &mut Editor {
        &mut self.ed
    }
    fn store(&self) -> &Store {
        &self.session.store
    }
    fn project(&self) -> Option<&Project> {
        self.session
            .project
            .as_ref()
            .filter(|_| self.st.home.is_none())
    }
    fn canvas_open(&self) -> bool {
        self.st.home.is_none()
    }
    fn open_project(&mut self, dir: &std::path::Path) -> Result<(), String> {
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
    /// Pixels are read back asynchronously here: `get_screenshot` defers to the shell.
    fn gpu(&mut self) -> Option<Headless> {
        None
    }
    fn defer_pixels(&self) -> bool {
        true
    }
}

impl Web {
    /// A DOM `paste`: into an egui text field, the canvas text editor, or as layers / an image.
    fn paste_event(&mut self, text: String, files: Option<web_sys::FileList>) {
        self.dirty = true;
        if self.ctx.egui_wants_keyboard_input() {
            self.events.push(egui::Event::Paste(text));
            return;
        }
        if self.st.home.is_some() {
            return;
        }
        if self.ed.text_edit.is_some() && !text.is_empty() {
            self.ed.handle(InputEvent::Text(text));
            return;
        }
        if let Some(files) = files.filter(|f| f.length() > 0) {
            self.finish(
                async move { dom::read_list(&files).await },
                |w, r| match r {
                    Ok(files) => {
                        let image = files
                            .iter()
                            .find_map(|f| image::load_from_memory(&f.bytes).ok());
                        let Some(image) = image else {
                            return w.import_files(files, None);
                        };
                        {
                            let mut m = w.mirror.borrow_mut();
                            m.text = None;
                            m.image = Some(image.to_rgba8());
                        }
                        w.ed.run(Action::Paste);
                    }
                    Err(e) => w.error(e),
                },
            );
            return;
        }
        if !text.is_empty() {
            let mut m = self.mirror.borrow_mut();
            m.text = Some(text);
            m.image = None;
        }
        self.ed.run(Action::Paste);
    }

    /// A DOM `copy` / `cut` that the keydown handler did not already take (browser menu, or
    /// focus in an egui text field).
    fn copy_event(&mut self, e: &web_sys::ClipboardEvent, cut: bool) {
        self.dirty = true;
        e.prevent_default();
        if self.ctx.egui_wants_keyboard_input() {
            self.events.push(if cut {
                egui::Event::Cut
            } else {
                egui::Event::Copy
            });
            return;
        }
        if self.st.home.is_some() {
            return;
        }
        self.ed.run(if cut { Action::Cut } else { Action::Copy });
        let text = self.mirror.borrow().text.clone();
        if let (Some(text), Some(data)) = (text, e.clipboard_data()) {
            let _ = data.set_data("text/plain", &text);
        }
    }
}

fn run(web: Shared) {
    let canvas = web.borrow().canvas.clone();
    let weak = Rc::downgrade(&web);
    web.borrow_mut().me = weak.clone();
    // A hidden tab gets no animation frames, but an agent may still be driving it.
    web.borrow().relay.set_wake(Rc::new(move || {
        if !window().document().is_some_and(|d| d.hidden()) {
            return;
        }
        if let Some(w) = weak.upgrade()
            && let Ok(mut w) = w.try_borrow_mut()
        {
            w.pump_calls();
        }
    }));
    let on = |name: &str, f: Box<dyn FnMut(web_sys::Event)>| {
        let cb = Closure::wrap(f);
        let _ = canvas.add_event_listener_with_callback(name, cb.as_ref().unchecked_ref());
        cb.forget();
    };
    for (name, pressed) in [
        ("pointerdown", Some(true)),
        ("pointerup", Some(false)),
        ("pointermove", None),
    ] {
        let w = web.clone();
        on(
            name,
            Box::new(move |e| {
                if let Ok(e) = e.dyn_into::<PointerEvent>() {
                    w.borrow_mut().pointer_event(&e, pressed);
                }
            }),
        );
    }
    let w = web.clone();
    on(
        "wheel",
        Box::new(move |e| {
            e.prevent_default();
            if let Ok(e) = e.dyn_into::<WheelEvent>() {
                w.borrow_mut().wheel_event(&e);
            }
        }),
    );
    on("contextmenu", Box::new(|e| e.prevent_default()));
    // Keys and paste go to the window; the canvas never holds DOM focus.
    let doc = window();
    for (name, down) in [("keydown", true), ("keyup", false)] {
        let w = web.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            if let Ok(e) = e.dyn_into::<KeyboardEvent>()
                && w.borrow_mut().key_event(&e, down)
            {
                e.prevent_default();
            }
        });
        let _ = doc.add_event_listener_with_callback(name, cb.as_ref().unchecked_ref());
        cb.forget();
    }
    let w = web.clone();
    let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
        let Ok(e) = e.dyn_into::<web_sys::ClipboardEvent>() else {
            return;
        };
        e.prevent_default();
        let data = e.clipboard_data();
        let text = data
            .as_ref()
            .and_then(|d| d.get_data("text/plain").ok())
            .unwrap_or_default();
        let files = data.and_then(|d| d.files());
        w.borrow_mut().paste_event(text, files);
    });
    let _ = doc.add_event_listener_with_callback("paste", cb.as_ref().unchecked_ref());
    cb.forget();
    for (name, cut) in [("copy", false), ("cut", true)] {
        let w = web.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            if let Ok(e) = e.dyn_into::<web_sys::ClipboardEvent>() {
                w.borrow_mut().copy_event(&e, cut);
            }
        });
        let _ = doc.add_event_listener_with_callback(name, cb.as_ref().unchecked_ref());
        cb.forget();
    }
    on("dragover", Box::new(|e| e.prevent_default()));
    let w = web.clone();
    on(
        "drop",
        Box::new(move |e| {
            e.prevent_default();
            let Ok(e) = e.dyn_into::<web_sys::DragEvent>() else {
                return;
            };
            let Some(files) = e.data_transfer().and_then(|d| d.files()) else {
                return;
            };
            if files.length() == 0 {
                return;
            }
            let at = egui::pos2(e.offset_x() as f32, e.offset_y() as f32);
            w.borrow().finish(
                async move { dom::read_list(&files).await },
                move |w, r| match r {
                    Ok(files) => w.import_files(files, Some(at)),
                    Err(e) => w.error(e),
                },
            );
        }),
    );
    let w = web.clone();
    let cb = Closure::<dyn FnMut()>::new(move || {
        let mut w = w.borrow_mut();
        if w.session.project.is_some() {
            w.save_project(true);
        }
    });
    let _ = doc.add_event_listener_with_callback("pagehide", cb.as_ref().unchecked_ref());
    cb.forget();
    raf(web);
}

fn raf(web: Shared) {
    let slot: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let again = slot.clone();
    *slot.borrow_mut() = Some(Closure::new(move || {
        web.borrow_mut().frame();
        if let Some(cb) = again.borrow().as_ref() {
            let _ = window().request_animation_frame(cb.as_ref().unchecked_ref());
        }
    }));
    if let Some(cb) = slot.borrow().as_ref() {
        let _ = window().request_animation_frame(cb.as_ref().unchecked_ref());
    }
}

fn map_key(name: &str) -> Option<Key> {
    Some(match name {
        "Escape" => Key::Escape,
        "Enter" => Key::Enter,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Tab" => Key::Tab,
        " " => Key::Space,
        "ArrowLeft" => Key::Left,
        "ArrowRight" => Key::Right,
        "ArrowUp" => Key::Up,
        "ArrowDown" => Key::Down,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        n if n.chars().count() == 1 => Key::Char(n.chars().next()?),
        _ => return None,
    })
}

/// Layout-independent character for a `KeyboardEvent.code` (shortcuts ignore shift state).
fn code_char(code: &str) -> Option<Key> {
    let c = if let Some(l) = code.strip_prefix("Key") {
        l.to_ascii_lowercase().chars().next()?
    } else if let Some(d) = code.strip_prefix("Digit") {
        d.chars().next()?
    } else {
        match code {
            "BracketLeft" => '[',
            "BracketRight" => ']',
            "Equal" => '=',
            "Minus" => '-',
            "Quote" => '\'',
            "Slash" => '/',
            "Backslash" => '\\',
            "Comma" => ',',
            "Period" => '.',
            _ => return None,
        }
    };
    Some(Key::Char(c))
}
