//! KC8 to KC10: export and import (TOML and JSON), the round trip, errors with a line, a refused key, and an import cut in the middle.

use fs_settings::{Place, ProfileRef, ProgramName, SettingCode, SettingsStore};
use fs_settings_server::exchange::{ExportScope, Format, ImportEnv, ImportMode, ImportReport, export, import, import_with};
use fs_settings_server::{DbSettingsStore, SettingCode as Code};

mod support;
use support::{at, db_file, iri, key, open, open_instance, program, raw_str, text_of};

fn exported(store: &DbSettingsStore, format: Format) -> String {
    let mut out = Vec::new();
    let programs = vec![ProgramName::new("tasks").unwrap()];
    export(store, &ExportScope::All(programs), true, format, std::path::Path::new("out"), &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn env<'a>(by: Option<&'a fs_types::Reference>) -> ImportEnv<'a> {
    ImportEnv { catalog: None, by, now: at(1_700_000_000_000), device: true }
}

fn fill(store: &DbSettingsStore) {
    let who = fs_settings::Stampd { by: iri("https://example.org/admin"), at: at(1) };
    store.write(&fs_settings::Scope::General, Place::Profile, 0, &key("fs.shell.language"), Some(&raw_str("de"))).unwrap();
    store.write(&program("tasks"), Place::Profile, 0, &key("fs.shell.frontend"), Some(&raw_str("day"))).unwrap();
    store.write(&program("tasks"), Place::Device, 0, &key("fs.shell.scheme"), Some(&raw_str("dark"))).unwrap();
    store
        .update_index(&mut |set| {
            *set.get_mut(&ProfileRef::Base).unwrap() = 1;
            set.create(fs_settings::ProfileName::new("Arbeit").unwrap(), None)?;
            *set.get_mut(&ProfileRef::Named(fs_settings::ProfileName::new("Arbeit").unwrap())).unwrap() = 1;
            Ok(())
        })
        .unwrap();
    store.write(&fs_settings::Scope::General, Place::Profile, 1, &key("fs.shell.language"), Some(&raw_str("en"))).unwrap();
    fs_settings::InstanceStore::write_instance(store, &key("fs.shell.style"), Some(&raw_str("calm")), &who).unwrap();
}

fn rows_of(store: &DbSettingsStore) -> Vec<String> {
    let mut rows = Vec::new();
    for profile in [0_u64, 1] {
        for place in [Place::Profile, Place::Device] {
            for scope in [fs_settings::Scope::General, program("tasks")] {
                for (k, v) in store.profile(&scope, place, profile).values {
                    rows.push(format!("{profile} {place:?} {scope:?} {} {v:?}", k.as_str()));
                }
            }
        }
    }
    rows.sort();
    rows
}

/// KC8: DB -> export -> empty DB -> import -> export: the same bytes, the same rows (the time of change aside), for TOML and for JSON.
#[test]
fn kc8_round_trip() {
    for format in [Format::Toml, Format::Json] {
        let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (a, b) = (open_instance(&db_file(one.path()), 10), open_instance(&db_file(two.path()), 20));
        fill(&a);
        let text = exported(&a, format);
        assert_eq!(text, exported(&a, format), "deterministic: the same DB gives the same bytes");
        let by = iri("https://example.org/admin");
        let report = import(&b, Some(&b), &mut text.as_bytes(), std::path::Path::new("in"), format, ImportMode::Apply, &env(Some(&by))).unwrap();
        assert!(report.refused.is_empty(), "{:?}", report.refused);
        assert_eq!(exported(&b, format), text, "{format:?}");
        assert_eq!(rows_of(&b), rows_of(&a));
        assert_eq!(b.instance().values.values, a.instance().values.values);
        let again = import(&b, Some(&b), &mut text.as_bytes(), std::path::Path::new("in"), format, ImportMode::Apply, &env(Some(&by))).unwrap();
        assert_eq!((again.applied, again.refused.len()), (0, 0), "the second import finds everything there");
    }
}

/// KC9: a broken text names the line and changes nothing; `--dry-run` only reports; a locked key is refused alone and the rest goes in.
#[test]
fn kc9_errors_dry_run_and_locks() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_instance(&db_file(dir.path()), 10);
    let stamp = store.stamp();
    let broken = "[base.values.general]\n\"fs.shell.language\" = \"de\"\nthis is = not toml\n";
    let err = import(&store, None, &mut broken.as_bytes(), std::path::Path::new("in.toml"), Format::Toml, ImportMode::Apply, &env(None)).unwrap_err();
    assert_eq!(err.code(), SettingCode::ImportInvalid);
    assert_eq!(text_of(&err, "file").as_deref(), Some("in.toml"));
    assert!(err.params().any(|(k, v)| k == "line" && matches!(v, fs_settings::Param::Number(3))), "{err:?}");
    assert_eq!(store.stamp(), stamp);

    let good = "[base.values.general]\n\"fs.shell.language\" = \"de\"\n\"fs.shell.style\" = \"calm\"\n";
    let dry = import(&store, None, &mut good.as_bytes(), std::path::Path::new("g"), Format::Toml, ImportMode::DryRun, &env(None)).unwrap();
    assert_eq!(dry.applied, 2);
    assert_eq!(store.stamp(), stamp, "a dry run writes nothing");

    let by = iri("https://example.org/admin");
    let who = fs_settings::Stampd { by, at: at(2) };
    let lock = fs_settings::LockChange::Lock { value: fs_settings::Raw::Str("calm".to_owned()), reason: "policy".to_owned() };
    fs_settings::InstanceStore::write_lock(&store, &key("fs.shell.style"), &lock, &who).unwrap();
    let report: ImportReport = import(&store, None, &mut good.as_bytes(), std::path::Path::new("g"), Format::Toml, ImportMode::Apply, &env(None)).unwrap();
    assert_eq!(report.applied, 1, "the rest goes in");
    assert_eq!(report.refused.len(), 1);
    assert_eq!(report.refused[0].code(), Code::ImportConflict);
    assert_eq!(text_of(&report.refused[0], "cause").as_deref(), Some("locked"));
    assert_eq!(store.profile(&fs_settings::Scope::General, Place::Profile, 0).values.len(), 1);
}

/// KC10: a fault after the first write undoes everything: the values are as before and the created profile is gone again.
#[test]
fn kc10_a_fault_in_the_middle_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&db_file(dir.path()));
    let text = "[base.values.general]\n\"fs.shell.language\" = \"de\"\n\"fs.shell.style\" = \"calm\"\n[profiles.Arbeit.values.general]\n\"fs.shell.language\" = \"en\"\n";
    let mut step = |i: usize| if i == 1 { Err(fs_settings::Coded::new(SettingCode::WriteFailed)) } else { Ok(()) };
    let err = import_with(&store, None, &mut text.as_bytes(), std::path::Path::new("in"), Format::Toml, ImportMode::Apply, &env(None), &mut step).unwrap_err();
    assert_eq!(err.code(), SettingCode::WriteFailed);
    assert!(store.profile(&fs_settings::Scope::General, Place::Profile, 0).values.is_empty());
    assert_eq!(store.index().0.names().count(), 0, "the profile created for the import is deleted again");
}
