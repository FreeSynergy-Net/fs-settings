//! The form source of the program: `FormSource` on the SAME socket as the hub `settings.capnp`, the settings as a module of the
//! contract `module_v1`, behind `Guarded`.
//!
//! Pattern: Decorator (on `PerConnection`) + Factory per connection - the hub of fs-settings-rpc is handed out as before; next to it the
//! guarded `SettingsCore` answers `FormSource` (`with_form_source`). `SettingsCore` checks no right and is safe ONLY behind `Guarded`
//! (rights asked by `SettingsRules` before every `page` and `apply`); this file never serves it any other way. Ports: the actor is
//! `OsUserIsPerson` (the transport admits the service's own OS user only), the person is the owner "me" of the module person
//! (`LocalPerson`, read once). Without that person the form source does not start: the hub runs on, and the report names the
//! `owner-*` code.

use std::path::Path;
use std::sync::Arc;

use fs_interfaces::owner::{LocalOwner, OwnerProblem};
use fs_rpc::{
    Caller, CallerError, Guarded, LocalPerson, OsUserIsPerson, PerConnection, PersonOfConnection,
    Ports, with_form_source,
};
use fs_settings_rpc::{SettingsCallers, SettingsCore, SettingsService};

use crate::db_store::DbSettingsStore;
use crate::ports::ProgramName;

/// The hub per connection, and - when the owner is known - `FormSource` next to it on the same capability.
pub struct SettingsWithForm {
    hub: SettingsCallers<DbSettingsStore, DbSettingsStore>,
    form: Option<Guarded<SettingsCore<DbSettingsStore>>>,
}

impl SettingsWithForm {
    /// The service `service` over `store`, with the settings of `program` as a guarded module when `person` is known. A person that
    /// is not known (`Err`) is told to `report` as `fs-settings: owner-...` and the form source stays off.
    ///
    /// `core` is made BEFORE the loop starts, outside any runtime: `SettingsCore::new` reads the store once, and the database store
    /// is sync over a runtime of its own (see [`form_core`]).
    ///
    /// # Panics
    /// Outside a `LocalSet`: `Guarded::new` starts the pump of the change feed as a local task. Build it inside the program's `LocalSet`.
    pub fn new<P: PersonOfConnection + 'static>(
        service: SettingsService<DbSettingsStore, DbSettingsStore>,
        core: SettingsCore<DbSettingsStore>,
        person: Result<P, OwnerProblem>,
        report: &dyn Fn(&str),
    ) -> Self {
        let form = match person {
            Ok(person) => {
                // An empty catalog is no failure, but the form source then shows nothing and refuses every `key`: say it once.
                if service.catalog.iter().next().is_none() {
                    report(
                        "fs-settings: the catalog is empty (no program declares settings yet); the form source shows no setting",
                    );
                }
                Some(Guarded::new(core, Ports::new(OsUserIsPerson, person)))
            }
            Err(why) => {
                report(&format!(
                    "fs-settings: {why}; the form source does not start, the hub runs on"
                ));
                None
            }
        };
        Self {
            hub: service.callers(),
            form,
        }
    }

    /// Does this service serve `FormSource`?
    #[must_use]
    pub fn serves_form(&self) -> bool {
        self.form.is_some()
    }
}

impl PerConnection for SettingsWithForm {
    fn bootstrap(&mut self, caller: &Caller) -> capnp::capability::Client {
        let hub = self.hub.bootstrap(caller);
        match &self.form {
            Some(guarded) => with_form_source(hub, guarded.connect(caller)),
            None => hub,
        }
    }

    fn refused(&mut self, why: &CallerError) {
        self.hub.refused(why);
    }
}

/// The core of the form source over `store`: the settings of `program` with the catalog of `service`. Call it OUTSIDE any runtime
/// (before the loop): it reads the store once, and `DbSettingsStore` is sync over a runtime of its own.
#[must_use]
pub fn form_core(
    service: &SettingsService<DbSettingsStore, DbSettingsStore>,
    store: Arc<DbSettingsStore>,
    program: ProgramName,
) -> SettingsCore<DbSettingsStore> {
    SettingsCore::new(program, service.catalog.clone(), store)
}

/// The owner source of the program that runs as `running`: the service of the module person on its usual socket (started from the
/// program file next to `running` or in `/usr/bin`), as in fs-modules-planning. Where `running` is unknown the source says so.
#[must_use]
pub fn person_owner(running: Option<&Path>) -> Box<dyn LocalOwner> {
    let found = running.and_then(fs_person_service::PersonServiceOwner::standard);
    match found {
        Some(owner) => Box::new(owner),
        None => Box::new(NoPlace),
    }
}

/// The person "me" of `owner`, read ONCE: every connection the transport admits acts for it (ADR 0090).
///
/// # Errors
/// The [`OwnerProblem`] of the source (`owner-no-me`, `owner-unreachable`, `owner-invalid`).
pub fn local_person(owner: &dyn LocalOwner) -> Result<LocalPerson, OwnerProblem> {
    LocalPerson::new(owner)
}

/// The program does not know where the module person is.
struct NoPlace;

impl LocalOwner for NoPlace {
    fn owner(&self) -> Result<fs_objects::PersonRef, OwnerProblem> {
        Err(OwnerProblem::Unreachable {
            reason: "the place of the module person is unknown (no absolute path of this program)"
                .into(),
        })
    }
}
