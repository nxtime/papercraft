//! Application updater backend.
//!
//! Talks to the GitHub release API, picks the package that matches this machine and
//! this install, downloads and verifies it. Files are only replaced, and installers
//! only run, by [`install`] and [`relaunch`], once the person has agreed to restart.

use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use semver::Version;
use serde::Deserialize;

/// Latest-release API of the official repository.
pub const RELEASES_URL: &str =
    "https://api.github.com/repos/nxtime/papercraft/releases/latest";

/// A downloadable release artifact.
#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// SHA-256 of the file, as hex.
    pub digest: String,
}

/// A published release plus the packages that match this machine, if any.
#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    /// The installer: for machine-wide installs, and every install on macOS.
    pub asset: Option<Asset>,
    /// Windows only: the bundle a copy outside Program Files replaces itself from.
    pub portable: Option<Asset>,
}

impl Release {
    /// What this copy of the app updates from. An installer would leave a portable copy
    /// behind and add a second one, so a portable copy never falls back to it.
    pub fn package(&self) -> Option<&Asset> {
        let exe = std::env::current_exe().ok();
        if cfg!(windows) && exe.is_some_and(|exe| !in_program_files(&exe, &program_files())) {
            self.portable.as_ref()
        } else {
            self.asset.as_ref()
        }
    }
}

/// The only host allowed to serve release metadata and installers.
const OFFICIAL_HOST: &str = "github.com";
/// The API host that serves release metadata.
const API_HOST: &str = "api.github.com";
/// Where GitHub redirects release downloads.
const ASSET_HOST_SUFFIX: &str = ".githubusercontent.com";
/// Repository path of the official project.
const REPO_PREFIX: &str = "/nxtime/papercraft/releases/";
/// Repository path of uploaded release files.
const DOWNLOAD_PREFIX: &str = "/nxtime/papercraft/releases/download/";
/// Metadata is a small JSON document; anything bigger is refused.
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
/// Installers are desktop packages, not disk images of the internet.
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
/// Deadline for every request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Deadline for a whole download, which on a slow line takes far longer than metadata.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Redirects one download may follow.
const MAX_REDIRECTS: usize = 5;
/// The programs of a portable bundle, in the folder the app runs from.
const BUNDLE: [&str; 4] = ["papercraft", "papercraft-cli", "papercraft-mcp", "papercraft-relay"];
/// Seconds for the app to let go of its port before the next one starts.
const RELAUNCH_DELAY_SECS: u32 = 2;

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
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
            digest: parse_digest(raw_asset.digest.as_deref()),
        };
        check_asset_safe(&asset)?;
        assets.push(asset);
    }

    let version = latest.to_string();
    let asset = select_asset(&assets, &version, os, arch);
    let portable = select_portable(&assets, &version, os, arch);
    Ok(Some(Release {
        version,
        notes: raw.body.unwrap_or_default(),
        url,
        asset,
        portable,
    }))
}

/// Checks the official feed for a newer release.
///
/// The request is bounded: no redirects, a 30 s deadline and a metadata cap.
pub async fn check() -> Result<Option<Release>, String> {
    let client = client()?;
    let response = client
        .get(RELEASES_URL)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
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
    if file_digest(&dest).is_ok_and(|digest| digest == asset.digest) {
        return Ok(dest);
    }

    let client = client()?;
    let mut response = client
        .get(&asset.url)
        .timeout(DOWNLOAD_TIMEOUT)
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
        if file_digest(&temp)? != asset.digest {
            return Err("the download does not match the release's checksum".to_string());
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

/// Makes a downloaded package ready for [`install`]: a bundle is unpacked into a folder,
/// an installer stays as it is.
pub fn unpack(package: &Path) -> Result<PathBuf, String> {
    if !package.extension().is_some_and(|e| e == "zip") {
        return Ok(package.to_path_buf());
    }
    let dir = package.with_extension("");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("could not clear {}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    // By full path: another `tar` on PATH, like Git's, cannot read zip files.
    let tar = std::env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from("tar"), |root| PathBuf::from(root).join("System32").join("tar.exe"));
    let mut command = std::process::Command::new(tar);
    crate::chat::path::no_window(&mut command);
    let status = command
        .arg("-xf")
        .arg(package)
        .arg("-C")
        .arg(&dir)
        .status()
        .map_err(|e| format!("could not unpack the update: {e}"))?;
    if !status.success() {
        return Err(format!("could not unpack the update: tar failed ({status})"));
    }
    Ok(dir)
}

/// Replaces the programs of a portable copy with an unpacked bundle.
///
/// New files are copied in beside the old ones first, so a read-only folder or a full disk
/// fails before anything changes. A running program can be renamed but not overwritten, so
/// the old ones step aside and [`remove_replaced`] deletes them on a later launch.
pub fn install(bundle: &Path, dir: &Path) -> Result<(), String> {
    let exe = std::env::consts::EXE_SUFFIX;
    let names: Vec<String> = BUNDLE
        .iter()
        .map(|name| format!("{name}{exe}"))
        .filter(|name| bundle.join(name).is_file())
        .collect();
    if !names.iter().any(|name| *name == format!("{}{exe}", BUNDLE[0])) {
        return Err("the update does not contain the app".into());
    }
    let tag = format!("{:016x}", fastrand::u64(..));
    let incoming = |name: &str| dir.join(format!("{name}.{tag}.new"));
    let outgoing = |name: &str| dir.join(format!("{name}.{tag}.old"));
    let clean_up = || {
        for name in &names {
            remove_if_there(&incoming(name));
        }
    };
    for name in &names {
        if let Err(e) = std::fs::copy(bundle.join(name), incoming(name)) {
            clean_up();
            return Err(format!("could not write to {}: {e}", dir.display()));
        }
    }
    let mut done: Vec<&String> = Vec::new();
    for name in &names {
        let target = dir.join(name);
        let swapped = (!target.exists() || std::fs::rename(&target, outgoing(name)).is_ok())
            && std::fs::rename(incoming(name), &target).is_ok();
        if !swapped {
            // Back to the old set: half of one version and half of another may not start.
            for name in done.iter().copied().chain([name]) {
                if outgoing(name).exists() {
                    remove_if_there(&dir.join(name));
                    if let Err(e) = std::fs::rename(outgoing(name), dir.join(name)) {
                        log::warn!("could not restore {name}: {e}");
                    }
                }
            }
            clean_up();
            return Err(format!("could not replace {name}, so the update was undone"));
        }
        done.push(name);
    }
    Ok(())
}

/// Deletes what [`install`] moved aside. A program still running stays until a later launch.
pub fn remove_replaced(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("papercraft") && name.ends_with(".old") {
            remove_if_there(&entry.path());
        }
    }
}

fn remove_if_there(path: &Path) {
    if let Err(e) = std::fs::remove_file(path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        log::debug!("could not remove {}: {e}", path.display());
    }
}

/// Starts the app again once this process has gone, after running `installer` if there is one.
pub fn relaunch(installer: Option<&Path>) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("could not find the app: {e}"))?;
    run_detached(&relaunch_script(installer, &exe))
}

#[cfg(windows)]
fn run_detached(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new("cmd");
    // `cmd` has its own quoting, which the standard escaping would break.
    command.raw_arg(script);
    crate::chat::path::no_window(&mut command);
    command.spawn().map(|_| ()).map_err(|e| format!("could not restart: {e}"))
}

#[cfg(not(windows))]
fn run_detached(_script: &str) -> Result<(), String> {
    Err("restarting into an update is only supported on Windows".into())
}

/// The `cmd` line of [`relaunch`]. An installer that is cancelled still brings the app back.
fn relaunch_script(installer: Option<&Path>, exe: &Path) -> String {
    let wait = format!("ping -n {} 127.0.0.1 >nul", RELAUNCH_DELAY_SECS + 1);
    let install = installer.map_or_else(String::new, |msi| {
        format!("start \"\" /wait msiexec /i \"{}\" /passive /norestart & ", msi.display())
    });
    format!("/c {wait} & {install}start \"\" \"{}\"", exe.display())
}

/// Whether `exe` sits under one of `roots`, which machine-wide installs do. Paths on Windows
/// ignore case, and the environment and the file system can disagree on it.
fn in_program_files(exe: &Path, roots: &[PathBuf]) -> bool {
    let exe = exe.to_string_lossy().to_lowercase();
    roots.iter().any(|root| {
        let root = root.to_string_lossy().to_lowercase();
        !root.is_empty() && Path::new(&exe).starts_with(Path::new(&root))
    })
}

fn program_files() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect()
}

/// `sha256:<hex>` as the API reports it, down to the hex. Empty when there is none, which
/// [`validate_asset`] refuses.
fn parse_digest(raw: Option<&str>) -> String {
    raw.and_then(|d| d.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .map_or_else(String::new, str::to_lowercase)
}

/// The SHA-256 of a file, as hex.
fn file_digest(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Whether a redirect stays on GitHub's release hosts.
fn redirect_allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.host_str().is_some_and(|host| {
            host == OFFICIAL_HOST || host == API_HOST || host.ends_with(ASSET_HOST_SUFFIX)
        })
}

/// Checks that an asset is a PaperCraft desktop package served by the official host.
///
/// Called again on the download boundary: an asset handed in from the outside is
/// never trusted just because it was parsed from the feed.
pub fn validate_asset(asset: &Asset) -> Result<(), String> {
    check_asset_safe(asset)?;
    let name = asset.name.as_str();
    if !name.starts_with("papercraft-") {
        return Err(format!("not a PaperCraft installer: {name}"));
    }
    if name.starts_with("papercraft-cli-")
        || !(name.ends_with(".dmg") || name.ends_with(".msi") || name.ends_with("-portable.zip"))
    {
        return Err(format!("not a desktop installer: {name}"));
    }
    if asset.digest.is_empty() {
        return Err(format!("the release lists no checksum for {name}"));
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
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() < MAX_REDIRECTS && redirect_allowed(attempt.url()) {
                        attempt.follow()
                    } else {
                        attempt.stop()
                    }
                }))
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

/// The bundle a portable Windows copy updates from.
fn select_portable(assets: &[Asset], version: &str, os: &str, arch: &str) -> Option<Asset> {
    if os != "windows" {
        return None;
    }
    let name = format!("papercraft-{version}-windows-{}-portable.zip", windows_arch(arch)?);
    assets.iter().find(|a| a.name == name).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pc-up-{name}-{:016x}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn redirects_stay_on_github() {
        let ok = |u: &str| redirect_allowed(&reqwest::Url::parse(u).unwrap());
        assert!(ok("https://release-assets.githubusercontent.com/a/b?sig=1"));
        assert!(!ok("http://release-assets.githubusercontent.com/a"));
        assert!(!ok("https://githubusercontent.com.evil.example/a"));
        assert!(!ok("https://evil.example/a"));
    }

    #[test]
    fn program_files_is_matched_by_folder_and_ignoring_case() {
        let roots = [PathBuf::from("C:\\Program Files")];
        let installed = Path::new("c:\\program files\\PaperCraft\\papercraft.exe");
        let beside = Path::new("C:\\Program Files Extra\\papercraft.exe");
        assert_eq!(in_program_files(installed, &roots), cfg!(windows));
        assert!(!in_program_files(beside, &roots));
        assert!(!in_program_files(Path::new("C:\\Users\\me\\bin\\papercraft.exe"), &roots));
    }

    #[test]
    fn digests_must_be_sha256_hex() {
        let hex = "A".repeat(64);
        assert_eq!(parse_digest(Some(&format!("sha256:{hex}"))), "a".repeat(64));
        assert_eq!(parse_digest(Some("md5:abc")), "");
        assert_eq!(parse_digest(Some("sha256:xyz")), "");
        assert_eq!(parse_digest(None), "");
    }

    #[test]
    fn install_swaps_the_programs_and_leaves_other_files_alone() {
        let exe = std::env::consts::EXE_SUFFIX;
        let (bundle, dir) = (scratch("bundle"), scratch("dir"));
        std::fs::write(bundle.join(format!("papercraft{exe}")), "new").unwrap();
        std::fs::write(bundle.join(format!("papercraft-cli{exe}")), "new-cli").unwrap();
        std::fs::write(dir.join(format!("papercraft{exe}")), "old").unwrap();
        std::fs::write(dir.join("notes.txt"), "mine").unwrap();

        install(&bundle, &dir).unwrap();

        assert_eq!(std::fs::read_to_string(dir.join(format!("papercraft{exe}"))).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(dir.join(format!("papercraft-cli{exe}"))).unwrap(), "new-cli");
        remove_replaced(&dir);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        // Sorted like `left`: with ".exe" the CLI sorts first, without a suffix the app does.
        let mut want = ["notes.txt".to_string(), format!("papercraft-cli{exe}"), format!("papercraft{exe}")];
        want.sort();
        assert_eq!(left, want);
    }

    #[test]
    fn install_refuses_a_bundle_without_the_app() {
        let (bundle, dir) = (scratch("empty"), scratch("target"));
        std::fs::write(dir.join("papercraft"), "old").unwrap();
        assert!(install(&bundle, &dir).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("papercraft")).unwrap(), "old");
    }

    #[test]
    fn relaunch_waits_then_installs_then_starts_the_app() {
        let plain = relaunch_script(None, Path::new("C:\\a b\\papercraft.exe"));
        assert!(plain.ends_with("start \"\" \"C:\\a b\\papercraft.exe\""), "{plain}");
        assert!(!plain.contains("msiexec"));
        let msi = relaunch_script(Some(Path::new("C:\\c\\x.msi")), Path::new("C:\\papercraft.exe"));
        let (wait, install, start) = (msi.find("ping").unwrap(), msi.find("msiexec").unwrap(), msi.rfind("start").unwrap());
        assert!(wait < install && install < start, "{msi}");
    }
}
