//! The PATH agents are found and run with. An app opened from the Dock gets the bare system PATH
//! (`/usr/bin:/bin:/usr/sbin:/sbin`), so `~/.local/bin` and Homebrew are invisible. The login
//! shell's PATH is read once, off the UI thread, and merged with the process PATH and the usual
//! install folders.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const SHELL_TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(20);
/// The PATH is framed by markers: rc/exit-hook noise on stdout before or after cannot replace it.
const SHELL_SCRIPT: &str = r#"printf '\n__PC_PATH_BEGIN__%s__PC_PATH_END__\n' "$PATH""#;
const PATH_BEGIN: &str = "__PC_PATH_BEGIN__";
const PATH_END: &str = "__PC_PATH_END__";
/// Same keys the agent spawns drop: a login shell must not hand them to anything.
pub(super) const CREDENTIAL_ENV: [&str; 4] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "CODEX_API_KEY"];
/// Under `$HOME`.
const HOME_DIRS: [&str; 6] = [".local/bin", ".cargo/bin", ".bun/bin", ".npm-global/bin", ".volta/bin", ".nvm/current/bin"];
const SYSTEM_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

static RESOLVED: OnceLock<String> = OnceLock::new();

/// Starts resolving in the background; call once at app start.
pub fn init() {
    std::thread::spawn(|| {
        resolved_path();
    });
}

/// The merged PATH. Blocks only while the first resolution is still running.
pub fn resolved_path() -> &'static str {
    RESOLVED.get_or_init(|| {
        let shell = std::env::var("SHELL").ok().filter(|_| cfg!(unix)).and_then(|s| shell_path(&s, SHELL_SCRIPT, SHELL_TIMEOUT));
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let extras = HOME_DIRS.iter().filter_map(|d| home.as_ref().map(|h| h.join(d))).chain(SYSTEM_DIRS.iter().map(PathBuf::from)).filter(|d| d.is_dir()).collect();
        merge(shell.as_deref().unwrap_or_default(), &std::env::var_os("PATH").unwrap_or_default(), extras)
    })
}

/// Shell PATH first (the user's own order), then the process PATH, then the extra folders; each
/// folder once.
fn merge(shell: &str, process: &OsStr, extras: Vec<PathBuf>) -> String {
    let mut out: Vec<PathBuf> = vec![];
    for d in std::env::split_paths(shell).chain(std::env::split_paths(process)).chain(extras) {
        if !d.as_os_str().is_empty() && !out.contains(&d) {
            out.push(d);
        }
    }
    std::env::join_paths(out).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Runs `shell -l -c script` and returns the text between the PATH markers; None on failure or timeout.
fn shell_path(shell: &str, script: &str, timeout: Duration) -> Option<String> {
    let mut cmd = Command::new(shell);
    cmd.args(["-l", "-c", script]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    for k in CREDENTIAL_ENV {
        cmd.env_remove(k);
    }
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // Read on a thread: a shell stuck in an rc file never closes the pipe.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        let _ = tx.send(s);
    });
    let start = Instant::now();
    let out = loop {
        match rx.recv_timeout(POLL) {
            Ok(s) => break Some(s),
            Err(_) if start.elapsed() >= timeout => break None,
            Err(_) => {}
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    let out = out?;
    let from = out.rfind(PATH_BEGIN)? + PATH_BEGIN.len();
    let len = out[from..].find(PATH_END)?;
    Some(out[from..from + len].trim().to_string()).filter(|p| !p.is_empty())
}

/// First executable named `name` on the resolved PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, OsStr::new(resolved_path()))
}

pub(super) fn which_in(name: &str, path: &OsStr) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) { ["exe", "cmd", "bat"].iter().map(|e| format!("{name}.{e}")).collect() } else { vec![name.into()] };
    std::env::split_paths(path).flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    p.is_file()
}

/// Sets the merged PATH on a child, so node-based CLIs find `node`.
pub fn apply(cmd: &mut Command) -> &mut Command {
    cmd.env("PATH", OsString::from(resolved_path()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn merge_dedupes_and_keeps_order() {
        let m = merge("/a:/b:/a", OsStr::new("/b:/c::/d"), vec!["/d".into(), "/e".into()]);
        assert_eq!(m, "/a:/b:/c:/d:/e");
    }

    #[test]
    fn which_finds_only_executables_on_the_given_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("pc-path-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let (bin, plain) = (dir.join("fake-agent"), dir.join("not-exec"));
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        std::fs::write(&plain, "x").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::join_paths(["/nonexistent", dir.to_str().unwrap()]).unwrap();
        assert_eq!(which_in("fake-agent", &path), Some(bin));
        assert_eq!(which_in("not-exec", &path), None);
        assert_eq!(which_in("missing", &path), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shell_path_reads_between_markers_and_times_out() {
        let script = r#"echo noise; printf '\n__PC_PATH_BEGIN__%s__PC_PATH_END__\n' /x:/y; echo exit-hook-noise"#;
        assert_eq!(shell_path("/bin/sh", script, Duration::from_secs(3)).as_deref(), Some("/x:/y"));
        assert_eq!(shell_path("/bin/sh", SHELL_SCRIPT, Duration::from_secs(3)).is_some(), true);
        assert_eq!(shell_path("/bin/sh", "echo /only/noise", Duration::from_secs(3)), None);
        let t = Instant::now();
        assert_eq!(shell_path("/bin/sh", "sleep 10", Duration::from_millis(200)), None);
        assert!(t.elapsed() < Duration::from_secs(3));
        assert_eq!(shell_path("/nonexistent-shell", SHELL_SCRIPT, Duration::from_secs(1)), None);
    }
}
