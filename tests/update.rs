//! Updater backend: version ordering, installer selection and URL hardening.
//!
//! Everything here is a fixture: no network, no downloads.

use papercraft::update::{Asset, parse_release, validate_asset};

const HOST: &str = "https://git.marcospaulo.dev.br";
const DOWNLOAD: &str = "https://git.marcospaulo.dev.br/masi/papercraft/releases/download";

/// A release feed with the given assets.
fn feed(version: &str, assets: &[(&str, &str)]) -> String {
    let mut list = String::new();
    for (name, url) in assets {
        if !list.is_empty() {
            list.push(',');
        }
        list.push_str(&format!(
            r#"{{"name":"{name}","browser_download_url":"{url}","size":1024}}"#
        ));
    }
    format!(
        r#"{{"tag_name":"{version}","body":"notes","draft":false,"prerelease":false,"html_url":"{HOST}/masi/papercraft/releases/tag/{version}","assets":[{list}]}}"#
    )
}

/// The download URL of an asset of a release.
fn url(version: &str, name: &str) -> String {
    format!("{DOWNLOAD}/v{version}/{name}")
}

/// The macOS installer of a release.
fn macos_asset(version: &str, arch: &str) -> (String, String) {
    let name = format!("papercraft-{version}-macos-{arch}.dmg");
    let asset_url = url(version, &name);
    (name, asset_url)
}

/// The Windows installer of a release.
fn windows_asset(version: &str, arch: &str) -> (String, String) {
    let name = format!("papercraft-{version}-windows-{arch}.msi");
    let asset_url = url(version, &name);
    (name, asset_url)
}

#[test]
fn newer_release_is_offered() {
    let json = feed("v0.4.0", &[]);
    let release = parse_release(&json, "0.3.1", "macos", "aarch64")
        .expect("feed parses")
        .expect("a newer release exists");
    assert_eq!(release.version, "0.4.0");
    assert_eq!(release.notes, "notes");
    assert_eq!(
        release.url,
        format!("{HOST}/masi/papercraft/releases/tag/v0.4.0")
    );
}

#[test]
fn same_version_is_up_to_date() {
    let json = feed("v0.3.1", &[]);
    assert!(
        parse_release(&json, "0.3.1", "macos", "aarch64")
            .expect("feed parses")
            .is_none()
    );
}

#[test]
fn older_release_is_up_to_date() {
    let json = feed("v0.3.0", &[]);
    assert!(
        parse_release(&json, "0.3.1", "macos", "aarch64")
            .expect("feed parses")
            .is_none()
    );
}

#[test]
fn versions_compare_numerically_not_alphabetically() {
    let json = feed("v0.10.0", &[]);
    assert!(
        parse_release(&json, "0.9.0", "macos", "aarch64")
            .expect("feed parses")
            .is_some(),
        "0.10.0 is newer than 0.9.0"
    );
    let json = feed("v0.2.0", &[]);
    assert!(
        parse_release(&json, "0.10.0", "macos", "aarch64")
            .expect("feed parses")
            .is_none(),
        "0.2.0 is older than 0.10.0"
    );
}

#[test]
fn drafts_and_prereleases_are_refused() {
    let draft = feed("v0.4.0", &[]).replace("\"draft\":false", "\"draft\":true");
    assert!(parse_release(&draft, "0.3.1", "macos", "aarch64").is_err());

    let pre = feed("v0.4.0", &[]).replace("\"prerelease\":false", "\"prerelease\":true");
    assert!(parse_release(&pre, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn malformed_versions_are_refused() {
    let json = feed("not-a-version", &[]);
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
    assert!(parse_release(&json, "also-not-a-version", "macos", "aarch64").is_err());
    assert!(parse_release("{not json", "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn macos_picks_the_native_build() {
    let (native, native_url) = macos_asset("0.4.0", "aarch64");
    let (universal, universal_url) = macos_asset("0.4.0", "universal");
    let json = feed(
        "v0.4.0",
        &[
            (native.as_str(), native_url.as_str()),
            (universal.as_str(), universal_url.as_str()),
        ],
    );
    let release = parse_release(&json, "0.3.1", "macos", "aarch64")
        .expect("feed parses")
        .expect("a newer release exists");
    let asset = release.asset.expect("a macOS installer");
    assert_eq!(asset.name, native);
    assert_eq!(asset.url, native_url);
    assert_eq!(asset.size, 1024);
}

#[test]
fn macos_falls_back_to_the_universal_build() {
    let (universal, universal_url) = macos_asset("0.4.0", "universal");
    let (other, other_url) = macos_asset("0.4.0", "x86_64");
    let json = feed(
        "v0.4.0",
        &[
            (other.as_str(), other_url.as_str()),
            (universal.as_str(), universal_url.as_str()),
        ],
    );
    let release = parse_release(&json, "0.3.1", "macos", "aarch64")
        .expect("feed parses")
        .expect("a newer release exists");
    let asset = release.asset.expect("a macOS installer");
    assert_eq!(asset.name, universal);
}

#[test]
fn macos_without_an_installer_reports_no_asset() {
    let json = feed("v0.4.0", &[]);
    let release = parse_release(&json, "0.3.1", "macos", "aarch64")
        .expect("feed parses")
        .expect("a newer release exists");
    assert!(release.asset.is_none(), "no installer for this machine");
}

#[test]
fn windows_maps_every_architecture() {
    let (x64, x64_url) = windows_asset("0.4.0", "x64");
    let (arm64, arm64_url) = windows_asset("0.4.0", "arm64");
    let (x86, x86_url) = windows_asset("0.4.0", "x86");
    let json = feed(
        "v0.4.0",
        &[
            (x64.as_str(), x64_url.as_str()),
            (arm64.as_str(), arm64_url.as_str()),
            (x86.as_str(), x86_url.as_str()),
        ],
    );
    for (arch, expected) in [("x86_64", &x64), ("aarch64", &arm64), ("x86", &x86)] {
        let release = parse_release(&json, "0.3.1", "windows", arch)
            .expect("feed parses")
            .expect("a newer release exists");
        let asset = release.asset.expect("a Windows installer");
        assert_eq!(&asset.name, expected, "windows/{arch}");
    }
}

#[test]
fn linux_gets_the_release_page_but_no_installer() {
    let json = feed("v0.4.0", &[]);
    let release = parse_release(&json, "0.3.1", "linux", "x86_64")
        .expect("feed parses")
        .expect("a newer release exists");
    assert!(release.asset.is_none(), "no Linux installer is published");
    assert!(release.url.starts_with(HOST));
}

#[test]
fn cli_and_web_archives_are_never_selected() {
    let cli = "papercraft-cli-0.4.0-macos-aarch64.zip";
    let web = "papercraft-web-0.4.0.zip";
    let json = feed(
        "v0.4.0",
        &[(cli, &url("0.4.0", cli)), (web, &url("0.4.0", web))],
    );
    let release = parse_release(&json, "0.3.1", "macos", "aarch64")
        .expect("feed parses")
        .expect("a newer release exists");
    assert!(release.asset.is_none(), "only archives were published");
}

#[test]
fn portable_archives_are_never_selected() {
    let name = "papercraft-0.4.0-windows-x64-portable.zip";
    let json = feed("v0.4.0", &[(name, &url("0.4.0", name))]);
    let release = parse_release(&json, "0.3.1", "windows", "x86_64")
        .expect("feed parses")
        .expect("a newer release exists");
    assert!(release.asset.is_none());
}

#[test]
fn foreign_hosts_are_refused() {
    let name = "papercraft-0.4.0-macos-aarch64.dmg";
    let evil = format!("https://evil.example.com/masi/papercraft/releases/download/v0.4.0/{name}");
    let json = feed("v0.4.0", &[(name, &evil)]);
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn plain_http_is_refused() {
    let name = "papercraft-0.4.0-macos-aarch64.dmg";
    let plain =
        format!("http://git.marcospaulo.dev.br/masi/papercraft/releases/download/v0.4.0/{name}");
    let json = feed("v0.4.0", &[(name, &plain)]);
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn credentials_and_ports_are_refused() {
    let name = "papercraft-0.4.0-macos-aarch64.dmg";
    let creds = format!(
        "https://user:pass@git.marcospaulo.dev.br/masi/papercraft/releases/download/v0.4.0/{name}"
    );
    let json = feed("v0.4.0", &[(name, &creds)]);
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());

    let port = format!(
        "https://git.marcospaulo.dev.br:8443/masi/papercraft/releases/download/v0.4.0/{name}"
    );
    let json = feed("v0.4.0", &[(name, &port)]);
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn fragments_are_refused() {
    let name = "papercraft-0.4.0-macos-aarch64.dmg";
    let fragment = format!("{DOWNLOAD}/v0.4.0/{name}#evil");
    let json = feed("v0.4.0", &[(name, &fragment)]);
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn release_pages_of_other_repositories_are_refused() {
    let json = feed("v0.4.0", &[]).replace(
        &format!("{HOST}/masi/papercraft/releases/tag/v0.4.0"),
        "https://git.marcospaulo.dev.br/other/repo/releases/tag/v0.4.0",
    );
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn traversing_asset_names_are_refused() {
    let name = "../../etc/passwd";
    let asset_url = format!("{DOWNLOAD}/v0.4.0/{name}");
    let json = feed("v0.4.0", &[(name, &asset_url)]);
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn asset_names_must_match_their_url() {
    let name = "papercraft-0.4.0-macos-aarch64.dmg";
    let other = "papercraft-0.4.0-macos-x86_64.dmg";
    let json = feed("v0.4.0", &[(name, &url("0.4.0", other))]);
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn validate_asset_accepts_an_official_installer() {
    let asset = Asset {
        name: "papercraft-0.4.0-macos-aarch64.dmg".into(),
        url: url("0.4.0", "papercraft-0.4.0-macos-aarch64.dmg"),
        size: 1024,
    };
    assert!(validate_asset(&asset).is_ok());
}

#[test]
fn validate_asset_refuses_archives_and_foreign_urls() {
    let archive = Asset {
        name: "papercraft-cli-0.4.0-macos-aarch64.zip".into(),
        url: url("0.4.0", "papercraft-cli-0.4.0-macos-aarch64.zip"),
        size: 1024,
    };
    assert!(validate_asset(&archive).is_err());

    let foreign = Asset {
        name: "papercraft-0.4.0-macos-aarch64.dmg".into(),
        url: "https://evil.example.com/papercraft-0.4.0-macos-aarch64.dmg".into(),
        size: 1024,
    };
    assert!(validate_asset(&foreign).is_err());

    let traversal = Asset {
        name: "../papercraft-0.4.0-macos-aarch64.dmg".into(),
        url: url("0.4.0", "../papercraft-0.4.0-macos-aarch64.dmg"),
        size: 1024,
    };
    assert!(validate_asset(&traversal).is_err());

    let huge = Asset {
        name: "papercraft-0.4.0-macos-aarch64.dmg".into(),
        url: url("0.4.0", "papercraft-0.4.0-macos-aarch64.dmg"),
        size: 512 * 1024 * 1024,
    };
    assert!(validate_asset(&huge).is_err());
}
