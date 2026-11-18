//! `DbSettingsStore`: the port `fs-settings` over the rows of `fs-objects` `v5::setting`. The start sequence (PLAN 5.4) lives here.
//!
//! Pattern: Adapter (port <- rows). Sync over async: ONE current-thread runtime per store; every call is `block_on` on the CALLER's
//! thread (closures of `update_index` need not be `Send`). Several threads may call at once (probe question P3, answered
//! by KC6/KC6b). The RPC skeleton calls the store
//! through `spawn_blocking`, never on a runtime thread.

use std::future::Future;
use std::path::{Path, PathBuf};

use crate::clock::{Clock, OsUser, system_os_user};
use crate::error_map::{broken, map};
use crate::instance_source::InstanceSource;
use crate::paths::{DbPlace, resolve};
use crate::ports::{Coded, DOMAIN, Health, Inspect, SettingCode, SettingRows, Store, params};

/// The adapter. Build with [`DbSettingsStore::open`].
pub struct DbSettingsStore {
    rt: tokio::runtime::Runtime,
    pub(crate) store: Option<Store>,
    pub(crate) file: PathBuf,
    pub(crate) health: Health,
    pub(crate) instance_mode: bool,
    pub(crate) source: Box<dyn InstanceSource>,
    pub(crate) now: Clock,
    pub(crate) os_user: OsUser,
    created: bool,
}

struct Started {
    store: Option<Store>,
    health: Health,
    created: bool,
}

impl Started {
    fn emergency(code: Coded<SettingCode>) -> Self {
        Self {
            store: None,
            health: Health::Defaults(vec![code]),
            created: false,
        }
    }
}

impl DbSettingsStore {
    /// Start sequence: `resolve`; file absent: create and register (a first start); present: `Store::inspect` FIRST; `Fits`: open
    /// and register; `Newer` or `Broken`: DO NOT OPEN, emergency mode with `Health::Defaults[store-newer | store-broken]`, every
    /// snapshot empty, every write `read-only{which=person}`. An existing file is never deleted, overwritten or created again.
    /// No folder to put the file in (`no-folder`) is emergency mode too (an empty file name), never a silent default.
    ///
    /// `instance` = this file family is the INSTANCE database (`serve --instance`): the instance tables and `InstanceStore` are active.
    ///
    /// # Errors
    /// Only when the operating system refuses to create the runtime (`store-broken{cause=open}`).
    pub fn open(
        place: &DbPlace,
        instance: bool,
        source: Box<dyn InstanceSource>,
        now: Clock,
    ) -> Result<Self, Coded<SettingCode>> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|_| broken(Path::new(""), "open"))?;
        let (file, started) = match resolve(place) {
            Ok(file) => {
                let started = start(&rt, &file, &now);
                (file, started)
            }
            Err(code) => (PathBuf::new(), Started::emergency(code)),
        };
        Ok(Self {
            rt,
            store: started.store,
            file,
            health: started.health,
            instance_mode: instance,
            source,
            now,
            os_user: system_os_user(),
            created: started.created,
        })
    }

    /// Another OS-user source (the service's own, tests').
    #[must_use]
    pub fn with_os_user(mut self, os_user: OsUser) -> Self {
        self.os_user = os_user;
        self
    }

    /// The file this store was started on (empty when no place was found).
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// True when this start created the file (the one moment of the legacy import).
    #[must_use]
    pub fn was_created(&self) -> bool {
        self.created
    }

    /// Runs `f` to the end on the store's runtime. Concurrent callers are safe: a current-thread runtime may be entered by several
    /// threads (the first drives, the others wait for the driver; tokio 1.54 `Runtime::block_on`, "can be called concurrently from
    /// multiple threads"), and every write is one database transaction anyway. Proof by test: `kc6b_many_threads_many_rounds`.
    pub(crate) fn run<T>(&self, f: impl Future<Output = T>) -> T {
        self.rt.block_on(f)
    }

    /// The open store, or `read-only{which}` in emergency mode.
    pub(crate) fn writable(&self, which: &str) -> Result<&Store, Coded<SettingCode>> {
        self.store
            .as_ref()
            .ok_or_else(|| Coded::new(SettingCode::ReadOnly).with_text(params::WHICH, which))
    }

    /// Opens the rows on `store` (registered at start) and maps a failure.
    pub(crate) async fn rows<'s>(
        &self,
        store: &'s Store,
    ) -> Result<SettingRows<'s>, Coded<SettingCode>> {
        SettingRows::new(store)
            .await
            .map_err(|e| map(&e, &self.file, None))
    }
}

fn start(rt: &tokio::runtime::Runtime, file: &Path, now: &Clock) -> Started {
    let Some(path) = file.to_str() else {
        return Started::emergency(broken(file, "open"));
    };
    match rt.block_on(Store::inspect(path, &DOMAIN)) {
        Ok(Inspect::Absent) => {
            if let Some(dir) = file.parent()
                && std::fs::create_dir_all(dir).is_err()
            {
                return Started::emergency(broken(file, "open"));
            }
            open_rows(rt, path, file, now, true)
        }
        Ok(Inspect::Fits { .. }) => open_rows(rt, path, file, now, false),
        Ok(Inspect::Newer { found, known }) => Started::emergency(map(
            &crate::ports::RowsError::Newer {
                domain: DOMAIN.prefix.to_owned(),
                found,
                known,
            },
            file,
            None,
        )),
        Ok(Inspect::Broken { cause }) => Started::emergency(broken(file, cause)),
        Ok(_) | Err(_) => Started::emergency(broken(file, "open")),
    }
}

fn open_rows(
    rt: &tokio::runtime::Runtime,
    path: &str,
    file: &Path,
    now: &Clock,
    created: bool,
) -> Started {
    match rt.block_on(SettingRows::open(path, now())) {
        Ok(store) => Started {
            store: Some(store),
            health: Health::Ok,
            created,
        },
        Err(e) => Started::emergency(map(&e, file, None)),
    }
}
