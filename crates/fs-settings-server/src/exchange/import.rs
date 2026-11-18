//! `import`: read a text, check EVERY value, then apply it. A broken text changes nothing (`import-invalid{file,line,cause}`); a value
//! that cannot be taken (locked, not declared, wrong type) is refused alone (`import-conflict{key,cause}`) and the rest goes in.
//!
//! Pattern: three steps (read -> plan -> apply); the plan is pure and writes nothing, so `DryRun` is the same code without step 3.

use std::io::Read;
use std::path::Path;

use super::Format;
use super::doc::Doc;
use super::plan::{invalid, plan};
use crate::db_store::DbSettingsStore;
use crate::ports::{Catalog, Coded, InstanceStore, Reference, SettingCode};

/// The biggest text an import reads: settings are small; a device file or a huge file is refused.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Report only, or apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMode {
    /// Say what would happen; change nothing.
    DryRun,
    /// Apply.
    Apply,
}

/// What an import needs besides the text.
#[derive(Clone, Copy, Debug)]
pub struct ImportEnv<'a> {
    /// The declarations every value is checked against. An undeclared key is NEVER written (`import-conflict{cause=not-declared}`);
    /// an empty catalog therefore refuses everything (the command line has no declarations of other programs yet).
    pub catalog: &'a Catalog,
    /// The actor (IRI) of instance writes; without it instance sections are refused.
    pub by: Option<&'a Reference>,
    /// Take device values too.
    pub device: bool,
}

/// The result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Values written (or, in a dry run, that would be).
    pub applied: u32,
    /// Values refused one by one (`import-conflict`).
    pub refused: Vec<Coded<SettingCode>>,
    /// Values that were there already.
    pub same: u32,
    /// Device values in the text that were left because `device` is off.
    pub skipped_device: u32,
}

/// Reads `src` as `format` and imports it into `store`: every value is checked first (nothing is written for a broken text), then
/// ALL writes of the plan run in ONE transaction of the database (`SettingRows::apply`): all or nothing, no undo journal.
///
/// # Errors
/// `import-invalid{file,line,cause}` for a broken text (nothing changed); the store's own error when the transaction fails (then
/// nothing was written).
pub fn import(
    store: &DbSettingsStore,
    src: &mut dyn Read,
    file: &Path,
    format: Format,
    mode: ImportMode,
    env: &ImportEnv<'_>,
) -> Result<ImportReport, Coded<SettingCode>> {
    let mut bytes = Vec::new();
    src.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid(file, 0, "read"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err(invalid(file, 0, "too-large"));
    }
    let text = String::from_utf8(bytes).map_err(|_| invalid(file, 0, "not-text"))?;
    let doc = Doc::read(&text, format).map_err(|(line, cause)| invalid(file, line, &cause))?;
    let instance: Option<&dyn InstanceStore> = store.is_instance().then_some(store);
    let planned = plan(&doc, store, instance, env, file)?;
    let mut report = ImportReport {
        refused: planned.refused,
        same: planned.same,
        skipped_device: planned.skipped_device,
        ..ImportReport::default()
    };
    report.applied = match mode {
        ImportMode::DryRun => u32::try_from(planned.ops.len()).unwrap_or(u32::MAX),
        ImportMode::Apply => store.apply_ops(&planned.ops, env.by)?,
    };
    Ok(report)
}
