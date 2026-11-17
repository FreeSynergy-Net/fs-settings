//! KC6, KC7, KC14, KC17: the port over the database: many writers, the profile index, the person service in v1, `Settings` over it.

use std::sync::Arc;

use fs_settings::InstanceStore;
use fs_settings::{
    Catalog, Declaration, GroupId, Health, Kind, Layer, Owner, OwnerKind, Place, ProfileCode,
    ProfileName, ProfileRef, ProgramName, Raw, Scope, SettingCode, SettingKey, Settings,
    SettingsStore, Stampd, Target, Value,
};
use fs_settings_server::{DbSettingsStore, NoInstance};

mod support;
use support::{db_file, iri, open, open_instance};

fn name(text: &str) -> ProfileName {
    ProfileName::new(text).unwrap()
}

/// Adds a named profile the way `Settings` does: the next id is `top + 1`, and the base value is the new top.
fn create(
    set: &mut fs_settings::ProfileSet<u64>,
    text: &str,
) -> Result<(), fs_settings::Coded<ProfileCode>> {
    let id = set.get(&ProfileRef::Base).copied().unwrap() + 1;
    *set.get_mut(&ProfileRef::Base).unwrap() = id;
    set.create(name(text), None)?;
    *set.get_mut(&ProfileRef::Named(name(text))).unwrap() = id;
    Ok(())
}

/// KC6: eight writers at the same time, from threads that go through `spawn_blocking` of a current-thread runtime: all values there,
/// the stamp rises by 8, nothing hangs (limit 10 s).
#[test]
fn kc6_eight_concurrent_writes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(open(&db_file(dir.path())));
    let before = store.stamp();
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let done = rt.block_on(async {
        let tasks: Vec<_> = (0..8_i64)
            .map(|n| {
                let store = Arc::clone(&store);
                tokio::task::spawn_blocking(move || {
                    let k = SettingKey::parse(&format!("fs.shell.slot-{n}")).unwrap();
                    store.write(&Scope::General, Place::Profile, 0, &k, Some(&Raw::Int(n)))
                })
            })
            .collect();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let mut results = Vec::new();
            for t in tasks {
                results.push(t.await.unwrap());
            }
            results
        })
        .await
    });
    assert!(done.expect("no hang").iter().all(Result::is_ok));
    assert_eq!(
        store
            .profile(&Scope::General, Place::Profile, 0)
            .values
            .len(),
        8
    );
    assert_eq!(store.stamp().0, before.0 + 8);
}

/// KC7: two threads create a profile each (ids 1 and 2); a failing closure changes nothing.
#[test]
fn kc7_profiles_from_two_threads_and_a_failing_closure() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(open(&db_file(dir.path())));
    let handles: Vec<_> = ["Arbeit", "Vortrag"]
        .into_iter()
        .map(|n| {
            let store = Arc::clone(&store);
            std::thread::spawn(move || store.update_index(&mut |set| create(set, n)).unwrap())
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let (set, problems) = store.index();
    assert!(problems.is_empty());
    let mut ids: Vec<u64> = set
        .names()
        .map(|n| set.get(&ProfileRef::Named(n.clone())).copied().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2]);

    let stamp = store.stamp();
    let refused = store.update_index(&mut |set| {
        create(set, "Nie")?;
        Err(fs_settings::Coded::new(ProfileCode::Unknown))
    });
    assert_eq!(refused.unwrap_err().code(), SettingCode::Profile);
    assert_eq!(store.stamp(), stamp, "nothing changed");
    assert_eq!(store.index().0.names().count(), 2);
}

/// KC14: the person service in v1 has no instance layer: `instance()` is empty and the health is `Ok`.
#[test]
fn kc14_person_service_has_no_instance_layer() {
    let dir = tempfile::tempdir().unwrap();
    let store = DbSettingsStore::open(
        &fs_settings_server::DbPlace::File(db_file(dir.path())),
        false,
        Box::new(NoInstance),
        fs_settings_server::system_clock(),
    )
    .unwrap();
    let layer = store.instance();
    assert!(layer.values.values.is_empty() && layer.locks.is_empty());
    assert_eq!(store.health(), Health::Ok);
}

fn week_start(owner: &str) -> (SettingKey, Declaration) {
    let k = SettingKey::new(
        &Owner::new(OwnerKind::Program, owner).unwrap(),
        "week-start",
    )
    .unwrap();
    let d = Declaration::new(
        k.clone(),
        Kind::Whole { min: 1, max: 7 },
        Value::Whole(1),
        GroupId::new("calendar").unwrap(),
    );
    (k, d)
}

fn settings(store: &Arc<DbSettingsStore>, who: &str, k: &SettingKey, d: &Declaration) -> Settings {
    let _ = k;
    Settings::open(
        ProgramName::new(who).unwrap(),
        Catalog::of(vec![d.clone()]).unwrap(),
        Arc::clone(store) as Arc<dyn SettingsStore>,
        Arc::new(fs_settings::NoSystem),
    )
}

/// KC17: `Settings` over the database: "this program" / "all programs" / a lock of the instance layer, the same meaning as over the
/// memory store (K8 and K10 of the probe `setting_v1`).
#[test]
fn kc17_settings_over_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(open_instance(&db_file(dir.path()), 1_700_000_000_000));
    let (k, d) = week_start("tasks");
    let (a, b) = (
        settings(&store, "tasks", &k, &d),
        settings(&store, "calendars", &k, &d),
    );

    a.save(&k, Some(&Value::Whole(3)), Target::ThisProgram)
        .unwrap();
    b.refresh();
    assert_eq!(
        a.get(&k).map(|e| (e.value, e.from)),
        Some((Value::Whole(3), Layer::Program))
    );
    assert_eq!(
        b.get(&k).map(|e| e.from),
        Some(Layer::Default),
        "the other program is untouched"
    );

    a.save(&k, Some(&Value::Whole(5)), Target::AllPrograms)
        .unwrap();
    b.refresh();
    assert_eq!(
        a.get(&k).map(|e| (e.value, e.from)),
        Some((Value::Whole(5), Layer::Person)),
        "the own override is gone"
    );
    assert_eq!(b.get(&k).map(|e| e.value), Some(Value::Whole(5)));

    // a lock of the instance layer (the instance tables of this very file) refuses the save, with who and why
    let by = iri("https://example.org/admin");
    let who = Stampd {
        by,
        at: support::at(1),
    };
    let lock = fs_settings::LockChange::Lock {
        value: Raw::Int(2),
        reason: "policy".to_owned(),
    };
    assert!(store.write_lock(&k, &lock, &who).unwrap());
    a.refresh();
    let refused = a
        .save(&k, Some(&Value::Whole(6)), Target::ThisProgram)
        .unwrap_err();
    assert_eq!(refused.code(), SettingCode::Locked);
    assert_eq!(
        a.get(&k).map(|e| (e.value, e.from)),
        Some((Value::Whole(2), Layer::Instance))
    );
}
