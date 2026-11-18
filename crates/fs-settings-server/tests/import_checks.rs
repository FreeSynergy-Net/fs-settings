//! Y2 and Y6: the checks of an import before anything is written: undeclared and wrongly typed values, and a held instance lock.

use std::path::Path;

use fs_settings::{Catalog, InstanceStore, LockChange, Place, Stampd};
use fs_settings_server::exchange::{Format, ImportMode, import};

mod support;
use support::{at, catalog, db_file, env, iri, key, open_instance, raw_str, text_of};

/// Y6: a key that is not declared is NEVER written; a value of the wrong type is refused alone, the declared rest goes in.
#[test]
fn y6_undeclared_and_wrong_type_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_instance(&db_file(dir.path()), 10);
    let cat = catalog();
    let text = "[base.values.general]\n\"fs.shell.language\" = \"de\"\n\"fs.shell.unknown-thing\" = \"x\"\n\"fs.shell.style\" = 5\n";
    let report = import(
        &store,
        &mut text.as_bytes(),
        Path::new("in"),
        Format::Toml,
        ImportMode::Apply,
        &env(None, &cat),
    )
    .unwrap();
    let causes: Vec<String> = report
        .refused
        .iter()
        .map(|c| text_of(c, "cause").unwrap())
        .collect();
    assert_eq!(causes.len(), 2, "{:?}", report.refused);
    assert!(
        causes.contains(&"not-declared".to_owned()) && causes.contains(&"wrong-type".to_owned())
    );
    let general = store
        .profile(&fs_settings::Scope::General, Place::Profile, 0)
        .values;
    assert_eq!(
        general.len(),
        1,
        "only the declared, well-typed value is written"
    );
    assert!(general.contains_key(&key("fs.shell.language")));
    // an empty catalog (the command line today) refuses every value
    let empty = Catalog::new();
    let none = import(
        &store,
        &mut text.as_bytes(),
        Path::new("in"),
        Format::Toml,
        ImportMode::DryRun,
        &env(None, &empty),
    )
    .unwrap();
    assert_eq!((none.applied, none.refused.len()), (0, 3));
}

/// Y2: an instance import that meets a HELD lock refuses that key alone (`import-conflict{cause=locked}`) and writes the rest.
#[test]
fn y2_a_held_instance_lock_is_refused_alone() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_instance(&db_file(dir.path()), 10);
    let cat = catalog();
    let by = iri("https://example.org/admin");
    let who = Stampd {
        by: by.clone(),
        at: at(1),
    };
    let lock = LockChange::Lock {
        value: fs_settings::Raw::Str("calm".to_owned()),
        reason: "policy".to_owned(),
    };
    store
        .write_lock(&key("fs.shell.style"), &lock, &who)
        .unwrap();
    let text = "[instance]\n\"fs.shell.style\" = \"craft\"\n\"fs.shell.language\" = \"de\"\n";
    let report = import(
        &store,
        &mut text.as_bytes(),
        Path::new("in"),
        Format::Toml,
        ImportMode::Apply,
        &env(Some(&by), &cat),
    )
    .unwrap();
    assert_eq!(report.applied, 1, "the rest goes in");
    assert_eq!(report.refused.len(), 1);
    assert_eq!(
        text_of(&report.refused[0], "cause").as_deref(),
        Some("locked")
    );
    let layer = store.instance();
    assert_eq!(
        layer.values.values.get(&key("fs.shell.style")),
        Some(&raw_str("calm")),
        "the held value stays"
    );
    assert_eq!(
        layer.values.values.get(&key("fs.shell.language")),
        Some(&raw_str("de"))
    );
}
