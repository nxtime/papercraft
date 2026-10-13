//! The relay end to end: an agent over streamable HTTP, a fake tab over a real WebSocket.
//!
//! The tab is the blocking tungstenite client on a thread: the relay keeps no state, so all
//! the test proves is that a call reaches a socket and its answer comes back as MCP content.

use papercraft::mcp::Out;
use papercraft::mcp::relay::{self, NO_TAB_ERROR};
use papercraft::mcp::relay_proto::{Call as CallFrame, Reply};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message as Ws, connect as ws_connect};

/// What the fake tab answers with.
const TAB_TEXT: &str = "answered in the tab";
/// How long to wait for the tab's socket before giving up.
const CONNECT_WAIT: Duration = Duration::from_secs(10);
const CONNECT_POLL: Duration = Duration::from_millis(20);

/// A link token as a tab generates one: 32 lowercase hex characters.
fn token() -> String {
    format!("{:032x}", fastrand::u64(..))
}

/// A 1x1 transparent PNG, so the image path is exercised with real bytes.
fn png() -> Vec<u8> {
    papercraft::assets::base64_decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    )
    .expect("the PNG fixture is valid base64")
}

fn spawn_relay() -> relay::Running {
    let listen: SocketAddr = "127.0.0.1:0".parse().expect("a valid address");
    relay::spawn(listen, None, None).expect("the relay binds an ephemeral port")
}

fn tool(name: &str, v: Value) -> CallToolRequestParams {
    let mut p = CallToolRequestParams::new("");
    if let Value::Object(m) = v {
        p.arguments = Some(m);
    }
    p.name = name.to_string().into();
    p
}

fn text_of(r: &CallToolResult) -> String {
    r.content
        .iter()
        .find_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("a text block")
}

/// A tab that answers `answers` calls with a text block and a PNG, then hangs up so the relay
/// can shut down. Returns the calls it saw.
fn fake_tab(
    url: String,
    answers: usize,
    connected: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Vec<CallFrame>> {
    std::thread::spawn(move || {
        let (mut ws, _) = ws_connect(&url).expect("the tab connects to the relay");
        let mut seen = Vec::new();
        while seen.len() < answers {
            match ws.read() {
                // The relay pings as soon as the link is live; that is the "connected" signal.
                Ok(Ws::Ping(_)) => connected.store(true, Ordering::SeqCst),
                Ok(Ws::Text(t)) => {
                    let Ok(call) = serde_json::from_str::<CallFrame>(&t) else {
                        continue;
                    };
                    seen.push(call.clone());
                    let reply = Reply::ok(
                        call.id,
                        vec![
                            Out::Text(TAB_TEXT.to_string()),
                            Out::Image {
                                data: png(),
                                mime: "image/png".to_string(),
                            },
                        ],
                    );
                    let json = serde_json::to_string(&reply).expect("a reply encodes");
                    ws.send(Ws::Text(json.into()))
                        .expect("the reply reaches the relay");
                }
                Ok(Ws::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            }
        }
        seen
    })
}

async fn wait_for_tab(connected: &AtomicBool) {
    let deadline = tokio::time::Instant::now() + CONNECT_WAIT;
    while !connected.load(Ordering::SeqCst) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the fake tab never connected"
        );
        tokio::time::sleep(CONNECT_POLL).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_reaches_the_tab_and_its_answer_reaches_the_agent() {
    let running = spawn_relay();
    let token = token();
    let connected = Arc::new(AtomicBool::new(false));
    let tab = fake_tab(
        format!("ws://127.0.0.1:{}/tab/{token}", running.port()),
        1,
        connected.clone(),
    );
    wait_for_tab(&connected).await;

    let transport = rmcp::transport::StreamableHttpClientTransport::from_uri(format!(
        "http://127.0.0.1:{}/s/{token}/mcp",
        running.port()
    ));
    let client = ().serve(transport).await.expect("the agent connects");
    // The relay serves the same catalogue as the desktop app.
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "get_basic_info"));

    let r = client
        .call_tool(tool("get_basic_info", json!({})))
        .await
        .expect("the call completes");
    assert_eq!(text_of(&r), TAB_TEXT, "the text came back from the tab");
    let image = r
        .content
        .iter()
        .find_map(|c| match c {
            ContentBlock::Image(i) => Some(i),
            _ => None,
        })
        .expect("the tab's image block survives the relay");
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.data, papercraft::assets::base64(&png()));

    let seen = tab.join().expect("the tab thread finished");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].name, "get_basic_info");
    assert_eq!(seen[0].args, json!({}));

    client.cancel().await.unwrap();
    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_without_a_tab_says_so() {
    let running = spawn_relay();
    let token = token();
    let transport = rmcp::transport::StreamableHttpClientTransport::from_uri(format!(
        "http://127.0.0.1:{}/s/{token}/mcp",
        running.port()
    ));
    let client = ().serve(transport).await.expect("the agent connects");
    let r = client
        .call_tool(tool("get_basic_info", json!({})))
        .await
        .expect("the call completes");
    assert_eq!(r.is_error, Some(true), "{r:?}");
    assert_eq!(text_of(&r), NO_TAB_ERROR);
    client.cancel().await.unwrap();
    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_tokens_browsers_and_unknown_routes_are_refused() {
    let running = spawn_relay();
    let base = format!("http://127.0.0.1:{}", running.port());
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;
    let http = reqwest::Client::new();
    let good = token();

    // Agents are not browsers: anything carrying an Origin is turned away.
    let r = http
        .post(format!("{base}/s/{good}/mcp"))
        .header("content-type", "application/json")
        .header("origin", "https://evil.example")
        .body(init)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);

    // Tokens are 32..=64 lowercase hex characters; anything else is a plain 404.
    for (label, bad) in [
        ("uppercase", good.to_uppercase()),
        ("too short", "abc".to_string()),
        ("non-hex", "z".repeat(32)),
        ("empty", String::new()),
    ] {
        let r = http
            .post(format!("{base}/s/{bad}/mcp"))
            .header("content-type", "application/json")
            .body(init)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404, "{label} token");
        let r = http.get(format!("{base}/tab/{bad}")).send().await.unwrap();
        assert_eq!(r.status(), 404, "{label} token on the tab route");
    }

    let r = http.get(format!("{base}/nope")).send().await.unwrap();
    assert_eq!(r.status(), 404);
    running.stop();
}
/// One tab per link: a newer connection takes over and the older socket is closed with a reason.
#[test]
fn a_newer_tab_takes_over_the_link_and_closes_the_old_socket() {
    let running = spawn_relay();
    let token = token();
    let url = format!("ws://127.0.0.1:{}/tab/{token}", running.port());
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (close_tx, close_rx) = std::sync::mpsc::channel();
    let first = std::thread::spawn({
        let url = url.clone();
        move || {
            let (mut ws, _) = ws_connect(&url).expect("the first tab connects");
            while !matches!(ws.read().expect("the link pings"), Ws::Ping(_)) {}
            ready_tx.send(()).expect("the test is waiting");
            while let Ok(msg) = ws.read() {
                if let Ws::Close(Some(frame)) = msg {
                    close_tx
                        .send(frame.reason.to_string())
                        .expect("the test waits");
                    return;
                }
            }
        }
    });
    ready_rx
        .recv_timeout(CONNECT_WAIT)
        .expect("the first tab is live");

    let (mut second, _) = ws_connect(&url).expect("the second tab connects");
    while !matches!(second.read().expect("the link pings"), Ws::Ping(_)) {}

    assert_eq!(
        close_rx
            .recv_timeout(CONNECT_WAIT)
            .expect("the old socket is closed"),
        "replaced by a newer tab"
    );
    first.join().expect("the first tab thread finished");
    drop(second);
    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn static_files_are_served_with_their_type() {
    let dir = std::env::temp_dir().join(format!("papercraft-relay-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(dir.join("pkg")).unwrap();
    std::fs::write(dir.join("index.html"), "<html>paper</html>").unwrap();
    std::fs::write(dir.join("pkg/papercraft.js"), "export {};").unwrap();
    std::fs::write(dir.join("pkg/papercraft_bg.wasm"), b"\0asm").unwrap();
    let listen: SocketAddr = "127.0.0.1:0".parse().expect("a valid address");
    let running = relay::spawn(listen, Some(dir.clone()), None).expect("the relay binds");
    let base = format!("http://127.0.0.1:{}", running.port());
    let http = reqwest::Client::new();

    let r = http.get(&base).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers().get("content-type").unwrap(), "text/html");
    assert_eq!(r.text().await.unwrap(), "<html>paper</html>");

    let r = http
        .get(format!("{base}/pkg/papercraft_bg.wasm"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers().get("content-type").unwrap(), "application/wasm");

    let r = http
        .get(format!("{base}/pkg/missing.js"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);

    std::fs::remove_dir_all(&dir).ok();
    running.stop();
}
