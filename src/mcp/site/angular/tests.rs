use super::write;
use crate::mcp::site::model::{Framework, Locale, Options, Output, Page, Render, Site, Styling};
use crate::mcp::site::style::{Class, FontFace, StyleModel, Token};
use crate::mcp::site::tree::{Child, Element, LinkTarget, Text};
use std::collections::{BTreeMap, BTreeSet};

const HOSTILE: &str = "</script>{{x}}@if (a) {b}${x}`<img onerror=alert(1)>";

fn el(tag: &str, class: &str, attrs: &[(&str, &str)], children: Vec<Child>) -> Element {
    Element {
        tag: tag.into(),
        class: class.into(),
        attrs: attrs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        link: None,
        tw: if class.is_empty() {
            String::new()
        } else {
            format!("tw-{class} flex")
        },
        has_overrides: false,
        children,
    }
}

fn text(s: &str) -> Child {
    Child::Text(Text::Lit(s.into()))
}

fn msg(k: &str) -> Child {
    Child::Text(Text::Msg(k.into()))
}

fn link(target: LinkTarget, children: Vec<Child>) -> Child {
    let mut a = el("a", "button", &[], children);
    a.link = Some(target);
    Child::El(a)
}

fn locale(tag: &str, first: bool) -> Locale {
    Locale {
        tag: Some(tag.into()),
        hreflang: true,
        folder: Some(tag.into()),
        label: format!("{tag} {HOSTILE}"),
        scope: if first {
            String::new()
        } else {
            format!(":root:lang({tag})")
        },
        class: None,
    }
}

fn class(
    name: &str,
    pages: &[usize],
    root_of: Option<usize>,
    overrides: Vec<(usize, Vec<(String, String)>)>,
) -> Class {
    Class {
        name: name.into(),
        decls: vec![
            ("display".into(), "flex".into()),
            ("background".into(), "var(--brand)".into()),
        ],
        overrides,
        media: Vec::new(),
        pages: pages.iter().copied().collect::<BTreeSet<_>>(),
        root_of,
    }
}

/// Two pages (home, pricing); `multi` adds a second locale with its own messages.
fn sample(render: Render, styling: Styling, multi: bool) -> Site {
    let locales = if multi {
        vec![locale("en", true), locale("pt-BR", false)]
    } else {
        vec![Locale {
            tag: Some("en".into()),
            hreflang: false,
            folder: None,
            label: String::new(),
            scope: String::new(),
            class: None,
        }]
    };
    let n = locales.len();
    let title = |a: &str| (0..n).map(|i| format!("{a} {i}")).collect::<Vec<_>>();
    let home = el(
        "div",
        "home",
        &[],
        vec![
            Child::El(el(
                "h1",
                "",
                &[],
                vec![if multi {
                    msg("home.hero_title")
                } else {
                    text("Hello")
                }],
            )),
            Child::El(el("p", "", &[], vec![text(HOSTILE)])),
            Child::El(el(
                "img",
                "",
                &[("src", "assets/hero.png"), ("alt", HOSTILE)],
                vec![],
            )),
            link(
                LinkTarget::Page(1),
                vec![if multi { msg("2cta") } else { text("Pricing") }],
            ),
            link(
                LinkTarget::Url("https://x.test/?a=\"><b>".into()),
                vec![text("Out")],
            ),
        ],
    );
    let pricing = el(
        "main",
        "pricing",
        &[],
        vec![Child::El(el("h1", "", &[], vec![text("Pricing")]))],
    );
    let pages = vec![
        Page {
            name: "Home".into(),
            stem: "index".into(),
            component: "HomePage".into(),
            root: home,
            title: title("Home"),
            description: title("Desc"),
        },
        Page {
            name: "Pricing".into(),
            stem: "pricing".into(),
            component: "PricingPage".into(),
            root: pricing,
            title: title("Pricing"),
            description: title("Desc"),
        },
    ];
    let messages = if multi {
        vec![
            BTreeMap::from([
                ("home.hero_title".to_owned(), "Hello".to_owned()),
                ("2cta".to_owned(), "Pricing".to_owned()),
            ]),
            BTreeMap::from([
                ("home.hero_title".to_owned(), HOSTILE.to_owned()),
                ("2cta".to_owned(), "Precos".to_owned()),
            ]),
        ]
    } else {
        vec![BTreeMap::new()]
    };
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
        classes: vec![
            class(
                "home",
                &[0],
                Some(0),
                if multi {
                    vec![(1, vec![("font-size".into(), "40px".into())])]
                } else {
                    vec![]
                },
            ),
            class("pricing", &[1], Some(1), vec![]),
            class("button", &[0, 1], None, vec![]),
        ],
        scopes: locales.iter().map(|l| l.scope.clone()).collect(),
    };
    let assets = BTreeMap::from([
        ("assets/hero.png".to_owned(), vec![1, 2, 3]),
        ("assets/fonts/f.ttf".to_owned(), vec![4, 5]),
    ]);
    Site {
        options: Options {
            framework: Framework::Angular,
            render,
            styling,
        },
        site_title: format!("Acme {HOSTILE}"),
        base_url: Some("https://x.test".into()),
        locales,
        pages,
        messages,
        assets,
        style,
        warnings: vec![],
    }
}

fn run(render: Render, styling: Styling, multi: bool) -> Output {
    write(&sample(render, styling, multi)).unwrap()
}

fn get<'a>(o: &'a Output, path: &str) -> &'a str {
    std::str::from_utf8(
        o.files.get(path).unwrap_or_else(|| {
            panic!("missing {path} in {:?}", o.files.keys().collect::<Vec<_>>())
        }),
    )
    .unwrap()
}

const VARIANTS: [(Render, Styling); 6] = [
    (Render::Static, Styling::Css),
    (Render::Static, Styling::Scss),
    (Render::Static, Styling::Tailwind),
    (Render::Spa, Styling::Css),
    (Render::Spa, Styling::Scss),
    (Render::Spa, Styling::Tailwind),
];

#[test]
fn every_variant_writes_the_project_files() {
    for (r, s) in VARIANTS {
        let o = run(r, s, true);
        for f in [
            "package.json",
            "angular.json",
            "tsconfig.json",
            "tsconfig.app.json",
            "src/index.html",
            "src/main.ts",
            "src/app/app.ts",
            "src/app/app.config.ts",
            "src/app/app.routes.ts",
            "src/app/seo.ts",
            "src/app/site.json",
            "src/app/i18n.ts",
            "src/i18n/en.json",
            "src/i18n/pt-br.json",
            "src/app/pages/index.ts",
            "src/app/pages/index.html",
            "src/app/pages/pricing.ts",
            "src/app/pages/lang-index.ts",
            "public/robots.txt",
            "public/sitemap.xml",
            "public/assets/hero.png",
            "public/assets/fonts/f.ttf",
        ] {
            assert!(o.files.contains_key(f), "{r:?} {s:?}: {f}");
        }
        let ssr = r == Render::Static;
        for f in [
            "src/main.server.ts",
            "src/app/app.config.server.ts",
            "src/app/app.routes.server.ts",
        ] {
            assert_eq!(o.files.contains_key(f), ssr, "{r:?} {f}");
        }
        assert_eq!(
            o.files.contains_key(".postcssrc.json"),
            s == Styling::Tailwind
        );
        assert_eq!(
            o.files.contains_key("src/styles/main.scss"),
            s == Styling::Scss
        );
        assert_eq!(o.files.contains_key("src/styles.css"), s != Styling::Scss);
        assert_eq!(
            o.files.contains_key("src/app/pages/index.css"),
            s == Styling::Css,
            "{r:?} {s:?}"
        );
        assert!(get(&o, "angular.json").contains(if s == Styling::Scss {
            "src/styles/main.scss"
        } else {
            "src/styles.css"
        }));
        assert!(get(&o, "package.json").contains("tailwindcss") == (s == Styling::Tailwind));
        assert_eq!(
            get(&o, "angular.json").contains("\"outputMode\": \"static\""),
            ssr
        );
        assert_eq!(
            get(&o, "src/app/app.config.ts").contains("provideClientHydration()"),
            ssr
        );
        assert!(
            o.next_steps[0].contains("npm install")
                && o.next_steps[0].contains("dist/site/browser")
        );
    }
}

#[test]
fn several_locales_route_by_language_and_read_messages_with_brackets() {
    let o = run(Render::Static, Styling::Css, true);
    let routes = get(&o, "src/app/app.routes.ts");
    assert!(
        routes.contains("path: \"\", component: LanguageIndex")
            || routes.contains("path: '', component: LanguageIndex"),
        "{routes}"
    );
    assert!(
        routes.contains("path: \":lang\", component: HomePage")
            && routes.contains("path: \":lang/pricing\", component: PricingPage"),
        "{routes}"
    );
    let server = get(&o, "src/app/app.routes.server.ts");
    assert!(
        server.contains("getPrerenderParams: langParams") && server.contains("path: '**'"),
        "{server}"
    );
    let home = get(&o, "src/app/pages/index.html");
    assert!(
        home.contains("{{ t()[\"home.hero_title\"] }}") && home.contains("{{ t()[\"2cta\"] }}"),
        "{home}"
    );
    assert!(
        home.contains("[routerLink]=\"['/', lang(), &quot;pricing&quot;]\"")
            && home.contains("src=\"/assets/hero.png\""),
        "{home}"
    );
    assert!(get(&o, "src/app/pages/index.ts").contains("this.seo.set(0, this.lang())"));
    assert!(get(&o, "src/app/i18n.ts").contains("LANGS: string[] = [\"en\", \"pt-BR\"]"));
    let site = get(&o, "src/app/site.json");
    assert!(
        site.contains("https://x.test/pt-BR/pricing/")
            && site.contains("x-default")
            && site.contains("og:locale"),
        "{site}"
    );
    let sitemap = get(&o, "public/sitemap.xml");
    assert!(
        sitemap.contains("<loc>https://x.test/</loc>")
            && sitemap.contains("<loc>https://x.test/en/pricing/</loc>"),
        "{sitemap}"
    );
    assert!(get(&o, "src/index.html").contains(
        "<link rel=\"preload\" href=\"/assets/fonts/f.ttf\" as=\"font\" type=\"font/ttf\" crossorigin>"
    ));
}

#[test]
fn one_locale_has_root_routes_and_literal_text() {
    let o = run(Render::Spa, Styling::Css, false);
    assert!(
        !o.files.contains_key("src/app/i18n.ts")
            && !o.files.contains_key("src/app/pages/lang-index.ts")
    );
    let routes = get(&o, "src/app/app.routes.ts");
    assert!(
        routes.contains("path: \"\", component: HomePage")
            && routes.contains("path: \"pricing\", component: PricingPage")
            && !routes.contains(":lang"),
        "{routes}"
    );
    let home = get(&o, "src/app/pages/index.html");
    assert!(
        home.contains(">Hello</h1>")
            && home.contains("[routerLink]=\"['/', &quot;pricing&quot;]\""),
        "{home}"
    );
    assert!(!get(&o, "src/app/pages/index.ts").contains("input."));
}

#[test]
fn styling_variants_carry_their_own_sources() {
    let tw = run(Render::Static, Styling::Tailwind, true);
    assert!(get(&tw, "src/styles.css").starts_with("@import \"tailwindcss\";\n@theme {"));
    assert!(get(&tw, "src/app/pages/index.html").contains("class=\"tw-home flex\""));
    assert_eq!(
        get(&tw, ".postcssrc.json")
            .matches("@tailwindcss/postcss")
            .count(),
        1
    );
    let scss = run(Render::Spa, Styling::Scss, true);
    assert!(get(&scss, "src/styles/main.scss").contains("@use 'pages/index';"));
    let css = run(Render::Static, Styling::Css, true);
    assert!(
        get(&css, "src/styles.css").contains("url(/assets/fonts/f.ttf)")
            && get(&css, "src/styles.css").contains(".button {")
    );
    assert!(
        get(&css, "src/app/pages/index.css").contains(".home {")
            && get(&css, "src/app/pages/index.css").contains(":lang(pt-BR) .home")
    );
    assert!(get(&css, "src/app/pages/index.ts").contains("styleUrl: \"./index.css\""));
}

#[test]
fn hostile_text_stays_inert_in_templates_and_typescript() {
    for (r, s) in VARIANTS {
        for multi in [true, false] {
            let o = run(r, s, multi);
            for (path, bytes) in &o.files {
                if !path.starts_with("src/app/")
                    || !(path.ends_with(".html") || path.ends_with(".ts"))
                {
                    continue;
                }
                let t = std::str::from_utf8(bytes).unwrap().replace("{{ '{' }}", "");
                let bindings = t.matches("{{").count();
                assert_eq!(
                    bindings,
                    t.matches("{{ t()[").count(),
                    "{path}: an interpolation that is not a message in {t}"
                );
                for bad in ["</script>", "@if", "${x}", "<img onerror", "alert(1)>"] {
                    assert!(!t.contains(bad), "{path}: `{bad}` in {t}");
                }
                if path.starts_with("src/app/") && path.ends_with(".html") {
                    assert!(!t.contains('`'), "{path}: backtick");
                }
            }
            let index = get(&o, "src/app/pages/index.html");
            assert!(
                index.contains("&lt;/script&gt;{{ '{' }}{{ '{' }}x&#125;&#125;&#64;if"),
                "{index}"
            );
        }
    }
}

#[test]
fn hostile_texts_are_data_in_json() {
    let o = run(Render::Static, Styling::Css, true);
    let catalog: serde_json::Value = serde_json::from_str(get(&o, "src/i18n/pt-br.json")).unwrap();
    assert_eq!(catalog["home.hero_title"], HOSTILE);
    let data: serde_json::Value = serde_json::from_str(get(&o, "src/app/site.json")).unwrap();
    assert!(data["index"]["title"].as_str().unwrap().contains(HOSTILE));
    let lang = get(&o, "src/app/pages/lang-index.html");
    assert!(
        !lang.contains("{{x}}") && !lang.contains("</script>") && !lang.contains('`'),
        "{lang}"
    );
    assert!(
        lang.contains("[routerLink]=\"['/', &quot;pt-BR&quot;]\""),
        "{lang}"
    );
}

#[test]
fn output_is_deterministic() {
    for (r, s) in VARIANTS {
        let (a, b) = (run(r, s, true), run(r, s, true));
        assert_eq!(a.files, b.files, "{r:?} {s:?}");
    }
}

#[test]
fn font_and_token_text_only_reaches_style_files_never_a_typescript_literal() {
    let evil = "a${x}`</style>";
    for (r, s) in VARIANTS {
        let mut site = sample(r, s, true);
        site.style.fonts[0].family = evil.into();
        site.style.tokens[0].name = "t".into();
        site.style.tokens[0].value = format!("\"{evil}\"");
        let o = write(&site).unwrap();
        for (path, bytes) in &o.files {
            let t = std::str::from_utf8(bytes).unwrap_or_default();
            if path.starts_with("src/app/") && path.ends_with(".ts") {
                for bad in ["`", "${", "</style>"] {
                    assert!(!t.contains(bad), "{path}: `{bad}` in a TypeScript file");
                }
            }
            if path.starts_with("src/app/") && path.ends_with(".html") {
                assert!(!t.contains("</style>") && !t.contains("${x}"), "{path}");
            }
        }
    }
}
