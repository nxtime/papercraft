//! `papercraft-relay`: serves MCP for a PaperCraft tab running in a browser.
//!
//! The tab connects to `GET /tab/<token>` and an agent to `POST /s/<token>/mcp`; the relay
//! forwards tool calls between them and holds no document state. `--static` also serves the
//! built web app (`web/index.html` + `web/pkg`) from the same origin, so one process hosts it.

use papercraft::mcp::relay;
use papercraft::mcp::store::Data;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
papercraft-relay — MCP relay for a PaperCraft tab in a browser

USAGE:
  papercraft-relay [--listen 0.0.0.0:8787] [--static <dir>] [--allow-origin <origin>] [--data <dir>] [--library]

OPTIONS:
  --listen <addr>       address to serve on (default: $PAPERCRAFT_RELAY_LISTEN or 0.0.0.0:8787)
  --static <dir>        serve the built web app from <dir> (e.g. the repo's web/ directory)
  --allow-origin <o>    only let this browser origin connect a tab (default: any)
  --data <dir>          keep shared projects in <dir> (off without it): /p/{token}/...
  --library             with --data: a personal library. Every project in <dir> is listed at
                        /library and synced to every device that opens this server, with no
                        link needed, so only run it where the network is the boundary
  --help                print this text

The tab picks a 32..64 hex character token and shows the agent URL
  http://<host>:<port>/s/<token>/mcp
";

/// Address used when neither `--listen` nor `$PAPERCRAFT_RELAY_LISTEN` says otherwise.
const DEFAULT_LISTEN: &str = "0.0.0.0:8787";
const ENV_LISTEN: &str = "PAPERCRAFT_RELAY_LISTEN";

struct Args {
    listen: String,
    static_dir: Option<PathBuf>,
    allow_origin: Option<String>,
    data_dir: Option<PathBuf>,
    library: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut listen = std::env::var(ENV_LISTEN).unwrap_or_else(|_| DEFAULT_LISTEN.to_string());
    let mut static_dir = None;
    let mut allow_origin = None;
    let mut data_dir = None;
    let mut library = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--listen" => listen = value()?,
            "--static" => static_dir = Some(PathBuf::from(value()?)),
            "--allow-origin" => allow_origin = Some(value()?),
            "--data" => data_dir = Some(PathBuf::from(value()?)),
            "--library" => library = true,
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }
    Ok(Args {
        listen,
        static_dir,
        allow_origin,
        data_dir,
        library,
    })
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("papercraft-relay: {e}");
            return ExitCode::FAILURE;
        }
    };
    if args.library && args.data_dir.is_none() {
        eprintln!("papercraft-relay: --library needs --data <dir>");
        return ExitCode::FAILURE;
    }
    let listen: SocketAddr = match args.listen.parse() {
        Ok(a) => a,
        Err(_) => {
            eprintln!(
                "papercraft-relay: '{}' is not an address like 0.0.0.0:8787 (set {ENV_LISTEN} to change it)",
                args.listen
            );
            return ExitCode::FAILURE;
        }
    };
    let running = match relay::spawn_with_data(
        listen,
        args.static_dir.clone(),
        args.allow_origin.clone(),
        args.data_dir.clone().map(|dir| Data {
            dir,
            library: args.library,
        }),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("papercraft-relay: {e}");
            return ExitCode::FAILURE;
        }
    };
    // The port is the real one, which matters when the request asked for 0.
    println!("papercraft-relay: listening on port {}", running.port());
    if let Some(dir) = &args.static_dir {
        println!("papercraft-relay: serving {}", dir.display());
    }
    println!(
        "papercraft-relay: agent endpoints are http://<this host>:{}/s/<token>/mcp",
        running.port()
    );
    // Serve until the process is signalled; `running` then stops the server on the way out.
    loop {
        std::thread::park();
    }
}
