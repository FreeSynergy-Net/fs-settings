//! The profile index as the port knows it (`ProfileSet<u64>`: base value = the highest id ever issued, a named profile's value = its id)
//! and as the rows know it (`IndexRows`, `IndexChange`). Pure functions; tested without a database.
//! Pattern: Adapter (pure functions between two models of the profile index).

use crate::error_map::from_profile_code;
use crate::ports::{
    Coded, IndexChange, IndexRows, ProfileName, ProfileRef, ProfileSet, SettingCode,
};

fn id_of(set: &ProfileSet<u64>, r: &ProfileRef) -> u64 {
    set.get(r).copied().unwrap_or(0)
}

/// The rows -> the set. Tombstones are not in it; `top` is the base value.
///
/// # Errors
/// `profile{..}` when a stored name is not a legal profile name (a damaged row, said, never skipped).
pub fn to_set(ix: &IndexRows) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
    let mut set = ProfileSet::new(u64::try_from(ix.top).unwrap_or(0));
    let mut live: Vec<_> = ix
        .profiles
        .iter()
        .filter(|p| p.deleted.is_none() && p.id > 0)
        .collect();
    live.sort_by_key(|p| p.id);
    for p in &live {
        let name = ProfileName::new(p.name.as_deref().unwrap_or("")).map_err(from_profile_code)?;
        set.create(name.clone(), None).map_err(from_profile_code)?;
        if let Some(v) = set.get_mut(&ProfileRef::Named(name)) {
            *v = u64::try_from(p.id).unwrap_or(0);
        }
    }
    if ix.active > 0
        && let Some(p) = live.iter().find(|p| p.id == ix.active)
    {
        let name = ProfileName::new(p.name.as_deref().unwrap_or("")).map_err(from_profile_code)?;
        set.switch(ProfileRef::Named(name))
            .map_err(from_profile_code)?;
    }
    Ok(set)
}

fn named(set: &ProfileSet<u64>) -> Vec<(u64, ProfileName)> {
    let mut all: Vec<_> = set
        .names()
        .map(|n| (id_of(set, &ProfileRef::Named(n.clone())), n.clone()))
        .collect();
    all.sort_by_key(|(id, _)| *id);
    all
}

fn active_id(set: &ProfileSet<u64>) -> u64 {
    match set.active() {
        ProfileRef::Base => 0,
        named @ ProfileRef::Named(_) => id_of(set, named),
    }
}

fn signed(id: u64) -> i64 {
    i64::try_from(id).unwrap_or(i64::MAX)
}

/// What turns `old` into `new`, in the order the rows apply it: deletes, renames, creates, the switch. Ids are the identity of a
/// profile (a name can change, an id never); `index_step_ok` has checked `new` already.
#[must_use]
pub fn changes(old: &ProfileSet<u64>, new: &ProfileSet<u64>) -> Vec<IndexChange> {
    let (before, after) = (named(old), named(new));
    let mut out = Vec::new();
    for (id, _) in &before {
        if !after.iter().any(|(i, _)| i == id) {
            out.push(IndexChange::Delete { id: signed(*id) });
        }
    }
    for (id, name) in &after {
        if let Some((_, was)) = before.iter().find(|(i, _)| i == id)
            && was.as_str() != name.as_str()
        {
            out.push(IndexChange::Rename {
                id: signed(*id),
                name: name.as_str().to_owned(),
                name_key: key_of(name),
            });
        }
    }
    for (id, name) in &after {
        if !before.iter().any(|(i, _)| i == id) {
            out.push(IndexChange::Create {
                id: signed(*id),
                name: name.as_str().to_owned(),
                name_key: key_of(name),
            });
        }
    }
    if active_id(old) != active_id(new) {
        out.push(IndexChange::Switch {
            id: signed(active_id(new)),
        });
    }
    out
}

/// The comparison key of a name: the same rule as `ProfileName::same` (case-insensitive).
fn key_of(name: &ProfileName) -> String {
    name.as_str().to_lowercase()
}
