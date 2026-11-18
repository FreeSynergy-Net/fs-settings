//! `impl SettingsStore for DbSettingsStore`: the port over the rows (ENTWURF C.2). Every writing call is ONE transaction of the rows.

use std::collections::BTreeMap;

use crate::db_index::{changes, to_set};
use crate::db_instance::{enrich, lock_error};
use crate::db_store::DbSettingsStore;
use crate::error_map::{from_profile_code, map, profile};
use crate::ports::{
    Coded, Health, IndexRows, Instance, Lock, Place, ProfileSet, Raw, RowsError, Scope,
    SettingCode, SettingKey, SettingsStore, Snapshot, Stamp, Writer, index_step_ok, params,
};
use crate::prim::{place_row, scope_row, to_prim, to_raw};

/// Rows of values -> a snapshot; a key that no longer parses is a problem in the snapshot, never skipped silently.
pub(crate) fn snapshot_of(rows: Vec<(String, crate::ports::Prim)>) -> Snapshot {
    let mut values = BTreeMap::new();
    let mut problems = Vec::new();
    for (text, prim) in rows {
        match SettingKey::parse(&text) {
            Ok(key) => {
                values.insert(key, to_raw(&prim));
            }
            Err(problem) => problems.push(problem),
        }
    }
    Snapshot { values, problems }
}

fn profile_id(profile_no: u64) -> Result<i64, Coded<SettingCode>> {
    i64::try_from(profile_no).map_err(|_| profile("id-invalid"))
}

impl SettingsStore for DbSettingsStore {
    /// The rows' revision plus the instance layer's stamp: either moving moves the sum.
    fn stamp(&self) -> Stamp {
        let own = match &self.store {
            Some(store) => self
                .run(async {
                    let rows = self.rows(store).await?;
                    rows.rev().await.map_err(|e| map(&e, &self.file, None))
                })
                .map_or(0, |rev| u64::try_from(rev).unwrap_or(0)),
            None => 0,
        };
        Stamp(own.saturating_add(self.source.stamp().0))
    }

    fn health(&self) -> Health {
        self.health.clone()
    }

    fn instance(&self) -> Instance {
        if self.instance_mode {
            self.own_instance()
        } else {
            self.source.instance()
        }
    }

    fn profile(&self, scope: &Scope, place: Place, profile_no: u64) -> Snapshot {
        let Some(store) = &self.store else {
            return Snapshot::default();
        };
        let read = async {
            let rows = self.rows(store).await?;
            rows.values(profile_id(profile_no)?, scope_row(scope), place_row(place))
                .await
                .map_err(|e| map(&e, &self.file, None))
        };
        match self.run(read) {
            Ok(rows) => snapshot_of(rows),
            Err(problem) => Snapshot {
                values: BTreeMap::new(),
                problems: vec![problem],
            },
        }
    }

    fn write(
        &self,
        scope: &Scope,
        place: Place,
        profile_no: u64,
        key: &SettingKey,
        raw: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        let store = self.writable("person")?;
        // The second guard next to `Settings::save` (decision Q5): the lock as it is NOW.
        if let Some(lock) = self.instance().locks.get(key) {
            return Err(locked(key, lock));
        }
        let (id, prim, at) = (profile_id(profile_no)?, raw.map(to_prim), (self.now)());
        let via = match scope {
            Scope::General => None,
            Scope::Program(p) => Some(p.as_str()),
        };
        let write = async {
            let rows = self.rows(store).await?;
            rows.put(
                id,
                scope_row(scope),
                place_row(place),
                key.as_str(),
                prim.as_ref(),
                Writer { by: None, via },
                at,
            )
            .await
            .map(|_| ())
            .map_err(|e| lock_error(&e, &self.file, key))
        };
        self.run(write).map_err(|c| enrich(self, c, key))
    }

    fn clear(&self, scope: &Scope, profile_no: u64) -> Result<(), Coded<SettingCode>> {
        let store = self.writable("person")?;
        let (id, at) = (profile_id(profile_no)?, (self.now)());
        let clear = async {
            let rows = self.rows(store).await?;
            rows.clear_profile(id, scope_row(scope), at)
                .await
                .map(|_| ())
                .or_else(|e| match &e {
                    // Clearing what is not there (a deleted or unknown profile) is done already: the store suite wants it idempotent.
                    RowsError::Invalid(p) if p.first().is_some_and(|p| p.reason == "unknown") => {
                        Ok(())
                    }
                    _ => Err(map(&e, &self.file, None)),
                })
        };
        self.run(clear)
    }

    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        let Some(store) = &self.store else {
            return (ProfileSet::new(0), Vec::new());
        };
        let read = async {
            let rows = self.rows(store).await?;
            rows.index().await.map_err(|e| map(&e, &self.file, None))
        };
        match self.run(read).and_then(|ix| to_set(&ix)) {
            Ok(set) => (set, Vec::new()),
            Err(problem) => (ProfileSet::new(0), vec![problem]),
        }
    }

    /// One transaction of the rows: `f` sees the index as stored now; an error of `f` or a refused step changes nothing.
    fn update_index(
        &self,
        f: &mut fs_settings::IndexFn<'_>,
    ) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        let store = self.writable("person")?;
        let at = (self.now)();
        let mut step =
            |ix: &IndexRows| -> Result<Vec<crate::ports::IndexChange>, Coded<SettingCode>> {
                let old = to_set(ix)?;
                let mut new = old.clone();
                f(&mut new).map_err(from_profile_code)?;
                index_step_ok(&old, &new)?;
                Ok(changes(&old, &new))
            };
        let change = async {
            let rows = self.rows(store).await?;
            rows.change_index(at, &mut step)
                .await
                .map_err(|e| map(&e, &self.file, None))
        };
        let stored = self.run(change)??;
        to_set(&stored)
    }
}

/// `locked{key, reason, by}` from a lock of the instance layer.
pub(crate) fn locked(key: &SettingKey, lock: &Lock) -> Coded<SettingCode> {
    Coded::new(SettingCode::Locked)
        .with_text(params::KEY, key.as_str())
        .with_text(params::REASON, lock.reason.clone())
        .with_text(params::BY, lock.locked_by.clone())
}
