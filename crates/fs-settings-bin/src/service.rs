//! `service`: the same program under the file name the host looks for, `<modules>/settings/service` (`service serve`, see ADR 0097 and
//! the layout of the starter). The words are those of `fs-settings`; nothing else lives here (one code, one place).
//! Pattern: Facade over `fs-settings-server`.

fn main() -> std::process::ExitCode {
    let words: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|w| w.to_string_lossy().into_owned())
        .collect();
    let code = fs_settings_server::cli::run(&words);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}
