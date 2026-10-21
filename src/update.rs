//! Application updater backend.
//!
//! Talks to the official Gitea release API, picks the installer that matches this
//! machine and downloads it on request. It never installs, replaces or executes
//! anything: the caller decides what to do with the downloaded file.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use semver::Version;
use serde::Deserialize;

/// Latest-release API of the official repository.
pub const RELEASES_URL: &str =
    "https://git.marcospaulo.dev.br/api/v1/repos/masi/papercraft/releases/latest";

/// A downloadable release artifact.
#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// A published release plus the installer that matches this machine, if any.
#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub asset: Option<Asset>,
}

/// The only host allowed to serve release metadata and installers.
const OFFICIAL_HOST: &str = "git.marcospaulo.dev.br";
/// Repository path of the official project.
const REPO_PREFIX: &str = "/masi/papercraft/releases/";
/// Repository path of uploaded release files.
const DOWNLOAD_PREFIX: &str = "/masi/papercraft/releases/download/";
/// Metadata is a small JSON document; anything bigger is refused.
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
/// Installers are desktop packages, not disk images of the internet.
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
/// Deadline for every request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

/// Parses a release feed and reports the update, if any.
///
/// `os` and `arch` are the `std::env::consts` values of the running machine.
/// Returns `Ok(None)` when the release is not newer than `current`.
pub fn parse_release(
    json: &str,
    current: &str,
    os: &str,
    arch: &str,
) -> Result<Option<Release>, String> {
    let raw: RawRelease =
        serde_json::from_str(json).map_err(|e| format!("could not read the release feed: {e}"))?;
    if raw.draft {
        return Err("the release is a draft".into());
    }
    if raw.prerelease {
        return Err("the release is a prerelease".into());
    }

    let latest = parse_version(&raw.tag_name)?;
    if !latest.pre.is_empty() {
        return Err("the release is a prerelease".into());
    }
    let current = parse_version(current)?;
    if latest <= current {
        return Ok(None);
    }

    let url = raw.html_url.clone().unwrap_or_default();
    validate_official_url(&url, REPO_PREFIX)?;

    let mut assets = Vec::with_capacity(raw.assets.len());
    for raw_asset in raw.assets {
        let asset = Asset {
            name: raw_asset.name,
            url: raw_asset.browser_download_url,
            size: raw_asset.size,
        };
        check_asset_safe(&asset)?;
        assets.push(asset);
    }

    let version = latest.to_string();
    let asset = select_asset(&assets, &version, os, arch);
    Ok(Some(Release {
        version,
        notes: raw.body.unwrap_or_default(),
        url,
        asset,
    }))
}

/// Checks the official feed for a newer release.
///
/// The request is bounded: no redirects, a 30 s deadline and a metadata cap.
pub async fn check() -> Result<Option<Release>, String> {
    let client = client()?;
    let response = client
        .get(RELEASES_URL)
        .send()
        .await
        .map_err(|e| format!("could not reach the release server: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the release server answered HTTP {status}"));
    }
    let body = read_capped(response, MAX_METADATA_BYTES).await?;
    let text =
        String::from_utf8(body).map_err(|_| "the release feed is not valid UTF-8".to_string())?;
    parse_release(
        &text,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// Downloads an installer into the user cache and returns its path.
///
/// The file is written to a temporary name first and only renamed once it is
/// complete. Nothing is executed and the running binary is never touched.
pub async fn download(asset: &Asset, progress: impl Fn(u64, u64)) -> Result<PathBuf, String> {
    validate_asset(asset)?;

    let dir = update_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let temp = dir.join(format!("{}.{:016x}.part", asset.name, fastrand::u64(..)));
    let dest = dir.join(&asset.name);

    let client = client()?;
    let mut response = client
        .get(&asset.url)
        .send()
        .await
        .map_err(|e| format!("could not download the installer: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the download failed with HTTP {status}"));
    }
    if let Some(len) = response.content_length()
        && len > MAX_DOWNLOAD_BYTES
    {
        return Err(format!("the installer is too large ({len} bytes)"));
    }

    let total = if asset.size > 0 {
        asset.size
    } else {
        response.content_length().unwrap_or(0)
    };

    let transferred = async {
        let mut file = std::fs::File::create(&temp)
            .map_err(|e| format!("could not create {}: {e}", temp.display()))?;
        let mut downloaded: u64 = 0;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("the download was interrupted: {e}"))?
        {
            downloaded += chunk.len() as u64;
            if downloaded > MAX_DOWNLOAD_BYTES {
                return Err("the installer is too large".to_string());
            }
            file.write_all(&chunk)
                .map_err(|e| format!("could not write {}: {e}", temp.display()))?;
            progress(downloaded, total);
        }
        file.sync_all()
            .map_err(|e| format!("could not save installer: {e}"))?;
        if total > 0 && downloaded != total {
            return Err(format!(
                "the installer is incomplete: expected {total} bytes, got {downloaded}"
            ));
        }
        Ok(())
    }
    .await;
    if let Err(error) = transferred {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }

    if dest.exists() {
        std::fs::remove_file(&dest)
            .map_err(|e| format!("could not replace {}: {e}", dest.display()))?;
    }
    std::fs::rename(&temp, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("could not store {}: {e}", dest.display())
    })?;
    Ok(dest)
}

/// Checks that an asset is a PaperCraft desktop installer served by the official host.
///
/// Called again on the download boundary: an asset handed in from the outside is
/// never trusted just because it was parsed from the feed.
pub fn validate_asset(asset: &Asset) -> Result<(), String> {
    check_asset_safe(asset)?;
    let name = asset.name.as_str();
    if !name.starts_with("papercraft-") {
        return Err(format!("not a PaperCraft installer: {name}"));
    }
    if !(name.ends_with(".dmg") || name.ends_with(".msi")) {
        return Err(format!("not a desktop installer: {name}"));
    }
    Ok(())
}

/// The cache folder that holds downloaded installers.
fn update_dir() -> Result<PathBuf, String> {
    let base = dirs::cache_dir().ok_or("this system has no cache folder")?;
    Ok(base.join("PaperCraft").join("updates"))
}

/// Checks that an asset is structurally safe: official URL, plain file name.
fn check_asset_safe(asset: &Asset) -> Result<(), String> {
    let name = asset.name.as_str();
    if name.is_empty() {
        return Err("the release lists an asset without a name".into());
    }
    if name != name.trim() {
        return Err(format!("unsafe asset name: {name}"));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!("unsafe asset name (path separator): {name}"));
    }
    if name.contains("..") {
        return Err(format!("unsafe asset name (traversal): {name}"));
    }
    if asset.size > MAX_DOWNLOAD_BYTES {
        return Err(format!("the installer is too large ({} bytes)", asset.size));
    }
    validate_official_url(&asset.url, DOWNLOAD_PREFIX)?;
    let base = asset.url.rsplit('/').next().unwrap_or_default();
    if base != name {
        return Err(format!("the asset URL does not match its name: {name}"));
    }
    Ok(())
}

/// Accepts only plain HTTPS URLs of the official repository.
fn validate_official_url(url: &str, prefix: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("invalid release URL: {e}"))?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some(OFFICIAL_HOST)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.fragment().is_some()
        || parsed.query().is_some()
        || !parsed.path().starts_with(prefix)
    {
        return Err("the release URL is outside the official repository".into());
    }
    Ok(())
}

/// Picks the installer for this platform: exact native build first, then universal.
fn select_asset(assets: &[Asset], version: &str, os: &str, arch: &str) -> Option<Asset> {
    let mut wanted: Vec<String> = Vec::new();
    match os {
        "macos" | "darwin" => {
            let native = macos_arch(arch)?;
            wanted.push(format!("papercraft-{version}-macos-{native}.dmg"));
            wanted.push(format!("papercraft-{version}-macos-universal.dmg"));
        }
        "windows" => {
            if let Some(windows) = windows_arch(arch) {
                wanted.push(format!("papercraft-{version}-windows-{windows}.msi"));
            }
        }
        _ => {}
    }
    wanted
        .iter()
        .find_map(|name| assets.iter().find(|a| &a.name == name).cloned())
}

/// The macOS build names of a Rust architecture.
fn macos_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "aarch64" | "arm64" => Some("aarch64"),
        "x86_64" | "amd64" | "x64" => Some("x86_64"),
        _ => None,
    }
}

/// The Windows installer names of a Rust architecture.
fn windows_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" | "amd64" | "x64" => Some("x64"),
        "aarch64" | "arm64" => Some("arm64"),
        "x86" | "i686" | "i586" => Some("x86"),
        _ => None,
    }
}

/// Parses a release tag: `v1.2.3`, `1.2.3`, `1.2` or `1`.
fn parse_version(text: &str) -> Result<Version, String> {
    let raw = text.trim();
    let raw = raw
        .strip_prefix('v')
        .or_else(|| raw.strip_prefix('V'))
        .unwrap_or(raw);
    if let Ok(version) = Version::parse(raw) {
        return Ok(version);
    }
    let parts: Vec<&str> = raw.split('.').collect();
    if (1..=2).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        let padded = if parts.len() == 1 {
            format!("{}.0.0", parts[0])
        } else {
            format!("{}.{}.0", parts[0], parts[1])
        };
        if let Ok(version) = Version::parse(&padded) {
            return Ok(version);
        }
    }
    Err(format!("invalid version: {text}"))
}

/// The shared HTTP client: no redirects, a hard deadline.
fn client() -> Result<reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(REQUEST_TIMEOUT)
                .user_agent(concat!("PaperCraft/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|e| e.to_string())
        })
        .clone()
}

/// Reads a response body, refusing anything above `cap`.
async fn read_capped(mut response: reqwest::Response, cap: usize) -> Result<Vec<u8>, String> {
    if let Some(len) = response.content_length()
        && len > cap as u64
    {
        return Err(format!("the release feed is too large ({len} bytes)"));
    }
    let mut buf = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("could not read the response: {e}"))?
    {
        if buf.len() + chunk.len() > cap {
            return Err("the release feed is too large".into());
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}
