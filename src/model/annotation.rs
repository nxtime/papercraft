//! Annotations: notes that explain a design, attached to a layer (or a point) with a leader line.
//!
//! Pure model. The anchor is relative to the target's rect and the note card is positioned
//! relative to the target's top-left, so moving or resizing the target keeps both attached.
//! Once the target is deleted the annotation is `detached` and `position` / `point` are world.

use super::document::Document;
use super::node::{Color, NodeId};
use crate::geom::V2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
    #[default]
    Center,
}

/// Where the leader line ends: a side of the target's rect and a fraction `t` (0..1) along it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Anchor {
    pub side: Side,
    #[serde(default)]
    pub t: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Author {
    #[default]
    User,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<NodeId>,
    /// World point the line ends at when there is no target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<V2>,
    #[serde(default)]
    pub anchor: Anchor,
    pub label: String,
    #[serde(default)]
    pub body: String,
    /// Category id (see [`AnnotationCategory`]).
    #[serde(default)]
    pub category: String,
    /// Note card top-left: relative to the target's top-left when `target` is set, else world.
    pub position: V2,
    #[serde(default)]
    pub author: Author,
    /// Unix milliseconds.
    #[serde(default)]
    pub created: u64,
    /// The target was deleted; the note stays where it was, with a warning badge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detached: bool,
}

impl Annotation {
    pub fn new(target: Option<NodeId>, label: &str, category: &str) -> Annotation {
        Annotation {
            id: 0,
            target,
            point: None,
            anchor: Anchor::default(),
            label: label.into(),
            body: String::new(),
            category: category.into(),
            position: V2::ZERO,
            author: Author::User,
            created: 0,
            detached: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationCategory {
    pub id: String,
    /// Short type key used by agents and the UI (`rule`, `a11y`, ...). Empty means the id.
    #[serde(default)]
    pub key: String,
    pub name: String,
    pub color: Color,
    pub icon: String,
}

/// Id (and key) of the category new notes get when none is named.
pub const DEFAULT_CATEGORY: &str = "rule";

impl AnnotationCategory {
    /// The type key (the id when none was set).
    pub fn key(&self) -> &str {
        if self.key.is_empty() { &self.id } else { &self.key }
    }
}

/// Find a category by its type key (case-insensitive).
pub fn category_by_key<'a>(cats: &'a [AnnotationCategory], key: &str) -> Option<&'a AnnotationCategory> {
    cats.iter().find(|c| c.key().eq_ignore_ascii_case(key))
}

/// Size the canvas gives a note card, used to stack automatically placed notes.
pub const CARD_W: f64 = 220.0;
pub const CARD_H: f64 = 72.0;
const CARD_GAP: f64 = 24.0;

/// Where a new note on `target` goes, as a `position` (relative to the target's top-left):
/// right of the target's top-level frame, level with the target, pushed down past any
/// existing note card so none overlap.
pub fn auto_position(doc: &Document, target: NodeId) -> V2 {
    let Some(t) = doc.world_rect(target) else { return V2::ZERO };
    let frame = frame_of(doc, target)
        .and_then(|f| doc.world_rect(f))
        .unwrap_or(t);
    let x = frame.right() + CARD_GAP;
    let mut y = t.y;
    let cards: Vec<V2> = doc.annotations.iter().map(|a| card_world(doc, a)).collect();
    while cards.iter().any(|c| {
        x < c.x + CARD_W && c.x < x + CARD_W && y < c.y + CARD_H && c.y < y + CARD_H
    }) {
        y += CARD_H + CARD_GAP / 2.0;
    }
    V2 { x: x, y: y } - t.min()
}

/// The six built-in categories, with stable ids.
pub fn default_categories() -> Vec<AnnotationCategory> {
    let c = |id: &str, name: &str, color: Color, icon: &str| AnnotationCategory {
        id: id.into(),
        key: id.into(),
        name: name.into(),
        color,
        icon: icon.into(),
    };
    vec![
        c("rule", "Business rule", Color::rgb(0x7c, 0x5c, 0xff), "scale"),
        c("behaviour", "Behaviour", Color::rgb(0x2f, 0x80, 0xed), "zap"),
        c("content", "Content", Color::rgb(0x1f, 0xa8, 0x7a), "type"),
        c("data", "Data", Color::rgb(0xe0, 0x8a, 0x1e), "database"),
        c("a11y", "Accessibility", Color::rgb(0xd9, 0x4f, 0x8a), "accessibility"),
        c("dev", "Dev note", Color::rgb(0x6b, 0x72, 0x80), "code"),
    ]
}

// ------------------------------------------------------------------------------ templates

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Display {
    #[default]
    Full,
    Badges,
}

/// One kind of note a template asks for: guidance for what it should say and optional
/// structured `Name: value` lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateCategory {
    pub key: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationTemplate {
    pub id: String,
    pub name: String,
    pub categories: Vec<TemplateCategory>,
    #[serde(default)]
    pub display: Display,
}

impl AnnotationTemplate {
    /// The enabled category with this type key (case-insensitive).
    pub fn enabled(&self, key: &str) -> Option<&TemplateCategory> {
        self.categories.iter().find(|c| c.enabled && c.key.eq_ignore_ascii_case(key))
    }

    /// Keys of the enabled categories, in order.
    pub fn enabled_keys(&self) -> Vec<&str> {
        self.categories.iter().filter(|c| c.enabled).map(|c| c.key.as_str()).collect()
    }
}

/// Id of the template used when neither the project nor the page picks one.
pub const DEFAULT_TEMPLATE: &str = "product-spec";

fn default_prompt(key: &str) -> &'static str {
    match key {
        "rule" => "The business rule or constraint this part enforces.",
        "behaviour" => "What happens on interaction: states, transitions, edge cases.",
        "content" => "The copy here: meaning, tone and limits.",
        "data" => "Where the data comes from, its format and what if it is missing.",
        "dev" => "Implementation note for developers.",
        "a11y" => "Accessibility requirement: role, label, focus order, contrast.",
        _ => "",
    }
}

/// The five built-in templates (editable copies are stored in `design.json`).
pub fn builtin_templates() -> Vec<AnnotationTemplate> {
    let t = |id: &str, name: &str, cats: &[(&str, &[&str])]| AnnotationTemplate {
        id: id.into(),
        name: name.into(),
        categories: cats
            .iter()
            .map(|(key, fields)| TemplateCategory {
                key: (*key).into(),
                enabled: true,
                prompt: default_prompt(key).into(),
                fields: fields.iter().map(|f| (*f).into()).collect(),
            })
            .collect(),
        display: Display::Full,
    };
    vec![
        t("product-spec", "Product spec", &[("rule", &[]), ("behaviour", &[]), ("content", &[]), ("data", &[])]),
        t("dev-handoff", "Dev handoff", &[("behaviour", &[]), ("data", &[]), ("dev", &[]), ("a11y", &[])]),
        t("accessibility-review", "Accessibility review", &[("a11y", &["Issue", "WCAG", "Fix"])]),
        t("content-review", "Content review", &[("content", &["Tone", "Max length"])]),
        t("minimal", "Minimal", &[("rule", &[])]),
    ]
}

/// The leader line's end in world space.
pub fn anchor_world(doc: &Document, a: &Annotation) -> V2 {
    let Some(r) = a.target.and_then(|t| doc.world_rect(t)) else {
        return a.point.unwrap_or(a.position);
    };
    let t = a.anchor.t.clamp(0.0, 1.0);
    match a.anchor.side {
        Side::Top => V2 { x: r.x + t * r.w, y: r.y },
        Side::Bottom => V2 { x: r.x + t * r.w, y: r.y + r.h },
        Side::Left => V2 { x: r.x, y: r.y + t * r.h },
        Side::Right => V2 { x: r.x + r.w, y: r.y + t * r.h },
        Side::Center => r.center(),
    }
}

/// The note card's top-left in world space.
pub fn card_world(doc: &Document, a: &Annotation) -> V2 {
    match a.target.and_then(|t| doc.world_rect(t)) {
        Some(r) => r.min() + a.position,
        None => a.position,
    }
}

/// The top-level frame a node lives in (the node itself when it is top level).
pub fn frame_of(doc: &Document, id: NodeId) -> Option<NodeId> {
    doc.nodes.get(&id)?;
    Some(doc.ancestors(id).last().copied().unwrap_or(id))
}

/// Number of every annotation, per top-level frame in reading order (target's world y, then x).
/// Annotations without a frame come last, numbered globally from 1, with frame `None`.
pub fn numbers(doc: &Document) -> HashMap<u64, (Option<NodeId>, u32)> {
    let mut keyed: Vec<(Option<NodeId>, V2, u64)> = doc
        .annotations
        .iter()
        .map(|a| {
            let frame = a.target.and_then(|t| frame_of(doc, t));
            let at = a
                .target
                .and_then(|t| doc.world_rect(t))
                .map_or_else(|| anchor_world(doc, a), |r| r.min());
            (frame, at, a.id)
        })
        .collect();
    keyed.sort_by(|a, b| {
        (a.0.is_none(), a.1.y, a.1.x, a.2)
            .partial_cmp(&(b.0.is_none(), b.1.y, b.1.x, b.2))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut count: HashMap<Option<NodeId>, u32> = HashMap::new();
    let mut out = HashMap::new();
    for (frame, _, id) in keyed {
        let n = count.entry(frame).or_insert(0);
        *n += 1;
        out.insert(id, (frame, *n));
    }
    out
}

/// Copies of the annotations whose target is a key of `map`, retargeted to its value. Ids are
/// unset (`Tx::add_annotation_copies` allocates them).
pub fn copies_for(
    annotations: &[Annotation],
    map: &HashMap<NodeId, NodeId>,
) -> Vec<Annotation> {
    annotations
        .iter()
        .filter_map(|a| {
            let t = map.get(&a.target?)?;
            Some(Annotation { target: Some(*t), ..a.clone() })
        })
        .collect()
}
