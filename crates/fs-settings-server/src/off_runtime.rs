//! `OffRuntime`: the port `SettingsStore` of the database for a caller that runs on an async runtime thread.
//!
//! Pattern: Proxy (protection) over the port - `DbSettingsStore` is sync over async (ONE runtime of its own, every call a `block_on`)
//! and so must never be called on a runtime thread (tokio: "cannot start a runtime from within a runtime"). The hub of fs-settings-rpc
//! calls it through `spawn_blocking`; `SettingsCore` (the form source) calls the port inside its async methods, so it gets THIS proxy:
//! every call runs to the end on a thread of its own (scoped, joined before the call returns). Deviation: the thread of the runtime
//! waits for the call, as it would for any sync store; `update_index` (a closure that need not be `Send`) cannot cross threads and is
//! refused with `write-failed{cause=off-runtime}` - the module `SettingsCore` never calls it.

use std::sync::Arc;

use fs_settings::{IndexFn, Instance, Place, ProfileSet, Raw, Scope, SettingKey, Snapshot, Stamp};

use crate::ports::{Coded, Health, SettingCode, SettingsStore, params};

/// The proxy over `S`, shared with the hub (the same store, one source of truth).
pub struct OffRuntime<S>(pub Arc<S>);

/// Runs `f` on a thread of its own and waits for it (a panic goes on).
fn hop<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| match scope.spawn(f).join() {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    })
}

impl<S: SettingsStore> SettingsStore for OffRuntime<S> {
    fn stamp(&self) -> Stamp {
        hop(|| self.0.stamp())
    }

    fn health(&self) -> Health {
        hop(|| self.0.health())
    }

    fn instance(&self) -> Instance {
        hop(|| self.0.instance())
    }

    fn profile(&self, scope: &Scope, place: Place, profile: u64) -> Snapshot {
        hop(|| self.0.profile(scope, place, profile))
    }

    fn write(
        &self,
        scope: &Scope,
        place: Place,
        profile: u64,
        key: &SettingKey,
        raw: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        hop(|| self.0.write(scope, place, profile, key, raw))
    }

    fn clear(&self, scope: &Scope, profile: u64) -> Result<(), Coded<SettingCode>> {
        hop(|| self.0.clear(scope, profile))
    }

    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        hop(|| self.0.index())
    }

    fn update_index(&self, _: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        Err(Coded::new(SettingCode::WriteFailed).with_text(params::CAUSE, "off-runtime"))
    }
}
