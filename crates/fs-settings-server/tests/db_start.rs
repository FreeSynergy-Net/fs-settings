//! KC2 to KC5: the start sequence, the emergency mode (the file stays as it was) and `repair`.

use fs_objects::v5::{DOMAIN, Domain, Step, Store};
use fs_settings::{Health, Place, Scope, SettingCode, SettingsStore};
use fs_settings_server::{LegacyEnv, import_legacy, repair};

mod support;
use support::{at, db_file, fingerprint, key, listing, open, open_at, raw_str, text_of};

fn health_codes(store: &impl SettingsStore) -> Vec<(SettingCode, Option<String>)> {
    match store.health() {
        Health::Defaults(codes) => codes.iter().map(|c| (c.code(), text_of(c, "cause"))).collect(),
        other => panic!("expected Defaults, got {other:?}"),
    }
}

/// A domain with one more step than this program knows (the store of a NEWER program).
fn newer_domain() -> Domain {
    let mut steps = DOMAIN.steps.to_vec();
    steps.push(Step::Sql("CREATE TABLE IF NOT EXISTS settings_future (x INTEGER) STRICT"));
    let leaked: &'static [Step] = Box::leak(steps.into_boxed_slice());
    Domain::new(DOMAIN.prefix, DOMAIN.about, DOMAIN.tables, leaked)
}

/// The domain of a program whose step 2 is another one (a changed step).
fn changed_domain() -> Domain {
    let mut steps = DOMAIN.steps.to_vec();
    steps[1] = Step::Sql("SELECT 1");
    let leaked: &'static [Step] = Box::leak(steps.into_boxed_slice());
    Domain::new(DOMAIN.prefix, DOMAIN.about, DOMAIN.tables, leaked)
}

fn store_with(file: &std::path::Path, domain: Domain) {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    rt.block_on(async {
        let store = Store::open(file.to_str().unwrap(), 1).await.unwrap();
        store.register_domain(&domain, 1).await.unwrap();
    });
}

/// KC2: the first start creates the file and the old files are read once; the second start imports nothing again.
#[test]
fn kc2_first_start_then_second() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    std::fs::create_dir_all(config.join("freesynergy")).unwrap();
    std::fs::write(config.join("freesynergy").join("language"), "de\n").unwrap();
    let env = LegacyEnv { config_home: Some(config), data_home: None };
    let file = db_file(&dir.path().join("db"));
    let first = open(&file);
    assert!(first.was_created());
    assert_eq!(first.health(), Health::Ok);
    let report = import_legacy(&first, &env);
    assert_eq!(report.len(), 1);
    drop(first);
    let second = open(&file);
    assert!(!second.was_created(), "no second import: the file exists");
    assert_eq!(second.health(), Health::Ok);
    assert!(second.profile(&Scope::General, Place::Profile, 0).values.contains_key(&key("fs.shell.language")));
}

/// KC3: a written value is there after the service is started again.
#[test]
fn kc3_value_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let file = db_file(dir.path());
    let k = key("fs.shell.language");
    {
        let store = open(&file);
        store.write(&Scope::General, Place::Profile, 0, &k, Some(&raw_str("de"))).unwrap();
    }
    let again = open(&file);
    assert_eq!(again.profile(&Scope::General, Place::Profile, 0).values.get(&k), Some(&raw_str("de")));
}

/// KC4: rubbish, half a valid file, a newer store, a changed step, a folder: each starts the emergency mode with the exact code,
/// snapshots are empty, writes say `read-only{which=person}`, the file (bytes, length, mtime) and the folder stay as they were,
/// and a second start answers the same.
#[test]
fn kc4_emergency_leaves_the_file_alone() {
    let dir = tempfile::tempdir().unwrap();
    let k = key("fs.shell.language");

    // rubbish
    let junk = dir.path().join("junk");
    std::fs::create_dir_all(&junk).unwrap();
    let file = db_file(&junk);
    std::fs::write(&file, vec![0xA5_u8; 1024]).unwrap();
    check_emergency(&junk, &file, &k, SettingCode::StoreBroken);

    // half a valid file
    let half = dir.path().join("half");
    std::fs::create_dir_all(&half).unwrap();
    let good = dir.path().join("good.db");
    drop(open(&good));
    let bytes = std::fs::read(&good).unwrap();
    std::fs::write(db_file(&half), &bytes[..bytes.len() / 2]).unwrap();
    check_emergency(&half, &db_file(&half), &k, SettingCode::StoreBroken);

    // newer
    let newer = dir.path().join("newer");
    std::fs::create_dir_all(&newer).unwrap();
    store_with(&db_file(&newer), newer_domain());
    check_emergency(&newer, &db_file(&newer), &k, SettingCode::StoreNewer);

    // a step that differs
    let changed = dir.path().join("changed");
    std::fs::create_dir_all(&changed).unwrap();
    store_with(&db_file(&changed), changed_domain());
    check_emergency(&changed, &db_file(&changed), &k, SettingCode::StoreBroken);
}

fn check_emergency(folder: &std::path::Path, file: &std::path::Path, k: &fs_settings::SettingKey, expected: SettingCode) {
    let (before, names) = (fingerprint(file), listing(folder));
    for _ in 0..2 {
        let store = open_at(file, 5);
        let codes = health_codes(&store);
        assert_eq!(codes[0].0, expected, "{codes:?}");
        assert!(store.profile(&Scope::General, Place::Profile, 0).values.is_empty());
        let refused = store.write(&Scope::General, Place::Profile, 0, k, Some(&raw_str("de"))).unwrap_err();
        assert_eq!(refused.code(), SettingCode::ReadOnly);
        assert_eq!(text_of(&refused, "which").as_deref(), Some("person"));
        assert_eq!(fingerprint(file), before, "the file is byte-identical, length and mtime too");
        assert_eq!(listing(folder), names, "no new file appeared");
    }
}

/// KC4: a folder where the file should be is `store-broken{cause=not-regular}`, and nothing is created.
#[test]
fn kc4_a_folder_is_not_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let file = db_file(dir.path());
    std::fs::create_dir_all(&file).unwrap();
    let store = open(&file);
    assert_eq!(health_codes(&store)[0], (SettingCode::StoreBroken, Some("not-regular".to_owned())));
    assert!(file.is_dir());
}

/// KC5: `repair` renames the broken file (bytes kept, UTC of the injected clock in the name), creates a new database, and offers the
/// newest export; it never overwrites an earlier repair.
#[test]
fn kc5_repair_moves_aside() {
    let dir = tempfile::tempdir().unwrap();
    let file = db_file(dir.path());
    std::fs::write(&file, vec![7_u8; 300]).unwrap();
    let first = repair(&file, at(1_700_000_000_000)).unwrap();
    assert!(first.moved_to.to_string_lossy().ends_with("settings.db.broken-20231114T221320Z"), "{:?}", first.moved_to);
    assert_eq!(std::fs::read(&first.moved_to).unwrap(), vec![7_u8; 300]);
    assert_eq!(open(&file).health(), Health::Ok);
    assert_eq!(first.offer, None);
    std::fs::write(dir.path().join("settings-export-a.toml"), "").unwrap();
    std::fs::write(&file, b"again").unwrap();
    let second = repair(&file, at(1_700_000_000_000)).unwrap();
    assert_ne!(first.moved_to, second.moved_to);
    assert_eq!(std::fs::read(&first.moved_to).unwrap(), vec![7_u8; 300]);
    assert_eq!(second.offer, Some(dir.path().join("settings-export-a.toml")));
}
