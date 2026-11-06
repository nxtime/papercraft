//! Runs a user-configured command template (`{prompt}`, `{out}`, ...) through the platform shell
//! without letting any value act as shell syntax: each value travels in its own environment
//! variable and the shell only dereferences it after parsing.

use std::process::Command;

/// `template` with each `(placeholder, variable, value)` turned into a quoted reference to the
/// variable, as a shell command with the variables set.
pub fn command(template: &str, vars: &[(&str, &str, &str)]) -> Command {
    let mut script = template.to_owned();
    for (placeholder, name, _) in vars {
        // The reference brings its own quotes: quotes the template already puts around the
        // placeholder would pair with them and leave the value bare.
        for q in ['"', '\''] {
            script = script.replace(&format!("{q}{placeholder}{q}"), placeholder);
        }
        script = script.replace(placeholder, &reference(name));
    }
    let mut cmd = shell(&script);
    for (_, name, value) in vars {
        cmd.env(name, env_value(value));
    }
    cmd
}

#[cfg(windows)]
fn shell(script: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("cmd.exe");
    // `/V:ON` expands `!VAR!` after cmd has parsed the line (`%VAR%` expands before, so `&` or `<`
    // in a value would be syntax). `/S` plus raw quotes: Rust's own quoting is not cmd's.
    cmd.args(["/V:ON", "/S", "/C"]).raw_arg(format!("\"{script}\""));
    cmd
}

#[cfg(not(windows))]
fn shell(script: &str) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", script]);
    cmd
}

fn reference(name: &str) -> String {
    if cfg!(windows) { format!("\"!{name}!\"") } else { format!("\"${name}\"") }
}

/// The text a variable carries. A value starting with `-` gets a space in front so the program
/// cannot read it as an option. On Windows the expanded text is read again by batch-file shims
/// (npm's `pi.cmd`, `gf.cmd`, ...) that inherit `/V:ON`: a `"` would end the quoting and let `&`
/// run a command, `!` would expand variables and a line break would cut the line, so those become
/// `'`, `！` and a space. A shim that re-expands an argument unquoted (`%~1`) is still unsafe.
fn env_value(value: &str) -> String {
    let value = if value.starts_with('-') { format!(" {value}") } else { value.to_owned() };
    if !cfg!(windows) {
        return value;
    }
    let mut out: String = value
        .chars()
        .map(|c| match c {
            '"' => '\'',
            '!' => '\u{FF01}',
            '\r' | '\n' => ' ',
            c => c,
        })
        .collect();
    // The closing quote follows: trailing backslashes must not escape it.
    let trailing = out.len() - out.trim_end_matches('\\').len();
    out.push_str(&"\\".repeat(trailing));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const NASTY: &str = "a & b | c < d > e ^ f %PATH% !PATH! \"q\" \\\" g\\ \n h";

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("papercraft-shell-{}-{:08x}", std::process::id(), fastrand::u32(..)));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Runs `template` with `{prompt}` set to `value`; what the program wrote to `{out}`.
    fn received(dir: &Path, template: &str, value: &str) -> String {
        let out = dir.join("arg.txt");
        let mut cmd = command(template, &[("{prompt}", "PAPERCRAFT_PROMPT", value), ("{out}", "PAPERCRAFT_OUT", &out.display().to_string())]);
        cmd.current_dir(dir);
        assert!(cmd.output().unwrap().status.success());
        std::fs::read_to_string(&out).unwrap()
    }

    #[test]
    fn a_value_starting_with_a_dash_is_not_read_as_an_option() {
        let cmd = command("prog {p}", &[("{p}", "PAPERCRAFT_P", "--help")]);
        let value = cmd.get_envs().find(|(k, _)| *k == "PAPERCRAFT_P").and_then(|(_, v)| v);
        assert_eq!(value, Some(std::ffi::OsStr::new(" --help")));
    }

    #[test]
    #[cfg(unix)]
    fn a_value_reaches_the_program_as_one_literal_argument() {
        let dir = scratch();
        let got = received(&dir, "printf '%s' {prompt} > {out}", NASTY);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(got, NASTY);
    }

    /// A PowerShell script that writes its first argument to the file its second names.
    #[cfg(windows)]
    fn writer(dir: &Path) -> PathBuf {
        let script = dir.join("write.ps1");
        std::fs::write(&script, "[IO.File]::WriteAllText($args[1], $args[0])").unwrap();
        script
    }

    #[test]
    #[cfg(windows)]
    fn a_value_reaches_the_program_as_one_literal_argument() {
        let dir = scratch();
        let template = format!("powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" {{prompt}} {{out}}", writer(&dir).display());
        let got = received(&dir, &template, NASTY);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(got, "a & b | c < d > e ^ f %PATH% \u{FF01}PATH\u{FF01} 'q' \\' g\\   h");
    }

    #[test]
    #[cfg(windows)]
    fn a_batch_file_shim_cannot_be_made_to_run_a_command() {
        let dir = scratch();
        // What npm's shims do: hand the arguments on with `%*`, which cmd parses again.
        let shim = dir.join("tool.cmd");
        std::fs::write(&shim, format!("@powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" %*\r\n", writer(&dir).display())).unwrap();
        // Escaping the quote as `\"` (enough for a program's own argv parser) lets this one run.
        let attack = "x\" & echo pwned> injected.txt & \"y !PATH!";
        let got = received(&dir, &format!("\"{}\" \"{{prompt}}\" {{out}}", shim.display()), attack);
        let injected = dir.join("injected.txt").exists();
        std::fs::remove_dir_all(&dir).ok();
        assert!(!injected, "the prompt ran a command");
        assert_eq!(got, "x' & echo pwned> injected.txt & 'y \u{FF01}PATH\u{FF01}");
    }
}
