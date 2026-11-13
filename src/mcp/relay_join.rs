//! `/join/<token>`: an invite link that lets a person create their own Logto account and puts
//! it in the team's organization, so public sign-up can stay closed.
//!
//! Secrets never reach the log: only that a join worked or failed and Logto's status code.

use super::relay_invite;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

const DEFAULT_RESOURCE: &str = "https://default.logto.app/api";
/// POSTs allowed per window, across all tokens.
const MAX_POSTS_PER_WINDOW: usize = 10;
const WINDOW: Duration = Duration::from_secs(60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const INVALID: &str = "This invite has expired or is not valid";
const TRY_LATER: &str = "<h1>Something went wrong. Try again later.</h1>";

#[derive(Clone)]
pub struct JoinConfig {
    pub endpoint: String,
    pub client_id: String,
    pub client_secret: String,
    pub org: String,
    pub resource: String,
}

impl JoinConfig {
    /// `Some` only when every required variable is set.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let need = |k| get(k).filter(|v| !v.is_empty());
        Some(Self {
            endpoint: need("PAPERCRAFT_JOIN_LOGTO_ENDPOINT")?
                .trim_end_matches('/')
                .to_string(),
            client_id: need("PAPERCRAFT_JOIN_CLIENT_ID")?,
            client_secret: need("PAPERCRAFT_JOIN_CLIENT_SECRET")?,
            org: need("PAPERCRAFT_JOIN_ORG")?,
            resource: need("PAPERCRAFT_JOIN_RESOURCE").unwrap_or_else(|| DEFAULT_RESOURCE.into()),
        })
    }
}

pub struct Join {
    cfg: JoinConfig,
    dir: PathBuf,
    client: reqwest::Client,
    /// One join at a time, so a single-use invite cannot be spent twice by a race.
    gate: tokio::sync::Mutex<()>,
    posts: Mutex<VecDeque<Instant>>,
}

impl Join {
    pub fn new(cfg: JoinConfig, dir: PathBuf) -> Result<Arc<Self>, String> {
        let client = crate::net::client_builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        Ok(Arc::new(Self {
            cfg,
            dir,
            client,
            gate: tokio::sync::Mutex::new(()),
            posts: Mutex::new(VecDeque::new()),
        }))
    }

    fn rate_limited(&self) -> bool {
        let mut posts = self.posts.lock().unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        while posts
            .front()
            .is_some_and(|t| now.duration_since(*t) >= WINDOW)
        {
            posts.pop_front();
        }
        if posts.len() >= MAX_POSTS_PER_WINDOW {
            return true;
        }
        posts.push_back(now);
        false
    }
}

pub fn router(join: Arc<Join>) -> Router {
    Router::new()
        .route("/join/{token}", get(show).post(submit))
        .with_state(join)
}

#[derive(Default, Clone)]
struct Fields {
    name: String,
    username: String,
    email: String,
    password: String,
    confirm: String,
}

/// Why a Logto call failed; `in_use` is Logto saying the username or email exists.
struct Fail {
    status: u16,
    in_use: bool,
}

impl Fail {
    fn status(status: u16) -> Self {
        Self {
            status,
            in_use: false,
        }
    }
}

fn escape(s: &str) -> String {
    s.chars()
        .fold(String::with_capacity(s.len()), |mut out, c| {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                c => out.push(c),
            }
            out
        })
}

fn percent_encode(s: &str) -> String {
    s.bytes().fold(String::new(), |mut out, b| {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
        out
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let digits = (
                    bytes.get(i + 1).copied().and_then(hex),
                    bytes.get(i + 2).copied().and_then(hex),
                );
                if let (Some(hi), Some(lo)) = digits {
                    out.push((hi * 16 + lo) as u8);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_form(body: &str) -> Fields {
    let mut f = Fields::default();
    for pair in body.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = percent_decode(v);
        match percent_decode(k).as_str() {
            "name" => f.name = v,
            "username" => f.username = v,
            "email" => f.email = v,
            "password" => f.password = v,
            "confirm" => f.confirm = v,
            _ => {}
        }
    }
    f
}

fn valid_username(u: &str) -> bool {
    let b = u.as_bytes();
    (3..=32).contains(&b.len())
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

fn valid_email(e: &str) -> bool {
    e.len() <= 254
        && e.split_once('@').is_some_and(|(local, host)| {
            !local.is_empty()
                && !host.contains('@')
                && host.contains('.')
                && !host.starts_with('.')
                && !host.ends_with('.')
        })
}

/// The first problem with the form, worded for the person filling it in.
fn validate(f: &Fields) -> Option<&'static str> {
    let name = f.name.chars().count();
    let password = f.password.chars().count();
    if !(1..=80).contains(&name) {
        Some("Enter your name (up to 80 characters).")
    } else if !valid_username(&f.username) {
        Some(
            "The username needs 3 to 32 letters, digits or underscores, and cannot start with a digit.",
        )
    } else if !valid_email(&f.email) {
        Some("Enter a valid email address.")
    } else if !(10..=128).contains(&password) {
        Some("The password needs 10 to 128 characters.")
    } else if f.password != f.confirm {
        Some("The two passwords do not match.")
    } else {
        None
    }
}

const STYLE: &str = "body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#14161a;color:#e8eaed;font:16px/1.5 system-ui,sans-serif}\
main{width:100%;max-width:26rem;padding:1.5rem;box-sizing:border-box}h1{font-size:1.4rem;margin:0 0 1rem}\
label{display:block;margin:.8rem 0 .25rem;font-size:.9rem;color:#aab}input{width:100%;box-sizing:border-box;padding:.65rem;border:1px solid #3a3f4a;border-radius:6px;background:#1d2026;color:inherit;font:inherit}\
button,a.b{display:inline-block;margin-top:1.2rem;padding:.7rem 1.2rem;border:0;border-radius:6px;background:#4c8dff;color:#fff;font:inherit;text-decoration:none;cursor:pointer}\
.err{margin:0 0 1rem;padding:.6rem .8rem;border-radius:6px;background:#3a1d22;color:#ffb4bd}";

fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{STYLE}</style></head><body><main>{body}</main></body></html>",
        escape(title)
    );
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        html,
    )
        .into_response()
}

fn invalid() -> Response {
    page(
        StatusCode::NOT_FOUND,
        "Invite not valid",
        &format!("<h1>{INVALID}</h1>"),
    )
}

fn server_error(e: &str) -> Response {
    log::error!("join: {e}");
    page(StatusCode::INTERNAL_SERVER_ERROR, "Error", TRY_LATER)
}

fn form_page(status: StatusCode, token: &str, f: &Fields, error: Option<&str>) -> Response {
    let input = |label: &str, name: &str, kind: &str, value: &str, extra: &str| {
        format!(
            "<label for=\"{name}\">{label}</label><input id=\"{name}\" name=\"{name}\" type=\"{kind}\" value=\"{}\" required {extra}>",
            escape(value)
        )
    };
    let error = error
        .map(|e| format!("<p class=\"err\" role=\"alert\">{}</p>", escape(e)))
        .unwrap_or_default();
    let body = format!(
        "<h1>Join PaperCraft</h1>{error}<form method=\"post\" action=\"/join/{}\">{}{}{}{}{}<button type=\"submit\">Create account</button></form>",
        escape(token),
        input(
            "Name",
            "name",
            "text",
            &f.name,
            "maxlength=\"80\" autocomplete=\"name\""
        ),
        input(
            "Username",
            "username",
            "text",
            &f.username,
            "maxlength=\"32\" autocomplete=\"username\" autocapitalize=\"none\""
        ),
        input(
            "Email",
            "email",
            "email",
            &f.email,
            "maxlength=\"254\" autocomplete=\"email\""
        ),
        input(
            "Password (10 or more characters)",
            "password",
            "password",
            "",
            "minlength=\"10\" maxlength=\"128\" autocomplete=\"new-password\""
        ),
        input(
            "Confirm password",
            "confirm",
            "password",
            "",
            "minlength=\"10\" maxlength=\"128\" autocomplete=\"new-password\""
        ),
    );
    page(status, "Join PaperCraft", &body)
}

async fn show(State(join): State<Arc<Join>>, Path(token): Path<String>) -> Response {
    match relay_invite::find(&join.dir, &token, relay_invite::now()) {
        Ok(Some(_)) => form_page(StatusCode::OK, &token, &Fields::default(), None),
        Ok(None) => invalid(),
        Err(e) => server_error(&e),
    }
}

async fn submit(
    State(join): State<Arc<Join>>,
    Path(token): Path<String>,
    body: String,
) -> Response {
    if join.rate_limited() {
        return page(
            StatusCode::TOO_MANY_REQUESTS,
            "Slow down",
            "<h1>Too many attempts. Try again in a minute.</h1>",
        );
    }
    let _one_at_a_time = join.gate.lock().await;
    let invite = match relay_invite::find(&join.dir, &token, relay_invite::now()) {
        Ok(Some(i)) => i,
        Ok(None) => return invalid(),
        Err(e) => return server_error(&e),
    };
    let fields = parse_form(&body);
    if let Some(problem) = validate(&fields) {
        return form_page(StatusCode::BAD_REQUEST, &token, &fields, Some(problem));
    }
    let retry = |status, msg| form_page(status, &token, &fields, Some(msg));
    match join.create_member(&fields, &invite.role).await {
        Ok(()) => {
            log::info!("join: account created");
            if let Err(e) = relay_invite::consume(&join.dir, &token) {
                log::error!("join: {e}");
            }
            page(
                StatusCode::OK,
                "Account created",
                "<h1>Account created</h1><p>Sign in to PaperCraft with your username and password.</p><a class=\"b\" href=\"/\">Open PaperCraft</a>",
            )
        }
        Err(fail) if fail.in_use => retry(
            StatusCode::CONFLICT,
            "That username or email is already taken.",
        ),
        Err(fail) if fail.status == 400 || fail.status == 422 => retry(
            StatusCode::BAD_REQUEST,
            "These details were not accepted. Try a stronger password or another username.",
        ),
        Err(_) => retry(
            StatusCode::BAD_GATEWAY,
            "The account could not be created right now. Try again later.",
        ),
    }
}

impl Join {
    /// Steps a to d of the join; a half-made user is deleted again.
    async fn create_member(&self, f: &Fields, role: &str) -> Result<(), Fail> {
        use reqwest::Method;
        let access = self.access_token().await?;
        let user = self
            .logto(
                &access,
                Method::POST,
                "/api/users",
                Some(json!({
                    "username": f.username,
                    "primaryEmail": f.email,
                    "password": f.password,
                    "name": f.name,
                })),
            )
            .await?;
        let id = user
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(Fail::status(0))?;
        let org = percent_encode(&self.cfg.org);
        let uid = percent_encode(&id);
        let added = async {
            self.logto(
                &access,
                Method::POST,
                &format!("/api/organizations/{org}/users"),
                Some(json!({ "userIds": [id] })),
            )
            .await?;
            self.logto(
                &access,
                Method::POST,
                &format!("/api/organizations/{org}/users/{uid}/roles"),
                Some(json!({ "organizationRoleNames": [role] })),
            )
            .await
        }
        .await;
        if let Err(fail) = added {
            log::error!(
                "join: organization step failed (status {}), removing the new user",
                fail.status
            );
            if let Err(undo) = self
                .logto(&access, Method::DELETE, &format!("/api/users/{uid}"), None)
                .await
            {
                log::error!("join: rollback failed (status {})", undo.status);
            }
            return Err(Fail::status(fail.status));
        }
        Ok(())
    }

    async fn access_token(&self) -> Result<String, Fail> {
        let body = format!(
            "grant_type=client_credentials&resource={}&scope=all",
            percent_encode(&self.cfg.resource)
        );
        let resp = self
            .client
            .post(format!("{}/oidc/token", self.cfg.endpoint))
            .basic_auth(&self.cfg.client_id, Some(&self.cfg.client_secret))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|_| Fail::status(0))?;
        let status = resp.status().as_u16();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        match (status, v.get("access_token").and_then(Value::as_str)) {
            (200, Some(t)) => Ok(t.to_string()),
            _ => {
                log::error!("join: Logto token request failed (status {status})");
                Err(Fail::status(status))
            }
        }
    }

    async fn logto(
        &self,
        access: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, Fail> {
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.cfg.endpoint))
            .bearer_auth(access);
        if let Some(b) = &body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(|_| Fail::status(0))?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(v);
        }
        let in_use = status == StatusCode::CONFLICT
            || v.get("code")
                .and_then(Value::as_str)
                .is_some_and(|c| c.contains("in_use"));
        Err(Fail {
            status: status.as_u16(),
            in_use,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> Fields {
        Fields {
            name: "Ada".into(),
            username: "ada_l".into(),
            email: "ada@example.com".into(),
            password: "correct horse".into(),
            confirm: "correct horse".into(),
        }
    }

    #[test]
    fn a_good_form_passes() {
        assert_eq!(validate(&good()), None);
    }

    #[test]
    fn each_field_is_checked() {
        let bad = |edit: fn(&mut Fields)| {
            let mut f = good();
            edit(&mut f);
            validate(&f).is_some()
        };
        assert!(bad(|f| f.name = String::new()));
        assert!(bad(|f| f.name = "x".repeat(81)));
        assert!(bad(|f| f.username = "1ada".into()));
        assert!(bad(|f| f.username = "ab".into()));
        assert!(bad(|f| f.username = "a-b-c".into()));
        assert!(bad(|f| f.email = "ada@localhost".into()));
        assert!(bad(|f| f.email = "a@b@c.com".into()));
        assert!(bad(|f| f.email = format!("{}@x.com", "a".repeat(250))));
        assert!(bad(|f| f.password = "short".into()));
        assert!(bad(|f| f.confirm = "different pass".into()));
    }

    #[test]
    fn forms_decode_and_pages_escape() {
        let f = parse_form("name=A%26B+%3Cb%3E&username=u&email=a%40b.co&password=100%");
        assert_eq!(f.name, "A&B <b>");
        assert_eq!(f.email, "a@b.co");
        assert_eq!(f.password, "100%");
        assert_eq!(
            escape("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn config_needs_every_variable() {
        assert!(JoinConfig::from_lookup(|k| Some(format!("v-{k}"))).is_some());
        assert!(
            JoinConfig::from_lookup(|k| (k != "PAPERCRAFT_JOIN_ORG").then(|| "x".into())).is_none()
        );
        let c = JoinConfig::from_lookup(|k| (k != "PAPERCRAFT_JOIN_RESOURCE").then(|| "x/".into()))
            .expect("config");
        assert_eq!(c.resource, DEFAULT_RESOURCE);
        assert_eq!(c.endpoint, "x");
    }
}
