//! The actor of a command: the OS user of the CALLER, asked from the operating system (never an argument, never an environment variable:
//! [AKTEUR-AUS-VERBINDUNG]). As an IRI: `urn:fs:os-user:<name>` (characters outside `A-Za-z0-9._-` are percent-encoded).
//!
//! Pattern: Factory (the one place that makes the actor IRI of a command line).

#[cfg(not(unix))]
use crate::clock::system_os_user;
use crate::ports::{BasicValue, Coded, Reference, SettingCode};
#[cfg(not(unix))]
use crate::ports::params;

fn encode(name: &str) -> String {
    use std::fmt::Write;
    name.bytes().fold(String::new(), |mut s, b| {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            s.push(char::from(b));
        } else {
            let _ = write!(s, "%{b:02X}");
        }
        s
    })
}

/// The actor IRI of the user who runs this process: on Unix the one of fs-settings-rpc (`actor_of_os_user`), the service's own form.
///
/// # Errors
/// `no-right{key=os-user}` when the system names no user (the command is refused, never run as nobody).
pub fn os_actor() -> Result<Reference, Coded<SettingCode>> {
    #[cfg(unix)]
    {
        Ok(fs_settings_rpc::actor_of_os_user())
    }
    #[cfg(not(unix))]
    {
        let refused = || Coded::new(SettingCode::NoRight).with_text(params::KEY, "os-user");
        let name = system_os_user()().ok_or_else(refused)?;
        Reference::parse(&format!("urn:fs:os-user:{}", encode(&name))).map_err(|_| refused())
    }
}

/// The actor of a connection: `urn:fs:os-user:<Caller::id>`, from the id the system gave for the connection and NOTHING else (not the
/// name, which is for display only and may not exist; not a claim of the client).
#[must_use]
pub fn actor_of_caller(caller: &fs_rpc::Caller) -> Reference {
    Reference::parse(&format!("urn:fs:os-user:{}", encode(caller.id()))).unwrap_or_else(|_| {
        unreachable!("`urn:fs:os-user:` and percent-encoded text is a valid IRI")
    })
}

/// What the log shows as the OS user of a connection: the name for display, else the id.
#[must_use]
pub fn os_user_label(caller: &fs_rpc::Caller) -> String {
    caller.name().unwrap_or(caller.id()).to_owned()
}
