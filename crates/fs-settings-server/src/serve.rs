//! `fs-settings serve`: opens the database (emergency mode when it is broken or newer), runs the legacy import ONCE at a first start of a
//! person database, and serves the contract `settings` until a signal or `Control.stop`.
//!
//! Pattern: Facade. The contract skeleton `SettingsService` comes from fs-settings-rpc (part B2), the transport from fs-rpc; what is
//! assumed of B2 (`into_client`, `ServerIdentity`) is listed in the report and lives only in this file.

use std::path::PathBuf;
use std::sync::Arc;

use fs_rpc::{ServerIdentity, run_until_signal_as};
use fs_settings_rpc::SettingsService;

use crate::clock::{env_os_user, system_clock};
use crate::db_store::DbSettingsStore;
use crate::instance_source::NoInstance;
use crate::legacy::{LegacyEnv, LegacyLine, import_legacy};
use crate::ports::{Coded, Health, SettingCode, SettingsStore};
use crate::reply::code_line;
use crate::words::{Args, ServeWords, serve_words};

/// The program name `Control.version` carries.
pub const PROGRAM: &str = "fs-settings";

/// The help text of `serve`.
pub const HELP: &str = "fs-settings serve [--instance] [--data DIR | --settings FILE] [--socket PATH]\n\
Defaults: the person's database at $XDG_DATA_HOME/freesynergy/settings/settings.db (Windows %LOCALAPPDATA%\\FreeSynergy\\settings),\n\
with --instance /var/lib/freesynergy/instance/settings.db (Windows %ProgramData%\\FreeSynergy\\instance); the socket settings.sock.\n\
A broken or newer database is not opened: the service answers the built-in defaults and says why.";

/// Why `serve` ended without serving.
#[derive(Debug, PartialEq, Eq)]
pub enum ServeError {
    /// The words were wrong (exit 2); the text is `usage word=<word>`.
    Usage(String),
    /// Could not start or ended with an error (exit 1); the text is a code line.
    Failed(String),
}

/// The socket name of the person service and of the instance service.
#[must_use]
pub fn socket_program(instance: bool) -> &'static str {
    if instance { "settings-instance" } else { "settings" }
}

/// The socket: `--socket` or the convention of fs-rpc.
///
/// # Errors
/// `no-folder{which=socket}` when neither is available.
pub fn socket_of(words: &ServeWords) -> Result<PathBuf, Coded<SettingCode>> {
    words
        .socket
        .clone()
        .or_else(|| fs_rpc::default_socket(socket_program(words.instance)))
        .ok_or_else(|| Coded::new(SettingCode::NoFolder).with_text(crate::ports::params::WHICH, "socket"))
}

/// One report line per old file, for stderr (a code line or `imported`/`kept`).
#[must_use]
pub fn legacy_lines(lines: &[LegacyLine]) -> Vec<String> {
    lines
        .iter()
        .map(|l| match l {
            LegacyLine::Imported { file, key } => format!("legacy imported key={key} file={}", file.display()),
            LegacyLine::Kept { file, key } => format!("legacy kept key={key} file={}", file.display()),
            LegacyLine::Problem { file, code } => format!("legacy {} file={}", code_line(code), file.display()),
        })
        .collect()
}

/// Opens the store of `words` and, at a first start of a person database, imports the old files once.
///
/// # Errors
/// As [`DbSettingsStore::open`].
pub fn open_store(words: &ServeWords, legacy: &LegacyEnv) -> Result<(DbSettingsStore, Vec<LegacyLine>), Coded<SettingCode>> {
    let store = DbSettingsStore::open(&words.place, words.instance, Box::new(NoInstance), system_clock())?;
    let lines = if store.was_created() && !words.instance { import_legacy(&store, legacy) } else { Vec::new() };
    Ok((store, lines))
}

/// `serve`: `args` are the words after `serve`.
///
/// # Errors
/// [`ServeError`].
pub fn serve(args: &[String]) -> Result<(), ServeError> {
    let parsed = Args::parse(args).map_err(|u| ServeError::Usage(format!("usage word={}", u.word)))?;
    if parsed.flag("--help") {
        println!("{HELP}");
        return Ok(());
    }
    let words = serve_words(&parsed).map_err(|u| ServeError::Usage(format!("usage word={}", u.word)))?;
    let failed = |c: &Coded<SettingCode>| ServeError::Failed(code_line(c));
    let socket = socket_of(&words).map_err(|c| failed(&c))?;
    let (store, lines) = open_store(&words, &LegacyEnv::from_process()).map_err(|c| failed(&c))?;
    for line in legacy_lines(&lines) {
        eprintln!("fs-settings: {line}");
    }
    if let Health::Defaults(codes) = store.health() {
        // Never silent: the service runs on the built-in defaults and says why, here and to every client.
        for code in &codes {
            eprintln!("fs-settings: {}", code_line(code));
        }
    }
    let shared = Arc::new(store);
    let service = SettingsService {
        person: Arc::clone(&shared),
        instance: words.instance.then(|| Arc::clone(&shared)),
        os_user: env_os_user(),
        now: system_clock(),
    };
    let identity = ServerIdentity::new(PROGRAM, env!("CARGO_PKG_VERSION"));
    run_until_signal_as(&socket, identity, move || async move { Ok(service.into_client()) })
        .map_err(|e| ServeError::Failed(format!("serve-failed cause={e}")))
}
