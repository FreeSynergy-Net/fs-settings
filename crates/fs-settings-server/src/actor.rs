//! The actor of a command that has no connection (`export` and `import` open the database file themselves): the OS user of the process,
//! asked from the operating system (never an argument, never an environment variable: [AKTEUR-AUS-VERBINDUNG]), as the SAME actor IRI the
//! service derives from a connection (`fs_settings_rpc::actor_of`: `urn:fs:os-user:<id>`).
//!
//! Pattern: Adapter (system answer -> `Caller` -> actor), so the command line and the service name one user one way.

use fs_rpc::Caller;

use crate::clock::process_user;
use crate::ports::{Coded, Reference, SettingCode, params};

/// The actor IRI of a user: `urn:fs:os-user:<id>` from the ID (Unix: the uid, Windows: the SID), the one form the service derives from a
/// connection (`fs_settings_rpc::actor_of`). The NAME is display only: it may hold spaces, may be missing, and is never the actor.
#[must_use]
pub fn actor_of_user(id: impl Into<String>, name: Option<String>) -> Reference {
    fs_settings_rpc::actor_of(&Caller::new(id, name))
}

/// The actor IRI of a user the system named: `(id, name)` as `process_user` gives it (the PURE part of [`os_actor`], so a test can hand it
/// any user, also one without a passwd line). A user with no name keeps its id as the actor; only "no user at all" is refused.
///
/// # Errors
/// `no-right{key=os-user}` when the system named no user (the command is refused, never run as nobody).
pub fn os_actor_of(
    user: Option<(String, Option<String>)>,
) -> Result<Reference, Coded<SettingCode>> {
    let (id, name) =
        user.ok_or_else(|| Coded::new(SettingCode::NoRight).with_text(params::KEY, "os-user"))?;
    Ok(actor_of_user(id, name))
}

/// The actor IRI of the user who runs this process.
///
/// # Errors
/// `no-right{key=os-user}` when the system names no user (the command is refused, never run as nobody).
pub fn os_actor() -> Result<Reference, Coded<SettingCode>> {
    os_actor_of(process_user())
}
