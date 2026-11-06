//! The style model of a site and its serializers: tokens, font faces, base rules and one class
//! per distinct set of declarations, with the per-locale differences kept apart so a framework
//! writer can print plain CSS, SCSS partials or a Tailwind theme from the same data.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Ordered `(property, value)` pairs, properties in kebab case.
pub type Decls = Vec<(String, String)>;

/// Sizes in the design are border-box, and a phone must not scroll sideways. `a` and `button`
/// are reset because layers draw their own text and fills.
pub const BASE_CSS: &str = "*, *::before, *::after { box-sizing: border-box; }\n\
body { margin: 0; }\n\
img, svg { max-width: 100%; }\n\
h1, h2, h3, h4, h5, h6, p { margin: 0; }\n\
a { color: inherit; text-decoration: none; }\n\
button { padding: 0; border: 0; background: none; font: inherit; color: inherit; cursor: pointer; text-align: inherit; }\n";

/// A custom property: `--name`, with the value other locales give it.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub name: String,
    pub value: String,
    /// A colour variable (Tailwind's `--color-*` namespace) rather than a plain token.
    pub color: bool,
    /// `(locale index, value)` where a locale differs from the first.
    pub overrides: Vec<(usize, String)>,
}

/// One `@font-face`.
#[derive(Debug, Clone, PartialEq)]
pub struct FontFace {
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    /// Site-root relative: `assets/fonts/<hash>.<ext>`.
    pub path: String,
    pub format: &'static str,
}

/// A class: the declarations one or more layers share.
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    /// Readable, from the first layer's name, unique site-wide, a valid CSS identifier.
    pub name: String,
    pub decls: Decls,
    /// Only the declarations a locale changes (a removed one is `initial`).
    pub overrides: Vec<(usize, Decls)>,
    /// Pages whose markup uses the class.
    pub pages: BTreeSet<usize>,
    /// The class of that page's root frame (holds the responsive rule).
    pub root_of: Option<usize>,
}

#[derive(Debug, Default)]
pub struct StyleModel {
    pub tokens: Vec<Token>,
    pub fonts: Vec<FontFace>,
    /// A metric-matched system font per family, drawn until the web font loads.
    pub fallbacks: Vec<super::fontsub::Fallback>,
    /// Index into `fonts` of the body text face, to preload.
    pub preload: Option<usize>,
    /// Per page, index into `fonts` of the face its h1 is set in (the second font to preload).
    pub h1_faces: Vec<Option<usize>>,
    /// Site-root relative path of the layer named Favicon, written as the site icon.
    pub favicon: Option<String>,
    pub classes: Vec<Class>,
    /// Per locale, the selector prefix its overrides sit under: empty for the first locale,
    /// `:root:lang(pt-BR)` for a language, `:root.mode-dark` for any other mode.
    pub scopes: Vec<String>,
}

/// What makes two classes the same: declarations, per-locale overrides and the page root.
type Signature = (Decls, Vec<(usize, Decls)>, Option<usize>);

/// Collects classes while the pages are built.
#[derive(Default)]
pub struct Classes {
    pub list: Vec<Class>,
    by_sig: HashMap<Signature, usize>,
    taken: BTreeSet<String>,
}

impl Classes {
    /// Claims a name up front (page roots, so a layer cannot take them).
    pub fn reserve(&mut self, hint: &str) -> String {
        unique(&mut self.taken, class_slug(hint))
    }

    /// The class for these declarations: an existing identical one, else a new one named after `hint`.
    /// `name` is a name from [`Self::reserve`] to use for a new class.
    pub fn get(
        &mut self,
        hint: &str,
        name: Option<String>,
        page: usize,
        decls: Decls,
        overrides: Vec<(usize, Decls)>,
        root_of: Option<usize>,
    ) -> String {
        let key = (decls.clone(), overrides.clone(), root_of);
        if let Some(&i) = self.by_sig.get(&key) {
            self.list[i].pages.insert(page);
            return self.list[i].name.clone();
        }
        let name = name.unwrap_or_else(|| self.reserve(hint));
        self.by_sig.insert(key, self.list.len());
        self.list.push(Class {
            name: name.clone(),
            decls,
            overrides,
            pages: BTreeSet::from([page]),
            root_of,
        });
        name
    }
}

/// A class-safe slug: lower case letters and digits, one `-` between runs, never starting with a digit.
pub fn class_slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    let out: String = out.chars().take(40).collect();
    let out = out.trim_end_matches('-');
    match out.chars().next() {
        None => "layer".into(),
        Some(c) if c.is_ascii_digit() => format!("n-{out}"),
        _ => out.to_owned(),
    }
}

/// `stem`, or `stem-2`, `stem-3`... when taken (names compare lower case).
pub fn unique(taken: &mut BTreeSet<String>, stem: String) -> String {
    let mut name = stem.clone();
    let mut n = 2;
    while !taken.insert(name.to_lowercase()) {
        name = format!("{stem}-{n}");
        n += 1;
    }
    name
}

/// Removes what could end a rule or a `<style>` block from a value that is not a quoted string.
pub fn safe_value(v: &str) -> String {
    v.chars()
        .map(|c| {
            if matches!(c, '{' | '}' | ';' | '<' | '>') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// A custom property name piece the stylesheet can hold.
fn rebase(value: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        value.to_owned()
    } else {
        value.replace("url(assets/", &format!("url({prefix}assets/"))
    }
}

fn block(selector: &str, decls: &Decls, prefix: &str, indent: &str, out: &mut String) {
    out.push_str(&format!("{indent}{selector} {{\n"));
    for (k, v) in decls {
        out.push_str(&format!("{indent}  {k}: {};\n", rebase(v, prefix)));
    }
    out.push_str(&format!("{indent}}}\n"));
}

impl StyleModel {
    /// `:root` custom properties, then one block per locale that changes some.
    pub fn tokens_css(&self) -> String {
        let mut out = String::new();
        if !self.tokens.is_empty() {
            let decls: Decls = self
                .tokens
                .iter()
                .map(|t| (format!("--{}", t.name), t.value.clone()))
                .collect();
            block(":root", &decls, "", "", &mut out);
        }
        for (i, scope) in self.scopes.iter().enumerate().skip(1) {
            let decls: Decls = self
                .tokens
                .iter()
                .filter_map(|t| {
                    t.overrides
                        .iter()
                        .find(|o| o.0 == i)
                        .map(|o| (format!("--{}", t.name), o.1.clone()))
                })
                .collect();
            if !decls.is_empty() {
                block(&scope_selector(scope, ""), &decls, "", "", &mut out);
            }
        }
        out
    }

    /// `@font-face` rules; `prefix` is what the stylesheet must put before `assets/` to reach the site root.
    pub fn fonts_css(&self, prefix: &str) -> String {
        let mut out = String::new();
        for f in &self.fonts {
            out.push_str(&format!(
                "@font-face {{\n  font-family: {};\n  font-style: {};\n  font-weight: {};\n  font-display: swap;\n  src: url({prefix}{}) format(\"{}\");\n}}\n",
                super::super::codegen::css_string(&f.family),
                if f.italic { "italic" } else { "normal" },
                f.weight,
                f.path,
                f.format
            ));
        }
        for f in &self.fallbacks {
            out.push_str(&format!(
                "@font-face {{
  font-family: {};
  src: local({});
  size-adjust: {}%;
  ascent-override: {}%;
  descent-override: {}%;
  line-gap-override: {}%;
}}
",
                super::super::codegen::css_string(&f.family),
                super::super::codegen::css_string(f.local),
                f.size_adjust,
                f.ascent,
                f.descent,
                f.gap
            ));
        }
        out
    }

    /// The rules of the classes `keep` accepts, and after them the locale overrides of those classes.
    pub fn classes_css(&self, prefix: &str, keep: impl Fn(&Class) -> bool) -> String {
        let mut out = String::new();
        for c in self.classes.iter().filter(|c| keep(c)) {
            block(&format!(".{}", c.name), &c.decls, prefix, "", &mut out);
        }
        out.push_str(&self.overrides_css(prefix, keep));
        out
    }

    /// Only the locale overrides of the classes `keep` accepts.
    pub fn overrides_css(&self, prefix: &str, keep: impl Fn(&Class) -> bool) -> String {
        let mut out = String::new();
        for c in self.classes.iter().filter(|c| keep(c)) {
            for (loc, decls) in &c.overrides {
                if let Some(scope) = self.scopes.get(*loc) {
                    block(
                        &scope_selector(scope, &format!(".{}", c.name)),
                        decls,
                        prefix,
                        "",
                        &mut out,
                    );
                }
            }
        }
        out
    }

    /// Everything the site shares: tokens, fonts, base rules and the classes more than one page
    /// uses (or none does).
    pub fn global_css(&self, prefix: &str) -> String {
        format!(
            "{}{}{BASE_CSS}{}",
            self.tokens_css(),
            self.fonts_css(prefix),
            self.classes_css(prefix, |c| c.pages.len() != 1)
        )
    }

    /// The rules only page `page` uses, its root first.
    pub fn page_css(&self, page: usize, prefix: &str) -> String {
        self.classes_css(prefix, |c| c.pages.len() == 1 && c.pages.contains(&page))
    }

    /// The whole site in one stylesheet (the html writer's `assets/site-<hash>.css`).
    pub fn all_css(&self, prefix: &str) -> String {
        format!(
            "{}{}{BASE_CSS}{}",
            self.tokens_css(),
            self.fonts_css(prefix),
            self.classes_css(prefix, |_| true)
        )
    }

    /// A Tailwind v4 `@theme` block: colour variables as `--color-*`, other tokens as they are.
    /// Locale values stay in `tokens_css` (they are plain custom properties, not theme keys).
    pub fn tailwind_theme(&self) -> String {
        let mut out = String::from("@theme {\n");
        for t in &self.tokens {
            let name = if t.color {
                format!("--color-{}", t.name)
            } else {
                format!("--{}", t.name)
            };
            out.push_str(&format!("  {name}: {};\n", t.value));
        }
        out.push_str("}\n");
        out
    }

    /// SCSS partials as `(path relative to the styles folder, content)`: `main.scss` `@use`s a
    /// partial each for tokens, fonts, base, the shared classes and every page, whose own classes
    /// nest under the page's root class. `pages` are `(page index, file stem)`.
    pub fn scss(&self, prefix: &str, pages: &[(usize, String)]) -> Vec<(String, String)> {
        let sass = |s: String| s.replace("#{", "\\23 {");
        let mut files = vec![
            ("_tokens.scss".to_owned(), sass(self.tokens_css())),
            ("_fonts.scss".to_owned(), sass(self.fonts_css(prefix))),
            ("_base.scss".to_owned(), BASE_CSS.to_owned()),
            (
                "_shared.scss".to_owned(),
                sass(self.classes_css(prefix, |c| c.pages.len() != 1)),
            ),
        ];
        let mut main =
            String::from("@use 'tokens';\n@use 'fonts';\n@use 'base';\n@use 'shared';\n");
        for (page, stem) in pages {
            let mine = |c: &Class| c.pages.len() == 1 && c.pages.contains(page);
            let mut out = String::new();
            if let Some(root) = self.classes.iter().find(|c| c.root_of == Some(*page)) {
                // Declarations first, then nested rules: Sass warns about the other order.
                out.push_str(&format!(".{} {{\n", root.name));
                for (k, v) in &root.decls {
                    out.push_str(&format!("  {k}: {};\n", rebase(v, prefix)));
                }
                for c in self
                    .classes
                    .iter()
                    .filter(|c| mine(c) && c.root_of.is_none())
                {
                    block(&format!(".{}", c.name), &c.decls, prefix, "  ", &mut out);
                }
                out.push_str("}\n");
            }
            out.push_str(&self.overrides_css(prefix, mine));
            let file = format!("_{}.scss", class_slug(stem));
            files.push((format!("pages/{file}"), sass(out)));
            main.push_str(&format!("@use 'pages/{}';\n", class_slug(stem)));
        }
        files.push(("main.scss".into(), main));
        files
    }
}

/// `scope` followed by a class selector: `:root:lang(pt-BR) .x`.
fn scope_selector(scope: &str, class: &str) -> String {
    format!("{scope}{}{class}", if class.is_empty() { "" } else { " " })
}

/// Per-locale differences between two declaration lists: what changes or is added, and
/// `initial` for what the locale drops.
pub fn diff(base: &Decls, other: &Decls) -> Decls {
    let have: BTreeMap<&str, &str> = other
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut out: Decls = other
        .iter()
        .filter(|(k, v)| base.iter().find(|b| b.0 == *k).is_none_or(|b| b.1 != *v))
        .cloned()
        .collect();
    out.extend(
        base.iter()
            .filter(|(k, _)| !have.contains_key(k.as_str()))
            .map(|(k, _)| (k.clone(), "initial".to_owned())),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(items: &[(&str, &str)]) -> Decls {
        items
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn identical_declarations_share_a_class_named_after_the_first_layer() {
        let mut c = Classes::default();
        let a = c.get("Card Title", None, 0, d(&[("color", "red")]), vec![], None);
        let b = c.get("Other", None, 1, d(&[("color", "red")]), vec![], None);
        let other = c.get("Card Title", None, 0, d(&[("color", "blue")]), vec![], None);
        assert_eq!(
            (a.as_str(), b.as_str(), other.as_str()),
            ("card-title", "card-title", "card-title-2")
        );
        assert_eq!(c.list[0].pages.len(), 2);
        assert_eq!(class_slug("3 Cards"), "n-3-cards");
        assert_eq!(class_slug("\u{1F600}"), "layer");
    }

    #[test]
    fn locale_diff_keeps_changes_and_resets_dropped_properties() {
        let base = d(&[("color", "red"), ("width", "1px")]);
        let pt = d(&[("color", "blue"), ("width", "1px"), ("gap", "2px")]);
        assert_eq!(diff(&base, &pt), d(&[("color", "blue"), ("gap", "2px")]));
        assert_eq!(
            diff(&base, &d(&[("color", "red")])),
            d(&[("width", "initial")])
        );
    }

    #[test]
    fn scss_nests_page_rules_under_the_root_and_guards_interpolation() {
        let mut c = Classes::default();
        c.get("Home", None, 0, d(&[("width", "100%")]), vec![], Some(0));
        c.get(
            "Title",
            None,
            0,
            d(&[("content", "\"#{x}\"")]),
            vec![(1, d(&[("color", "red")]))],
            None,
        );
        c.get("Shared", None, 0, d(&[("a", "b")]), vec![], None);
        c.get("Shared", None, 1, d(&[("a", "b")]), vec![], None);
        let m = StyleModel {
            classes: c.list,
            scopes: vec![String::new(), ":root:lang(pt-BR)".into()],
            ..Default::default()
        };
        let files: BTreeMap<_, _> = m.scss("../", &[(0, "index".into())]).into_iter().collect();
        let page = &files["pages/_index.scss"];
        assert!(
            page.contains(".home {\n  width: 100%;\n  .title {"),
            "{page}"
        );
        assert!(
            page.contains(":root:lang(pt-BR) .title {") && !page.contains("#{"),
            "{page}"
        );
        assert!(files["_shared.scss"].contains(".shared {"));
        assert!(
            files["main.scss"].starts_with("@use 'tokens';")
                && files["main.scss"].contains("@use 'pages/index';")
        );
    }
}
