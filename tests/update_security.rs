use papercraft::update::{Asset, parse_release, validate_asset};
use serde_json::json;

fn feed(url: &str, tag: &str) -> String {
    json!({"tag_name":tag,"html_url":url,"assets":[]}).to_string()
}

#[test]
fn similarly_named_repository_is_not_trusted() {
    let json = feed(
        "https://git.marcospaulo.dev.br/masi/papercraft-untrusted/releases/tag/v0.4.0",
        "v0.4.0",
    );
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[test]
fn normalized_download_paths_must_stay_in_official_repository() {
    let asset = Asset {
        name: "papercraft-0.4.0-macos-aarch64.dmg".into(),
        url: "https://git.marcospaulo.dev.br/masi/papercraft/releases/download/v0.4.0/%2e%2e/%2e%2e/%2e%2e/%2e%2e/other/papercraft-0.4.0-macos-aarch64.dmg".into(),
        size: 1024,
    };
    assert!(validate_asset(&asset).is_err());
}

#[test]
fn prerelease_tag_is_refused_even_if_feed_flag_is_missing() {
    let json = feed(
        "https://git.marcospaulo.dev.br/masi/papercraft/releases/tag/v0.4.0-beta.1",
        "v0.4.0-beta.1",
    );
    assert!(parse_release(&json, "0.3.1", "macos", "aarch64").is_err());
}

#[tokio::test]
#[ignore = "downloads an installer from the official release server"]
async fn official_release_can_be_checked_and_downloaded() {
    use papercraft::update::{RELEASES_URL, check, download};
    check().await.expect("official update check succeeds");
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(RELEASES_URL)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let json = response.text().await.unwrap();
    let release = parse_release(&json, "0.0.0", std::env::consts::OS, std::env::consts::ARCH)
        .unwrap()
        .expect("published release exists");
    let asset = release
        .asset
        .expect("native installer published for this machine");
    let path = download(&asset, |_, _| {})
        .await
        .expect("installer downloads completely");
    assert_eq!(std::fs::metadata(path).unwrap().len(), asset.size);
}
