//! The instance layer of the DB: `InstanceStore` (only for `serve --instance`), reading the instance rows, the log.
//! Time and OS user are set HERE (the service), never taken from the caller: `Stampd.at` and any caller word are ignored for the
//! log (ENTWURF C.4; MC9). A caller that is not the instance service gets `read-only{which=instance}`.

use std::collections::BTreeMap;

use crate::db_port::{locked, snapshot_of};
use crate::db_store::DbSettingsStore;
use crate::error_map::map;
use crate::ports::{
    Actor, BasicValue, Coded, Instance, InstanceLogEntry, InstanceStore, Lock, LockChange, ObjectId, Outcome, Raw, RowsError,
    SettingCode, SettingKey, SettingsStore, Snapshot, Stampd, params,
};
use crate::prim::{to_prim, to_raw};

/// A write refused because the key is locked: `locked{key, reason, by}` when the lock can be read, else `locked{key}`.
pub(crate) fn lock_error(db: &DbSettingsStore, e: &RowsError, key: &SettingKey) -> Coded<SettingCode> {
    let mapped = map(e, db.file(), Some(key.as_str()));
    if mapped.code() == SettingCode::Locked {
        if let Some(lock) = db.instance().locks.get(key) {
            return locked(key, lock);
        }
    }
    mapped
}

fn action_word(action: &str) -> &'static str {
    match action {
        "set" => "set",
        "clear" => "clear",
        "lock" => "lock",
        _ => "unlock",
    }
}

impl DbSettingsStore {
    /// The instance rows of this very file (instance mode), as the port's `Instance`.
    pub(crate) fn own_instance(&self) -> Instance {
        let Some(store) = &self.store else { return Instance::default() };
        let read = async {
            let rows = self.rows(store).await?;
            rows.instance().await.map_err(|e| map(&e, &self.file, None))
        };
        match self.run(read) {
            Err(problem) => Instance { values: Snapshot { values: BTreeMap::new(), problems: vec![problem] }, locks: BTreeMap::new() },
            Ok(rows) => {
                let mut values = snapshot_of(rows.values);
                let mut locks = BTreeMap::new();
                for l in rows.locks {
                    match SettingKey::parse(&l.key) {
                        Ok(key) => {
                            locks.insert(key, Lock { reason: l.reason, locked_by: l.by.canonical(), locked_at: l.at });
                        }
                        Err(problem) => values.problems.push(problem),
                    }
                }
                Instance { values, locks }
            }
        }
    }

    fn writable_instance(&self) -> Result<&crate::ports::Store, Coded<SettingCode>> {
        if self.instance_mode { self.writable("instance") } else { Err(read_only_instance()) }
    }
}

fn read_only_instance() -> Coded<SettingCode> {
    Coded::new(SettingCode::ReadOnly).with_text(params::WHICH, "instance")
}

fn changed(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Changed { .. })
}

impl InstanceStore for DbSettingsStore {
    fn write_instance(&self, key: &SettingKey, raw: Option<&Raw>, who: &Stampd) -> Result<bool, Coded<SettingCode>> {
        let store = self.writable_instance()?;
        let (prim, os_user, at) = (raw.map(to_prim), (self.os_user)(), (self.now)());
        let write = async {
            let rows = self.rows(store).await?;
            let actor = Actor { by: &who.by, os_user: os_user.as_deref() };
            rows.put_instance(key.as_str(), prim.as_ref(), actor, at).await.map_err(|e| lock_error(self, &e, key))
        };
        self.run(write).map(changed)
    }

    fn write_lock(&self, key: &SettingKey, change: &LockChange, who: &Stampd) -> Result<bool, Coded<SettingCode>> {
        let store = self.writable_instance()?;
        let (os_user, at) = ((self.os_user)(), (self.now)());
        let write = async {
            let rows = self.rows(store).await?;
            let actor = Actor { by: &who.by, os_user: os_user.as_deref() };
            let done = match change {
                LockChange::Lock { value, reason } => rows.lock(key.as_str(), &to_prim(value), reason, actor, at).await,
                LockChange::Unlock { reason } => rows.unlock(key.as_str(), reason, actor, at).await,
            };
            done.map_err(|e| lock_error(self, &e, key))
        };
        self.run(write).map(changed)
    }

    fn instance_log(&self, after: Option<ObjectId>, size: u32) -> Vec<InstanceLogEntry> {
        let Some(store) = self.store.as_ref().filter(|_| self.instance_mode) else { return Vec::new() };
        let read = async {
            let rows = self.rows(store).await?;
            rows.log(after, size).await.map_err(|e| map(&e, &self.file, None))
        };
        // A log row whose key no longer parses cannot be shown as a key; the DB check keeps keys valid, so this is "never".
        self.run(read)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|row| {
                Some(InstanceLogEntry {
                    id: row.id,
                    at: row.at,
                    by: row.by.canonical(),
                    os_user: row.os_user,
                    action: action_word(row.action),
                    key: SettingKey::parse(&row.key).ok()?,
                    reason: row.reason,
                    old: row.old.as_ref().map(to_raw),
                    new: row.new.as_ref().map(to_raw),
                })
            })
            .collect()
    }
}
