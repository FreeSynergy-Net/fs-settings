//! `export`: the settings as one deterministic text. Profiles by NAME, device values only with `device`, instance only when asked.
//! Pattern: Adapter (settings to the exchange document).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use super::Format;
use super::doc::{Body, Doc, LockDoc, Map, Scopes, V};
use crate::ports::{
    Coded, Place, ProfileRef, ProgramName, Scope, SettingCode, SettingsStore, params,
};

/// What to export.
#[derive(Clone, Debug)]
pub enum ExportScope {
    /// The person: the base profile and the named profiles, general and the given programs.
    Person(Vec<ProgramName>),
    /// One program's settings (and the general ones are left out).
    Program(ProgramName),
    /// The instance defaults and locks.
    Instance,
    /// Person and instance.
    All(Vec<ProgramName>),
}

/// The programs are named by the caller: the rows cannot list the programs that have values (request to part A, in the report).
fn programs_of(what: &ExportScope) -> (bool, Vec<ProgramName>, bool) {
    match what {
        ExportScope::Person(p) => (true, p.clone(), false),
        ExportScope::Program(p) => (false, vec![p.clone()], false),
        ExportScope::Instance => (false, Vec::new(), true),
        ExportScope::All(p) => (true, p.clone(), true),
    }
}

fn collect(
    store: &dyn SettingsStore,
    profile: u64,
    general: bool,
    programs: &[ProgramName],
    place: Place,
) -> Scopes {
    let mut out = Scopes::default();
    let texts = |scope: &Scope| -> Map {
        store
            .profile(scope, place, profile)
            .values
            .iter()
            .map(|(k, raw)| (k.as_str().to_owned(), V::from(raw)))
            .collect()
    };
    if general {
        out.general = texts(&Scope::General);
    }
    for p in programs {
        let map = texts(&Scope::Program(p.clone()));
        if !map.is_empty() {
            out.programs.insert(p.as_str().to_owned(), map);
        }
    }
    out
}

fn body(
    store: &dyn SettingsStore,
    profile: u64,
    general: bool,
    programs: &[ProgramName],
    device: bool,
) -> Body {
    Body {
        values: collect(store, profile, general, programs, Place::Profile),
        device: if device {
            collect(store, profile, general, programs, Place::Device)
        } else {
            Scopes::default()
        },
    }
}

/// Writes the exchange text of `what` to `out`. Same store state -> same bytes.
///
/// # Errors
/// `export-failed{file, kind}` (`kind` = `write` | `format`); `label` names the target in the error.
pub fn export(
    store: &dyn SettingsStore,
    what: &ExportScope,
    device: bool,
    format: Format,
    label: &Path,
    out: &mut dyn Write,
) -> Result<(), Coded<SettingCode>> {
    let failed = |kind: &str| {
        Coded::new(SettingCode::ExportFailed)
            .with_text(params::FILE, label.to_string_lossy())
            .with_text(params::KIND, kind)
    };
    let (person, programs, instance) = programs_of(what);
    let mut doc = Doc::default();
    let general = person;
    if person || !programs.is_empty() {
        doc.base = body(store, 0, general, &programs, device);
        let (set, _) = store.index();
        for name in set.names() {
            let id = set
                .get(&ProfileRef::Named(name.clone()))
                .copied()
                .unwrap_or(0);
            let b = body(store, id, general, &programs, device);
            if !b.is_empty() {
                doc.profiles.insert(name.as_str().to_owned(), b);
            }
        }
    }
    if instance {
        let layer = store.instance();
        doc.instance = layer
            .values
            .values
            .iter()
            .map(|(k, raw)| (k.as_str().to_owned(), V::from(raw)))
            .collect();
        let locks: BTreeMap<String, LockDoc> = layer
            .locks
            .iter()
            .map(|(k, l)| {
                let at = l.locked_at.to_rfc3339().unwrap_or_default();
                (
                    k.as_str().to_owned(),
                    LockDoc {
                        reason: l.reason.clone(),
                        by: l.locked_by.clone(),
                        at,
                    },
                )
            })
            .collect();
        doc.locks = locks;
    }
    let text = doc.write(format).map_err(|_| failed("format"))?;
    out.write_all(text.as_bytes()).map_err(|_| failed("write"))
}
