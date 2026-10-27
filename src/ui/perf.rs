//! Performance monitor: frame times from the shell, process RAM and CPU sampled once a second,
//! the GPU adapter, an optional overlay on the canvas and a flag when frames get too slow.

use super::kit;
use crate::theme::{self, Icon, Surface as _};
use egui::{RichText, Ui};
use std::collections::VecDeque;

/// Frame-rate floors to choose from in Settings.
pub const THRESHOLDS: [u32; 5] = [30, 45, 60, 90, 120];
const DEFAULT_MIN_FPS: u32 = 60;
/// Frames kept for the averages (about two seconds of steady animation).
const WINDOW: usize = 120;
/// Frames that must run slow before the flag shows, so one hitch doesn't raise it.
const SLOW_STREAK: usize = 20;
const SAMPLE_SECS: f64 = 1.0;
const MB: f64 = 1024.0 * 1024.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PerfPrefs {
    pub overlay: bool,
    pub min_fps: u32,
}

impl Default for PerfPrefs {
    fn default() -> Self {
        PerfPrefs { overlay: false, min_fps: DEFAULT_MIN_FPS }
    }
}

impl PerfPrefs {
    fn path() -> Option<std::path::PathBuf> {
        Some(dirs::config_dir()?.join("papercraft").join("performance"))
    }

    /// `"<overlay 0|1> <min fps>"`.
    pub fn load() -> PerfPrefs {
        let s = Self::path().and_then(|p| crate::vfs::read_to_string(p).ok()).unwrap_or_default();
        let mut it = s.split_whitespace();
        let d = PerfPrefs::default();
        PerfPrefs {
            overlay: it.next().map_or(d.overlay, |v| v == "1"),
            min_fps: it.next().and_then(|v| v.parse().ok()).unwrap_or(d.min_fps),
        }
    }

    pub fn save(self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = crate::vfs::create_dir_all(dir);
        }
        let _ = crate::vfs::write(p, format!("{} {}", u8::from(self.overlay), self.min_fps));
    }
}

#[derive(Default)]
pub struct Perf {
    pub prefs: PerfPrefs,
    /// GPU adapter, "name (backend)"; set by the shell.
    pub gpu: String,
    /// (time in seconds, CPU time of the frame in ms), oldest first.
    frames: VecDeque<(f64, f32)>,
    ram: Option<f64>,
    cpu: Option<f32>,
    last_sample: Option<(f64, f64)>,
}

impl Perf {
    /// One rendered frame that took `ms` of CPU time, at `now` seconds.
    pub fn record(&mut self, now: f64, ms: f32) {
        if self.frames.len() == WINDOW {
            self.frames.pop_front();
        }
        self.frames.push_back((now, ms));
        if self.last_sample.is_some_and(|(t, _)| now - t < SAMPLE_SECS) {
            return;
        }
        self.ram = rss_bytes().map(|b| b as f64 / MB);
        let cpu_now = cpu_seconds();
        if let (Some((t, c)), Some(c2)) = (self.last_sample, cpu_now) {
            self.cpu = Some(((c2 - c) / (now - t) * 100.0) as f32);
        }
        self.last_sample = cpu_now.map(|c| (now, c));
    }

    /// Frames shown in the last second.
    pub fn fps(&self) -> usize {
        let Some(&(last, _)) = self.frames.back() else { return 0 };
        self.frames.iter().filter(|(t, _)| last - t < 1.0).count()
    }

    /// Average and worst frame time over the window, in ms.
    pub fn frame_ms(&self) -> (f32, f32) {
        if self.frames.is_empty() {
            return (0.0, 0.0);
        }
        let sum: f32 = self.frames.iter().map(|f| f.1).sum();
        (sum / self.frames.len() as f32, self.frames.iter().map(|f| f.1).fold(0.0, f32::max))
    }

    /// Frames are slower than the floor allows: the last [`SLOW_STREAK`] frames all took
    /// longer than one frame at `min_fps`. Measured on work, not on the frame rate, since the
    /// app draws nothing while idle.
    pub fn is_slow(&self) -> bool {
        let budget = 1000.0 / self.prefs.min_fps as f32;
        self.frames.len() >= SLOW_STREAK && self.frames.iter().rev().take(SLOW_STREAK).all(|f| f.1 > budget)
    }

    /// The frame rate the current work allows (1000 / average frame time).
    fn capacity(&self) -> f32 {
        let (avg, _) = self.frame_ms();
        if avg > 0.0 { 1000.0 / avg } else { 0.0 }
    }

    /// The readout: one row per metric.
    pub fn stats(&self, ui: &mut Ui) {
        let p = theme::pal(ui);
        let (avg, worst) = self.frame_ms();
        let budget = 1000.0 / self.prefs.min_fps as f32;
        let warn = |bad: bool| if bad { p.danger } else { p.text };
        let na = || "n/a".to_string();
        let rows = [
            ("FPS", format!("{} drawn \u{b7} {:.0} possible", self.fps(), self.capacity()), warn(self.is_slow())),
            ("Frame", format!("{avg:.1} ms avg \u{b7} {worst:.1} ms worst"), warn(avg > budget)),
            ("RAM", self.ram.map_or_else(na, |m| format!("{m:.0} MB")), p.text),
            ("CPU", self.cpu.map_or_else(na, |c| format!("{c:.0}%")), p.text),
            ("GPU", if self.gpu.is_empty() { na() } else { self.gpu.clone() }, p.text),
        ];
        egui::Grid::new("perf-stats").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for (label, value, ink) in rows {
                ui.label(RichText::new(label).size(theme::SECONDARY).color(p.tertiary));
                ui.label(RichText::new(value).size(theme::SECONDARY).color(ink).monospace());
                ui.end_row();
            }
        });
    }
}

/// Settings > Performance.
pub fn settings_page(ui: &mut Ui, perf: &mut Perf) {
    let before = perf.prefs;
    kit::setting_row(ui, "Show overlay", "FPS, frame time, RAM, CPU and GPU over the canvas.", |ui| {
        kit::toggle(ui, &mut perf.prefs.overlay);
    });
    ui.add_space(12.0);
    kit::section_header(ui, "Warn below", None);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 8.0);
        for fps in THRESHOLDS {
            if kit::chip(ui, &format!("{fps} FPS"), perf.prefs.min_fps == fps).clicked() {
                perf.prefs.min_fps = fps;
            }
        }
    });
    ui.label(RichText::new("A badge shows on the canvas while frames take longer than this rate allows.").size(theme::SECONDARY).color(theme::pal(ui).tertiary));
    ui.add_space(16.0);
    kit::section_header(ui, "Now", None);
    ui.add_space(4.0);
    perf.stats(ui);
    // Keep the readout live while the page is open.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(SAMPLE_SECS));
    if perf.prefs != before {
        perf.prefs.save();
    }
}

/// The overlay in the canvas's top-right corner, or, with the overlay off, a warning badge
/// while frames are slow (a click shows the overlay).
pub fn overlay(ctx: &egui::Context, perf: &mut Perf, canvas: egui::Rect) {
    const INSET: f32 = 16.0;
    let slow = perf.is_slow();
    if !perf.prefs.overlay && !slow {
        return;
    }
    if perf.prefs.overlay {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAMPLE_SECS));
    }
    let mut open = false;
    egui::Area::new("perf-overlay".into())
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(canvas.right_top() + egui::vec2(-INSET, INSET))
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            let frame = egui::Frame::popup(ui.style()).fill(p.popover).corner_radius(theme::r(12.0)).inner_margin(egui::Margin::symmetric(12, 8));
            if !perf.prefs.overlay {
                let r = frame
                    .stroke(egui::Stroke::new(1.0, p.danger))
                    .show_surface(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            theme::paint_icon(ui.painter(), r, Icon::Zap, p.danger);
                            ui.label(RichText::new(format!("Slow: {:.0} FPS, below {}", perf.capacity(), perf.prefs.min_fps)).size(theme::SECONDARY).color(p.danger));
                        });
                    })
                    .response
                    .interact(egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Show the performance overlay");
                open = r.clicked();
                return;
            }
            frame.stroke(egui::Stroke::new(1.0, if slow { p.danger } else { p.hover })).show_surface(ui, |ui| perf.stats(ui));
        });
    if open {
        perf.prefs.overlay = true;
        perf.prefs.save();
    }
}

/// Resident memory of this process, in bytes: the footprint Activity Monitor shows on macOS.
#[cfg(target_os = "macos")]
fn rss_bytes() -> Option<u64> {
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a rusage_info_v2, the flavor asked for.
    let ok = unsafe { libc::proc_pid_rusage(libc::getpid(), libc::RUSAGE_INFO_V2, (&raw mut info).cast()) } == 0;
    ok.then_some(info.ri_phys_footprint)
}

#[cfg(target_os = "linux")]
fn rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: sysconf has no preconditions.
    Some(pages * unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn rss_bytes() -> Option<u64> {
    None
}

/// User plus system CPU time this process has used, in seconds.
#[cfg(all(unix, not(target_arch = "wasm32")))]
fn cpu_seconds() -> Option<f64> {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `u` is a valid rusage to fill.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) } != 0 {
        return None;
    }
    let secs = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    Some(secs(u.ru_utime) + secs(u.ru_stime))
}

#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
fn cpu_seconds() -> Option<f64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_only_after_a_streak_over_budget() {
        let mut p = Perf::default();
        for i in 0..SLOW_STREAK - 1 {
            p.record(i as f64 * 0.03, 30.0);
        }
        assert!(!p.is_slow(), "too few frames");
        p.record(1.0, 30.0);
        assert!(p.is_slow(), "30 ms frames are under 60 FPS");
        p.record(1.01, 5.0);
        assert!(!p.is_slow(), "one fast frame clears it");
        p.prefs.min_fps = 30;
        for i in 0..SLOW_STREAK {
            p.record(2.0 + i as f64 * 0.03, 30.0);
        }
        assert!(!p.is_slow(), "30 ms fits a 30 FPS floor");
    }

    #[test]
    fn samples_memory() {
        let mut p = Perf::default();
        p.record(0.0, 1.0);
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(p.ram.is_some_and(|m| m > 1.0));
    }
}
