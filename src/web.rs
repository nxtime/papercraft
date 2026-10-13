//! Browser shell: a canvas with a WebGPU/WebGL surface, DOM events translated into egui input
//! and editor input. The native counterpart is `app.rs`; this one has no project store, no
//! dialogs and no MCP server yet, it edits one scratch document.

use crate::editor::{Button, Editor, InputEvent, Key, Modifiers};
use crate::geom::{V2, v2};
use crate::render::frame::{self, Painter};
use crate::theme;
use crate::ui::UiState;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, KeyboardEvent, PointerEvent, WheelEvent};

/// One DOM wheel notch in pixels (browsers report 3 lines ≈ 120px in pixel mode already).
const LINE_PX: f64 = 40.0;
/// Largest ctrl+wheel delta that still zooms smoothly (mouse notches report ±100).
const MAX_PINCH_DELTA: f64 = 30.0;
const PINCH_ZOOM_PER_PX: f64 = 0.01;

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
    events: Vec<egui::Event>,
    mods: Modifiers,
    pointer: Option<egui::Pos2>,
    captured: bool,
    canvas_rect: egui::Rect,
    start_ms: f64,
    /// Another frame is needed (input arrived, or egui asked for a repaint).
    dirty: bool,
    repaint_at: Option<f64>,
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
    let _ = console_log::init_with_level(log::Level::Info);
    let canvas = window()
        .document()
        .and_then(|d| d.get_element_by_id("papercraft"))
        .and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok())
        .expect("index.html needs <canvas id=\"papercraft\">");
    wasm_bindgen_futures::spawn_local(async move {
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
        Ok(Web {
            painter: Painter::new(&device, format),
            canvas,
            surface,
            device,
            queue,
            config,
            ctx,
            ed: if demo {
                crate::demo::showcase()
            } else {
                Editor::default()
            },
            st: UiState::default(),
            events: vec![],
            mods: Modifiers::NONE,
            pointer: None,
            captured: false,
            canvas_rect: egui::Rect::NOTHING,
            start_ms: now_ms(),
            dirty: true,
            repaint_at: None,
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
        if self.ctx.egui_wants_keyboard_input() {
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

    fn frame(&mut self) {
        let now = now_ms();
        let due = self.repaint_at.is_some_and(|t| now >= t);
        if !self.dirty && !due {
            return;
        }
        self.dirty = false;
        self.repaint_at = None;
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
        let mut l = frame::layout(&self.ctx, input, &mut self.ed, &mut self.st);
        self.canvas_rect = l.canvas;
        // Requests (dialogs, files, clipboard) have no web implementation yet.
        self.ed.requests.clear();
        let delay = l
            .output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        if delay.is_zero() {
            self.dirty = true;
        } else if delay != std::time::Duration::MAX {
            self.repaint_at = Some(now + delay.as_secs_f64() * 1000.0);
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

fn run(web: Shared) {
    let canvas = web.borrow().canvas.clone();
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
        if let Ok(e) = e.dyn_into::<web_sys::ClipboardEvent>()
            && let Some(text) = e
                .clipboard_data()
                .and_then(|d| d.get_data("text/plain").ok())
        {
            let mut w = w.borrow_mut();
            w.events.push(egui::Event::Paste(text));
            w.dirty = true;
        }
    });
    let _ = doc.add_event_listener_with_callback("paste", cb.as_ref().unchecked_ref());
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
