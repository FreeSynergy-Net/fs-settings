//! The CLI's connection to the services: make sure the service runs (fs-rpc `ensure_single` through fs-settings-rpc `ensure_service`),
//! then the clients of part B2. Everything assumed of B2's constructors is in this file (see the report).

use std::path::Path;
use std::time::Duration;

use fs_rpc::{CommandStarter, Expected, SingleTimes};
use fs_settings_rpc::{ClientTimes, RpcInstanceAdmin, RpcSettingsStore, ensure_service};

use crate::ports::{Coded, Reference, SettingCode};
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
    let starter = CommandStarter::new(me, words);
    let expected = Expected::new(env!("CARGO_PKG_VERSION")).with_program(PROGRAM);
    ensure_service(socket, &expected, &starter, &SingleTimes::default())
}

/// The client for the program fs-settings itself: every scope.
#[must_use]
pub fn manage(socket: &Path) -> RpcSettingsStore {
    RpcSettingsStore::manage(socket, times())
}

/// The admin client of the instance service for the actor `by`.
#[must_use]
pub fn admin(socket: &Path, by: &Reference) -> RpcInstanceAdmin {
    RpcInstanceAdmin::open(socket, by, times())
}
