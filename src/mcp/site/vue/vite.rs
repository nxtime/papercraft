//! `render: spa`: a Vite 8 app with vue-router 5; @unhead/vue sets the head in the browser.

use super::pages::{index_sfc, page_path, page_sfc};
use super::project::package_json;
use super::{Cx, js};
use crate::mcp::site::model::{Output, Styling};

const TSCONFIG: &str = "{\n  \"files\": [],\n  \"references\": [\n    { \"path\": \"./tsconfig.node.json\" },\n    { \"path\": \"./tsconfig.app.json\" }\n  ]\n}\n";
const TSCONFIG_APP: &str = "{\n  \"extends\": \"@vue/tsconfig/tsconfig.dom.json\",\n  \"include\": [\"env.d.ts\", \"src/**/*\", \"src/**/*.vue\"],\n  \"compilerOptions\": {\n    \"noUncheckedIndexedAccess\": true,\n    \"tsBuildInfoFile\": \"./node_modules/.tmp/tsconfig.app.tsbuildinfo\"\n  }\n}\n";
const TSCONFIG_NODE: &str = "{\n  \"extends\": \"@tsconfig/node24/tsconfig.json\",\n  \"include\": [\"vite.config.*\"],\n  \"compilerOptions\": {\n    \"module\": \"preserve\",\n    \"moduleResolution\": \"bundler\",\n    \"allowImportingTsExtensions\": true,\n    \"types\": [\"node\"],\n    \"noEmit\": true,\n    \"tsBuildInfoFile\": \"./node_modules/.tmp/tsconfig.node.tsbuildinfo\"\n  }\n}\n";

pub(super) fn write(cx: &Cx, out: &mut Output) -> Result<(), String> {
    let site = cx.site;
    let styling = site.options.styling;
    let mut dev = vec![
        ("@tsconfig/node24", "^24.0.5"),
        ("@types/node", "^24.13.4"),
        ("@vitejs/plugin-vue", "^6.0.8"),
        ("@vue/tsconfig", "^0.9.1"),
        ("typescript", "~6.0.0"),
        ("vite", "^8.2.2"),
        ("vue-tsc", "^3.3.11"),
    ];
    match styling {
        Styling::Scss => dev.push(("sass", "^1.107.1")),
        Styling::Tailwind => {
            dev.extend([("@tailwindcss/vite", "^4.3.4"), ("tailwindcss", "^4.3.4")])
        }
        _ => {}
    }
    let scripts = [
        ("build", "vue-tsc --build && vite build"),
        ("dev", "vite"),
        ("preview", "vite preview"),
    ];
    let deps = [
        ("@unhead/vue", "^3.4.3"),
        ("vue", "^3.5.42"),
        ("vue-router", "^5.3.1"),
    ];
    let name = crate::mcp::site::style::class_slug(&site.site_title);
    out.files.insert(
        "package.json".into(),
        package_json(&name, &scripts, &deps, &dev, true)?,
    );
    out.files
        .insert("tsconfig.json".into(), TSCONFIG.as_bytes().to_vec());
    out.files
        .insert("tsconfig.app.json".into(), TSCONFIG_APP.as_bytes().to_vec());
    out.files.insert(
        "tsconfig.node.json".into(),
        TSCONFIG_NODE.as_bytes().to_vec(),
    );
    out.files.insert(
        "env.d.ts".into(),
        b"/// <reference types=\"vite/client\" />\n".to_vec(),
    );
    out.files.insert(
        ".gitignore".into(),
        b"node_modules\ndist\n*.tsbuildinfo\n".to_vec(),
    );
    out.files.insert(
        "vite.config.ts".into(),
        vite_config(styling == Styling::Tailwind).into_bytes(),
    );
    out.files
        .insert("index.html".into(), index_html(cx).into_bytes());
    let css = if styling == Styling::Scss {
        "main.scss"
    } else {
        "main.css"
    };
    out.files.insert("src/main.ts".into(), format!("import {}\n\nimport {{ createApp }} from 'vue'\nimport {{ createHead }} from '@unhead/vue/client'\nimport App from './App.vue'\nimport router from './router'\n\ncreateApp(App).use(createHead()).use(router).mount('#app')\n", js(&format!("./assets/{css}"))).into_bytes());
    out.files.insert(
        "src/App.vue".into(),
        b"<template>\n  <RouterView />\n</template>\n".to_vec(),
    );
    out.files
        .insert("src/router.ts".into(), router(cx).into_bytes());
    for page in 0..site.pages.len() {
        out.files
            .insert(page_path(cx, page), page_sfc(cx, page).into_bytes());
    }
    if cx.multi {
        out.files.insert(
            "src/views/LanguageIndex.vue".into(),
            index_sfc(cx).into_bytes(),
        );
    }
    out.next_steps.push("In the output folder run `npm install`, then `npm run build`: the app is written to dist/. `npm run dev` serves it while you edit. Host dist/ with a fallback that serves index.html for unknown paths, or deep links return 404.".into());
    out.warnings.push("render: spa sets the title, description, canonical and hreflang tags in the browser: crawlers that do not run JavaScript only see the fallback title. Use render: static for search engines.".into());
    Ok(())
}

fn vite_config(tailwind: bool) -> String {
    if tailwind {
        "import { defineConfig } from 'vite'\nimport vue from '@vitejs/plugin-vue'\nimport tailwindcss from '@tailwindcss/vite'\n\nexport default defineConfig({ plugins: [vue(), tailwindcss()] })\n".into()
    } else {
        "import { defineConfig } from 'vite'\nimport vue from '@vitejs/plugin-vue'\n\nexport default defineConfig({ plugins: [vue()] })\n".into()
    }
}

/// The static shell: the title and language a visitor sees before the app starts.
fn index_html(cx: &Cx) -> String {
    use crate::mcp::site::markup::{Dialect, escape_attr, escape_text};
    let site = cx.site;
    let lang = site
        .locales
        .first()
        .and_then(|l| l.tag.as_deref())
        .map(|t| format!(" lang=\"{}\"", escape_attr(Dialect::Html, t)))
        .unwrap_or_default();
    format!(
        "<!DOCTYPE html>\n<html{lang}>\n  <head>\n    <meta charset=\"UTF-8\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n    <title>{}</title>\n  </head>\n  <body>\n    <div id=\"app\"></div>\n    <script type=\"module\" src=\"/src/main.ts\"></script>\n  </body>\n</html>\n",
        escape_text(Dialect::Html, &site.site_title)
    )
}

fn router(cx: &Cx) -> String {
    let site = cx.site;
    let lazy = |page: usize| {
        format!(
            "() => import({})",
            js(&format!(
                ".{}",
                page_path(cx, page).trim_start_matches("src")
            ))
        )
    };
    let mut out = String::from("import { createRouter, createWebHistory } from 'vue-router'\n");
    let routes = if cx.multi {
        out.push_str("import { isLocale } from './i18n'\nimport LanguageIndex from './views/LanguageIndex.vue'\n");
        let children: Vec<String> = (0..site.pages.len())
            .map(|p| {
                format!(
                    "        {{ path: {}, component: {} }},\n",
                    js(if p == 0 {
                        ""
                    } else {
                        site.pages[p].stem.as_str()
                    }),
                    lazy(p)
                )
            })
            .collect();
        format!(
            "    {{ path: '/', component: LanguageIndex }},\n    {{\n      path: '/:lang',\n      // An unknown language goes to the language index.\n      beforeEnter: (to) => isLocale(to.params.lang) || '/',\n      children: [\n{}      ],\n    }},\n",
            children.concat()
        )
    } else {
        (0..site.pages.len())
            .map(|p| {
                format!(
                    "    {{ path: {}, component: {} }},\n",
                    js(&format!(
                        "/{}",
                        if p == 0 {
                            ""
                        } else {
                            site.pages[p].stem.as_str()
                        }
                    )),
                    lazy(p)
                )
            })
            .collect()
    };
    out.push_str(&format!("\nexport default createRouter({{\n  history: createWebHistory(import.meta.env.BASE_URL),\n  routes: [\n{routes}    {{ path: '/:pathMatch(.*)*', redirect: '/' }},\n  ],\n}})\n"));
    out
}
