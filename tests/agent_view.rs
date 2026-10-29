//! What agents see and steer: the viewport (`get_basic_info`/`get_selection`, `set_viewport`),
//! screenshots with notes, `annotate {tidy}` and the CLI's version check and live tool list.

use papercraft::editor::InputEvent;
use papercraft::geom::v2;
use papercraft::mcp::{Out, exec};
use papercraft::model::NodeId;
use papercraft::project::Store;
use serde_json::{Value, json};

struct Fixture {
    host: exec::SessionHost,
    path: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("papercraft-agent-view-{:016x}", fastrand::u64(..)));
        let mut host = exec::SessionHost::new(Store::open(&path).unwrap());
        host.session.create(&mut host.ed, "View").unwrap();
        Self { host, path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    let o = exec::call(h, name, &args).unwrap_or_else(|e| panic!("{name} failed: {e}"));
    match o.into_iter().next() {
        Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        _ => Value::Null,
    }
}

fn artboard(h: &mut exec::SessionHost, name: &str, x: f64) -> Value {
    call(h, "create_artboard", json!({"name": name,
        "styles": {"left": format!("{x}px"), "top": "0px", "width": "300px", "height": "200px"}}))["id"].clone()
}

fn num(id: &Value) -> NodeId {
    NodeId(id.as_str().unwrap().rsplit(':').next().unwrap().parse().unwrap())
}

#[test]
fn viewport_is_reported_by_basic_info_and_selection() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    h.ed.camera.pan = v2(100.0, 50.0);
    h.ed.camera.zoom = 2.0;
    let page = h.ed.pages[0].id.clone();
    let want = json!({"pageId": page, "rect": [100.0, 50.0, h.ed.viewport.x / 2.0, h.ed.viewport.y / 2.0], "zoom": 2.0});
    assert_eq!(call(h, "get_basic_info", json!({}))["viewport"], want);
    assert_eq!(call(h, "get_selection", json!({}))["viewport"], want);
}

#[test]
fn set_viewport_switches_page_fits_and_zooms() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let a = artboard(h, "A", 0.0);
    let b = call(h, "create_page", json!({"name": "B"}))["pageId"].clone();
    let on_b = call(h, "create_artboard", json!({"pageId": b, "name": "Far",
        "styles": {"left": "5000px", "top": "5000px", "width": "100px", "height": "50px"}}))["id"].clone();
    assert_eq!(h.ed.active_page, 0);

    // Another page's node: select_nodes refuses and points at set_viewport, which switches.
    let e = exec::call(h, "select_nodes", &json!({"nodeIds": [on_b]})).unwrap_err();
    assert!(e.contains("set_viewport"), "{e}");
    let v = call(h, "set_viewport", json!({"nodeIds": [on_b]}))["viewport"].clone();
    assert_eq!(h.ed.active_page, 1);
    assert_eq!(v["pageId"], b);
    let r = h.ed.doc.world_rect(num(&on_b)).unwrap();
    let vis = h.ed.camera.visible_world(h.ed.viewport);
    assert!(vis.contains_rect(&r), "{vis:?} fits {r:?}");
    assert!(h.ed.camera.zoom > 1.0, "a small node fits above the 1.0 cap: {}", h.ed.camera.zoom);

    // Explicit zoom centres the rect at that level, beyond select_nodes' cap.
    let first = h.ed.pages[0].id.clone();
    call(h, "set_viewport", json!({"pageId": first, "nodeIds": [a], "zoom": 12}));
    assert_eq!(h.ed.active_page, 0);
    assert_eq!(h.ed.camera.zoom, 12.0);
    let c = h.ed.doc.world_rect(num(&a)).unwrap().center();
    let vis = h.ed.camera.visible_world(h.ed.viewport);
    assert!((vis.center().x - c.x).abs() < 1e-6 && (vis.center().y - c.y).abs() < 1e-6);

    // A world rect, and zoom limits.
    call(h, "set_viewport", json!({"rect": [-100, -100, 200, 200], "padding": 0}));
    let vis = h.ed.camera.visible_world(h.ed.viewport);
    assert!(vis.contains_rect(&papercraft::geom::Rect::new(-100.0, -100.0, 200.0, 200.0)));
    call(h, "set_viewport", json!({"zoom": 100000}));
    assert_eq!(h.ed.camera.zoom, papercraft::geom::MAX_ZOOM);
    assert!(exec::call(h, "set_viewport", &json!({"rect": [1, 2]})).is_err());
}

#[test]
fn set_viewport_waits_while_the_user_moves_the_canvas() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    artboard(h, "A", 0.0);
    h.ed.handle(InputEvent::Scroll { delta: v2(0.0, 40.0), pos: v2(10.0, 10.0) });
    let cam = h.ed.camera;
    let e = exec::call(h, "set_viewport", &json!({"zoom": 3})).unwrap_err();
    assert!(e.contains("user is moving the canvas"), "{e}");
    assert_eq!(h.ed.camera, cam);
    call(h, "set_viewport", json!({"zoom": 3, "force": true}));
    assert_eq!(h.ed.camera.zoom, 3.0);
}

#[test]
fn annotate_tidy_without_notes_reports_moves() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = artboard(h, "Sign in", 0.0);
    call(h, "annotate", json!({"notes": [{"node": id, "text": "One"}, {"node": id, "text": "Two"}]}));
    assert_eq!(call(h, "annotate", json!({"tidy": true}))["tidied"], 0);
    let note = h.ed.doc.annotations[0].id;
    h.ed.update_annotation(note, |a| a.position = v2(2000.0, 2000.0));
    assert_eq!(call(h, "annotate", json!({"tidy": true}))["tidied"], 1);
}

#[test]
fn notes_in_screenshots_grow_the_bounds_to_the_cards() {
    use papercraft::model::annotation::card_world;
    use papercraft::model::annotation_layout::card_size;
    let mut f = Fixture::new();
    let h = &mut f.host;
    let id = artboard(h, "Card", 0.0);
    let other = artboard(h, "Other", 2000.0);
    call(h, "annotate", json!({"notes": [
        {"node": id, "text": "Primary action\nOnly one per view"},
        {"node": id, "text": "Second note", "type": "content"},
        {"node": other, "text": "Not captured"},
    ]}));
    let doc = h.ed.page_doc(0).clone();
    let roots = [num(&id)];
    let (noted, shot) = exec::with_notes(&doc, &roots, &h.ed.design.categories());
    let b = papercraft::io::svg::export_bounds(&noted, &shot).unwrap();
    for a in doc.annotations.iter().filter(|a| a.target == Some(num(&id))) {
        let (at, s) = (card_world(&doc, a), card_size(a));
        let card = papercraft::geom::Rect::new(at.x, at.y, s.x, s.y);
        assert!(b.inflate(0.5).contains_rect(&card), "{b:?} holds {card:?}");
    }
    // Two lines, two dots, two cards; the other frame's note stays out.
    assert_eq!(shot.len(), 1 + 6);
    assert!(!b.intersects(&doc.world_rect(num(&other)).unwrap()));
    assert!(noted.validate().is_ok());

    if papercraft::render::gpu::Headless::new().is_err() {
        eprintln!("no GPU adapter; skipping the render half");
        return;
    }
    let png = |h: &mut exec::SessionHost, args: Value| match exec::call(h, "get_screenshot", &args).unwrap().remove(0) {
        Out::Image { data, .. } => data,
        o => panic!("{o:?}"),
    };
    let plain = png(h, json!({"nodeId": id}));
    assert_eq!(plain, png(h, json!({"nodeId": id, "annotations": false})), "off is byte-identical");
    let with = image::load_from_memory(&png(h, json!({"nodeId": id, "annotations": true}))).unwrap();
    let plain = image::load_from_memory(&plain).unwrap();
    assert_eq!((plain.width(), plain.height()), (300, 200));
    let dims = (with.width(), with.height());
    assert!(dims.0 as f64 >= b.w.floor() && dims.1 as f64 >= b.h.floor() && dims != (300, 200), "{dims:?} for {b:?}");
}

// ------------------------------------------------------------------------------ CLI

mod fake {
    use rmcp::model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
    };
    use rmcp::service::RequestContext;
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };
    use rmcp::{ErrorData, RoleServer, ServerHandler};

    pub const VERSION: &str = "0.0.1-fake";
    pub const NEW_TOOL: &str = "brand_new_tool";

    /// An app of another version that serves one tool the CLI's built-in list lacks.
    #[derive(Clone)]
    pub struct App;

    impl ServerHandler for App {
        fn get_info(&self) -> ServerConfig {
            ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
                .with_server_info(Implementation::new("PaperCraft", VERSION))
        }

        async fn list_tools(
            &self,
            _request: Option<PaginatedRequestParams>,
            _context: RequestContext<RoleServer>,
        ) -> Result<ListToolsResult, ErrorData> {
            let mut tools = papercraft::mcp::server::tool_list();
            tools.push(
                serde_json::from_value(serde_json::json!({"name": NEW_TOOL,
                    "description": "Only the app has it.",
                    "inputSchema": {"type": "object", "properties": {}}}))
                .unwrap(),
            );
            Ok(ListToolsResult::with_all_items(tools))
        }

        async fn call_tool(
            &self,
            _request: CallToolRequestParams,
            _context: RequestContext<RoleServer>,
        ) -> Result<CallToolResponse, ErrorData> {
            Ok(CallToolResult::success(vec![ContentBlock::text("{}")]).into())
        }
    }

    /// Serve [`App`] on a free loopback port; the thread lives until the test process ends.
    pub fn spawn() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let service: StreamableHttpService<App, LocalSessionManager> = StreamableHttpService::new(
                    || Ok(App),
                    Default::default(),
                    StreamableHttpServerConfig::default(),
                );
                let app = axum::Router::new().nest_service(papercraft::mcp::ENDPOINT, service);
                let l = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(l, app).await.unwrap();
            });
        });
        port
    }
}

#[test]
fn cli_warns_on_version_mismatch_and_lists_the_apps_tools() {
    let port = fake::spawn();
    let cli = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_papercraft-cli"))
            .env("PAPERCRAFT_MCP_PORT", port.to_string())
            .args(args)
            .output()
            .unwrap()
    };
    let o = cli(&["call", "get_basic_info"]);
    let err = String::from_utf8_lossy(&o.stderr);
    let want = format!("papercraft-cli {} \u{2260} app {}; reinstall the CLI", env!("CARGO_PKG_VERSION"), fake::VERSION);
    assert_eq!(err.matches(&want).count(), 1, "{err}");
    assert!(o.status.success());

    let list = String::from_utf8_lossy(&cli(&["list"]).stdout).to_string();
    assert!(list.lines().any(|l| l.starts_with(&format!("{}\t", fake::NEW_TOOL))), "{list}");
    assert!(papercraft::mcp::server::tool_list().iter().all(|t| t.name != fake::NEW_TOOL));
    assert!(cli(&["schema", fake::NEW_TOOL]).status.success());
}

#[test]
fn find_nodes_finds_phrases_inside_text_on_any_page() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    // A component whose text is rich (a bold span) and nested two frames deep.
    let main = artboard(h, "Banner", 0.0);
    call(h, "write_html", json!({"targetNodeId": main, "mode": "insert-children",
        "html": "<div><div><p>Welcome back. <strong>Elena lets you in</strong> when it starts.</p></div></div>"}));
    call(h, "create_component", json!({"nodeId": main}));
    // On another page, not the active one: an instance and a text broken across lines.
    let cand = call(h, "create_page", json!({"name": "Candidate"}))["pageId"].clone();
    let screen = call(h, "create_artboard", json!({"pageId": cand, "name": "Lobby",
        "styles": {"width": "400px", "height": "300px"}}))["id"].clone();
    call(h, "write_html", json!({"pageId": cand, "targetNodeId": screen, "mode": "insert-children",
        "html": "<section><div><p>Elena lets\nyou\u{a0}in shortly</p></div></section>"}));
    call(h, "create_instance", json!({"pageId": cand, "componentId": main, "parentId": screen}));
    assert_eq!(h.ed.active_page, 0);

    let found = call(h, "find_nodes", json!({"pageId": cand, "textValue": "elena LETS you in"}));
    assert_eq!(found["count"], 2, "{found}");
    assert!(found["results"].as_array().unwrap().iter().all(|r| r["pageId"] == cand));
    // Whole project, wildcards still anchor to the whole value.
    assert_eq!(call(h, "find_nodes", json!({"textValue": "Elena lets you in"}))["count"], 3);
    assert_eq!(call(h, "find_nodes", json!({"textValue": "Welcome*starts."}))["count"], 2);
    assert_eq!(call(h, "find_nodes", json!({"textValue": "Elena*"}))["count"], 1);
}

#[test]
fn tree_summary_full_text_on_request() {
    let mut f = Fixture::new();
    let h = &mut f.host;
    let ab = artboard(h, "Copy", 0.0);
    let long = "This sentence is much longer than forty characters, so it gets cut.";
    call(h, "write_html", json!({"targetNodeId": ab, "mode": "insert-children", "html": format!("<p>{long}</p>")}));
    let short = call(h, "get_tree_summary", json!({"nodeId": ab}));
    assert!(!short.as_str().unwrap().contains(long), "{short}");
    let full = call(h, "get_tree_summary", json!({"nodeId": ab, "fullText": true}));
    assert!(full.as_str().unwrap().contains(long), "{full}");
}

#[test]
fn cli_online_only_refuses_to_work_offline() {
    let home = std::env::temp_dir().join(format!("papercraft-online-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&home).unwrap();
    // A port nothing listens on: bind, read it, release.
    let port = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
    let cli = |env: Option<&str>, args: &[&str]| {
        let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_papercraft-cli"));
        c.env("PAPERCRAFT_HOME", &home).env("PAPERCRAFT_MCP_PORT", port.to_string()).env_remove("PAPERCRAFT_CLI_NO_OFFLINE");
        if let Some(v) = env {
            c.env("PAPERCRAFT_CLI_NO_OFFLINE", v);
        }
        c.args(args).output().unwrap()
    };
    for o in [cli(Some("1"), &["call", "list_resources"]), cli(None, &["--online", "call", "list_resources"])] {
        assert!(!o.status.success());
        assert!(String::from_utf8_lossy(&o.stderr).contains("offline work is off"), "{}", String::from_utf8_lossy(&o.stderr));
    }
    assert!(cli(None, &["call", "list_resources"]).status.success(), "the default still works offline");
    let _ = std::fs::remove_dir_all(&home);
}
