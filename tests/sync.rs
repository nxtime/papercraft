//! Shared projects end to end: two project folders kept in step through a real relay.

use papercraft::mcp::relay;
use papercraft::mcp::store::Data;
use papercraft::sync::{self, SHARE_FILE, Share};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

fn temp(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "papercraft-sync-{label}-{:016x}",
        fastrand::u64(..)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn read(dir: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(rel)).ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_folders_follow_each_other_through_the_relay() {
    let data = temp("data");
    let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let running = relay::spawn_with_data(
        listen,
        None,
        None,
        Some(Data {
            dir: data.clone(),
            library: false,
        }),
    )
    .unwrap();
    let server = format!("http://127.0.0.1:{}", running.port());

    // A shares a project with two pages.
    let a = temp("a");
    std::fs::create_dir_all(a.join("pages")).unwrap();
    std::fs::write(a.join("project.json"), r#"{"manifest":1}"#).unwrap();
    std::fs::write(a.join("pages/p1.json"), r#"{"page":"one"}"#).unwrap();
    let share = Share::new(&server);
    share.save(&a).unwrap();
    assert!(
        !sync::sync_once(&a).await.unwrap(),
        "uploading is not an applied change"
    );

    // B joins from the link and gets everything, but not A's local-only state.
    let b = temp("b");
    let joined = Share::from_link(&share.link()).unwrap();
    joined.save(&b).unwrap();
    assert!(sync::sync_once(&b).await.unwrap());
    assert_eq!(
        read(&b, "project.json").as_deref(),
        Some(r#"{"manifest":1}"#)
    );
    assert_eq!(
        read(&b, "pages/p1.json").as_deref(),
        Some(r#"{"page":"one"}"#)
    );
    // Nothing changed: both sides settle.
    assert!(!sync::sync_once(&b).await.unwrap());
    assert!(!sync::sync_once(&a).await.unwrap());

    // B edits one page and adds another; A receives both.
    std::fs::write(b.join("pages/p1.json"), r#"{"page":"one, edited in B"}"#).unwrap();
    std::fs::write(b.join("pages/p2.json"), r#"{"page":"two"}"#).unwrap();
    sync::sync_once(&b).await.unwrap();
    assert!(sync::sync_once(&a).await.unwrap());
    assert_eq!(
        read(&a, "pages/p1.json").as_deref(),
        Some(r#"{"page":"one, edited in B"}"#)
    );
    assert_eq!(
        read(&a, "pages/p2.json").as_deref(),
        Some(r#"{"page":"two"}"#)
    );

    // A deletes a page; B loses it too.
    std::fs::remove_file(a.join("pages/p2.json")).unwrap();
    sync::sync_once(&a).await.unwrap();
    assert!(sync::sync_once(&b).await.unwrap());
    assert!(read(&b, "pages/p2.json").is_none());

    // Both edit the same file before syncing: the one that syncs last (the folder) wins.
    std::fs::write(a.join("pages/p1.json"), r#"{"from":"A"}"#).unwrap();
    std::fs::write(b.join("pages/p1.json"), r#"{"from":"B"}"#).unwrap();
    sync::sync_once(&a).await.unwrap();
    sync::sync_once(&b).await.unwrap();
    sync::sync_once(&a).await.unwrap();
    assert_eq!(read(&a, "pages/p1.json"), read(&b, "pages/p1.json"));

    // The sync state never travels: the server holds no .share.
    assert!(!data.join(&share.token).join(SHARE_FILE).exists());

    for d in [&a, &b, &data] {
        std::fs::remove_dir_all(d).ok();
    }
    running.stop();
}

fn project(root: &Path, name: &str) -> PathBuf {
    let store = papercraft::project::Store::open(root).unwrap();
    store.create(name).unwrap().0.dir
}

/// Every project directory under `root` that a device would list.
fn names(root: &Path) -> Vec<String> {
    let store = papercraft::project::Store::open(root).unwrap();
    store.list().into_iter().map(|e| e.manifest.name).collect()
}

fn library_relay(data: &Path, library: bool) -> (relay::Running, String) {
    let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let data = Data {
        dir: data.to_path_buf(),
        library,
    };
    let running = relay::spawn_with_data(listen, None, None, Some(data)).unwrap();
    let server = format!("http://127.0.0.1:{}", running.port());
    (running, server)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_library_shows_every_project_on_every_device() {
    let data = temp("lib-data");
    let (_relay, server) = library_relay(&data, true);

    // Device A has a project; one library round shares it and a sync uploads it.
    let (a, b) = (temp("lib-a"), temp("lib-b"));
    let alpha = project(&a, "Alpha");
    assert!(sync::library_round(&a, &server).await.unwrap());
    let token = Share::load(&alpha).expect("shared by the library").token;
    sync::sync_once(&alpha).await.unwrap();

    // Device B starts empty, joins what the library lists and downloads it.
    assert!(sync::library_round(&b, &server).await.unwrap());
    let joined = b.join(format!("shared-{}", &token[..8]));
    sync::sync_once(&joined).await.unwrap();
    assert_eq!(names(&b), ["Alpha"]);

    // Another round changes nothing (no duplicate join, no re-share).
    sync::library_round(&b, &server).await.unwrap();
    sync::library_round(&a, &server).await.unwrap();
    assert_eq!(names(&a), ["Alpha"]);
    assert_eq!(names(&b), ["Alpha"]);

    // B deletes it on the server: the data is kept in .trash, A is told it is deleted.
    let http = reqwest::Client::new();
    let res = http
        .delete(format!("{server}/p/{token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    assert!(data.join(".trash").read_dir().unwrap().next().is_some());
    let listed = http
        .get(format!("{server}/library"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!listed.contains(&token), "{listed}");
    match sync::sync_once(&alpha).await {
        Err(sync::SyncError::Gone(sync::Gone::Deleted)) => {}
        other => panic!("expected Gone(Deleted), got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_lost_its_data_does_not_delete_projects() {
    let data = temp("lost-data");
    let (_relay, server) = library_relay(&data, true);
    let a = temp("lost-a");
    let proj = project(&a, "Keep me");
    sync::library_round(&a, &server).await.unwrap();
    sync::sync_once(&proj).await.unwrap();

    // The server's disk is wiped.
    let token = Share::load(&proj).unwrap().token;
    std::fs::remove_dir_all(data.join(&token)).unwrap();
    match sync::sync_once(&proj).await {
        Err(sync::SyncError::Gone(sync::Gone::Lost)) => {}
        other => panic!("expected Gone(Lost), got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_relay_without_library_says_so() {
    let (_relay, server) = library_relay(&temp("plain-data"), false);
    let a = temp("plain-a");
    project(&a, "Private");
    assert!(!sync::library_round(&a, &server).await.unwrap());
    // Nothing was shared behind the user's back.
    assert!(names(&a).len() == 1);
    let projects = papercraft::project::Store::open(&a).unwrap().list();
    assert!(Share::load(&projects[0].dir).is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_sharing_detaches_this_folder_but_leaves_the_relay_copy() {
    let data = temp("unshare-data");
    let running = relay::spawn_with_data(
        "127.0.0.1:0".parse().unwrap(),
        None,
        None,
        Some(Data {
            dir: data,
            library: false,
        }),
    )
    .unwrap();
    let server = format!("http://127.0.0.1:{}", running.port());
    let a = temp("unshare-a");
    std::fs::create_dir_all(a.join("pages")).unwrap();
    std::fs::write(a.join("project.json"), r#"{"manifest":1}"#).unwrap();
    std::fs::write(a.join("pages/p1.json"), r#"{"page":"one"}"#).unwrap();
    let share = Share::new(&server);
    share.save(&a).unwrap();
    sync::sync_once(&a).await.unwrap();

    let mut engine = sync::Sync::default();
    engine.unshare(&a);
    assert!(Share::load(&a).is_none(), "the share marker is gone");

    // Later local edits never reach the relay...
    std::fs::write(
        a.join("pages/p1.json"),
        r#"{"page":"edited after unsharing"}"#,
    )
    .unwrap();
    let b = temp("unshare-b");
    Share::from_link(&share.link()).unwrap().save(&b).unwrap();
    sync::sync_once(&b).await.unwrap();
    // ...and the copy already on the relay is still there for whoever holds the link.
    assert_eq!(
        read(&b, "pages/p1.json").as_deref(),
        Some(r#"{"page":"one"}"#)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_people_editing_one_page_keep_both_changes() {
    let running = relay::spawn_with_data(
        "127.0.0.1:0".parse().unwrap(),
        None,
        None,
        Some(Data {
            dir: temp("merge-data"),
            library: false,
        }),
    )
    .unwrap();
    let server = format!("http://127.0.0.1:{}", running.port());
    let page = |a: i64, b: i64, extra: &str| {
        format!(
            r#"{{"next_id":3,"root":[1,2],"nodes":[{{"id":1,"x":{a}}},{{"id":2,"x":{b}}}{extra}]}}"#
        )
    };
    let a = temp("merge-a");
    std::fs::create_dir_all(a.join("pages")).unwrap();
    std::fs::write(a.join("project.json"), r#"{"manifest":1}"#).unwrap();
    std::fs::write(a.join("pages/p1.json"), page(0, 0, "")).unwrap();
    let share = Share::new(&server);
    share.save(&a).unwrap();
    sync::sync_once(&a).await.unwrap();
    let b = temp("merge-b");
    Share::from_link(&share.link()).unwrap().save(&b).unwrap();
    sync::sync_once(&b).await.unwrap();

    // Both edit the same page at the same time: A moves node 1, B moves node 2.
    std::fs::write(a.join("pages/p1.json"), page(10, 0, "")).unwrap();
    std::fs::write(b.join("pages/p1.json"), page(0, 20, "")).unwrap();
    sync::sync_once(&b).await.unwrap();
    assert!(
        sync::sync_once(&a).await.unwrap(),
        "A applied B's change by merging"
    );
    sync::sync_once(&b).await.unwrap();

    for dir in [&a, &b] {
        let v: serde_json::Value =
            serde_json::from_str(&read(dir, "pages/p1.json").unwrap()).unwrap();
        assert_eq!(v["nodes"][0]["x"], 10, "A's edit survived in {dir:?}");
        assert_eq!(v["nodes"][1]["x"], 20, "B's edit survived in {dir:?}");
    }
}

async fn merge_pair(label: &str, page: &str) -> (relay::Running, PathBuf, PathBuf) {
    let running = relay::spawn_with_data(
        "127.0.0.1:0".parse().unwrap(),
        None,
        None,
        Some(Data {
            dir: temp(&format!("{label}-data")),
            library: false,
        }),
    )
    .unwrap();
    let server = format!("http://127.0.0.1:{}", running.port());
    let a = temp(&format!("{label}-a"));
    std::fs::create_dir_all(a.join("pages")).unwrap();
    std::fs::write(a.join("project.json"), r#"{"manifest":1}"#).unwrap();
    std::fs::write(a.join("pages/p1.json"), page).unwrap();
    let share = Share::new(&server);
    share.save(&a).unwrap();
    sync::sync_once(&a).await.unwrap();
    let b = temp(&format!("{label}-b"));
    Share::from_link(&share.link()).unwrap().save(&b).unwrap();
    sync::sync_once(&b).await.unwrap();
    (running, a, b)
}

fn fake_page(a: i64, b: i64) -> String {
    format!(r#"{{"next_id":3,"root":[1,2],"nodes":[{{"id":1,"x":{a}}},{{"id":2,"x":{b}}}]}}"#)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_download_waits_for_the_save_and_then_merges() {
    use std::sync::atomic::AtomicBool;
    let (_relay, a, b) = merge_pair("hold", &fake_page(0, 0)).await;
    std::fs::write(b.join("pages/p1.json"), fake_page(0, 20)).unwrap();
    sync::sync_once(&b).await.unwrap();

    // A has unsaved edits: B's change must not land in the folder yet.
    let hold = AtomicBool::new(true);
    assert!(!sync::sync_once_holding(&a, &hold).await.unwrap());
    assert_eq!(read(&a, "pages/p1.json").unwrap(), fake_page(0, 0));

    // A saves its edit, the hold lifts, and both edits meet in the merge.
    std::fs::write(a.join("pages/p1.json"), fake_page(10, 0)).unwrap();
    hold.store(false, std::sync::atomic::Ordering::Relaxed);
    assert!(sync::sync_once_holding(&a, &hold).await.unwrap());
    let v: serde_json::Value = serde_json::from_str(&read(&a, "pages/p1.json").unwrap()).unwrap();
    assert_eq!((v["nodes"][0]["x"].as_i64(), v["nodes"][1]["x"].as_i64()), (Some(10), Some(20)));
}

fn real_page(nodes: &str, root: &str) -> String {
    format!(
        r#"{{"format":"paper-rs-page","version":4,"id":"p1","name":"P","next_id":10,"root":{root},"nodes":[{nodes}]}}"#
    )
}

fn frame(id: u64, children: &str) -> String {
    format!(
        r#"{{"id":{id},"name":"F{id}","type":"frame","rect":{{"x":0,"y":0,"w":50,"h":50}},"children":{children}}}"#
    )
}

fn rect(id: u64, parent: Option<u64>) -> String {
    let parent = parent.map(|p| format!(r#","parent":{p}"#)).unwrap_or_default();
    format!(r#"{{"id":{id},"name":"R{id}","type":"rect","rect":{{"x":0,"y":0,"w":5,"h":5}}{parent}}}"#)
}

async fn assert_opens(dir: &Path) -> papercraft::project::format::PageData {
    let text = read(dir, "pages/p1.json").unwrap();
    papercraft::project::format::page_from_json(&text).expect("the merged page opens")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_layer_moved_into_two_frames_still_opens() {
    let base = real_page(&format!("{},{},{}", frame(1, "[]"), frame(2, "[]"), rect(3, None)), "[1,2,3]");
    let (_relay, a, b) = merge_pair("two-parents", &base).await;
    let into = |frame_id: u64| {
        let (f1, f2) = if frame_id == 1 { ("[3]", "[]") } else { ("[]", "[3]") };
        real_page(
            &format!("{},{},{}", frame(1, f1), frame(2, f2), rect(3, Some(frame_id))),
            "[1,2]",
        )
    };
    std::fs::write(a.join("pages/p1.json"), into(1)).unwrap();
    std::fs::write(b.join("pages/p1.json"), into(2)).unwrap();
    sync::sync_once(&b).await.unwrap();
    sync::sync_once(&a).await.unwrap();
    sync::sync_once(&b).await.unwrap();
    for dir in [&a, &b] {
        assert_opens(dir).await.doc.validate().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_layer_added_to_a_deleted_frame_is_kept_and_the_page_opens() {
    let base = real_page(&frame(1, "[]"), "[1]");
    let (_relay, a, b) = merge_pair("orphan", &base).await;
    std::fs::write(a.join("pages/p1.json"), real_page("", "[]")).unwrap();
    let added = real_page(&format!("{},{}", frame(1, "[2]"), rect(2, Some(1))), "[1]");
    std::fs::write(b.join("pages/p1.json"), added).unwrap();
    sync::sync_once(&b).await.unwrap();
    sync::sync_once(&a).await.unwrap();
    sync::sync_once(&b).await.unwrap();
    for dir in [&a, &b] {
        let page = assert_opens(dir).await;
        assert!(page.doc.nodes.contains_key(&papercraft::model::NodeId(2)), "the new layer survived");
    }
}
