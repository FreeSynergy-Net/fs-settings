//! The actor of a command that has no connection (`export` and `import` open the database file themselves): the OS user of the process,
//! asked from the operating system (never an argument, never an environment variable: [AKTEUR-AUS-VERBINDUNG]), as the SAME actor IRI the
//! service derives from a connection (`fs_settings_rpc::actor_of`: `urn:fs:os-user:<id>`).
//!
//! Pattern: Adapter (system answer -> `Caller` -> actor), so the command line and the service name one user one way.

use fs_rpc::Caller;

use crate::clock::process_user;
use crate::ports::{Coded, Reference, SettingCode, params};

/// The actor IRI of the user who runs this process.
///
/// # Errors
/// `no-right{key=os-user}` when the system names no user (the command is refused, never run as nobody).
pub fn os_actor() -> Result<Reference, Coded<SettingCode>> {
    let (id, name) = process_user()
        .ok_or_else(|| Coded::new(SettingCode::NoRight).with_text(params::KEY, "os-user"))?;
    Ok(fs_settings_rpc::actor_of(&Caller::new(id, name)))
}
