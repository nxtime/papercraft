//! Cache and fallback files for the static hosts, written beside the project's other files.
//!
//! Every framework already names its built assets after their content, so those paths can be
//! cached for a year. HTML gets no rule on purpose: Netlify, Cloudflare Pages and Vercel all
//! answer `max-age=0, must-revalidate` for it by default, and an extra catch-all rule would be
//! merged with (not replace) the asset rule on Netlify and Cloudflare.

use super::model::{Framework, Options, Render};
use std::collections::BTreeMap;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// The content-hashed paths of a build, as globs from the site root. `assets/` holds the fonts
/// and images the exporter names by hash; the rest is what each toolchain emits.
fn hashed(o: &Options) -> Vec<&'static str> {
    let mut v = match (o.framework, o.render) {
        (Framework::Astro, _) => vec!["/_astro/*"],
        (Framework::Vue, Render::Static) => vec!["/_nuxt/*"],
        (Framework::React, Render::Static) => vec!["/_next/static/*"],
        // Angular hashes its root files (main-*.js, chunk-*.js, styles-*.css, polyfills-*.js).
        (Framework::Angular, _) => vec!["/*.js", "/*.css"],
        _ => vec![],
    };
    // Vite and the html writer put theirs in assets/ as well.
    v.push("/assets/*");
    v
}

/// The same globs as a Vercel `source` (path-to-regexp).
fn vercel_source(glob: &str) -> String {
    match glob.strip_prefix("/*.") {
        Some(ext) => format!(r"/(.*)\.{ext}"),
        None => glob.replace('*', "(.*)"),
    }
}

fn is_spa(o: &Options) -> bool {
    o.render == Render::Spa && o.framework != Framework::Html && o.framework != Framework::Astro
}

/// `_` folders that GitHub Pages' Jekyll would otherwise hide.
fn needs_nojekyll(o: &Options) -> bool {
    matches!(
        (o.framework, o.render),
        (Framework::Astro, _)
            | (Framework::Vue, Render::Static)
            | (Framework::React, Render::Static)
    )
}

/// Adds `_headers`, `vercel.json`, the SPA fallback and `.nojekyll` to `files`.
pub(super) fn add(o: &Options, files: &mut BTreeMap<String, Vec<u8>>) {
    // Netlify, Cloudflare and GitHub read theirs from the built folder, which the npm
    // frameworks fill from `public/`; Vercel reads `vercel.json` from the project root. The
    // html writer's output folder is both.
    let dir = if o.framework == Framework::Html {
        ""
    } else {
        "public/"
    };
    let globs = hashed(o);
    let mut headers = String::new();
    for g in &globs {
        headers.push_str(&format!("{g}\n  Cache-Control: {IMMUTABLE}\n"));
    }
    let mut put = |name: &str, text: String| {
        files.insert(format!("{dir}{name}"), text.into_bytes());
    };
    put("_headers", headers);
    let rules: Vec<_> = globs
        .iter()
        .map(|g| {
            serde_json::json!({
                "source": vercel_source(g),
                "headers": [{"key": "Cache-Control", "value": IMMUTABLE}],
            })
        })
        .collect();
    let mut vercel = serde_json::json!({"headers": rules});
    if is_spa(o) {
        // Served only when no file matches, so assets and real pages still win.
        put("_redirects", "/*  /index.html  200\n".into());
        vercel["rewrites"] = serde_json::json!([{"source": "/(.*)", "destination": "/index.html"}]);
    }
    if needs_nojekyll(o) {
        put(".nojekyll", String::new());
    }
    files.insert("vercel.json".into(), format!("{vercel:#}\n").into_bytes());
}
