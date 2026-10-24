//! `fetch_reference_images`: images from the web for the chat to design from. Takes image URLs
//! or page URLs (a page gives its `og:image` and its first large `<img>`s). Only public http(s)
//! addresses: the app runs on the user's machine, so loopback and private networks are refused,
//! at every redirect, and the checked address is the one connected to. Images are scaled like
//! chat attachments and saved in the chat's folder, the original kept for `zoom_image`.

use super::{Out, ToolResult};
use reqwest::Url;
use serde_json::{Value, json};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::Duration;

pub const TOOL: &str = "fetch_reference_images";
const MAX_URLS: usize = 6;
const MAX_IMAGES: usize = 6;
/// Images tried and kept per page (the og:image first).
const PAGE_TRIES: usize = 8;
const PAGE_KEEP: usize = 4;
const MAX_PAGE_BYTES: usize = 3 << 20;
const MAX_IMAGE_BYTES: usize = 10 << 20;
/// Smaller images are icons and logos, not references.
const MIN_SIDE: u32 = 300;
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 4;
/// `detail`: grey copy size, the step that counts as an edge, and the share below which an
/// image is a backdrop rather than a reference.
const DETAIL_SIDE: u32 = 96;
const DETAIL_STEP: i16 = 16;
const DETAIL_MIN: f32 = 0.03;
/// An honest agent: Wikimedia and others refuse generic or browser-faking ones.
const USER_AGENT: &str = concat!("PaperCraft/", env!("CARGO_PKG_VERSION"), " (design reference fetcher)");

/// Fetches, saves and returns the images. `chat`: the calling chat's id (its folder).
pub fn fetch(project: Option<&Path>, chat: Option<&str>, args: &Value) -> ToolResult {
    let folder = chat.filter(|c| crate::chat::valid_id(c)).unwrap_or("references");
    let dir = project.ok_or("open a project first")?.join(crate::chat::TMP_DIR).join(folder);
    let urls: Vec<&str> = args["urls"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if !(1..=MAX_URLS).contains(&urls.len()) {
        return Err(format!("pass 1 to {MAX_URLS} urls"));
    }
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let (found, skipped) = rt.block_on(collect(&urls));
    if found.is_empty() {
        return Err(format!("no usable image: {}", json!(skipped)));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut outs = Vec::new();
    let mut listed = Vec::new();
    let stamp = crate::project::format::now_ms();
    for (n, (source, mime, data)) in found.into_iter().enumerate() {
        let ext = match mime.as_str() {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            _ => "png",
        };
        let img = crate::chat::events::ImageAttachment { mime, data };
        let (name, img, full) = crate::chat::shrink_image(format!("ref-{stamp}-{n}.{ext}"), img);
        let path = dir.join(name);
        std::fs::write(&path, &img.data).map_err(|e| e.to_string())?;
        if let Some(full) = full {
            std::fs::write(crate::chat::full_image_path(&path), full).map_err(|e| e.to_string())?;
        }
        let (w, h) = image::load_from_memory(&img.data).map(|i| (i.width(), i.height())).unwrap_or_default();
        listed.push(json!({"path": path, "source": source, "width": w, "height": h}));
        outs.push(Out::Image { data: img.data, mime: img.mime });
    }
    outs.push(Out::Text(json!({"images": listed, "skipped": skipped}).to_string()));
    Ok(outs)
}

/// `(source url, mime, bytes)` of each usable image, and `{url, reason}` for what was skipped.
async fn collect(urls: &[&str]) -> (Vec<(String, String, Vec<u8>)>, Vec<Value>) {
    let (mut found, mut skipped) = (Vec::new(), Vec::new());
    let mut twins = Vec::new();
    for url in urls {
        if found.len() >= MAX_IMAGES {
            break;
        }
        let (final_url, mime, body) = match get(url, MAX_PAGE_BYTES.max(MAX_IMAGE_BYTES)).await {
            Ok(r) => r,
            Err(e) => {
                skipped.push(json!({"url": url, "reason": e}));
                continue;
            }
        };
        if mime.starts_with("image/") {
            match usable(&body) {
                Ok(size) => {
                    twins.push(twin_key(&final_url, size));
                    found.push((final_url.to_string(), mime, body));
                }
                Err(e) => skipped.push(json!({"url": url, "reason": e})),
            }
            continue;
        }
        if !mime.contains("html") {
            skipped.push(json!({"url": url, "reason": format!("not an image or a page ({mime})")}));
            continue;
        }
        let html = String::from_utf8_lossy(&body[..body.len().min(MAX_PAGE_BYTES)]).into_owned();
        let candidates = page_images(&html, &final_url);
        let before = found.len();
        for c in candidates.into_iter().take(PAGE_TRIES) {
            if found.len() >= MAX_IMAGES || found.len() - before >= PAGE_KEEP {
                break;
            }
            let Ok((u, mime, data)) = get(c.as_str(), MAX_IMAGE_BYTES).await else { continue };
            let Ok(size) = usable(&data).map_err(drop).and_then(|s| if mime.starts_with("image/") { Ok(s) } else { Err(()) }) else { continue };
            let key = twin_key(&u, size);
            if !twins.contains(&key) {
                twins.push(key);
                found.push((u.to_string(), mime, data));
            }
        }
        if found.len() == before {
            skipped.push(json!({"url": url, "reason": "the page has no large image"}));
        }
    }
    (found, skipped)
}

/// The image's size when it is readable and big enough to be a reference.
fn usable(data: &[u8]) -> Result<(u32, u32), String> {
    let img = image::load_from_memory(data).map_err(|e| format!("unreadable image: {e}"))?;
    if img.width().max(img.height()) < MIN_SIDE {
        return Err(format!("too small ({}x{})", img.width(), img.height()));
    }
    if detail(&img) < DETAIL_MIN {
        return Err("a plain backdrop (gradient or flat color), not a reference".into());
    }
    Ok((img.width(), img.height()))
}

/// Share of neighbouring pixels (on a small grey copy) that differ clearly: a UI or a product
/// shot is full of edges, a gradient or a soft backdrop has almost none.
fn detail(img: &image::DynamicImage) -> f32 {
    let g = img.thumbnail(DETAIL_SIDE, DETAIL_SIDE).to_luma8();
    let (w, h) = g.dimensions();
    let mut edges = 0u32;
    let mut pairs = 0u32;
    for y in 0..h {
        for x in 0..w {
            let v = i16::from(g.get_pixel(x, y)[0]);
            for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                if nx < w && ny < h {
                    pairs += 1;
                    edges += u32::from((v - i16::from(g.get_pixel(nx, ny)[0])).abs() > DETAIL_STEP);
                }
            }
        }
    }
    edges as f32 / pairs.max(1) as f32
}

/// The same picture in two formats or qualities (`900x506sr.webp`, `900x506sr-60.jpg`): same
/// folder, same file name without its extension and a trailing `-NN`, same size.
fn twin_key(url: &Url, size: (u32, u32)) -> (String, u32, u32) {
    let (folder, file) = url.path().rsplit_once('/').unwrap_or(("", url.path()));
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let stem = stem.rsplit_once('-').filter(|(_, q)| !q.is_empty() && q.chars().all(|c| c.is_ascii_digit())).map_or(stem, |(s, _)| s);
    (format!("{}{folder}/{stem}", url.host_str().unwrap_or_default()), size.0, size.1)
}

/// A blocking GET of one public URL (plugins): its content type and body.
pub(crate) fn download(url: &str, cap: usize) -> Result<(String, Vec<u8>), String> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    rt.block_on(get(url, cap)).map(|(_, mime, body)| (mime, body))
}

/// GET with redirects followed by hand, each hop checked by [`public_addr`] and pinned to it.
async fn get(url: &str, cap: usize) -> Result<(Url, String, Vec<u8>), String> {
    let mut url = Url::parse(url).map_err(|e| format!("bad url: {e}"))?;
    for _ in 0..=MAX_REDIRECTS {
        let addr = public_addr(&url).await?;
        let host = url.host_str().unwrap_or_default().to_owned();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .user_agent(USER_AGENT)
            .resolve(&host, addr)
            .build()
            .map_err(|e| e.to_string())?;
        let mut resp = client.get(url.clone()).send().await.map_err(|e| e.to_string())?;
        if resp.status().is_redirection() {
            let to = resp.headers().get("location").and_then(|v| v.to_str().ok()).ok_or("redirect without a location")?;
            url = url.join(to).map_err(|e| e.to_string())?;
            continue;
        }
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let mime = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default().to_ascii_lowercase();
        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            body.extend_from_slice(&chunk);
            if body.len() > cap {
                return Err(format!("larger than {} MB", cap >> 20));
            }
        }
        return Ok((url, mime, body));
    }
    Err("too many redirects".into())
}

/// The address to connect to, when the URL is http(s) and every address of its host is public.
async fn public_addr(url: &Url) -> Result<SocketAddr, String> {
    if !["http", "https"].contains(&url.scheme()) {
        return Err("only http and https".into());
    }
    let host = url.host_str().ok_or("no host")?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.trim_matches(['[', ']']), port)).await.map_err(|e| format!("{host}: {e}"))?.collect();
    if addrs.is_empty() || !addrs.iter().all(|a| is_public(a.ip())) {
        return Err(format!("{host} is not a public address"));
    }
    Ok(addrs[0])
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, ..] = v.octets();
            let shared = a == 100 && (b & 0xc0) == 64; // 100.64.0.0/10, carrier-grade NAT
            !(v.is_private() || v.is_loopback() || v.is_link_local() || v.is_broadcast() || v.is_documentation() || v.is_unspecified() || v.is_multicast() || shared || a == 0)
        }
        IpAddr::V6(v) => {
            let s0 = v.segments()[0];
            let local = (s0 & 0xfe00) == 0xfc00 || (s0 & 0xffc0) == 0xfe80; // unique local, link local
            match v.to_ipv4_mapped() {
                Some(m) => is_public(IpAddr::V4(m)),
                None => !(v.is_loopback() || v.is_unspecified() || v.is_multicast() || local),
            }
        }
    }
}

/// Image URLs a page offers, best first: `<picture>` `<source>`s and `<img>`s widest first by
/// their declared width (`srcset`'s widest entry, or `width`), then `og:image` /
/// `twitter:image` (usually a social card, not the interface). Small ones, icons and logos are
/// dropped, and an image in several formats counts once.
fn page_images(html: &str, base: &Url) -> Vec<Url> {
    let mut meta: Vec<String> = Vec::new();
    for tag in tags(html, "meta") {
        let key = attr(tag, "property").or_else(|| attr(tag, "name")).unwrap_or_default().to_ascii_lowercase();
        if ["og:image", "og:image:url", "og:image:secure_url", "twitter:image", "twitter:image:src"].contains(&key.as_str())
            && let Some(c) = attr(tag, "content")
        {
            meta.push(c);
        }
    }
    // (declared width, url); 0 = not declared.
    let mut sized: Vec<(u32, String)> = Vec::new();
    for tag in tags(html, "source").into_iter().chain(tags(html, "img")) {
        let widest = attr(tag, "srcset").and_then(|set| srcset(&set).into_iter().max_by_key(|(w, _)| *w));
        let declared = || attr(tag, "width").and_then(|w| w.parse().ok()).unwrap_or(0);
        let entry = widest.or_else(|| attr(tag, "data-src").or_else(|| attr(tag, "src")).map(|u| (declared(), u)));
        if let Some((w, u)) = entry.filter(|(w, _)| *w == 0 || *w >= MIN_SIDE) {
            sized.push((w, u));
        }
    }
    sized.sort_by_key(|(w, _)| std::cmp::Reverse(*w));
    let mut out: Vec<Url> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for r in sized.into_iter().map(|(_, u)| u).chain(meta) {
        let Ok(u) = base.join(&r.replace("&amp;", "&")) else { continue };
        let path = u.path().to_ascii_lowercase();
        if ["icon", "logo", "favicon", "avatar", "sprite"].iter().any(|w| path.contains(w)) {
            continue;
        }
        // The same image as .webp and .jpg counts once.
        let stem = format!("{}{}", u.host_str().unwrap_or_default(), u.path().rsplit_once('.').map_or(u.path(), |(s, _)| s));
        if ["http", "https"].contains(&u.scheme()) && !seen.contains(&stem) {
            seen.push(stem);
            out.push(u);
        }
    }
    out
}

/// `(width, url)` of each `srcset` entry (width 0 for an `x` or missing descriptor). URLs may
/// hold commas (`w=756,h=407`), so entries are told apart by their whitespace, as the HTML
/// spec does, not by splitting on commas.
fn srcset(set: &str) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = Vec::new();
    for token in set.split_whitespace() {
        let bare = token.trim_end_matches(',');
        let width = bare.strip_suffix('w').and_then(|n| n.parse().ok());
        let density = bare.strip_suffix('x').is_some_and(|n| n.parse::<f32>().is_ok());
        match (width, out.last_mut()) {
            (Some(w), Some(last)) => last.0 = w,
            _ if density => {}
            _ => out.push((0, bare.trim_start_matches(',').to_owned())),
        }
    }
    out
}

/// The opening tags named `name` (`<meta ...>`), case-insensitively.
fn tags<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{name}");
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find(&open).map(|i| at + i) {
        let end = lower[i..].find('>').map_or(html.len(), |e| i + e);
        if lower[i + open.len()..].starts_with(|c: char| c.is_whitespace() || c == '/') {
            out.push(&html[i..end]);
        }
        at = end.max(i + 1);
    }
    out
}

/// A quoted (or bare) attribute value of one tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find(name).map(|i| at + i) {
        at = i + name.len();
        let before_ok = lower[..i].ends_with(|c: char| c.is_whitespace());
        let rest = lower[at..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let start = tag.len() - rest.len() + 1;
        let value = tag[start..].trim_start();
        return Some(match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or_default().to_owned(),
            _ => value.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or_default().to_owned(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_addresses_are_fetched() {
        for private in ["127.0.0.1", "10.1.2.3", "192.168.0.10", "172.16.5.4", "169.254.1.1", "100.64.0.1", "0.0.0.0", "::1", "fd00::1", "fe80::1", "::ffff:127.0.0.1"] {
            assert!(!is_public(private.parse().unwrap()), "{private}");
        }
        for public in ["93.184.216.34", "2606:4700::1111", "::ffff:8.8.8.8"] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        for url in ["http://127.0.0.1:29980/mcp", "http://localhost/x.png", "file:///etc/passwd", "ftp://example.com/a.png"] {
            assert!(rt.block_on(public_addr(&Url::parse(url).unwrap())).is_err(), "{url}");
        }
    }

    #[test]
    fn a_page_offers_its_og_image_then_its_largest_images() {
        let base = Url::parse("https://dribbble.com/shots/1-chat").unwrap();
        let html = r#"<html><head><META property="og:image" content="https://cdn.example.com/shot.png?a=1&amp;b=2">
            <meta name='twitter:image' content='/tw.jpg'><meta charset="utf-8"></head>
            <body><picture><source srcset="/icon.webp 100w, /icon2.webp 200w"><source srcset="/icon.jpg 100w, /icon2.jpg 200w"></picture>
            <img src="/AppIcon-1024.png" width="1024"><img src="/brand-logo.png" width="800">
            <picture><source srcset="/p1.webp 300w, /p2.webp 1200w" type="image/webp"><source srcset="/p1.jpg 300w, /p2.jpg 1200w"></picture>
            <img src="/small.png" srcset="/s1.png 400w, /s2.png 800w"><img data-src="lazy.webp" src="data:x"><image href="no"></body>"#;
        let urls: Vec<String> = page_images(html, &base).iter().map(Url::to_string).collect();
        assert_eq!(urls, [
            "https://dribbble.com/p2.webp",
            "https://dribbble.com/s2.png",
            "https://dribbble.com/shots/lazy.webp",
            "https://cdn.example.com/shot.png?a=1&b=2",
            "https://dribbble.com/tw.jpg",
        ], "content images first, social cards last; icons dropped; the jpg twin of p2 counted once");
    }

    #[test]
    fn gradients_are_backdrops_and_interfaces_are_not() {
        let gradient = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(800, 500, |x, _| image::Rgb([(x * 255 / 800) as u8, 40, 200])));
        assert!(detail(&gradient) < DETAIL_MIN);
        // Rows of "text": dark bars on a light ground, like a list or a message thread.
        let ui = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(800, 500, |x, y| if y % 24 < 8 && x % 90 < 70 { image::Rgb([30, 30, 30]) } else { image::Rgb([245, 245, 245]) }));
        assert!(detail(&ui) > DETAIL_MIN, "{}", detail(&ui));
    }

    #[test]
    fn srcset_urls_may_hold_commas() {
        let set = "https://a.com/cdn/w=756,h=407,f=webp/x.png 756w, https://a.com/cdn/w=1512,h=814/x.png 1512w";
        assert_eq!(srcset(set), [(756, "https://a.com/cdn/w=756,h=407,f=webp/x.png".to_owned()), (1512, "https://a.com/cdn/w=1512,h=814/x.png".to_owned())]);
        assert_eq!(srcset("/a.png 1x, /b.png 2x"), [(0, "/a.png".to_owned()), (0, "/b.png".to_owned())]);
        assert_eq!(srcset("/only.png"), [(0, "/only.png".to_owned())]);
    }

    #[test]
    fn twins_are_one_picture_in_two_formats_not_a_gallery() {
        let u = |s: &str| Url::parse(s).unwrap();
        let size = (900, 506);
        assert_eq!(twin_key(&u("https://cdn.x/a/900x506sr.webp"), size), twin_key(&u("https://cdn.x/a/900x506sr-60.jpg"), size));
        assert_ne!(twin_key(&u("https://cdn.x/f/one.png"), size), twin_key(&u("https://cdn.x/f/two.png"), size), "a gallery of same-size shots");
    }

    #[test]
    fn urls_are_required_and_capped() {
        let dir = std::env::temp_dir();
        assert!(fetch(Some(&dir), None, &json!({"urls": []})).is_err());
        assert!(fetch(None, None, &json!({"urls": ["https://example.com/a.png"]})).is_err(), "needs a project");
    }
}
