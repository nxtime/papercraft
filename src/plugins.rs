//! Plugins: open asset services `write_html` can place with `<x-plugin use="…" …/>`, each one
//! turned on in Settings (all off by default). Avatars, icons, photos, flags and QR codes, from
//! services that need no account. The asset is fetched once, cached on disk by its URL, and
//! placed as vector layers (SVG; masked parts as images) or as an image (photos, QR codes).
//!
//! Attributes other than the reserved ones below go to the service as query parameters, so the
//! agent can use any option a service documents (`topType="LongHairStraight"` for Avataaars).

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Svg,
    Raster,
}

pub struct Plugin {
    pub id: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub license: &'static str,
    pub kind: Kind,
    /// Width when the element gives none.
    pub default_size: f64,
    /// One line for the agent: the element and its useful attributes.
    pub hint: &'static str,
}

pub const PLUGINS: [Plugin; 6] = [
    Plugin {
        id: "avataaars",
        name: "Avataaars",
        about: "Cartoon avatars by Pablo Stanley (getavataaars.com).",
        license: "Free for personal and commercial use",
        kind: Kind::Svg,
        default_size: 64.0,
        hint: r#"<x-plugin use="avataaars" size="64" topType="LongHairStraight" hairColor="Auburn" accessoriesType="Round" facialHairType="Blank" clotheType="Hoodie" clotheColor="PastelBlue" eyeType="Happy" eyebrowType="Default" mouthType="Smile" skinColor="Light"/> (option names and values as on getavataaars.com; avatarStyle="Transparent" drops the circle)"#,
    },
    Plugin {
        id: "dicebear",
        name: "DiceBear",
        about: "Avatars in about 30 styles from a seed (dicebear.com).",
        license: "Code MIT; each style has its own licence (CC0 or CC BY 4.0), see dicebear.com/licenses",
        kind: Kind::Svg,
        default_size: 64.0,
        hint: r#"<x-plugin use="dicebear" set="adventurer" seed="Lyra Vance" size="48"/> (sets: adventurer, avataaars, big-smile, bottts, lorelei, micah, notionists, personas, pixel-art, thumbs, shapes, identicon; the same seed gives the same face)"#,
    },
    Plugin {
        id: "iconify",
        name: "Iconify",
        about: "About 200,000 open icons from 150 sets, no install (iconify.design).",
        license: "Each set's own open licence (MIT, Apache, CC BY...), shown on icon-sets.iconify.design",
        kind: Kind::Svg,
        default_size: 24.0,
        hint: r##"<x-plugin use="iconify" icon="game-icons:crossed-swords" size="24" color="#f5c451"/> (any prefix:name from icon-sets.iconify.design: mdi, ph, tabler, game-icons, twemoji...)"##,
    },
    Plugin {
        id: "picsum",
        name: "Lorem Picsum",
        about: "Placeholder photos (picsum.photos, from Unsplash).",
        license: "Unsplash licence: free to use",
        kind: Kind::Raster,
        default_size: 320.0,
        hint: r#"<x-plugin use="picsum" seed="forge" width="320" height="200"/> (a random photo that stays the same for a seed; for real art use generate_image)"#,
    },
    Plugin {
        id: "flags",
        name: "Flags",
        about: "Country flags by ISO code (flagcdn.com).",
        license: "Public domain",
        kind: Kind::Svg,
        default_size: 32.0,
        hint: r#"<x-plugin use="flags" code="br" size="32"/> (ISO 3166 alpha-2 code)"#,
    },
    Plugin {
        id: "qr",
        name: "QR codes",
        about: "QR codes for any text or link (goqr.me API).",
        license: "Free to use",
        kind: Kind::Raster,
        default_size: 120.0,
        hint: r#"<x-plugin use="qr" data="https://example.com" size="120"/>"#,
    },
];

/// Attributes that shape the element, not the asset.
const RESERVED: [&str; 10] = ["use", "size", "width", "height", "layer-name", "style", "class", "set", "icon", "code"];
/// The services' option names are camelCase, and the HTML parser lowercases attribute names:
/// these are given back their case (`toptype` -> `topType`).
const CAMEL: [&str; 21] = [
    "avatarStyle", "topType", "accessoriesType", "hairColor", "hatColor", "facialHairType", "facialHairColor",
    "clotheType", "clotheColor", "graphicType", "eyeType", "eyebrowType", "mouthType", "skinColor",
    "backgroundColor", "backgroundType", "backgroundRotation", "translateX", "translateY", "clip", "randomizeIds",
];
/// Raster assets are fetched at this many pixels per layer pixel, so they stay sharp zoomed in.
const RASTER_DENSITY: f64 = 2.0;
const MAX_BYTES: usize = 8 << 20;

pub fn plugin(id: &str) -> Option<&'static Plugin> {
    PLUGINS.iter().find(|p| p.id == id)
}

/// The service URL for an element's attributes; `width` is the layer width it will get.
pub fn url(p: &Plugin, attrs: &[(String, String)], width: f64, height: f64) -> Result<String, String> {
    let get = |k: &str| attrs.iter().find(|(a, _)| a.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str());
    let px = |v: f64| ((v * RASTER_DENSITY).round() as u32).max(1);
    let base = match p.id {
        "avataaars" => "https://avataaars.io/".to_owned(),
        "dicebear" => format!("https://api.dicebear.com/9.x/{}/svg", slug(get("set").unwrap_or("adventurer"))?),
        "iconify" => {
            let icon = get("icon").ok_or("iconify needs icon=\"prefix:name\"")?;
            let (set, name) = icon.split_once(':').ok_or("iconify icon is prefix:name, like mdi:sword")?;
            format!("https://api.iconify.design/{}/{}.svg", slug(set)?, slug(name)?)
        }
        "picsum" => format!("https://picsum.photos/seed/{}/{}/{}", slug(get("seed").unwrap_or("papercraft"))?, px(width), px(height)),
        "flags" => format!("https://flagcdn.com/{}.svg", slug(&get("code").ok_or("flags needs code=\"br\"")?.to_ascii_lowercase())?),
        "qr" => format!("https://api.qrserver.com/v1/create-qr-code/?size={0}x{0}", px(width)),
        other => return Err(format!("no plugin `{other}`")),
    };
    let mut u = reqwest::Url::parse(&base).map_err(|e| e.to_string())?;
    {
        let mut q = u.query_pairs_mut();
        if p.id == "avataaars" && get("avatarStyle").is_none() {
            q.append_pair("avatarStyle", "Circle");
        }
        for (k, v) in attrs {
            let skip = RESERVED.iter().any(|r| k.eq_ignore_ascii_case(r)) || (p.id == "picsum" && k.eq_ignore_ascii_case("seed"));
            if !skip {
                let name = CAMEL.iter().find(|c| c.eq_ignore_ascii_case(k)).copied().unwrap_or(k);
                q.append_pair(name, v);
            }
        }
    }
    let mut s = u.to_string();
    if s.ends_with('?') {
        s.pop();
    }
    Ok(s)
}

/// A path part from an attribute: letters, digits, `-`, `_`, `.` only.
fn slug(v: &str) -> Result<&str, String> {
    let ok = !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) && !v.contains("..");
    ok.then_some(v).ok_or_else(|| format!("`{v}` is not a valid name"))
}

/// The cached file for a URL, fetched first when missing.
pub fn fetch(url: &str, kind: Kind) -> Result<PathBuf, String> {
    let path = cache_path(url, kind);
    if path.is_file() {
        return Ok(path);
    }
    let (mime, bytes) = crate::mcp::refs::download(url, MAX_BYTES)?;
    let fits = match kind {
        Kind::Svg => mime.contains("svg") || bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml"),
        Kind::Raster => mime.starts_with("image/"),
    };
    if !fits {
        return Err(format!("the service answered {mime}, not an image"));
    }
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

fn cache_path(url: &str, kind: Kind) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    let ext = if kind == Kind::Svg { "svg" } else { "img" };
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("papercraft").join("plugins").join(format!("{:016x}.{ext}", h.finish()))
}

/// Layer size from `size` / `width` / `height`: width first, height from the aspect when only
/// one is given (`aspect` = height / width of the asset, 1 when unknown).
pub fn layer_size(p: &Plugin, attrs: &[(String, String)], aspect: f64) -> (f64, f64) {
    let num = |k: &str| attrs.iter().find(|(a, _)| a.eq_ignore_ascii_case(k)).and_then(|(_, v)| v.trim_end_matches("px").parse::<f64>().ok());
    match (num("width").or(num("size")), num("height")) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w * aspect),
        (None, Some(h)) => (h / aspect, h),
        (None, None) => (p.default_size, p.default_size * aspect),
    }
}

/// Fetches, in parallel, the assets the `<x-plugin>` elements of a parsed HTML tree need, so
/// placing them does not wait on the network one by one. Errors are left to the placing step.
pub fn prefetch(tree: &[crate::mcp::html::Html], enabled: &[String]) {
    let mut wanted = Vec::new();
    collect(tree, enabled, &mut wanted);
    std::thread::scope(|s| {
        for (url, kind) in &wanted {
            s.spawn(move || fetch(url, *kind));
        }
    });
}

fn collect(tree: &[crate::mcp::html::Html], enabled: &[String], out: &mut Vec<(String, Kind)>) {
    for h in tree {
        let crate::mcp::html::Html::Element(e) = h else { continue };
        if e.tag == "x-plugin"
            && let Some(p) = e.attr("use").and_then(plugin).filter(|p| enabled.iter().any(|id| id == p.id))
        {
            let (w, h) = layer_size(p, &e.attrs, 1.0);
            if let Ok(u) = url(p, &e.attrs, w, h)
                && !out.iter().any(|(x, _)| *x == u)
            {
                out.push((u, p.kind));
            }
        }
        collect(&e.children, enabled, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn urls_carry_the_options_and_refuse_odd_names() {
        // As the HTML parser hands them over: lowercase names.
        let a = url(plugin("avataaars").unwrap(), &attrs(&[("use", "avataaars"), ("size", "64"), ("toptype", "LongHairStraight")]), 64.0, 64.0).unwrap();
        assert_eq!(a, "https://avataaars.io/?avatarStyle=Circle&topType=LongHairStraight");
        let d = url(plugin("dicebear").unwrap(), &attrs(&[("set", "notionists"), ("seed", "Lyra Vance")]), 48.0, 48.0).unwrap();
        assert_eq!(d, "https://api.dicebear.com/9.x/notionists/svg?seed=Lyra+Vance");
        let i = url(plugin("iconify").unwrap(), &attrs(&[("icon", "game-icons:crossed-swords"), ("color", "#f5c451")]), 24.0, 24.0).unwrap();
        assert_eq!(i, "https://api.iconify.design/game-icons/crossed-swords.svg?color=%23f5c451");
        let p = url(plugin("picsum").unwrap(), &attrs(&[("seed", "forge")]), 320.0, 200.0).unwrap();
        assert_eq!(p, "https://picsum.photos/seed/forge/640/400", "fetched at twice the layer size");
        assert!(url(plugin("dicebear").unwrap(), &attrs(&[("set", "../x")]), 1.0, 1.0).is_err());
        assert!(url(plugin("flags").unwrap(), &attrs(&[]), 1.0, 1.0).is_err());
    }

    #[test]
    fn size_follows_the_attributes_then_the_aspect() {
        let p = plugin("avataaars").unwrap();
        assert_eq!(layer_size(p, &attrs(&[("size", "40")]), 280.0 / 264.0).0, 40.0);
        assert_eq!(layer_size(p, &attrs(&[("height", "50px")]), 0.5), (100.0, 50.0));
        assert_eq!(layer_size(p, &attrs(&[]), 1.0), (64.0, 64.0));
    }
}
