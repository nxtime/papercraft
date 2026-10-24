//! The logic behind `papercraft-cli` that doesn't need a network: running tools in-process
//! when the app is closed (`--offline`), the safety check against a second writer, and parsing
//! shell scripts. The binary (`src/bin/papercraft-cli.rs`) does the I/O.

use crate::mcp::exec::{self, Host, SessionHost};
use crate::mcp::{Out, ToolResult, tools};
use crate::project::Store;
use base64::Engine;
use serde_json::{Value, json};
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Held (exclusive advisory lock) by a running app in its projects folder, from start-up on.
pub const APP_LOCK: &str = ".papercraft-app.lock";
/// Held by an offline CLI session in the project folder while it may write the project.
pub const PROJECT_LOCK: &str = ".papercraft-cli.lock";
/// How long an offline session waits for another one to release the project.
pub const PROJECT_LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(100);

fn lock_file(path: &Path) -> std::io::Result<File> {
    File::options().create(true).truncate(false).write(true).open(path)
}

/// Mark the app as running for `papercraft-cli` (see [`app_running`]); keep the file alive.
pub fn hold_app_lock(root: &Path) -> Option<File> {
    let f = lock_file(&root.join(APP_LOCK)).ok()?;
    f.try_lock().ok()?;
    Some(f)
}

/// An app holds the projects folder at `root` (it may still be starting its server).
pub fn app_running(root: &Path) -> bool {
    lock_file(&root.join(APP_LOCK))
        .is_ok_and(|f| matches!(f.try_lock(), Err(TryLockError::WouldBlock)))
}

/// Take the offline write lock of project `dir`, waiting up to `wait` for another holder.
pub fn lock_project(dir: &Path, wait: Duration) -> Result<File, String> {
    let path = dir.join(PROJECT_LOCK);
    let f = lock_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let start = Instant::now();
    loop {
        match f.try_lock() {
            Ok(()) => return Ok(f),
            Err(TryLockError::WouldBlock) if start.elapsed() < wait => std::thread::sleep(LOCK_POLL),
            Err(TryLockError::WouldBlock) => {
                return Err(format!(
                    "another papercraft-cli is writing {} (waited {} s); retry when it is done, or \
                     open the project in the app so calls go through it",
                    dir.display(),
                    wait.as_secs()
                ));
            }
            Err(TryLockError::Error(e)) => return Err(format!("{}: {e}", path.display())),
        }
    }
}

/// Tools that work with no project open, so they never trigger the default-project open.
const NO_PROJECT_TOOLS: [&str; 6] = [
    "create_file",
    "open_file",
    "list_resources",
    "get_guide",
    "get_font_family_info",
    "search_icons",
];

/// An in-process host on a project store: the same tools as the app, saved to disk after
/// every editing call.
pub struct Offline {
    pub host: SessionHost,
    /// The top-level node (artboard) the last editing call touched, for `:watch`.
    pub last_touched: Option<String>,
    /// Checked with `open_file`'s `fileId` before it runs (the binary refuses projects the app has open).
    pub open_guard: Option<Box<dyn Fn(&str) -> Result<(), String>>>,
    default_tried: bool,
    /// The project this session may write, and its lock ([`PROJECT_LOCK`]).
    lock: Option<(PathBuf, File)>,
}

impl Offline {
    /// `project` (name, id or path) is opened right away; without it the most recently modified
    /// project opens on the first call that needs one.
    pub fn new(store: Store, project: Option<&str>) -> Result<Offline, String> {
        let mut o = Offline {
            host: SessionHost::new(store),
            last_touched: None,
            open_guard: None,
            default_tried: project.is_some(),
            lock: None,
        };
        if let Some(p) = project {
            exec::call(&mut o.host, "open_file", &json!({"fileId": p}))?;
        }
        Ok(o)
    }

    pub fn call(&mut self, name: &str, args: &Value) -> ToolResult {
        if let ("open_file", Some(g), Some(id)) = (name, &self.open_guard, args["fileId"].as_str()) {
            g(id)?;
        }
        if !self.default_tried && !NO_PROJECT_TOOLS.contains(&name) {
            self.default_tried = true;
            self.open_default();
        }
        let edits = tools::edits_doc(name, args);
        if edits && self.host.project().is_none() {
            return Err("no project is open; run create_file, then open_file".into());
        }
        if edits {
            self.lock_for_write()?;
        }
        let r = exec::call(&mut self.host, name, args)?;
        if edits {
            self.host.save()?;
            self.last_touched = touched_key(args, &r).and_then(|k| self.artboard_of(&k));
        }
        Ok(r)
    }

    /// Before the first write to a project: take its lock, then reload it, so writes another
    /// session saved before the lock was ours are not overwritten.
    fn lock_for_write(&mut self) -> Result<(), String> {
        let Some(dir) = self.host.project().map(|p| p.dir.clone()) else {
            return Ok(());
        };
        if self.lock.as_ref().is_some_and(|(d, _)| *d == dir) {
            return Ok(());
        }
        let f = lock_project(&dir, PROJECT_LOCK_WAIT)?;
        self.lock = Some((dir.clone(), f));
        let page = self.host.ed.active_page_id().to_string();
        self.host
            .session
            .open(&mut self.host.ed, &dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        if let Ok(i) = exec::find_page(&self.host.ed, &page) {
            self.host.ed.switch_page(i);
        }
        Ok(())
    }

    fn open_default(&mut self) {
        if self.host.project().is_some() {
            return;
        }
        if let Some(e) = self.host.store().list().first() {
            let id = e.manifest.id.clone();
            let _ = exec::call(&mut self.host, "open_file", &json!({"fileId": id}));
        }
    }

    /// Make a page (id, id prefix or name) the active one.
    pub fn switch_page(&mut self, key: &str) -> Result<String, String> {
        let i = exec::find_page(&self.host.ed, key)?;
        self.host.ed.switch_page(i);
        Ok(self.host.ed.pages[i].name.clone())
    }

    /// The top-level ancestor of a `"<page>:<n>"` node key.
    fn artboard_of(&mut self, key: &str) -> Option<String> {
        let ed = &self.host.ed;
        let (pfx, n) = key.split_once(':')?;
        let page = exec::find_page(ed, pfx).ok()?;
        let doc = ed.page_doc(page);
        let mut id = crate::model::NodeId(n.parse().ok()?);
        doc.get(id)?;
        while let Some(p) = doc.get(id)?.parent {
            id = p;
        }
        Some(exec::node_key(ed, page, id))
    }
}

/// The node an editing call was about: from its arguments, else from its result.
pub fn touched_key(args: &Value, result: &[Out]) -> Option<String> {
    let from_args = ["nodeId", "targetNodeId"]
        .iter()
        .find_map(|k| args[k].as_str())
        .or_else(|| args["nodeIds"][0].as_str());
    if let Some(k) = from_args {
        return Some(k.to_string());
    }
    let Some(Out::Text(t)) = result.first() else {
        return None;
    };
    let v: Value = serde_json::from_str(t).ok()?;
    ["id", "nodeId"]
        .iter()
        .find_map(|k| v[k].as_str())
        .or_else(|| v["ids"][0].as_str())
        .map(String::from)
}

/// `--offline` with the app running: refuse when the app has the target project open.
/// `items` is the app's `list_resources` result; `target` is `--project`, or None for the
/// default (most recently modified, the first item).
pub fn refuse_if_open(items: &Value, target: Option<&str>) -> Result<(), String> {
    let list = items["items"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let canon = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
    let hit = match target {
        None => list.first(),
        Some(t) => {
            let t = t.trim().trim_start_matches("/file/");
            let p = |i: &Value| i["path"].as_str().map(String::from);
            list.iter()
                .find(|i| {
                    i["id"] == t
                        || i["name"] == t
                        || p(i).is_some_and(|p| p == t || canon(&p) == canon(t))
                })
                .or_else(|| {
                    list.iter().find(|i| {
                        i["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(t))
                            || p(i)
                                .is_some_and(|p| Path::new(&p).file_name().is_some_and(|f| f == t))
                    })
                })
        }
    };
    if hit.is_some_and(|i| i["open"] == true) {
        return Err("PaperCraft has this project open; close it or drop --offline".into());
    }
    Ok(())
}

// -------------------------------------------------------------------------------- scripts

#[derive(Debug, PartialEq)]
pub enum Stmt {
    Call { tool: String, args: Value },
    Cmd { name: String, args: Vec<String> },
}

/// True when every `{` / `[` outside a string is closed.
pub fn balanced(s: &str) -> bool {
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for c in s.chars() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            _ => {}
        }
    }
    depth <= 0 && !in_str
}

/// Collects lines into statements: a command is one line; a tool call continues over lines
/// until its JSON closes. Blank lines and `#` comments between statements are skipped.
#[derive(Default)]
pub struct Accum {
    buf: String,
    start: usize,
}

impl Accum {
    /// Feed one line (`line_no` counts from 1). Returns a finished statement and its first line.
    pub fn push(&mut self, line_no: usize, line: &str) -> Option<(usize, String)> {
        if self.buf.is_empty() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                return None;
            }
            self.start = line_no;
            self.buf = t.to_string();
            if t.starts_with(':') {
                return Some((self.start, std::mem::take(&mut self.buf)));
            }
        } else {
            self.buf.push('\n');
            self.buf.push_str(line);
        }
        balanced(&self.buf).then(|| (self.start, std::mem::take(&mut self.buf)))
    }

    /// A statement is half-read (the shell shows a continuation prompt).
    pub fn pending(&self) -> bool {
        !self.buf.is_empty()
    }

    /// At end of input: an unfinished statement (it will fail to parse).
    pub fn finish(&mut self) -> Option<(usize, String)> {
        (!self.buf.is_empty()).then(|| (self.start, std::mem::take(&mut self.buf)))
    }
}

pub fn parse_statement(s: &str) -> Result<Stmt, String> {
    let s = s.trim();
    if let Some(cmd) = s.strip_prefix(':') {
        let mut w = cmd.split_whitespace().map(String::from);
        let name = w.next().ok_or("empty command")?;
        return Ok(Stmt::Cmd {
            name,
            args: w.collect(),
        });
    }
    let (tool, rest) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
    let rest = rest.trim();
    let args = if rest.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(rest).map_err(|e| format!("arguments are not valid JSON: {e}"))?
    };
    Ok(Stmt::Call {
        tool: tool.into(),
        args,
    })
}

/// A whole script: `(first line number, parsed statement)`.
pub fn parse_script(text: &str) -> Vec<(usize, Result<Stmt, String>)> {
    let mut acc = Accum::default();
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some((at, s)) = acc.push(i + 1, line) {
            out.push((at, parse_statement(&s)));
        }
    }
    if let Some((at, s)) = acc.finish() {
        out.push((at, parse_statement(&s)));
    }
    out
}

// -------------------------------------------------------------------------------- images

const KITTY_CHUNK: usize = 4096;

/// The escape sequence that shows a PNG inline: iTerm2 / WezTerm (OSC 1337) or kitty; None
/// for any other terminal.
pub fn inline_image(term_program: &str, term: &str, png: &[u8]) -> Option<String> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(png);
    if ["iTerm.app", "WezTerm"].contains(&term_program) {
        return Some(format!(
            "\x1b]1337;File=inline=1;size={}:{b64}\x07\n",
            png.len()
        ));
    }
    if term != "xterm-kitty" {
        return None;
    }
    let chunks: Vec<&[u8]> = b64.as_bytes().chunks(KITTY_CHUNK).collect();
    let mut s = String::new();
    for (i, c) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        let head = if i == 0 { "a=T,f=100," } else { "" };
        s += &format!(
            "\x1b_G{head}m={more};{}\x1b\\",
            std::str::from_utf8(c).unwrap_or("")
        );
    }
    s.push('\n');
    Some(s)
}
