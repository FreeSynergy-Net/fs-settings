//! The clock and the OS user, as injectable values (tests give their own; the service never takes either from the caller).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ports::Utc;

/// A clock: UTC milliseconds.
pub type Clock = Arc<dyn Fn() -> Utc + Send + Sync>;

/// Who the OS says the user is.
pub type OsUser = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// The system clock.
#[must_use]
pub fn system_clock() -> Clock {
    Arc::new(|| {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(0);
        Utc::from_millis(ms)
    })
}

/// A clock that always says `at` (tests, `repair`).
#[must_use]
pub fn fixed_clock(at: Utc) -> Clock {
    Arc::new(move || at)
}

/// The user of this process as the environment names it (`USER`, `LOGNAME`, `USERNAME`). Same-user transport: the caller IS this user.
/// Honest limit: an environment variable is a word, not a proof; the port has no field for a peer credential yet (report, ENTWURF C).
#[must_use]
pub fn env_os_user() -> OsUser {
    Arc::new(|| {
        ["USER", "LOGNAME", "USERNAME"]
            .iter()
            .find_map(|k| std::env::var(k).ok())
            .filter(|v| !v.trim().is_empty())
    })
}
