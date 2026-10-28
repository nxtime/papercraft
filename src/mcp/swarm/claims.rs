//! Claims: an agent's short lease on layers (each with everything under it) or on a shared
//! resource, so other agents work elsewhere (`specs/swarm-safety/spec.md`, section 3).
//!
//! Pure logic over ids and a clock. A claim is all-or-nothing, never waited on, and expires
//! `ttl` after its holder's last call, so no agent can hold the canvas for good and no two can
//! deadlock. The person at the canvas is never checked.

use crate::model::NodeId;

/// Live claims one agent may hold, and all agents together: a runaway agent cannot fence off
/// the canvas.
pub const MAX_PER_CLIENT: usize = 16;
pub const MAX_CLAIMS: usize = 64;
pub const DEFAULT_TTL_SECS: u64 = 300;
pub const MIN_TTL_SECS: u64 = 10;
pub const MAX_TTL_SECS: u64 = 1800;
/// Shared things that cannot be split by layer.
pub const RESOURCES: [&str; 3] = ["variables", "components", "pages"];

/// Who holds a claim.
#[derive(Debug, Clone, PartialEq)]
pub struct Holder {
    /// `Client::key`: name and session.
    pub key: String,
    /// What the person and other agents see ("Codex", "Chat / build 1").
    pub label: String,
    pub session: String,
}

/// One layer of one page.
pub type Layer = (String, NodeId);

#[derive(Debug, Clone)]
pub struct Claim {
    pub id: String,
    pub holder: Holder,
    pub layers: Vec<Layer>,
    pub resources: Vec<String>,
    pub note: String,
    pub ttl_ms: u64,
    pub expires: u64,
}

impl Claim {
    pub fn expires_in_secs(&self, now: u64) -> u64 {
        self.expires.saturating_sub(now).div_ceil(1000)
    }
}

/// What an agent asks to claim.
pub struct Request {
    pub holder: Holder,
    pub layers: Vec<Layer>,
    pub resources: Vec<String>,
    pub note: String,
    pub ttl_secs: u64,
}

/// A request that met someone else's claim.
#[derive(Debug, Clone, PartialEq)]
pub struct Denied {
    /// The requested layer (or resource) that overlaps.
    pub what: String,
    /// `self`, `ancestor` (the claim covers a parent of it), `descendant` or `resource`.
    pub relation: &'static str,
    pub claim: String,
}

/// How a requested layer relates to a claimed one: the same, inside it, or around it.
fn relation(want: &Layer, held: &Layer, ancestors: &dyn Fn(&Layer) -> Vec<NodeId>) -> Option<&'static str> {
    if want.0 != held.0 {
        return None;
    }
    if want.1 == held.1 {
        Some("self")
    } else if ancestors(want).contains(&held.1) {
        Some("ancestor")
    } else if ancestors(held).contains(&want.1) {
        Some("descendant")
    } else {
        None
    }
}

#[derive(Debug, Default)]
pub struct Claims {
    pub live: Vec<Claim>,
    next: u32,
}

impl Claims {
    /// Drops the claims whose time is up.
    pub fn expire(&mut self, now: u64) {
        self.live.retain(|c| c.expires > now);
    }

    /// Any call by the holder keeps its claims alive.
    pub fn renew(&mut self, key: &str, now: u64) {
        for c in self.live.iter_mut().filter(|c| c.holder.key == key) {
            c.expires = now + c.ttl_ms;
        }
    }

    /// Other agents' claims that `layers` or `resources` overlap; `exempt` holders (an agent's
    /// own, or one it delegated to) are skipped.
    pub fn overlaps(
        &self,
        layers: &[Layer],
        resources: &[String],
        exempt: &dyn Fn(&Holder) -> bool,
        ancestors: &dyn Fn(&Layer) -> Vec<NodeId>,
    ) -> Vec<Denied> {
        let mut out = Vec::new();
        for c in self.live.iter().filter(|c| !exempt(&c.holder)) {
            for want in layers {
                if let Some(rel) = c.layers.iter().find_map(|held| relation(want, held, ancestors)) {
                    out.push(Denied { what: format!("{}:{}", want.0, want.1.0), relation: rel, claim: c.id.clone() });
                }
            }
            for r in resources.iter().filter(|r| c.resources.contains(r)) {
                out.push(Denied { what: r.clone(), relation: "resource", claim: c.id.clone() });
            }
        }
        out
    }

    /// Claims the request's layers and resources for its holder, all or nothing. Overlap with
    /// the holder's own claim merges into it; overlap with anyone else's (but `exempt`
    /// holders') denies the whole request.
    pub fn claim(&mut self, req: Request, now: u64, exempt: &dyn Fn(&Holder) -> bool, ancestors: &dyn Fn(&Layer) -> Vec<NodeId>) -> Result<String, Vec<Denied>> {
        let Request { holder, layers, resources, note, ttl_secs } = req;
        self.expire(now);
        let mine = |h: &Holder| h.key == holder.key || exempt(h);
        let denied = self.overlaps(&layers, &resources, &mine, ancestors);
        if !denied.is_empty() {
            return Err(denied);
        }
        let ttl_ms = ttl_secs.clamp(MIN_TTL_SECS, MAX_TTL_SECS) * 1000;
        // Merge into the holder's claim that overlaps (or its only one), so a lease grows
        // instead of piling up.
        let own = self.live.iter().position(|c| {
            c.holder.key == holder.key
                && (layers.iter().any(|w| c.layers.iter().any(|h| relation(w, h, ancestors).is_some()))
                    || resources.iter().any(|r| c.resources.contains(r)))
        });
        if let Some(i) = own {
            let c = &mut self.live[i];
            for l in layers {
                if !c.layers.contains(&l) {
                    c.layers.push(l);
                }
            }
            for r in resources {
                if !c.resources.contains(&r) {
                    c.resources.push(r);
                }
            }
            if !note.is_empty() {
                c.note = note;
            }
            c.ttl_ms = c.ttl_ms.max(ttl_ms);
            c.expires = now + c.ttl_ms;
            return Ok(c.id.clone());
        }
        if self.live.iter().filter(|c| c.holder.key == holder.key).count() >= MAX_PER_CLIENT {
            return Err(vec![Denied { what: format!("{MAX_PER_CLIENT} claims"), relation: "limit", claim: String::new() }]);
        }
        if self.live.len() >= MAX_CLAIMS {
            return Err(vec![Denied { what: format!("{MAX_CLAIMS} claims on the project"), relation: "limit", claim: String::new() }]);
        }
        self.next += 1;
        let id = format!("c{}", self.next);
        self.live.push(Claim { id: id.clone(), holder, layers, resources, note, ttl_ms, expires: now + ttl_ms });
        Ok(id)
    }

    /// Releases the holder's claims: the one with `id`, those covering `layers`, or all of them.
    /// Returns the released ids.
    pub fn release(&mut self, key: &str, id: Option<&str>, layers: &[Layer]) -> Vec<String> {
        let hit = |c: &Claim| {
            c.holder.key == key && id.is_none_or(|i| c.id == i) && (layers.is_empty() || c.layers.iter().any(|l| layers.contains(l)))
        };
        let gone: Vec<String> = self.live.iter().filter(|c| hit(c)).map(|c| c.id.clone()).collect();
        self.live.retain(|c| !hit(c));
        gone
    }

    /// Releases every claim of the holders `drop` picks (a finished chat turn, a project switch).
    pub fn release_where(&mut self, drop: impl Fn(&Holder) -> bool) {
        self.live.retain(|c| !drop(&c.holder));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(k: &str) -> Holder {
        Holder { key: k.into(), label: k.into(), session: k.into() }
    }

    /// A page "p" where 2 sits in 1 and 3 in 2.
    fn ancestors(l: &Layer) -> Vec<NodeId> {
        match l.1.0 {
            3 => vec![NodeId(2), NodeId(1)],
            2 => vec![NodeId(1)],
            _ => vec![],
        }
    }

    fn at(n: u64) -> Layer {
        ("p".into(), NodeId(n))
    }

    fn req(k: &str, layers: Vec<Layer>, resources: Vec<String>) -> Request {
        Request { holder: who(k), layers, resources, note: String::new(), ttl_secs: 60 }
    }

    fn none(_: &Holder) -> bool {
        false
    }

    #[test]
    fn a_claim_covers_its_subtree_and_denies_around_and_inside_it() {
        let mut c = Claims::default();
        c.claim(req("a", vec![at(2)], vec![]), 0, &none, &ancestors).unwrap();
        for (n, rel) in [(2, "self"), (3, "ancestor"), (1, "descendant")] {
            let d = c.claim(req("b", vec![at(n)], vec![]), 0, &none, &ancestors).unwrap_err();
            assert_eq!(d[0].relation, rel, "layer {n}");
        }
        assert!(c.claim(req("b", vec![at(9)], vec![]), 0, &none, &ancestors).is_ok());
    }

    #[test]
    fn a_denied_request_claims_nothing() {
        let mut c = Claims::default();
        c.claim(req("a", vec![at(2)], vec![]), 0, &none, &ancestors).unwrap();
        assert!(c.claim(req("b", vec![at(9), at(3)], vec![]), 0, &none, &ancestors).is_err());
        assert!(c.live.iter().all(|x| x.holder.key == "a"), "9 was not claimed either");
    }

    #[test]
    fn own_claims_merge_and_leases_slide_until_the_holder_goes_quiet() {
        let mut c = Claims::default();
        let first = c.claim(req("a", vec![at(1)], vec![]), 0, &none, &ancestors).unwrap();
        let again = c.claim(req("a", vec![at(3)], vec![]), 1_000, &none, &ancestors).unwrap();
        assert_eq!((first.clone(), c.live.len()), (again, 1));
        c.renew("a", 50_000);
        c.expire(100_000);
        assert_eq!(c.live.len(), 1, "renewed at 50 s, lives to 110 s");
        c.expire(110_001);
        assert!(c.live.is_empty());
    }

    #[test]
    fn resources_and_release() {
        let mut c = Claims::default();
        c.claim(req("a", vec![], vec!["variables".into()]), 0, &none, &ancestors).unwrap();
        let d = c.overlaps(&[], &["variables".into()], &none, &ancestors);
        assert_eq!(d[0].relation, "resource");
        assert!(c.overlaps(&[], &["variables".into()], &|h| h.key == "a", &ancestors).is_empty(), "exempt holder");
        assert_eq!(c.release("b", None, &[]), Vec::<String>::new(), "only the holder releases");
        assert_eq!(c.release("a", None, &[]).len(), 1);
    }

    #[test]
    fn limits_stop_a_runaway_agent() {
        let mut c = Claims::default();
        for n in 0..MAX_PER_CLIENT as u64 {
            c.claim(req("a", vec![at(100 + n)], vec![]), 0, &none, &ancestors).unwrap();
        }
        let d = c.claim(req("a", vec![at(500)], vec![]), 0, &none, &ancestors).unwrap_err();
        assert_eq!(d[0].relation, "limit");
    }
}
