//! `render: static`: a Nuxt 4 project that `nuxt generate` prerenders, one HTML file per route.

use super::pages::{index_sfc, page_path, page_sfc};
use super::project::package_json;
use super::{Cx, js};
use crate::mcp::site::model::{Output, Styling};

const TSCONFIG: &str = "{\n  \"files\": [],\n  \"references\": [\n    { \"path\": \"./.nuxt/tsconfig.app.json\" },\n    { \"path\": \"./.nuxt/tsconfig.server.json\" },\n    { \"path\": \"./.nuxt/tsconfig.shared.json\" },\n    { \"path\": \"./.nuxt/tsconfig.node.json\" }\n  ]\n}\n";

pub(super) fn write(cx: &Cx, out: &mut Output) -> Result<(), String> {
    let site = cx.site;
    let styling = site.options.styling;
    let mut dev: Vec<(&str, &str)> = Vec::new();
    match styling {
        Styling::Scss => dev.push(("sass", "^1.107.1")),
        Styling::Tailwind => {
            dev.extend([("@tailwindcss/vite", "^4.3.4"), ("tailwindcss", "^4.3.4")])
        }
        _ => {}
    }
    let scripts = [
        ("build", "nuxt build"),
        ("dev", "nuxt dev"),
        ("generate", "nuxt generate"),
        ("postinstall", "nuxt prepare"),
        ("preview", "nuxt preview"),
    ];
    let deps = [
        ("nuxt", "^4.6.4"),
        ("vue", "^3.5.44"),
        ("vue-router", "^5.4.0"),
    ];
    let name = crate::mcp::site::style::class_slug(&site.site_title);
    out.files.insert(
        "package.json".into(),
        package_json(&name, &scripts, &deps, &dev, false)?,
    );
    out.files
        .insert("tsconfig.json".into(), TSCONFIG.as_bytes().to_vec());
    out.files.insert(
        ".gitignore".into(),
        b"node_modules\n.nuxt\n.output\ndist\n".to_vec(),
    );
    out.files
        .insert("nuxt.config.ts".into(), config(cx).into_bytes());
    out.files.insert(
        "app/app.vue".into(),
        b"<template>\n  <NuxtPage />\n</template>\n".to_vec(),
    );
    for page in 0..site.pages.len() {
        out.files
            .insert(page_path(cx, page), page_sfc(cx, page).into_bytes());
    }
    if cx.multi {
        out.files
            .insert("app/pages/index.vue".into(), index_sfc(cx).into_bytes());
    }
    out.next_steps.push("In the output folder run `npm install`, then `npm run generate`: it prerenders every page and language into .output/public, ready for any static host. `npm run dev` serves it while you edit.".into());
    Ok(())
}

fn config(cx: &Cx) -> String {
    let site = cx.site;
    let mut routes: Vec<String> = Vec::new();
    if cx.multi {
        routes.push("/".into());
    }
    for loc in 0..site.locales.len() {
        routes.extend((0..site.pages.len()).map(|p| cx.route(p, loc)));
    }
    let (css, import, plugin) = match site.options.styling {
        Styling::Scss => ("main.scss", "", ""),
        Styling::Tailwind => (
            "main.css",
            "import tailwindcss from '@tailwindcss/vite'\n\n",
            "  vite: { plugins: [tailwindcss()] },\n",
        ),
        _ => ("main.css", "", ""),
    };
    format!(
        "{import}export default defineNuxtConfig({{\n  compatibilityDate: '2025-07-15',\n  devtools: {{ enabled: false }},\n  css: [{}],\n  // The pages have no event handler (prototype overlays, back and close links are dropped)\n  // and every link is a plain anchor, so no script is shipped. The app manifest serves only\n  // client navigation, and payload files only hydration.\n  features: {{ noScripts: true }},\n  experimental: {{ payloadExtraction: false, appManifest: false }},\n  nitro: {{ prerender: {{ failOnError: true, routes: {} }} }},\n{plugin}}})\n",
        js(&format!("~/assets/css/{css}")),
        js(&routes)
    )
}
