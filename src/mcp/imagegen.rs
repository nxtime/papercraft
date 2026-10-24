//! `generate_image`: art for a design (card art, heroes, thumbnails) or reference images, from the
//! provider the user turned on (Settings, Chat): the Codex CLI's image generation on their own
//! Codex login, or a command of their own (any CLI or script that writes images to a folder).
//! Codex makes the whole batch in one `codex exec` (each run costs Codex a fixed prompt) and
//! leaves the images in `~/.codex/generated_images/<thread>/`. The images are saved in the chat's
//! folder like fetched references (scaled for the chat, the original kept for placing).

use super::{Out, ToolResult};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const TOOL: &str = "generate_image";
const MAX_PROMPTS: usize = 4;
/// Image generation takes a while; a batch of four can take minutes.
const TIMEOUT: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_millis(250);
/// Keys that would bill an API account instead of the user's Codex login.
const CREDENTIAL_ENV: [&str; 2] = ["OPENAI_API_KEY", "CODEX_API_KEY"];
const IMAGE_EXTS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// Where images come from; off unless the user turned one on.
#[derive(Debug, Clone, PartialEq)]
pub enum Provider {
    Off,
    Codex,
    /// A shell command run once per prompt (in parallel for a batch). `{prompt}`, `{aspect}`,
    /// `{ratio}` and `{out}` are replaced by quoted references to `$PAPERCRAFT_PROMPT`,
    /// `$PAPERCRAFT_ASPECT`, `$PAPERCRAFT_RATIO` and `$PAPERCRAFT_OUT`, so a prompt is never part
    /// of the shell text. The images it writes into `{out}` are kept.
    Command(String),
}

/// The "Google Flow (gf)" choice: the user's `gf` CLI (Flow in their signed-in Chrome).
pub const GF_COMMAND: &str = "gf generate image --prompt {prompt} --ratio {ratio} --out {out} --no-headed --quiet";

/// `{ratio}` for an aspect: what Flow-style tools take.
fn ratio(aspect: &str) -> &'static str {
    match aspect {
        "portrait" => "3:4",
        "landscape" => "16:9",
        _ => "1:1",
    }
}

/// Generates, saves and returns the images. `chat`: the calling chat's id (its folder).
pub fn generate(project: Option<&Path>, chat: Option<&str>, provider: &Provider, args: &Value) -> ToolResult {
    let folder = chat.filter(|c| crate::chat::valid_id(c)).unwrap_or("references");
    let dir = project.ok_or("open a project first")?.join(crate::chat::TMP_DIR).join(folder);
    let prompts: Vec<&str> = args["prompts"].as_array().map(|a| a.iter().filter_map(Value::as_str).filter(|p| !p.trim().is_empty()).collect()).unwrap_or_default();
    if !(1..=MAX_PROMPTS).contains(&prompts.len()) {
        return Err(format!("pass 1 to {MAX_PROMPTS} prompts"));
    }
    let aspect = args["aspect"].as_str().filter(|a| ["portrait", "landscape"].contains(a)).unwrap_or("square");
    let images = match provider {
        Provider::Off => return Err("Image generation is off. The user can turn it on in Settings, Chat, Images (Codex or a command of their own); keep quiet placeholders until then.".into()),
        Provider::Codex => {
            let (bin, _) = crate::chat::codex::detect(None).ok_or("the Codex CLI is not installed (or not on PATH): install it and sign in with `codex login` to generate images")?;
            let described = match aspect { "portrait" => "portrait (2:3)", "landscape" => "landscape (3:2)", _ => "square (1:1)" };
            run_codex(&bin, &codex_prompt(&prompts, described))?
        }
        // In parallel: a tool with a daemon (gf) queues past its own limit.
        Provider::Command(template) => std::thread::scope(|s| {
            let runs: Vec<_> = prompts.iter().map(|p| s.spawn(move || run_command(template, p, aspect))).collect();
            runs.into_iter().map(|r| r.join().unwrap_or_else(|_| Err("the image command crashed".into()))).collect::<Result<Vec<_>, _>>()
        })?
        .concat(),
    };
    if images.is_empty() {
        return Err("the image tool made no image".into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = crate::project::format::now_ms();
    let (mut outs, mut listed) = (Vec::new(), Vec::new());
    for (n, src) in images.iter().enumerate() {
        let data = std::fs::read(src).map_err(|e| e.to_string())?;
        let mime = match image::guess_format(&data) {
            Ok(image::ImageFormat::Jpeg) => "image/jpeg",
            Ok(image::ImageFormat::WebP) => "image/webp",
            _ => "image/png",
        };
        let ext = match mime {
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            _ => "png",
        };
        let img = crate::chat::events::ImageAttachment { mime: mime.into(), data };
        let (name, img, full) = crate::chat::shrink_image(format!("gen-{stamp}-{n}.{ext}"), img);
        let path = dir.join(name);
        std::fs::write(&path, &img.data).map_err(|e| e.to_string())?;
        if let Some(full) = full {
            std::fs::write(crate::chat::full_image_path(&path), full).map_err(|e| e.to_string())?;
        }
        let prompt = prompts.get(n).copied().unwrap_or(prompts[0]);
        listed.push(json!({"path": path, "source": format!("Generated: {prompt}")}));
        outs.push(Out::Image { data: img.data, mime: img.mime });
    }
    outs.push(Out::Text(json!({"images": listed, "note": "Place one with <img src=\"<path>\"> in write_html; it is copied into the project."}).to_string()));
    Ok(outs)
}

/// What Codex is asked: only the images, no commands or files of its own.
fn codex_prompt(prompts: &[&str], aspect: &str) -> String {
    let list = prompts.iter().enumerate().map(|(i, p)| format!("{}. {p}", i + 1)).collect::<Vec<_>>().join("\n");
    format!(
        "Use your image generation tool to make {n} image{s}, one per description below, each {aspect}, with no text, letters or logos in the image. Do not run commands or edit files. When done, reply with only the word done.\n\n{list}",
        n = prompts.len(),
        s = if prompts.len() == 1 { "" } else { "s" },
    )
}

/// Runs `codex exec --json` and returns the images its thread generated, oldest first.
fn run_codex(bin: &Path, prompt: &str) -> Result<Vec<PathBuf>, String> {
    let scratch = std::env::temp_dir().join(format!("papercraft-imagegen-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(bin);
    cmd.env("PATH", crate::chat::path::resolved_path());
    for k in CREDENTIAL_ENV {
        cmd.env_remove(k);
    }
    let mut child = cmd
        .args(["exec", "--json", "--skip-git-repo-check", "-s", "read-only", "-C"])
        .arg(&scratch)
        .arg(prompt)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start Codex: {e}"))?;
    let started = Instant::now();
    while child.try_wait().map_err(|e| e.to_string())?.is_none() {
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            return Err(format!("Codex took longer than {} minutes", TIMEOUT.as_secs() / 60));
        }
        std::thread::sleep(POLL);
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let events = String::from_utf8_lossy(&out.stdout);
    let thread = thread_id(&events).ok_or_else(|| {
        let err = String::from_utf8_lossy(&out.stderr);
        format!("Codex failed: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output"))
    })?;
    let home = std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| dirs::home_dir().map(|h| h.join(".codex"))).ok_or("no home folder")?;
    let mut images: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(home.join("generated_images").join(&thread))
        .map(|d| d.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "png")).filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p))).collect())
        .unwrap_or_default();
    images.sort();
    Ok(images.into_iter().map(|(_, p)| p).collect())
}

/// The user's command for one prompt, in a fresh folder; the images it wrote there.
fn run_command(template: &str, prompt: &str, aspect: &str) -> Result<Vec<PathBuf>, String> {
    let out = std::env::temp_dir().join(format!("papercraft-imagegen-{}-{}-{:08x}", std::process::id(), crate::project::format::now_ms(), fastrand::u32(..)));
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let script = shell_script(template);
    let mut child = Command::new("/bin/sh")
        .args(["-c", &script])
        .env("PAPERCRAFT_PROMPT", prompt)
        .env("PAPERCRAFT_ASPECT", aspect)
        .env("PAPERCRAFT_RATIO", ratio(aspect))
        .env("PAPERCRAFT_OUT", &out)
        // An app opened from Finder has a bare PATH: give the command the user's shell PATH.
        .env("PATH", crate::chat::path::resolved_path())
        .current_dir(&out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start the image command: {e}"))?;
    let started = Instant::now();
    while child.try_wait().map_err(|e| e.to_string())?.is_none() {
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            return Err(format!("the image command took longer than {} minutes", TIMEOUT.as_secs() / 60));
        }
        std::thread::sleep(POLL);
    }
    let done = child.wait_with_output().map_err(|e| e.to_string())?;
    if !done.status.success() {
        let err = String::from_utf8_lossy(&done.stderr);
        return Err(format!("the image command failed: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message")));
    }
    let mut images: Vec<PathBuf> = std::fs::read_dir(&out)
        .map(|d| d.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().and_then(|x| x.to_str()).is_some_and(|x| IMAGE_EXTS.contains(&x.to_ascii_lowercase().as_str()))).collect())
        .unwrap_or_default();
    images.sort();
    Ok(images)
}

/// The template with its placeholders turned into quoted variable references.
fn shell_script(template: &str) -> String {
    template
        .replace("{prompt}", "\"$PAPERCRAFT_PROMPT\"")
        .replace("{aspect}", "\"$PAPERCRAFT_ASPECT\"")
        .replace("{ratio}", "\"$PAPERCRAFT_RATIO\"")
        .replace("{out}", "\"$PAPERCRAFT_OUT\"")
}

/// The thread id of a `codex exec --json` run (`thread.started`).
fn thread_id(events: &str) -> Option<String> {
    events
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["type"] == "thread.started")
        .and_then(|v| v["thread_id"].as_str().filter(|t| crate::chat::valid_id(t)).map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thread_comes_from_the_event_stream() {
        let events = "noise\n{\"type\":\"thread.started\",\"thread_id\":\"01a16f48-c2af\"}\n{\"type\":\"turn.started\"}";
        assert_eq!(thread_id(events).as_deref(), Some("01a16f48-c2af"));
        assert_eq!(thread_id("{\"type\":\"thread.started\",\"thread_id\":\"../etc\"}"), None, "only a plain id becomes a folder name");
    }

    #[test]
    fn prompts_are_numbered_and_capped() {
        let p = codex_prompt(&["a phoenix", "a drake"], "portrait (2:3)");
        assert!(p.contains("2 images") && p.contains("1. a phoenix\n2. a drake") && p.contains("portrait (2:3)"));
        let dir = std::env::temp_dir();
        assert!(generate(Some(&dir), None, &Provider::Codex, &json!({"prompts": []})).is_err());
        assert!(generate(Some(&dir), None, &Provider::Codex, &json!({"prompts": ["a", "b", "c", "d", "e"]})).is_err());
        let off = generate(Some(&dir), None, &Provider::Off, &json!({"prompts": ["a"]})).unwrap_err();
        assert!(off.contains("off"), "{off}");
    }

    #[test]
    fn a_custom_command_gets_the_prompt_as_data_not_as_shell() {
        // The prompt tries to run a command; it must arrive as plain text.
        let prompt = "dragon\"; touch pwned; echo \"";
        let template = "printf '%s|%s|%s' {prompt} {aspect} {ratio} > {out}/args.txt && cp args.txt {out}/x.png";
        let images = run_command(template, prompt, "portrait").unwrap();
        let written = std::fs::read_to_string(images[0].with_file_name("args.txt")).unwrap();
        assert_eq!(written, format!("{prompt}|portrait|3:4"));
        assert!(!images[0].with_file_name("pwned").exists(), "nothing ran from the prompt");
        assert!(run_command("exit 3", "a", "square").is_err());
    }
}
