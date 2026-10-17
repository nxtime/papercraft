//! `papercraft-cli`: drive the running papercraft app from a shell, with no MCP client or tool
//! schemas in an agent's context. Same tools as the MCP server, over the same endpoint
//! (`http://127.0.0.1:29980/mcp`, or `$PAPERCRAFT_MCP_PORT`).

use base64::Engine;
use papercraft::icons;
use papercraft::mcp::{self, server};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::Value;
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "\
papercraft-cli: call the running papercraft app's tools from a shell

USAGE:
  papercraft-cli list                       tool names and one-line descriptions (works offline)
  papercraft-cli schema <tool>              a tool's full description and JSON input schema
  papercraft-cli call <tool> [JSON|-]       run a tool; JSON arguments inline, or `-` for stdin
        [-o|--out FILE]                   where to save an image result (default: <tool>.png, or {\"out\": \"FILE\"})
  papercraft-cli icons list                 installed and installable open icon sets
  papercraft-cli icons add <set>...         download sets (lucide tabler phosphor heroicons material)
  papercraft-cli icons search <words> [--set S]   find `set:name` references (works offline)
  papercraft-cli --url                      print the endpoint it talks to

Icons are placed with <x-paper-icon name='lucide:check' size='20' color='var(--text)'/> in
write_html. Sets are fetched from the npm registry with curl and tar into the icons dir
($PAPERCRAFT_ICONS, else the user data dir) together with their licence file.

Text results go to stdout; image results are saved to a file and the path is printed.
Exit status is 1 when the tool reports an error or the app isn't running.
Start the app and open a project first: it serves the tools while open.
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

fn schema(name: &str) -> ExitCode {
    let Some(t) = server::tool_list().into_iter().find(|t| t.name == name) else {
        return fail(format!("no tool named {name} (see `papercraft-cli list`)"));
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::to_value(&t).unwrap_or(Value::Null))
            .unwrap_or_default()
    );
    ExitCode::SUCCESS
}

async fn call(name: String, args: Value, out: Option<String>) -> Result<bool, String> {
    let out = out.or_else(|| {
        args.get("out")
            .or_else(|| args.get("path"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from)
    });
    let url = mcp::url(mcp::port());
    let client = ()
        .serve(StreamableHttpClientTransport::from_uri(url.clone()))
        .await
        .map_err(|e| format!("cannot reach papercraft at {url}: {e}. Is the app running?"))?;
    let mut p = CallToolRequestParams::new(name.clone());
    match args {
        Value::Object(m) => p.arguments = Some(m),
        Value::Null => {}
        _ => return Err("arguments must be a JSON object".into()),
    }
    let r = client.call_tool(p).await.map_err(|e| e.to_string())?;
    // Blocks are read through their JSON form, so this doesn't depend on rmcp's enum layout.
    for block in r.content {
        let v = serde_json::to_value(&block).map_err(|e| e.to_string())?;
        match v.get("type").and_then(Value::as_str) {
            Some("text") => println!("{}", v["text"].as_str().unwrap_or("")),
            Some("image") => {
                let data = base64::engine::general_purpose::STANDARD
                    .decode(v["data"].as_str().unwrap_or(""))
                    .map_err(|e| format!("bad image data: {e}"))?;
                let path = out.clone().unwrap_or_else(|| format!("{name}.png"));
                if let Some(p) = std::path::Path::new(&path)
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                {
                    let _ = std::fs::create_dir_all(p);
                }
                std::fs::write(&path, data).map_err(|e| format!("{path}: {e}"))?;
                println!("{path}");
            }
            _ => println!("{v}"),
        }
    }
    Ok(r.is_error != Some(true))
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut out: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "--out" || args[i] == "-o") && i + 1 < args.len() {
            out = Some(args.drain(i..=i + 1).nth(1).unwrap());
        } else if let Some(v) = args[i].strip_prefix("--out=") {
            let v = v.to_string();
            args.remove(i);
            out = Some(v);
        } else {
            i += 1;
        }
    }
    match args.first().map(String::as_str) {
        Some("list") => list(),
        Some("schema") => match args.get(1) {
            Some(n) => return schema(n),
            None => return fail("schema needs a tool name"),
        },
        Some("icons") => return icons_cmd(&args[1..]),
        Some("--url") => println!("{}", mcp::url(mcp::port())),
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
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => return fail(e),
            };
            return match rt.block_on(call(name, value, out)) {
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
