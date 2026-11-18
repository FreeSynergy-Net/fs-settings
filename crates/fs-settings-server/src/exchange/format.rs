//! The text format of an exchange file.
//!
//! Pattern: Value object.

/// The text format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// TOML 1.0.
    Toml,
    /// JSON (RFC 8259).
    Json,
}

impl Format {
    /// `toml` | `json`.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "toml" => Some(Self::Toml),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}
