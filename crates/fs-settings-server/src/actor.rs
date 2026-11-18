//! The actor of a command: the OS user of the CALLER, asked from the operating system (never an argument, never an environment variable:
//! [AKTEUR-AUS-VERBINDUNG]). As an IRI: `urn:fs:os-user:<name>` (characters outside `A-Za-z0-9._-` are percent-encoded).
//!
//! Pattern: Factory (the one place that makes the actor IRI of a command line).

use crate::clock::system_os_user;
use crate::ports::{BasicValue, Coded, Reference, SettingCode, params};

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

/// The actor IRI of the user who runs this process.
///
/// # Errors
/// `no-right{key=os-user}` when the system names no user (the command is refused, never run as nobody).
pub fn os_actor() -> Result<Reference, Coded<SettingCode>> {
    let refused = || Coded::new(SettingCode::NoRight).with_text(params::KEY, "os-user");
    let name = system_os_user()().ok_or_else(refused)?;
    Reference::parse(&format!("urn:fs:os-user:{}", encode(&name))).map_err(|_| refused())
}
