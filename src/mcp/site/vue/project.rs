//! Files both targets share: the catalogs, `i18n`, the head data, the composables and the
//! global stylesheet. Only the imports differ (Nuxt auto-import aliases versus packages).

use super::{Cx, global_stylesheet, js, js_pretty};
use crate::mcp::site::model::{HeadTag, Output, Site};
use serde_json::{Value, json};

/// Rejects what could not be a file name, a route segment or an identifier, before any file is
/// written. Names come from layer and page names, so this is a boundary.
pub(super) fn validate(site: &Site) -> Result<(), String> {
    let word = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    let mut seen = std::collections::BTreeSet::new();
    for p in &site.pages {
        let ident = p
            .component
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && p.component.chars().all(|c| c.is_ascii_alphanumeric());
        if !ident || !seen.insert(p.component.to_lowercase()) {
            return Err(format!(
                "page component name `{}` is not a unique identifier",
                p.component
            ));
        }
        if !word(&p.stem) {
            return Err(format!(
                "page route `{}` has characters a route segment cannot hold",
                p.stem
            ));
        }
    }
    for l in &site.locales {
        if l.folder.as_deref().is_some_and(|f| !word(f)) {
            return Err(format!(
                "locale folder `{}` has characters a route segment cannot hold",
                l.folder.as_deref().unwrap_or_default()
            ));
        }
    }
    let key_ok = |k: &str| {
        !k.is_empty()
            && k.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
    };
    if let Some(bad) = site
        .messages
        .iter()
        .flat_map(|m| m.keys())
        .find(|k| !key_ok(k))
    {
        return Err(format!(
            "message key `{bad}` has characters a catalog key cannot hold"
        ));
    }
    Ok(())
}

pub(super) fn write_shared(cx: &Cx, out: &mut Output) -> Result<(), String> {
    let src = cx.src();
    let (css_name, css) = global_stylesheet(cx.site);
    out.files.insert(
        format!(
            "{src}/assets/{}{css_name}",
            if cx.spa { "" } else { "css/" }
        ),
        css.into_bytes(),
    );
    out.files
        .insert(format!("{src}/head.ts"), head_ts(cx).into_bytes());
    out.files.insert(
        format!("{src}/composables/usePageHead.ts"),
        use_page_head(cx).into_bytes(),
    );
    if cx.multi {
        out.files.insert(
            format!("{src}/i18n/index.ts"),
            i18n_ts(cx.site).into_bytes(),
        );
        out.files.insert(
            format!("{src}/composables/useLocale.ts"),
            use_locale(cx).into_bytes(),
        );
        for (i, l) in cx.site.locales.iter().enumerate() {
            let json =
                serde_json::to_string_pretty(&cx.site.messages[i]).map_err(|e| e.to_string())?;
            out.files.insert(
                format!(
                    "{src}/i18n/messages/{}.json",
                    l.folder.as_deref().unwrap_or_default()
                ),
                format!("{json}\n").into_bytes(),
            );
        }
    }
    Ok(())
}

fn i18n_ts(site: &Site) -> String {
    let folders: Vec<&str> = site
        .locales
        .iter()
        .map(|l| l.folder.as_deref().unwrap_or_default())
        .collect();
    let mut out = String::new();
    for (i, f) in folders.iter().enumerate() {
        out.push_str(&format!(
            "import m{i} from {}\n",
            js(&format!("./messages/{f}.json"))
        ));
    }
    out.push_str(&format!("\n// The route segment of each locale, in catalog order; the first is the default.\nexport const LOCALES: string[] = {}\n", js(&folders)));
    let names: Vec<String> = (0..folders.len()).map(|i| format!("m{i}")).collect();
    out.push_str(&format!(
        "export const MESSAGES: Record<string, string>[] = [{}]\n\n",
        names.join(", ")
    ));
    out.push_str("export const isLocale = (v: unknown): boolean => typeof v === 'string' && LOCALES.includes(v)\n");
    out
}

fn use_locale(cx: &Cx) -> String {
    let import = if cx.spa {
        "import { useRoute } from 'vue-router'"
    } else {
        "import { useRoute } from '#imports'"
    };
    format!(
        "import {{ computed }} from 'vue'\n{import}\nimport {{ LOCALES, MESSAGES }} from '../i18n'\n\n\
         // The locale of the current route; an unknown `:lang` falls back to the default one.\n\
         export function useLocale() {{\n  const route = useRoute()\n  \
         const index = computed(() => Math.max(0, LOCALES.indexOf(String(route.params.lang))))\n  \
         const lang = computed(() => LOCALES[index.value] as string)\n  \
         const t = computed(() => MESSAGES[index.value] as Record<string, string>)\n  \
         return {{ index, lang, t }}\n}}\n"
    )
}

fn use_page_head(cx: &Cx) -> String {
    let import = if cx.spa {
        "import { useHead } from '@unhead/vue'"
    } else {
        "import { useHead } from '#imports'"
    };
    format!(
        "{import}\nimport type {{ HeadData }} from '../head'\n\n\
         // The head data is generated, so it is already valid; unhead's per-tag unions do not fit\n// plain string records, hence the cast.\n\
         export function usePageHead(data: () => HeadData) {{\n  \
         const input = () => {{\n    const d = data()\n    \
         return {{ title: d.title, htmlAttrs: {{ lang: d.lang, class: d.htmlClass }}, meta: d.meta, link: d.link }}\n  }}\n  \
         useHead(input as never)\n}}\n"
    )
}

/// One `{ title, lang, htmlClass, meta, link }` per page and locale, plus the language index.
fn head_ts(cx: &Cx) -> String {
    let site = cx.site;
    let url = |_from: usize, to: usize, page: usize| cx.address(page, to);
    let mut per_locale = Vec::new();
    for loc in 0..site.locales.len() {
        let l = &site.locales[loc];
        let mut pages = Vec::new();
        for page in 0..site.pages.len() {
            let mut tags = site.head_tags(page, loc, &url);
            tags.extend(site.asset_tags(page, "/"));
            let mut d = head_data(&site.pages[page].title[loc], &tags);
            if let Some(t) = &l.tag {
                d["lang"] = json!(t);
            }
            if let Some(c) = &l.class {
                d["htmlClass"] = json!(c);
            }
            pages.push(d);
        }
        per_locale.push(Value::Array(pages));
    }
    let mut out = String::from(
        "export interface HeadData {\n  title: string\n  lang?: string\n  htmlClass?: string\n  meta: Record<string, string>[]\n  link: Record<string, string>[]\n}\n\n\
         // Indexed by locale, then page.\n",
    );
    out.push_str(&format!(
        "export const HEAD: HeadData[][] = {}\n",
        js_pretty(&Value::Array(per_locale))
    ));
    if cx.multi {
        out.push_str(&format!(
            "\nexport const INDEX_HEAD: HeadData = {}\n",
            js_pretty(&index_head(cx))
        ));
    }
    out
}

fn head_data(title: &str, tags: &[HeadTag]) -> Value {
    let to_obj = |t: &HeadTag| -> Value {
        let m: serde_json::Map<String, Value> = t
            .attrs
            .iter()
            .map(|(k, v)| {
                (
                    (*k).to_owned(),
                    json!(if v.is_empty() {
                        "anonymous"
                    } else {
                        v.as_str()
                    }),
                )
            })
            .collect();
        Value::Object(m)
    };
    let of = |kind: &str| Value::Array(tags.iter().filter(|t| t.tag == kind).map(to_obj).collect());
    json!({"title": title, "meta": of("meta"), "link": of("link")})
}

/// The head of the language index at `/`: canonical and the alternates of every language.
fn index_head(cx: &Cx) -> Value {
    let site = cx.site;
    let base = site.base_url.as_deref().unwrap_or_default();
    let langs = site.language_locales();
    let alt = |tag: &str, loc: usize| HeadTag {
        tag: "link",
        attrs: vec![
            ("rel", "alternate".into()),
            ("hreflang", tag.to_owned()),
            ("href", format!("{base}{}", cx.locale_route(loc))),
        ],
    };
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
            attrs: vec![("rel", "canonical".into()), ("href", format!("{base}/"))],
        },
    ];
    tags.extend(
        langs
            .iter()
            .map(|&l| alt(site.locales[l].tag.as_deref().unwrap_or_default(), l)),
    );
    if let Some(&first) = langs.first() {
        tags.push(alt("x-default", first));
    }
    let mut d = head_data(&site.site_title, &tags);
    if let Some(t) = site.index_lang() {
        d["lang"] = json!(t);
    }
    d
}

/// The text of a `package.json`; JSON object keys come out sorted, so the file is stable.
pub(super) fn package_json(
    name: &str,
    scripts: &[(&str, &str)],
    deps: &[(&str, &str)],
    dev: &[(&str, &str)],
    engines: bool,
) -> Result<Vec<u8>, String> {
    let map = |items: &[(&str, &str)]| -> Value {
        Value::Object(
            items
                .iter()
                .map(|(k, v)| ((*k).to_owned(), json!(v)))
                .collect(),
        )
    };
    let mut pkg = json!({"name": name, "version": "0.0.0", "private": true, "type": "module", "scripts": map(scripts), "dependencies": map(deps)});
    if !dev.is_empty() {
        pkg["devDependencies"] = map(dev);
    }
    if engines {
        pkg["engines"] = json!({"node": "^22.18.0 || >=24.12.0"});
    }
    let mut b = serde_json::to_vec_pretty(&pkg).map_err(|e| e.to_string())?;
    b.push(b'\n');
    Ok(b)
}
