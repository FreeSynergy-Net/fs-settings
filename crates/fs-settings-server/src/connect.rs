//! The CLI's connection to the services: make sure the service runs (fs-rpc `ensure_single` through fs-settings-rpc `ensure_service`),
//! then the clients of part B2. Everything assumed of B2's constructors is in this file (see the report).
//! Pattern: Facade over the clients and `ensure_service` of fs-settings-rpc.

use std::path::Path;
use std::time::Duration;

use fs_rpc::{CommandStarter, Expected, SingleTimes};
use fs_settings_rpc::{ClientTimes, RpcInstanceAdmin, RpcSettingsStore, ensure_service_with};

use crate::ports::{Coded, SettingCode};
use crate::serve::PROGRAM;

/// The limits of the CLI's calls, from one place ("einstellbar statt fest": these are the defaults of the words, not facts).
#[must_use]
pub fn times() -> ClientTimes {
    ClientTimes {
        call: Duration::from_secs(10),
        retry: Duration::from_secs(2),
    }
}

/// Makes sure the service for `socket` runs; starts `fs-settings serve [--instance] --socket S` if not.
///
/// # Errors
/// `store-unreachable{socket, cause}` (from fs-settings-rpc).
pub fn ensure(socket: &Path, instance: bool) -> Result<(), Coded<SettingCode>> {
    let me = std::env::current_exe().map_err(|_| {
        Coded::new(SettingCode::StoreUnreachable)
            .with_text(crate::ports::params::SOCKET, socket.to_string_lossy())
    })?;
    let mut words: Vec<std::ffi::OsString> = vec!["serve".into()];
    if instance {
        words.push("--instance".into());
    }
    words.extend(["--socket".into(), socket.as_os_str().to_owned()]);
    let starter = CommandStarter::new(me.clone(), words);
    let expected = Expected::new(env!("CARGO_PKG_VERSION")).with_program(PROGRAM);
    ensure_service_with(
        socket,
        &expected,
        &starter,
        Some(&me),
        &SingleTimes::default(),
    )
}

/// The client for the program fs-settings itself: every scope.
#[must_use]
pub fn manage(socket: &Path) -> RpcSettingsStore {
    RpcSettingsStore::manage(socket, times())
}

/// The admin client of the instance service. `claim` is the `by` the contract 0.2 still carries: only a CLAIM, which the service logs and
/// ignores (the actor is the identity of the connection). The command line asks nobody and claims nothing (`""`); the contract 0.3 drops `by`.
#[must_use]
pub fn admin(socket: &Path, claim: &str) -> RpcInstanceAdmin {
    RpcInstanceAdmin::open(socket, claim, times())
}
