use super::write;
use crate::mcp::site::model::{Framework, Locale, Options, Page, Render, Site, Styling};
use crate::mcp::site::style::{Class, FontFace, StyleModel, Token};
use crate::mcp::site::tree::{Child, Element, LinkTarget, Text};
use std::collections::{BTreeMap, BTreeSet};

const HOSTILE: &str = "</script>{x} `${x}` \"q\" 'a' \\ <b>&amp;";

const RENDERS: [Render; 2] = [Render::Static, Render::Spa];
const STYLINGS: [Styling; 6] = [
    Styling::Css,
    Styling::Scss,
    Styling::Tailwind,
    Styling::VanillaExtract,
    Styling::StyledComponents,
    Styling::Emotion,
];

fn el(tag: &str, class: &str, children: Vec<Child>) -> Element {
    Element {
        tag: tag.into(),
        class: class.into(),
        attrs: Vec::new(),
        link: None,
        tw: "flex gap-4".into(),
        has_overrides: false,
        children,
    }
}

fn lit(s: &str) -> Child {
    Child::Text(Text::Lit(s.into()))
}

fn msg(k: &str) -> Child {
    Child::Text(Text::Msg(k.into()))
}

fn link(target: LinkTarget, class: &str, text: Child) -> Child {
    let mut a = el("a", class, vec![text]);
    a.link = Some(target);
    Child::El(a)
}

fn class(name: &str, decls: &[(&str, &str)], pages: &[usize], root_of: Option<usize>) -> Class {
    Class {
        name: name.into(),
        decls: decls
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        overrides: Vec::new(),
        media: Vec::new(),
        pages: pages.iter().copied().collect::<BTreeSet<_>>(),
        root_of,
    }
}

fn home() -> Element {
    let mut img = el("img", "pic", Vec::new());
    img.attrs = vec![
        ("src".into(), "assets/a.png".into()),
        ("alt".into(), HOSTILE.into()),
    ];
    let mut h1 = el("h1", "card-title", vec![msg("home.hero_title")]);
    h1.attrs = vec![("data-x".into(), HOSTILE.into())];
    el(
        "div",
        "home",
        vec![
            Child::El(h1),
            Child::El(el("p", "card", vec![lit(HOSTILE), msg("1st.key")])),
            link(LinkTarget::Page(1), "cta", lit("Pricing")),
            link(
                LinkTarget::Url("https://example.com/?a=1&b=2".into()),
                "cta",
                lit("Docs"),
            ),
            Child::El(img),
            Child::El(el("div", "bg", Vec::new())),
        ],
    )
}

fn pricing() -> Element {
    el(
        "div",
        "pricing",
        vec![
            Child::El(el("h2", "card", vec![lit("Prices")])),
            link(LinkTarget::Page(0), "cta", msg("1st.key")),
        ],
    )
}

/// Two locales (or one) with hostile titles, messages whose keys have dots and a leading digit, an
/// image, a font, a link to a page, an external link and an image background.
fn site(render: Render, styling: Styling, multi: bool, base: bool) -> Site {
    let locale = |tag: &str, i: usize| Locale {
        tag: Some(tag.into()),
        hreflang: multi,
        folder: multi.then(|| tag.to_owned()),
        label: if i == 0 {
            "English".into()
        } else {
            "Portugu\u{ea}s".into()
        },
        scope: if i == 0 {
            String::new()
        } else {
            format!(":root:lang({tag})")
        },
        class: None,
    };
    let locales: Vec<Locale> = if multi {
        vec![locale("en", 0), locale("pt-BR", 1)]
    } else {
        vec![locale("en", 0)]
    };
    let n = locales.len();
    let each = |s: &str| vec![s.to_owned(); n];
    let page = |name: &str, stem: &str, component: &str, root: Element| Page {
        name: name.into(),
        stem: stem.into(),
        component: component.into(),
        root,
        title: each(HOSTILE),
        description: each("Tom's \"best\" ${x} `plan`"),
    };
    let mut messages = vec![BTreeMap::from([
        ("home.hero_title".to_owned(), HOSTILE.to_owned()),
        ("1st.key".to_owned(), "One".to_owned()),
    ])];
    if multi {
        messages.push(BTreeMap::from([
            ("home.hero_title".to_owned(), "Ol\u{e1} {x}".to_owned()),
            ("1st.key".to_owned(), "Um".to_owned()),
        ]));
    }
    let style = StyleModel {
        tokens: vec![Token {
            name: "c-ink".into(),
            value: "#111111".into(),
            color: true,
            overrides: if multi {
                vec![(1, "#eeeeee".into())]
            } else {
                Vec::new()
            },
        }],
        fallbacks: Vec::new(),
        fonts: vec![FontFace {
            family: "Inter".into(),
            weight: 400,
            italic: false,
            path: "assets/fonts/abc.ttf".into(),
            format: "truetype",
        }],
        preload: Some(0),
        favicon: None,
        h1_faces: Vec::new(),
        classes: vec![
            class(
                "home",
                &[("display", "flex"), ("color", "var(--c-ink)")],
                &[0],
                Some(0),
            ),
            class("pricing", &[("display", "block")], &[1], Some(1)),
            class(
                "card-title",
                &[("font-size", "32px"), ("font-family", "Inter, sans-serif")],
                &[0],
                None,
            ),
            class("card", &[("padding", "8px")], &[0, 1], None),
            class("cta", &[("color", "red")], &[0, 1], None),
            class("pic", &[("width", "10px")], &[0], None),
            class(
                "bg",
                &[
                    ("background-image", "url(assets/a.png)"),
                    ("height", "20px"),
                ],
                &[0],
                None,
            ),
        ],
        scopes: locales.iter().map(|l| l.scope.clone()).collect(),
    };
    let mut assets = BTreeMap::new();
    assets.insert("assets/a.png".to_owned(), vec![1, 2, 3]);
    assets.insert("assets/fonts/abc.ttf".to_owned(), vec![4, 5, 6]);
    Site {
        options: Options {
            framework: Framework::React,
            render,
            styling,
        },
        site_title: "Acme </title> \"Co\"".into(),
        base_url: base.then(|| "https://example.com".to_owned()),
        locales,
        pages: vec![
            page("Home", "index", "HomePage", home()),
            page("Pricing </script>", "pricing", "PricingPage", pricing()),
        ],
        messages,
        assets,
        style,
        warnings: Vec::new(),
    }
}

fn text_files(site: &Site) -> BTreeMap<String, String> {
    let out = write(site).expect("the site is written");
    out.files
        .into_iter()
        .filter_map(|(k, v)| String::from_utf8(v).ok().map(|t| (k, t)))
        .collect()
}

fn each_variant(multi: bool, check: impl Fn(&str, Render, Styling, &BTreeMap<String, String>)) {
    for render in RENDERS {
        for styling in STYLINGS {
            let files = text_files(&site(render, styling, multi, true));
            check(&format!("{render:?}-{styling:?}"), render, styling, &files);
        }
    }
}

fn dir(render: Render) -> &'static str {
    if render == Render::Static { "" } else { "src/" }
}

#[test]
fn every_variant_writes_its_shape_and_styling_files() {
    each_variant(true, |name, render, styling, files| {
        let p = dir(render);
        let has = |path: &str| {
            assert!(
                files.contains_key(path),
                "{name}: missing {path} in {:?}",
                files.keys().collect::<Vec<_>>()
            )
        };
        for path in ["package.json", "public/robots.txt", "public/sitemap.xml"] {
            has(path);
        }
        for path in [
            "lib/site.ts",
            "lib/messages.ts",
            "messages/en.json",
            "messages/pt-BR.json",
            "components/HomePage.tsx",
            "components/PricingPage.tsx",
        ] {
            has(&format!("{p}{path}"));
        }
        if render == Render::Static {
            for path in [
                "next.config.ts",
                "tsconfig.json",
                "lib/seo.ts",
                "app/(root)/layout.tsx",
                "app/(root)/page.tsx",
                "app/[lang]/layout.tsx",
                "app/[lang]/page.tsx",
                "app/[lang]/pricing/page.tsx",
            ] {
                has(path);
            }
        } else {
            for path in [
                "index.html",
                "vite.config.ts",
                "tsconfig.app.json",
                "src/main.tsx",
                "src/router.tsx",
                "src/lib/head.tsx",
                "src/pages/LangLayout.tsx",
                "src/pages/LanguageIndex.tsx",
            ] {
                has(path);
            }
        }
        match styling {
            Styling::Css => assert!(
                files.contains_key(&format!("{p}components/HomePage.module.css"))
                    && files.contains_key(&format!("{p}styles/global.css")),
                "{name}"
            ),
            Styling::Scss => assert!(
                files.contains_key(&format!("{p}components/HomePage.module.scss"))
                    && files.contains_key(&format!("{p}styles/global.scss")),
                "{name}"
            ),
            Styling::Tailwind => assert!(
                files[&format!("{p}styles/global.css")].contains("@import \"tailwindcss\"")
                    || files[&format!("{p}styles/global.css")].contains("tailwindcss"),
                "{name}"
            ),
            Styling::VanillaExtract => assert!(
                files.contains_key(&format!("{p}components/HomePage.css.ts")),
                "{name}"
            ),
            Styling::StyledComponents => assert!(
                files[&format!("{p}components/HomePage.tsx")].contains("styled-components"),
                "{name}"
            ),
            Styling::Emotion => assert!(
                files[&format!("{p}components/HomePage.tsx")].contains("@emotion/styled"),
                "{name}"
            ),
        }
        let pkg: serde_json::Value =
            serde_json::from_str(&files["package.json"]).expect("package.json is JSON");
        let deps = pkg["dependencies"]
            .as_object()
            .expect("dependencies")
            .keys()
            .chain(
                pkg["devDependencies"]
                    .as_object()
                    .expect("devDependencies")
                    .keys(),
            )
            .cloned()
            .collect::<Vec<_>>();
        let want = match styling {
            Styling::Scss => Some("sass"),
            Styling::Tailwind => Some("tailwindcss"),
            Styling::VanillaExtract => Some("@vanilla-extract/css"),
            Styling::StyledComponents => Some("styled-components"),
            Styling::Emotion => Some("@emotion/styled"),
            Styling::Css => None,
        };
        if let Some(dep) = want {
            assert!(
                deps.iter().any(|d| d == dep),
                "{name}: {dep} missing from {deps:?}"
            );
        }
    });
}

/// Anchors in a mixed order: each must print the tag of its own element, not of whichever anchor
/// happens to come at that position in some other walk of the tree.
#[test]
fn every_anchor_prints_the_tag_of_its_own_element() {
    let url = || LinkTarget::Url("https://example.com/".into());
    let anchors = [
        link(url(), "cta", lit("a")),
        link(LinkTarget::Page(1), "cta", lit("b")),
        link(url(), "card", lit("c")),
        link(LinkTarget::Page(1), "card", lit("d")),
        link(LinkTarget::Page(1), "cta", lit("e")),
    ];
    let tags = |styling: Styling| {
        let mut s = site(Render::Static, styling, false, false);
        s.pages[0].root = el("div", "home", anchors.to_vec());
        let files = text_files(&s);
        files["components/HomePage.tsx"]
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with('<') && (l.contains(" href=") || l.contains(" to=")))
            .filter_map(|l| l[1..].split_whitespace().next().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    assert_eq!(tags(Styling::Css), ["a", "Link", "a", "Link", "Link"]);
    let styled = tags(Styling::StyledComponents);
    assert_eq!(styled.len(), 5, "{styled:?}");
    // One styled component per class and kind of target: the repeated pair shares a name and the
    // others do not.
    assert_eq!(styled[1], styled[4], "{styled:?}");
    let unique: BTreeSet<_> = styled[..4].iter().collect();
    assert_eq!(unique.len(), 4, "{styled:?}");
}

#[test]
fn hostile_text_never_reaches_code_unescaped() {
    each_variant(true, |name, _, _, files| {
        let index = files.get("index.html");
        if let Some(index) = index {
            assert!(
                index.contains("&lt;/title&gt;") && index.matches("</title>").count() == 1,
                "{name}: the title is escaped in index.html"
            );
        }
    });
}

#[test]
fn message_keys_use_bracket_access() {
    each_variant(true, |name, render, _, files| {
        let home = &files[&format!("{}components/HomePage.tsx", dir(render))];
        assert!(
            home.contains("m[\"home.hero_title\"]") && home.contains("m[\"1st.key\"]"),
            "{name}: {home}"
        );
        assert!(
            !home.contains("m.home") && !home.contains("m.1st"),
            "{name}"
        );
        let json: serde_json::Value =
            serde_json::from_str(&files[&format!("{}messages/pt-BR.json", dir(render))])
                .expect("catalog is JSON");
        assert_eq!(json["1st.key"], "Um", "{name}");
    });
}

#[test]
fn single_locale_sites_use_root_routes_and_literal_text() {
    for render in RENDERS {
        for styling in STYLINGS {
            let name = format!("{render:?}-{styling:?}");
            let files = text_files(&site(render, styling, false, false));
            let p = dir(render);
            assert!(
                !files
                    .keys()
                    .any(|k| k.contains("messages") || k.contains("[lang]")),
                "{name}: {:?}",
                files.keys().collect::<Vec<_>>()
            );
            assert!(
                !files.contains_key("public/sitemap.xml"),
                "{name}: no sitemap without base_url"
            );
            let home = &files[&format!("{p}components/HomePage.tsx")];
            assert!(
                home.contains("One") && !home.contains("catalogs"),
                "{name}: {home}"
            );
            if render == Render::Static {
                assert!(
                    files.contains_key("app/page.tsx")
                        && files.contains_key("app/pricing/page.tsx"),
                    "{name}"
                );
            } else {
                assert!(
                    files["src/router.tsx"].contains("path: \"/pricing\"")
                        && !files["src/router.tsx"].contains(":lang"),
                    "{name}"
                );
            }
        }
    }
}

#[test]
fn the_font_is_preloaded_once_per_shape() {
    each_variant(true, |name, render, _, files| {
        if render == Render::Static {
            assert!(
                files["app/[lang]/layout.tsx"].contains("ReactDOM.preload(FONT_URL"),
                "{name}"
            );
            assert!(
                !files.values().any(|t| t.contains("rel=\"preload\"")),
                "{name}: no second preload link"
            );
        } else {
            assert_eq!(
                files["index.html"].matches("rel=\"preload\"").count(),
                1,
                "{name}"
            );
        }
    });
}

#[test]
fn styled_names_are_valid_and_unique_per_file() {
    for render in RENDERS {
        for styling in [Styling::StyledComponents, Styling::Emotion] {
            let files = text_files(&site(render, styling, true, true));
            for (path, text) in files.iter().filter(|(k, _)| k.contains("components/")) {
                let names: Vec<&str> = text
                    .lines()
                    .filter_map(|l| l.strip_prefix("const ").and_then(|r| r.split(" = ").next()))
                    .collect();
                let unique: BTreeSet<&&str> = names.iter().collect();
                assert_eq!(names.len(), unique.len(), "{path}: {names:?}");
                assert!(
                    names.iter().all(
                        |n| n.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                            && n.chars().all(|c| c.is_ascii_alphanumeric())
                    ),
                    "{path}: {names:?}"
                );
            }
        }
    }
}

#[test]
fn css_modules_reach_assets_from_the_site_root() {
    let files = text_files(&site(Render::Static, Styling::Css, true, true));
    let css = &files["components/HomePage.module.css"];
    assert!(css.contains("url(/assets/a.png)"), "{css}");
}

#[test]
fn the_output_is_deterministic() {
    for render in RENDERS {
        for styling in STYLINGS {
            let a = write(&site(render, styling, true, true)).expect("first");
            let b = write(&site(render, styling, true, true)).expect("second");
            assert_eq!(a.files, b.files, "{render:?}-{styling:?}");
            assert_eq!(a.next_steps, b.next_steps);
        }
    }
}

#[test]
fn next_without_a_base_url_warns_and_the_sitemap_needs_one() {
    let out = write(&site(Render::Static, Styling::Css, true, false)).expect("written");
    assert!(
        out.warnings.iter().any(|w| w.contains("base_url")),
        "{:?}",
        out.warnings
    );
    assert!(!out.files.contains_key("public/sitemap.xml"));
    let with = text_files(&site(Render::Static, Styling::Css, true, true));
    assert!(
        with["public/sitemap.xml"].contains("https://example.com/pt-BR/pricing/"),
        "{}",
        with["public/sitemap.xml"]
    );
}

#[test]
fn next_links_the_icon_and_preloads_each_pages_h1_face_with_its_own_type() {
    let mut s = site(Render::Static, Styling::Css, true, true);
    let face = |path: &str, weight| FontFace {
        family: "Inter".into(),
        weight,
        italic: false,
        path: path.into(),
        format: "woff2",
    };
    s.style.fonts = vec![
        face("assets/fonts/a.woff2", 400),
        face("assets/fonts/b.woff2", 700),
    ];
    s.style.h1_faces = vec![Some(1), Some(0)];
    let files = text_files(&s);
    let site_ts = &files["lib/site.ts"];
    assert!(site_ts.contains(r#"FONT_TYPE = "font/woff2""#), "{site_ts}");
    assert!(site_ts.contains(r#"ICON = "data:,""#), "{site_ts}");
    assert_eq!(files["lib/seo.ts"].matches("icons: ICON").count(), 2);
    let home = &files["app/[lang]/page.tsx"];
    let bold = r#"ReactDOM.preload("/assets/fonts/b.woff2", { as: 'font', type: "font/woff2""#;
    assert!(home.contains(bold), "{home}");
    let pricing = &files["app/[lang]/pricing/page.tsx"];
    assert!(
        !pricing.contains("ReactDOM"),
        "the layout preloads that face: {pricing}"
    );
    s.style.favicon = Some("assets/f.svg".into());
    assert!(text_files(&s)["lib/site.ts"].contains(r#"ICON = "/assets/f.svg""#));
}

#[test]
fn next_flattens_prefetch_files_after_the_build() {
    let files = text_files(&site(Render::Static, Styling::Css, true, true));
    let build = &files["package.json"];
    assert!(
        build.contains("next build && node scripts/flatten-prefetch.mjs"),
        "{build}"
    );
    assert!(files["scripts/flatten-prefetch.mjs"].contains("__next."));
    let vite = text_files(&site(Render::Spa, Styling::Css, true, true));
    assert!(
        !vite.contains_key("scripts/flatten-prefetch.mjs")
            && !vite["package.json"].contains("flatten")
    );
}

#[test]
fn css_in_js_keeps_template_syntax_in_css_text_inert() {
    const NASTY: &str = r"A${x}`b\c";
    for render in RENDERS {
        for styling in [Styling::StyledComponents, Styling::Emotion] {
            let mut s = site(render, styling, true, true);
            s.style.fonts[0].family = NASTY.into();
            s.style.tokens[0].value = NASTY.into();
            s.style.classes[2].decls[1].1 = NASTY.into();
            let files = text_files(&s);
            let p = dir(render);
            let global = files
                .iter()
                .find(|(k, _)| k.starts_with(&format!("{p}styles/global.")))
                .expect("global styles")
                .1;
            // The only template literal is the styled-components `createGlobalStyle` tag, which holds `${CSS}` alone;
            // the CSS itself is a double-quoted JSON string, where a backtick or `${` is plain text.
            let templates: usize = global
                .lines()
                .filter(|l| !l.starts_with("const CSS = "))
                .map(|l| l.matches('`').count())
                .sum();
            assert_eq!(
                templates,
                if styling == Styling::StyledComponents {
                    2
                } else {
                    0
                },
                "{render:?}-{styling:?}: {global}"
            );
            assert!(
                global.contains("A${x}"),
                "the text is kept, inside a JSON string: {global}"
            );
            let home = &files[&format!("{p}components/HomePage.tsx")];
            // Page text may hold backticks (inert in JSX), but no style definition is a template literal.
            assert!(
                home.lines()
                    .filter(|l| l.starts_with("const "))
                    .all(|l| !l.contains('`')),
                "{render:?}-{styling:?}: {home}"
            );
        }
    }
}
