//! The React writer: a Next.js 16 static export or a Vite + React Router single-page app, in any
//! of the six stylings. One component per page frame; with several locales the routes are
//! `/<folder>/..`, one JSON catalog per locale and a language index at `/`.
//!
//! Every string that reaches a TypeScript file or a config goes through [`js`] (a JSON string
//! literal) and every piece of page text through the core's JSX escaping.

mod component;
mod files;
mod next;
mod styles;
#[cfg(test)]
mod tests;
mod vite;

use super::model::{Output, Render, Site, Styling};
use std::collections::BTreeMap;

pub type Files = BTreeMap<String, Vec<u8>>;

/// What the generators share: the site and which of the two project shapes is being written.
pub struct Ctx<'a> {
    pub site: &'a Site,
    pub next: bool,
    /// Every locale has a folder: routes are `/<folder>/..` and text comes from catalogs.
    pub multi: bool,
}

impl<'a> Ctx<'a> {
    fn new(site: &'a Site) -> Self {
        Ctx {
            site,
            next: site.options.render == Render::Static,
            multi: site.locales.iter().any(|l| l.folder.is_some()),
        }
    }

    pub fn styling(&self) -> Styling {
        self.site.options.styling
    }

    /// Where the sources live: the project root for Next, `src/` for Vite.
    pub fn prefix(&self) -> &'static str {
        if self.next { "" } else { "src/" }
    }

    /// The address of a page, always with a trailing slash.
    pub fn route(&self, loc: usize, page: usize) -> String {
        let dir = self.site.locales[loc]
            .folder
            .as_deref()
            .map(|d| format!("{d}/"))
            .unwrap_or_default();
        let stem = if page == 0 {
            String::new()
        } else {
            format!("{}/", self.site.pages[page].stem)
        };
        format!("/{dir}{stem}")
    }
}

/// A JavaScript string literal: JSON, which is valid JavaScript and escapes quotes, backslashes,
/// control characters and (as ` `) the line separators.
pub fn js(s: &str) -> String {
    serde_json::Value::String(s.to_owned()).to_string()
}

pub fn put(files: &mut Files, path: &str, text: impl Into<String>) {
    files.insert(path.to_owned(), text.into().into_bytes());
}

pub fn write(site: &Site) -> Result<Output, String> {
    let cx = Ctx::new(site);
    let mut out = Output::default();
    let mut files = Files::new();
    let kit = styles::Kit::new(&cx);
    files::shared(&cx, &kit, &mut files)?;
    kit.write_global(&cx, &mut files);
    let props: Vec<component::Props> = (0..site.pages.len())
        .map(|page| component::write(&cx, &kit, page, &mut files))
        .collect();
    kit.write_shared(&cx, &mut files);
    if cx.next {
        next::write(&cx, &kit, &props, &mut files, &mut out);
    } else {
        vite::write(&cx, &kit, &mut files, &mut out);
    }
    out.files = files;
    Ok(out)
}
