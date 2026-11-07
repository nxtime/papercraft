//! Presence through a real relay, and the compatibility with relays and devices that predate it.

use papercraft::mcp::relay;
use papercraft::mcp::store::Data;
use papercraft::presence::{self, Exchange, Link};
use papercraft::sync::{self, Share};
use std::net::SocketAddr;
use std::path::PathBuf;

fn temp(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("papercraft-presence-{label}-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn relay() -> (relay::Running, String) {
    let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let data = Data { dir: temp("data"), library: false };
    let running = relay::spawn_with_data(listen, None, None, Some(data)).unwrap();
    let server = format!("http://127.0.0.1:{}", running.port());
    (running, server)
}

fn device(label: &str) -> Link {
    Link::new(format!("dev-{label}"), label.into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn devices_see_each_others_cursor_and_selection() {
    let (_relay, server) = relay();
    let base = Share::new(&server).base();
    let (a, b) = (device("Laptop"), device("Phone"));
    a.set_local("page-1", Some([10.0, 20.0]), &[4, 5]);
    b.set_local("page-1", None, &[]);

    assert_eq!(presence::exchange(&base, &a).await, Exchange::Done);
    assert_eq!(presence::exchange(&base, &b).await, Exchange::Done);
    let seen = b.peers();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].label.as_str(), seen[0].cursor, seen[0].sel.as_slice()),
        ("Laptop", Some([10.0, 20.0]), [4, 5].as_slice())
    );
    assert!(a.peers().is_empty(), "A has not asked since B arrived");

    // Another project's devices are not shown here.
    let other = Share::new(&server).base();
    let c = device("Tablet");
    c.set_local("p", None, &[]);
    assert_eq!(presence::exchange(&other, &c).await, Exchange::Done);
    assert!(c.peers().is_empty());

    // Moving again (past the relay's per-device rate limit) reaches B.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    a.set_local("page-1", Some([30.0, 40.0]), &[]);
    presence::exchange(&base, &a).await;
    presence::exchange(&base, &b).await;
    assert_eq!(b.peers()[0].cursor, Some([30.0, 40.0]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_relay_refuses_bad_reports_without_storing_anything() {
    let (_relay, server) = relay();
    let url = format!("{}/presence", Share::new(&server).base());
    let client = reqwest::Client::new();
    let put = |body: Vec<u8>| client.put(&url).body(body).send();
    assert_eq!(put(b"not json".to_vec()).await.unwrap().status(), 400);
    assert_eq!(put(br#"{"id":"../x"}"#.to_vec()).await.unwrap().status(), 400);
    assert_eq!(put(vec![b' '; presence::MAX_BODY + 1]).await.unwrap().status(), 413);
    let bad_token = format!("{server}/p/short/presence");
    assert_eq!(client.put(bad_token).body("{}").send().await.unwrap().status(), 404);
    // A newer device may add fields; this relay ignores them.
    let ok = put(br#"{"id":"dev-new","future":[1,2,3]}"#.to_vec()).await.unwrap();
    assert_eq!(ok.status(), 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_device_stops_asking_an_old_relay_and_sync_still_works() {
    // What a relay without the presence route answers: 404 for everything it does not know.
    let old = axum::Router::new().fallback(|| async { axum::http::StatusCode::NOT_FOUND });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, old).await.ok();
    });
    let link = device("Laptop");
    link.set_local("p", None, &[]);
    let base = Share::new(&format!("http://{addr}")).base();
    assert_eq!(presence::exchange(&base, &link).await, Exchange::Unsupported);
    assert!(!link.is_supported(), "no further asking");

    // A device that never reports presence (an older build) syncs with the new relay as before.
    let (_relay, server) = relay();
    let dir = temp("old-client");
    Share::new(&server).save(&dir).unwrap();
    std::fs::write(dir.join("project.json"), r#"{"manifest":1}"#).unwrap();
    sync::sync_once(&dir).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn something_other_than_a_relay_answering_200_is_unsupported_too() {
    let page = axum::Router::new().fallback(|| async { "<html>the web app</html>" });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, page).await.ok();
    });
    let link = device("Laptop");
    link.set_local("p", None, &[]);
    let base = Share::new(&format!("http://{addr}")).base();
    assert_eq!(presence::exchange(&base, &link).await, Exchange::Unsupported);
}
