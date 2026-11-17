//! `fs-settings`: the one binary of the program (ADR 0063). Pattern: Facade over `fs-settings-server`.
//!
//! ```text
//! fs-settings                     make sure the settings service runs
//! fs-settings serve [options]     the service (owns the database)
//! fs-settings instance ...        instance defaults and locks (admin script, reason mandatory)
//! fs-settings export | import | repair
//! fs-settings --version | --help
//! ```

fn main() -> std::process::ExitCode {
    let words: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|w| w.to_string_lossy().into_owned())
        .collect();
    let code = fs_settings_server::cli::run(&words);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}
