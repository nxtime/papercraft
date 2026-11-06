//! How classes reach the components in each styling: CSS or SCSS modules, Tailwind utilities,
//! vanilla-extract exports, or styled components. The kit scans every page once, so imports
//! never name something a page does not use (the Vite project fails on unused locals).

use super::{Ctx, Files, js, put};
use crate::mcp::site::markup::Val;
use crate::mcp::site::model::{Site, Styling};
use crate::mcp::site::style::{BASE_CSS, Class, StyleModel};
use crate::mcp::site::tree::{Child, Element, LinkTarget};
use std::collections::{BTreeSet, HashMap};

/// npm packages as (name, version range).
pub(super) type Deps = Vec<(&'static str, &'static str)>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Base {
    Tag(String),
    Link,
}

/// A styled component: one class on one base element.
struct Pair {
    name: String,
    class: String,
    base: Base,
    pages: BTreeSet<usize>,
}

/// What one page needs, found by walking its tree in the order the markup printer does.
#[derive(Default)]
struct Scan {
    own: bool,
    shared: bool,
}

pub struct Kit<'a> {
    site: &'a Site,
    styling: Styling,
    next: bool,
    classes: HashMap<&'a str, &'a Class>,
    idents: HashMap<&'a str, String>,
    pairs: Vec<Pair>,
    pair_of: HashMap<(String, Base), usize>,
    scans: Vec<Scan>,
    taken: BTreeSet<String>,
}

const REGISTRY_STYLED: &str = "'use client'

import { type ReactNode, useState } from 'react'
import { useServerInsertedHTML } from 'next/navigation'
import { ServerStyleSheet, StyleSheetManager } from 'styled-components'

/** Collects styled-components rules while the server renders and injects them into <head>. */
export function StyledRegistry({ children }: { children: ReactNode }) {
  const [sheet] = useState(() => new ServerStyleSheet())
  useServerInsertedHTML(() => {
    const styles = sheet.getStyleElement()
    sheet.instance.clearTag()
    return <>{styles}</>
  })
  if (typeof window !== 'undefined') return <>{children}</>
  return <StyleSheetManager sheet={sheet.instance}>{children}</StyleSheetManager>
}
";

const REGISTRY_EMOTION: &str = "'use client'

import { type ReactNode, useState } from 'react'
import { useServerInsertedHTML } from 'next/navigation'
import createCache from '@emotion/cache'
import { CacheProvider } from '@emotion/react'

/** Records the rules Emotion inserts while the server renders and flushes them into <head>. */
export function EmotionRegistry({ children }: { children: ReactNode }) {
  const [{ cache, flush }] = useState(() => {
    const cache = createCache({ key: 'css' })
    cache.compat = true
    const prevInsert = cache.insert
    let inserted: string[] = []
    cache.insert = (...args) => {
      const serialized = args[1]
      if (cache.inserted[serialized.name] === undefined) inserted.push(serialized.name)
      return prevInsert(...args)
    }
    const flush = () => {
      const prev = inserted
      inserted = []
      return prev
    }
    return { cache, flush }
  })
  useServerInsertedHTML(() => {
    const names = flush()
    if (names.length === 0) return null
    let styles = ''
    for (const name of names) styles += cache.inserted[name]
    return <style key={cache.key} data-emotion={`${cache.key} ${names.join(' ')}`} dangerouslySetInnerHTML={{ __html: styles }} />
  })
  return <CacheProvider value={cache}>{children}</CacheProvider>
}
";

/// Words an exported name must avoid: reserved words, plus what the generated file imports.
const RESERVED: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "static",
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
    "await",
    "async",
    "arguments",
    "eval",
    "style",
    "StyleRule",
    "undefined",
    "NaN",
    "Infinity",
];

fn pascal(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w[..1].to_ascii_uppercase() + &w[1..])
        .collect()
}

fn camel(s: &str) -> String {
    let p = pascal(s);
    p[..p.len().min(1)].to_ascii_lowercase() + p.get(1..).unwrap_or_default()
}

/// A free name: `base`, or `base2`, `base3`...
fn claim(taken: &mut BTreeSet<String>, base: String) -> String {
    let mut name = base.clone();
    let mut n = 2;
    while !taken.insert(name.clone()) {
        name = format!("{base}{n}");
        n += 1;
    }
    name
}

fn usable(c: &Class) -> bool {
    !c.decls.is_empty() || !c.overrides.is_empty()
}

/// Declarations name assets from the site root; the project serves them from `/assets/`.
fn url(v: &str) -> String {
    v.replace("url(assets/", "url(/assets/")
}

/// `display-name` to `displayName`, and a vendor prefix (`-webkit-x`) to `WebkitX`.
fn camel_prop(k: &str) -> String {
    let mut parts = k.split('-');
    let mut out = parts.next().unwrap_or_default().to_owned();
    for p in parts.filter(|p| !p.is_empty()) {
        out.push_str(&p[..1].to_ascii_uppercase());
        out.push_str(&p[1..]);
    }
    out
}

fn entry(pad: &str, k: &str, v: &str) -> String {
    format!("{pad}{}: {},\n", js(k), js(&url(v)))
}

fn is_own(c: &Class, page: usize) -> bool {
    c.pages.len() == 1 && c.pages.contains(&page)
}

impl<'a> Kit<'a> {
    pub fn new(cx: &Ctx<'a>) -> Self {
        let site = cx.site;
        let mut kit = Kit {
            site,
            styling: site.options.styling,
            next: cx.next,
            classes: site
                .style
                .classes
                .iter()
                .map(|c| (c.name.as_str(), c))
                .collect(),
            idents: HashMap::new(),
            pairs: Vec::new(),
            pair_of: HashMap::new(),
            scans: Vec::new(),
            taken: site
                .pages
                .iter()
                .map(|p| p.component.clone())
                .chain(["Link".to_owned(), "Messages".to_owned(), "Props".to_owned()])
                .collect(),
        };
        if kit.styling == Styling::VanillaExtract {
            let mut taken: BTreeSet<String> = BTreeSet::new();
            for c in site.style.classes.iter().filter(|c| usable(c)) {
                let mut id = camel(&c.name);
                if id.is_empty()
                    || RESERVED.contains(&id.as_str())
                    || id.starts_with(|ch: char| ch.is_ascii_digit())
                {
                    id.insert(0, '_');
                }
                kit.idents.insert(c.name.as_str(), claim(&mut taken, id));
            }
        }
        for (i, p) in site.pages.iter().enumerate() {
            let mut scan = Scan::default();
            kit.visit(i, &p.root, &mut scan);
            kit.scans.push(scan);
        }
        kit
    }

    fn class_of(&self, name: &str) -> Option<&'a Class> {
        self.classes.get(name).copied().filter(|c| usable(c))
    }

    fn css_in_js(&self) -> bool {
        matches!(self.styling, Styling::StyledComponents | Styling::Emotion)
    }

    fn visit(&mut self, page: usize, el: &Element, scan: &mut Scan) {
        let is_link = el.link.is_some() && el.tag == "a";
        let internal = is_link && matches!(el.link, Some(LinkTarget::Page(_)));
        // Tailwind classes come from the element itself, not the class table.
        if let Some(c) = self
            .class_of(&el.class)
            .filter(|_| self.styling != Styling::Tailwind)
        {
            if self.css_in_js() {
                let base = if internal {
                    Base::Link
                } else {
                    Base::Tag(el.tag.clone())
                };
                self.pair(&c.name, base, page);
            } else if is_own(c, page) {
                scan.own = true;
            } else {
                scan.shared = true;
            }
        }
        for ch in &el.children {
            if let Child::El(e) = ch {
                self.visit(page, e, scan);
            }
        }
    }

    fn pair(&mut self, class: &str, base: Base, page: usize) -> usize {
        let key = (class.to_owned(), base.clone());
        if let Some(&i) = self.pair_of.get(&key) {
            self.pairs[i].pages.insert(page);
            return i;
        }
        let suffix = match &base {
            Base::Link => "Link".to_owned(),
            Base::Tag(t) => pascal(t),
        };
        let name = claim(&mut self.taken, pascal(class) + &suffix);
        self.pairs.push(Pair {
            name,
            class: class.to_owned(),
            base,
            pages: BTreeSet::from([page]),
        });
        self.pair_of.insert(key, self.pairs.len() - 1);
        self.pairs.len() - 1
    }

    /// The `class` attribute of an element, if its styling uses one.
    pub fn class_val(&self, page: usize, el: &Element) -> Option<Val> {
        if self.styling == Styling::Tailwind {
            return Some(el.tailwind_classes())
                .filter(|c| !c.is_empty())
                .map(Val::Lit);
        }
        let c = self.class_of(&el.class)?;
        let ns = if is_own(c, page) { "s" } else { "g" };
        match self.styling {
            Styling::Css | Styling::Scss => Some(Val::Expr(format!("{ns}[{}]", js(&c.name)))),
            Styling::VanillaExtract => {
                Some(Val::Expr(format!("{ns}.{}", self.idents[c.name.as_str()])))
            }
            _ => None,
        }
    }

    /// The styled component standing for a non-anchor element.
    pub fn tag_name(&self, el: &Element) -> Option<String> {
        let c = self.class_of(&el.class).filter(|_| self.css_in_js())?;
        let i = self
            .pair_of
            .get(&(c.name.clone(), Base::Tag(el.tag.clone())))?;
        Some(self.pairs[*i].name.clone())
    }

    /// The tag an anchor prints as: its styled component, else `Link` for a page and `a` for a URL.
    pub fn link_tag(&self, el: &Element) -> String {
        let internal = matches!(el.link, Some(LinkTarget::Page(_)));
        let base = if internal {
            Base::Link
        } else {
            Base::Tag(el.tag.clone())
        };
        self.class_of(&el.class)
            .filter(|_| self.css_in_js())
            .and_then(|c| self.pair_of.get(&(c.name.clone(), base)))
            .map_or_else(
                || if internal { "Link" } else { "a" }.to_owned(),
                |i| self.pairs[*i].name.clone(),
            )
    }

    /// `'use client'` for components that define styled components under Next.
    pub fn client(&self) -> bool {
        self.css_in_js() && self.next
    }

    /// The import lines for the page's styles (paths relative to `components/`).
    pub fn imports(&self, page: usize) -> String {
        let (scan, comp) = (&self.scans[page], &self.site.pages[page].component);
        let mut out = String::new();
        match self.styling {
            Styling::Css | Styling::Scss => {
                let ext = if self.styling == Styling::Css {
                    "css"
                } else {
                    "scss"
                };
                if scan.own {
                    out.push_str(&format!("import s from './{comp}.module.{ext}'\n"));
                }
                if scan.shared {
                    out.push_str(&format!("import g from './shared.module.{ext}'\n"));
                }
            }
            Styling::VanillaExtract => {
                if scan.own {
                    out.push_str(&format!("import * as s from './{comp}.css'\n"));
                }
                if scan.shared {
                    out.push_str("import * as g from './shared.css'\n");
                }
            }
            Styling::StyledComponents | Styling::Emotion => {
                if self
                    .pairs
                    .iter()
                    .any(|p| p.pages.len() == 1 && p.pages.contains(&page))
                {
                    out.push_str(&format!("import styled from '{}'\n", self.styled_pkg()));
                }
                let shared: Vec<&str> = self
                    .pairs
                    .iter()
                    .filter(|p| p.pages.len() > 1 && p.pages.contains(&page))
                    .map(|p| p.name.as_str())
                    .collect();
                if !shared.is_empty() {
                    out.push_str(&format!(
                        "import {{ {} }} from './shared'\n",
                        shared.join(", ")
                    ));
                }
            }
            Styling::Tailwind => {}
        }
        out
    }

    fn styled_pkg(&self) -> &'static str {
        if self.styling == Styling::Emotion {
            "@emotion/styled"
        } else {
            "styled-components"
        }
    }

    /// The styled components only this page uses, printed above the component.
    pub fn prelude(&self, page: usize) -> String {
        let own = self
            .pairs
            .iter()
            .filter(|p| p.pages.len() == 1 && p.pages.contains(&page));
        own.map(|p| self.define(p, "const"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn define(&self, p: &Pair, kw: &str) -> String {
        let Some(c) = self.classes.get(p.class.as_str()) else {
            return String::new();
        };
        let base = match &p.base {
            Base::Link => "(Link)".to_owned(),
            Base::Tag(t) => format!(".{t}"),
        };
        let mut s = format!("{kw} {} = styled{base}({{\n", p.name);
        for (k, v) in &c.decls {
            s.push_str(&entry("  ", k, v));
        }
        for (loc, decls) in &c.overrides {
            let Some(scope) = self.site.style.scopes.get(*loc) else {
                continue;
            };
            s.push_str(&format!("  {}: {{\n", js(&format!("{scope} &"))));
            for (k, v) in decls {
                s.push_str(&entry("    ", k, v));
            }
            s.push_str("  },\n");
        }
        s.push_str("})\n");
        s
    }

    fn module_css(&self, keep: impl Fn(&Class) -> bool) -> String {
        // A class name in a mode scope would be hashed like any other, so it is marked global.
        let scopes = self
            .site
            .style
            .scopes
            .iter()
            .map(|s| {
                s.strip_prefix(":root.")
                    .map_or_else(|| s.clone(), |r| format!(":root:global(.{r})"))
            })
            .collect();
        let model = StyleModel {
            tokens: Vec::new(),
            fonts: Vec::new(),
            fallbacks: Vec::new(),
            preload: None,
            favicon: None,
            h1_faces: Vec::new(),
            classes: self.site.style.classes.clone(),
            scopes,
        };
        let css = model.classes_css("/", |c| usable(c) && keep(c));
        if self.styling == Styling::Scss {
            css.replace("#{", "\\23 {")
        } else {
            css
        }
    }

    fn global_css(&self) -> String {
        let st = &self.site.style;
        if self.styling == Styling::Tailwind {
            return format!(
                "@import \"tailwindcss\";\n{}{}{}{}",
                st.tailwind_theme(),
                st.tokens_css(),
                st.fonts_css("/"),
                st.overrides_css("/", |_| true)
            );
        }
        format!("{}{}{BASE_CSS}", st.tokens_css(), st.fonts_css("/"))
    }

    /// The page's own style file, if its styling keeps one per page.
    pub fn write_page(&self, cx: &Ctx, page: usize, files: &mut Files) {
        let (p, comp) = (cx.prefix(), &self.site.pages[page].component);
        if !self.scans[page].own {
            return;
        }
        let own = |c: &Class| is_own(c, page);
        match self.styling {
            Styling::Css => put(
                files,
                &format!("{p}components/{comp}.module.css"),
                self.module_css(own),
            ),
            Styling::Scss => put(
                files,
                &format!("{p}components/{comp}.module.scss"),
                self.module_css(own),
            ),
            Styling::VanillaExtract => put(
                files,
                &format!("{p}components/{comp}.css.ts"),
                self.vanilla(own),
            ),
            _ => {}
        }
    }

    /// The styles more than one page uses.
    pub fn write_shared(&self, cx: &Ctx, files: &mut Files) {
        let p = cx.prefix();
        let shared = |c: &Class| c.pages.len() != 1;
        let any = self.scans.iter().any(|s| s.shared);
        match self.styling {
            Styling::Css if any => put(
                files,
                &format!("{p}components/shared.module.css"),
                self.module_css(shared),
            ),
            Styling::Scss if any => put(
                files,
                &format!("{p}components/shared.module.scss"),
                self.module_css(shared),
            ),
            Styling::VanillaExtract if any => put(
                files,
                &format!("{p}components/shared.css.ts"),
                self.vanilla(shared),
            ),
            Styling::StyledComponents | Styling::Emotion => self.write_shared_pairs(cx, files),
            _ => {}
        }
    }

    fn write_shared_pairs(&self, cx: &Ctx, files: &mut Files) {
        let pairs: Vec<&Pair> = self.pairs.iter().filter(|p| p.pages.len() > 1).collect();
        if pairs.is_empty() {
            return;
        }
        let mut s = String::new();
        if self.client() {
            s.push_str("'use client'\n\n");
        }
        if pairs.iter().any(|p| p.base == Base::Link) {
            s.push_str(if self.next {
                "import Link from 'next/link'\n"
            } else {
                "import { Link } from 'react-router'\n"
            });
        }
        s.push_str(&format!("import styled from '{}'\n\n", self.styled_pkg()));
        s.push_str(
            &pairs
                .iter()
                .map(|p| self.define(p, "export const"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        put(files, &format!("{}components/shared.ts", cx.prefix()), s);
    }

    fn vanilla(&self, keep: impl Fn(&Class) -> bool) -> String {
        let mut s = String::from("import { style, type StyleRule } from '@vanilla-extract/css'\n");
        for c in self
            .site
            .style
            .classes
            .iter()
            .filter(|c| usable(c) && keep(c))
        {
            s.push_str(&format!(
                "\nexport const {} = style({{\n",
                self.idents[c.name.as_str()]
            ));
            s.push_str(&ve_decls(&c.decls, "  "));
            let selectors: String = c
                .overrides
                .iter()
                .filter_map(|(loc, d)| {
                    self.site.style.scopes.get(*loc).map(|scope| {
                        format!(
                            "    {}: {{\n{}    }},\n",
                            js(&format!("{scope} &")),
                            ve_decls(d, "      ")
                        )
                    })
                })
                .collect();
            if !selectors.is_empty() {
                s.push_str(&format!("  selectors: {{\n{selectors}  }},\n"));
            }
            s.push_str("} as StyleRule)\n");
        }
        s
    }

    /// The import lines a layout (Next) or `main.tsx` (Vite) needs for the global styles.
    pub fn global_imports(&self) -> String {
        let at = if self.next { "@/" } else { "./" };
        match self.styling {
            Styling::StyledComponents | Styling::Emotion => {
                let reg = match (self.next, self.styling) {
                    (true, Styling::Emotion) => {
                        format!("import {{ EmotionRegistry }} from '{at}lib/registry'\n")
                    }
                    (true, _) => format!("import {{ StyledRegistry }} from '{at}lib/registry'\n"),
                    _ => String::new(),
                };
                format!("{reg}import {{ GlobalStyle }} from '{at}styles/global'\n")
            }
            Styling::Scss => format!("import '{at}styles/global.scss'\n"),
            _ => format!("import '{at}styles/global.css'\n"),
        }
    }

    /// `inner` (a JSX expression) with the style providers around it.
    pub fn wrap(&self, inner: &str) -> String {
        if !self.css_in_js() {
            return inner.to_owned();
        }
        if !self.next {
            return format!("<><GlobalStyle />{inner}</>");
        }
        let reg = if self.styling == Styling::Emotion {
            "EmotionRegistry"
        } else {
            "StyledRegistry"
        };
        format!("<{reg}><GlobalStyle />{inner}</{reg}>")
    }

    pub fn write_global(&self, cx: &Ctx, files: &mut Files) {
        let p = cx.prefix();
        let css = self.global_css();
        match self.styling {
            Styling::Scss => put(
                files,
                &format!("{p}styles/global.scss"),
                css.replace("#{", "\\23 {"),
            ),
            Styling::StyledComponents | Styling::Emotion => {
                let head = if self.client() {
                    "'use client'\n\n"
                } else {
                    ""
                };
                let emotion = self.styling == Styling::Emotion;
                let body = if emotion {
                    format!(
                        "{head}import {{ Global }} from '@emotion/react'\n\nconst CSS = {}\n\nexport const GlobalStyle = () => <Global styles={{CSS}} />\n",
                        js(&css)
                    )
                } else {
                    format!(
                        "{head}import {{ createGlobalStyle }} from 'styled-components'\n\nconst CSS = {}\n\nexport const GlobalStyle = createGlobalStyle`${{CSS}}`\n",
                        js(&css)
                    )
                };
                put(
                    files,
                    &format!("{p}styles/global.{}", if emotion { "tsx" } else { "ts" }),
                    body,
                );
                if self.next {
                    put(
                        files,
                        "lib/registry.tsx",
                        if emotion {
                            REGISTRY_EMOTION
                        } else {
                            REGISTRY_STYLED
                        },
                    );
                }
            }
            _ => put(files, &format!("{p}styles/global.css"), css),
        }
    }

    /// Package dependencies and dev dependencies this styling adds.
    pub fn deps(&self) -> (Deps, Deps) {
        let next = self.next;
        match self.styling {
            Styling::Css => (vec![], vec![]),
            Styling::Scss => (vec![], vec![("sass", "^1.107.1")]),
            Styling::Tailwind => (
                vec![],
                vec![
                    if next {
                        ("@tailwindcss/postcss", "^4.3.4")
                    } else {
                        ("@tailwindcss/vite", "^4.3.4")
                    },
                    ("tailwindcss", "^4.3.4"),
                ],
            ),
            Styling::VanillaExtract if next => (
                vec![
                    ("@vanilla-extract/css", "^1.21.3"),
                    ("@vanilla-extract/next-plugin", "^2.5.2"),
                ],
                vec![],
            ),
            Styling::VanillaExtract => (
                vec![("@vanilla-extract/css", "^1.21.3")],
                vec![("@vanilla-extract/vite-plugin", "^5.2.6")],
            ),
            Styling::StyledComponents => (vec![("styled-components", "^6.6.0")], vec![]),
            Styling::Emotion if next => (
                vec![
                    ("@emotion/cache", "^11.14.0"),
                    ("@emotion/react", "^11.14.0"),
                    ("@emotion/styled", "^11.14.1"),
                ],
                vec![],
            ),
            Styling::Emotion => (
                vec![
                    ("@emotion/react", "^11.14.0"),
                    ("@emotion/styled", "^11.14.1"),
                ],
                vec![],
            ),
        }
    }
}

/// The plain properties of a vanilla-extract rule, then its custom properties under `vars`.
fn ve_decls(decls: &[(String, String)], pad: &str) -> String {
    let mut out = String::new();
    for (k, v) in decls.iter().filter(|(k, _)| !k.starts_with("--")) {
        out.push_str(&entry(pad, &camel_prop(k), v));
    }
    let vars: String = decls
        .iter()
        .filter(|(k, _)| k.starts_with("--"))
        .map(|(k, v)| entry(&format!("{pad}  "), k, v))
        .collect();
    if !vars.is_empty() {
        out.push_str(&format!("{pad}vars: {{\n{vars}{pad}}},\n"));
    }
    out
}
