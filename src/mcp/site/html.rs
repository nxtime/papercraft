//! The html writer: one static `.html` file per page and language, no JavaScript, a shared
//! stylesheet (written directly, or built by `npm run build` from SCSS or Tailwind), the font
//! faces and pictures under `assets/`, `robots.txt` and, with a `base_url`, `sitemap.xml`.

use super::markup::{self, Dialect, Hooks, Link, TextOut, Val, escape_attr, escape_text};
use super::model::{HeadTag, Output, Site, Styling};
use super::tree::{Element, LinkTarget};
use crate::assets;
use serde_json::json;

/// Where the compiled stylesheet lands when `npm run build` makes it.
const BUILT_CSS: &str = "assets/site.css";
/// Hex digits of the content hash in `assets/site-<hash>.css`.
const CSS_HASH_CHARS: usize = 8;
const INDEX_CSS: &str = "body { font-family: system-ui, sans-serif; margin: 2rem; }\n";

pub fn write(site: &Site) -> Result<Output, String> {
    let mut out = Output {
        files: site.assets.clone(),
        ..Output::default()
    };
    let css = stylesheet(site, &mut out)?;
    for loc in 0..site.locales.len() {
        for page in 0..site.pages.len() {
            out.files.insert(
                site.url_path(page, loc),
                page_html(site, page, loc, &css).into_bytes(),
            );
        }
    }
    let multi = site.locales.iter().any(|l| l.folder.is_some());
    if multi {
        out.files
            .insert("index.html".into(), language_index(site).into_bytes());
    }
    if let Some(base) = &site.base_url {
        let abs = |loc: usize, page: usize| format!("{base}/{}", site.url_path(page, loc));
        out.files.insert(
            "sitemap.xml".into(),
            site.sitemap(&abs, multi.then(|| format!("{base}/index.html")))
                .into_bytes(),
        );
    }
    out.files
        .insert("robots.txt".into(), site.robots().into_bytes());
    Ok(out)
}

/// Writes the stylesheet sources and returns the path pages link, relative to the site root.
fn stylesheet(site: &Site, out: &mut Output) -> Result<String, String> {
    let stems: Vec<(usize, String)> = site
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p.stem.clone()))
        .collect();
    // The stylesheet sits in `assets/`, so the pictures it names are one folder up.
    match site.options.styling {
        Styling::Scss => {
            for (path, content) in site.style.scss("../", &stems) {
                out.files
                    .insert(format!("src/styles/{path}"), content.into_bytes());
            }
            out.files.insert(
                "package.json".into(),
                package_json(
                    "sass --no-source-map src/styles/main.scss assets/site.css",
                    &[("sass", "^1.107.0")],
                )?,
            );
            out.next_steps.push("Run `npm install`, then `npm run build` in the output folder: it compiles src/styles/main.scss to assets/site.css, which every page links. Run `npm run build` again after changing the styles.".into());
            Ok(BUILT_CSS.into())
        }
        Styling::Tailwind => {
            let input = format!(
                "@import \"tailwindcss\";\n{}{}{}{}",
                site.style.tailwind_theme(),
                site.style.tokens_css(),
                site.style.fonts_css("../"),
                site.style.overrides_css("../", |_| true)
            );
            out.files.insert("src/input.css".into(), input.into_bytes());
            out.files.insert(
                "package.json".into(),
                package_json(
                    "tailwindcss -i src/input.css -o assets/site.css --minify",
                    &[("@tailwindcss/cli", "^4.0.0"), ("tailwindcss", "^4.0.0")],
                )?,
            );
            out.next_steps.push("Run `npm install`, then `npm run build` in the output folder: Tailwind scans the pages and writes assets/site.css, which every page links. Run it again after changing a page.".into());
            Ok(BUILT_CSS.into())
        }
        _ => {
            let css = site.style.all_css("../");
            let path = format!(
                "assets/site-{}.css",
                &assets::hash(css.as_bytes())[..CSS_HASH_CHARS]
            );
            out.files.insert(path.clone(), css.into_bytes());
            Ok(path)
        }
    }
}

fn package_json(build: &str, dev: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let deps: serde_json::Map<String, serde_json::Value> = dev
        .iter()
        .map(|(k, v)| ((*k).to_owned(), json!(v)))
        .collect();
    let pkg = json!({"private": true, "scripts": {"build": build}, "devDependencies": deps});
    serde_json::to_vec_pretty(&pkg)
        .map(|mut b| {
            b.push(b'\n');
            b
        })
        .map_err(|e| e.to_string())
}

struct HtmlHooks<'a> {
    site: &'a Site,
    loc: usize,
    /// What a page in this locale's folder puts before a site-root path.
    prefix: &'a str,
}

impl Hooks for HtmlHooks<'_> {
    fn class(&self, el: &Element) -> Option<Val> {
        let class = if self.site.options.styling == Styling::Tailwind {
            el.tailwind_classes()
        } else {
            el.class.clone()
        };
        (!class.is_empty()).then_some(Val::Lit(class))
    }

    fn link(&self, _: &Element, target: &LinkTarget) -> Link {
        let href = match target {
            LinkTarget::Page(i) => format!("{}.html", self.site.pages[*i].stem),
            LinkTarget::Url(u) => u.clone(),
        };
        Link {
            tag: "a".into(),
            attrs: vec![("href".into(), Val::Lit(href))],
        }
    }

    fn asset(&self, path: &str) -> Val {
        Val::Lit(format!("{}{path}", self.prefix))
    }

    fn message(&self, key: &str) -> TextOut {
        TextOut::Lit(
            self.site.messages[self.loc]
                .get(key)
                .cloned()
                .unwrap_or_default(),
        )
    }
}

fn page_html(site: &Site, page: usize, loc: usize, css: &str) -> String {
    let l = &site.locales[loc];
    let prefix = if l.folder.is_some() { "../" } else { "" };
    let url = |from: usize, to: usize, p: usize| {
        link_to(
            site.base_url.as_deref(),
            site.locales[from].folder.as_deref(),
            site.locales[to].folder.as_deref(),
            &format!("{}.html", site.pages[p].stem),
        )
    };
    let mut tags = site.head_tags(page, loc, &url);
    tags.extend(site.asset_tags(page, prefix));
    tags.push(HeadTag {
        tag: "link",
        attrs: vec![
            ("rel", "stylesheet".into()),
            ("href", format!("{prefix}{css}")),
        ],
    });
    let mut body = String::new();
    markup::render(
        Dialect::Html,
        &HtmlHooks { site, loc, prefix },
        &site.pages[page].root,
        1,
        &mut body,
    );
    shell(
        l.tag.as_deref(),
        l.class.as_deref(),
        &site.pages[page].title[loc],
        &tags,
        &body,
        None,
    )
}

/// The document around `body`. `inline_css` is for the one page that has no stylesheet.
fn shell(
    lang: Option<&str>,
    class: Option<&str>,
    title: &str,
    tags: &[HeadTag],
    body: &str,
    inline_css: Option<&str>,
) -> String {
    let attr = |k: &str, v: Option<&str>| {
        v.map(|v| format!(" {k}=\"{}\"", escape_attr(Dialect::Html, v)))
            .unwrap_or_default()
    };
    let mut out = format!(
        "<!doctype html>\n<html{}{}>\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
        attr("lang", lang),
        attr("class", class)
    );
    out.push_str(&format!(
        "  <title>{}</title>\n",
        escape_text(Dialect::Html, title)
    ));
    for t in tags {
        out.push_str(&format!("  {}\n", t.to_html()));
    }
    if let Some(css) = inline_css {
        out.push_str(&format!(
            "  <style>\n    {}\n  </style>\n",
            css.trim_end().replace("</", r"<\/")
        ));
    }
    out.push_str("</head>\n<body>\n");
    out.push_str(body);
    out.push_str("</body>\n</html>\n");
    out
}

/// The root page of a site with several languages or modes: one link per version.
fn language_index(site: &Site) -> String {
    let url = |to: usize| {
        link_to(
            site.base_url.as_deref(),
            None,
            site.locales[to].folder.as_deref(),
            "index.html",
        )
    };
    let langs = site.language_locales();
    let mut tags = vec![
        HeadTag {
            tag: "meta",
            attrs: vec![
                ("name", "description".into()),
                ("content", "Choose a version of this site.".into()),
            ],
        },
        HeadTag {
            tag: "link",
            attrs: vec![
                ("rel", "canonical".into()),
                (
                    "href",
                    link_to(site.base_url.as_deref(), None, None, "index.html"),
                ),
            ],
        },
    ];
    for &l in &langs {
        tags.push(HeadTag {
            tag: "link",
            attrs: vec![
                ("rel", "alternate".into()),
                ("hreflang", site.locales[l].tag.clone().unwrap_or_default()),
                ("href", url(l)),
            ],
        });
    }
    if let Some(&first) = langs.first() {
        tags.push(HeadTag {
            tag: "link",
            attrs: vec![
                ("rel", "alternate".into()),
                ("hreflang", "x-default".into()),
                ("href", url(first)),
            ],
        });
    }
    let items: String = site
        .locales
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let href = link_to(None, None, l.folder.as_deref(), "index.html");
            let lang = l
                .tag
                .as_ref()
                .filter(|_| langs.contains(&i))
                .map(|t| {
                    format!(
                        " lang=\"{0}\" hreflang=\"{0}\"",
                        escape_attr(Dialect::Html, t)
                    )
                })
                .unwrap_or_default();
            format!(
                "    <li><a href=\"{}\"{lang}>{}</a></li>\n",
                escape_attr(Dialect::Html, &href),
                escape_text(Dialect::Html, &l.label)
            )
        })
        .collect();
    let body = format!(
        "  <main>\n    <h1>{}</h1>\n    <ul>\n{items}    </ul>\n  </main>\n",
        escape_text(Dialect::Html, &site.site_title)
    );
    shell(
        site.index_lang(),
        None,
        &site.site_title,
        &tags,
        &body,
        Some(INDEX_CSS),
    )
}

/// A URL for `file` in folder `to`, as seen from a page in folder `from`: absolute under `base`,
/// else relative.
pub fn link_to(base: Option<&str>, from: Option<&str>, to: Option<&str>, file: &str) -> String {
    if let Some(base) = base {
        return format!(
            "{base}/{}{file}",
            to.map(|d| format!("{d}/")).unwrap_or_default()
        );
    }
    match (from, to) {
        (f, t) if f == t => file.to_owned(),
        (f, Some(t)) => format!("{}{t}/{file}", if f.is_some() { "../" } else { "" }),
        (_, None) => file.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_cannot_close_the_style_block_of_the_index_page() {
        let out = shell(
            None,
            None,
            "t",
            &[],
            "",
            Some(".x { font-family: \"</style><script>x</script>\"; }"),
        );
        assert!(
            out.contains(r"<\/style>") && out.matches("</style>").count() == 1,
            "{out}"
        );
    }

    #[test]
    fn links_are_relative_between_folders_and_absolute_with_a_base() {
        assert_eq!(link_to(None, Some("en"), Some("en"), "a.html"), "a.html");
        assert_eq!(
            link_to(None, Some("en"), Some("pt-BR"), "a.html"),
            "../pt-BR/a.html"
        );
        assert_eq!(
            link_to(None, None, Some("pt-BR"), "index.html"),
            "pt-BR/index.html"
        );
        assert_eq!(
            link_to(Some("https://x.test"), Some("en"), Some("pt-BR"), "a.html"),
            "https://x.test/pt-BR/a.html"
        );
    }
}
