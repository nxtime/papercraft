//! The join page against a fake Logto: the four Management API calls, the rollback when the
//! organization step fails, and an invite that is used up.

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use papercraft::mcp::relay_invite;
use papercraft::mcp::relay_join::{Join, JoinConfig, router};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Fake {
    /// "METHOD /path" with the body, in arrival order.
    calls: Mutex<Vec<(String, String)>>,
    auths: Mutex<Vec<String>>,
    fail_org: Mutex<bool>,
}

async fn fake_logto(
    State(f): State<Arc<Fake>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: String,
) -> (StatusCode, String) {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    f.auths.lock().expect("lock").push(auth);
    let path = uri.path().to_string();
    f.calls
        .lock()
        .expect("lock")
        .push((format!("{method} {path}"), body));
    let fail_org = *f.fail_org.lock().expect("lock");
    match (method.as_str(), path.as_str()) {
        ("POST", "/oidc/token") => (StatusCode::OK, r#"{"access_token":"AT"}"#.into()),
        ("POST", "/api/users") => (StatusCode::OK, r#"{"id":"u1"}"#.into()),
        ("POST", "/api/organizations/org1/users") if fail_org => {
            (StatusCode::INTERNAL_SERVER_ERROR, String::new())
        }
        ("POST", _) => (StatusCode::CREATED, String::new()),
        ("DELETE", _) => (StatusCode::NO_CONTENT, String::new()),
        _ => (StatusCode::NOT_FOUND, String::new()),
    }
}

async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await });
    addr
}

struct Setup {
    base: String,
    fake: Arc<Fake>,
    dir: PathBuf,
}

async fn setup() -> Setup {
    let fake = Arc::new(Fake::default());
    let logto = serve(Router::new().fallback(fake_logto).with_state(fake.clone())).await;
    let dir = std::env::temp_dir().join(format!("pc-join-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).expect("dir");
    let cfg = JoinConfig {
        endpoint: format!("http://{logto}"),
        client_id: "cid".into(),
        client_secret: "secret".into(),
        org: "org1".into(),
        resource: "https://default.logto.app/api".into(),
    };
    let join = Join::new(cfg, dir.clone()).expect("join");
    let addr = serve(router(join)).await;
    Setup {
        base: format!("http://{addr}"),
        fake,
        dir,
    }
}

const FORM: &str = "name=Ada+L&username=ada_l&email=ada%40example.com&password=correct+horse&confirm=correct+horse";

async fn post(url: &str, body: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body.to_string())
        .send()
        .await
        .expect("request");
    (resp.status().as_u16(), resp.text().await.expect("text"))
}

async fn get(url: &str) -> (u16, String) {
    let resp = reqwest::get(url).await.expect("request");
    (resp.status().as_u16(), resp.text().await.expect("text"))
}

#[tokio::test]
async fn a_join_makes_the_user_adds_them_and_uses_up_the_invite() {
    let s = setup().await;
    let token = relay_invite::create(&s.dir, 1, "editor", 1).expect("invite");
    let url = format!("{}/join/{token}", s.base);

    let (status, page) = get(&url).await;
    assert_eq!(status, 200);
    assert!(page.contains("name=\"confirm\""));

    let (status, page) = post(&url, FORM).await;
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("Account created"));

    let calls: Vec<String> = s
        .fake
        .calls
        .lock()
        .expect("lock")
        .iter()
        .map(|c| c.0.clone())
        .collect();
    assert_eq!(
        calls,
        [
            "POST /oidc/token",
            "POST /api/users",
            "POST /api/organizations/org1/users",
            "POST /api/organizations/org1/users/u1/roles",
        ]
    );
    let bodies: Vec<String> = s
        .fake
        .calls
        .lock()
        .expect("lock")
        .iter()
        .map(|c| c.1.clone())
        .collect();
    assert!(bodies[0].contains("grant_type=client_credentials") && bodies[0].contains("scope=all"));
    assert!(bodies[1].contains("\"primaryEmail\":\"ada@example.com\""));
    assert!(bodies[3].contains("\"organizationRoleNames\":[\"editor\"]"));
    let auths = s.fake.auths.lock().expect("lock").clone();
    assert!(auths[0].starts_with("Basic "));
    assert_eq!(auths[1], "Bearer AT");

    assert_eq!(get(&url).await.0, 404, "a single-use invite is spent");
    assert_eq!(post(&url, FORM).await.0, 404);
}

#[tokio::test]
async fn a_failed_organization_step_removes_the_new_user() {
    let s = setup().await;
    *s.fake.fail_org.lock().expect("lock") = true;
    let token = relay_invite::create(&s.dir, 1, "editor", 1).expect("invite");
    let url = format!("{}/join/{token}", s.base);

    let (status, page) = post(&url, FORM).await;
    assert_eq!(status, 502);
    assert!(page.contains("ada_l"), "what they typed stays");
    assert!(!page.contains("correct horse"), "passwords are not echoed");

    let calls = s.fake.calls.lock().expect("lock").clone();
    assert_eq!(
        calls.last().map(|c| c.0.as_str()),
        Some("DELETE /api/users/u1")
    );
    assert_eq!(get(&url).await.0, 200, "the invite was not spent");
}

#[tokio::test]
async fn bad_tokens_and_bad_forms_are_refused() {
    let s = setup().await;
    let (status, page) = get(&format!("{}/join/{}", s.base, "0".repeat(64))).await;
    assert_eq!(status, 404);
    assert!(page.contains("This invite has expired or is not valid"));

    let token = relay_invite::create(&s.dir, 1, "editor", 0).expect("invite");
    let url = format!("{}/join/{token}", s.base);
    let (status, _) = post(&url, &FORM.replace("confirm=correct+horse", "confirm=nope")).await;
    assert_eq!(status, 400);
    assert!(
        s.fake.calls.lock().expect("lock").is_empty(),
        "Logto is not called"
    );
}

#[tokio::test]
async fn posts_are_rate_limited() {
    let s = setup().await;
    let url = format!("{}/join/{}", s.base, "0".repeat(64));
    for _ in 0..10 {
        assert_eq!(post(&url, FORM).await.0, 404);
    }
    assert_eq!(post(&url, FORM).await.0, 429);
}
