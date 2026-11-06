//! What a framework writer receives: the options, the pages as element trees, the locales and
//! their messages, the style model, the asset bytes and the shared head, sitemap and robots.

use super::style::StyleModel;
use super::tree::Element;
use crate::mcp::codegen::{Escape, escape};
use crate::model::NodeId;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framework {
    Html,
    React,
    Vue,
    Angular,
    Astro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Render {
    Static,
    Spa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Styling {
    Css,
    Scss,
    Tailwind,
    VanillaExtract,
    StyledComponents,
    Emotion,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub framework: Framework,
    pub render: Render,
    pub styling: Styling,
}

impl Options {
    /// The options of a call: defaults are html, static and css; combinations that do not exist
    /// are errors naming what to change.
    pub fn parse(a: &Value) -> Result<Self, String> {
        let pick = |key: &str, allowed: &[&str]| -> Result<Option<String>, String> {
            match a
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                None => Ok(None),
                Some(v) if allowed.contains(&v) => Ok(Some(v.to_owned())),
                Some(v) => Err(format!(
                    "`{key}` must be one of {} (got `{v}`)",
                    allowed.join(", ")
                )),
            }
        };
        let framework =
            match pick("framework", &["html", "react", "vue", "angular", "astro"])?.as_deref() {
                Some("react") => Framework::React,
                Some("vue") => Framework::Vue,
                Some("angular") => Framework::Angular,
                Some("astro") => Framework::Astro,
                _ => Framework::Html,
            };
        let render_given = pick("render", &["static", "spa"])?;
        let render = if render_given.as_deref() == Some("spa") {
            Render::Spa
        } else {
            Render::Static
        };
        let styling_given = pick(
            "styling",
            &[
                "css",
                "scss",
                "tailwind",
                "vanilla-extract",
                "styled-components",
                "emotion",
            ],
        )?;
        let styling = match styling_given.as_deref() {
            Some("scss") => Styling::Scss,
            Some("tailwind") => Styling::Tailwind,
            Some("vanilla-extract") => Styling::VanillaExtract,
            Some("styled-components") => Styling::StyledComponents,
            Some("emotion") => Styling::Emotion,
            _ => Styling::Css,
        };
        if matches!(
            styling,
            Styling::VanillaExtract | Styling::StyledComponents | Styling::Emotion
        ) && framework != Framework::React
        {
            return Err(format!(
                "`styling: {}` is CSS-in-JS for React: use `framework: react`, or pick css, scss or tailwind",
                styling_given.unwrap_or_default()
            ));
        }
        if render == Render::Spa
            && !matches!(
                framework,
                Framework::React | Framework::Vue | Framework::Angular
            )
        {
            return Err("`render: spa` applies to react, vue and angular; html and astro always write static pages".into());
        }
        Ok(Options {
            framework,
            render,
            styling,
        })
    }
}

/// A screen to export, before it is built.
#[derive(Debug, Clone)]
pub struct Frame {
    pub id: NodeId,
    pub name: String,
    /// File name stem and route segment: `index` for the first, then a unique slug.
    pub stem: String,
    /// PascalCase, unique, valid as a TypeScript, Vue, Angular or Astro identifier.
    pub component: String,
}

/// One language or mode of the site.
#[derive(Debug, Clone)]
pub struct Locale {
    /// BCP 47 tag of the content: a language mode's name, else the `lang` argument.
    pub tag: Option<String>,
    /// A language version (a mode named like `pt-BR`): `hreflang` links point to it.
    pub hreflang: bool,
    /// Folder or route prefix; None for a single-locale site.
    pub folder: Option<String>,
    pub label: String,
    /// Selector prefix of this locale's style overrides: empty for the first locale.
    pub scope: String,
    /// Class the `<html>` element carries so a non-language mode's scope matches.
    pub class: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub name: String,
    pub stem: String,
    pub component: String,
    pub root: Element,
    /// Per locale.
    pub title: Vec<String>,
    pub description: Vec<String>,
}

pub struct Site {
    pub options: Options,
    pub site_title: String,
    /// Without a trailing slash.
    pub base_url: Option<String>,
    pub locales: Vec<Locale>,
    pub pages: Vec<Page>,
    /// Per locale: message key to text. Only keys whose text differs between locales exist.
    pub messages: Vec<BTreeMap<String, String>>,
    /// Site-root relative path (`assets/..`) to bytes: pictures, vector files and fonts.
    pub assets: BTreeMap<String, Vec<u8>>,
    pub style: StyleModel,
    pub warnings: Vec<String>,
}

/// What a writer returns: files relative to the output folder, and what to do next.
#[derive(Default)]
pub struct Output {
    pub files: BTreeMap<String, Vec<u8>>,
    pub next_steps: Vec<String>,
    pub warnings: Vec<String>,
}

/// A `<meta>` or `<link>` for the head, attributes in print order.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadTag {
    pub tag: &'static str,
    pub attrs: Vec<(&'static str, String)>,
}

/// The address of a page: `(from locale, to locale, page index)` to the href a page of `from`
/// writes for `page` in `to`.
pub type UrlFn<'a> = &'a dyn Fn(usize, usize, usize) -> String;

fn tag(kind: &'static str, attrs: &[(&'static str, &str)]) -> HeadTag {
    HeadTag {
        tag: kind,
        attrs: attrs.iter().map(|(k, v)| (*k, (*v).to_owned())).collect(),
    }
}

impl HeadTag {
    /// The tag as HTML, without indentation or a line break. A valueless attribute (`crossorigin`)
    /// is printed bare.
    pub fn to_html(&self) -> String {
        let mut out = format!("<{}", self.tag);
        for (k, v) in &self.attrs {
            if v.is_empty() {
                out.push_str(&format!(" {k}"));
            } else {
                out.push_str(&format!(" {k}=\"{}\"", escape(Escape::Html, v)));
            }
        }
        out.push('>');
        out
    }
}

/// `type` of a font or icon file from its extension.
pub(super) fn mime(path: &str, family: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or_default();
    match ext {
        "svg" => "image/svg+xml".into(),
        "jpg" => "image/jpeg".into(),
        _ => format!("{family}/{ext}"),
    }
}

impl Site {
    /// The site icon's address, with `prefix` before a site-root path. No icon layer: `data:,`
    /// keeps the browser from asking for `/favicon.ico`.
    pub fn icon_href(&self, prefix: &str) -> String {
        self.style
            .favicon
            .as_ref()
            .map_or_else(|| "data:,".to_owned(), |p| format!("{prefix}{p}"))
    }

    /// The site icon and the font preloads of a page, with `prefix` before the site-root paths.
    /// Preloads: the face of the page's h1 and the face most body text uses, never more than two.
    pub fn asset_tags(&self, page: usize, prefix: &str) -> Vec<HeadTag> {
        let mut icon = vec![("rel", "icon".to_owned())];
        if let Some(p) = &self.style.favicon {
            icon.push(("type", mime(p, "image")));
        }
        icon.push(("href", self.icon_href(prefix)));
        let mut tags = vec![HeadTag {
            tag: "link",
            attrs: icon,
        }];
        let h1 = self.style.h1_faces.get(page).copied().flatten();
        let mut faces: Vec<usize> = h1.into_iter().chain(self.style.preload).collect();
        faces.dedup();
        for face in faces.iter().filter_map(|i| self.style.fonts.get(*i)) {
            tags.push(HeadTag {
                tag: "link",
                attrs: vec![
                    ("rel", "preload".into()),
                    ("href", format!("{prefix}{}", face.path)),
                    ("as", "font".into()),
                    ("type", mime(&face.path, "font")),
                    ("crossorigin", String::new()),
                ],
            });
        }
        tags
    }

    /// Locales that are languages: the ones `hreflang` links between.
    pub fn language_locales(&self) -> Vec<usize> {
        (0..self.locales.len())
            .filter(|i| self.locales[*i].hreflang)
            .collect()
    }

    /// The `lang` of the language picker page: the first language's tag.
    pub fn index_lang(&self) -> Option<&str> {
        let first = self.language_locales().first().copied().unwrap_or(0);
        self.locales.get(first)?.tag.as_deref()
    }

    /// The path of a page under the site root: `about.html` or `pt-BR/about.html` for html, a
    /// route (`/about`, `/pt-BR/about`, `/` for the first page) for every other framework.
    pub fn url_path(&self, page: usize, locale: usize) -> String {
        let (stem, folder) = (
            &self.pages[page].stem,
            self.locales[locale].folder.as_deref(),
        );
        if self.options.framework == Framework::Html {
            return folder.map_or_else(|| format!("{stem}.html"), |f| format!("{f}/{stem}.html"));
        }
        let route = if page == 0 {
            String::new()
        } else {
            stem.clone()
        };
        match folder {
            Some(f) if route.is_empty() => format!("/{f}"),
            Some(f) => format!("/{f}/{route}"),
            None => format!("/{route}"),
        }
    }

    /// Meta and link tags of a page in one locale: description, canonical, `hreflang`
    /// alternates, Open Graph and Twitter card. `url` decides absolute or relative addresses.
    pub fn head_tags(&self, page: usize, loc: usize, url: UrlFn) -> Vec<HeadTag> {
        let (p, l) = (&self.pages[page], &self.locales[loc]);
        let canonical = url(loc, loc, page);
        let mut out = vec![
            tag(
                "meta",
                &[("name", "description"), ("content", &p.description[loc])],
            ),
            tag("link", &[("rel", "canonical"), ("href", &canonical)]),
        ];
        let langs = self.language_locales();
        for &to in &langs {
            out.push(tag(
                "link",
                &[
                    ("rel", "alternate"),
                    (
                        "hreflang",
                        self.locales[to].tag.as_deref().unwrap_or_default(),
                    ),
                    ("href", &url(loc, to, page)),
                ],
            ));
        }
        if let Some(&first) = langs.first() {
            out.push(tag(
                "link",
                &[
                    ("rel", "alternate"),
                    ("hreflang", "x-default"),
                    ("href", &url(loc, first, page)),
                ],
            ));
        }
        out.push(tag(
            "meta",
            &[("property", "og:title"), ("content", &p.title[loc])],
        ));
        out.push(tag(
            "meta",
            &[
                ("property", "og:description"),
                ("content", &p.description[loc]),
            ],
        ));
        out.push(tag(
            "meta",
            &[("property", "og:type"), ("content", "website")],
        ));
        if let Some(t) = &l.tag {
            out.push(tag(
                "meta",
                &[("property", "og:locale"), ("content", &t.replace('-', "_"))],
            ));
        }
        for &other in langs.iter().filter(|o| **o != loc) {
            let t = self.locales[other]
                .tag
                .as_deref()
                .unwrap_or_default()
                .replace('-', "_");
            out.push(tag(
                "meta",
                &[("property", "og:locale:alternate"), ("content", &t)],
            ));
        }
        if self.base_url.is_some() {
            out.push(tag(
                "meta",
                &[("property", "og:url"), ("content", &canonical)],
            ));
        }
        out.push(tag(
            "meta",
            &[("name", "twitter:card"), ("content", "summary")],
        ));
        out
    }

    /// `sitemap.xml`: one entry per page and locale, each with its language alternates.
    /// `abs(locale, page)` is the absolute address; `root` is the language index page, if any.
    pub fn sitemap(&self, abs: &dyn Fn(usize, usize) -> String, root: Option<String>) -> String {
        let e = |t: &str| escape(Escape::Html, t);
        let mut out = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
        );
        let mut entry = |loc: String, alternates: Vec<(String, String)>| {
            out.push_str(&format!("  <url>\n    <loc>{}</loc>\n", e(&loc)));
            for (lang, href) in alternates {
                out.push_str(&format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\"/>\n",
                    e(&lang),
                    e(&href)
                ));
            }
            out.push_str("  </url>\n");
        };
        if let Some(root) = root {
            entry(root, Vec::new());
        }
        let langs = self.language_locales();
        for loc in 0..self.locales.len() {
            for page in 0..self.pages.len() {
                let alt = langs
                    .iter()
                    .map(|&l| {
                        (
                            self.locales[l].tag.clone().unwrap_or_default(),
                            abs(l, page),
                        )
                    })
                    .collect();
                entry(abs(loc, page), alt);
            }
        }
        out.push_str("</urlset>\n");
        out
    }

    /// `robots.txt`: everything may be crawled; the sitemap is named once the address is known.
    pub fn robots(&self) -> String {
        let mut out = String::from("User-agent: *\nAllow: /\n");
        if let Some(base) = &self.base_url {
            out.push_str(&format!("Sitemap: {base}/sitemap.xml\n"));
        }
        out
    }
}

/// A BCP 47 tag the way designers write one: two letters, then region or script subtags
/// (`en`, `pt-BR`, `zh-Hant`). Longer names (`Dark`, `compact`) are plain modes.
pub fn is_lang_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    let primary = parts.next().unwrap_or_default();
    primary.len() == 2
        && primary.chars().all(|c| c.is_ascii_lowercase())
        && parts.all(|p| {
            matches!(p.len(), 2 | 4) && p.chars().all(|c| c.is_ascii_alphabetic())
                || p.len() == 3 && p.chars().all(|c| c.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Result<Options, String> {
        Options::parse(&v)
    }

    #[test]
    fn defaults_are_plain_static_html() {
        let o = parse(json!({})).unwrap();
        assert_eq!(
            (o.framework, o.render, o.styling),
            (Framework::Html, Render::Static, Styling::Css)
        );
    }

    #[test]
    fn css_in_js_needs_react_and_spa_needs_a_client_framework() {
        for s in ["vanilla-extract", "styled-components", "emotion"] {
            assert!(
                parse(json!({"styling": s}))
                    .unwrap_err()
                    .contains("framework: react"),
                "{s}"
            );
            assert!(parse(json!({"framework": "vue", "styling": s})).is_err());
            assert!(parse(json!({"framework": "react", "styling": s})).is_ok());
        }
        assert!(
            parse(json!({"render": "spa"}))
                .unwrap_err()
                .contains("react, vue and angular")
        );
        assert!(parse(json!({"framework": "astro", "render": "spa"})).is_err());
        assert!(parse(json!({"framework": "angular", "render": "spa", "styling": "scss"})).is_ok());
        assert!(
            parse(json!({"framework": "svelte"}))
                .unwrap_err()
                .contains("one of html, react")
        );
        assert!(parse(json!({"styling": "less"})).is_err());
    }

    #[test]
    fn language_tags_are_short_codes_not_mode_names() {
        for ok in ["en", "pt-BR", "zh-Hant", "es-419"] {
            assert!(is_lang_tag(ok), "{ok}");
        }
        for no in ["Dark", "compact", "EN", "e", "en-"] {
            assert!(!is_lang_tag(no), "{no}");
        }
    }
}
