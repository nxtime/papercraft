use super::*;
use crate::mcp::site::model::{Framework, Options, Page, Render};
use crate::mcp::site::style::{Class, FontFace, StyleModel};
use crate::mcp::site::tree::{Child, Text};
use std::collections::BTreeMap;

const HOSTILE: &str = "{x} </script> `b` ${x} ---";

fn el(tag: &str, class: &str, children: Vec<Child>) -> Element {
    Element {
        tag: tag.into(),
        class: class.into(),
        attrs: vec![],
        link: None,
        tw: String::new(),
        has_overrides: false,
        children,
    }
}

fn locale(tag: &str, folder: Option<&str>) -> Locale {
    Locale {
        tag: Some(tag.into()),
        hreflang: true,
        folder: folder.map(Into::into),
        label: HOSTILE.into(),
        scope: String::new(),
        class: None,
    }
}

fn site(styling: Styling, multi: bool) -> Site {
    let link = Element {
        link: Some(LinkTarget::Page(1)),
        ..el("a", "", vec![Child::Text(Text::Lit(HOSTILE.into()))])
    };
    let ext = Element {
        link: Some(LinkTarget::Url("https://example.com/?a=1&b=2".into())),
        ..el("a", "", vec![])
    };
    let root = el(
        "main",
        "home",
        vec![
            Child::El(el(
                "h1",
                "title",
                vec![Child::Text(Text::Msg("home.title".into()))],
            )),
            Child::El(link),
            Child::El(ext),
        ],
    );
    let n = if multi { 2 } else { 1 };
    let page = |stem: &str, component: &str, root: Element| Page {
        name: HOSTILE.into(),
        stem: stem.into(),
        component: component.into(),
        root,
        title: vec![HOSTILE.into(); n],
        description: vec!["d".into(); n],
    };
    let locales = if multi {
        vec![locale("en", Some("en")), locale("pt-BR", Some("pt-BR"))]
    } else {
        vec![locale("en", None)]
    };
    let msg = |v: &str| BTreeMap::from([("home.title".to_owned(), v.to_owned())]);
    let style = StyleModel {
        fallbacks: Vec::new(),
        fonts: vec![FontFace {
            family: "V".into(),
            weight: 400,
            italic: false,
            path: "assets/fonts/ab.ttf".into(),
            format: "truetype",
        }],
        preload: Some(0),
        favicon: None,
        h1_faces: Vec::new(),
        classes: vec![Class {
            name: "home".into(),
            decls: vec![("color".into(), "red".into())],
            overrides: vec![],
            pages: [0].into(),
            root_of: Some(0),
        }],
        scopes: vec![String::new(); n],
        ..Default::default()
    };
    Site {
        options: Options {
            framework: Framework::Astro,
            render: Render::Static,
            styling,
        },
        site_title: HOSTILE.into(),
        base_url: Some("https://example.com".into()),
        locales,
        pages: vec![
            page("index", "HomePage", root),
            page("pricing", "PricingPage", el("main", "", vec![])),
        ],
        messages: if multi {
            vec![msg(HOSTILE), msg("Ola")]
        } else {
            vec![msg(HOSTILE)]
        },
        assets: BTreeMap::from([("assets/fonts/ab.ttf".to_owned(), vec![1, 2])]),
        style,
        warnings: vec![],
    }
}

fn text(o: &Output, path: &str) -> String {
    String::from_utf8(
        o.files
            .get(path)
            .unwrap_or_else(|| panic!("missing {path}: {:?}", o.files.keys()))
            .clone(),
    )
    .unwrap()
}

#[test]
fn multi_locale_css_files_and_snippets() {
    let o = write(&site(Styling::Css, true)).unwrap();
    for f in [
        "package.json",
        "astro.config.mjs",
        "tsconfig.json",
        "src/site.ts",
        "src/layouts/Layout.astro",
        "src/styles/global.css",
        "src/components/HomePage.astro",
        "src/components/PricingPage.astro",
        "src/pages/index.astro",
        "src/pages/[lang]/index.astro",
        "src/pages/[lang]/pricing.astro",
        "src/i18n/en.json",
        "src/i18n/pt-BR.json",
        "public/robots.txt",
        "public/sitemap.xml",
        "public/assets/fonts/ab.ttf",
    ] {
        assert!(o.files.contains_key(f), "{f}");
    }
    let c = text(&o, "src/components/HomePage.astro");
    assert!(
        c.contains("{t[\"home.title\"]}") && c.contains("const t = messages[lang];"),
        "{c}"
    );
    assert!(
        c.contains("href={\"/\" + lang + \"/pricing/\"}") && c.contains("<style>"),
        "{c}"
    );
    assert!(text(&o, "src/pages/[lang]/index.astro").contains("getStaticPaths"));
    let ts = text(&o, "src/site.ts");
    assert!(
        ts.contains("\"hreflang\": \"x-default\"")
            && ts.contains("https://example.com/pt-BR/pricing/")
            && ts.contains("\"rel\": \"preload\""),
        "{ts}"
    );
    assert!(text(&o, "astro.config.mjs").contains("site: \"https://example.com\""));
}

#[test]
fn single_locale_writes_literal_text_at_the_root() {
    let o = write(&site(Styling::Css, false)).unwrap();
    assert!(o.files.contains_key("src/pages/pricing.astro"));
    assert!(
        !o.files
            .keys()
            .any(|k| k.contains("i18n") || k.contains("[lang]"))
    );
    let c = text(&o, "src/components/HomePage.astro");
    assert!(!c.contains("t["), "{c}");
    assert!(c.contains("href=\"/pricing/\""), "{c}");
}

#[test]
fn styling_variants() {
    let scss = write(&site(Styling::Scss, true)).unwrap();
    assert!(
        scss.files.contains_key("src/styles/global.scss")
            && text(&scss, "package.json").contains("\"sass\"")
    );
    assert!(text(&scss, "src/components/HomePage.astro").contains("<style lang=\"scss\">"));
    assert!(text(&scss, "src/layouts/Layout.astro").contains("global.scss"));
    let tw = write(&site(Styling::Tailwind, true)).unwrap();
    assert!(text(&tw, "src/styles/global.css").starts_with("@import \"tailwindcss\";"));
    assert!(
        text(&tw, "astro.config.mjs").contains("tailwindcss()")
            && text(&tw, "package.json").contains("@tailwindcss/vite")
    );
    assert!(!text(&tw, "src/components/HomePage.astro").contains("<style"));
}

#[test]
fn hostile_text_stays_inert() {
    let o = write(&site(Styling::Css, true)).unwrap();
    let home = text(&o, "src/pages/index.astro");
    assert!(
        !home.contains("{x}") && !home.contains("</script>") && !home.contains("${x}"),
        "{home}"
    );
    let comp = text(&o, "src/components/HomePage.astro");
    assert!(
        !comp.contains("{x}") && !comp.contains("${x}") && !comp.contains("</script>"),
        "{comp}"
    );
    let ts = text(&o, "src/site.ts");
    assert!(ts.contains("title: \"{x} </script> `b` ${x} ---\""), "{ts}");
}

#[test]
fn message_key_with_odd_characters_never_becomes_code() {
    let mut s = site(Styling::Css, true);
    s.pages[0].root = el(
        "p",
        "",
        vec![Child::Text(Text::Msg("a\"]+alert(1)+t[\"".into()))],
    );
    s.messages[0].insert("a\"]+alert(1)+t[\"".into(), "x".into());
    let c = text(&write(&s).unwrap(), "src/components/HomePage.astro");
    assert!(
        c.contains(r#"{t["a\"]+alert(1)+t[\""]}"#),
        "the key stays one JSON string: {c}"
    );
}

#[test]
fn output_is_deterministic() {
    let a = write(&site(Styling::Scss, true)).unwrap();
    let b = write(&site(Styling::Scss, true)).unwrap();
    assert_eq!(a.files, b.files);
    assert!(a.next_steps[0].contains("npm install && npm run build"));
}

#[test]
fn unusable_names_are_refused() {
    let mut s = site(Styling::Css, true);
    s.pages[1].component = "homepage".into();
    assert!(write(&s).is_err());
    let mut s = site(Styling::Css, true);
    s.pages[1].component = "HOMEPAGE".into();
    assert!(write(&s).is_err(), "case-insensitive clash");
    let mut s = site(Styling::Css, true);
    s.locales[1].folder = Some("../x".into());
    assert!(write(&s).is_err());
}

#[test]
fn css_values_cannot_close_the_style_block() {
    let mut s = site(Styling::Css, true);
    let evil = "x</style><script>`${x}`</script>";
    s.style.classes[0]
        .decls
        .push(("font-family".into(), evil.into()));
    s.style.fonts[0].family = evil.into();
    let o = write(&s).unwrap();
    let c = text(&o, "src/components/HomePage.astro");
    assert_eq!(c.matches("</style>").count(), 1, "{c}");
}
