//! The exchange document and its two writers/readers. Value objects with sorted maps (deterministic bytes).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Format;
use crate::ports::Raw;

/// A value: yes/no, whole number, text (the three storage classes, ADR 0086; no JSON blob).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum V {
    /// yes/no.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// Text.
    Str(String),
}

impl From<&Raw> for V {
    fn from(raw: &Raw) -> Self {
        match raw {
            Raw::Bool(b) => Self::Bool(*b),
            Raw::Int(n) => Self::Int(*n),
            Raw::Str(s) => Self::Str(s.clone()),
        }
    }
}

impl From<&V> for Raw {
    fn from(v: &V) -> Self {
        match v {
            V::Bool(b) => Self::Bool(*b),
            V::Int(n) => Self::Int(*n),
            V::Str(s) => Self::Str(s.clone()),
        }
    }
}

/// `key = value` pairs.
pub type Map = BTreeMap<String, V>;

/// The values of one place: general and per program.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scopes {
    /// Settings of all programs.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub general: Map,
    /// Settings of one program each.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub programs: BTreeMap<String, Map>,
}

impl Scopes {
    /// Nothing in it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.general.is_empty() && self.programs.is_empty()
    }
}

/// One profile: its values and (only with `--device`) its device values.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    /// General and program values.
    #[serde(default, skip_serializing_if = "Scopes::is_empty")]
    pub values: Scopes,
    /// The device values.
    #[serde(default, skip_serializing_if = "Scopes::is_empty")]
    pub device: Scopes,
}

impl Body {
    /// Nothing in it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.device.is_empty()
    }
}

/// A lock: why, by whom (an IRI), when (RFC 3339, UTC).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockDoc {
    /// The reason.
    pub reason: String,
    /// Who locked (IRI).
    pub by: String,
    /// When (UTC).
    pub at: String,
}

/// The whole text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Doc {
    /// The base profile.
    #[serde(default, skip_serializing_if = "Body::is_empty")]
    pub base: Body,
    /// Named profiles by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, Body>,
    /// The instance defaults.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub instance: Map,
    /// The instance locks by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub locks: BTreeMap<String, LockDoc>,
}

impl Doc {
    /// The text of this document: deterministic (sorted maps).
    ///
    /// # Errors
    /// The writer's own message (never in practice: every value is a primitive).
    pub fn write(&self, format: Format) -> Result<String, String> {
        match format {
            Format::Toml => toml::to_string(self).map_err(|e| e.to_string()),
            Format::Json => serde_json::to_string_pretty(self)
                .map(|mut t| {
                    t.push('\n');
                    t
                })
                .map_err(|e| e.to_string()),
        }
    }

    /// Reads `text`. The error carries the line (1-based; 0 when the reader names none) and a short cause.
    ///
    /// # Errors
    /// `(line, cause)`.
    pub fn read(text: &str, format: Format) -> Result<Self, (u64, String)> {
        match format {
            Format::Toml => toml::from_str(text).map_err(|e| {
                let line = e.span().map_or(0, |s| {
                    1 + text[..s.start.min(text.len())]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                });
                (u64::try_from(line).unwrap_or(0), e.message().to_owned())
            }),
            Format::Json => serde_json::from_str(text)
                .map_err(|e| (u64::try_from(e.line()).unwrap_or(0), e.to_string())),
        }
    }
}
