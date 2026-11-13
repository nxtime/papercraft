//! Invites for `/join/<token>`: who may create an account, until when and with which role.
//!
//! They live in `<data>/invites.json` and hold only the SHA-256 of the token, so a copy of the
//! file lets nobody in. The relay re-reads the file on every request, which is how an invite made
//! by `papercraft-relay invite` reaches the running server. Joins are rare and the relay
//! serialises them, so the read-modify-write here takes no file lock.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE: &str = "invites.json";
const TOKEN_BYTES: usize = 32;
/// Hex length of a token; anything else cannot be one of ours.
pub const TOKEN_HEX_LEN: usize = TOKEN_BYTES * 2;
const MAX_ROLE_CHARS: usize = 64;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Invite {
    /// Hex SHA-256 of the token.
    hash: String,
    /// Unix seconds.
    pub expires: u64,
    pub role: String,
    /// Joins left; 0 means unlimited until expiry.
    pub uses: u32,
}

#[derive(Serialize, Deserialize, Default)]
struct Book {
    invites: Vec<Invite>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn hash_of(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

fn load(dir: &Path) -> Result<Book, String> {
    match std::fs::read(path(dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{FILE} is damaged: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Book::default()),
        Err(e) => Err(format!("cannot read {FILE}: {e}")),
    }
}

fn save(dir: &Path, book: &Book) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(book).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    crate::io::atomic::write(&path(dir), &body).map_err(|e| format!("cannot write {FILE}: {e}"))
}

/// Make an invite; returns the raw token, which is not kept anywhere. Expired invites are dropped.
pub fn create(dir: &Path, hours: u64, role: &str, uses: u32) -> Result<String, String> {
    if role.is_empty()
        || role.chars().count() > MAX_ROLE_CHARS
        || role.chars().any(char::is_control)
    {
        return Err("the role must be 1..64 characters without control characters".into());
    }
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| format!("no random number generator: {e}"))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let now = now();
    let mut book = load(dir)?;
    book.invites.retain(|i| i.expires > now);
    book.invites.push(Invite {
        hash: hash_of(&token),
        expires: now.saturating_add(hours.saturating_mul(3600)),
        role: role.to_string(),
        uses,
    });
    save(dir, &book)?;
    Ok(token)
}

/// The invite this token opens, if it exists, has not expired and has joins left.
pub fn find(dir: &Path, token: &str, now: u64) -> Result<Option<Invite>, String> {
    if token.len() != TOKEN_HEX_LEN {
        return Ok(None);
    }
    let hash = hash_of(token);
    Ok(load(dir)?
        .invites
        .into_iter()
        .find(|i| i.hash == hash && i.expires > now))
}

/// Count one join against the invite; a single-use invite is removed.
pub fn consume(dir: &Path, token: &str) -> Result<(), String> {
    let hash = hash_of(token);
    let mut book = load(dir)?;
    for invite in &mut book.invites {
        if invite.hash == hash && invite.uses > 0 {
            invite.uses -= 1;
            if invite.uses == 0 {
                // 0 means unlimited, so a used-up invite has to go instead.
                invite.expires = 0;
            }
        }
    }
    book.invites.retain(|i| i.expires > 0);
    save(dir, &book)
}

/// Delete every invite; returns how many there were.
pub fn revoke_all(dir: &Path) -> Result<usize, String> {
    let book = load(dir)?;
    save(dir, &Book::default())?;
    Ok(book.invites.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pc-invite-{:016x}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        dir
    }

    #[test]
    fn a_new_invite_is_valid_and_carries_its_role() {
        let dir = tmp();
        let token = create(&dir, 1, "editor", 0).expect("created");
        let invite = find(&dir, &token, now()).expect("read").expect("valid");
        assert_eq!(invite.role, "editor");
        assert_eq!(find(&dir, "00", now()).expect("read"), None);
    }

    #[test]
    fn the_file_never_holds_the_raw_token() {
        let dir = tmp();
        let token = create(&dir, 1, "editor", 0).expect("created");
        let text = std::fs::read_to_string(path(&dir)).expect("file");
        assert!(!text.contains(&token));
        assert!(text.contains(&hash_of(&token)));
    }

    #[test]
    fn an_expired_invite_is_not_found() {
        let dir = tmp();
        let token = create(&dir, 1, "editor", 0).expect("created");
        assert_eq!(find(&dir, &token, now() + 3601).expect("read"), None);
    }

    #[test]
    fn a_limited_invite_is_used_up_and_unlimited_is_not() {
        let dir = tmp();
        let once = create(&dir, 1, "editor", 1).expect("created");
        let always = create(&dir, 1, "editor", 0).expect("created");
        consume(&dir, &once).expect("consumed");
        consume(&dir, &always).expect("consumed");
        assert_eq!(find(&dir, &once, now()).expect("read"), None);
        assert!(find(&dir, &always, now()).expect("read").is_some());
    }

    #[test]
    fn revoke_all_removes_every_invite() {
        let dir = tmp();
        let a = create(&dir, 1, "editor", 0).expect("created");
        create(&dir, 1, "viewer", 3).expect("created");
        assert_eq!(revoke_all(&dir).expect("revoked"), 2);
        assert_eq!(find(&dir, &a, now()).expect("read"), None);
    }

    #[test]
    fn a_bad_role_is_refused() {
        assert!(create(&tmp(), 1, "", 0).is_err());
        assert!(create(&tmp(), 1, "a\nb", 0).is_err());
    }
}
