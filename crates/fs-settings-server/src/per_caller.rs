//! The capability of EACH connection, built from its caller (`fs_rpc::PerConnection`): the actor of every request is `Caller::id`, from the
//! system only; the name is display only. A claimed `by` stays a claim: it is logged and never takes effect. No capability that is bound to
//! a caller lives in state that other connections can fetch (H2 of the caller design): every connection gets a service of its own.
//!
//! Pattern: Factory per connection (the shared state is only the database store, which knows no caller).

use std::sync::Arc;

use fs_rpc::{Caller, CallerError, PerConnection};
use fs_settings_rpc::SettingsService;

use crate::actor::{actor_of_caller, os_user_label};
use crate::clock::system_clock;
use crate::db_store::DbSettingsStore;

/// Where the program's log lines go (the default is stderr; tests collect them).
pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

/// The sink of the program: stderr, prefixed with the program's name.
#[must_use]
pub fn stderr_sink() -> LogSink {
    Arc::new(|line| eprintln!("fs-settings: {line}"))
}

/// The service of the program, per connection.
pub struct PerCaller {
    person: Arc<DbSettingsStore>,
    instance: Option<Arc<DbSettingsStore>>,
    log: LogSink,
}

impl PerCaller {
    /// `instance` is `Some` for `serve --instance` (admin calls).
    #[must_use]
    pub fn new(
        person: Arc<DbSettingsStore>,
        instance: Option<Arc<DbSettingsStore>>,
        log: LogSink,
    ) -> Self {
        Self {
            person,
            instance,
            log,
        }
    }

    /// The service of ONE connection: its actor and its OS user come from `caller`, its claim hook writes to the log.
    #[must_use]
    pub fn service_for(
        &self,
        caller: &Caller,
    ) -> SettingsService<DbSettingsStore, DbSettingsStore> {
        let mut service = SettingsService::new(Arc::clone(&self.person), self.instance.clone());
        let actor = actor_of_caller(caller);
        let label = os_user_label(caller);
        service.actor_of_connection = Arc::new(move || actor.clone());
        service.os_user = Arc::new(move || Some(label.clone()));
        service.now = system_clock();
        // `{by:?}` quotes the claim: no control character can forge a log line.
        let log = Arc::clone(&self.log);
        service.claimed_by = Some(Arc::new(move |by: &str| {
            log(&format!("claimed-by={by:?} ignored"));
        }));
        service
    }
}

impl PerConnection for PerCaller {
    fn bootstrap(&mut self, caller: &Caller) -> capnp::capability::Client {
        self.service_for(caller).bootstrap()
    }

    fn refused(&mut self, why: &CallerError) {
        (self.log)(&format!("refused connection: {why}"));
    }
}
