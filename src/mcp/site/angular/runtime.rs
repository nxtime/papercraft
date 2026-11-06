//! What the pages share at run time: the `Seo` service and its data (`site.json`, the head of
//! every page and locale as the core computed it), the message catalogs and the serializer that
//! keeps hrefs ending in `/`.

use crate::mcp::site::model::{HeadTag, Output, Site};
use crate::mcp::site::style::{class_slug, unique};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const SEO_TS: &str = r#"import { DOCUMENT, Injectable, inject } from '@angular/core';
import { Title } from '@angular/platform-browser';
import data from './site.json';

interface Tag { tag: string; attrs: [string, string][] }
interface Head { title: string; lang?: string; tags: Tag[] }
interface SiteData {
  locales: { folder: string | null; lang: string | null; class: string | null }[];
  pages: Head[][];
  index: Head | null;
}

const site = data as unknown as SiteData;

/** Writes the head of a page: title, description, canonical, hreflang, Open Graph and Twitter tags. */
@Injectable({ providedIn: 'root' })
export class Seo {
  private readonly title = inject(Title);
  private readonly doc = inject(DOCUMENT);

  /** The head of one page in the locale with this route folder (the first locale when unknown). */
  set(page: number, folder: string | null = null): void {
    const i = Math.max(0, site.locales.findIndex((l) => l.folder === folder));
    const locale = site.locales[i];
    this.apply(site.pages[page][i], locale.lang, locale.class);
  }

  /** The language index at "/". */
  setIndex(): void {
    if (site.index) this.apply(site.index, site.index.lang ?? null, null);
  }

  private apply(head: Head, lang: string | null, cls: string | null): void {
    const root = this.doc.documentElement;
    if (lang) root.setAttribute('lang', lang); else root.removeAttribute('lang');
    if (cls) root.setAttribute('class', cls); else root.removeAttribute('class');
    this.title.setTitle(head.title);
    // Tags of the previous page go first, so client navigation never duplicates them.
    this.doc.head.querySelectorAll('[data-seo]').forEach((n) => n.remove());
    for (const t of head.tags) {
      const el = this.doc.createElement(t.tag);
      for (const [k, v] of t.attrs) el.setAttribute(k, v);
      el.setAttribute('data-seo', '');
      this.doc.head.appendChild(el);
    }
  }
}
"#;

const TRAILING_SLASH_TS: &str = r#"import { DefaultUrlSerializer, UrlTree } from '@angular/router';

/** Makes routerLink hrefs and router URLs end with "/" (directory style). */
export class TrailingSlashSerializer extends DefaultUrlSerializer {
  override serialize(tree: UrlTree): string {
    const url = super.serialize(tree);
    const i = url.search(/[?#]/);
    const path = i < 0 ? url : url.slice(0, i);
    return (path.endsWith('/') ? path : path + '/') + (i < 0 ? '' : url.slice(i));
  }
}
"#;

pub fn write(site: &Site, multi: bool, out: &mut Output) -> Result<(), String> {
    out.files.insert("src/app/seo.ts".into(), SEO_TS.into());
    out.files
        .insert("src/app/trailing-slash.ts".into(), TRAILING_SLASH_TS.into());
    let data = site_json(site, multi);
    out.files.insert("src/app/site.json".into(), pretty(&data)?);
    if multi {
        catalogs(site, out)?;
    }
    Ok(())
}

fn pretty(v: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(v)
        .map(|mut b| {
            b.push(b'\n');
            b
        })
        .map_err(|e| e.to_string())
}

/// The address of a page under the site: its route with a trailing slash.
pub fn route_url(site: &Site, page: usize, loc: usize) -> String {
    let route = site.url_path(page, loc);
    if route.ends_with('/') {
        route
    } else {
        format!("{route}/")
    }
}

fn absolute(site: &Site, route: &str) -> String {
    format!("{}{route}", site.base_url.as_deref().unwrap_or_default())
}

fn tag_json(t: &HeadTag) -> Value {
    json!({"tag": t.tag, "attrs": t.attrs.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>()})
}

fn site_json(site: &Site, multi: bool) -> Value {
    let url = |_from: usize, to: usize, page: usize| absolute(site, &route_url(site, page, to));
    let locales: Vec<Value> = site
        .locales
        .iter()
        .map(|l| json!({"folder": l.folder, "lang": l.tag, "class": l.class}))
        .collect();
    let pages: Vec<Value> = (0..site.pages.len())
        .map(|p| {
            let per_locale: Vec<Value> = (0..site.locales.len())
                .map(|l| json!({"title": site.pages[p].title[l], "tags": site.head_tags(p, l, &url).iter().map(tag_json).collect::<Vec<_>>()}))
                .collect();
            Value::Array(per_locale)
        })
        .collect();
    json!({"locales": locales, "pages": pages, "index": multi.then(|| index_head(site))})
}

/// The head of the language index: the alternates point at the first page of each language.
fn index_head(site: &Site) -> Value {
    let langs = site.language_locales();
    let at = |loc: usize| absolute(site, &route_url(site, 0, loc));
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
            attrs: vec![("rel", "canonical".into()), ("href", absolute(site, "/"))],
        },
    ];
    for &l in &langs {
        tags.push(HeadTag {
            tag: "link",
            attrs: vec![
                ("rel", "alternate".into()),
                ("hreflang", site.locales[l].tag.clone().unwrap_or_default()),
                ("href", at(l)),
            ],
        });
    }
    if let Some(&first) = langs.first() {
        tags.push(HeadTag {
            tag: "link",
            attrs: vec![
                ("rel", "alternate".into()),
                ("hreflang", "x-default".into()),
                ("href", at(first)),
            ],
        });
    }
    json!({"title": site.site_title, "lang": site.index_lang(), "tags": tags.iter().map(tag_json).collect::<Vec<_>>()})
}

/// `src/i18n/<locale>.json` per locale and `src/app/i18n.ts`, which maps route folders to them.
fn catalogs(site: &Site, out: &mut Output) -> Result<(), String> {
    let mut taken = BTreeSet::new();
    let mut imports = String::new();
    let mut entries = Vec::new();
    let mut folders = Vec::new();
    for (i, l) in site.locales.iter().enumerate() {
        let folder = l.folder.clone().unwrap_or_default();
        let file = unique(&mut taken, class_slug(&folder));
        out.files.insert(
            format!("src/i18n/{file}.json"),
            pretty(&json!(site.messages[i]))?,
        );
        imports.push_str(&format!("import c{i} from '../i18n/{file}.json';\n"));
        let key = serde_json::to_string(&folder).map_err(|e| e.to_string())?;
        entries.push(format!("[{key}, c{i}]"));
        folders.push(key);
    }
    let ts = format!(
        "{imports}\nexport type Messages = Record<string, string>;\n\n/** The route folder of every language and mode. */\nexport const LANGS: string[] = [{}];\n\nconst catalogs = new Map<string, Messages>([{}]);\n\n/** Unknown codes fall back to the first catalog. */\nexport const messages = (lang: string): Messages => catalogs.get(lang) ?? c0;\n",
        folders.join(", "),
        entries.join(", ")
    );
    out.files.insert("src/app/i18n.ts".into(), ts.into());
    Ok(())
}
