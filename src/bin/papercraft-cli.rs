//! `papercraft-cli`: drive the running papercraft app from a shell, with no MCP client or tool
//! schemas in an agent's context. Same tools as the MCP server, over the same endpoint
//! (`http://127.0.0.1:29980/mcp`, or `$PAPERCRAFT_MCP_PORT`).

use base64::Engine;
use papercraft::cli::{self, Accum, Offline, Stmt};
use papercraft::icons;
use papercraft::mcp::{self, Out, server, tools};
use papercraft::project::Store;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::{Value, json};
use std::io::{BufRead, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "\
papercraft-cli: call the running papercraft app's tools from a shell

USAGE:
  papercraft-cli list                       tool names and one-line descriptions (works offline)
  papercraft-cli schema <tool>              a tool's full description and JSON input schema
  papercraft-cli call <tool> [JSON|-]       run a tool; JSON arguments inline, or `-` for stdin
        [-o|--out FILE]                   where to save an image result (default: <tool>.png, or {\"out\": \"FILE\"})
        [--offline] [--project NAME|ID|PATH]   work without the app, see below
  papercraft-cli shell                      interactive session; reads lines from a pipe too (see below)
  papercraft-cli icons list                 installed and installable open icon sets
  papercraft-cli icons add <set>...         download sets (lucide tabler phosphor heroicons material)
  papercraft-cli icons search <words> [--set S]   find `set:name` references (works offline)
  papercraft-cli --url                      print the endpoint it talks to

Icons are placed with <x-paper-icon name='lucide:check' size='20' color='var(--text)'/> in
write_html. Sets are fetched from the npm registry with curl and tar into the icons dir
($PAPERCRAFT_ICONS, else the user data dir) together with their licence file.

Text results go to stdout; image results are saved to a file and the path is printed.
Exit status is 1 when the tool reports an error.

With the app running and a project open, calls go to the app. Otherwise (app not running, or
--offline) the tools run in this process on the project store ($PAPERCRAFT_HOME, else the
default folder) and every editing call is saved. --project opens that project first; without it
the most recently modified one is used (create_file and open_file work as usual). --offline is
refused while the app has the same project open.

shell [--project X] [--offline]: one session, many calls. A line is `<tool> <json>` (JSON may
span lines until its braces close) or a command:
  :help  :tools  :schema <tool>  :open <project>  :pages  :page <name|id>  :shot [nodeId] [file]
  :undo  :redo  :save  :watch on|off  :quit
:watch re-renders the artboard last edited to preview.png after each editing call (shown inline
on iTerm2, WezTerm and kitty). Blank lines and lines starting with # are skipped. Input is plain
stdin (no line editing, no history; wrap with rlwrap if you want it). Piped input prints no
prompt and exits 1 when any line failed, listing the failed line numbers on stderr:
  papercraft-cli shell < script.pc
";

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("papercraft-cli: {msg}");
    ExitCode::FAILURE
}

fn list() {
    for t in server::tool_list() {
        let d = t.description.as_deref().unwrap_or("");
        let first = d.split(['.', '\n']).next().unwrap_or("");
        println!("{}\t{first}", t.name);
    }
}

fn run(cmd: &mut std::process::Command, what: &str) -> Result<Vec<u8>, String> {
    let out = cmd
        .output()
        .map_err(|e| format!("{what}: cannot run {:?}: {e}", cmd.get_program()))?;
    if !out.status.success() {
        return Err(format!(
            "{what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// Download one icon set from the npm registry into the icons dir. Returns the icon count.
fn add_set(id: &str) -> Result<usize, String> {
    let spec = icons::spec(id).ok_or_else(|| {
        format!(
            "unknown set `{id}`; sets: {}",
            icons::SETS
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let meta = run(
        std::process::Command::new("curl").args([
            "-fsSL",
            &format!("https://registry.npmjs.org/{}/latest", spec.package),
        ]),
        "looking up the package",
    )?;
    let meta: Value = serde_json::from_slice(&meta).map_err(|e| e.to_string())?;
    let tarball = meta["dist"]["tarball"]
        .as_str()
        .ok_or("the registry reply has no tarball")?
        .to_string();
    let tmp = std::env::temp_dir().join(format!("papercraft-icons-{id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let archive = tmp.join("pkg.tgz");
    let result = (|| {
        run(
            std::process::Command::new("curl")
                .args(["-fsSL", "-o"])
                .arg(&archive)
                .arg(&tarball),
            "downloading",
        )?;
        run(
            std::process::Command::new("tar")
                .arg("-xzf")
                .arg(&archive)
                .arg("-C")
                .arg(&tmp),
            "unpacking",
        )?;
        let dest = icons::dir().join(id);
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        let mut n = 0;
        let src = tmp.join("package").join(spec.dir);
        for e in std::fs::read_dir(&src)
            .map_err(|e| format!("{}: {e}", src.display()))?
            .flatten()
        {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "svg") {
                std::fs::copy(&p, dest.join(e.file_name())).map_err(|e| e.to_string())?;
                n += 1;
            }
        }
        for lic in ["LICENSE", "LICENSE.md", "LICENSE.txt", "license"] {
            let p = tmp.join("package").join(lic);
            if p.is_file() {
                let _ = std::fs::copy(&p, dest.join("LICENSE"));
                break;
            }
        }
        Ok(n)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

fn icons_cmd(args: &[String]) -> ExitCode {
    let root = icons::dir();
    match args.first().map(String::as_str) {
        Some("list") => {
            println!("icons dir: {}", root.display());
            let have = icons::installed(&root);
            for s in icons::SETS {
                let n = have.iter().find(|(h, _)| h == s.id).map_or(0, |h| h.1);
                let mark = if n > 0 {
                    format!("{n} installed")
                } else {
                    "not installed".into()
                };
                println!("{}\t{}\t{}\t{mark}", s.id, s.license, s.about);
            }
            ExitCode::SUCCESS
        }
        Some("add") if args.len() > 1 => {
            let mut ok = true;
            for id in &args[1..] {
                match add_set(id) {
                    Ok(n) => println!("{id}: {n} icons in {}", root.join(id).display()),
                    Err(e) => {
                        eprintln!("papercraft-cli: {id}: {e}");
                        ok = false;
                    }
                }
            }
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Some("search") if args.len() > 1 => {
            let set = args
                .iter()
                .position(|a| a == "--set")
                .and_then(|i| args.get(i + 1));
            let words: Vec<&str> = args[1..]
                .iter()
                .map(String::as_str)
                .filter(|a| *a != "--set" && Some(*a) != set.map(String::as_str))
                .collect();
            for r in icons::search(&root, &words.join(" "), set.map(String::as_str), 40) {
                println!("{r}");
            }
            ExitCode::SUCCESS
        }
        _ => fail("usage: icons list | icons add <set>... | icons search <words> [--set S]"),
    }
}

fn schema(name: &str) -> bool {
    let Some(t) = server::tool_list().into_iter().find(|t| t.name == name) else {
        eprintln!("papercraft-cli: no tool named {name} (see `papercraft-cli list`)");
        return false;
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::to_value(&t).unwrap_or(Value::Null))
            .unwrap_or_default()
    );
    true
}

const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

type Client = rmcp::service::RunningService<rmcp::service::RoleClient, ()>;

/// Where tools run: the app over HTTP, or this process.
enum Backend {
    Online(tokio::runtime::Runtime, Client),
    Offline(Box<Offline>),
}

impl Backend {
    /// The tool's output and whether it succeeded. Errors reach the caller as text, like the
    /// app's own error results.
    fn call(&mut self, name: &str, args: &Value) -> Result<(Vec<Out>, bool), String> {
        match self {
            Backend::Offline(o) => Ok(match o.call(name, args) {
                Ok(r) => (r, true),
                Err(e) => (vec![Out::Text(e)], false),
            }),
            Backend::Online(rt, client) => rt.block_on(online_call(client, name, args)),
        }
    }
}

async fn online_call(
    client: &Client,
    name: &str,
    args: &Value,
) -> Result<(Vec<Out>, bool), String> {
    let mut p = CallToolRequestParams::new(name.to_string());
    match args {
        Value::Object(m) => p.arguments = Some(m.clone()),
        Value::Null => {}
        _ => return Err("arguments must be a JSON object".into()),
    }
    let r = client.call_tool(p).await.map_err(|e| e.to_string())?;
    let mut outs = Vec::new();
    // Blocks are read through their JSON form, so this doesn't depend on rmcp's enum layout.
    for block in r.content {
        let v = serde_json::to_value(&block).map_err(|e| e.to_string())?;
        outs.push(match v.get("type").and_then(Value::as_str) {
            Some("text") => Out::Text(v["text"].as_str().unwrap_or("").to_string()),
            Some("image") => Out::Image {
                data: base64::engine::general_purpose::STANDARD
                    .decode(v["data"].as_str().unwrap_or(""))
                    .map_err(|e| format!("bad image data: {e}"))?,
                mime: v["mimeType"].as_str().unwrap_or("image/png").to_string(),
            },
            _ => Out::Text(v.to_string()),
        });
    }
    Ok((outs, r.is_error != Some(true)))
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
}

fn connect(rt: &tokio::runtime::Runtime) -> Result<Client, String> {
    let url = mcp::url(mcp::port());
    rt.block_on(async { ().serve(StreamableHttpClientTransport::from_uri(url.clone())).await })
        .map_err(|e| format!("cannot reach papercraft at {url}: {e}. Is the app running?"))
}

fn app_reachable() -> bool {
    use std::net::{Ipv4Addr, SocketAddr, TcpStream};
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, mcp::port()));
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok()
}

/// The app's `list_resources` result: `Ok(None)` when it isn't running, `Err` when it runs but
/// does not answer usably (the caller must refuse then, not guess).
fn app_items() -> Result<Option<Value>, String> {
    if !app_reachable() {
        return Ok(None);
    }
    let rt = runtime()?;
    let client = connect(&rt)?;
    let (outs, _) = rt.block_on(online_call(&client, "list_resources", &json!({"limit": LIST_ALL})))?;
    match outs.first() {
        Some(Out::Text(t)) => serde_json::from_str(t)
            .map(Some)
            .map_err(|e| format!("unreadable list_resources answer: {e}")),
        _ => Err("empty list_resources answer".into()),
    }
}

/// Large enough to list every project in one answer.
const LIST_ALL: u32 = 100_000;

/// Refuse to open `target` here while the app has it open, or when that cannot be told.
fn check_app_not_using(target: Option<&str>) -> Result<(), String> {
    match app_items() {
        Ok(Some(items)) => cli::refuse_if_open(&items, target),
        Ok(None) => Ok(()),
        Err(e) => Err(format!("cannot tell whether PaperCraft has the project open ({e}); close the app or retry")),
    }
}

fn open_offline(project: Option<&str>) -> Result<Backend, String> {
    let root = Store::default_root();
    let store = Store::open(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    let mut o = Offline::new(store, project)?;
    o.open_guard = Some(Box::new(|id| check_app_not_using(Some(id))));
    Ok(Backend::Offline(Box::new(o)))
}

/// `check_default`: also refuse when no `--project` is given and the app has the default
/// project open (false for a lone create_file, which touches no existing project).
fn open_backend(
    offline: bool,
    project: Option<&str>,
    check_default: bool,
) -> Result<Backend, String> {
    if offline {
        if project.is_some() || check_default {
            check_app_not_using(project)?;
        }
        return open_offline(project);
    }
    if app_reachable() {
        let rt = runtime()?;
        let client = connect(&rt)?;
        if let Some(p) = project {
            rt.block_on(online_call(&client, "open_file", &json!({"fileId": p})))?;
        }
        return Ok(Backend::Online(rt, client));
    }
    eprintln!("papercraft-cli: app not running; working offline");
    open_offline(project)
}

/// Print a tool's output: text to stdout, images to a file whose path is printed. Returns the
/// saved image bytes (for inline display).
fn emit(outs: &[Out], name: &str, out: Option<&str>) -> Result<Vec<Vec<u8>>, String> {
    let mut images = Vec::new();
    for o in outs {
        match o {
            Out::Text(t) => println!("{t}"),
            Out::Image { data, .. } => {
                let path = out.map_or_else(|| format!("{name}.png"), String::from);
                save(&path, data)?;
                println!("{path}");
                images.push(data.clone());
            }
        }
    }
    Ok(images)
}

fn save(path: &str, data: &[u8]) -> Result<(), String> {
    if let Some(p) = std::path::Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        let _ = std::fs::create_dir_all(p);
    }
    std::fs::write(path, data).map_err(|e| format!("{path}: {e}"))
}

fn out_arg(args: &Value, out: Option<String>) -> Option<String> {
    out.or_else(|| {
        args.get("out")
            .or_else(|| args.get("path"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from)
    })
}

fn call_once(
    name: String,
    args: Value,
    out: Option<String>,
    offline: bool,
    project: Option<String>,
) -> Result<bool, String> {
    let out = out_arg(&args, out);
    let mut be = open_backend(
        offline,
        project.as_deref(),
        name != "create_file" && name != "open_file",
    )?;
    let (outs, ok) = be.call(&name, &args)?;
    emit(&outs, &name, out.as_deref())?;
    Ok(ok)
}

// ---------------------------------------------------------------------------------- shell

const SHELL_HELP: &str = "\
<tool> <json>        run a tool (JSON may span lines until its braces close)
:tools               tool names        :schema <tool>   one tool's schema
:open <project>      open a project    :pages           pages (get_basic_info)
:page <name|id>      switch page (offline only)
:shot [nodeId] [file]  screenshot to file (default shot.png)
:undo  :redo  :save  :watch on|off  :quit";

const WATCH_FILE: &str = "preview.png";

struct Shell {
    be: Backend,
    offline: bool,
    watch: bool,
    last: Option<String>,
}

impl Shell {
    fn show(&self, png: &[u8]) {
        if !std::io::stdout().is_terminal() {
            return;
        }
        let tp = std::env::var("TERM_PROGRAM").unwrap_or_default();
        let term = std::env::var("TERM").unwrap_or_default();
        if let Some(seq) = cli::inline_image(&tp, &term, png) {
            print!("{seq}");
        }
    }

    fn call(&mut self, name: &str, args: &Value, out: Option<&str>) -> bool {
        let (outs, ok) = match self.be.call(name, args) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("papercraft-cli: {e}");
                return false;
            }
        };
        let images = match emit(&outs, name, out) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("papercraft-cli: {e}");
                return false;
            }
        };
        for png in &images {
            self.show(png);
        }
        if ok && tools::edits_doc(name, args) {
            self.last = match &self.be {
                Backend::Offline(o) => o.last_touched.clone(),
                Backend::Online(..) => cli::touched_key(args, &outs),
            }
            .or(self.last.take());
            if self.watch {
                self.preview();
            }
        }
        ok
    }

    fn preview(&mut self) {
        let args = match &self.last {
            Some(k) => json!({"nodeId": k}),
            None => Value::Null,
        };
        match self.be.call("get_screenshot", &args) {
            Ok((outs, true)) => {
                for o in outs {
                    if let Out::Image { data, .. } = o {
                        match save(WATCH_FILE, &data) {
                            Ok(()) => {
                                println!("{WATCH_FILE}");
                                self.show(&data);
                            }
                            Err(e) => eprintln!("papercraft-cli: {e}"),
                        }
                    }
                }
            }
            Ok((outs, false)) => {
                for o in outs {
                    if let Out::Text(t) = o {
                        eprintln!("papercraft-cli: watch: {t}");
                    }
                }
            }
            Err(e) => eprintln!("papercraft-cli: watch: {e}"),
        }
    }

    /// Run one statement. `None` means quit.
    fn run(&mut self, stmt: Stmt) -> Option<bool> {
        let (name, a) = match stmt {
            Stmt::Call { tool, args } => {
                let out = out_arg(&args, None);
                return Some(self.call(&tool, &args, out.as_deref()));
            }
            Stmt::Cmd { name, args } => (name, args),
        };
        let arg = a.first().map(String::as_str);
        Some(match (name.as_str(), arg) {
            ("quit" | "q" | "exit", _) => return None,
            ("help" | "h", _) => {
                println!("{SHELL_HELP}");
                true
            }
            ("tools", _) => {
                list();
                true
            }
            ("schema", Some(t)) => schema(t),
            ("open", Some(p)) => {
                if self.offline
                    && let Err(e) = check_app_not_using(Some(p))
                {
                    eprintln!("papercraft-cli: {e}");
                    return Some(false);
                }
                self.call("open_file", &json!({"fileId": p}), None)
            }
            ("pages", _) => self.call("get_basic_info", &Value::Null, None),
            ("page", Some(p)) => match &mut self.be {
                Backend::Offline(o) => match o.switch_page(p) {
                    Ok(n) => {
                        println!("page {n}");
                        true
                    }
                    Err(e) => {
                        eprintln!("papercraft-cli: {e}");
                        false
                    }
                },
                Backend::Online(..) => {
                    eprintln!("papercraft-cli: :page works offline only; switch pages in the app");
                    false
                }
            },
            ("shot", _) => {
                // A node id has a colon or is a number; anything else is the file.
                let is_node = |s: &str| s.contains(':') || s.chars().all(|c| c.is_ascii_digit());
                let (node, file) = match (a.first(), a.get(1)) {
                    (Some(n), f) if is_node(n) => (Some(n.clone()), f.cloned()),
                    (f, _) => (None, f.cloned()),
                };
                let args = node.map_or(Value::Null, |n| json!({"nodeId": n}));
                let file = file.unwrap_or_else(|| "shot.png".into());
                self.call("get_screenshot", &args, Some(&file))
            }
            ("undo", _) => self.call("undo", &Value::Null, None),
            ("redo", _) => self.call("redo", &Value::Null, None),
            ("save", _) => self.call("finish_working_on_nodes", &Value::Null, None),
            ("watch", Some("on")) => {
                self.watch = true;
                true
            }
            ("watch", Some("off")) => {
                self.watch = false;
                true
            }
            _ => {
                eprintln!("papercraft-cli: unknown or incomplete command :{name} (try :help)");
                false
            }
        })
    }
}

fn shell(offline: bool, project: Option<String>) -> ExitCode {
    let be = match open_backend(offline, project.as_deref(), true) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let mut sh = Shell {
        be,
        offline,
        watch: false,
        last: None,
    };
    let tty = std::io::stdin().is_terminal();
    let mut acc = Accum::default();
    let mut failed: Vec<usize> = Vec::new();
    let mut n = 0;
    let mut quit = false;
    let mut run = |sh: &mut Shell, at: usize, s: String| {
        match cli::parse_statement(&s) {
            Ok(stmt) => match sh.run(stmt) {
                None => return true,
                Some(false) => failed.push(at),
                Some(true) => {}
            },
            Err(e) => {
                eprintln!("papercraft-cli: line {at}: {e}");
                failed.push(at);
            }
        }
        false
    };
    let prompt = |acc: &Accum| {
        if tty {
            print!(
                "{}",
                if acc.pending() {
                    "...> "
                } else {
                    "papercraft> "
                }
            );
            let _ = std::io::stdout().flush();
        }
    };
    prompt(&acc);
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        n += 1;
        if let Some((at, s)) = acc.push(n, &line)
            && run(&mut sh, at, s)
        {
            quit = true;
            break;
        }
        prompt(&acc);
    }
    if !quit && let Some((at, s)) = acc.finish() {
        run(&mut sh, at, s);
    }
    if failed.is_empty() || tty {
        return ExitCode::SUCCESS;
    }
    eprintln!(
        "papercraft-cli: failed lines: {}",
        failed
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    ExitCode::FAILURE
}

fn take_flag(args: &mut Vec<String>, long: &str, short: Option<&str>) -> Option<String> {
    let eq = format!("{long}=");
    let mut i = 0;
    while i < args.len() {
        if (args[i] == long || Some(args[i].as_str()) == short) && i + 1 < args.len() {
            return Some(args.drain(i..=i + 1).nth(1).unwrap());
        }
        if let Some(v) = args[i].strip_prefix(&eq) {
            let v = v.to_string();
            args.remove(i);
            return Some(v);
        }
        i += 1;
    }
    None
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let out = take_flag(&mut args, "--out", Some("-o"));
    let project = take_flag(&mut args, "--project", None);
    let offline = args.iter().any(|a| a == "--offline");
    args.retain(|a| a != "--offline");
    match args.first().map(String::as_str) {
        Some("list") => list(),
        Some("schema") => match args.get(1) {
            Some(n) => {
                return if schema(n) {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                };
            }
            None => return fail("schema needs a tool name"),
        },
        Some("icons") => return icons_cmd(&args[1..]),
        Some("--url") => println!("{}", mcp::url(mcp::port())),
        Some("shell") => return shell(offline, project),
        Some("call") => {
            let Some(name) = args.get(1).cloned() else {
                return fail("call needs a tool name");
            };
            let json = match args.get(2).map(String::as_str) {
                None => String::new(),
                Some("-") => {
                    let mut s = String::new();
                    if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                        return fail(e);
                    }
                    s
                }
                Some(s) => s.to_string(),
            };
            let value = if json.trim().is_empty() {
                Value::Null
            } else {
                match serde_json::from_str(&json) {
                    Ok(v) => v,
                    Err(e) => return fail(format!("arguments are not valid JSON: {e}")),
                }
            };
            return match call_once(name, value, out, offline, project) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::FAILURE,
                Err(e) => fail(e),
            };
        }
        _ => {
            print!("{USAGE}");
            return if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
    }
    ExitCode::SUCCESS
}
