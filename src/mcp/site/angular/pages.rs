//! The page components, the language index and the routes. A page is a standalone component
//! whose template is the core's element tree printed in the Angular dialect.

use super::PageFile;
use crate::mcp::site::markup::{
    self, Dialect, Hooks, Link, TextOut, Val, escape_attr, escape_text,
};
use crate::mcp::site::model::{Output, Site, Styling};
use crate::mcp::site::tree::{Element, LinkTarget};

/// The language index is only a list of links; it styles itself.
const INDEX_STYLE: &str = "main { margin: 2rem; font-family: system-ui, sans-serif; }";

struct NgHooks<'a> {
    site: &'a Site,
    multi: bool,
}

/// A TypeScript or JSON string literal.
fn lit(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

impl Hooks for NgHooks<'_> {
    fn class(&self, el: &Element) -> Option<Val> {
        let class = if self.site.options.styling == Styling::Tailwind {
            el.tailwind_classes()
        } else {
            el.class.clone()
        };
        (!class.is_empty()).then_some(Val::Lit(class))
    }

    fn link(&self, _: &Element, target: &LinkTarget) -> Link {
        let attrs = match target {
            LinkTarget::Page(i) => vec![(
                "routerLink".to_owned(),
                Val::Expr(route_expr(self.site, self.multi, *i)),
            )],
            LinkTarget::Url(u) => vec![("href".to_owned(), Val::Lit(u.clone()))],
        };
        Link {
            tag: "a".into(),
            attrs,
        }
    }

    fn asset(&self, path: &str) -> Val {
        Val::Lit(format!("/{path}"))
    }

    fn message(&self, key: &str) -> TextOut {
        if self.multi {
            // Bracket access: keys hold dots and may start with a digit.
            TextOut::Raw(format!("{{{{ t()[{}] }}}}", lit(key)))
        } else {
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
}

/// The `routerLink` array for a page: `['/', lang(), "pricing"]`, or without the language.
fn route_expr(site: &Site, multi: bool, page: usize) -> String {
    let mut parts = vec!["'/'".to_owned()];
    if multi {
        parts.push("lang()".into());
    }
    if page != 0 {
        parts.push(lit(&site.pages[page].stem));
    }
    format!("[{}]", parts.join(", "))
}

pub fn write(site: &Site, multi: bool, files: &[PageFile], out: &mut Output) -> Result<(), String> {
    let hooks = NgHooks { site, multi };
    for (i, (page, file)) in site.pages.iter().zip(files).enumerate() {
        let mut html = String::new();
        markup::render(Dialect::Angular, &hooks, &page.root, 0, &mut html);
        let css = if site.options.styling == Styling::Css {
            site.style.page_css(i, "/")
        } else {
            String::new()
        };
        let has_css = !css.is_empty();
        if has_css {
            out.files
                .insert(format!("src/app/pages/{}.css", file.stem), css.into_bytes());
        }
        let ts = component(
            &page.component,
            file,
            i,
            multi,
            html.contains("[routerLink]"),
            has_css,
        );
        out.files.insert(
            format!("src/app/pages/{}.html", file.stem),
            html.into_bytes(),
        );
        out.files
            .insert(format!("src/app/pages/{}.ts", file.stem), ts.into_bytes());
    }
    routes(site, multi, files, out);
    if multi {
        language_index(site, out);
    }
    Ok(())
}

fn component(
    name: &str,
    file: &PageFile,
    page: usize,
    multi: bool,
    router_link: bool,
    has_css: bool,
) -> String {
    let stem = &file.stem;
    let style = if has_css {
        format!("  styleUrl: {},\n", lit(&format!("./{stem}.css")))
    } else {
        String::new()
    };
    let (core, hook, body) = if multi {
        (
            "Component, OnChanges, ViewEncapsulation, computed, inject, input",
            "OnChanges",
            format!(
                "  /** Bound from the :lang route parameter. */\n  readonly lang = input.required<string>();\n  protected readonly t = computed(() => messages(this.lang()));\n  private readonly seo = inject(Seo);\n\n  ngOnChanges(): void {{\n    this.seo.set({page}, this.lang());\n  }}\n"
            ),
        )
    } else {
        (
            "Component, OnInit, ViewEncapsulation, inject",
            "OnInit",
            format!(
                "  private readonly seo = inject(Seo);\n\n  ngOnInit(): void {{\n    this.seo.set({page});\n  }}\n"
            ),
        )
    };
    let router = if router_link {
        "import { RouterLink } from '@angular/router';\n"
    } else {
        ""
    };
    let i18n = if multi {
        "import { messages } from '../i18n';\n"
    } else {
        ""
    };
    let imports = if router_link {
        "  imports: [RouterLink],\n"
    } else {
        ""
    };
    format!(
        "import {{ {core} }} from '@angular/core';\n{router}{i18n}import {{ Seo }} from '../seo';\n\n@Component({{\n  selector: {},\n{imports}  templateUrl: {},\n{style}  encapsulation: ViewEncapsulation.None,\n}})\nexport class {name} implements {hook} {{\n{body}}}\n",
        lit(&file.selector),
        lit(&format!("./{stem}.html"))
    )
}

fn routes(site: &Site, multi: bool, files: &[PageFile], out: &mut Output) {
    let path = |i: usize| match (multi, i) {
        (true, 0) => ":lang".to_owned(),
        (true, _) => format!(":lang/{}", site.pages[i].stem),
        (false, 0) => String::new(),
        (false, _) => site.pages[i].stem.clone(),
    };
    let mut imports = String::new();
    let mut list = String::new();
    if multi {
        imports.push_str("import { LanguageIndex } from './pages/lang-index';\n");
        list.push_str("  { path: '', component: LanguageIndex },\n");
    }
    for (i, (page, file)) in site.pages.iter().zip(files).enumerate() {
        imports.push_str(&format!(
            "import {{ {} }} from {};\n",
            page.component,
            lit(&format!("./pages/{}", file.stem))
        ));
        list.push_str(&format!(
            "  {{ path: {}, component: {} }},\n",
            lit(&path(i)),
            page.component
        ));
    }
    out.files.insert("src/app/app.routes.ts".into(), format!("import {{ Routes }} from '@angular/router';\n{imports}\nexport const routes: Routes = [\n{list}];\n").into_bytes());
    if site.options.render != crate::mcp::site::model::Render::Static {
        return;
    }
    let mut server = String::from("import { RenderMode, ServerRoute } from '@angular/ssr';\n");
    let mut entries = String::new();
    if multi {
        server.push_str("import { LANGS } from './i18n';\n\nconst langParams = async () => LANGS.map((lang) => ({ lang }));\n");
        entries.push_str("  { path: '', renderMode: RenderMode.Prerender },\n");
    }
    for i in 0..site.pages.len() {
        let params = if multi {
            ", getPrerenderParams: langParams"
        } else {
            ""
        };
        entries.push_str(&format!(
            "  {{ path: {}, renderMode: RenderMode.Prerender{params} }},\n",
            lit(&path(i))
        ));
    }
    entries.push_str("  { path: '**', renderMode: RenderMode.Prerender },\n");
    out.files.insert(
        "src/app/app.routes.server.ts".into(),
        format!("{server}\nexport const serverRoutes: ServerRoute[] = [\n{entries}];\n")
            .into_bytes(),
    );
}

fn language_index(site: &Site, out: &mut Output) {
    let langs = site.language_locales();
    let items: String = site
        .locales
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let expr = format!("['/', {}]", lit(l.folder.as_deref().unwrap_or_default()));
            let lang = l
                .tag
                .as_ref()
                .filter(|_| langs.contains(&i))
                .map(|t| {
                    format!(
                        " lang=\"{0}\" hreflang=\"{0}\"",
                        escape_attr(Dialect::Angular, t)
                    )
                })
                .unwrap_or_default();
            format!(
                "    <li><a [routerLink]=\"{}\"{lang}>{}</a></li>\n",
                escape_attr(Dialect::Html, &expr),
                escape_text(Dialect::Angular, &l.label)
            )
        })
        .collect();
    let html = format!(
        "<main>\n  <h1>{}</h1>\n  <ul>\n{items}  </ul>\n</main>\n",
        escape_text(Dialect::Angular, &site.site_title)
    );
    let ts = format!(
        "import {{ Component, OnInit, inject }} from '@angular/core';\nimport {{ RouterLink }} from '@angular/router';\nimport {{ Seo }} from '../seo';\n\n@Component({{\n  selector: 'app-lang-index',\n  imports: [RouterLink],\n  templateUrl: './lang-index.html',\n  styles: [{}],\n}})\nexport class LanguageIndex implements OnInit {{\n  private readonly seo = inject(Seo);\n\n  ngOnInit(): void {{\n    this.seo.setIndex();\n  }}\n}}\n",
        lit(INDEX_STYLE)
    );
    out.files
        .insert("src/app/pages/lang-index.html".into(), html.into_bytes());
    out.files
        .insert("src/app/pages/lang-index.ts".into(), ts.into_bytes());
}
