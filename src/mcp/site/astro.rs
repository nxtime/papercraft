//! The astro writer: a static Astro 7 project (`npm run build` makes `dist/` with no client
//! JavaScript). One component per page frame, one Layout that owns the head, `public/` with the
//! assets, `sitemap.xml` and `robots.txt`.
//!
//! With several locales the pages are `src/pages/[lang]/*.astro` with `getStaticPaths`, the
//! messages are one JSON catalog per locale, and `/` lists the versions. A plain lookup is used
//! instead of Astro's i18n routing: the routes are fixed, so its middleware adds nothing, and its
//! helpers lower-case `pt-BR`. A single locale writes pages at the root with literal text.
//! All user text reaches the files through `markup`'s Astro escaping, or as JSON string literals
//! in `src/site.ts` (never inside the frontmatter of a page).

use super::markup::{self, Dialect, Hooks, Link, TextOut, Val, escape_attr, escape_text};
use super::model::{HeadTag, Locale, Output, Site, Styling};
use super::style::class_slug;
use super::tree::{Element, LinkTarget};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const ASTRO_RANGE: &str = "^7.4.7";
const SASS_RANGE: &str = "^1.107.1";
const TAILWIND_RANGES: [(&str, &str); 2] =
    [("@tailwindcss/vite", "4.3.4"), ("tailwindcss", "4.3.4")];
const NODE_RANGE: &str = ">=22.23.4";
/// Key of the only locale of a single-language site in `src/site.ts`.
const SINGLE_KEY: &str = "site";
const INDEX_CSS: &str = "body { font-family: system-ui, sans-serif; margin: 2rem; }";
const TSCONFIG: &str = "{\n  \"extends\": \"astro/tsconfigs/strict\",\n  \"include\": [\".astro/types.d.ts\", \"**/*\"],\n  \"exclude\": [\"dist\"]\n}\n";

pub fn write(site: &Site) -> Result<Output, String> {
    check_names(site)?;
    let multi = site.locales.iter().any(|l| l.folder.is_some());
    let mut out = Output::default();
    for (path, bytes) in &site.assets {
        out.files.insert(format!("public/{path}"), bytes.clone());
    }
    out.files
        .insert("public/robots.txt".into(), site.robots().into_bytes());
    if let Some(base) = &site.base_url {
        let abs = |loc: usize, page: usize| format!("{base}{}", route(site, page, loc));
        out.files.insert(
            "public/sitemap.xml".into(),
            site.sitemap(&abs, multi.then(|| format!("{base}/")))
                .into_bytes(),
        );
    }
    out.files.insert("package.json".into(), package_json(site)?);
    out.files
        .insert("astro.config.mjs".into(), config(site)?.into_bytes());
    out.files.insert("tsconfig.json".into(), TSCONFIG.into());
    out.files
        .insert("src/site.ts".into(), site_ts(site)?.into_bytes());
    out.files
        .insert("src/layouts/Layout.astro".into(), layout(site).into_bytes());
    styles(site, &mut out);
    for (i, page) in site.pages.iter().enumerate() {
        out.files.insert(
            format!("src/components/{}.astro", page.component),
            component(site, i, multi).into_bytes(),
        );
        let path = match (multi, i) {
            (true, _) => format!("src/pages/[lang]/{}.astro", page.stem),
            (false, _) => format!("src/pages/{}.astro", page.stem),
        };
        out.files
            .insert(path, page_file(site, i, multi).into_bytes());
    }
    if multi {
        write_catalogs(site, &mut out)?;
        out.files.insert(
            "src/pages/index.astro".into(),
            language_index(site).into_bytes(),
        );
    }
    out.next_steps.push("Run `npm install && npm run build` in the output folder: the static site is written to dist/ (`npm run dev` serves it while you edit). Run the export again after changing the design.".into());
    Ok(out)
}

/// Names that become file names or identifiers must be plain, and unique on a case-insensitive disk.
fn check_names(site: &Site) -> Result<(), String> {
    let plain = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    let mut taken = BTreeSet::from(["layout".to_owned()]);
    for p in &site.pages {
        let ident = p
            .component
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
            && p.component.chars().all(|c| c.is_ascii_alphanumeric());
        if !ident || !plain(&p.stem) || !taken.insert(p.component.to_lowercase()) {
            return Err(format!(
                "page `{}` has no usable component name (`{}`) or file name (`{}`)",
                p.name, p.component, p.stem
            ));
        }
    }
    for l in &site.locales {
        if l.folder.as_deref().is_some_and(|f| !plain(f)) {
            return Err(format!(
                "locale folder `{}` cannot be used as a route",
                l.folder.as_deref().unwrap_or_default()
            ));
        }
    }
    Ok(())
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn pretty(v: &Value) -> Result<Vec<u8>, String> {
    let mut b = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    b.push(b'\n');
    Ok(b)
}

fn package_json(site: &Site) -> Result<Vec<u8>, String> {
    let mut dev = serde_json::Map::new();
    match site.options.styling {
        Styling::Scss => {
            dev.insert("sass".into(), json!(SASS_RANGE));
        }
        Styling::Tailwind => {
            for (name, range) in TAILWIND_RANGES {
                dev.insert(name.into(), json!(range));
            }
        }
        _ => {}
    }
    let mut pkg = json!({
        "name": "site", "private": true, "type": "module", "version": "0.0.1",
        "engines": {"node": NODE_RANGE},
        "scripts": {"dev": "astro dev", "build": "astro build", "preview": "astro preview", "astro": "astro"},
        "dependencies": {"astro": ASTRO_RANGE},
    });
    if !dev.is_empty() {
        pkg["devDependencies"] = Value::Object(dev);
    }
    pretty(&pkg)
}

fn config(site: &Site) -> Result<String, String> {
    let tailwind = site.options.styling == Styling::Tailwind;
    let mut out = String::from("// @ts-check\nimport { defineConfig } from 'astro/config';\n");
    if tailwind {
        out.push_str("import tailwindcss from '@tailwindcss/vite';\n");
    }
    out.push_str("\nexport default defineConfig({\n");
    if let Some(base) = &site.base_url {
        out.push_str(&format!("  site: {},\n", json_str(base)));
    }
    out.push_str(
        "  output: 'static',\n  trailingSlash: 'always',\n  build: { format: 'directory' },\n",
    );
    if tailwind {
        out.push_str("  vite: { plugins: [tailwindcss()] },\n");
    }
    out.push_str("});\n");
    Ok(out)
}

/// The site-wide stylesheet, which the Layout imports, and for scss the page partials.
fn styles(site: &Site, out: &mut Output) {
    let style = &site.style;
    let (path, text) = match site.options.styling {
        Styling::Scss => {
            let stems: Vec<(usize, String)> = site
                .pages
                .iter()
                .enumerate()
                .map(|(i, p)| (i, p.stem.clone()))
                .collect();
            let parts = style.scss("/", &stems);
            let shared = ["_tokens.scss", "_fonts.scss", "_base.scss", "_shared.scss"];
            let text: String = parts
                .iter()
                .filter(|(p, _)| shared.contains(&p.as_str()))
                .map(|(_, c)| c.as_str())
                .collect();
            ("src/styles/global.scss", text)
        }
        Styling::Tailwind => (
            "src/styles/global.css",
            format!(
                "@import \"tailwindcss\";\n{}{}{}{}",
                style.tailwind_theme(),
                style.tokens_css(),
                style.fonts_css("/"),
                style.overrides_css("/", |_| true)
            ),
        ),
        _ => ("src/styles/global.css", style.global_css("/")),
    };
    out.files.insert(path.into(), text.into_bytes());
}

/// `locale`'s key in `src/site.ts`: its folder, or one fixed key for a single-language site.
fn key(l: &Locale) -> &str {
    l.folder.as_deref().unwrap_or(SINGLE_KEY)
}

/// The route of a page, with the trailing slash `trailingSlash: 'always'` serves.
fn route(site: &Site, page: usize, loc: usize) -> String {
    let r = site.url_path(page, loc);
    if r.ends_with('/') { r } else { format!("{r}/") }
}

/// `{"a": "b", ...}` with the keys in the given order.
fn object(pairs: &[(&str, &str)]) -> String {
    let items: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{}: {}", json_str(k), json_str(v)))
        .collect();
    format!("{{ {} }}", items.join(", "))
}

fn tag_object(t: &HeadTag) -> String {
    let attrs: Vec<(&str, &str)> = t.attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    format!("{{ tag: {}, attrs: {} }}", json_str(t.tag), object(&attrs))
}

/// The head of one page in one locale, as the data `Layout.astro` prints.
fn meta(site: &Site, page: usize, loc: usize) -> String {
    let base = site.base_url.as_deref().unwrap_or_default();
    let url = |_from: usize, to: usize, p: usize| format!("{base}{}", route(site, p, to));
    let mut tags = site.head_tags(page, loc, &url);
    tags.extend(site.asset_tags(page, "/"));
    let l = &site.locales[loc];
    let mut fields = vec![format!("title: {}", json_str(&site.pages[page].title[loc]))];
    fields.extend(l.tag.as_deref().map(|t| format!("lang: {}", json_str(t))));
    fields.extend(l.class.as_deref().map(|c| format!("cls: {}", json_str(c))));
    let tags: Vec<String> = tags
        .iter()
        .map(|t| format!("      {}", tag_object(t)))
        .collect();
    format!(
        "{{ {}, tags: [\n{}\n    ] }}",
        fields.join(", "),
        tags.join(",\n")
    )
}

/// `src/site.ts`: the locales, the catalogs and every page's head. Strings are JSON literals.
fn site_ts(site: &Site) -> Result<String, String> {
    let multi = site.locales.iter().any(|l| l.folder.is_some());
    let mut out = String::from("// Generated by papercraft export_site.\n");
    if multi {
        for (i, l) in site.locales.iter().enumerate() {
            out.push_str(&format!(
                "import m{i} from {};\n",
                json_str(&format!("./i18n/{}.json", key(l)))
            ));
        }
    }
    out.push_str("\nexport interface Tag { tag: string; attrs: Record<string, string> }\n");
    out.push_str(
        "export interface Meta { title: string; lang?: string; cls?: string; tags: Tag[] }\n\n",
    );
    let locales: Vec<String> = site
        .locales
        .iter()
        .map(|l| format!("  {}", object(&[("folder", key(l)), ("label", &l.label)])))
        .collect();
    out.push_str(&format!(
        "export const locales = [\n{},\n];\n",
        locales.join(",\n")
    ));
    let catalogs: Vec<String> = site
        .locales
        .iter()
        .enumerate()
        .map(|(i, l)| format!("  {}: m{i}", json_str(key(l))))
        .collect();
    if multi {
        out.push_str(&format!(
            "export const messages: Record<string, Record<string, string>> = {{\n{},\n}};\n",
            catalogs.join(",\n")
        ));
    }
    out.push_str("\nexport const pages: Record<string, Record<string, Meta>> = {\n");
    for (p, page) in site.pages.iter().enumerate() {
        out.push_str(&format!("  {}: {{\n", json_str(&page.stem)));
        for (loc, l) in site.locales.iter().enumerate() {
            out.push_str(&format!(
                "    {}: {},\n",
                json_str(key(l)),
                meta(site, p, loc)
            ));
        }
        out.push_str("  },\n");
    }
    out.push_str("};\n");
    Ok(out)
}

fn write_catalogs(site: &Site, out: &mut Output) -> Result<(), String> {
    for (l, messages) in site.locales.iter().zip(&site.messages) {
        out.files.insert(
            format!("src/i18n/{}.json", key(l)),
            pretty(&json!(messages))?,
        );
    }
    Ok(())
}

fn layout(site: &Site) -> String {
    let css = if site.options.styling == Styling::Scss {
        "scss"
    } else {
        "css"
    };
    let first = site.locales.first().map_or(SINGLE_KEY, key);
    format!(
        "---\nimport \"../styles/global.{css}\";\nimport {{ pages }} from \"../site\";\n\ninterface Props {{ page: string; lang?: string }}\nconst {{ page, lang = {} }} = Astro.props;\nconst meta = pages[page][lang];\n---\n\
<!doctype html>\n<html lang={{meta.lang}} class={{meta.cls}}>\n  <head>\n    <meta charset=\"utf-8\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n    <title>{{meta.title}}</title>\n    {{meta.tags.map(({{ tag, attrs }}) => (tag === \"meta\" ? <meta {{...attrs}} /> : <link {{...attrs}} />))}}\n  </head>\n  <body>\n    <slot />\n  </body>\n</html>\n",
        json_str(first)
    )
}

fn page_file(site: &Site, page: usize, multi: bool) -> String {
    let p = &site.pages[page];
    let depth = if multi { "../../" } else { "../" };
    let mut out = format!(
        "---\nimport Layout from \"{depth}layouts/Layout.astro\";\nimport {} from \"{depth}components/{}.astro\";\n",
        p.component, p.component
    );
    if multi {
        out.push_str(&format!("import {{ locales }} from \"{depth}site\";\n\nexport function getStaticPaths() {{\n  return locales.map((l) => ({{ params: {{ lang: l.folder }} }}));\n}}\n\nconst {{ lang }} = Astro.params;\n---\n"));
        out.push_str(&format!(
            "<Layout page={{{}}} lang={{lang}}>\n  <{} lang={{lang}} />\n</Layout>\n",
            json_str(&p.stem),
            p.component
        ));
    } else {
        out.push_str(&format!(
            "---\n<Layout page={{{}}}>\n  <{} />\n</Layout>\n",
            json_str(&p.stem),
            p.component
        ));
    }
    out
}

/// A component per page: the markup, and the styles only this page uses.
fn component(site: &Site, page: usize, multi: bool) -> String {
    let p = &site.pages[page];
    let mut out = String::from("---\n");
    if multi {
        out.push_str("import { messages } from \"../site\";\n\ninterface Props { lang: string }\nconst { lang } = Astro.props;\nconst t = messages[lang];\n");
    }
    out.push_str("---\n");
    markup::render(
        Dialect::Astro,
        &AstroHooks { site, multi },
        &p.root,
        0,
        &mut out,
    );
    let style = &site.style;
    let (attr, css) = match site.options.styling {
        Styling::Css => ("", style.page_css(page, "/")),
        Styling::Scss => (" lang=\"scss\"", scss_page(site, page)),
        _ => ("", String::new()),
    };
    if !css.trim().is_empty() {
        out.push_str(&format!(
            "<style{attr}>\n{}</style>\n",
            css.replace("</", r"<\/")
        ));
    }
    out
}

fn scss_page(site: &Site, page: usize) -> String {
    let stems: Vec<(usize, String)> = site
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p.stem.clone()))
        .collect();
    let file = format!("pages/_{}.scss", class_slug(&site.pages[page].stem));
    site.style
        .scss("/", &stems)
        .into_iter()
        .find(|(p, _)| *p == file)
        .map(|(_, c)| c)
        .unwrap_or_default()
}

struct AstroHooks<'a> {
    site: &'a Site,
    multi: bool,
}

impl Hooks for AstroHooks<'_> {
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
            LinkTarget::Url(u) => Val::Lit(u.clone()),
            LinkTarget::Page(i) if self.multi => {
                let tail = if *i == 0 {
                    "/".to_owned()
                } else {
                    format!("/{}/", self.site.pages[*i].stem)
                };
                Val::Expr(format!("\"/\" + lang + {}", json_str(&tail)))
            }
            LinkTarget::Page(i) => Val::Lit(if *i == 0 {
                "/".to_owned()
            } else {
                format!("/{}/", self.site.pages[*i].stem)
            }),
        };
        Link {
            tag: "a".into(),
            attrs: vec![("href".into(), href)],
        }
    }

    fn asset(&self, path: &str) -> Val {
        Val::Lit(format!("/{path}"))
    }

    fn message(&self, key: &str) -> TextOut {
        if self.multi {
            return TextOut::Raw(format!("{{t[{}]}}", json_str(key)));
        }
        TextOut::Lit(
            self.site
                .messages
                .first()
                .and_then(|m| m.get(key))
                .cloned()
                .unwrap_or_default(),
        )
    }
}

fn head_line(t: &HeadTag) -> String {
    let mut s = format!("    <{}", t.tag);
    for (k, v) in &t.attrs {
        if v.is_empty() {
            s.push_str(&format!(" {k}"));
        } else {
            s.push_str(&format!(" {k}=\"{}\"", escape_attr(Dialect::Astro, v)));
        }
    }
    s.push_str(" />\n");
    s
}

/// `/`: one link per version, with its own head (it is not a locale page).
fn language_index(site: &Site) -> String {
    let base = site.base_url.as_deref().unwrap_or_default();
    let langs = site.language_locales();
    let href = |to: usize| {
        format!(
            "{base}/{}/",
            site.locales[to].folder.as_deref().unwrap_or_default()
        )
    };
    let pair = |name: &'static str, v: String| HeadTag {
        tag: "meta",
        attrs: vec![("name", name.into()), ("content", v)],
    };
    let mut tags = vec![
        pair("description", "Choose a version of this site.".into()),
        HeadTag {
            tag: "link",
            attrs: vec![("rel", "canonical".into()), ("href", format!("{base}/"))],
        },
    ];
    for &l in &langs {
        tags.push(HeadTag {
            tag: "link",
            attrs: vec![
                ("rel", "alternate".into()),
                ("hreflang", site.locales[l].tag.clone().unwrap_or_default()),
                ("href", href(l)),
            ],
        });
    }
    if let Some(&first) = langs.first() {
        tags.push(HeadTag {
            tag: "link",
            attrs: vec![
                ("rel", "alternate".into()),
                ("hreflang", "x-default".into()),
                ("href", href(first)),
            ],
        });
    }
    let mut out = format!(
        "<!doctype html>\n<html{}>\n  <head>\n    <meta charset=\"utf-8\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n    <title>{}</title>\n",
        site.index_lang()
            .map(|t| format!(" lang=\"{}\"", escape_attr(Dialect::Astro, t)))
            .unwrap_or_default(),
        escape_text(Dialect::Astro, &site.site_title)
    );
    tags.iter().for_each(|t| out.push_str(&head_line(t)));
    out.push_str(&format!("    <style is:global>\n      {INDEX_CSS}\n    </style>\n  </head>\n  <body>\n    <main>\n      <h1>{}</h1>\n      <ul>\n", escape_text(Dialect::Astro, &site.site_title)));
    for (i, l) in site.locales.iter().enumerate() {
        let lang = l
            .tag
            .as_ref()
            .filter(|_| langs.contains(&i))
            .map(|t| {
                format!(
                    " lang=\"{0}\" hreflang=\"{0}\"",
                    escape_attr(Dialect::Astro, t)
                )
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "        <li><a href=\"/{}/\"{lang}>{}</a></li>\n",
            escape_attr(Dialect::Astro, l.folder.as_deref().unwrap_or_default()),
            escape_text(Dialect::Astro, &l.label)
        ));
    }
    out.push_str("      </ul>\n    </main>\n  </body>\n</html>\n");
    out
}

#[cfg(test)]
#[path = "astro_tests.rs"]
mod tests;
