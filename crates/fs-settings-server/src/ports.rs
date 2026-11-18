//! Everything this crate takes from the libraries below it, in ONE place (Facade). The three libraries (fs-objects part A,
//! fs-settings part B1, fs-settings-rpc part B2) were not compiled when this was written: a path that differs at the probe compile is
//! fixed here and nowhere else.
//! Pattern: Facade (all imports from the libraries below in one place).

pub use fs_objects::v5::setting::{
    Actor, DOMAIN, IndexChange, IndexRows, Outcome, PlaceRow, Prim, ScopeRow, SettingRows, Writer,
};
pub use fs_objects::v5::{Error as RowsError, Inspect, Store};
pub use fs_settings::coded_stub::Param;
pub use fs_settings::{
    Catalog, Coded, Health, Instance, InstanceLogEntry, InstanceStore, Layer, Lock, LockChange,
    Place, ProfileCode, ProfileName, ProfileRef, ProfileSet, ProgramName, Raw, Scope, SettingCode,
    SettingKey, SettingsStore, Snapshot, Stamp, Stampd, index_step_ok, keys,
};
pub use fs_types::{BasicValue, ObjectId, Reference, Utc};

/// `key=value` text parameters, `kind=...`: the names of the parameters of the codes (one spelling).
pub mod params {
    pub use fs_settings::keys::{BY, CAUSE, FILE, KEY, LINE, REASON, WHICH};
    /// The kind of a failed export.
    pub const KIND: &str = "kind";
    /// Found / known step counts of `store-newer`.
    pub const FOUND: &str = "found";
    /// See `FOUND`.
    pub const KNOWN: &str = "known";
    /// The socket of `store-unreachable`.
    pub const SOCKET: &str = "socket";
}
