//! `fs-settings serve`: opens the database (emergency mode when it is broken or newer), runs the legacy import ONCE at a first start of a
//! person database, and serves the contract `settings` until a signal or `Control.stop`.
//!
//! Pattern: Facade. The contract skeleton `SettingsService` comes from fs-settings-rpc (part B2), the transport from fs-rpc; what is
//! assumed of B2 (`into_client`, `ServerIdentity`) is listed in the report and lives only in this file.

use std::path::PathBuf;
use std::sync::Arc;

use fs_rpc::{ServerIdentity, run_until_signal_callers};
use fs_settings_rpc::SettingsService;

use crate::clock::system_clock;
use crate::db_store::DbSettingsStore;
use crate::guarded::{SettingsWithForm, form_core, local_person, person_owner};
use crate::instance_source::NoInstance;
use crate::legacy::{LegacyEnv, LegacyLine, import_legacy};
use crate::ports::{Coded, Health, ProgramName, SettingCode, SettingsStore};
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
    if instance {
        "settings-instance"
    } else {
        "settings"
    }
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
        .ok_or_else(|| {
            Coded::new(SettingCode::NoFolder).with_text(crate::ports::params::WHICH, "socket")
        })
}

/// One report line per old file, for stderr (a code line or `imported`/`kept`).
#[must_use]
pub fn legacy_lines(lines: &[LegacyLine]) -> Vec<String> {
    lines
        .iter()
        .map(|l| match l {
            LegacyLine::Imported { file, key } => {
                format!("legacy imported key={key} file={}", file.display())
            }
            LegacyLine::Kept { file, key } => {
                format!("legacy kept key={key} file={}", file.display())
            }
            LegacyLine::Problem { file, code } => {
                format!("legacy {} file={}", code_line(code), file.display())
            }
        })
        .collect()
}

/// Opens the store of `words` and, at a first start of a person database, imports the old files once.
///
/// # Errors
/// As [`DbSettingsStore::open`].
pub fn open_store(
    words: &ServeWords,
    legacy: &LegacyEnv,
) -> Result<(DbSettingsStore, Vec<LegacyLine>), Coded<SettingCode>> {
    let store = DbSettingsStore::open(
        &words.place,
        words.instance,
        Box::new(NoInstance),
        system_clock(),
    )?;
    let lines = if store.was_created() && !words.instance {
        import_legacy(&store, legacy)
    } else {
        Vec::new()
    };
    Ok((store, lines))
}

/// Where the program's log lines go (standard error in the program, a collector in tests).
pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

/// The log of the program: one line per call on standard error.
#[must_use]
pub fn stderr_log() -> LogSink {
    Arc::new(|line| eprintln!("{line}"))
}

/// The service of the program over its database: a claimed `by` and a refused connection (a caller the system could not name) both go to
/// `log` (the lines are those of fs-settings-rpc); the actor of every request is derived from the connection by the service itself.
/// Both hooks are wired here on purpose, so a program that forgets one cannot be silent: `serve_service` tests them.
#[must_use]
pub fn build_service(
    person: Arc<DbSettingsStore>,
    instance: Option<Arc<DbSettingsStore>>,
    log: LogSink,
) -> SettingsService<DbSettingsStore, DbSettingsStore> {
    // fs-settings-rpc v0.0.8 fails closed (every connection a bot, an empty catalog shows a bot nothing). The transport admits the OS user of
    // this service only, so that user is the person (ADR 0054 open point 1); the host says so on purpose.
    let mut service = SettingsService::new_os_user_is_person(person, instance);
    let claims = Arc::clone(&log);
    service.claimed_by = Some(Arc::new(move |by: &str| {
        claims(&fs_settings_rpc::claim_line(by));
    }));
    service.refused_note = Some(Arc::new(move |code: &str| {
        log(&fs_settings_rpc::refused_line(code));
    }));
    service
}

/// `serve`: `args` are the words after `serve`.
///
/// # Errors
/// [`ServeError`].
pub fn serve(args: &[String]) -> Result<(), ServeError> {
    let parsed =
        Args::parse(args).map_err(|u| ServeError::Usage(format!("usage word={}", u.word)))?;
    if parsed.flag("--help") {
        println!("{HELP}");
        return Ok(());
    }
    let words =
        serve_words(&parsed).map_err(|u| ServeError::Usage(format!("usage word={}", u.word)))?;
    let failed = |c: &Coded<SettingCode>| ServeError::Failed(code_line(c));
    let socket = socket_of(&words).map_err(|c| failed(&c))?;
    let (store, lines) = open_store(&words, &LegacyEnv::from_process()).map_err(|c| failed(&c))?;
    // The OS user of a write is the one the service set for the connection (`current_os_user` during the call): none of our own.
    let store = store.with_os_user(Arc::new(fs_settings_rpc::current_os_user));
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
    // One hub per connection, the actor from the connection's `Caller::id` only; a claimed `by` goes to the log and is ignored
    // (fs-settings-rpc: `SettingsService::callers`). Nothing here names the user: the service asks the system for each connection.
    let service = build_service(
        Arc::clone(&shared),
        words.instance.then(|| Arc::clone(&shared)),
        stderr_log(),
    );
    // The person "me" of the module person, read once and BEFORE the loop (it may start that service); without it only the form source stays off.
    let person = local_person(&*person_owner(std::env::current_exe().ok().as_deref()));
    let program = ProgramName::new(PROGRAM)
        .map_err(|_| ServeError::Failed("serve-failed cause=program-name".into()))?;
    // The core reads the store once: made here, outside the runtime of the loop (the store is sync over a runtime of its own).
    let core = form_core(&service, Arc::clone(&shared), program);
    let identity = ServerIdentity::new(PROGRAM, env!("CARGO_PKG_VERSION"));
    let beat_socket = socket.clone();
    run_until_signal_callers(&socket, identity, move || async move {
        // The heartbeat of the host (`ping`/`save` on `<socket>.beat`), started after the socket is bound and ticked by the loop of
        // THIS runtime: a loop that stands still stops ticking, so the host sees a hang and not a false "alive".
        keep_heartbeat(&beat_socket)?;
        // `Guarded::new` runs inside the `LocalSet` of the loop.
        Ok(SettingsWithForm::new(service, core, person, &|line| {
            eprintln!("{line}");
        }))
    })
    .map_err(|e| ServeError::Failed(format!("serve-failed cause={e}")))
}

/// How often the loop of the service says "I am not stuck" (the host's limit is 10 s).
const TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// Starts the heartbeat next to `socket` and ticks it from a task of the serving runtime; the task (and with it the heartbeat channel)
/// ends with the run.
///
/// # Errors
/// The heartbeat channel could not be opened (another one answers, the system refuses, the system has none): the service does not
/// start without it, never a silent success.
fn keep_heartbeat(socket: &std::path::Path) -> std::io::Result<()> {
    let beat = fs_rpc::Heartbeat::start(socket)?;
    tokio::task::spawn_local(async move {
        let mut every = tokio::time::interval(TICK);
        loop {
            every.tick().await;
            beat.tick();
        }
    });
    Ok(())
}
