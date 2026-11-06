//! The Angular writer: a standalone-component project built with `ng build`. One component per
//! page frame; with several locales the routes are `:lang`, each locale's messages are a JSON
//! catalog and `/` lists the languages. Each page's head comes from `site.json` (computed here
//! by the core's `head_tags`) and is applied by the `Seo` service, so prerendering writes it into
//! the HTML. Every TypeScript string literal and the JSON files come from `serde_json`; template
//! text goes through the core's Angular escaping.

mod pages;
mod project;
mod runtime;
#[cfg(test)]
mod tests;

use super::model::{Output, Render, Site, Styling};
use crate::mcp::site::style::{class_slug, unique};
use std::collections::BTreeSet;

/// File names that are not pages, so a page cannot take them.
const RESERVED_FILES: [&str; 1] = ["lang-index"];

/// Where a page component lives under `src/app/pages/`.
pub(super) struct PageFile {
    pub stem: String,
    pub selector: String,
}

pub fn write(site: &Site) -> Result<Output, String> {
    let multi = site.locales.iter().any(|l| l.folder.is_some());
    let files = page_files(site);
    let mut out = Output::default();
    for (path, bytes) in &site.assets {
        out.files.insert(format!("public/{path}"), bytes.clone());
    }
    project::write(site, &mut out)?;
    runtime::write(site, multi, &mut out)?;
    pages::write(site, multi, &files, &mut out)?;
    if let Some(base) = &site.base_url {
        let abs =
            |loc: usize, page: usize| format!("{base}{}", runtime::route_url(site, page, loc));
        out.files.insert(
            "public/sitemap.xml".into(),
            site.sitemap(&abs, multi.then(|| format!("{base}/")))
                .into_bytes(),
        );
    }
    out.files
        .insert("public/robots.txt".into(), site.robots().into_bytes());
    out.next_steps.push(next_step(site));
    Ok(out)
}

fn next_step(site: &Site) -> String {
    let deploy = match site.options.render {
        Render::Static => {
            "Every page is prerendered to <route>/index.html, so any static host serves it."
        }
        Render::Spa => {
            "It is a single-page app: configure the host to answer unknown paths with index.html."
        }
    };
    let styles = match site.options.styling {
        Styling::Scss => " The styles are src/styles/*.scss.",
        Styling::Tailwind => " Tailwind v4 reads the templates; the theme is in src/styles.css.",
        _ => "",
    };
    format!(
        "Run `npm install` in the output folder, then `npm start` to preview or `npm run build` to write dist/site/browser. {deploy}{styles}"
    )
}

/// File stems and selectors for the pages: slugs of the stems, unique and never a reserved name.
fn page_files(site: &Site) -> Vec<PageFile> {
    let mut taken: BTreeSet<String> = RESERVED_FILES.iter().map(|s| (*s).to_owned()).collect();
    site.pages
        .iter()
        .map(|p| {
            let stem = unique(&mut taken, class_slug(&p.stem));
            PageFile {
                selector: format!("app-{stem}"),
                stem,
            }
        })
        .collect()
}
