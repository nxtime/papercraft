//! Accessibility annotations on a layer: what a screen reader should say and in which order.
//! Everything is optional; a layer that sets none carries no `a11y` at all.

use serde::{Deserialize, Serialize};

/// Roles an author may force on a layer (the site export writes them as the matching tag, or as
/// a `role` attribute for `link` and `list`).
pub const ROLES: [&str; 10] =
    ["button", "link", "heading", "list", "nav", "main", "header", "footer", "aside", "section"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct A11y {
    /// Text alternative of a picture (or the accessible name of an icon button).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    /// The layer is ornament: assistive tech skips it, so it needs no alt text.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub decorative: bool,
    /// One of [`ROLES`], overriding what the layer's name suggests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Heading level 1..=6 when the role is `heading`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// Position among its siblings in the reading order (lower reads first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
}

impl A11y {
    pub fn is_empty(&self) -> bool {
        *self == A11y::default()
    }
}
