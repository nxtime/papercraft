//! One component per page frame.

use super::styles::Kit;
use super::{Ctx, Files, js, put};
use crate::mcp::site::markup::{self, Dialect, Hooks, Link, TextOut, Val};
use crate::mcp::site::tree::{Element, LinkTarget};
use std::cell::Cell;

/// The props a Next page component takes (the SPA reads both from the route).
#[derive(Clone, Copy, Default)]
pub struct Props {
    pub loc: bool,
    pub m: bool,
}

struct Printer<'a> {
    cx: &'a Ctx<'a>,
    kit: &'a Kit<'a>,
    page: usize,
    uses_link: Cell<bool>,
    uses_loc: Cell<bool>,
    uses_m: Cell<bool>,
}

impl Hooks for Printer<'_> {
    fn class(&self, el: &Element) -> Option<Val> {
        self.kit.class_val(self.page, el)
    }

    fn tag(&self, el: &Element) -> String {
        self.kit.tag_name(el).unwrap_or_else(|| el.tag.clone())
    }

    fn link(&self, el: &Element, target: &LinkTarget) -> Link {
        let tag = self.kit.link_tag(el);
        if tag == "Link" {
            self.uses_link.set(true);
        }
        let attrs = match target {
            LinkTarget::Url(u) => vec![("href".to_owned(), Val::Lit(u.clone()))],
            LinkTarget::Page(p) => {
                let value = if self.cx.multi {
                    self.uses_loc.set(true);
                    Val::Expr(format!("href(loc, {p})"))
                } else {
                    Val::Lit(self.cx.route(0, *p))
                };
                vec![(if self.cx.next { "href" } else { "to" }.to_owned(), value)]
            }
        };
        Link { tag, attrs }
    }

    fn asset(&self, path: &str) -> Val {
        Val::Lit(format!("/{path}"))
    }

    fn message(&self, key: &str) -> TextOut {
        if !self.cx.multi {
            return TextOut::Lit(
                self.cx
                    .site
                    .messages
                    .first()
                    .and_then(|m| m.get(key))
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        self.uses_m.set(true);
        TextOut::Raw(format!("{{m[{}]}}", js(key)))
    }
}

pub fn write(cx: &Ctx, kit: &Kit, page: usize, files: &mut Files) -> Props {
    let comp = &cx.site.pages[page].component;
    let printer = Printer {
        cx,
        kit,
        page,
        uses_link: Cell::new(false),
        uses_loc: Cell::new(false),
        uses_m: Cell::new(false),
    };
    let mut jsx = String::new();
    markup::render(
        Dialect::Jsx,
        &printer,
        &cx.site.pages[page].root,
        if cx.next { 2 } else { 3 },
        &mut jsx,
    );
    let (uses_loc, uses_m) = (printer.uses_loc.get(), printer.uses_m.get());

    let mut s = String::new();
    if kit.client() {
        s.push_str("'use client'\n\n");
    }
    if printer.uses_link.get() {
        s.push_str(if cx.next {
            "import Link from 'next/link'\n"
        } else {
            "import { Link } from 'react-router'\n"
        });
    }
    if uses_loc {
        s.push_str("import { href } from '../lib/site'\n");
    }
    if uses_m {
        s.push_str(&format!(
            "import {}\n",
            if cx.next {
                "type { Messages } from '../lib/messages'"
            } else {
                "{ catalogs } from '../lib/messages'"
            }
        ));
    }
    if !cx.next {
        s.push_str(&format!(
            "import {{ PageHead{} }} from '../lib/head'\n",
            if cx.multi { ", useLoc" } else { "" }
        ));
    }
    s.push_str(&kit.imports(page));
    s.push('\n');
    let prelude = kit.prelude(page);
    if !prelude.is_empty() {
        s.push_str(&prelude);
        s.push('\n');
    }
    if cx.next {
        s.push_str(&next_fn(comp, uses_loc, uses_m, &jsx));
    } else {
        s.push_str(&spa_fn(cx, comp, page, uses_m, &jsx));
    }
    put(files, &format!("{}components/{comp}.tsx", cx.prefix()), s);
    kit.write_page(cx, page, files);
    Props {
        loc: uses_loc,
        m: uses_m,
    }
}

fn next_fn(comp: &str, loc: bool, m: bool, jsx: &str) -> String {
    let (names, types): (Vec<&str>, Vec<&str>) =
        [(loc, "loc", "loc: number"), (m, "m", "m: Messages")]
            .iter()
            .filter(|x| x.0)
            .map(|x| (x.1, x.2))
            .unzip();
    let params = if names.is_empty() {
        String::new()
    } else {
        format!("{{ {} }}: {{ {} }}", names.join(", "), types.join("; "))
    };
    format!("export default function {comp}({params}) {{\n  return (\n{jsx}  )\n}}\n")
}

fn spa_fn(cx: &Ctx, comp: &str, page: usize, uses_m: bool, jsx: &str) -> String {
    let mut body = String::new();
    let loc = if cx.multi {
        body.push_str("  const loc = useLoc()\n");
        "loc"
    } else {
        "0"
    };
    if uses_m {
        body.push_str("  const m = catalogs[loc]\n");
    }
    format!(
        "export default function {comp}() {{\n{body}  return (\n    <>\n      <PageHead loc={{{loc}}} page={{{page}}} />\n{jsx}    </>\n  )\n}}\n"
    )
}
