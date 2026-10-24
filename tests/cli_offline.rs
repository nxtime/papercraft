//! `papercraft-cli` without the app: in-process calls on a temp store, the "app has it open"
//! refusal, and shell script parsing.

use papercraft::cli::{Offline, Stmt, parse_script, refuse_if_open};
use papercraft::mcp::Out;
use papercraft::project::Store;
use serde_json::{Value, json};

fn temp_store(tag: &str) -> Store {
    let dir = std::env::temp_dir().join(format!("pc-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Store::open(dir).unwrap()
}

fn json_of(outs: Vec<Out>) -> Value {
    let Some(Out::Text(t)) = outs.into_iter().next() else {
        panic!("no text result")
    };
    serde_json::from_str(&t).unwrap()
}

#[test]
fn offline_round_trip_saves_and_reopens() {
    let store = temp_store("rt");
    let root = store.root.clone();
    let mut o = Offline::new(store, None).unwrap();
    let file = json_of(o.call("create_file", &json!({"name": "Demo"})).unwrap());
    let id = file["fileId"].as_str().unwrap().to_string();
    o.call("open_file", &json!({"fileId": id})).unwrap();
    let ab = json_of(
        o.call(
            "create_artboard",
            &json!({"name": "Card", "styles": {"width": "200px", "height": "100px"}}),
        )
        .unwrap(),
    );
    let ab_id = ab["id"].as_str().unwrap().to_string();
    o.call(
        "write_html",
        &json!({"targetNodeId": ab_id, "mode": "insert-children",
                "html": "<div layer-name=\"Title\" style=\"font-size:20px\">Hi</div>"}),
    )
    .unwrap();
    assert_eq!(o.last_touched.as_deref(), Some(ab_id.as_str()));

    // A fresh host on the same folder: the default project is the one just saved.
    let mut again = Offline::new(Store::open(&root).unwrap(), None).unwrap();
    let tree = again
        .call("get_tree_summary", &json!({"nodeId": ab_id}))
        .unwrap();
    let Some(Out::Text(t)) = tree.first() else {
        panic!("no tree")
    };
    assert!(t.contains("Card") && t.contains("Title"), "{t}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn offline_edit_without_project_is_an_error() {
    let mut o = Offline::new(temp_store("none"), None).unwrap();
    let e = o.call("create_artboard", &json!({})).unwrap_err();
    assert!(e.contains("no project is open"), "{e}");
}

#[test]
fn refuses_when_app_has_project_open() {
    let items = json!({"items": [
        {"id": "a1", "name": "Alpha", "path": "/p/Alpha", "open": true},
        {"id": "b2", "name": "Beta", "path": "/p/Beta", "open": false},
    ]});
    let e = refuse_if_open(&items, Some("Alpha")).unwrap_err();
    assert!(e.contains("PaperCraft has this project open"), "{e}");
    assert!(refuse_if_open(&items, Some("a1")).is_err());
    assert!(refuse_if_open(&items, Some("/p/Alpha")).is_err());
    assert!(
        refuse_if_open(&items, None).is_err(),
        "default is the first item"
    );
    assert!(refuse_if_open(&items, Some("beta")).is_ok());
    assert!(refuse_if_open(&items, Some("Unknown")).is_ok());
    assert!(refuse_if_open(&json!({"items": []}), None).is_ok());
}

#[test]
fn script_parsing() {
    let script = r#"
# comment
create_file {"name": "a}b"}

write_html {
  "html": "<div style=\"x\">{ not a brace }</div>",
  "list": [1, 2]
}
:shot 1:2 out.png
get_basic_info
bad {"x":
"#;
    let s = parse_script(script);
    let lines: Vec<usize> = s.iter().map(|(n, _)| *n).collect();
    assert_eq!(lines, [3, 5, 9, 10, 11]);
    match &s[0].1 {
        Ok(Stmt::Call { tool, args }) => {
            assert_eq!(tool, "create_file");
            assert_eq!(args["name"], "a}b");
        }
        o => panic!("{o:?}"),
    }
    match &s[1].1 {
        Ok(Stmt::Call { tool, args }) => {
            assert_eq!(tool, "write_html");
            assert_eq!(args["list"][1], 2);
            assert!(args["html"].as_str().unwrap().contains("{ not a brace }"));
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(
        s[2].1,
        Ok(Stmt::Cmd {
            name: "shot".into(),
            args: vec!["1:2".into(), "out.png".into()]
        })
    );
    assert_eq!(
        s[3].1,
        Ok(Stmt::Call {
            tool: "get_basic_info".into(),
            args: Value::Null
        })
    );
    assert!(s[4].1.is_err(), "unterminated JSON fails at its first line");
}

#[test]
fn offline_screenshot_when_gpu_exists() {
    if papercraft::render::gpu::Headless::new().is_err() {
        return;
    }
    let store = temp_store("shot");
    let root = store.root.clone();
    let mut o = Offline::new(store, None).unwrap();
    let id = json_of(o.call("create_file", &json!({"name": "S"})).unwrap())["fileId"]
        .as_str()
        .unwrap()
        .to_string();
    o.call("open_file", &json!({"fileId": id})).unwrap();
    o.call(
        "create_artboard",
        &json!({"styles": {"width": "80px", "height": "60px", "backgroundColor": "#336699"}}),
    )
    .unwrap();
    let r = o.call("get_screenshot", &Value::Null).unwrap();
    assert!(
        r.iter()
            .any(|x| matches!(x, Out::Image { data, .. } if data.starts_with(b"\x89PNG")))
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn open_file_tool_goes_through_the_open_guard() {
    let store = temp_store("guard");
    let root = store.root.clone();
    let mut o = Offline::new(store, None).unwrap();
    o.open_guard = Some(Box::new(|id| Err(format!("app has {id} open"))));
    let e = o.call("open_file", &json!({"fileId": "X"})).unwrap_err();
    assert_eq!(e, "app has X open");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn refusal_compares_canonical_paths() {
    let dir = std::env::temp_dir().join(format!("pc-canon-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("p")).unwrap();
    let via_dots = dir.join("p").join("..").join("p");
    let items = json!({"items": [{"id": "a", "name": "A", "path": dir.join("p"), "open": true}]});
    assert!(refuse_if_open(&items, Some(via_dots.to_str().unwrap())).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

/// The same tool sequence on a headless host and on one set up like the app window gives the
/// same node rects, and the headless one saves no camera (the app fits the page when opened).
#[test]
fn offline_matches_app_placement_and_saves_no_camera() {
    use papercraft::mcp::exec::{self, SessionHost};
    use papercraft::geom::V2;

    fn run(app_like: bool) -> (Vec<String>, std::path::PathBuf, SessionHost) {
        let root = std::env::temp_dir().join(format!(
            "pc-cli-parity-{app_like}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut h = SessionHost::new(Store::open(&root).unwrap());
        if app_like {
            h.ed.headless = false;
            h.ed.viewport = V2 { x: 1123.0, y: 860.0 };
        }
        let mut call = |n: &str, a: Value| json_of(exec::call(&mut h, n, &a).unwrap());
        let f = call("create_file", json!({"name": "Demo"}));
        call("open_file", json!({"fileId": f["fileId"]}));
        let ab = call(
            "create_artboard",
            json!({"name": "Home", "styles": {"width": "390px", "height": "600px"}}),
        );
        call(
            "write_html",
            json!({"targetNodeId": ab["id"], "mode": "insert-children",
                   "html": "<div style=\"font-size:24px\">Hello</div>"}),
        );
        call("create_shape", json!({"type": "rectangle", "styles": {"width": "50px", "height": "50px"}}));
        call("create_artboard", json!({"name": "Two", "styles": {"width": "300px", "height": "300px"}}));
        let p = call("create_page", json!({"name": "Second"}));
        call(
            "create_artboard",
            json!({"name": "Other", "pageId": p["pageId"], "styles": {"width": "200px", "height": "200px"}}),
        );
        exec::call(&mut h, "finish_working_on_nodes", &json!({})).unwrap();
        let rects = (0..h.ed.pages.len())
            .flat_map(|i| {
                let d = h.ed.page_data(i).doc;
                d.nodes.values().map(|n| format!("{i} {} {:?}", n.name, n.rect)).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let dir = h.session.project.as_ref().unwrap().dir.clone();
        (rects, dir, h)
    }

    let (mut headless, dir_a, _a) = run(false);
    let (mut app, dir_b, _b) = run(true);
    headless.sort();
    app.sort();
    assert_eq!(headless, app);
    let cams = |d: &std::path::Path| {
        std::fs::read_dir(d.join("pages"))
            .unwrap()
            .filter(|e| std::fs::read_to_string(e.as_ref().unwrap().path()).unwrap().contains("\"camera\""))
            .count()
    };
    assert_eq!(cams(&dir_a), 0, "headless camera must not be saved");
    assert_eq!(cams(&dir_b), 1, "app saves the active page camera");
    let _ = std::fs::remove_dir_all(dir_a.parent().unwrap());
    let _ = std::fs::remove_dir_all(dir_b.parent().unwrap());
}

#[test]
fn offline_sessions_lock_the_project_and_never_lose_writes() {
    use papercraft::cli::{PROJECT_LOCK_WAIT, lock_project};
    let store = temp_store("lock");
    let root = store.root.clone();
    let (p, _) = store.create("Shared").unwrap();
    let id = p.manifest.id.clone();
    let open = || Offline::new(Store::open(&root).unwrap(), Some(&id)).unwrap();
    let artboard = |o: &mut Offline, name: &str| {
        o.call("create_artboard", &json!({"name": name, "styles": {"width": "100px", "height": "100px"}}))
    };
    // Both load the project; B writes first and finishes, then A writes: B's work survives.
    let mut a = open();
    let mut b = open();
    artboard(&mut b, "From B").unwrap();
    drop(b);
    artboard(&mut a, "From A").unwrap();
    // While A holds the project another writer waits, then fails with a clear message.
    let mut c = open();
    let t = std::time::Instant::now();
    let e = artboard(&mut c, "From C").unwrap_err();
    assert!(e.contains("another papercraft-cli is writing"), "{e}");
    assert!(t.elapsed() >= PROJECT_LOCK_WAIT);
    // Reads never wait.
    c.call("get_basic_info", &Value::Null).unwrap();
    drop(a);
    artboard(&mut c, "From C").unwrap();
    let names: Vec<String> = json_of(c.call("get_basic_info", &Value::Null).unwrap())["artboards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["From B", "From A", "From C"]);
    drop(c);
    let held = lock_project(&p.dir, std::time::Duration::ZERO).unwrap();
    assert!(lock_project(&p.dir, std::time::Duration::ZERO).is_err());
    drop(held);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn app_lock_marks_the_projects_folder_in_use() {
    use papercraft::cli::{app_running, hold_app_lock};
    let root = temp_store("applock").root.clone();
    assert!(!app_running(&root));
    let held = hold_app_lock(&root).unwrap();
    assert!(app_running(&root));
    drop(held);
    assert!(!app_running(&root));
    let _ = std::fs::remove_dir_all(&root);
}
