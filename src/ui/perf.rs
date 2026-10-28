//! Performance monitor: frame times from the shell, process RAM and CPU sampled once a second,
//! the GPU adapter, an optional overlay on the canvas and a flag when frames get too slow.

use super::kit;
use crate::theme::{self, Icon, Surface as _};
use egui::{RichText, Ui};
use std::collections::VecDeque;

/// Frame-rate caps to choose from in Settings, up to the fastest gaming monitors.
pub const THRESHOLDS: [u32; 9] = [30, 45, 60, 90, 120, 144, 165, 240, 360];
/// The stored rate that means "follow the display": no cap, frames paced by vsync.
pub const AUTO: u32 = 0;
/// 60 Hz, except on macOS where many screens run 120 Hz and the display decides.
const DEFAULT_RATE: u32 = if cfg!(target_os = "macos") { AUTO } else { 60 };
/// What "slow" is measured against when the rate is Auto.
const AUTO_JUDGE_FPS: u32 = 60;
/// A frame this much early still draws: timers wake late and vsync snaps the rest.
const CAP_SLACK: std::time::Duration = std::time::Duration::from_millis(1);
/// Frames kept for the averages (about two seconds of steady animation).
const WINDOW: usize = 120;
/// Frames that must run slow before the flag shows, so one hitch doesn't raise it.
const SLOW_STREAK: usize = 20;
const SAMPLE_SECS: f64 = 1.0;
const MB: f64 = 1024.0 * 1024.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PerfPrefs {
    pub overlay: bool,
    /// Most frames drawn per second, [`AUTO`] (0) for the display's own rate. Slow frames
    /// are judged against it too.
    pub min_fps: u32,
}

impl Default for PerfPrefs {
    fn default() -> Self {
        PerfPrefs { overlay: false, min_fps: DEFAULT_RATE }
    }
}

impl PerfPrefs {
    /// The rate frames are judged against: the cap, or 60 when there is none.
    pub fn judge_fps(self) -> u32 {
        if self.min_fps == AUTO { AUTO_JUDGE_FPS } else { self.min_fps }
    }

    /// The shortest time between two frames, or `None` to draw as fast as vsync allows.
    pub fn frame_interval(self) -> Option<std::time::Duration> {
        (self.min_fps != AUTO).then(|| std::time::Duration::from_secs_f64(1.0 / f64::from(self.min_fps)))
    }

    fn path() -> Option<std::path::PathBuf> {
        Some(dirs::config_dir()?.join("papercraft").join("performance"))
    }

    /// `"<overlay 0|1> <min fps>"`.
    pub fn load() -> PerfPrefs {
        let s = Self::path().and_then(|p| crate::vfs::read_to_string(p).ok()).unwrap_or_default();
        Self::parse(&s)
    }

    fn parse(s: &str) -> PerfPrefs {
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
            self.cpu = Some(cpu_percent(c2 - c, now - t, cpu_share()));
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
        let budget = 1000.0 / self.prefs.judge_fps() as f32;
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
        let budget = 1000.0 / self.prefs.judge_fps() as f32;
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
    kit::section_header(ui, "Refresh rate", None);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 8.0);
        if kit::chip(ui, "Auto", perf.prefs.min_fps == AUTO).on_hover_text("Follow the display: no cap").clicked() {
            perf.prefs.min_fps = AUTO;
        }
        for fps in THRESHOLDS {
            if kit::chip(ui, &format!("{fps} FPS"), perf.prefs.min_fps == fps).clicked() {
                perf.prefs.min_fps = fps;
            }
        }
    });
    ui.label(RichText::new("The most frames PaperCraft draws per second, so a fast monitor does not spend power on frames you cannot use. Auto follows the display. A badge shows on the canvas while frames take longer than this rate allows.").size(theme::SECONDARY).color(theme::pal(ui).tertiary));
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
                            ui.label(RichText::new(format!("Slow: {:.0} FPS, below {}", perf.capacity(), perf.prefs.judge_fps())).size(theme::SECONDARY).color(p.danger));
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

/// How long to wait before the next frame may be drawn, or `None` to draw now: the frame cap
/// `interval` against the time since the last frame started.
pub fn cap_wait(interval: Option<std::time::Duration>, since_last: std::time::Duration) -> Option<std::time::Duration> {
    let interval = interval?;
    (since_last < interval.saturating_sub(CAP_SLACK)).then(|| interval - since_last)
}

/// CPU use in percent: `cpu_secs` of processor time over `wall_secs`, scaled by `share`.
fn cpu_percent(cpu_secs: f64, wall_secs: f64, share: f64) -> f32 {
    if wall_secs <= 0.0 {
        return 0.0;
    }
    (cpu_secs / wall_secs * share * 100.0) as f32
}

/// What one busy core counts for. Task Manager shows the share of the whole CPU (a thread
/// pegging one core of 16 reads 6%); Activity Monitor and top count per core (100% per core).
fn cpu_share() -> f64 {
    if cfg!(windows) {
        std::thread::available_parallelism().map_or(1.0, |n| 1.0 / n.get() as f64)
    } else {
        1.0
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

/// The private working set: the "Memory" column of Task Manager's Processes tab. The plain
/// working set also counts pages shared with other processes (DLLs), which reads higher.
#[cfg(windows)]
fn rss_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut info: PROCESS_MEMORY_COUNTERS_EX2 = unsafe { std::mem::zeroed() };
    let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
    // SAFETY: the current-process pseudo handle is always valid; `cb` is the size of the
    // struct behind the pointer, whose first fields are a PROCESS_MEMORY_COUNTERS.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut info).cast::<PROCESS_MEMORY_COUNTERS>(), cb) } != 0;
    if ok {
        return Some(info.PrivateWorkingSetSize as u64);
    }
    // Before Windows 10 1809 the larger struct is refused: the plain working set is close.
    let mut basic: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    basic.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: as above, with the basic struct and its own size.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut basic, basic.cb) } != 0;
    ok.then_some(basic.WorkingSetSize as u64)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
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

#[cfg(windows)]
fn cpu_seconds() -> Option<f64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the current-process pseudo handle is always valid; the four out-pointers are live.
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut creation, &mut exit, &mut kernel, &mut user) } != 0;
    // FILETIME counts 100 ns ticks.
    let secs = |t: FILETIME| ((u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)) as f64 / 1e7;
    ok.then(|| secs(kernel) + secs(user))
}

#[cfg(not(any(all(unix, not(target_arch = "wasm32")), windows)))]
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
    fn refresh_rate_defaults_to_60_and_offers_high_rate_monitors() {
        assert_eq!(PerfPrefs::default().min_fps, DEFAULT_RATE);
        if !cfg!(target_os = "macos") {
            assert_eq!(DEFAULT_RATE, 60);
        }
        assert_eq!(PerfPrefs::parse("").min_fps, DEFAULT_RATE, "nothing saved yet");
        assert!(THRESHOLDS.contains(&60) && THRESHOLDS.contains(&360));
        assert!(THRESHOLDS.windows(2).all(|w| w[0] < w[1]), "ascending, so the chips read in order");
    }

    #[test]
    fn saved_prefs_round_trip_the_chosen_rate() {
        let p = PerfPrefs { overlay: true, min_fps: 360 };
        let saved = format!("{} {}", u8::from(p.overlay), p.min_fps);
        assert!(PerfPrefs::parse(&saved) == p);
        assert_eq!(PerfPrefs::parse("1 garbage").min_fps, DEFAULT_RATE, "an unreadable rate falls back to the default");
        assert_eq!(PerfPrefs::parse("0 0").min_fps, AUTO, "Auto is a saved choice, not a missing one");
    }

    #[test]
    fn auto_has_no_cap_and_is_judged_at_60() {
        let auto = PerfPrefs { overlay: false, min_fps: AUTO };
        assert_eq!(auto.frame_interval(), None);
        assert_eq!(auto.judge_fps(), 60);
        let p = PerfPrefs { overlay: false, min_fps: 240 };
        assert_eq!(p.judge_fps(), 240);
        let ms = p.frame_interval().unwrap().as_secs_f64() * 1000.0;
        assert!((ms - 1000.0 / 240.0).abs() < 1e-6, "{ms}");
    }

    #[test]
    fn cap_waits_out_the_rest_of_the_interval() {
        use std::time::Duration as D;
        let sixty = Some(D::from_micros(16_667));
        assert_eq!(cap_wait(None, D::ZERO), None, "no cap, draw now");
        assert_eq!(cap_wait(sixty, D::from_millis(20)), None, "a late frame draws at once");
        assert_eq!(cap_wait(sixty, D::from_micros(16_000)), None, "within the slack of the beat");
        assert_eq!(cap_wait(sixty, D::from_millis(5)), Some(D::from_micros(11_667)), "too soon: wait for the remainder");
        assert_eq!(cap_wait(sixty, D::ZERO), Some(D::from_micros(16_667)));
    }

    #[test]
    fn cpu_percent_is_scaled_to_the_share_one_core_counts_for() {
        assert_eq!(cpu_percent(0.5, 1.0, 1.0), 50.0, "half a core, per-core accounting");
        assert_eq!(cpu_percent(1.0, 1.0, 0.0625), 6.25, "one core busy of 16 reads 6.25%");
        assert_eq!(cpu_percent(1.0, 0.0, 1.0), 0.0, "no elapsed time yields 0, not infinity");
    }

    #[test]
    fn samples_memory() {
        let mut p = Perf::default();
        p.record(0.0, 1.0);
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
        assert!(p.ram.is_some_and(|m| m > 1.0));
    }

    #[test]
    fn cpu_seconds_grow_with_work() {
        let Some(before) = cpu_seconds() else { return };
        let mut x = 0u64;
        while cpu_seconds().is_some_and(|now| now - before < 0.05) {
            x = std::hint::black_box(x.wrapping_add(1));
        }
        assert!(cpu_seconds().is_some_and(|now| now > before));
    }
}
