//! KC11: the published old files are read once, never written, never deleted; what is not there yet is taken, what is there stays.

use fs_settings::{Place, Scope, SettingsStore};
use fs_settings_server::{LegacyEnv, LegacyLine, import_legacy};

mod support;
use support::{db_file, key, open, program, raw_str};

fn put(path: &std::path::Path, text: &str) -> Vec<u8> {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
    std::fs::read(path).unwrap()
}

/// KC11: language `de` (general) and `en` (tasks) become `fs.shell.language`; an existing key is not overwritten; `settings.json`
/// `{"face":"day"}` becomes `fs.shell.frontend` of `tasks`; a broken JSON is said (`file-broken`); every old file is byte-identical.
#[test]
fn kc11_legacy_files() {
    let dir = tempfile::tempdir().unwrap();
    let (config, data) = (dir.path().join("config"), dir.path().join("data"));
    let general = config.join("freesynergy").join("language");
    let tasks = config.join("freesynergy").join("tasks").join("language");
    let json = data.join("freesynergy").join("tasks").join("settings.json");
    let kept = config.join("freesynergy").join("calendars").join("language");
    let before = [put(&general, "de\n"), put(&tasks, "en"), put(&json, "{\"face\":\"day\"}"), put(&kept, "fr")];
    let store = open(&db_file(&dir.path().join("db")));
    // the calendars program has its language already: it must stay
    let lang = key("fs.shell.language");
    store.write(&program("calendars"), Place::Profile, 0, &lang, Some(&raw_str("nl"))).unwrap();

    let env = LegacyEnv { config_home: Some(config), data_home: Some(data) };
    let lines = import_legacy(&store, &env);
    let imported = lines.iter().filter(|l| matches!(l, LegacyLine::Imported { .. })).count();
    assert_eq!(imported, 3, "{lines:?}");
    assert!(lines.iter().any(|l| matches!(l, LegacyLine::Kept { key, .. } if key == "fs.shell.language")));
    assert_eq!(store.profile(&Scope::General, Place::Profile, 0).values.get(&lang), Some(&raw_str("de")));
    assert_eq!(store.profile(&program("tasks"), Place::Profile, 0).values.get(&lang), Some(&raw_str("en")));
    assert_eq!(store.profile(&program("calendars"), Place::Profile, 0).values.get(&lang), Some(&raw_str("nl")));
    assert_eq!(
        store.profile(&program("tasks"), Place::Profile, 0).values.get(&key("fs.shell.frontend")),
        Some(&raw_str("day"))
    );
    let after = [&general, &tasks, &json, &kept].map(|p| std::fs::read(p).unwrap());
    assert_eq!(before.to_vec(), after.to_vec(), "no old file was written");

    // a broken JSON is a line in the report, never silent
    put(&json, "{not json");
    let other = open(&db_file(&dir.path().join("db2")));
    let lines = import_legacy(&other, &env);
    assert!(lines.iter().any(|l| matches!(l, LegacyLine::Problem { code, .. } if code.code() == fs_settings::SettingCode::FileBroken)));
}
