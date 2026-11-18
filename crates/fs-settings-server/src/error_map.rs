//! fs-objects `Error` -> `SettingCode` (ENTWURF C.6). The ONE mapping; `Coded` carries the parameters.
//! Pattern: Adapter (one mapping from the rows' errors to codes).

use std::path::Path;

use crate::ports::{Coded, ProfileCode, RowsError, SettingCode, params};

/// A `store-broken` with the file and the cause (`open` | `header` | `registry` | `steps` | `integrity` | `not-regular`).
#[must_use]
pub fn broken(file: &Path, cause: &str) -> Coded<SettingCode> {
    Coded::new(SettingCode::StoreBroken)
        .with_text(params::FILE, file.to_string_lossy())
        .with_text(params::CAUSE, cause)
}

/// `profile{cause}`: a profile rule broke (`exists`, `id-invalid`, `unknown`).
#[must_use]
pub fn profile(cause: &str) -> Coded<SettingCode> {
    Coded::new(SettingCode::Profile).with_text(params::CAUSE, cause)
}

/// A profile code (of the profile names) as the `profile` code of the store: same `cause`, the name kept.
#[must_use]
pub fn from_profile_code(c: Coded<ProfileCode>) -> Coded<SettingCode> {
    let erased = c.erase();
    let out = profile(erased.name);
    match erased.param(crate::ports::keys::NAME) {
        Some(crate::ports::Param::Text(name)) => {
            out.with_text(crate::ports::keys::NAME, name.clone())
        }
        _ => out,
    }
}

/// Maps `e` of the file `file`; `key` is the setting key a write was about (for `locked`, `reason-missing`).
#[must_use]
pub fn map(e: &RowsError, file: &Path, key: Option<&str>) -> Coded<SettingCode> {
    let with_key = |c: Coded<SettingCode>| match key {
        Some(k) => c.with_text(params::KEY, k),
        None => c,
    };
    match e {
        RowsError::Invalid(problems) => match problems.first().map(|p| p.reason.as_str()) {
            Some("locked") => with_key(Coded::new(SettingCode::Locked)),
            Some("reason-missing") => with_key(Coded::new(SettingCode::ReasonMissing)),
            Some(cause @ ("exists" | "id-invalid" | "unknown")) => profile(cause),
            _ => broken(file, "open"),
        },
        RowsError::Newer { found, known, .. } => Coded::new(SettingCode::StoreNewer)
            .with_text(params::FILE, file.to_string_lossy())
            .with_number(params::FOUND, u64::try_from(*found).unwrap_or(u64::MAX))
            .with_number(params::KNOWN, u64::try_from(*known).unwrap_or(u64::MAX)),
        RowsError::StepMismatch { .. } => broken(file, "steps"),
        RowsError::Unavailable(m) => {
            let m = m.to_lowercase();
            if m.contains("busy") || m.contains("locked") {
                Coded::new(SettingCode::StoreBusy).with_text(params::FILE, file.to_string_lossy())
            } else {
                broken(file, "open")
            }
        }
        _ => broken(file, "open"),
    }
}
