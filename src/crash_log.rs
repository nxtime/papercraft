//! Keeps panics on disk: a release build on Windows has no console, so stderr goes nowhere and
//! the app would simply vanish.

use std::backtrace::Backtrace;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_NAME: &str = "crash.log";

/// Where crash reports go.
pub fn path() -> Option<PathBuf> {
    Some(crate::project::config_dir()?.join(FILE_NAME))
}

/// Appends each panic, on any thread, to `path`; the default report still prints afterwards.
pub fn install(path: PathBuf) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let thread = std::thread::current();
        let report = format!(
            "--- unix time {secs}, version {}, thread {}\n{info}\n{}\n",
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
            Backtrace::force_capture(),
        );
        if let Err(error) = append(&path, &report) {
            eprintln!("could not write {}: {error}", path.display());
        }
        default_hook(info);
    }));
}

fn append(path: &Path, report: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(report.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_appended_to_the_log_with_where_it_happened() {
        let dir = std::env::temp_dir().join(format!("papercraft-crash-{}", std::process::id()));
        let log = dir.join(FILE_NAME);
        install(log.clone());
        for _ in 0..2 {
            assert!(std::panic::catch_unwind(|| panic!("boom for the log")).is_err());
        }
        drop(std::panic::take_hook());

        let text = std::fs::read_to_string(&log).unwrap();
        assert_eq!(text.matches("boom for the log").count(), 2);
        assert!(text.contains("crash_log"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
