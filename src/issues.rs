//! Issues for PaperCraft itself, filed on its repository from inside the app: by the user (a form)
//! or drafted by the chat agent when the app gets in its way (`report_issue`, sent only when the
//! user presses Send). Same rules as the `papercraft-self-report` skill: kinds `bug`, `docs`,
//! `feature` as a `[kind]` title prefix, and no new issue when an open or closed one already has
//! a matching title.
//!
//! Transport: a token saved in Settings (Gitea or GitHub REST API); without one, the `tea` CLI
//! with the user's own login for that server.

use serde_json::{Value, json};
use std::process::Command;

pub const KINDS: [&str; 3] = ["bug", "docs", "feature"];
pub const DEFAULT_REPO: &str = "https://git.marcospaulo.dev.br/masi/papercraft";
const KEY_USER: &str = "issues-token";
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
/// Titles shorter than this are not compared: too little to call two issues the same.
const MIN_MATCH: usize = 12;

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub kind: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filed {
    New { number: u64, url: String },
    /// An issue with a matching title exists already; nothing was filed.
    Duplicate { number: u64, url: String, title: String },
}

/// `(scheme://host, owner, repo)` of a repository URL.
pub fn repo_parts(url: &str) -> Option<(String, String, String)> {
    let u = reqwest::Url::parse(url.trim().trim_end_matches(".git")).ok()?;
    let mut seg = u.path_segments()?.filter(|s| !s.is_empty());
    let (owner, repo) = (seg.next()?.to_owned(), seg.next()?.to_owned());
    Some((format!("{}://{}", u.scheme(), u.host_str()?), owner, repo))
}

pub fn token() -> Option<String> {
    keyring::Entry::new("PaperCraft", KEY_USER).ok()?.get_password().ok().filter(|t| !t.trim().is_empty())
}

pub fn set_token(t: &str) -> Result<(), String> {
    let e = keyring::Entry::new("PaperCraft", KEY_USER).map_err(|e| e.to_string())?;
    if t.trim().is_empty() {
        return match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    e.set_password(t.trim()).map_err(|e| e.to_string())
}

/// Files the draft (or finds the issue it duplicates). Blocking: call from a worker thread.
pub fn file(repo: &str, d: &Draft) -> Result<Filed, String> {
    let (host, owner, name) = repo_parts(repo).ok_or("the issue repository is not a repository URL (Settings, Updates)")?;
    if !KINDS.contains(&d.kind.as_str()) {
        return Err(format!("kind must be one of {}", KINDS.join(", ")));
    }
    if d.title.trim().is_empty() {
        return Err("an issue needs a title".into());
    }
    let title = format!("[{}] {}", d.kind, d.title.trim());
    let body = format!(
        "{}\n\n---\nFiled from PaperCraft {} ({}).",
        d.body.trim(),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    let existing = match token() {
        Some(t) => rest_list(&host, &owner, &name, &t)?,
        None => tea_list(&host, &owner, &name)?,
    };
    if let Some((number, url, t)) = existing.into_iter().find(|(_, _, t)| same_issue(t, &title)) {
        return Ok(Filed::Duplicate { number, url, title: t });
    }
    let (number, url) = match token() {
        Some(t) => rest_create(&host, &owner, &name, &t, &title, &body)?,
        None => tea_create(&host, &owner, &name, &title, &body, &d.kind)?,
    };
    Ok(Filed::New { number, url })
}

/// Two titles describe the same issue: one contains the other, ignoring case and the `[kind]`.
fn same_issue(a: &str, b: &str) -> bool {
    let norm = |t: &str| {
        let t = t.trim().to_lowercase();
        t.strip_prefix('[').and_then(|r| r.split_once(']')).map_or(t.as_str(), |(_, rest)| rest).trim().to_owned()
    };
    let (a, b) = (norm(a), norm(b));
    a.len().min(b.len()) >= MIN_MATCH && (a.contains(&b) || b.contains(&a))
}

fn github(host: &str) -> bool {
    host.ends_with("://github.com")
}

fn api(host: &str, owner: &str, name: &str) -> String {
    if github(host) { format!("https://api.github.com/repos/{owner}/{name}/issues") } else { format!("{host}/api/v1/repos/{owner}/{name}/issues") }
}

fn rest(host: &str, token: &str, req: reqwest::RequestBuilder) -> Result<Value, String> {
    let auth = if github(host) { format!("Bearer {token}") } else { format!("token {token}") };
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let resp = req.header("authorization", auth).header("user-agent", "PaperCraft").header("accept", "application/json").timeout(TIMEOUT).send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(format!("the issue tracker answered {status}: {}", v["message"].as_str().unwrap_or("")));
        }
        Ok(v)
    })
}

fn rest_list(host: &str, owner: &str, name: &str, token: &str) -> Result<Vec<(u64, String, String)>, String> {
    let c = reqwest::Client::new();
    let url = format!("{}?state=all&limit=100&per_page=100&type=issues", api(host, owner, name));
    let v = rest(host, token, c.get(url))?;
    Ok(v.as_array().into_iter().flatten().filter_map(|i| Some((i["number"].as_u64()?, i["html_url"].as_str()?.to_owned(), i["title"].as_str()?.to_owned()))).collect())
}

fn rest_create(host: &str, owner: &str, name: &str, token: &str, title: &str, body: &str) -> Result<(u64, String), String> {
    let c = reqwest::Client::new();
    let v = rest(host, token, c.post(api(host, owner, name)).json(&json!({"title": title, "body": body})))?;
    Ok((v["number"].as_u64().ok_or("the tracker returned no issue number")?, v["html_url"].as_str().unwrap_or_default().to_owned()))
}

/// The `tea` login for a server (its name in tea's config), so tea talks to the right one.
fn tea_login(host: &str) -> Option<String> {
    // tea keeps its config in ~/.config/tea on every platform (and the OS config folder on some).
    let cfg = [dirs::home_dir().map(|h| h.join(".config")), dirs::config_dir()]
        .into_iter()
        .flatten()
        .find_map(|d| std::fs::read_to_string(d.join("tea").join("config.yml")).ok())?;
    let mut name = None;
    for line in cfg.lines().map(str::trim) {
        if let Some(n) = line.strip_prefix("- name:") {
            name = Some(n.trim().to_owned());
        } else if let Some(u) = line.strip_prefix("url:")
            && u.trim().trim_end_matches('/') == host
        {
            return name;
        }
    }
    None
}

fn tea(args: &[&str]) -> Result<String, String> {
    let bin = crate::chat::path::which("tea").ok_or("no issue token in Settings and no `tea` CLI: add a token in Settings, Updates, Issues")?;
    let out = Command::new(bin).args(args).env("PATH", crate::chat::path::resolved_path()).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("tea: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn tea_list(host: &str, owner: &str, name: &str) -> Result<Vec<(u64, String, String)>, String> {
    let login = tea_login(host).ok_or_else(|| format!("tea has no login for {host}: run `tea login add`, or add a token in Settings"))?;
    let repo = format!("{owner}/{name}");
    let out = tea(&["issues", "ls", "--login", &login, "--repo", &repo, "--state", "all", "--limit", "200", "--output", "json", "--fields", "index,title,url"])?;
    let v: Value = serde_json::from_str(&out).map_err(|e| format!("tea output: {e}"))?;
    Ok(v.as_array().into_iter().flatten().filter_map(|i| Some((i["index"].as_str()?.parse().ok()?, i["url"].as_str()?.to_owned(), i["title"].as_str()?.to_owned()))).collect())
}

fn tea_create(host: &str, owner: &str, name: &str, title: &str, body: &str, kind: &str) -> Result<(u64, String), String> {
    let login = tea_login(host).ok_or_else(|| format!("tea has no login for {host}"))?;
    let repo = format!("{owner}/{name}");
    // A label only when the repository has it; the [kind] prefix carries it otherwise.
    let labels = tea(&["labels", "ls", "--login", &login, "--repo", &repo, "--output", "json"]).ok().filter(|l| l.contains(&format!("\"{kind}\"")));
    let mut args = vec!["issues", "create", "--login", &login, "--repo", &repo, "--title", title, "--description", body];
    if labels.is_some() {
        args.extend(["--labels", kind]);
    }
    tea(&args)?;
    // tea prints a table, not JSON: read the new issue back by its title.
    tea_list(host, owner, name)?
        .into_iter()
        .find(|(_, _, t)| t == title)
        .map(|(n, u, _)| (n, u))
        .ok_or_else(|| "tea created the issue but it was not found afterwards".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_urls_and_duplicates() {
        assert_eq!(repo_parts("https://git.marcospaulo.dev.br/masi/papercraft.git"), Some(("https://git.marcospaulo.dev.br".into(), "masi".into(), "papercraft".into())));
        assert_eq!(repo_parts("not a url"), None);
        assert!(same_issue("[bug] write_html: border-left becomes a full border", "[docs] Write_HTML: border-left becomes a full border on frames"));
        assert!(!same_issue("[bug] short", "[bug] short"), "too short to call the same");
        assert!(!same_issue("[bug] rename field cannot be left", "[feature] copy layers between pages"));
    }
}
