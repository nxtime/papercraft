//! API keys kept as files in a private folder, behind `keyring::Entry`. Android has no keyring
//! store (its entries would live in memory and be lost on restart), so the app installs this one
//! over the app's private storage, which only the app (and root) can read.

use keyring::credential::{Credential, CredentialApi, CredentialBuilderApi, CredentialPersistence};
use keyring::{Error, Result};
use std::any::Any;
use std::path::{Path, PathBuf};

/// Makes `keyring::Entry` read and write files under `dir`. Call once, before any entry is made.
pub fn install(dir: &Path) {
    keyring::set_default_credential_builder(Box::new(FileStore { dir: dir.to_path_buf() }));
}

struct FileStore {
    dir: PathBuf,
}

struct FileEntry {
    path: PathBuf,
}

/// A file name from the service and user: hex, so a URL or a colon in either is safe.
fn file_name(service: &str, user: &str) -> String {
    let hex: String = format!("{service}\n{user}").bytes().map(|b| format!("{b:02x}")).collect();
    format!("{hex}.key")
}

impl CredentialBuilderApi for FileStore {
    fn build(&self, _target: Option<&str>, service: &str, user: &str) -> Result<Box<Credential>> {
        Ok(Box::new(FileEntry { path: self.dir.join(file_name(service, user)) }))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn persistence(&self) -> CredentialPersistence {
        CredentialPersistence::UntilDelete
    }
}

fn platform(e: std::io::Error) -> Error {
    Error::PlatformFailure(Box::new(e))
}

impl CredentialApi for FileEntry {
    fn set_secret(&self, secret: &[u8]) -> Result<()> {
        let dir = self.path.parent().ok_or(Error::Invalid("path".into(), "no folder".into()))?;
        std::fs::create_dir_all(dir).map_err(platform)?;
        // Written beside the target and renamed over it, so a crash never leaves half a key.
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, secret).map_err(platform)?;
        std::fs::rename(&tmp, &self.path).map_err(platform)
    }

    fn get_secret(&self) -> Result<Vec<u8>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::NoEntry),
            Err(e) => Err(platform(e)),
        }
    }

    fn delete_credential(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::NoEntry),
            Err(e) => Err(platform(e)),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_survives_a_new_store_and_deletes() {
        let dir = std::env::temp_dir().join(format!("pc-keys-{}", std::process::id()));
        let store = FileStore { dir: dir.clone() };
        let entry = store.build(None, "PaperCraft", "https://api.anthropic.com").unwrap();
        assert!(matches!(entry.get_password(), Err(Error::NoEntry)));
        entry.set_password("sk-test").unwrap();
        let again = FileStore { dir: dir.clone() }.build(None, "PaperCraft", "https://api.anthropic.com").unwrap();
        assert_eq!(again.get_password().unwrap(), "sk-test");
        again.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(Error::NoEntry)));
        std::fs::remove_dir_all(dir).ok();
    }
}
