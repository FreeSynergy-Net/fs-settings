//! `fs-settings-server`: the service of the program `fs-settings`. It owns the settings database (fs-objects `v5::setting`), implements
//! the port of `fs-settings` over it ([`DbSettingsStore`]), serves the contract `settings` (through fs-settings-rpc) and brings the
//! words of the command: `serve`, `instance`, `export`, `import`, `repair`.
//!
//! Rules: a broken or newer file is never opened, changed or deleted (emergency mode says why, openly); the only new file after a
//! broken one comes from `repair`, run by the person; instance changes need an actor and a reason where the words say so; time and
//! OS user are set by the service; TOML and JSON are for exchange only; the old published files are read once and never written.
//!
//! Status: probe, not compiled when written (see the README). Pattern: Adapter + Facade + Command.

#![deny(missing_docs)]
#![warn(clippy::pedantic)]

pub mod cli;
mod cli_files;
mod clock;
mod connect;
mod db_index;
mod db_instance;
mod db_port;
mod db_store;
mod error_map;
pub mod exchange;
mod instance_cmd;
mod instance_source;
mod legacy;
mod paths;
mod ports;
mod prim;
mod repair;
mod reply;
mod serve;
mod words;

pub use clock::{Clock, OsUser, env_os_user, fixed_clock, system_clock};
pub use db_index::{changes, to_set};
pub use db_store::DbSettingsStore;
pub use instance_cmd::{id_text, infer_raw, run_instance};
pub use instance_source::{InstanceSource, NoInstance};
pub use legacy::{LegacyEnv, LegacyLine, import_legacy};
pub use paths::{DB_NAME, DbPlace, Env, Os, resolve, resolve_in};
pub use ports::{Coded, SettingCode};
pub use repair::{RepairReport, repair};
pub use reply::{Reply, code_line};
pub use serve::{HELP, PROGRAM, ServeError, legacy_lines, open_store, serve, socket_of};
pub use words::{Args, InstanceCmd, ServeWords, Usage, instance_cmd, serve_words};
