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
    let running = relay::spawn_with_data(listen, None, None, Some(Data { dir: data.clone(), library: false })).unwrap();
    let server = format!("http://127.0.0.1:{}", running.port());

    // A shares a project with two pages.
    let a = temp("a");
    std::fs::create_dir_all(a.join("pages")).unwrap();
    std::fs::write(a.join("project.json"), "manifest v1").unwrap();
    std::fs::write(a.join("pages/p1.json"), "page one").unwrap();
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
    assert_eq!(read(&b, "project.json").as_deref(), Some("manifest v1"));
    assert_eq!(read(&b, "pages/p1.json").as_deref(), Some("page one"));
    // Nothing changed: both sides settle.
    assert!(!sync::sync_once(&b).await.unwrap());
    assert!(!sync::sync_once(&a).await.unwrap());

    // B edits one page and adds another; A receives both.
    std::fs::write(b.join("pages/p1.json"), "page one, edited in B").unwrap();
    std::fs::write(b.join("pages/p2.json"), "page two").unwrap();
    sync::sync_once(&b).await.unwrap();
    assert!(sync::sync_once(&a).await.unwrap());
    assert_eq!(
        read(&a, "pages/p1.json").as_deref(),
        Some("page one, edited in B")
    );
    assert_eq!(read(&a, "pages/p2.json").as_deref(), Some("page two"));

    // A deletes a page; B loses it too.
    std::fs::remove_file(a.join("pages/p2.json")).unwrap();
    sync::sync_once(&a).await.unwrap();
    assert!(sync::sync_once(&b).await.unwrap());
    assert!(read(&b, "pages/p2.json").is_none());

    // Both edit the same file before syncing: the one that syncs last (the folder) wins.
    std::fs::write(a.join("pages/p1.json"), "from A").unwrap();
    std::fs::write(b.join("pages/p1.json"), "from B").unwrap();
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
    let data = Data { dir: data.to_path_buf(), library };
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
    let res = http.delete(format!("{server}/p/{token}")).send().await.unwrap();
    assert_eq!(res.status(), 204);
    assert!(data.join(".trash").read_dir().unwrap().next().is_some());
    let listed = http.get(format!("{server}/library")).send().await.unwrap().text().await.unwrap();
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
