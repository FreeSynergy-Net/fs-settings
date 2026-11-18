//! `fs-settings repair`: the ONLY way to a new file after a broken one (ENTWURF C.3). Human only; the service must not run on the file.
//!
//! Pattern: Command. Nothing is deleted: the old file (and its `-wal` / `-shm`) are renamed, never overwritten.

use std::path::{Path, PathBuf};

use crate::clock::fixed_clock;
use crate::db_store::DbSettingsStore;
use crate::error_map::broken;
use crate::instance_source::NoInstance;
use crate::paths::DbPlace;
use crate::ports::{Coded, Health, SettingCode, Utc, params};

/// What `repair` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairReport {
    /// Where the old file lies now (`settings.db.broken-<UTC yyyymmddThhmmssZ>`).
    pub moved_to: PathBuf,
    /// The newest export next to it (`settings-export*.toml|json`): an offer, never applied without the person's `import`.
    pub offer: Option<PathBuf>,
}

/// `yyyymmddThhmmssZ` of `at`.
fn stamp_text(at: Utc) -> String {
    let text = at
        .to_rfc3339()
        .unwrap_or_else(|| "19700101T000000".to_owned());
    let mut out: String = text
        .chars()
        .take(19)
        .filter(|c| *c != '-' && *c != ':')
        .collect();
    out.push('Z');
    out
}

fn with_suffix(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// The target name for the old file: never an existing one (a counter is appended), so a second repair never overwrites a first.
fn free_target(file: &Path, at: Utc) -> PathBuf {
    let base = with_suffix(file, &format!(".broken-{}", stamp_text(at)));
    let mut candidate = base.clone();
    let mut n = 1_u32;
    while candidate.exists()
        || with_suffix(&candidate, "-wal").exists()
        || with_suffix(&candidate, "-shm").exists()
    {
        n += 1;
        candidate = with_suffix(&base, &format!("-{n}"));
    }
    candidate
}

fn newest_export(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let ext = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let is_export = name.starts_with("settings-export")
            && (ext.eq_ignore_ascii_case("toml") || ext.eq_ignore_ascii_case("json"));
        let Some(time) = entry
            .metadata()
            .ok()
            .filter(|m| is_export && m.is_file())
            .and_then(|m| m.modified().ok())
        else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| time > *t) {
            best = Some((time, entry.path()));
        }
    }
    best.map(|(_, path)| path)
}

/// Moves `file` aside, creates a new database at the old place and reports.
///
/// # Errors
/// `file-unreadable{file}` when there is no file; `store-broken{cause=open}` when a rename fails (then nothing else was changed
/// beyond the renames already done, which are reported by the error's file) or the new database does not open.
pub fn repair(file: &Path, now: Utc) -> Result<RepairReport, Coded<SettingCode>> {
    if std::fs::symlink_metadata(file).is_err() {
        return Err(
            Coded::new(SettingCode::FileUnreadable).with_text(params::FILE, file.to_string_lossy())
        );
    }
    let target = free_target(file, now);
    std::fs::rename(file, &target).map_err(|_| broken(file, "open"))?;
    for side in ["-wal", "-shm"] {
        let old = with_suffix(file, side);
        if old.exists() {
            std::fs::rename(&old, with_suffix(&target, side)).map_err(|_| broken(&old, "open"))?;
        }
    }
    let fresh = DbSettingsStore::open(
        &DbPlace::File(file.to_path_buf()),
        false,
        Box::new(NoInstance),
        fixed_clock(now),
    )?;
    if fresh.health != Health::Ok {
        return Err(broken(file, "open"));
    }
    let offer = file.parent().and_then(newest_export);
    Ok(RepairReport {
        moved_to: target,
        offer,
    })
}
