//! The vue writer: a Nuxt 4 project (`render: static`, `nuxt generate` prerenders every route)
//! or a Vite 8 single-page app (`render: spa`, vue-router and @unhead/vue). One SFC per page
//! frame; with several locales the routes are `/<locale>/...`, each locale has a JSON catalog
//! and `/` is a language index.
//!
//! User text reaches the output only through the core's Vue escaping (markup, entities) or as a
//! JSON string (`js`), never pasted into a template, a script or a config.

mod nuxt;
mod pages;
mod project;
#[cfg(test)]
mod tests;
mod vite;

use super::model::{Output, Render, Site, Styling};
use serde::Serialize;

/// Backslash, `u003c`: how a JS string says `<`.
const ESCAPED_LT: &str = "\x5cu003c";

/// Escapes a value as a JS or TS literal. `<` becomes `<` so nothing can end a `<script>`.
pub(super) fn js<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v)
        .unwrap_or_else(|_| "null".into())
        .replace('<', ESCAPED_LT)
}

/// Pretty JSON for `.ts` data files, with the same `<` rule.
pub(super) fn js_pretty<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string_pretty(v)
        .unwrap_or_else(|_| "null".into())
        .replace('<', ESCAPED_LT)
}

/// What the file writers share: the site and the target.
pub(super) struct Cx<'a> {
    pub site: &'a Site,
    pub spa: bool,
    /// Several locales: `[lang]` routes, catalogs and a language index.
    pub multi: bool,
}

impl<'a> Cx<'a> {
    fn new(site: &'a Site) -> Self {
        Cx {
            site,
            spa: site.options.render == Render::Spa,
            multi: site.locales.iter().any(|l| l.folder.is_some()),
        }
    }

    /// The folder holding the app's source: `app` for Nuxt, `src` for Vite.
    pub fn src(&self) -> &'static str {
        if self.spa { "src" } else { "app" }
    }

    /// The route of a page with the trailing slash: `/`, `/pricing/`, `/pt-BR/pricing/`.
    pub fn route(&self, page: usize, loc: usize) -> String {
        let p = self.site.url_path(page, loc);
        if p.ends_with('/') { p } else { format!("{p}/") }
    }

    /// The address a head tag or the sitemap uses: absolute with a `base_url`, else the route.
    pub fn address(&self, page: usize, loc: usize) -> String {
        format!(
            "{}{}",
            self.site.base_url.as_deref().unwrap_or_default(),
            self.route(page, loc)
        )
    }

    /// The address of a locale's root route (`/en/`), for the language index.
    pub fn locale_route(&self, loc: usize) -> String {
        self.site.locales[loc]
            .folder
            .as_ref()
            .map_or_else(|| "/".into(), |f| format!("/{f}/"))
    }
}

pub fn write(site: &Site) -> Result<Output, String> {
    project::validate(site)?;
    let cx = Cx::new(site);
    let mut out = Output::default();
    for (path, bytes) in &site.assets {
        out.files.insert(format!("public/{path}"), bytes.clone());
    }
    let abs = |loc: usize, page: usize| cx.address(page, loc);
    if site.base_url.is_some() {
        let root = cx
            .multi
            .then(|| format!("{}/", site.base_url.as_deref().unwrap_or_default()));
        out.files.insert(
            "public/sitemap.xml".into(),
            site.sitemap(&abs, root).into_bytes(),
        );
    }
    out.files
        .insert("public/robots.txt".into(), site.robots().into_bytes());
    project::write_shared(&cx, &mut out)?;
    if cx.spa {
        vite::write(&cx, &mut out)?;
    } else {
        nuxt::write(&cx, &mut out)?;
    }
    Ok(out)
}

/// The stylesheet the app imports, by styling.
pub(super) fn global_stylesheet(site: &Site) -> (String, String) {
    let st = &site.style;
    match site.options.styling {
        Styling::Scss => ("main.scss".into(), sass_safe(st.global_css("/"))),
        Styling::Tailwind => (
            "main.css".into(),
            format!(
                "@import \"tailwindcss\";\n{}{}{}{}",
                st.tailwind_theme(),
                st.tokens_css(),
                st.fonts_css("/"),
                st.overrides_css("/", |_| true)
            ),
        ),
        _ => ("main.css".into(), st.global_css("/")),
    }
}

/// Sass would read `#{` in a value as interpolation.
pub(super) fn sass_safe(css: String) -> String {
    css.replace("#{", "\\23 {")
}
