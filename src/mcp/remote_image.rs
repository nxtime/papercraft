//! Fetching `http(s)` images for `write_html` (`<img src>` and CSS `background-image`).
//!
//! The URL comes from an agent, so the fetch must not become a way into the person's network:
//! only http and https, only public addresses (checked on the addresses the connection is then
//! pinned to, so DNS cannot change its answer in between), redirects re-checked hop by hop, a
//! size cap, a deadline and image content types only. Each URL is fetched once per tool call.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

const MAX_BYTES: usize = 10 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 3;
/// What the image decoder accepts (`assets::register`).
const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

thread_local! {
    static FETCHED: RefCell<HashMap<String, Result<Vec<u8>, String>>> = RefCell::new(HashMap::new());
    // Tests serve from loopback; nothing outside `cfg(test)` can turn this on.
    #[cfg(test)]
    static ALLOW_PRIVATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Forget the images fetched by the call that just ended.
pub fn end_call() {
    FETCHED.with(|f| f.borrow_mut().clear());
}

/// The bytes at `url`, fetched at most once per tool call (failures are remembered too).
pub fn fetch(url: &str) -> Result<Vec<u8>, String> {
    if let Some(done) = FETCHED.with(|f| f.borrow().get(url).cloned()) {
        return done;
    }
    #[cfg(test)]
    let allow_private = ALLOW_PRIVATE.with(|a| a.get());
    #[cfg(not(test))]
    let allow_private = false;
    let result = fetch_uncached(url, allow_private);
    FETCHED.with(|f| f.borrow_mut().insert(url.to_string(), result.clone()));
    result
}

/// Runs on its own thread and runtime: tool calls may come from inside a Tokio runtime, where
/// blocking on another would panic.
fn fetch_uncached(url: &str, allow_private: bool) -> Result<Vec<u8>, String> {
    let url = url.to_string();
    let shown = url.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        rt.block_on(async {
            tokio::time::timeout(TIMEOUT, get(&url, allow_private)).await.unwrap_or_else(|_| Err("timed out".into()))
        })
    })
    .join()
    .unwrap_or_else(|_| Err("the fetch failed unexpectedly".into()))
    .map_err(|e| format!("{shown}: {e}"))
}

async fn get(start: &str, allow_private: bool) -> Result<Vec<u8>, String> {
    let mut url = reqwest::Url::parse(start).map_err(|e| format!("bad URL: {e}"))?;
    for _ in 0..=MAX_REDIRECTS {
        if !matches!(url.scheme(), "http" | "https") {
            return Err("only http and https images can be fetched".into());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("URLs with credentials are not fetched".into());
        }
        let host = url.host_str().ok_or("the URL has no host")?.to_string();
        let port = url.port_or_known_default().ok_or("the URL has no port")?;
        let literal = host.trim_matches(['[', ']']).parse::<IpAddr>().ok();
        let addrs: Vec<SocketAddr> = match literal {
            Some(ip) => vec![SocketAddr::new(ip, port)],
            None => tokio::net::lookup_host((host.as_str(), port)).await.map_err(|e| format!("could not resolve {host}: {e}"))?.collect(),
        };
        if addrs.is_empty() || (!allow_private && addrs.iter().any(|a| !is_public(a.ip()))) {
            return Err("private, loopback and link-local addresses are not fetched".into());
        }
        let mut client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .no_proxy()
            .user_agent(concat!("PaperCraft/", env!("CARGO_PKG_VERSION")));
        if literal.is_none() {
            client = client.resolve_to_addrs(&host, &addrs);
        }
        let response = client.build().map_err(|e| e.to_string())?.get(url.clone()).send().await.map_err(|e| e.to_string())?;
        if response.status().is_redirection() {
            let next = response.headers().get(reqwest::header::LOCATION).and_then(|l| l.to_str().ok()).ok_or("a redirect without a location")?;
            url = url.join(next).map_err(|e| format!("bad redirect: {e}"))?;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }
        let kind = response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        let kind = kind.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
        if !IMAGE_TYPES.contains(&kind.as_str()) {
            return Err(format!("`{kind}` is not a PNG, JPEG, GIF or WebP image"));
        }
        return read_capped(response).await;
    }
    Err("too many redirects".into())
}

async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let too_big = || format!("larger than {} MB", MAX_BYTES / (1024 * 1024));
    if response.content_length().is_some_and(|n| n > MAX_BYTES as u64) {
        return Err(too_big());
    }
    let mut buf = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if buf.len() + chunk.len() > MAX_BYTES {
            return Err(too_big());
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => public_v4(v4),
            None => {
                let s0 = v6.segments()[0];
                !(v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || s0 & 0xfe00 == 0xfc00 || s0 & 0xffc0 == 0xfe80)
            }
        },
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    // 0/8 "this network", 100.64/10 carrier-grade NAT and 240/4 reserved have no std helper.
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && o[1] & 0xc0 == 64)
        || o[0] >= 240)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    type Route = (&'static str, &'static str, String, Vec<u8>);

    fn png() -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(3, 2).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// Serves `routes` (path, status line, extra headers, body) on loopback; returns the base
    /// URL and a request counter.
    fn serve(routes: Vec<Route>) -> (String, Arc<AtomicUsize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        std::thread::spawn(move || {
            for mut conn in listener.incoming().flatten() {
                let (mut req, mut buf) = (Vec::new(), [0u8; 1024]);
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    let Ok(n @ 1..) = conn.read(&mut buf) else { break };
                    req.extend_from_slice(&buf[..n]);
                }
                count.fetch_add(1, Ordering::SeqCst);
                let line = String::from_utf8_lossy(&req);
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, headers, body) = routes
                    .iter()
                    .find(|r| r.0 == path)
                    .map(|r| (r.1, r.2.clone(), r.3.clone()))
                    .unwrap_or(("404 Not Found", String::new(), vec![]));
                let head = format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                conn.write_all(head.as_bytes()).and_then(|()| conn.write_all(&body)).ok();
            }
        });
        (base, hits)
    }

    fn image_route(path: &'static str) -> Route {
        (path, "200 OK", "Content-Type: image/png\r\n".into(), png())
    }

    #[test]
    fn only_public_addresses_pass() {
        for ip in ["8.8.8.8", "93.184.216.34", "2606:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "127.0.0.1", "10.0.0.1", "172.16.5.5", "192.168.1.1", "169.254.169.254", "0.0.0.0", "100.64.0.1", "224.0.0.1",
            "255.255.255.255", "::1", "::", "fe80::1", "fc00::1", "fd12::1", "::ffff:127.0.0.1", "::ffff:192.168.0.1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn loopback_is_refused_without_the_test_override_and_never_contacted() {
        let (base, hits) = serve(vec![image_route("/a.png")]);
        let err = fetch(&format!("{base}/a.png")).unwrap_err();
        assert!(err.contains("private, loopback"), "{err}");
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        for bad in ["ftp://example.com/a.png", "file:///etc/passwd", "http://user:pw@example.com/a.png", "http://localhost/a.png", "http://[::1]/a.png"] {
            assert!(fetch(bad).is_err(), "{bad}");
        }
        end_call();
    }

    #[test]
    fn fetches_once_follows_redirects_and_registers_the_image() {
        ALLOW_PRIVATE.with(|a| a.set(true));
        let (base, hits) = serve(vec![image_route("/a.png"), ("/r", "302 Found", "Location: /a.png\r\n".into(), vec![])]);
        let url = format!("{base}/r");
        let first = crate::mcp::exec::image_asset(&url).unwrap();
        let second = crate::mcp::exec::image_asset(&url).unwrap();
        assert_eq!(first, second);
        assert_eq!(hits.load(Ordering::SeqCst), 2, "one redirect hop and one image, fetched once for two uses");
        end_call();
    }

    #[test]
    fn refuses_other_content_types_oversize_bodies_and_errors() {
        ALLOW_PRIVATE.with(|a| a.set(true));
        let (base, _) = serve(vec![
            ("/page", "200 OK", "Content-Type: text/html\r\n".into(), b"<html>".to_vec()),
            ("/svg", "200 OK", "Content-Type: image/svg+xml\r\n".into(), b"<svg/>".to_vec()),
            ("/big", "200 OK", "Content-Type: image/png\r\n".into(), vec![0; MAX_BYTES + 1]),
        ]);
        for path in ["/page", "/svg", "/big", "/missing"] {
            assert!(fetch(&format!("{base}{path}")).is_err(), "{path}");
        }
        end_call();
    }
}
