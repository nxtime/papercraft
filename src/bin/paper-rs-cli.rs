//! `paper-rs-cli`: drive the running paper-rs app from a shell, with no MCP client or tool
//! schemas in an agent's context. Same tools as the MCP server, over the same endpoint
//! (`http://127.0.0.1:29980/mcp`, or `$PAPER_RS_MCP_PORT`).

use base64::Engine;
use paper_rs::mcp::{self, server};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::StreamableHttpClientTransport;
use serde_json::Value;
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "\
paper-rs-cli: call the running paper-rs app's tools from a shell

USAGE:
  paper-rs-cli list                       tool names and one-line descriptions (works offline)
  paper-rs-cli schema <tool>              a tool's full description and JSON input schema
  paper-rs-cli call <tool> [JSON|-]       run a tool; JSON arguments inline, or `-` for stdin
        [--out FILE]                      where to save an image result (default: <tool>.png)
  paper-rs-cli --url                      print the endpoint it talks to

Text results go to stdout; image results are saved to a file and the path is printed.
Exit status is 1 when the tool reports an error or the app isn't running.
Start the app and open a project first: it serves the tools while open.
";

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("paper-rs-cli: {msg}");
    ExitCode::FAILURE
}

fn list() {
    for t in server::tool_list() {
        let d = t.description.as_deref().unwrap_or("");
        let first = d.split(['.', '\n']).next().unwrap_or("");
        println!("{}\t{first}", t.name);
    }
}

fn schema(name: &str) -> ExitCode {
    let Some(t) = server::tool_list().into_iter().find(|t| t.name == name) else {
        return fail(format!("no tool named {name} (see `paper-rs-cli list`)"));
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::to_value(&t).unwrap_or(Value::Null))
            .unwrap_or_default()
    );
    ExitCode::SUCCESS
}

async fn call(name: String, args: Value, out: Option<String>) -> Result<bool, String> {
    let url = mcp::url(mcp::port());
    let client = ()
        .serve(StreamableHttpClientTransport::from_uri(url.clone()))
        .await
        .map_err(|e| format!("cannot reach paper-rs at {url}: {e}. Is the app running?"))?;
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
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| (i + 1 < args.len()).then(|| args.drain(i..=i + 1).nth(1)))
        .flatten();
    match args.first().map(String::as_str) {
        Some("list") => list(),
        Some("schema") => match args.get(1) {
            Some(n) => return schema(n),
            None => return fail("schema needs a tool name"),
        },
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
