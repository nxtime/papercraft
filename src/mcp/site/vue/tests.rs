use super::write;
use crate::mcp::site::model::{Framework, Locale, Options, Output, Page, Render, Site, Styling};
use crate::mcp::site::style::{Classes, FontFace, StyleModel, Token};
use crate::mcp::site::tree::{Child, Element, LinkTarget, Text};
use std::collections::BTreeMap;

const HOSTILE: &str = "{{x}}</script><style>`${x}` v-html=\"y\"";

fn el(tag: &str, class: &str, attrs: &[(&str, &str)], children: Vec<Child>) -> Element {
    Element {
        tag: tag.into(),
        class: class.into(),
        attrs: attrs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        link: None,
        tw: String::new(),
        has_overrides: false,
        children,
    }
}

fn root(extra: Vec<Child>) -> Element {
    let mut link = el("a", "", &[], vec![Child::Text(Text::Lit("Go".into()))]);
    link.link = Some(LinkTarget::Page(1));
    let mut ext = el("a", "", &[], vec![Child::Text(Text::Lit("Out".into()))]);
    ext.link = Some(LinkTarget::Url("https://x.test/?a=\"b".into()));
    let mut kids = vec![
        Child::El(el(
            "h1",
            "",
            &[],
            vec![Child::Text(Text::Msg("home.hero_title".into()))],
        )),
        Child::El(el(
            "img",
            "",
            &[("src", "assets/hero.png"), ("alt", HOSTILE)],
            vec![],
        )),
        Child::El(link),
        Child::El(ext),
        Child::El(el(
            "p",
            "",
            &[],
            vec![Child::Text(Text::Lit(HOSTILE.into()))],
        )),
    ];
    kids.extend(extra);
    el("div", "home", &[], kids)
}

fn site(render: Render, styling: Styling, multi: bool) -> Site {
    let locale = |tag: &str, folder: Option<&str>| Locale {
        tag: Some(tag.into()),
        hreflang: true,
        folder: folder.map(str::to_owned),
        label: tag.into(),
        scope: String::new(),
        class: None,
    };
    let locales = if multi {
        vec![locale("en", Some("en")), locale("pt-BR", Some("pt-BR"))]
    } else {
        vec![locale("en", None)]
    };
    let n = locales.len();
    let page = |name: &str, stem: &str, comp: &str| Page {
        name: name.into(),
        stem: stem.into(),
        component: comp.into(),
        root: root(vec![]),
        title: vec![format!("{name} | Acme"); n],
        description: vec![format!("About {name}"); n],
    };
    let mut classes = Classes::default();
    classes.get(
        "Home",
        None,
        0,
        vec![("width".into(), "100%".into())],
        vec![],
        Some(0),
    );
    let style = StyleModel {
        tokens: vec![Token {
            name: "brand".into(),
            value: "#4f46e5".into(),
            color: true,
            overrides: vec![],
        }],
        fallbacks: Vec::new(),
        fonts: vec![FontFace {
            family: "Varela Round".into(),
            weight: 400,
            italic: false,
            path: "assets/fonts/f.ttf".into(),
            format: "truetype",
        }],
        preload: Some(0),
        favicon: None,
        h1_faces: Vec::new(),
        classes: classes.list,
        scopes: vec![String::new(); n],
    };
    let messages = if multi {
        vec![
            BTreeMap::from([("home.hero_title".to_owned(), "Hello".to_owned())]),
            BTreeMap::from([("home.hero_title".to_owned(), "Ola".to_owned())]),
        ]
    } else {
        vec![BTreeMap::from([(
            "home.hero_title".to_owned(),
            "Hello".to_owned(),
        )])]
    };
    Site {
        options: Options {
            framework: Framework::Vue,
            render,
            styling,
        },
        site_title: "Acme".into(),
        base_url: Some("https://example.com".into()),
        locales,
        pages: vec![
            page("Home", "index", "Home"),
            page("Pricing", "pricing", "Pricing"),
        ],
        messages,
        assets: BTreeMap::from([
            ("assets/hero.png".to_owned(), vec![1, 2, 3]),
            ("assets/fonts/f.ttf".to_owned(), vec![4]),
        ]),
        style,
        warnings: vec![],
    }
}

fn text(o: &Output, path: &str) -> String {
    String::from_utf8(
        o.files
            .get(path)
            .unwrap_or_else(|| panic!("missing {path}: {:?}", o.files.keys().collect::<Vec<_>>()))
            .clone(),
    )
    .unwrap()
}

#[test]
fn nuxt_project_has_prerendered_routes_pages_catalogs_and_public_files() {
    let o = write(&site(Render::Static, Styling::Css, true)).unwrap();
    for f in [
        "package.json",
        "nuxt.config.ts",
        "tsconfig.json",
        "app/app.vue",
        "app/pages/index.vue",
        "app/pages/[lang]/index.vue",
        "app/pages/[lang]/pricing.vue",
        "app/i18n/index.ts",
        "app/i18n/messages/pt-BR.json",
        "app/head.ts",
        "app/composables/useLocale.ts",
        "app/composables/usePageHead.ts",
        "app/assets/css/main.css",
        "public/sitemap.xml",
        "public/robots.txt",
        "public/assets/hero.png",
    ] {
        assert!(o.files.contains_key(f), "{f}");
    }
    let cfg = text(&o, "nuxt.config.ts");
    assert!(
        cfg.contains("\"/\",\"/en/\",\"/en/pricing/\",\"/pt-BR/\",\"/pt-BR/pricing/\"")
            && cfg.contains("failOnError: true")
            && cfg.contains("payloadExtraction: false")
            && cfg.contains("appManifest: false")
            && cfg.contains("features: { noScripts: true }"),
        "{cfg}"
    );
    let page = text(&o, "app/pages/[lang]/index.vue");
    assert!(
        page.contains("{{ t[\"home.hero_title\"] }}")
            && page.contains("definePageMeta")
            && page.contains("<NuxtLink :to=\"'/' + lang + '/pricing/'\">"),
        "{page}"
    );
    assert!(
        text(&o, "app/head.ts").contains("\"hreflang\": \"x-default\"")
            && text(&o, "app/head.ts").contains("\"crossorigin\": \"anonymous\"")
    );
    assert!(text(&o, "app/i18n/messages/pt-BR.json").contains("Ola"));
    assert!(text(&o, "public/sitemap.xml").contains("https://example.com/pt-BR/pricing/"));
}

#[test]
fn single_locale_routes_sit_at_the_root_with_literal_text() {
    let o = write(&site(Render::Static, Styling::Css, false)).unwrap();
    assert!(
        o.files.contains_key("app/pages/index.vue")
            && o.files.contains_key("app/pages/pricing.vue")
    );
    assert!(
        !o.files.contains_key("app/i18n/index.ts") && !o.files.keys().any(|k| k.contains("[lang]"))
    );
    let page = text(&o, "app/pages/index.vue");
    assert!(
        page.contains("<h1>Hello</h1>")
            && page.contains("<NuxtLink to=\"/pricing/\">")
            && !page.contains("useLocale"),
        "{page}"
    );
    assert!(text(&o, "nuxt.config.ts").contains("[\"/\",\"/pricing/\"]"));
}

#[test]
fn spa_project_has_a_router_views_and_unhead() {
    let o = write(&site(Render::Spa, Styling::Css, true)).unwrap();
    for f in [
        "index.html",
        "vite.config.ts",
        "tsconfig.app.json",
        "tsconfig.node.json",
        "env.d.ts",
        "src/main.ts",
        "src/App.vue",
        "src/router.ts",
        "src/views/HomePage.vue",
        "src/views/PricingPage.vue",
        "src/views/LanguageIndex.vue",
        "src/assets/main.css",
        "src/i18n/index.ts",
    ] {
        assert!(o.files.contains_key(f), "{f}");
    }
    let r = text(&o, "src/router.ts");
    assert!(
        r.contains("path: \"pricing\", component: () => import(\"./views/PricingPage.vue\")")
            && r.contains("isLocale"),
        "{r}"
    );
    assert!(text(&o, "src/views/HomePage.vue").contains("<RouterLink :to="));
    assert!(
        text(&o, "package.json").contains("\"@unhead/vue\"")
            && text(&o, "package.json").contains("vue-tsc --build && vite build")
    );
    assert!(o.warnings.iter().any(|w| w.contains("browser")));
}

#[test]
fn styling_picks_the_stylesheet_the_dependencies_and_the_style_blocks() {
    let css = write(&site(Render::Static, Styling::Css, true)).unwrap();
    assert!(
        text(&css, "app/pages/[lang]/index.vue").contains("<style scoped>")
            && text(&css, "app/assets/css/main.css").contains("@font-face")
    );
    let scss = write(&site(Render::Spa, Styling::Scss, true)).unwrap();
    assert!(
        scss.files.contains_key("src/assets/main.scss")
            && text(&scss, "package.json").contains("\"sass\"")
            && text(&scss, "src/main.ts").contains("main.scss")
    );
    assert!(text(&scss, "src/views/HomePage.vue").contains("<style lang=\"scss\" scoped>"));
    let tw = write(&site(Render::Static, Styling::Tailwind, true)).unwrap();
    assert!(
        text(&tw, "app/assets/css/main.css").starts_with("@import \"tailwindcss\";")
            && text(&tw, "nuxt.config.ts").contains("tailwindcss()")
            && !text(&tw, "app/pages/[lang]/index.vue").contains("<style")
    );
    assert!(text(&tw, "package.json").contains("@tailwindcss/vite"));
}

#[test]
fn hostile_text_stays_inert_in_every_variant() {
    for render in [Render::Static, Render::Spa] {
        for styling in [Styling::Css, Styling::Scss, Styling::Tailwind] {
            let o = write(&site(render, styling, true)).unwrap();
            for (path, content) in &o.files {
                if !path.ends_with(".vue") {
                    continue;
                }
                let c = String::from_utf8_lossy(content);
                assert!(
                    !c.contains("{{x}}") && !c.contains("v-html=\"y\"") && !c.contains("<style>`"),
                    "{path}: {c}"
                );
                assert!(
                    c.contains("&#123;&#123;x&#125;&#125;") || !c.contains("<p>"),
                    "{path}"
                );
                assert_eq!(
                    c.matches("</script>").count(),
                    usize::from(c.contains("<script")),
                    "{path}"
                );
            }
        }
    }
}

#[test]
fn hostile_names_are_rejected_or_escaped_in_data_files() {
    let mut s = site(Render::Static, Styling::Css, true);
    s.pages[0].title = vec![HOSTILE.into(), HOSTILE.into()];
    s.site_title = "</script>".into();
    let o = write(&s).unwrap();
    for f in ["app/head.ts", "nuxt.config.ts"] {
        assert!(!text(&o, f).contains("</script>"), "{f}");
    }
    assert!(text(&o, "app/pages/index.vue").contains("&lt;/script&gt;"));
    let mut bad = site(Render::Spa, Styling::Css, true);
    bad.pages[1].component = "Home".into();
    assert!(
        write(&bad)
            .err()
            .expect("rejected")
            .contains("unique identifier")
    );
    let mut bad = site(Render::Spa, Styling::Css, true);
    bad.pages[1].component = "Pri'cing".into();
    assert!(write(&bad).is_err());
    let mut bad = site(Render::Spa, Styling::Css, true);
    bad.messages[0].insert("a\"]}}x".into(), "y".into());
    assert!(write(&bad).err().expect("rejected").contains("message key"));
    let mut bad = site(Render::Static, Styling::Css, true);
    bad.pages[1].stem = "../x".into();
    assert!(write(&bad).is_err());
}

#[test]
fn message_keys_are_read_with_bracket_access() {
    let mut s = site(Render::Static, Styling::Css, true);
    s.pages[0].root = root(vec![Child::El(el(
        "span",
        "",
        &[],
        vec![Child::Text(Text::Msg("1st.title".into()))],
    ))]);
    for m in &mut s.messages {
        m.insert("1st.title".into(), "x".into());
    }
    let page = text(&write(&s).unwrap(), "app/pages/[lang]/index.vue");
    assert!(
        page.contains("{{ t[\"1st.title\"] }}") && !page.contains("t.1st"),
        "{page}"
    );
}

#[test]
fn the_same_site_writes_the_same_bytes() {
    for render in [Render::Static, Render::Spa] {
        let a = write(&site(render, Styling::Scss, true)).unwrap();
        let b = write(&site(render, Styling::Scss, true)).unwrap();
        assert_eq!(a.files, b.files);
        assert!(!a.next_steps.is_empty() && a.next_steps[0].contains("npm install"));
    }
}

#[test]
fn css_in_a_style_block_cannot_end_it() {
    for styling in [Styling::Css, Styling::Scss] {
        let mut s = site(Render::Static, styling, true);
        let mut c = Classes::default();
        c.get(
            "Home",
            None,
            0,
            vec![("content".into(), "\"</style><script>${x}`\"".into())],
            vec![],
            Some(0),
        );
        s.style.classes = c.list;
        s.style.fonts[0].family = "</style>${x}`".into();
        s.style.tokens[0].value = "</style>${x}`".into();
        let o = write(&s).unwrap();
        let page = text(&o, "app/pages/[lang]/index.vue");
        assert_eq!(page.matches("</style>").count(), 1, "{page}");
        assert!(page.contains("<\\/style>"), "{page}");
        // The shared stylesheet is a file of its own, so a style block cannot be ended there.
        assert!(
            o.files
                .keys()
                .any(|k| k.starts_with("app/assets/css/main."))
        );
    }
}
