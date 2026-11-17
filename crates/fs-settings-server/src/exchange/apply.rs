//! Step three of an import: write the plan. One journal of what was there before; a failing step undoes the journal in reverse and the
//! created profiles are deleted again (the effective values are as before; stamps and tombstones of ids are not undone, ids are never
//! reused). Honest limit: this is an undo, not ONE transaction; the rows have no batch yet (request to part A, in the report).

use super::import::ImportEnv;
use super::plan::{Op, Target};
use crate::ports::{
    BasicValue, Coded, InstanceStore, LockChange, Place, ProfileName, ProfileRef, ProfileSet, Raw,
    Scope, SettingCode, SettingKey, SettingsStore, Stampd, params,
};

enum Undo {
    Value {
        scope: Scope,
        place: Place,
        id: u64,
        key: SettingKey,
        prior: Option<Raw>,
    },
    Instance {
        key: SettingKey,
        prior: Option<Raw>,
    },
    Lock {
        key: SettingKey,
        prior: Option<Raw>,
    },
}

fn missing(key: &str, which: &str) -> Coded<SettingCode> {
    Coded::new(SettingCode::ReadOnly)
        .with_text(params::WHICH, which)
        .with_text(params::KEY, key)
}

/// New names get `top + 1` each (the rule `index_step_ok` states). Returns the names that did not exist before.
fn create_profiles(
    store: &dyn SettingsStore,
    names: &[ProfileName],
) -> Result<Vec<ProfileName>, Coded<SettingCode>> {
    let before: Vec<ProfileName> = store.index().0.names().cloned().collect();
    store.update_index(&mut |set: &mut ProfileSet<u64>| {
        for name in names {
            if set.names().any(|n| n.same(name)) {
                continue;
            }
            let id = set.get(&ProfileRef::Base).copied().unwrap_or(0) + 1;
            if let Some(top) = set.get_mut(&ProfileRef::Base) {
                *top = id;
            }
            set.create(name.clone(), None)?;
            if let Some(v) = set.get_mut(&ProfileRef::Named(name.clone())) {
                *v = id;
            }
        }
        Ok(())
    })?;
    let mut created: Vec<ProfileName> = Vec::new();
    for name in names {
        if !before.iter().any(|n| n.same(name)) && !created.iter().any(|n| n.same(name)) {
            created.push(name.clone());
        }
    }
    Ok(created)
}

fn delete_profiles(
    store: &dyn SettingsStore,
    names: &[ProfileName],
) -> Result<(), Coded<SettingCode>> {
    store
        .update_index(&mut |set: &mut ProfileSet<u64>| {
            for name in names {
                set.delete(name)?;
            }
            Ok(())
        })
        .map(|_| ())
}

fn id_of(store: &dyn SettingsStore, target: &Target) -> u64 {
    match target {
        Target::Base => 0,
        Target::Named(n) => store
            .index()
            .0
            .get(&ProfileRef::Named(n.clone()))
            .copied()
            .unwrap_or(0),
    }
}

fn run_op(
    store: &dyn SettingsStore,
    instance: Option<&dyn InstanceStore>,
    who: &Stampd,
    op: &Op,
) -> Result<(Undo, bool), Coded<SettingCode>> {
    match op {
        Op::Value {
            target,
            scope,
            place,
            key,
            raw,
        } => {
            let id = id_of(store, target);
            let prior = store.profile(scope, *place, id).values.get(key).cloned();
            store.write(scope, *place, id, key, Some(raw))?;
            Ok((
                Undo::Value {
                    scope: scope.clone(),
                    place: *place,
                    id,
                    key: key.clone(),
                    prior,
                },
                true,
            ))
        }
        Op::Instance { key, raw } => {
            let inst = instance.ok_or_else(|| missing(key.as_str(), "instance"))?;
            let prior = inst.instance().values.values.get(key).cloned();
            let changed = inst.write_instance(key, Some(raw), who)?;
            Ok((
                Undo::Instance {
                    key: key.clone(),
                    prior,
                },
                changed,
            ))
        }
        Op::Lock { key, raw, reason } => {
            let inst = instance.ok_or_else(|| missing(key.as_str(), "instance"))?;
            let prior = inst.instance().values.values.get(key).cloned();
            let change = LockChange::Lock {
                value: raw.clone(),
                reason: reason.clone(),
            };
            let changed = inst.write_lock(key, &change, who)?;
            Ok((
                Undo::Lock {
                    key: key.clone(),
                    prior,
                },
                changed,
            ))
        }
    }
}

/// Undoes one entry; `false` = it could not be undone.
fn undo(
    store: &dyn SettingsStore,
    instance: Option<&dyn InstanceStore>,
    who: &Stampd,
    entry: &Undo,
) -> bool {
    match entry {
        Undo::Value {
            scope,
            place,
            id,
            key,
            prior,
        } => store.write(scope, *place, *id, key, prior.as_ref()).is_ok(),
        Undo::Instance { key, prior } => {
            instance.is_some_and(|i| i.write_instance(key, prior.as_ref(), who).is_ok())
        }
        Undo::Lock { key, prior } => instance.is_some_and(|i| {
            let unlock = LockChange::Unlock {
                reason: "import undone".to_owned(),
            };
            i.write_lock(key, &unlock, who).is_ok()
                && i.write_instance(key, prior.as_ref(), who).is_ok()
        }),
    }
}

/// Writes `ops`; returns how many changed something.
pub(super) fn apply(
    store: &dyn SettingsStore,
    instance: Option<&dyn InstanceStore>,
    env: &ImportEnv<'_>,
    ops: &[Op],
    step: &mut dyn FnMut(usize) -> Result<(), Coded<SettingCode>>,
) -> Result<u32, Coded<SettingCode>> {
    // Instance steps always have an actor (the plan refuses them otherwise); person values never write an actor row.
    let who = Stampd {
        by: env.by.cloned().unwrap_or_else(placeholder),
        at: env.now,
    };
    let names: Vec<ProfileName> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Value {
                target: Target::Named(n),
                ..
            } => Some(n.clone()),
            _ => None,
        })
        .collect();
    let created = create_profiles(store, &names)?;
    let (mut journal, mut applied) = (Vec::new(), 0_u32);
    let mut failure = None;
    for (i, op) in ops.iter().enumerate() {
        match step(i).and_then(|()| run_op(store, instance, &who, op)) {
            Ok((entry, changed)) => {
                applied += u32::from(changed);
                journal.push(entry);
            }
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    let Some(error) = failure else {
        return Ok(applied);
    };
    let stuck = journal
        .iter()
        .rev()
        .filter(|entry| !undo(store, instance, &who, entry))
        .count();
    let stuck = stuck + usize::from(delete_profiles(store, &created).is_err());
    Err(if stuck > 0 {
        error.with_number("undo-failed", u64::try_from(stuck).unwrap_or(u64::MAX))
    } else {
        error
    })
}

/// An actor for steps that never write an instance row (person values).
fn placeholder() -> crate::ports::Reference {
    // A constant, valid IRI; the fallback never reaches a row (see above).
    crate::ports::Reference::parse("urn:fs:import")
        .unwrap_or_else(|_| unreachable!("a constant IRI"))
}
