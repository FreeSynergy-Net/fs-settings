//! `value.set` / `value.reset` over `FormSource` with the real database store: the store call does not run on the loop thread, does not
//! stop the loop while it lasts, and does not panic on the runtime (`SettingsCore` runs it on the blocking pool). And the product path
//! without a place for the module person: `owner-unreachable`. Not claimed here: the OS user in the change log - `db_port.rs` writes
//! none today (card S3 in the list of open features); the scope of the caller is proven in fs-rpc (`module_os_user.rs`).
//!
//! Pattern: Spy (test double) - a store that delegates to the database and notes the thread and the time of every write.
#![allow(clippy::unused_async_trait_impl, clippy::result_large_err)]

mod guarded_support;
mod support;

use std::sync::{Arc, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

use fs_interfaces::owner::OwnerProblem;
use fs_rpc::{FixedPerson, Guarded, OsUserIsPerson, Ports};
use fs_settings::{
    Coded, Health, IndexFn, Instance, Place, ProfileSet, Raw, Scope, SettingCode, SettingKey,
    SettingsStore, Snapshot, Stamp,
};
use fs_settings_rpc::SettingsCore;
use fs_settings_server::{DbSettingsStore, local_person, person_owner};
use guarded_support::{PERSON_ID, WEEK, catalog, connect, invoke, program, run, the_person};

/// How long a write lasts in this test: long enough for the loop to tick many times.
const SLOW: Duration = Duration::from_millis(300);

struct Spy {
    inner: Arc<DbSettingsStore>,
    writes: Mutex<Vec<ThreadId>>,
}

impl SettingsStore for Spy {
    fn stamp(&self) -> Stamp {
        self.inner.stamp()
    }
    fn health(&self) -> Health {
        self.inner.health()
    }
    fn instance(&self) -> Instance {
        self.inner.instance()
    }
    fn profile(&self, scope: &Scope, place: Place, profile: u64) -> Snapshot {
        self.inner.profile(scope, place, profile)
    }
    fn write(
        &self,
        scope: &Scope,
        place: Place,
        profile: u64,
        key: &SettingKey,
        raw: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.writes.lock().unwrap().push(std::thread::current().id());
        std::thread::sleep(SLOW);
        self.inner.write(scope, place, profile, key, raw)
    }
    fn clear(&self, scope: &Scope, profile: u64) -> Result<(), Coded<SettingCode>> {
        self.inner.clear(scope, profile)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.inner.index()
    }
    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        self.inner.update_index(f)
    }
}

#[test]
fn a_set_over_the_form_source_does_not_stop_the_loop() {
    // The store is opened BEFORE the runtime (it has a runtime of its own).
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(support::open(&support::db_file(dir.path())));
    let spy = Arc::new(Spy {
        inner: db,
        writes: Mutex::default(),
    });
    let loop_thread = std::thread::current().id();
    let ticks = run(async {
        let core = SettingsCore::new(program(), catalog(), spy.clone());
        let mut g = Guarded::new(core, Ports::new(OsUserIsPerson, FixedPerson(the_person())));
        let me = connect(&mut g, PERSON_ID);
        // a task of the same loop that ticks every 10 ms while the call lasts
        let counter = Arc::new(Mutex::new(0_u32));
        let c = counter.clone();
        let ticker = tokio::task::spawn_local(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(10)).await;
                *c.lock().unwrap() += 1;
            }
        });
        let set = invoke(&me, "value.set", WEEK, &[("value", "4")]).await;
        assert!(!set.failed, "{set:?}");
        let after_set = *counter.lock().unwrap();
        let reset = invoke(&me, "value.reset", WEEK, &[]).await;
        assert!(!reset.failed, "{reset:?}");
        ticker.abort();
        after_set
    });
    let writes = spy.writes.lock().unwrap().clone();
    assert_eq!(writes.len(), 2, "set and reset reach the store");
    assert!(
        writes.iter().all(|t| *t != loop_thread),
        "the store call runs on the blocking pool, not on the loop thread"
    );
    // The call lasted 300 ms; a loop that stood still would not tick (about 30 ticks are expected, 5 is the floor).
    assert!(ticks >= 5, "the loop kept running: {ticks} ticks");
}

/// The product path without a place for the module person (`person_owner(None)`) says `owner-unreachable`, not a panic or a guess.
#[test]
fn without_a_place_for_the_module_person_the_owner_is_unreachable() {
    let problem = local_person(&*person_owner(None))
        .err()
        .expect("no place: no person");
    assert!(matches!(problem, OwnerProblem::Unreachable { .. }), "{problem:?}");
}

/// An empty catalog is told once on start (stderr in the program), not silently served.
#[test]
fn an_empty_catalog_is_told_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(support::open(&support::db_file(dir.path())));
    run(async {
        let told = std::cell::RefCell::new(Vec::<String>::new());
        let mut svc = fs_settings_rpc::SettingsService::new_os_user_is_person(db.clone(), None);
        svc.catalog = fs_settings::Catalog::new();
        let form = fs_settings_server::SettingsWithForm::new(
            svc,
            db.clone(),
            program(),
            Ok(FixedPerson(the_person())),
            &|line| told.borrow_mut().push(line.to_owned()),
        );
        assert!(form.serves_form());
        let told = told.borrow();
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].contains("catalog is empty"), "{told:?}");
    });
}
