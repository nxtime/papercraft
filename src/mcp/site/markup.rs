//! Prints an element tree as HTML, JSX, a Vue or Angular template, or Astro. Every piece of
//! user text (layer names, copy, URLs, font names) is escaped for the dialect it lands in; the
//! hooks only choose names and expressions, never raw user text.

use super::tree::{Child, Element, LinkTarget, Text};
use crate::mcp::codegen::{Escape, escape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Html,
    Jsx,
    Vue,
    Angular,
    Astro,
}

/// An attribute value: a literal (escaped here) or an expression written by a hook.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Lit(String),
    Expr(String),
}

/// Text content: a literal (escaped here) or a snippet that is already valid in the dialect
/// (`{t.key}`, `{{ t.key }}`).
#[derive(Debug, Clone, PartialEq)]
pub enum TextOut {
    Lit(String),
    Raw(String),
}

/// How a link is written: its tag (`a`, `Link`, `NuxtLink`...) and attributes.
pub struct Link {
    pub tag: String,
    pub attrs: Vec<(String, Val)>,
}

/// What differs between frameworks. Defaults write plain HTML.
pub trait Hooks {
    /// The `class` attribute of `el`, if it has one.
    fn class(&self, el: &Element) -> Option<Val> {
        (!el.class.is_empty()).then(|| Val::Lit(el.class.clone()))
    }

    fn tag(&self, el: &Element) -> String {
        el.tag.clone()
    }

    /// The anchor for a link, with a locale-aware `href`. `el` is the anchor element itself.
    fn link(&self, el: &Element, target: &LinkTarget) -> Link;

    /// A site-root relative asset path (`assets/..`) as the page must write it.
    fn asset(&self, path: &str) -> Val {
        Val::Lit(path.to_owned())
    }

    /// The text for a message key.
    fn message(&self, key: &str) -> TextOut;
}

pub fn render(d: Dialect, h: &dyn Hooks, el: &Element, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let (tag, mut attrs) = match (&el.link, el.tag.as_str()) {
        (Some(target), "a") => {
            let l = h.link(el, target);
            (l.tag, l.attrs)
        }
        _ => (h.tag(el), Vec::new()),
    };
    let mut all: Vec<(String, Val)> = h
        .class(el)
        .map(|c| ("class".to_owned(), c))
        .into_iter()
        .collect();
    all.append(&mut attrs);
    for (k, v) in &el.attrs {
        let val = if k == "src" {
            h.asset(v)
        } else {
            Val::Lit(v.clone())
        };
        all.push((k.clone(), val));
    }
    let mut open = format!("{pad}<{tag}");
    for (k, v) in &all {
        open.push(' ');
        open.push_str(&attribute(d, k, v));
    }
    if is_void(&el.tag) && el.children.is_empty() {
        out.push_str(&open);
        out.push_str(if d == Dialect::Html { ">\n" } else { " />\n" });
        return;
    }
    open.push('>');
    if let [Child::Text(t)] = el.children.as_slice() {
        out.push_str(&format!("{open}{}</{tag}>\n", text(d, h, t)));
        return;
    }
    out.push_str(&open);
    out.push('\n');
    for c in &el.children {
        match c {
            Child::El(e) => render(d, h, e, depth + 1, out),
            Child::Text(t) => out.push_str(&format!("{pad}  {}\n", text(d, h, t))),
        }
    }
    out.push_str(&format!("{pad}</{tag}>\n"));
}

fn is_void(tag: &str) -> bool {
    matches!(tag, "img" | "br" | "hr" | "input" | "meta" | "link")
}

fn text(d: Dialect, h: &dyn Hooks, t: &Text) -> String {
    let out = match t {
        Text::Lit(s) => TextOut::Lit(s.clone()),
        Text::Msg(key) => h.message(key),
    };
    match out {
        TextOut::Raw(s) => s,
        TextOut::Lit(s) => escape_text(d, &s),
    }
}

/// Text between tags.
pub fn escape_text(d: Dialect, s: &str) -> String {
    match d {
        // A string expression: JSX would parse braces and entities in bare text.
        Dialect::Jsx => escape(Escape::Jsx, s),
        _ => entities(d, s),
    }
}

/// A double-quoted attribute value.
pub fn escape_attr(d: Dialect, s: &str) -> String {
    entities(d, s)
}

/// Numeric and named entities for what each template language would act on: HTML's own
/// characters everywhere, braces for Vue, Angular and Astro, and Angular's `@`, plus the
/// characters that would end an inline TypeScript template literal.
fn entities(d: Dialect, s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Angular decodes entities before it looks for `{{`, so a literal brace must be an interpolation of a string.
            '{' if d == Dialect::Angular => out.push_str("{{ '{' }}"),
            '{' | '}' if matches!(d, Dialect::Vue | Dialect::Angular | Dialect::Astro) => {
                out.push_str(if c == '{' { "&#123;" } else { "&#125;" })
            }
            '@' if d == Dialect::Angular => out.push_str("&#64;"),
            '\\' if d == Dialect::Angular => out.push_str("&#92;"),
            '`' if d == Dialect::Angular => out.push_str("&#96;"),
            '$' if d == Dialect::Angular => out.push_str("&#36;"),
            c => out.push(c),
        }
    }
    out
}

fn attribute(d: Dialect, name: &str, v: &Val) -> String {
    let name = attribute_name(d, name);
    match (v, d) {
        (Val::Lit(s), _) => format!("{name}=\"{}\"", escape_attr(d, s)),
        (Val::Expr(e), Dialect::Jsx | Dialect::Astro) => format!("{name}={{{e}}}"),
        (Val::Expr(e), Dialect::Vue) => format!(":{name}=\"{}\"", entities(Dialect::Html, e)),
        (Val::Expr(e), Dialect::Angular) => format!("[{name}]=\"{}\"", entities(Dialect::Html, e)),
        // Plain HTML has no expressions; show the text rather than run it.
        (Val::Expr(e), Dialect::Html) => format!("{name}=\"{}\"", entities(Dialect::Html, e)),
    }
}

fn attribute_name(d: Dialect, name: &str) -> String {
    if d != Dialect::Jsx {
        return name.to_owned();
    }
    match name {
        "class" => "className",
        "fetchpriority" => "fetchPriority",
        "tabindex" => "tabIndex",
        "for" => "htmlFor",
        "srcset" => "srcSet",
        other => other,
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plain;
    impl Hooks for Plain {
        fn link(&self, _: &Element, t: &LinkTarget) -> Link {
            let href = match t {
                LinkTarget::Page(i) => format!("p{i}.html"),
                LinkTarget::Url(u) => u.clone(),
            };
            Link {
                tag: "a".into(),
                attrs: vec![("href".into(), Val::Lit(href))],
            }
        }

        fn message(&self, key: &str) -> TextOut {
            TextOut::Raw(format!("{{t.{key}}}"))
        }
    }

    const HOSTILE: &str = "</script>{{x}}@if (a) {b}${x}`<img onerror=alert(1)>";

    fn page(d: Dialect) -> String {
        let mut out = String::new();
        let mut root = Element {
            tag: "div".into(),
            class: "root".into(),
            attrs: vec![],
            link: None,
            tw: String::new(),
            has_overrides: false,
            children: vec![],
        };
        let alt = HOSTILE.to_owned();
        root.children.push(Child::El(Element {
            tag: "img".into(),
            class: "c".into(),
            attrs: vec![("alt".into(), alt), ("fetchpriority".into(), "high".into())],
            link: None,
            tw: String::new(),
            has_overrides: false,
            children: vec![],
        }));
        root.children.push(Child::El(Element {
            tag: "a".into(),
            class: String::new(),
            attrs: vec![],
            link: Some(LinkTarget::Url("https://x.test/?a=\"><b>&c".into())),
            tw: String::new(),
            has_overrides: false,
            children: vec![Child::Text(Text::Lit(HOSTILE.into()))],
        }));
        render(d, &Plain, &root, 0, &mut out);
        out
    }

    #[test]
    fn html_escapes_text_and_attribute_values() {
        let out = page(Dialect::Html);
        assert!(
            !out.contains("<img onerror") && !out.contains("</script>") && !out.contains("\"><b>"),
            "{out}"
        );
        assert!(
            out.contains("&lt;/script&gt;")
                && out.contains("href=\"https://x.test/?a=&quot;&gt;&lt;b&gt;&amp;c\""),
            "{out}"
        );
    }

    #[test]
    fn jsx_writes_class_name_self_closes_and_turns_hostile_text_into_a_string() {
        let out = page(Dialect::Jsx);
        assert!(
            out.contains("<img className=\"c\"")
                && out.contains("fetchPriority=\"high\"")
                && out.contains(" />"),
            "{out}"
        );
        let json = serde_json::to_string(HOSTILE).unwrap();
        assert!(out.contains(&format!("{{{json}}}")), "{out}");
        assert!(!out.contains("alt=\"</"), "{out}");
    }

    #[test]
    fn vue_and_astro_cannot_interpolate_user_text() {
        for d in [Dialect::Vue, Dialect::Astro] {
            let out = page(d);
            assert!(
                !out.contains("{{") && !out.contains("{b}") && !out.contains("<img onerror"),
                "{d:?}: {out}"
            );
            assert!(out.contains("&#123;&#123;x&#125;&#125;"), "{d:?}: {out}");
        }
    }

    #[test]
    fn angular_text_has_no_binding_block_template_literal_or_control_flow_syntax() {
        let out = page(Dialect::Angular);
        let out = out.replace("{{ '{' }}", "");
        for bad in ["{{", "@if", "${", "`", "\\", "{b}", "<img onerror"] {
            assert!(!out.contains(bad), "`{bad}` in {out}");
        }
        assert!(
            out.contains("&#64;if") && out.contains("&#36;x&#125;"),
            "{out}"
        );
    }

    #[test]
    fn expressions_use_each_dialects_binding_syntax() {
        let (v, e) = (Val::Expr("styles.card".into()), "class");
        assert_eq!(attribute(Dialect::Jsx, e, &v), "className={styles.card}");
        assert_eq!(attribute(Dialect::Astro, e, &v), "class={styles.card}");
        assert_eq!(
            attribute(Dialect::Vue, e, &Val::Expr("a \"b\"".into())),
            ":class=\"a &quot;b&quot;\""
        );
        assert_eq!(
            attribute(Dialect::Angular, e, &v),
            "[class]=\"styles.card\""
        );
    }

    #[test]
    fn messages_come_from_the_hook_verbatim() {
        let root = Element {
            tag: "p".into(),
            class: String::new(),
            attrs: vec![],
            link: None,
            tw: String::new(),
            has_overrides: false,
            children: vec![Child::Text(Text::Msg("hero".into()))],
        };
        let mut out = String::new();
        render(Dialect::Jsx, &Plain, &root, 0, &mut out);
        assert_eq!(out, "<p>{t.hero}</p>\n");
    }
}
