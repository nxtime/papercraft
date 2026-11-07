//! Presence for shared projects: where the other devices' cursors and selections are.
//!
//! Ephemeral by design: it lives in memory on the relay and in [`Link`] on each device, and is
//! never written to a document, a project file or the undo history. Each device `PUT`s its own
//! [`Report`] to `/p/{token}/presence` and gets the other devices' reports back in the same
//! round trip, so it needs no new socket and works the same native and in the browser.
//!
//! Compatibility: an old relay answers 404 (the device then stops asking, see [`Exchange`]); an
//! old device never sends a report, so it is simply not shown. Unknown JSON fields are ignored on
//! both sides, so the report can grow.

use crate::sync::net;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use web_time::Instant;

/// Longest id, label and page id accepted off the network.
const MAX_ID: usize = 64;
const MAX_LABEL: usize = 40;
const MAX_PAGE: usize = 64;
/// Most selected layers one report carries.
const MAX_SELECTION: usize = 64;
/// Largest report body the relay reads.
pub const MAX_BODY: usize = 8 << 10;
/// World coordinates beyond this are garbage, not a cursor.
const MAX_COORD: f32 = 1.0e7;
/// Devices shown per project, and projects the relay tracks at once.
const MAX_DEVICES: usize = 32;
const MAX_ROOMS: usize = 1024;
/// A report closer than this to the previous one from the same device is dropped.
const MIN_GAP: Duration = Duration::from_millis(100);
/// A device that has not reported for this long has left.
const TTL: Duration = Duration::from_secs(20);
/// How often an active device reports, and an idle one.
const ACTIVE_TICK: Duration = Duration::from_millis(700);
const IDLE_TICK: Duration = Duration::from_secs(4);
/// Idle seconds before a device counts as idle (reports slow down), fades, and is gone.
const IDLE_AFTER: u32 = 3;
pub const FADE_START: u32 = 5;
pub const GONE_AFTER: u32 = 60;

/// One device's presence, as sent and as received.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    #[serde(default)]
    pub label: String,
    /// Page id the device is looking at.
    #[serde(default)]
    pub page: String,
    /// Cursor in world coordinates, `None` while the pointer is off the canvas.
    #[serde(default)]
    pub cursor: Option<[f32; 2]>,
    /// Selected layer ids on `page`.
    #[serde(default)]
    pub sel: Vec<u64>,
    /// Seconds since the device last moved or selected anything.
    #[serde(default)]
    pub idle: u32,
    /// The device is closing the project; drop it now instead of waiting for [`TTL`].
    #[serde(default)]
    pub leave: bool,
}

/// The relay's answer: everyone else on the project.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Peers {
    #[serde(default)]
    pub peers: Vec<Report>,
}

fn clip(s: &str, max: usize) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(max).collect()
}

impl Report {
    /// The report with every field forced into bounds, or `None` when its id is unusable.
    pub fn sanitized(self) -> Option<Report> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= MAX_ID
            && self.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !id_ok {
            return None;
        }
        let cursor = self.cursor.filter(|c| c.iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD));
        let mut sel = self.sel;
        sel.truncate(MAX_SELECTION);
        Some(Report {
            id: self.id,
            label: clip(&self.label, MAX_LABEL),
            page: clip(&self.page, MAX_PAGE),
            cursor,
            sel,
            idle: self.idle.min(GONE_AFTER),
            leave: self.leave,
        })
    }
}

/// A stable hue (degrees) for a device, so every screen paints it the same colour.
pub fn hue(id: &str) -> f32 {
    let h = id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    (h % 360) as f32
}

struct Seen {
    report: Report,
    at: Instant,
}

/// The relay's memory of who is on which project.
#[derive(Default)]
pub struct Book {
    rooms: HashMap<String, HashMap<String, Seen>>,
}

impl Book {
    /// Records `report` for project `token` (when it is not too soon after the last one) and
    /// returns the other devices there, their idle time aged by how long ago each reported.
    pub fn exchange(&mut self, token: &str, report: Report, now: Instant) -> Vec<Report> {
        if self.rooms.len() >= MAX_ROOMS && !self.rooms.contains_key(token) {
            self.rooms.retain(|_, r| {
                r.retain(|_, s| now.duration_since(s.at) < TTL);
                !r.is_empty()
            });
            if self.rooms.len() >= MAX_ROOMS {
                return Vec::new();
            }
        }
        let room = self.rooms.entry(token.to_string()).or_default();
        room.retain(|_, s| now.duration_since(s.at) < TTL);
        let me = report.id.clone();
        if report.leave {
            room.remove(&me);
        } else if let Some(prev) = room.get(&me) {
            if now.duration_since(prev.at) >= MIN_GAP {
                room.insert(me.clone(), Seen { report, at: now });
            }
        } else if room.len() < MAX_DEVICES {
            room.insert(me.clone(), Seen { report, at: now });
        }
        let mut out: Vec<Report> = room
            .iter()
            .filter(|(id, _)| **id != me)
            .map(|(_, s)| {
                let aged = now.duration_since(s.at).as_secs() as u32;
                Report { idle: s.report.idle.saturating_add(aged).min(GONE_AFTER), ..s.report.clone() }
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }
}

#[derive(Default)]
struct State {
    page: String,
    cursor: Option<[f32; 2]>,
    sel: Vec<u64>,
    active: Option<Instant>,
    peers: Vec<(Report, Instant)>,
    left: bool,
}

/// This device's side: what it shows of itself and what it last heard of the others.
pub struct Link {
    id: String,
    label: String,
    /// The relay did not understand a report (an older relay): stop asking.
    unsupported: AtomicBool,
    state: Mutex<State>,
}

impl Default for Link {
    fn default() -> Link {
        Link::new(crate::sync::new_token(), device_label())
    }
}

impl Link {
    pub fn new(id: String, label: String) -> Link {
        Link { id, label, unsupported: AtomicBool::new(false), state: Mutex::default() }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What this device is doing now; called per frame. Any change counts as activity.
    pub fn set_local(&self, page: &str, cursor: Option<[f32; 2]>, sel: &[u64]) {
        let mut s = self.state();
        if s.page != page || s.cursor != cursor || s.sel != sel || s.active.is_none() {
            s.page = page.to_string();
            s.cursor = cursor;
            s.sel = sel.to_vec();
            s.active = Some(Instant::now());
            s.left = false;
        }
    }

    /// The other devices to draw, with their idle time brought up to date; the ones gone idle
    /// for good are left out.
    pub fn peers(&self) -> Vec<Report> {
        let now = Instant::now();
        self.state()
            .peers
            .iter()
            .map(|(r, at)| Report { idle: r.idle.saturating_add(now.duration_since(*at).as_secs() as u32), ..r.clone() })
            .filter(|r| r.idle < GONE_AFTER)
            .collect()
    }

    pub fn is_supported(&self) -> bool {
        !self.unsupported.load(Ordering::Relaxed)
    }

    /// The report to send, or `None` when there is nothing to say (never active, or idle for so
    /// long that this device already left).
    fn report(&self) -> Option<Report> {
        let mut s = self.state();
        let idle = s.active?.elapsed().as_secs() as u32;
        let leave = idle >= GONE_AFTER;
        if leave && s.left {
            return None;
        }
        s.left = leave;
        Some(Report {
            id: self.id.clone(),
            label: self.label.clone(),
            page: s.page.clone(),
            cursor: s.cursor,
            sel: s.sel.clone(),
            idle,
            leave,
        })
    }

    fn is_active(&self) -> bool {
        self.state().active.is_some_and(|a| a.elapsed().as_secs() < u64::from(IDLE_AFTER))
    }
}

/// What one round trip to the relay came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Exchange {
    Done,
    /// Nothing to send this time.
    Quiet,
    /// The relay is too old for presence (or something else answered); do not ask again.
    Unsupported,
    Failed(String),
}

/// One report out and the peers back, against the project at `base` (`{server}/p/{token}`).
pub async fn exchange(base: &str, link: &Link) -> Exchange {
    let Some(report) = link.report() else {
        return Exchange::Quiet;
    };
    let body = match serde_json::to_vec(&report) {
        Ok(b) => b,
        Err(e) => return Exchange::Failed(e.to_string()),
    };
    let (code, bytes) = match net::request("PUT", &format!("{base}/presence"), Some(body)).await {
        Ok(r) => r,
        Err(e) => return Exchange::Failed(e),
    };
    // 404/405 is an old relay; 5xx and the like may pass, so only a clean "no" is final.
    let peers = match (code, serde_json::from_slice::<Peers>(&bytes)) {
        (200, Ok(p)) => p.peers,
        (200 | 404 | 405 | 501, _) => {
            link.unsupported.store(true, Ordering::Relaxed);
            return Exchange::Unsupported;
        }
        (c, _) => return Exchange::Failed(format!("presence answered {c}")),
    };
    let now = Instant::now();
    let mut s = link.state();
    s.peers = peers
        .into_iter()
        .filter_map(Report::sanitized)
        .filter(|r| r.id != link.id && !r.leave)
        .take(MAX_DEVICES)
        .map(|r| (r, now))
        .collect();
    Exchange::Done
}

/// Reports every tick until `stop` is set or the relay turns out not to support presence.
pub async fn run(base: String, link: Arc<Link>, stop: impl Fn() -> bool) {
    while !stop() && link.is_supported() {
        let wait = if link.is_active() { ACTIVE_TICK } else { IDLE_TICK };
        if let Exchange::Failed(e) = exchange(&base, &link).await {
            log::debug!("presence: {e}");
        }
        net::sleep(wait).await;
    }
}

/// Name other devices see: the machine's name, or the kind of device.
pub fn device_label() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let name = ["COMPUTERNAME", "HOSTNAME"].iter().find_map(|k| std::env::var(k).ok()).filter(|n| !n.trim().is_empty());
        clip(&name.unwrap_or_else(|| "Desktop".into()), MAX_LABEL)
    }
    #[cfg(target_arch = "wasm32")]
    {
        "Browser".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(id: &str) -> Report {
        Report { id: id.into(), label: id.into(), page: "p".into(), ..Report::default() }
    }

    #[test]
    fn a_device_sees_the_others_but_not_itself() {
        let (mut book, t0) = (Book::default(), Instant::now());
        assert!(book.exchange("tok", rep("a"), t0).is_empty());
        let seen = book.exchange("tok", rep("b"), t0);
        assert_eq!(seen.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["a"]);
        assert!(book.exchange("other", rep("c"), t0).is_empty(), "projects are separate");
    }

    #[test]
    fn a_silent_device_leaves_and_a_leave_frame_removes_it_at_once() {
        let (mut book, t0) = (Book::default(), Instant::now());
        book.exchange("t", rep("a"), t0);
        book.exchange("t", rep("b"), t0);
        assert_eq!(book.exchange("t", rep("b"), t0 + Duration::from_secs(1)).len(), 1);
        assert!(book.exchange("t", rep("b"), t0 + TTL + Duration::from_secs(1)).is_empty());
        book.exchange("t", rep("a"), t0 + TTL + Duration::from_secs(2));
        book.exchange("t", Report { leave: true, ..rep("a") }, t0 + TTL + Duration::from_secs(3));
        assert!(book.exchange("t", rep("b"), t0 + TTL + Duration::from_secs(4)).is_empty());
    }

    #[test]
    fn idle_time_grows_while_a_device_is_quiet_and_reports_are_rate_limited() {
        let (mut book, t0) = (Book::default(), Instant::now());
        book.exchange("t", Report { cursor: Some([1.0, 2.0]), ..rep("a") }, t0);
        // Too soon after the last report: the old cursor stays.
        book.exchange("t", Report { cursor: Some([9.0, 9.0]), ..rep("a") }, t0 + Duration::from_millis(10));
        let seen = book.exchange("t", rep("b"), t0 + Duration::from_secs(7));
        assert_eq!((seen[0].cursor, seen[0].idle), (Some([1.0, 2.0]), 7));
    }

    #[test]
    fn the_relay_bounds_devices_and_rooms() {
        let (mut book, t0) = (Book::default(), Instant::now());
        for i in 0..MAX_DEVICES + 5 {
            book.exchange("t", rep(&format!("d{i}")), t0);
        }
        assert_eq!(book.exchange("t", rep("probe"), t0).len(), MAX_DEVICES);
        for i in 0..MAX_ROOMS + 5 {
            book.exchange(&format!("room{i}"), rep("a"), t0);
        }
        assert!(book.rooms.len() <= MAX_ROOMS);
    }

    #[test]
    fn network_input_is_forced_into_bounds() {
        let wild = Report {
            id: "dev-1".into(),
            label: format!("  ev\u{7}il{}  ", "x".repeat(200)),
            page: "p".repeat(500),
            cursor: Some([1.0, f32::NAN]),
            sel: (0..1000).collect(),
            idle: u32::MAX,
            leave: false,
        };
        let r = wild.sanitized().unwrap();
        assert_eq!((r.label.chars().count(), r.page.len(), r.sel.len(), r.idle), (MAX_LABEL, MAX_PAGE, MAX_SELECTION, GONE_AFTER));
        assert!(!r.label.contains('\u{7}') && r.cursor.is_none());
        for bad in ["", "has space", "../x", &"a".repeat(65)] {
            assert!(Report { id: bad.into(), ..Report::default() }.sanitized().is_none(), "{bad:?}");
        }
        let far = Report { cursor: Some([1.0e9, 0.0]), ..rep("a") };
        assert!(far.sanitized().unwrap().cursor.is_none());
    }

    #[test]
    fn unknown_fields_are_ignored_and_missing_ones_default() {
        let r: Report = serde_json::from_str(r#"{"id":"a","future":{"x":1}}"#).unwrap();
        assert_eq!((r.label.as_str(), r.cursor, r.idle), ("", None, 0));
        let p: Peers = serde_json::from_str(r#"{"peers":[{"id":"a","new":1}],"extra":true}"#).unwrap();
        assert_eq!(p.peers.len(), 1);
    }

    #[test]
    fn a_device_goes_quiet_then_leaves_once() {
        let link = Link::new("me".into(), "Me".into());
        assert!(link.report().is_none(), "nothing to say before any activity");
        link.set_local("p", Some([1.0, 1.0]), &[3]);
        let r = link.report().unwrap();
        assert_eq!((r.sel.as_slice(), r.leave), ([3].as_slice(), false));
        link.state().active = Some(Instant::now() - Duration::from_secs(u64::from(GONE_AFTER) + 1));
        assert!(link.report().unwrap().leave);
        assert!(link.report().is_none(), "the leave frame is sent once");
        link.set_local("p", Some([2.0, 2.0]), &[3]);
        assert!(!link.report().unwrap().leave, "moving again brings it back");
    }

    #[test]
    fn colours_are_stable_per_device() {
        assert_eq!(hue("abc"), hue("abc"));
        assert_ne!(hue("abc"), hue("abd"));
    }
}
