//! Keeps the URL hash and what is open in step (see `crate::route`): opening a design or a page
//! adds a history entry, and back, forward or a pasted link opens what the URL names.

use super::{Web, window};
use crate::route::{self, Route};
use crate::ui::home::HomeAction;
use wasm_bindgen::JsValue;

/// How long a link may wait for its design to arrive from the library before it gives up.
const WAIT_MS: f64 = 15_000.0;

#[derive(Default)]
pub(super) struct Routing {
    /// The hash this tab last wrote or acted on; a different one means the user navigated.
    written: String,
    pending: Option<Pending>,
}

struct Pending {
    route: Route,
    since: f64,
    /// The design was asked to open, so the request is not repeated every frame.
    asked: bool,
}

impl Web {
    pub(super) fn sync_route(&mut self, now: f64) {
        let hash = window().location().hash().unwrap_or_default();
        // `#demo` and plain anchors are not routes: leave the URL alone.
        let Some(at_url) = route::parse(&hash) else {
            return;
        };
        if hash != self.routing.written {
            self.routing.written = hash;
            self.routing.pending = Some(Pending {
                route: at_url,
                since: now,
                asked: false,
            });
        }
        // Filling in what a navigation named (its page, say) replaces that history entry.
        let settled = self.routing.pending.is_some();
        if let Some(mut p) = self.routing.pending.take()
            && !self.apply_route(&mut p)
        {
            if now - p.since < WAIT_MS {
                self.routing.pending = Some(p);
                self.wake_at(now + 250.0);
                return;
            }
            // An open that was asked for and failed has shown its own error.
            if !p.asked {
                self.error("That design is not in this browser's library.".into());
            }
        }
        let current = route::format(&self.current_route());
        if current != self.routing.written {
            let loc = window().location();
            let url = if current.is_empty() {
                format!(
                    "{}{}",
                    loc.pathname().unwrap_or_default(),
                    loc.search().unwrap_or_default()
                )
            } else {
                current.clone()
            };
            if let Err(e) = window().history().and_then(|h| match settled {
                true => h.replace_state_with_url(&JsValue::NULL, "", Some(&url)),
                false => h.push_state_with_url(&JsValue::NULL, "", Some(&url)),
            }) {
                log::warn!("route: could not update the URL: {e:?}");
            }
            self.routing.written = current;
        }
    }

    /// What is open; an id `parse` would refuse (an old import's page) is left out of the URL, so
    /// the tab can still read back what it wrote.
    fn current_route(&self) -> Route {
        match &self.session.project {
            Some(p) if route::valid_id(&p.manifest.id) => {
                let page = self.ed.active_page_id();
                Route::Design {
                    project: p.manifest.id.clone(),
                    page: route::valid_id(page).then(|| page.to_owned()),
                }
            }
            _ => Route::Home,
        }
    }

    /// Moves toward `p`'s route; true once the app shows it (or as close as it can: a page id the
    /// design does not have keeps the current page).
    fn apply_route(&mut self, p: &mut Pending) -> bool {
        let Route::Design { project, page } = &p.route else {
            if self.session.project.is_some() {
                self.go_home();
            }
            return true;
        };
        if self
            .session
            .project
            .as_ref()
            .is_some_and(|open| open.manifest.id == *project)
        {
            if let Some(i) = page
                .as_ref()
                .and_then(|id| self.ed.pages.iter().position(|pg| pg.id == *id))
            {
                self.ed.switch_page(i);
            }
            return true;
        }
        if p.asked {
            return false;
        }
        // Not listed yet: a design another device made may still be syncing from the library.
        let Some(dir) = self
            .session
            .store
            .list()
            .into_iter()
            .find(|e| e.manifest.id == *project)
            .map(|e| e.dir)
        else {
            return false;
        };
        if self.session.project.is_some() {
            self.go_home();
        }
        if let Some(h) = &mut self.st.home {
            h.actions.push(HomeAction::Open(dir));
            p.asked = true;
        }
        false
    }
}
