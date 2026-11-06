//! Frames to a framework-neutral element tree and its style model.
//!
//! The tree is built from the first locale's document. Every other locale is compared with it
//! node by node: differing text becomes a message key, differing declarations become
//! locale-scoped rules, and anything else (structure, images) keeps the first locale's version
//! with a warning.

use super::breakpoints;
use super::fluid;
use super::media::{self, Media};
use super::model::{Frame, Locale, Page};
use super::semantic::{self, Sem};
use super::style::{Classes, Decls, StyleModel, Token, diff, safe_value};
use super::{contrast, icon};
use crate::mcp::lint::DESKTOP_MIN_W;
use crate::mcp::{codegen, css};
use crate::model::link::valid_url;
use crate::model::{Document, ImageMode, LinkAction, Node, NodeId, NodeKind, Paint};
use std::collections::{BTreeMap, HashMap, HashSet};

/// An image whose top edge is above this many px is in the first screen: it loads at once.
const FOLD_PX: f64 = 900.0;
const PC_LINK: &str = "pc-link";

#[derive(Debug, Clone, PartialEq)]
pub enum Text {
    Lit(String),
    /// A key into the per-locale messages.
    Msg(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// Index into the site's pages.
    Page(usize),
    /// An external address that passed `valid_url`.
    Url(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Child {
    El(Element),
    Text(Text),
}

/// One HTML element. Attribute names are lower case HTML; a dialect renames them.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub tag: String,
    /// The merged class (empty when styling is Tailwind and nothing differs per locale).
    pub class: String,
    /// `src` holds a site-root relative path (`assets/..`) the writer rebases.
    pub attrs: Vec<(String, String)>,
    /// Set on `a` elements: where the anchor goes.
    pub link: Option<LinkTarget>,
    /// Tailwind utilities for this element, empty unless styling is Tailwind.
    pub tw: String,
    /// The class carries per-locale or breakpoint rules, so Tailwind output keeps it next to `tw`.
    pub has_overrides: bool,
    pub children: Vec<Child>,
}

impl Element {
    fn new(tag: &str) -> Self {
        Element {
            tag: tag.into(),
            class: String::new(),
            attrs: Vec::new(),
            link: None,
            tw: String::new(),
            has_overrides: false,
            children: Vec::new(),
        }
    }

    /// `tw`, then the merged class when it holds locale rules.
    pub fn tailwind_classes(&self) -> String {
        match (self.tw.is_empty(), self.has_overrides) {
            (false, true) => format!("{} {}", self.tw, self.class),
            (true, true) => self.class.clone(),
            _ => self.tw.clone(),
        }
    }
}

pub struct Params<'a> {
    pub docs: &'a [Document],
    pub frames: &'a [Frame],
    pub locales: &'a [Locale],
    pub tailwind: bool,
    /// Per locale; None where the caller gave none.
    pub title: &'a [Option<String>],
    pub description: &'a [Option<String>],
}

pub struct Built {
    pub pages: Vec<Page>,
    pub style: StyleModel,
    pub messages: Vec<BTreeMap<String, String>>,
    pub media: Media,
    pub warnings: Vec<String>,
    pub resolved: usize,
    pub unresolved: Vec<String>,
}

pub fn build(p: Params) -> Built {
    let mut w = Walk::new(&p);
    let mut pages = Vec::new();
    for (i, f) in p.frames.iter().enumerate() {
        let root = w.page(i, f);
        let title = |(loc, doc): (usize, &Document)| {
            let name = doc.get(f.id).map(|n| n.name.clone()).unwrap_or_default();
            match (p.title[loc].as_deref(), i) {
                (Some(t), 0) => t.to_owned(),
                (Some(t), _) => format!("{name} | {t}"),
                (None, _) => name,
            }
        };
        pages.push(Page {
            name: f.name.clone(),
            stem: f.stem.clone(),
            component: f.component.clone(),
            root,
            title: p.docs.iter().enumerate().map(title).collect(),
            description: p
                .docs
                .iter()
                .enumerate()
                .map(|(loc, d)| p.description[loc].clone().unwrap_or_else(|| describe(d, f)))
                .collect(),
        });
    }
    w.finish(pages, &p)
}

/// The first line of visible copy, as a default meta description; the frame name otherwise.
fn describe(doc: &Document, f: &Frame) -> String {
    const DESCRIPTION_CHARS: usize = 160;
    let copy = doc
        .subtree(f.id)
        .into_iter()
        .filter_map(|id| doc.get(id))
        .find_map(|n| match &n.kind {
            NodeKind::Text { text, .. } if !text.trim().is_empty() => {
                Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
            }
            _ => None,
        });
    copy.map_or_else(
        || f.name.clone(),
        |c| c.chars().take(DESCRIPTION_CHARS).collect(),
    )
}

#[derive(Clone, Copy, Default)]
struct Cx {
    in_link: bool,
    in_button: bool,
    /// Landmark tags of the ancestors, one bit each (`LANDMARKS`): a landmark never nests in itself.
    landmarks: u8,
}

const LANDMARKS: [&str; 4] = ["header", "nav", "footer", "aside"];

/// A narrower frame of the page being walked.
struct Bp {
    max: u32,
    paths: HashMap<String, NodeId>,
    hidden: usize,
}

struct Walk<'a> {
    docs: &'a [Document],
    frames: &'a [Frame],
    tailwind: bool,
    classes: Classes,
    roots: Vec<String>,
    messages: Vec<BTreeMap<String, String>>,
    message_values: HashMap<String, Vec<String>>,
    media: Media,
    warnings: Vec<String>,
    warned: HashSet<(NodeId, usize)>,
    resolved: usize,
    unresolved: Vec<String>,
    // The page being walked.
    page: usize,
    page_name: String,
    stem: String,
    top: f64,
    sem: Sem,
    hero_seen: bool,
    fixed_layout: usize,
    /// The page's narrower frames, widest first.
    bps: Vec<Bp>,
    /// Paths of the page frame's layers that every walked layer matched against the variants.
    bp_seen: HashSet<String>,
    /// Per page: the face `(family, bold, italic)` of its h1.
    h1_faces: Vec<Option<(String, bool, bool)>>,
}

impl<'a> Walk<'a> {
    fn new(p: &Params<'a>) -> Self {
        let mut classes = Classes::default();
        classes.reserve(PC_LINK);
        let roots = p.frames.iter().map(|f| classes.reserve(&f.name)).collect();
        Walk {
            docs: p.docs,
            frames: p.frames,
            tailwind: p.tailwind,
            classes,
            roots,
            messages: vec![BTreeMap::new(); p.docs.len()],
            message_values: HashMap::new(),
            media: Media::default(),
            warnings: Vec::new(),
            warned: HashSet::new(),
            resolved: 0,
            unresolved: Vec::new(),
            page: 0,
            page_name: String::new(),
            stem: String::new(),
            top: 0.0,
            sem: Sem::default(),
            hero_seen: false,
            fixed_layout: 0,
            bps: Vec::new(),
            bp_seen: HashSet::new(),
            h1_faces: Vec::new(),
        }
    }

    fn page(&mut self, i: usize, f: &Frame) -> Element {
        let doc = &self.docs[0];
        self.page = i;
        self.page_name = f.name.clone();
        self.stem = f.stem.clone();
        self.top = doc.world_rect(f.id).map_or(0.0, |r| r.y);
        self.sem = semantic::semantics(doc, f.id);
        let h1 = self
            .sem
            .heads
            .iter()
            .find(|(_, level)| **level == 1)
            .and_then(|(id, _)| doc.get(*id));
        self.h1_faces.push(h1.map(|n| {
            (
                media::family(&n.text_style),
                n.text_style.bold,
                n.text_style.italic,
            )
        }));
        self.hero_seen = false;
        self.fixed_layout = 0;
        self.bp_seen.clear();
        self.bps = f
            .variants
            .iter()
            .map(|(vid, max)| Bp {
                max: *max,
                paths: breakpoints::paths(doc, *vid),
                hidden: 0,
            })
            .collect();
        let root = self
            .node(f.id, Cx::default(), true)
            .unwrap_or_else(|| Element::new("div"));
        if self.fixed_layout > 0 {
            self.warnings.push(format!("{}: {} frame(s) without auto layout are absolutely positioned and will not reflow on a phone", f.name, self.fixed_layout));
        }
        self.warn_breakpoints(f);
        root
    }

    /// What the variants could not express: layers only one of the frames has.
    fn warn_breakpoints(&mut self, f: &Frame) {
        for (bp, (vid, _)) in self.bps.iter().zip(&f.variants) {
            let name = self.docs[0].get(*vid).map_or("", |n| n.name.as_str());
            let only_there = bp
                .paths
                .keys()
                .filter(|k| {
                    !self.bp_seen.contains(*k)
                        && (k.is_empty() || self.bp_seen.contains(breakpoints::parent_path(k)))
                })
                .count();
            if bp.hidden > 0 {
                self.warnings.push(format!("{} / {name}: {} layer(s) of the widest frame are missing here and are hidden at {}px and below", f.name, bp.hidden, bp.max));
            }
            if only_there > 0 {
                self.warnings.push(format!("{} / {name}: {only_there} layer(s) exist only in this frame and are left out; add them to the widest frame to show them", f.name));
            }
        }
    }

    fn finish(mut self, pages: Vec<Page>, p: &Params) -> Built {
        let roots: Vec<NodeId> = p
            .frames
            .iter()
            .flat_map(|f| std::iter::once(f.id).chain(f.variants.iter().map(|v| v.0)))
            .collect();
        for f in media::missing_families(&self.docs[0], &roots) {
            self.warnings.push(format!("font `{f}` is not installed, so its text is drawn in Inter; install it before exporting to keep the typeface"));
        }
        let uses = media::font_uses(self.docs, &roots);
        let (fonts, preload, fallbacks, font_warnings) = media::faces(&mut self.media, &uses);
        self.warnings.extend(font_warnings);
        for id in std::mem::take(&mut self.media.missing) {
            self.warnings.push(format!(
                "image {id} is not loaded; its pages keep the original reference"
            ));
        }
        let h1_faces = self
            .h1_faces
            .iter()
            .map(|key| {
                let (family, bold, italic) = key.as_ref()?;
                let weight = if *bold { 700 } else { 400 };
                fonts
                    .iter()
                    .position(|f| f.family == *family && f.weight == weight && f.italic == *italic)
            })
            .collect();
        let labels: Vec<String> = p.locales.iter().map(|l| l.label.clone()).collect();
        self.warnings
            .extend(contrast::warnings(self.docs, p.frames, &labels));
        // A custom property nothing reads is dead weight (copy text lives in the messages, not in CSS).
        // shortcut: a textual search of the built tree and classes, not a walk; upgrade if a token
        // name ever has to be told apart from one that merely contains it.
        let used = format!("{pages:?}{:?}", self.classes.list);
        let mut tokens = tokens(self.docs);
        tokens.retain(|t| {
            [")", ",", " "]
                .iter()
                .any(|end| used.contains(&format!("var(--{}{end}", t.name)))
        });
        let style = StyleModel {
            tokens,
            fonts,
            fallbacks,
            preload,
            h1_faces,
            favicon: icon::favicon(&mut self.media, &self.docs[0]),
            classes: self.classes.list,
            scopes: p.locales.iter().map(|l| l.scope.clone()).collect(),
        };
        Built {
            pages,
            style,
            messages: self.messages,
            media: self.media,
            warnings: self.warnings,
            resolved: self.resolved,
            unresolved: self.unresolved,
        }
    }

    fn warn_once(&mut self, id: NodeId, locale: usize, msg: String) {
        if self.warned.insert((id, locale)) {
            self.warnings.push(msg);
        }
    }

    /// The layer `id` of locale `i` when it can be compared with the first locale's: same kind,
    /// same children and same pictures. Otherwise the first locale's version is used.
    fn twin(&mut self, id: NodeId, n: &Node, i: usize) -> Option<&'a Node> {
        let docs = self.docs;
        let other = docs[i].get(id)?;
        let images = |n: &Node| {
            n.style
                .fills
                .iter()
                .filter_map(|p| {
                    if let Paint::Image { asset, .. } = p {
                        Some(asset.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        let same = std::mem::discriminant(&n.kind) == std::mem::discriminant(&other.kind)
            && n.children == other.children
            && images(n) == images(other);
        if !same {
            let msg = format!(
                "{} / {}: differs between locales in structure or pictures; the first locale is used",
                self.page_name, n.name
            );
            self.warn_once(id, i, msg);
        }
        same.then_some(other)
    }

    fn node(&mut self, id: NodeId, cx: Cx, is_root: bool) -> Option<Element> {
        let docs = self.docs;
        let n = docs[0].get(id)?;
        let link = self.link(n, cx.in_link);
        let button = semantic::is_button(n);
        let inner = Cx {
            in_link: cx.in_link || link.is_some(),
            in_button: cx.in_button || button,
            landmarks: cx.landmarks,
        };
        let mut el = self.element(id, n, inner, is_root, link.is_some() && button)?;
        let mut out = match link {
            // An icon drawn as `<img>` keeps its picture inside an anchor instead of becoming one.
            Some(target) if button && el.tag != "img" => {
                el.tag = "a".into();
                el.attrs.retain(|(k, _)| k != "type");
                el.link = Some(target);
                el
            }
            Some(target) => {
                let mut a = Element::new("a");
                if self.tailwind {
                    a.tw = "contents".into();
                } else {
                    a.class = self.classes.get(
                        PC_LINK,
                        Some(PC_LINK.into()),
                        self.page,
                        vec![("display".into(), "contents".into())],
                        Vec::new(),
                        Vec::new(),
                        None,
                    );
                }
                a.link = Some(target);
                a.children.push(Child::El(el));
                a
            }
            None => el,
        };
        if (out.link.is_some() || out.tag == "button") && !has_name(&out) {
            self.label(&mut out, n);
        }
        Some(out)
    }

    /// A link or button with no text or described picture in it is announced as just "link" or
    /// "button", so the layer (or icon) name becomes its label.
    fn label(&mut self, el: &mut Element, n: &Node) {
        let name = match explicit_alt(n).filter(|a| !a.is_empty()) {
            Some(alt) => alt,
            None => codegen::icon_of(&self.docs[0], n).map_or_else(
                || n.name.trim().to_owned(),
                |(_, icon)| icon.replace('-', " "),
            ),
        };
        if is_generic_name(&name) {
            let what = if el.tag == "button" { "button" } else { "link" };
            self.warnings.push(format!("{} / {}: a {what} with no text and a generic layer name has no accessible name; rename the layer to say what it does", self.page_name, n.name));
        } else {
            el.attrs.push(("aria-label".into(), name));
        }
    }

    /// Where the layer's link goes, counting it as resolved or not and warning about the ones a
    /// static page cannot keep.
    fn link(&mut self, n: &Node, in_link: bool) -> Option<LinkTarget> {
        let label = format!("{} / {}", self.page_name, n.name);
        let target = if let Some(to) = n.link {
            match self
                .frames
                .iter()
                .position(|f| f.id == to || f.variants.iter().any(|v| v.0 == to))
            {
                Some(i) => {
                    self.resolved += 1;
                    LinkTarget::Page(i)
                }
                None => {
                    let name = self.docs[0]
                        .get(to)
                        .map_or("a missing frame", |t| t.name.as_str());
                    self.unresolved
                        .push(format!("{label} -> {name} (not exported)"));
                    return None;
                }
            }
        } else {
            match &n.link_action {
                Some(LinkAction::Url { url }) => match valid_url(url) {
                    Ok(u) => LinkTarget::Url(u),
                    Err(e) => {
                        self.warnings.push(format!("{label}: link dropped: {e}"));
                        return None;
                    }
                },
                Some(other) => {
                    let kind = match other {
                        LinkAction::Back => "back",
                        LinkAction::Overlay { .. } => "overlay",
                        _ => "close",
                    };
                    self.warnings.push(format!("{label}: a `{kind}` link only works in a prototype and is left out of the site"));
                    return None;
                }
                None => return None,
            }
        };
        if in_link {
            self.warnings.push(format!("{label}: a link inside a link; the page keeps only the outer one (nested anchors are invalid HTML)"));
            return None;
        }
        Some(target)
    }

    fn element(
        &mut self,
        id: NodeId,
        n: &Node,
        cx: Cx,
        is_root: bool,
        own_link: bool,
    ) -> Option<Element> {
        if let Some(el) = self.vector(id, n, is_root) {
            return Some(el);
        }
        match &n.kind {
            NodeKind::Text { .. } => Some(self.text(id, n, cx, is_root)),
            NodeKind::Sticky { .. } => Some(self.sticky(id, n, is_root)),
            NodeKind::Rect | NodeKind::Ellipse if n.children.is_empty() => {
                let img = self.image(id, n, is_root);
                Some(img.unwrap_or_else(|| self.container(id, n, cx, is_root, own_link)))
            }
            _ => Some(self.container(id, n, cx, is_root, own_link)),
        }
    }

    fn container(
        &mut self,
        id: NodeId,
        n: &Node,
        cx: Cx,
        is_root: bool,
        own_link: bool,
    ) -> Element {
        let tag = if self.sem.main == Some(id) {
            "main"
        } else {
            match semantic::landmark_of(n).filter(|_| semantic::is_container(n)) {
                Some("button")
                    if !own_link
                        && !self.docs[0]
                            .subtree(id)
                            .iter()
                            .skip(1)
                            .any(|c| self.docs[0].get(*c).is_some_and(semantic::has_link)) =>
                {
                    "button"
                }
                Some(t @ ("header" | "nav" | "footer" | "section" | "aside")) => {
                    match LANDMARKS.iter().position(|l| *l == t) {
                        Some(bit) if cx.landmarks & (1 << bit) != 0 => "div",
                        _ => t,
                    }
                }
                Some("main") => "section",
                _ => "div",
            }
        };
        let mut el = Element::new(tag);
        if tag == "button" {
            el.attrs.push(("type".into(), "button".into()));
        }
        if n.is_frame() && !n.children.is_empty() && n.layout.is_none() {
            self.fixed_layout += 1;
        }
        if let Some(r @ ("link" | "list")) = n.a11y.as_ref().and_then(|a| a.role.as_deref()) {
            el.attrs.push(("role".into(), r.into()));
        }
        self.style(&mut el, id, n, is_root, false);
        let mut inner = cx;
        if let Some(bit) = LANDMARKS.iter().position(|l| *l == tag) {
            inner.landmarks |= 1 << bit;
        }
        for c in self.reading_order(n) {
            if let Some(child) = self.node(c, inner, false) {
                el.children.push(Child::El(child));
            }
        }
        el
    }

    /// Children in the order a screen reader meets them. Only a frame without auto layout may
    /// reorder: its children are placed by position, so the source order is free; elsewhere
    /// it also decides where they sit.
    fn reading_order(&self, n: &Node) -> Vec<NodeId> {
        let mut kids = n.children.clone();
        if n.layout.is_none() {
            let doc = &self.docs[0];
            let key = |c: &NodeId| doc.get(*c).and_then(|k| k.a11y.as_ref()).and_then(|a| a.order);
            // Unnumbered layers keep their place after the numbered ones.
            kids.sort_by_key(|c| key(c).unwrap_or(u32::MAX));
        }
        kids
    }

    fn text(&mut self, id: NodeId, n: &Node, cx: Cx, is_root: bool) -> Element {
        let NodeKind::Text {
            text, auto_width, ..
        } = &n.kind
        else {
            unreachable!("text() is called for text layers")
        };
        let chars = text.chars().count();
        let tag = if cx.in_link || cx.in_button {
            "span".to_owned()
        } else if let Some(level) = self.sem.heads.get(&id) {
            format!("h{level}")
        } else if self.sem.subtitles.contains(&id) {
            "p".to_owned()
        } else {
            semantic::text_name(&n.name)
                .filter(|t| *t == "p")
                .unwrap_or_else(|| semantic::text_tag(chars, !auto_width))
                .to_owned()
        };
        let mut el = Element::new(&tag);
        self.style(&mut el, id, n, is_root, false);
        el.children.push(Child::Text(self.content(id, n)));
        el
    }

    fn sticky(&mut self, id: NodeId, n: &Node, is_root: bool) -> Element {
        let mut el = Element::new("div");
        self.style(&mut el, id, n, is_root, false);
        el.children.push(Child::Text(self.content(id, n)));
        el
    }

    /// The layer's text: literal, or a message key when a locale words it differently.
    fn content(&mut self, id: NodeId, n: &Node) -> Text {
        let own = layer_text(n);
        let mut values = vec![own.clone()];
        for i in 1..self.docs.len() {
            values.push(
                self.twin(id, n, i)
                    .map(layer_text)
                    .unwrap_or_else(|| own.clone()),
            );
        }
        if values.iter().all(|v| *v == own) {
            return Text::Lit(own);
        }
        let base = n
            .bindings
            .get("text")
            .map(|b| key_part(b))
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| format!("{}.{}", key_part(&self.stem), key_part(&n.name)));
        let mut key = base.clone();
        let mut suffix = 2;
        // Two layers sharing a key must share the words; otherwise the later one gets its own.
        while self.message_values.get(&key).is_some_and(|v| *v != values) {
            key = format!("{base}_{suffix}");
            suffix += 1;
        }
        for (i, v) in values.iter().enumerate() {
            self.messages[i].insert(key.clone(), v.clone());
        }
        self.message_values.insert(key.clone(), values);
        Text::Msg(key)
    }

    /// A picture: a rect or ellipse with nothing but one image fill, as `<img>`. A tiled fill
    /// stays a CSS background.
    fn image(&mut self, id: NodeId, n: &Node, is_root: bool) -> Option<Element> {
        let [Paint::Image { asset, mode, .. }] = n.style.fills.as_slice() else {
            return None;
        };
        if *mode == ImageMode::Tile {
            return None;
        }
        let src = self
            .media
            .image(asset, n.rect.w)
            .unwrap_or_else(|| format!("paper-asset://{asset}"));
        let mut el = Element::new("img");
        let y = self.docs[0].world_rect(id).map_or(0.0, |r| r.y) - self.top;
        let hero = !self.hero_seen && y < FOLD_PX;
        self.hero_seen |= hero;
        let alt = if let Some(a) = explicit_alt(n) {
            a
        } else if is_generic_name(&n.name) {
            self.warnings.push(format!("{} / {}: the layer has a generic name, so the image has an empty `alt`; rename the layer to describe the picture", self.page_name, n.name));
            String::new()
        } else {
            n.name.clone()
        };
        el.attrs = vec![("src".into(), src), ("alt".into(), alt)];
        push_size(&mut el, n);
        if hero {
            el.attrs.push(("fetchpriority".into(), "high".into()));
        } else {
            el.attrs.push(("loading".into(), "lazy".into()));
        }
        el.attrs.push(("decoding".into(), "async".into()));
        self.style(&mut el, id, n, is_root, true);
        Some(el)
    }

    /// Vector drawings and icon frames, as an SVG file shown with `<img>`: the same in every
    /// dialect, with no attribute rewriting and nothing in it that could close a template.
    fn vector(&mut self, id: NodeId, n: &Node, is_root: bool) -> Option<Element> {
        let drawn = codegen::icon_of(&self.docs[0], n).is_some()
            || !matches!(
                n.kind,
                NodeKind::Frame { .. }
                    | NodeKind::Group
                    | NodeKind::Rect
                    | NodeKind::Ellipse
                    | NodeKind::Text { .. }
                    | NodeKind::Sticky { .. }
            );
        if !drawn {
            return None;
        }
        let svg = codegen::inline_svg(&self.docs[0], id);
        if !svg.contains("<svg") {
            return None;
        }
        let mut el = Element::new("img");
        el.attrs = vec![
            ("src".into(), self.media.svg(&svg)),
            ("alt".into(), explicit_alt(n).unwrap_or_default()),
        ];
        push_size(&mut el, n);
        self.style(&mut el, id, n, is_root, true);
        Some(el)
    }

    /// The class (and Tailwind utilities) of `el`, from the layer's declarations in every locale.
    fn style(&mut self, el: &mut Element, id: NodeId, n: &Node, is_root: bool, replaced: bool) {
        let base = self.decls(0, id, is_root, replaced).unwrap_or_default();
        let mut overrides = Vec::new();
        for i in 1..self.docs.len() {
            if self.twin(id, n, i).is_none() {
                continue;
            }
            if let Some(d) = self.decls(i, id, is_root, replaced) {
                let changed = diff(&base, &d);
                if !changed.is_empty() {
                    overrides.push((i, changed));
                }
            }
        }
        let media = self.breakpoint_decls(id, &base, is_root, replaced);
        el.has_overrides = !overrides.is_empty() || !media.is_empty();
        if self.tailwind {
            el.tw = self.tailwind_classes(n, is_root, replaced);
        }
        if self.tailwind && !el.has_overrides {
            return;
        }
        let root = is_root.then_some(self.page);
        let name = root.map(|p| self.roots[p].clone());
        el.class = self
            .classes
            .get(&n.name, name, self.page, base, overrides, media, root);
    }

    /// What each narrower frame changes on layer `id`, matched by layer path; a layer the
    /// frame lacks is hidden at its width.
    fn breakpoint_decls(
        &mut self,
        id: NodeId,
        base: &Decls,
        is_root: bool,
        replaced: bool,
    ) -> Vec<(u32, Decls)> {
        if self.bps.is_empty() {
            return Vec::new();
        }
        let root = self.frames[self.page].id;
        let key = breakpoints::path(&self.docs[0], root, id);
        let parent_found =
            |bp: &Bp| key.is_empty() || bp.paths.contains_key(breakpoints::parent_path(&key));
        let mut out = Vec::new();
        for k in 0..self.bps.len() {
            let changed = match self.bps[k].paths.get(&key).copied() {
                Some(vid) => {
                    let d = self.decls(0, vid, is_root, replaced).unwrap_or_default();
                    let mut changed = diff(base, &d);
                    if is_root {
                        // The page keeps its desktop frame's width rule; a phone column is not centred on a tablet.
                        changed.retain(|(p, _)| p != "max-width" && p != "margin");
                    }
                    changed
                }
                None if parent_found(&self.bps[k]) => {
                    self.bps[k].hidden += 1;
                    vec![("display".to_owned(), "none".to_owned())]
                }
                None => Vec::new(),
            };
            if !changed.is_empty() {
                out.push((self.bps[k].max, changed));
            }
        }
        self.bp_seen.insert(key);
        out
    }

    fn tailwind_classes(&self, n: &Node, is_root: bool, replaced: bool) -> String {
        let doc = &self.docs[0];
        let mut tw = codegen::tw_classes(doc, n, is_root);
        if matches!(n.kind, NodeKind::Text { .. }) {
            // codegen names only a font the text sets, without fallbacks, and Tailwind's default
            // is the system font: give every text the CSS dialects' stack instead.
            tw = tw
                .split(' ')
                .filter(|c| !c.starts_with("font-['"))
                .collect::<Vec<_>>()
                .join(" ");
            tw.push(' ');
            tw.push_str(&tw_font(&media::family(&n.text_style)));
        }
        let fluid = fluid::decls(doc, n, replaced);
        if !fluid.is_empty() {
            // Replace what the fluid rule overrides: Tailwind has no cascade order to lean on.
            let drop_h = fluid.iter().any(|d| d.0 == "height");
            let drop_s = fluid.iter().any(|d| d.0 == "font-size");
            // Type this big is always an arbitrary `text-[Npx]`, never a named size.
            tw = tw
                .split(' ')
                .filter(|c| {
                    !(drop_s && c.starts_with("text-[") && c.ends_with("px]"))
                        && !(drop_h && c.starts_with("h-["))
                })
                .collect::<Vec<_>>()
                .join(" ");
            for c in fluid::tailwind(&fluid) {
                tw.push(' ');
                tw.push_str(&c);
            }
        }
        if is_root && n.layout.is_some() {
            // The page rule of `extras`, as Tailwind classes.
            tw = tw
                .split(' ')
                .filter(|c| {
                    !["w-", "h-", "min-h-", "max-w-"]
                        .iter()
                        .any(|p| c.starts_with(p))
                })
                .collect::<Vec<_>>()
                .join(" ");
            let column = if n.rect.w < DESKTOP_MIN_W {
                format!(" max-w-[{}px] mx-auto", round2(n.rect.w))
            } else {
                String::new()
            };
            tw.push_str(&format!(
                " w-full{column} h-auto min-h-[max(100vh,{}px)]",
                round2(n.rect.h)
            ));
        }
        tw.trim().to_owned()
    }

    /// The declarations of layer `id` in locale `i`.
    fn decls(&mut self, i: usize, id: NodeId, is_root: bool, replaced: bool) -> Option<Decls> {
        let docs = self.docs;
        let doc = &docs[i];
        let n = doc.get(id)?;
        let mut m = css::computed(
            n,
            codegen::parent_layout(doc, n).as_ref(),
            n.parent.is_none(),
        );
        if is_root {
            for k in ["position", "left", "top"] {
                m.remove(k);
            }
        }
        if matches!(n.kind, NodeKind::Ellipse) {
            m.insert("borderRadius".into(), "50%".into());
        }
        if replaced {
            // An `<img>` draws its own pixels; a CSS fill would paint a box behind them.
            m.remove("background");
            m.remove("backgroundColor");
        }
        let mut out = Decls::new();
        for (k, v) in &m {
            let Some(val) = v.as_str() else { continue };
            let value = if k == "fontFamily" {
                media::font_stack(&media::family(&n.text_style))
            } else {
                self.value(val)
            };
            out.push((codegen::camel_to_kebab(k), value));
        }
        self.extras(&mut out, doc, n, is_root, replaced);
        Some(out)
    }

    fn extras(&self, out: &mut Decls, doc: &Document, n: &Node, is_root: bool, replaced: bool) {
        let set = |out: &mut Decls, k: &str, v: String| match out.iter_mut().find(|d| d.0 == k) {
            Some(d) => d.1 = v,
            None => out.push((k.to_owned(), v)),
        };
        if let [Paint::Image { mode, .. }] = n.style.fills.as_slice() {
            let (size, repeat) = match mode {
                ImageMode::Fill => ("cover", "no-repeat"),
                ImageMode::Fit => ("contain", "no-repeat"),
                ImageMode::Stretch => ("100% 100%", "no-repeat"),
                ImageMode::Tile => ("auto", "repeat"),
            };
            if replaced {
                let fit = match mode {
                    ImageMode::Fill => "cover",
                    ImageMode::Fit => "contain",
                    _ => "fill",
                };
                set(out, "object-fit", fit.into());
            } else if !n.children.is_empty()
                || *mode != ImageMode::Tile
                || matches!(n.kind, NodeKind::Rect | NodeKind::Ellipse)
            {
                set(out, "background-size", size.into());
                set(out, "background-repeat", repeat.into());
                set(out, "background-position", "center".into());
            }
        }
        for (k, v) in fluid::decls(doc, n, replaced) {
            set(out, &k, v);
        }
        if layer_text(n).contains('\n') {
            set(out, "white-space", "pre-line".into());
        }
        // Absolute children are placed against the nearest positioned ancestor: this layer.
        let places_children = n.layout.is_none()
            || n.children.iter().any(|c| doc.get(*c).is_some_and(|c| c.layout_child.absolute));
        if places_children && !n.children.is_empty() && !out.iter().any(|d| d.0 == "position") {
            set(out, "position", "relative".into());
        }
        if is_root && n.layout.is_some() {
            // A page takes the window's width and at least its height, so its background reaches
            // every edge, and grows instead of clipping. A screen narrower than a desktop stays a
            // column of its designed width: a phone layout stretched across a monitor falls apart.
            set(out, "width", "100%".into());
            if n.rect.w < DESKTOP_MIN_W {
                set(out, "max-width", format!("{}px", round2(n.rect.w)));
            }
            set(out, "height", "auto".into());
            set(
                out,
                "min-height",
                format!("max(100vh, {}px)", round2(n.rect.h)),
            );
            if n.rect.w < DESKTOP_MIN_W {
                set(out, "margin", "0 auto".into());
            }
        }
    }

    /// A declaration value: pictures named by id become site paths, and nothing that could end
    /// a rule or a style block survives.
    fn value(&mut self, val: &str) -> String {
        const SCHEME: &str = "paper-asset://";
        let mut out = String::with_capacity(val.len());
        let mut rest = val;
        while let Some(i) = rest.find(SCHEME) {
            out.push_str(&rest[..i]);
            let after = &rest[i + SCHEME.len()..];
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(after.len());
            match self.media.background(&after[..end]) {
                Some(path) => out.push_str(&path),
                None => out.push_str(&rest[i..i + SCHEME.len() + end]),
            }
            rest = &after[end..];
        }
        out.push_str(rest);
        safe_value(&out)
    }
}

/// The author's alt text: `Some("")` for a decorative picture, `None` when nothing was set.
fn explicit_alt(n: &Node) -> Option<String> {
    let a = n.a11y.as_ref()?;
    if a.decorative {
        return Some(String::new());
    }
    a.alt.clone()
}

fn layer_text(n: &Node) -> String {
    match &n.kind {
        NodeKind::Text { text, .. } | NodeKind::Sticky { text, .. } => text.clone(),
        _ => String::new(),
    }
}

fn push_size(el: &mut Element, n: &Node) {
    for (k, v) in [("width", n.rect.w), ("height", n.rect.h)] {
        if v >= 1.0 {
            el.attrs.push((k.into(), format!("{}", v.round() as i64)));
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// A name the app gave a layer ("Rectangle 12", "Image 3"): not a description of the picture.
/// Text, or a picture with a description, that a screen reader can announce.
fn has_name(el: &Element) -> bool {
    el.children.iter().any(|c| match c {
        Child::Text(Text::Lit(s)) => !s.trim().is_empty(),
        Child::Text(Text::Msg(_)) => true,
        Child::El(e) => e.attrs.iter().any(|(k, v)| k == "alt" && !v.is_empty()) || has_name(e),
    })
}

fn is_generic_name(name: &str) -> bool {
    const GENERIC: [&str; 14] = [
        "rectangle",
        "ellipse",
        "image",
        "img",
        "frame",
        "group",
        "vector",
        "photo",
        "picture",
        "layer",
        "shape",
        "fill",
        "mask",
        "copy",
    ];
    let mut words = name
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|w| !w.chars().all(|c| c.is_ascii_digit()))
        .peekable();
    words.peek().is_none() || words.all(|w| GENERIC.contains(&w.as_str()))
}

/// A message key part: lower case ASCII letters and digits, `_` between runs.
fn key_part(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_');
    if out.is_empty() {
        "text".into()
    } else {
        out.to_owned()
    }
}

/// `media::font_stack` as a Tailwind class: `'` quotes, as classes sit in double-quoted
/// attributes, and `_` for spaces.
// shortcut: a family name holding `_`, `'` or `\` comes out wrong; quote those if one turns up.
fn tw_font(family: &str) -> String {
    let stack = media::font_stack(family)
        .replace('"', "'")
        .replace(", ", ",")
        .replace(' ', "_");
    format!("font-[family-name:{stack}]")
}

/// Variables and tokens of the first locale, with the values other locales give them.
fn tokens(docs: &[Document]) -> Vec<Token> {
    let first = &docs[0];
    let others = |get: &dyn Fn(&Document) -> Option<String>| -> Vec<(usize, String)> {
        let base = get(first);
        (1..docs.len())
            .filter_map(|i| {
                get(&docs[i])
                    .filter(|v| Some(v) != base.as_ref())
                    .map(|v| (i, v))
            })
            .collect()
    };
    let colors = first.variables.iter().map(|(k, c)| Token {
        name: codegen::css_ident(k),
        value: c.to_hex(),
        color: true,
        overrides: others(&|d| d.variables.get(k).map(|c| c.to_hex())),
    });
    let plain = first.tokens.iter().map(|(k, v)| Token {
        name: codegen::css_ident(k),
        value: safe_value(v),
        color: false,
        overrides: others(&|d| d.tokens.get(k).map(|v| safe_value(v))),
    });
    colors.chain(plain).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_layer_names_are_not_alt_text() {
        for generic in [
            "Rectangle 12",
            "Image 3",
            "image",
            "Frame",
            "Ellipse 4 copy",
            "Group 1",
            "123",
        ] {
            assert!(is_generic_name(generic), "{generic}");
        }
        for described in ["Team photo", "Hero image", "Logo"] {
            assert!(!is_generic_name(described), "{described}");
        }
    }

    #[test]
    fn message_keys_are_lower_case_ascii() {
        assert_eq!(key_part("Hero / Title"), "hero_title");
        assert_eq!(key_part("hero.title"), "hero_title");
        assert_eq!(key_part("\u{3053}\u{3093}"), "text");
    }
}
