//! The single-file components: one per page frame and the language index.

use super::{Cx, js, sass_safe};
use crate::mcp::site::markup::{
    self, Dialect, Hooks, Link, TextOut, Val, escape_attr, escape_text,
};
use crate::mcp::site::model::{Site, Styling};
use crate::mcp::site::tree::{Element, LinkTarget};
use std::cell::Cell;

struct VueHooks<'a> {
    cx: &'a Cx<'a>,
    uses_lang: Cell<bool>,
    uses_t: Cell<bool>,
}

impl Hooks for VueHooks<'_> {
    fn class(&self, el: &Element) -> Option<Val> {
        let class = if self.cx.site.options.styling == Styling::Tailwind {
            el.tailwind_classes()
        } else {
            el.class.clone()
        };
        (!class.is_empty()).then_some(Val::Lit(class))
    }

    fn link(&self, _: &Element, target: &LinkTarget) -> Link {
        let site = self.cx.site;
        match target {
            LinkTarget::Url(u) => Link {
                tag: "a".into(),
                attrs: vec![("href".into(), Val::Lit(u.clone()))],
            },
            LinkTarget::Page(i) => {
                let tag = if self.cx.spa {
                    "RouterLink"
                } else {
                    "NuxtLink"
                }
                .to_owned();
                let to = if self.cx.multi {
                    self.uses_lang.set(true);
                    // The route is `/<lang>/<stem>/`, with the current language filled in at run time.
                    let rest = if *i == 0 {
                        "/".to_owned()
                    } else {
                        format!("/{}/", site.pages[*i].stem)
                    };
                    // Single quotes: the stem is validated to `[A-Za-z0-9_-]`, and vue-tsc does not decode `&quot;` in bindings.
                    Val::Expr(format!("'/' + lang + '{rest}'"))
                } else {
                    Val::Lit(self.cx.route(*i, 0))
                };
                Link {
                    tag,
                    attrs: vec![("to".into(), to)],
                }
            }
        }
    }

    fn asset(&self, path: &str) -> Val {
        Val::Lit(format!("/{path}"))
    }

    fn message(&self, key: &str) -> TextOut {
        if self.cx.multi {
            self.uses_t.set(true);
            // Bracket access with a JSON key: a key such as `1.title` is not a property name.
            TextOut::Raw(format!("{{{{ t[{}] }}}}", js(key)))
        } else {
            TextOut::Lit(
                self.cx
                    .site
                    .messages
                    .first()
                    .and_then(|m| m.get(key))
                    .cloned()
                    .unwrap_or_default(),
            )
        }
    }
}

/// Where a page's file lives, relative to the project root.
pub(super) fn page_path(cx: &Cx, page: usize) -> String {
    let p = &cx.site.pages[page];
    if cx.spa {
        return format!("src/views/{}Page.vue", p.component);
    }
    let name = if page == 0 { "index" } else { p.stem.as_str() };
    if cx.multi {
        format!("app/pages/[lang]/{name}.vue")
    } else {
        format!("app/pages/{name}.vue")
    }
}

/// What a file in the pages folder puts before a path under the source folder.
fn rel(cx: &Cx) -> &'static str {
    if !cx.spa && cx.multi { "../../" } else { "../" }
}

pub(super) fn page_sfc(cx: &Cx, page: usize) -> String {
    let site = cx.site;
    let hooks = VueHooks {
        cx,
        uses_lang: Cell::new(false),
        uses_t: Cell::new(false),
    };
    let mut body = String::new();
    markup::render(Dialect::Vue, &hooks, &site.pages[page].root, 1, &mut body);
    let r = rel(cx);
    let mut script = format!(
        "import {{ HEAD }} from '{r}head'\nimport {{ usePageHead }} from '{r}composables/usePageHead'\n"
    );
    if cx.multi {
        script.push_str(&format!(
            "import {{ useLocale }} from '{r}composables/useLocale'\n"
        ));
        if !cx.spa {
            script.push_str(&format!("import {{ isLocale }} from '{r}i18n'\n\n// An unknown `/xx/` is a 404, not a page in the default language.\ndefinePageMeta({{ validate: (route) => isLocale(route.params.lang) }})\n"));
        }
        let mut names = vec!["index"];
        if hooks.uses_lang.get() {
            names.push("lang");
        }
        if hooks.uses_t.get() {
            names.push("t");
        }
        script.push_str(&format!(
            "\nconst {{ {} }} = useLocale()\nusePageHead(() => HEAD[index.value]![{page}]!)\n",
            names.join(", ")
        ));
    } else {
        script.push_str(&format!("\nusePageHead(() => HEAD[0]![{page}]!)\n"));
    }
    let style = style_block(site, &site.style.page_css(page, "/"));
    format!(
        "<script setup lang=\"ts\">\n{script}</script>\n\n<template>\n{body}</template>\n{style}"
    )
}

/// A `<style scoped>` block for the page's own rules; Tailwind pages carry utilities instead.
fn style_block(site: &Site, css: &str) -> String {
    if css.is_empty() || site.options.styling == Styling::Tailwind {
        return String::new();
    }
    let (lang, css) = if site.options.styling == Styling::Scss {
        (" lang=\"scss\"", sass_safe(css.to_owned()))
    } else {
        ("", css.to_owned())
    };
    // A value cannot end the block: `</` becomes `<\/`, which CSS reads the same in a string.
    format!(
        "\n<style{lang} scoped>\n{}</style>\n",
        css.replace("</", "<\\/")
    )
}

/// `/`: one link per version of the site.
pub(super) fn index_sfc(cx: &Cx) -> String {
    let site = cx.site;
    let tag = if cx.spa { "RouterLink" } else { "NuxtLink" };
    let langs = site.language_locales();
    let items: String = site
        .locales
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let lang = l
                .tag
                .as_ref()
                .filter(|_| langs.contains(&i))
                .map(|t| {
                    format!(
                        " lang=\"{0}\" hreflang=\"{0}\"",
                        escape_attr(Dialect::Vue, t)
                    )
                })
                .unwrap_or_default();
            format!(
                "      <li><{tag} to=\"{}\"{lang}>{}</{tag}></li>\n",
                escape_attr(Dialect::Vue, &cx.locale_route(i)),
                escape_text(Dialect::Vue, &l.label)
            )
        })
        .collect();
    let imports = "import { INDEX_HEAD } from '../head'\nimport { usePageHead } from '../composables/usePageHead'\n";
    format!(
        "<script setup lang=\"ts\">\n{imports}\nusePageHead(() => INDEX_HEAD)\n</script>\n\n<template>\n  <main>\n    <h1>{}</h1>\n    <ul>\n{items}    </ul>\n  </main>\n</template>\n\n<style scoped>\nmain {{\n  font-family: system-ui, sans-serif;\n  margin: 2rem;\n}}\n</style>\n",
        escape_text(Dialect::Vue, &site.site_title)
    )
}
