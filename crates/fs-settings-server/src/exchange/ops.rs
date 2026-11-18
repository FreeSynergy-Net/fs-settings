//! The writes of an import, as plain values: what `DbSettingsStore::apply_ops` runs in ONE transaction.
//!
//! Pattern: Command list (the plan builds it, the database runs it).

use crate::ports::{Place, ProfileName, Raw, Scope, SettingKey};

/// Whose profile a value goes into.
#[derive(Clone, Debug)]
pub enum Target {
    /// The base profile.
    Base,
    /// A named profile (created in the same transaction when it does not exist).
    Named(ProfileName),
}

/// One write.
#[derive(Clone, Debug)]
pub enum Op {
    /// A person value.
    Value {
        /// The profile.
        target: Target,
        /// General or one program.
        scope: Scope,
        /// Profile or device.
        place: Place,
        /// The key.
        key: SettingKey,
        /// The value.
        raw: Raw,
    },
    /// An instance default (needs an actor).
    Instance {
        /// The key.
        key: SettingKey,
        /// The value.
        raw: Raw,
    },
    /// An instance default and its lock (needs an actor).
    Lock {
        /// The key.
        key: SettingKey,
        /// The value.
        raw: Raw,
        /// Why.
        reason: String,
    },
}
