//! Structured failures at the untrusted JSON boundary.

/// A malformed, unsupported, or over-budget animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The source exceeds the 512 KiB limit.
    TooLarge,
    /// The document is not valid JSON.
    Json {
        /// JSON decoder diagnostic.
        message: String,
    },
    /// A required value has the wrong type or range.
    Invalid {
        /// Property or violated invariant.
        what: String,
        /// Layer name/index and property path.
        at: String,
    },
    /// A feature or resource budget lies outside the supported subset.
    Unsupported {
        /// Unsupported feature or exceeded resource budget.
        what: String,
        /// Layer name/index and property path.
        at: String,
    },
}

pub(crate) fn invalid(what: impl Into<String>, at: &str) -> Error {
    Error::Invalid {
        what: what.into(),
        at: at.to_owned(),
    }
}

pub(crate) fn unsupported(what: impl Into<String>, at: &str) -> Error {
    Error::Unsupported {
        what: what.into(),
        at: at.to_owned(),
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "lottie: source exceeds 512 KiB"),
            Self::Json { message } => write!(f, "lottie: invalid JSON: {message}"),
            Self::Invalid { what, at } => write!(f, "lottie: invalid {what} at {at}"),
            Self::Unsupported { what, at } => write!(f, "lottie: unsupported {what} at {at}"),
        }
    }
}

impl std::error::Error for Error {}
