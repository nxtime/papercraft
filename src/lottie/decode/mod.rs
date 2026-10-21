//! Untrusted JSON decoding: document and layer parsing, shape and property
//! readers, and the animation-wide validation budgets.

pub(crate) mod parse;
mod properties;
mod shapes;
pub(crate) mod validate;
