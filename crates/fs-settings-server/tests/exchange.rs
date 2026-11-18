//! KC8 to KC10: export and import (TOML and JSON), the round trip, errors with a line, a refused key, and a batch that fails in the
//! middle (all in one transaction: nothing is written).

use std::path::Path;

use fs_settings::{
    InstanceStore, LockChange, Place, ProfileName, ProfileRef, SettingCode, SettingsStore, Stampd,
};
use fs_settings_server::exchange::{
    ExportScope, Format, ImportEnv, ImportMode, Op, Target, export, import,
};
use fs_settings_server::{DbSettingsStore, SettingCode as Code};

mod support;
use support::{at, db_file, iri, key, open_instance, program, raw_str, text_of};

fn exported(store: &DbSettingsStore, format: Format) -> String {
    let mut out = Vec::new();
    let programs = store.known_programs().unwrap();
    export(
        store,
        &ExportScope::All(programs),
        true,
        format,
        Path::new("out"),
        &mut out,
    )
    .unwrap();
    String::from_utf8(out).unwrap()
}

fn env(by: Option<&fs_types::Reference>) -> ImportEnv<'_> {
    ImportEnv {
        catalog: None,
        by,
        device: true,
    }
}

fn fill(store: &DbSettingsStore) {
    let who = Stampd {
        by: iri("https://example.org/admin"),
        at: at(1),
    };
    let put = |scope: &fs_settings::Scope, place, profile, k: &str, v: &str| {
        store
            .write(scope, place, profile, &key(k), Some(&raw_str(v)))
            .unwrap();
    };
    put(
        &fs_settings::Scope::General,
        Place::Profile,
        0,
        "fs.shell.language",
        "de",
    );
    put(
        &program("tasks"),
        Place::Profile,
        0,
        "fs.shell.frontend",
        "day",
    );
    put(
        &program("tasks"),
        Place::Device,
        0,
        "fs.shell.scheme",
        "dark",
    );
    store
        .update_index(&mut |set| {
            let name = ProfileName::new("Arbeit").unwrap();
            *set.get_mut(&ProfileRef::Base).unwrap() = 1;
            set.create(name.clone(), None)?;
            *set.get_mut(&ProfileRef::Named(name)).unwrap() = 1;
            Ok(())
        })
        .unwrap();
    put(
        &fs_settings::Scope::General,
        Place::Profile,
        1,
        "fs.shell.language",
        "en",
    );
    store
        .write_instance(&key("fs.shell.style"), Some(&raw_str("calm")), &who)
        .unwrap();
}

fn rows_of(store: &DbSettingsStore) -> Vec<String> {
    let mut rows = Vec::new();
    for profile in [0_u64, 1] {
        for place in [Place::Profile, Place::Device] {
            for scope in [fs_settings::Scope::General, program("tasks")] {
                for (k, v) in store.profile(&scope, place, profile).values {
                    rows.push(format!(
                        "{profile} {place:?} {scope:?} {} {v:?}",
                        k.as_str()
                    ));
                }
            }
        }
    }
    rows.sort();
    rows
}

/// KC8: DB -> export (the programs from `SettingRows::programs`) -> empty DB -> import (one transaction) -> export: the same bytes, the
/// same rows, for TOML and for JSON.
#[test]
fn kc8_round_trip() {
    for format in [Format::Toml, Format::Json] {
        let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (a, b) = (
            open_instance(&db_file(one.path()), 10),
            open_instance(&db_file(two.path()), 20),
        );
        fill(&a);
        assert_eq!(
            a.known_programs().unwrap().len(),
            1,
            "tasks is the only program with values"
        );
        let text = exported(&a, format);
        assert_eq!(
            text,
            exported(&a, format),
            "deterministic: the same DB gives the same bytes"
        );
        let by = iri("https://example.org/admin");
        let report = import(
            &b,
            &mut text.as_bytes(),
            Path::new("in"),
            format,
            ImportMode::Apply,
            &env(Some(&by)),
        )
        .unwrap();
        assert!(report.refused.is_empty(), "{:?}", report.refused);
        assert_eq!(exported(&b, format), text, "{format:?}");
        assert_eq!(rows_of(&b), rows_of(&a));
        assert_eq!(b.instance().values.values, a.instance().values.values);
        let again = import(
            &b,
            &mut text.as_bytes(),
            Path::new("in"),
            format,
            ImportMode::Apply,
            &env(Some(&by)),
        )
        .unwrap();
        assert_eq!(
            (again.applied, again.refused.len()),
            (0, 0),
            "the second import finds everything there"
        );
    }
}

/// KC9: a broken text names the line and changes nothing; `--dry-run` only reports; a locked key is refused alone and the rest goes in.
#[test]
fn kc9_errors_dry_run_and_locks() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_instance(&db_file(dir.path()), 10);
    let stamp = store.stamp();
    let broken = "[base.values.general]\n\"fs.shell.language\" = \"de\"\nthis is = not toml\n";
    let err = import(
        &store,
        &mut broken.as_bytes(),
        Path::new("in.toml"),
        Format::Toml,
        ImportMode::Apply,
        &env(None),
    )
    .unwrap_err();
    assert_eq!(err.code(), SettingCode::ImportInvalid);
    assert_eq!(text_of(&err, "file").as_deref(), Some("in.toml"));
    assert!(
        err.params()
            .any(|(k, v)| k == "line" && matches!(v, fs_settings::coded_stub::Param::Number(3))),
        "{err:?}"
    );
    assert_eq!(store.stamp(), stamp);

    let good =
        "[base.values.general]\n\"fs.shell.language\" = \"de\"\n\"fs.shell.style\" = \"calm\"\n";
    let dry = import(
        &store,
        &mut good.as_bytes(),
        Path::new("g"),
        Format::Toml,
        ImportMode::DryRun,
        &env(None),
    )
    .unwrap();
    assert_eq!(dry.applied, 2);
    assert_eq!(store.stamp(), stamp, "a dry run writes nothing");

    let who = Stampd {
        by: iri("https://example.org/admin"),
        at: at(2),
    };
    let lock = LockChange::Lock {
        value: fs_settings::Raw::Str("calm".to_owned()),
        reason: "policy".to_owned(),
    };
    store
        .write_lock(&key("fs.shell.style"), &lock, &who)
        .unwrap();
    let report = import(
        &store,
        &mut good.as_bytes(),
        Path::new("g"),
        Format::Toml,
        ImportMode::Apply,
        &env(None),
    )
    .unwrap();
    assert_eq!(report.applied, 1, "the rest goes in");
    assert_eq!(report.refused.len(), 1);
    assert_eq!(report.refused[0].code(), Code::ImportConflict);
    assert_eq!(
        text_of(&report.refused[0], "cause").as_deref(),
        Some("locked")
    );
    assert_eq!(
        store
            .profile(&fs_settings::Scope::General, Place::Profile, 0)
            .values
            .len(),
        1
    );
}

/// KC10: a batch that fails in the middle (an instance write without an actor after a value and a new profile) writes NOTHING: no value,
/// no profile, the stamp as before. This is the transaction of `SettingRows::apply`, not an undo.
#[test]
fn kc10_a_fault_in_the_middle_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_instance(&db_file(dir.path()), 10);
    let stamp = store.stamp();
    let ops = vec![
        Op::Value {
            target: Target::Named(ProfileName::new("Arbeit").unwrap()),
            scope: fs_settings::Scope::General,
            place: Place::Profile,
            key: key("fs.shell.language"),
            raw: raw_str("en"),
        },
        Op::Instance {
            key: key("fs.shell.style"),
            raw: raw_str("calm"),
        },
    ];
    let err = store.apply_ops(&ops, None).unwrap_err();
    assert_eq!(err.code(), Code::NoRight, "{err:?}");
    assert!(
        store
            .profile(&fs_settings::Scope::General, Place::Profile, 0)
            .values
            .is_empty()
    );
    assert_eq!(
        store.index().0.names().count(),
        0,
        "the profile of the batch was never created"
    );
    assert_eq!(store.stamp(), stamp);
    assert!(store.instance().values.values.is_empty());
}
