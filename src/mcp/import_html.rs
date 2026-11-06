//! `import_html`: opens a page exported by a prompt-to-design tool (one `.html`, or a `.zip` with
//! its stylesheets and images) as layers. The CSS is inlined into `style` attributes and the
//! images become project assets; `write_html` then maps the result like any other markup.
//!
//! Each HTML file becomes its own frame on the page, side by side, rather than its own page:
//! the files of one export are screens of one design, which are easier to compare and link when
//! they share a canvas, and the caller can still name a page to put them on.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{Value, json};

use super::ToolResult;
use super::exec::{Host, call_json, check_path, text};
use super::html::{self, Element, Html};
use crate::io::{backup::safe_relative, zip};

/// A page without a width of its own gets a desktop viewport, as a browser window would.
const DEFAULT_WIDTH: &str = "1280px";
/// Elements of a document head that make no layers.
const HEAD_TAGS: [&str; 3] = ["link", "meta", "base"];
/// Largest HTML file read, and largest zip (read whole, then inflated up to `MAX_INFLATED`).
const MAX_HTML_BYTES: u64 = 64 << 20;
const MAX_ZIP_BYTES: u64 = 256 << 20;
const MAX_INFLATED: usize = 256 << 20;
const VOID: [&str; 5] = ["img", "input", "br", "hr", "wbr"];

pub fn run(host: &mut dyn Host, args: &Value) -> ToolResult {
    let path = args["path"].as_str().ok_or("missing required string argument `path`")?.trim();
    check_path(path)?;
    let is_zip = check_file(path)?;
    let bytes = crate::vfs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let mut warnings = Vec::new();
    let (files, resources) = if is_zip {
        archive(&bytes, &mut warnings)?
    } else {
        let src = String::from_utf8(bytes).map_err(|_| format!("{path}: not UTF-8 text"))?;
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
        (vec![(name, src)], HashMap::new())
    };
    if files.is_empty() {
        return Err(format!("{path}: no .html file found"));
    }
    let (mut frames, mut ignored) = (Vec::new(), Vec::new());
    for (name, src) in files {
        let html = page(&name, &src, &resources, &mut warnings);
        let mut w = json!({"targetNodeId": "root", "html": html});
        if let Some(p) = args.get("pageId").filter(|p| !p.is_null()) {
            w["pageId"] = p.clone();
        }
        if let Some(f) = args.get("fileId").filter(|f| !f.is_null()) {
            w["fileId"] = f.clone();
        }
        let r = call_json(host, "write_html", w).map_err(|e| format!("{name}: {e}"))?;
        let ids: Vec<Value> = r["createdNodeIds"].as_array().cloned().unwrap_or_default();
        frames.push(json!({"file": name, "nodeIds": ids}));
        ignored.extend(r["ignoredStyles"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)));
        warnings.extend(r["warnings"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(|s| format!("{name}: {s}"))));
    }
    let mut v = json!({"imported": frames});
    if !ignored.is_empty() {
        v["ignoredStyles"] = json!(group_ignored(ignored.iter().map(String::as_str)));
    }
    if !warnings.is_empty() {
        v["warnings"] = json!(warnings);
    }
    text(v)
}

/// Whether `path` is a zip, after checking it is a page file of a sane size. The text of the
/// file comes back to the caller, so nothing else (`~/.ssh/config`) may be opened.
fn check_file(path: &str) -> Result<bool, String> {
    let lower = path.to_ascii_lowercase();
    let is_zip = lower.ends_with(".zip");
    if !is_zip && !lower.ends_with(".html") && !lower.ends_with(".htm") {
        return Err(format!("{path}: import_html opens only .html, .htm and .zip files"));
    }
    let cap = if is_zip { MAX_ZIP_BYTES } else { MAX_HTML_BYTES };
    let size = crate::vfs::metadata(path).map_err(|e| format!("{path}: {e}"))?.len();
    if size > cap {
        return Err(format!("{path}: {} MiB is over the {} MiB limit", size >> 20, cap >> 20));
    }
    Ok(is_zip)
}

/// The ignored-style entries of a write, one per property: `transform: rotate, scale`.
pub fn group_ignored<'a>(items: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut by: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for item in items {
        let (prop, detail) = item.split_once(": ").unwrap_or((item, ""));
        let details = by.entry(prop).or_default();
        details.extend(detail.split(", ").filter(|d| !d.is_empty()));
    }
    by.into_iter()
        .map(|(p, d)| if d.is_empty() { p.to_owned() } else { format!("{p}: {}", d.into_iter().collect::<Vec<_>>().join(", ")) })
        .collect()
}

/// The `.html` files of an archive in order, and every other safely named file by path.
/// Entries that would leave the archive's folder when extracted are skipped.
type Archive = (Vec<(String, String)>, HashMap<String, Vec<u8>>);

fn archive(bytes: &[u8], warnings: &mut Vec<String>) -> Result<Archive, String> {
    let (mut pages, mut files) = (Vec::new(), HashMap::new());
    for (name, data) in zip::read_capped(bytes, MAX_INFLATED)? {
        if name.starts_with("__MACOSX/") {
            continue;
        }
        if safe_relative(&name).is_none() {
            warnings.push(format!("skipped `{name}`: its path leaves the archive"));
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".html") || lower.ends_with(".htm") {
            match String::from_utf8(data) {
                Ok(src) => pages.push((name, src)),
                Err(_) => warnings.push(format!("skipped `{name}`: not UTF-8 text")),
            }
        } else {
            files.insert(name, data);
        }
    }
    Ok((pages, files))
}

/// The path `href` names, relative to the file in `dir`, if it stays inside the archive.
fn resolve(dir: &str, href: &str) -> Option<String> {
    let href = href.split(['?', '#']).next().unwrap_or_default();
    if href.is_empty() || href.contains("://") || href.starts_with("//") || href.starts_with("data:") {
        return None;
    }
    let mut parts: Vec<&str> = if href.starts_with('/') { vec![] } else { dir.split('/').filter(|p| !p.is_empty()).collect() };
    for p in href.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// One file as markup `write_html` takes: its CSS inlined, its images registered, and its body
/// as the one top-level frame.
fn page(name: &str, src: &str, resources: &HashMap<String, Vec<u8>>, warnings: &mut Vec<String>) -> String {
    let dir = name.rsplit_once('/').map_or("", |(d, _)| d);
    let mut tree = html::parse(src);
    let css = css_in_order(src, dir, resources, name, warnings);
    let rules = Rules::parse(&css, warnings);
    inline(&mut tree, &rules);
    images(&mut tree, dir, resources, name, warnings);
    let stem = name.rsplit('/').next().unwrap_or(name).rsplit_once('.').map_or(name, |(s, _)| s);
    let (mut style, children) = match find(&tree, "body") {
        Some(b) => (b.attr("style").unwrap_or("").to_owned(), b.children.clone()),
        None => (String::new(), tree),
    };
    if !declarations(&style).any(|(k, _)| k == "width") {
        style = format!("width:{DEFAULT_WIDTH};{style}");
    }
    let frame = Element {
        tag: "div".into(),
        attrs: vec![("layer-name".into(), stem.to_owned()), ("style".into(), style)],
        children: children.into_iter().filter(|c| !matches!(c, Html::Element(e) if HEAD_TAGS.contains(&e.tag.as_str()))).collect(),
        raw: None,
    };
    serialize(&Html::Element(frame))
}

fn find<'a>(tree: &'a [Html], tag: &str) -> Option<&'a Element> {
    tree.iter().find_map(|h| match h {
        Html::Element(e) if e.tag == tag => Some(e),
        Html::Element(e) => find(&e.children, tag),
        Html::Text(_) => None,
    })
}

/// The `href` of every `<link rel="stylesheet">`.
fn links(tree: &[Html]) -> Vec<String> {
    let mut out = Vec::new();
    for h in tree {
        if let Html::Element(e) = h {
            if e.tag == "link" && e.attr("rel").is_some_and(|r| r.eq_ignore_ascii_case("stylesheet")) {
                out.extend(e.attr("href").map(str::to_owned));
            }
            out.extend(links(&e.children));
        }
    }
    out
}

/// The CSS of every `<style>` block (the parser drops them from the tree) and linked stylesheet,
/// in document order, so a later rule overrides an earlier one as in a browser.
fn css_in_order(src: &str, dir: &str, resources: &HashMap<String, Vec<u8>>, name: &str, warnings: &mut Vec<String>) -> String {
    let lower = src.to_ascii_lowercase();
    let (mut out, mut at) = (String::new(), 0);
    while let Some(open) = [lower[at..].find("<style"), lower[at..].find("<link")].into_iter().flatten().min().map(|o| at + o) {
        let Some(gt) = lower[open..].find('>').map(|g| open + g) else { break };
        at = gt + 1;
        if lower[open..].starts_with("<style") {
            let end = lower[at..].find("</style").map_or(src.len(), |e| at + e);
            out.push_str(&src[at..end]);
            out.push('\n');
            at = end;
            continue;
        }
        for href in links(&html::parse(&src[open..at])) {
            match resolve(dir, &href).and_then(|p| resources.get(&p)) {
                Some(b) => out.push_str(&String::from_utf8_lossy(b)),
                None => warnings.push(format!("{name}: stylesheet `{href}` not found in the archive")),
            }
        }
    }
    out
}

/// Images in the archive become project assets, named by the id `write_html` accepts as a src.
fn images(tree: &mut [Html], dir: &str, resources: &HashMap<String, Vec<u8>>, name: &str, warnings: &mut Vec<String>) {
    for h in tree {
        let Html::Element(e) = h else { continue };
        if e.tag == "img"
            && let Some(src) = e.attr("src").map(str::to_owned)
            && let Some(path) = resolve(dir, &src)
        {
            let id = resources.get(&path).ok_or_else(|| "not in the archive".to_owned()).and_then(|b| crate::assets::register(b));
            match id {
                Ok(id) => {
                    if let Some(a) = e.attrs.iter_mut().find(|(k, _)| k == "src") {
                        a.1 = id;
                    }
                }
                Err(why) => warnings.push(format!("{name}: image `{src}`: {why}")),
            }
        }
        images(&mut e.children, dir, resources, name, warnings);
    }
}

// ---------------------------------------------------------------------------------- CSS

/// A selector of one element: `tag`, `.class`, `#id`, `*` and their combinations.
struct Simple {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

impl Simple {
    fn parse(sel: &str) -> Option<Simple> {
        let mut s = Simple { tag: None, id: None, classes: vec![] };
        let mut rest = sel.trim();
        let name = |r: &str| r.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')).unwrap_or(r.len());
        if let Some(r) = rest.strip_prefix('*') {
            rest = r;
        } else if !rest.starts_with(['.', '#']) {
            let n = name(rest);
            s.tag = Some(rest[..n].to_ascii_lowercase());
            rest = &rest[n..];
        }
        while !rest.is_empty() {
            let kind = rest.chars().next()?;
            let k = kind.len_utf8();
            let n = name(&rest[k..]);
            let word = rest[k..k + n].to_owned();
            match kind {
                '.' if n > 0 => s.classes.push(word),
                '#' if n > 0 => s.id = Some(word),
                _ => return None,
            }
            rest = &rest[k + n..];
        }
        (s.tag.as_deref() != Some("")).then_some(s)
    }

    fn matches(&self, e: &Element) -> bool {
        let class = e.attr("class").unwrap_or("");
        self.tag.as_ref().is_none_or(|t| *t == e.tag)
            && self.id.as_deref().is_none_or(|i| e.attr("id") == Some(i))
            && self.classes.iter().all(|c| class.split_whitespace().any(|k| k == c))
    }

    /// Ids, then classes, then the tag decide which rule wins.
    fn specificity(&self) -> (usize, usize, usize) {
        (usize::from(self.id.is_some()), self.classes.len(), usize::from(self.tag.is_some()))
    }
}

struct Rules {
    /// In cascade order: later wins.
    list: Vec<(Simple, String)>,
}

impl Rules {
    fn parse(css: &str, warnings: &mut Vec<String>) -> Rules {
        let (mut vars, mut raw, mut skipped) = (HashMap::new(), Vec::new(), BTreeSet::new());
        for (sel, body) in blocks(&strip_comments(css)) {
            if sel.starts_with('@') {
                skipped.insert(sel.split_whitespace().next().unwrap_or("@").to_owned());
                continue;
            }
            let is_root = matches!(sel.as_str(), ":root" | "html");
            for part in sel.split(',') {
                raw.push((part.trim().to_owned(), body.clone(), is_root));
            }
            if is_root {
                for (k, v) in declarations(&body).filter(|(k, _)| k.starts_with("--")) {
                    vars.insert(k, v);
                }
            }
        }
        let mut list: Vec<(Simple, String)> = Vec::new();
        for (sel, body, is_root) in raw {
            match Simple::parse(&sel) {
                Some(s) => {
                    let body = declarations(&body)
                        .filter(|(k, _)| !k.starts_with("--"))
                        .map(|(k, v)| format!("{k}:{}", substitute(&v, &vars)))
                        .collect::<Vec<_>>()
                        .join(";");
                    list.push((s, body));
                }
                None if is_root => {}
                None => {
                    skipped.insert(sel);
                }
            }
        }
        // Stable: equal specificity keeps source order.
        list.sort_by_key(|(s, _)| s.specificity());
        if !skipped.is_empty() {
            let shown: Vec<_> = skipped.iter().take(4).cloned().collect();
            warnings.push(format!("{} CSS rule(s) not inlined (only tag, .class and #id selectors are): {}", skipped.len(), shown.join(" | ")));
        }
        Rules { list }
    }
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = rest[i + 2..].split_once("*/").map_or("", |(_, r)| r);
    }
    out + rest
}

/// `selector { body }` pairs at the top level; at-rules keep their `@name` as selector.
fn blocks(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let sel = rest[..open].trim().to_owned();
        let (mut depth, mut end) = (0usize, rest.len());
        for (i, c) in rest[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push((sel, rest[open + 1..end].to_owned()));
        rest = rest.get(end + 1..).unwrap_or("");
    }
    out
}

/// Splits on `;` outside parentheses and quotes, so `url(data:...;base64,...)` and `"a;b"` stay whole.
fn split_declarations(body: &str) -> Vec<&str> {
    let (mut out, mut start, mut depth, mut quote) = (Vec::new(), 0, 0usize, None);
    for (i, c) in body.char_indices() {
        match (c, quote) {
            (q @ ('"' | '\''), None) => quote = Some(q),
            (q, Some(open)) if q == open => quote = None,
            ('(', None) => depth += 1,
            (')', None) => depth = depth.saturating_sub(1),
            (';', None) if depth == 0 => {
                out.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&body[start..]);
    out
}

fn declarations(body: &str) -> impl Iterator<Item = (String, String)> + '_ {
    split_declarations(body).into_iter().filter_map(|d| {
        let (k, v) = d.split_once(':')?;
        let v = v.replace("!important", "");
        Some((k.trim().to_ascii_lowercase(), v.trim().to_owned()))
    })
}

/// `var(--x)` and `var(--x, fallback)` with the page's own custom properties.
fn substitute(value: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(i) = rest.find("var(--") {
        out.push_str(&rest[..i]);
        let inner = &rest[i + 4..];
        let Some(end) = inner.find(')') else { break };
        let name = inner[..end].split(',').next().unwrap_or("").trim();
        match vars.get(name) {
            Some(v) => out.push_str(v),
            None => out.push_str(&rest[i..i + 4 + end + 1]),
        }
        rest = &inner[end + 1..];
    }
    out + rest
}

/// Puts the matching rules before each element's own `style`, so the element's own wins.
fn inline(tree: &mut [Html], rules: &Rules) {
    for h in tree {
        let Html::Element(e) = h else { continue };
        let css: Vec<&str> = rules.list.iter().filter(|(s, _)| s.matches(e)).map(|(_, b)| b.as_str()).filter(|b| !b.is_empty()).collect();
        if !css.is_empty() {
            let style = format!("{};{}", css.join(";"), e.attr("style").unwrap_or(""));
            match e.attrs.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case("style")) {
                Some(a) => a.1 = style,
                None => e.attrs.push(("style".into(), style)),
            }
        }
        inline(&mut e.children, rules);
    }
}

// ---------------------------------------------------------------------------------- markup

fn escape(s: &str, quote: bool) -> String {
    let s = s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    if quote { s.replace('"', "&quot;") } else { s }
}

fn serialize(h: &Html) -> String {
    match h {
        Html::Text(t) => escape(t, false),
        Html::Element(e) => {
            if let Some(raw) = &e.raw {
                return raw.clone();
            }
            let attrs: String = e.attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", escape(v, true))).collect();
            if VOID.contains(&e.tag.as_str()) {
                return format!("<{}{attrs}/>", e.tag);
            }
            format!("<{0}{attrs}>{1}</{0}>", e.tag, e.children.iter().map(serialize).collect::<String>())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_is_inlined_by_specificity_and_the_elements_own_style_wins() {
        let mut w = Vec::new();
        let rules = Rules::parse(":root{--c:#123} p{color:red} .a{color:var(--c);margin:0} #x{color:blue} .a .b{color:green} @media (x){p{color:pink}}", &mut w);
        let mut t = html::parse("<p id=x class=a style=\"margin:4px\">t</p>");
        inline(&mut t, &rules);
        let out = serialize(&t[0]);
        assert!(out.contains("color:red;color:#123;margin:0;color:blue;margin:4px"), "{out}");
        assert!(w[0].contains("2 CSS rule(s)"), "{w:?}");
    }

    #[test]
    fn non_ascii_selectors_do_not_panic() {
        let mut w = Vec::new();
        let rules = Rules::parse(".título{color:red} p€{color:red} p{color:blue}", &mut w);
        assert_eq!(rules.list.len(), 1, "only `p` is a supported selector");
        assert!(w[0].contains("2 CSS rule(s)"), "{w:?}");
    }

    #[test]
    fn declarations_split_outside_urls_and_strings() {
        let d: Vec<_> = declarations("background:url(data:image/png;base64,AAA);content:\"a;b\";color:red").collect();
        assert_eq!(d.len(), 3, "{d:?}");
        assert_eq!(d[0].1, "url(data:image/png;base64,AAA)");
        assert_eq!(d[1].1, "\"a;b\"");
    }

    #[test]
    fn css_follows_document_order_and_width_means_width() {
        let res = HashMap::from([("a.css".to_owned(), b"p{color:red}".to_vec())]);
        let mut w = Vec::new();
        let src = "<style>p{color:green}</style><link rel=stylesheet href=a.css><style>p{color:blue}</style><body><p>x</p></body>";
        let css = css_in_order(src, "", &res, "x.html", &mut w);
        assert!(css.find("green") < css.find("red") && css.find("red") < css.find("blue"), "{css}");
        let out = page("x.html", "<body style=\"max-width:600px;border-width:1px\"><p>x</p></body>", &res, &mut w);
        assert!(out.contains("width:1280px"), "{out}");
        let out = page("x.html", "<body style=\"width:600px\"><p>x</p></body>", &res, &mut w);
        assert!(!out.contains("1280px"), "{out}");
    }

    #[test]
    fn archive_skips_macos_and_non_utf8_pages() {
        let z = zip::write(&[
            ("__MACOSX/._a.html".into(), vec![0xff, 0xfe]),
            ("b.html".into(), vec![0xff, 0xfe]),
            ("a.html".into(), b"<p>x</p>".to_vec()),
        ]);
        let mut w = Vec::new();
        let (pages, _) = archive(&z, &mut w).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("b.html"));
    }

    #[test]
    fn only_page_files_of_a_sane_size_are_opened() {
        let dir = std::env::temp_dir();
        let txt = dir.join(format!("pc-imp-{:016x}.txt", fastrand::u64(..)));
        std::fs::write(&txt, "secret").unwrap();
        let err = check_file(&txt.to_string_lossy()).unwrap_err();
        assert!(err.contains("only .html"), "{err}");
        let big = dir.join(format!("pc-imp-{:016x}.HTML", fastrand::u64(..)));
        std::fs::File::create(&big).unwrap().set_len(MAX_HTML_BYTES + 1).unwrap();
        let err = check_file(&big.to_string_lossy()).unwrap_err();
        assert!(err.contains("limit"), "{err}");
        std::fs::remove_file(&txt).ok();
        std::fs::remove_file(&big).ok();
    }

    #[test]
    fn ignored_styles_group_by_property() {
        let g = group_ignored(["transform: rotate", "transform: scale", "float", "transform: rotate"]);
        assert_eq!(g, ["float", "transform: rotate, scale"]);
    }

    #[test]
    fn paths_stay_inside_the_archive() {
        assert_eq!(resolve("a/b", "../c.css").as_deref(), Some("a/c.css"));
        assert_eq!(resolve("", "../c.css"), None);
        assert_eq!(resolve("a", "https://x/y.css"), None);
    }
}
