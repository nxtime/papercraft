//! Shared projects end to end: two project folders kept in step through a real relay.

use papercraft::mcp::relay;
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
    let running = relay::spawn_with_data(listen, None, None, Some(data.clone())).unwrap();
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
