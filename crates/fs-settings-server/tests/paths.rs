//! KC1: where the database lies. Pure function with an injected environment, so the Windows places are tested on every system.

use std::path::PathBuf;

use fs_settings::SettingCode;
use fs_settings_server::{DbPlace, Env, Os, resolve_in};

mod support;
use support::text_of;

fn env() -> Env {
    Env {
        xdg_data_home: Some("/xdg".to_owned()),
        home: Some("/home/ada".to_owned()),
        local_app_data: Some("C:\\Users\\ada\\AppData\\Local".to_owned()),
        program_data: Some("C:\\ProgramData".to_owned()),
    }
}

/// KC1: `--settings F` is exactly `F`; `--data D` is `D/settings.db`.
#[test]
fn kc1_start_words_win() {
    let f = PathBuf::from("elsewhere").join("my.db");
    assert_eq!(resolve_in(&DbPlace::File(f.clone()), Os::Unix, &env()).unwrap(), f);
    let d = PathBuf::from("data-dir");
    assert_eq!(resolve_in(&DbPlace::Dir(d.clone()), Os::Unix, &Env::default()).unwrap(), d.join("settings.db"));
}

/// KC1: the person's convention on Unix: absolute `XDG_DATA_HOME`, else `HOME/.local/share`, relative XDG ignored, nothing -> `no-folder`.
#[test]
fn kc1_person_unix() {
    let place = DbPlace::Convention { instance: false };
    let ok = |e: &Env| resolve_in(&place, Os::Unix, e).unwrap().to_string_lossy().replace('\\', "/");
    assert_eq!(ok(&env()), "/xdg/freesynergy/settings/settings.db");
    let relative = Env { xdg_data_home: Some("rel/xdg".to_owned()), ..env() };
    assert_eq!(ok(&relative), "/home/ada/.local/share/freesynergy/settings/settings.db");
    let none = Env { xdg_data_home: None, home: None, ..env() };
    let err = resolve_in(&place, Os::Unix, &none).unwrap_err();
    assert_eq!(err.code(), SettingCode::NoFolder);
    assert_eq!(text_of(&err, "which").as_deref(), Some("data"));
}

/// KC1: the instance place on Unix is fixed.
#[test]
fn kc1_instance_unix() {
    let place = DbPlace::Convention { instance: true };
    let got = resolve_in(&place, Os::Unix, &Env::default()).unwrap();
    assert_eq!(got.to_string_lossy().replace('\\', "/"), "/var/lib/freesynergy/instance/settings.db");
}

/// KC1: the Windows places (`%LOCALAPPDATA%\FreeSynergy\settings`, `%ProgramData%\FreeSynergy\instance`), as a pure function.
#[test]
fn kc1_windows_places() {
    let person = resolve_in(&DbPlace::Convention { instance: false }, Os::Windows, &env()).unwrap();
    assert_eq!(person.to_string_lossy(), "C:\\Users\\ada\\AppData\\Local\\FreeSynergy\\settings\\settings.db");
    let instance = resolve_in(&DbPlace::Convention { instance: true }, Os::Windows, &env()).unwrap();
    assert_eq!(instance.to_string_lossy(), "C:\\ProgramData\\FreeSynergy\\instance\\settings.db");
    let none = Env { local_app_data: None, ..env() };
    assert_eq!(resolve_in(&DbPlace::Convention { instance: false }, Os::Windows, &none).unwrap_err().code(), SettingCode::NoFolder);
}
