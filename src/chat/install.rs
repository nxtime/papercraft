//! Installing an agent PaperCraft can run on free models (OpenCode, Pi): the methods each agent's
//! install docs give per OS (checked 2026-10-29), the ones that apply to this machine, and a
//! runner that streams their output. Nothing runs elevated; methods that need `sudo` or an admin
//! shell are offered as a command to copy.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Linux,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A package manager installs it.
    Package,
    /// A script downloaded from the agent's site runs: listed last, its URL shown in full.
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Method {
    pub label: &'static str,
    pub os: &'static [Os],
    /// Linux only: the distributions (os-release `ID` or `ID_LIKE`) it is for; empty = any.
    pub distros: &'static [&'static str],
    /// The program that must be on PATH (`brew`, `npm`, `curl`, ...).
    pub needs: &'static str,
    /// Lowest Node version (major, minor) an npm method needs.
    pub node: Option<(u32, u32)>,
    pub command: &'static str,
    pub kind: Kind,
    /// Asks for a password or an admin shell: shown to copy, never run.
    pub elevated: bool,
}

const UNIX: &[Os] = &[Os::Mac, Os::Linux];
const ALL: &[Os] = &[Os::Mac, Os::Linux, Os::Windows];
const WINDOWS: &[Os] = &[Os::Windows];

const fn method(label: &'static str, os: &'static [Os], needs: &'static str, command: &'static str, kind: Kind) -> Method {
    Method { label, os, distros: &[], needs, node: None, command, kind, elevated: false }
}

/// OpenCode 1.x: plain `brew install opencode` and Arch's `pacman -S opencode` install OpenCode 2,
/// which PaperCraft's ACP and model code has not been tested on.
const OPENCODE: &[Method] = &[
    method("Homebrew", UNIX, "brew", "brew install anomalyco/tap/opencode", Kind::Package),
    method("Scoop", WINDOWS, "scoop", "scoop install opencode", Kind::Package),
    method("npm", ALL, "npm", "npm install -g opencode-ai", Kind::Package),
    Method { distros: &["arch"], elevated: true, ..method("AUR (paru)", &[Os::Linux], "paru", "paru -S opencode-bin", Kind::Package) },
    Method { elevated: true, ..method("Chocolatey", WINDOWS, "choco", "choco install opencode", Kind::Package) },
    method("Install script", UNIX, "curl", "curl -fsSL https://opencode.ai/install | bash", Kind::Script),
];

const PI: &[Method] = &[
    Method { node: Some((22, 19)), ..method("npm", ALL, "npm", "npm install -g --ignore-scripts @earendil-works/pi-coding-agent", Kind::Package) },
    method("Homebrew", UNIX, "brew", "brew install pi-coding-agent", Kind::Package),
    method("Nix", UNIX, "nix", "nix profile add github:earendil-works/pi/stable", Kind::Package),
    method("Install script", UNIX, "curl", "curl -fsSL https://pi.dev/install.sh | sh", Kind::Script),
    method("Install script", WINDOWS, "powershell", "powershell -c \"irm https://pi.dev/install.ps1 | iex\"", Kind::Script),
];

/// The agents the wizard installs.
pub const AGENTS: [&str; 2] = ["opencode", "pi"];

/// What the wizard knows about this machine.
pub struct Host {
    pub os: Os,
    /// os-release `ID` and `ID_LIKE` words (Linux).
    pub distro: Vec<String>,
    /// Programs found on PATH, from those methods need.
    pub has: Vec<&'static str>,
    pub node: Option<(u32, u32)>,
}

/// Why a method this OS has is not offered: shown so the user knows what to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    pub label: &'static str,
    pub reason: String,
}

/// The methods for `agent` that can run here, best first, and the ones this OS has that cannot.
pub fn methods(agent: &str, host: &Host) -> (Vec<Method>, Vec<Missing>) {
    let all = match agent {
        "opencode" => OPENCODE,
        "pi" => PI,
        _ => &[],
    };
    let mut ok = vec![];
    let mut missing = vec![];
    for m in all.iter().filter(|m| m.os.contains(&host.os) && (m.distros.is_empty() || m.distros.iter().any(|d| host.distro.iter().any(|h| h == d)))) {
        let reason = if !host.has.contains(&m.needs) {
            Some(format!("needs {}", m.needs))
        } else {
            m.node.filter(|min| host.node.is_none_or(|v| v < *min)).map(|(a, b)| format!("needs Node {a}.{b} or newer"))
        };
        match reason {
            Some(reason) => missing.push(Missing { label: m.label, reason }),
            None => ok.push(*m),
        }
    }
    ok.sort_by_key(|m| m.kind == Kind::Script);
    (ok, missing)
}

/// One line of a running install, or its end.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Out(String),
    Done(Result<(), String>),
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{detect, run};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::mpsc::{Receiver, Sender, channel};

    /// `v22.19.0` as (22, 19).
    pub(super) fn parse_node(v: &str) -> Option<(u32, u32)> {
        let mut parts = v.trim().trim_start_matches('v').split('.');
        Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
    }

    /// os-release `ID` and `ID_LIKE` words.
    pub(super) fn parse_os_release(text: &str) -> Vec<String> {
        text.lines()
            .filter_map(|l| l.strip_prefix("ID=").or_else(|| l.strip_prefix("ID_LIKE=")))
            .flat_map(|v| v.trim_matches('"').split_whitespace().map(str::to_owned).collect::<Vec<_>>())
            .collect()
    }

    /// The OS, distribution, the programs methods need and the Node version. Runs `node
    /// --version`, so call it off the UI thread.
    pub fn detect() -> Host {
        let os = if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Linux
        };
        let distro = std::fs::read_to_string("/etc/os-release").map(|t| parse_os_release(&t)).unwrap_or_default();
        let mut needs: Vec<&'static str> = OPENCODE.iter().chain(PI).map(|m| m.needs).collect();
        needs.dedup();
        let has = needs.into_iter().filter(|n| crate::chat::path::which(n).is_some()).collect();
        let node = crate::chat::path::which("node").and_then(|n| {
            let mut cmd = Command::new(n);
            crate::chat::path::apply(cmd.arg("--version").stdin(Stdio::null()));
            cmd.output().ok().and_then(|o| parse_node(&String::from_utf8_lossy(&o.stdout)))
        });
        Host { os, distro, has, node }
    }

    /// Runs `m` through the shell (pipes and quoting as the docs write them) with the merged
    /// PATH, streaming stdout and stderr lines. An elevated method is refused.
    pub fn run(m: &Method) -> Receiver<Line> {
        let (tx, rx) = channel();
        if m.elevated {
            let _ = tx.send(Line::Done(Err(format!("{} needs an admin shell or a password: run it yourself", m.label))));
            return rx;
        }
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", m.command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", m.command]);
            c
        };
        crate::chat::path::apply(&mut cmd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let command = m.command;
        std::thread::spawn(move || {
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Line::Done(Err(format!("Could not start `{command}`: {e}"))));
                    return;
                }
            };
            let pipe = |r: Box<dyn std::io::Read + Send>, tx: Sender<Line>| {
                std::thread::spawn(move || {
                    for l in BufReader::new(r).lines().map_while(Result::ok) {
                        if tx.send(Line::Out(l)).is_err() {
                            break;
                        }
                    }
                })
            };
            let readers = [child.stdout.take().map(|o| pipe(Box::new(o), tx.clone())), child.stderr.take().map(|e| pipe(Box::new(e), tx.clone()))];
            let status = child.wait();
            for r in readers.into_iter().flatten() {
                let _ = r.join();
            }
            let end = match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => Err(format!("`{command}` failed ({s})")),
                Err(e) => Err(format!("`{command}`: {e}")),
            };
            let _ = tx.send(Line::Done(end));
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::native::{parse_node, parse_os_release};

    fn host(os: Os, distro: &[&str], has: &[&'static str], node: Option<(u32, u32)>) -> Host {
        Host { os, distro: distro.iter().map(|d| (*d).to_owned()).collect(), has: has.to_vec(), node }
    }

    fn labels(agent: &str, h: &Host) -> Vec<&'static str> {
        methods(agent, h).0.iter().map(|m| m.label).collect()
    }

    #[test]
    fn mac_with_homebrew_and_node() {
        let h = host(Os::Mac, &[], &["brew", "npm", "curl"], Some((24, 13)));
        assert_eq!(labels("opencode", &h), ["Homebrew", "npm", "Install script"]);
        assert_eq!(labels("pi", &h), ["npm", "Homebrew", "Install script"]);
    }

    #[test]
    fn mac_without_node_hides_npm_and_says_why() {
        let h = host(Os::Mac, &[], &["curl"], None);
        let (ok, missing) = methods("pi", &h);
        assert_eq!(ok.iter().map(|m| m.label).collect::<Vec<_>>(), ["Install script"]);
        assert!(missing.contains(&Missing { label: "npm", reason: "needs npm".into() }));
        let old = host(Os::Mac, &[], &["npm"], Some((22, 3)));
        assert!(methods("pi", &old).1.iter().any(|m| m.reason == "needs Node 22.19 or newer"));
    }

    #[test]
    fn ubuntu_and_arch() {
        let ubuntu = host(Os::Linux, &parse_os_release("ID=ubuntu\nID_LIKE=debian\n").iter().map(String::as_str).collect::<Vec<_>>(), &["curl", "npm", "paru"], Some((22, 20)));
        assert_eq!(labels("opencode", &ubuntu), ["npm", "Install script"], "paru is for Arch only");
        let arch = host(Os::Linux, &["arch"], &["curl", "paru"], None);
        let (ok, _) = methods("opencode", &arch);
        assert!(ok.iter().any(|m| m.label == "AUR (paru)" && m.elevated), "offered to copy, never run");
    }

    #[test]
    fn windows_with_scoop() {
        let h = host(Os::Windows, &[], &["scoop", "powershell"], None);
        assert_eq!(labels("opencode", &h), ["Scoop"]);
        assert_eq!(labels("pi", &h), ["Install script"]);
        assert!(methods("pi", &h).0[0].command.contains("install.ps1"));
    }

    #[test]
    fn versions_and_distros_parse() {
        assert_eq!(parse_node("v22.19.0\n"), Some((22, 19)));
        assert_eq!(parse_os_release("NAME=\"Arch Linux\"\nID=arch\n"), ["arch"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_run_streams_lines_and_reports_failure() {
        let ok = run(&method("echo", UNIX, "sh", "echo one; echo two >&2", Kind::Package));
        let lines: Vec<Line> = ok.iter().collect();
        assert!(lines.contains(&Line::Out("one".into())) && lines.contains(&Line::Out("two".into())));
        assert_eq!(lines.last(), Some(&Line::Done(Ok(()))));
        let bad: Vec<Line> = run(&method("false", UNIX, "sh", "exit 3", Kind::Package)).iter().collect();
        assert!(matches!(bad.last(), Some(Line::Done(Err(_)))));
        let sudo: Vec<Line> = run(&Method { elevated: true, ..method("x", UNIX, "sh", "echo never", Kind::Package) }).iter().collect();
        assert_eq!(sudo.len(), 1, "an elevated method never runs");
    }
}
