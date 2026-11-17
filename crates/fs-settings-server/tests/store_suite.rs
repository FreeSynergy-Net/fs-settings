//! KC16: the suite `store` of fs-settings over the database: 12 of 12 rules conformant. Feature `conformance` only.
//! The fixture's `break_store` writes rubbish into the file and starts the store again (emergency mode), so `store-health-said` runs.
#![cfg(feature = "conformance")]

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use fs_conformance::{Options, run};
use fs_settings::coded_stub::Coded;
use fs_settings::suite::{StoreFixture, StoreSuite};
use fs_settings::{
    Health, Instance, InstanceLogEntry, InstanceStore, LockChange, Place, ProfileCode, ProfileSet,
    Raw, Scope, SettingCode, SettingKey, SettingsStore, Snapshot, Stamp, Stampd,
};
use fs_settings_server::DbSettingsStore;
use fs_types::ObjectId;

mod support;
use support::{db_file, open_instance};

/// A store that can be started again on its file (what a fixture needs to break the file under a running service).
struct Reopening {
    file: PathBuf,
    inner: RwLock<DbSettingsStore>,
}

impl Reopening {
    fn restart(&self) {
        *self.inner.write().unwrap() = open_instance(&self.file, 1);
    }
}

impl SettingsStore for Reopening {
    fn stamp(&self) -> Stamp {
        self.inner.read().unwrap().stamp()
    }
    fn health(&self) -> Health {
        self.inner.read().unwrap().health()
    }
    fn instance(&self) -> Instance {
        self.inner.read().unwrap().instance()
    }
    fn profile(&self, scope: &Scope, place: Place, profile: u64) -> Snapshot {
        self.inner.read().unwrap().profile(scope, place, profile)
    }
    fn write(
        &self,
        scope: &Scope,
        place: Place,
        profile: u64,
        key: &SettingKey,
        raw: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.inner
            .read()
            .unwrap()
            .write(scope, place, profile, key, raw)
    }
    fn clear(&self, scope: &Scope, profile: u64) -> Result<(), Coded<SettingCode>> {
        self.inner.read().unwrap().clear(scope, profile)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.inner.read().unwrap().index()
    }
    fn update_index(
        &self,
        f: &mut fs_settings::IndexFn<'_>,
    ) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        self.inner.read().unwrap().update_index(f)
    }
}

impl InstanceStore for Reopening {
    fn write_instance(
        &self,
        key: &SettingKey,
        raw: Option<&Raw>,
        who: &Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.inner.read().unwrap().write_instance(key, raw, who)
    }
    fn write_lock(
        &self,
        key: &SettingKey,
        change: &LockChange,
        who: &Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.inner.read().unwrap().write_lock(key, change, who)
    }
    fn instance_log(&self, after: Option<ObjectId>, size: u32) -> Vec<InstanceLogEntry> {
        self.inner.read().unwrap().instance_log(after, size)
    }
}

struct DbFixture {
    dir: tempfile::TempDir,
    made: Mutex<Vec<Arc<Reopening>>>,
}

impl StoreFixture for DbFixture {
    fn fresh(&mut self) -> Arc<dyn InstanceStore> {
        let n = self.made.lock().unwrap().len();
        let file = db_file(&self.dir.path().join(format!("s{n}")));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let store = Arc::new(Reopening {
            inner: RwLock::new(open_instance(&file, 1)),
            file,
        });
        self.made.lock().unwrap().push(Arc::clone(&store));
        store
    }

    fn break_store(&mut self, store: &dyn InstanceStore) -> bool {
        let made = self.made.lock().unwrap();
        // the store the suite holds is the latest one made for it (rules take one fresh store each)
        let Some(target) = made.iter().rev().find(|m| {
            std::ptr::eq(
                Arc::as_ptr(m).cast::<()>(),
                (store as *const dyn InstanceStore).cast::<()>(),
            )
        }) else {
            return false;
        };
        std::fs::write(&target.file, vec![0xA5_u8; 512]).unwrap();
        target.restart();
        true
    }
}

/// KC16: 12 rules, conformant, none skipped.
#[tokio::test]
async fn kc16_store_suite_over_the_database() {
    let mut fixture = DbFixture {
        dir: tempfile::tempdir().unwrap(),
        made: Mutex::new(Vec::new()),
    };
    let report = run(&StoreSuite, &mut fixture, &Options::default()).await;
    assert!(report.is_conformant(), "{:?}", report.failures);
    assert_eq!((report.checked, report.skipped.len()), (12, 0));
}
