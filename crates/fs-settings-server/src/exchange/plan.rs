//! The plan of an import: every value read, named, checked; nothing written. Pure over the stores' read side.

use std::path::Path;

use super::doc::{Doc, Map, Scopes};
use super::import::ImportEnv;
use crate::ports::{
    BasicValue, Coded, InstanceStore, Layer, Place, ProfileName, ProfileRef, ProgramName, Raw,
    Reference, Scope, SettingCode, SettingKey, SettingsStore, params,
};

/// Whose profile a value goes into.
#[derive(Clone, Debug)]
pub(super) enum Target {
    Base,
    Named(ProfileName),
}

/// One write.
#[derive(Clone, Debug)]
pub(super) enum Op {
    Value {
        target: Target,
        scope: Scope,
        place: Place,
        key: SettingKey,
        raw: Raw,
    },
    Instance {
        key: SettingKey,
        raw: Raw,
    },
    Lock {
        key: SettingKey,
        raw: Raw,
        reason: String,
    },
}

#[derive(Default)]
pub(super) struct Plan {
    pub ops: Vec<Op>,
    pub refused: Vec<Coded<SettingCode>>,
    pub same: u32,
    pub unchecked: u32,
    pub skipped_device: u32,
}

pub(super) fn invalid(file: &Path, line: u64, cause: &str) -> Coded<SettingCode> {
    Coded::new(SettingCode::ImportInvalid)
        .with_text(params::FILE, file.to_string_lossy())
        .with_number(params::LINE, line)
        .with_text(params::CAUSE, cause)
}

fn conflict(key: &SettingKey, cause: &str) -> Coded<SettingCode> {
    Coded::new(SettingCode::ImportConflict)
        .with_text(params::KEY, key.as_str())
        .with_text(params::CAUSE, cause)
}

struct Ctx<'a> {
    file: &'a Path,
    store: &'a dyn SettingsStore,
    env: &'a ImportEnv<'a>,
    plan: Plan,
}

impl Ctx<'_> {
    fn key(&self, text: &str) -> Result<SettingKey, Coded<SettingCode>> {
        SettingKey::parse(text).map_err(|_| invalid(self.file, 0, "key-invalid"))
    }

    /// The declaration check; `false` = refused (already in `refused`).
    fn declared(&mut self, key: &SettingKey, raw: &Raw, layer: Layer) -> bool {
        let Some(catalog) = self.env.catalog else {
            self.plan.unchecked += 1;
            return true;
        };
        let Some(decl) = catalog.get(key) else {
            self.plan.refused.push(conflict(key, "not-declared"));
            return false;
        };
        if decl.kind.read(key, layer, raw).is_err() {
            self.plan.refused.push(conflict(key, "wrong-type"));
            return false;
        }
        true
    }

    fn scopes(
        &mut self,
        target: &Target,
        scopes: &Scopes,
        place: Place,
    ) -> Result<(), Coded<SettingCode>> {
        self.map(target, &Scope::General, place, &scopes.general)?;
        for (program, map) in &scopes.programs {
            let name =
                ProgramName::new(program).map_err(|_| invalid(self.file, 0, "program-invalid"))?;
            self.map(target, &Scope::Program(name), place, map)?;
        }
        Ok(())
    }

    fn map(
        &mut self,
        target: &Target,
        scope: &Scope,
        place: Place,
        map: &Map,
    ) -> Result<(), Coded<SettingCode>> {
        let locks = self.store.instance().locks;
        let id = match target {
            Target::Base => Some(0),
            Target::Named(n) => self
                .store
                .index()
                .0
                .get(&ProfileRef::Named(n.clone()))
                .copied(),
        };
        let layer = if matches!(target, Target::Base) {
            Layer::Person
        } else {
            Layer::NamedPerson
        };
        for (text, v) in map {
            let (key, raw) = (self.key(text)?, Raw::from(v));
            if locks.contains_key(&key) {
                self.plan.refused.push(conflict(&key, "locked"));
                continue;
            }
            if !self.declared(&key, &raw, layer) {
                continue;
            }
            let there = id.and_then(|i| {
                self.store
                    .profile(scope, place, i)
                    .values
                    .get(&key)
                    .cloned()
            });
            if there.as_ref() == Some(&raw) {
                self.plan.same += 1;
            } else {
                self.plan.ops.push(Op::Value {
                    target: target.clone(),
                    scope: scope.clone(),
                    place,
                    key,
                    raw,
                });
            }
        }
        Ok(())
    }

    fn profile(
        &mut self,
        target: &Target,
        body: &super::doc::Body,
    ) -> Result<(), Coded<SettingCode>> {
        self.scopes(target, &body.values, Place::Profile)?;
        if self.env.device {
            self.scopes(target, &body.device, Place::Device)
        } else {
            let n = body.device.general.len()
                + body.device.programs.values().map(Map::len).sum::<usize>();
            self.plan.skipped_device += u32::try_from(n).unwrap_or(u32::MAX);
            Ok(())
        }
    }

    fn instance(
        &mut self,
        doc: &Doc,
        instance: Option<&dyn InstanceStore>,
    ) -> Result<(), Coded<SettingCode>> {
        for (text, lock) in &doc.locks {
            if !doc.instance.contains_key(text) {
                return Err(invalid(self.file, 0, "lock-without-value"));
            }
            Reference::parse(&lock.by).map_err(|_| invalid(self.file, 0, "lock-by"))?;
        }
        let there = instance.map(<dyn InstanceStore>::instance);
        for (text, v) in &doc.instance {
            let (key, raw) = (self.key(text)?, Raw::from(v));
            let Some(layer) = there.as_ref().filter(|_| self.env.by.is_some()) else {
                let cause = if instance.is_none() {
                    "no-instance"
                } else {
                    "no-actor"
                };
                self.plan.refused.push(conflict(&key, cause));
                continue;
            };
            let wants_lock = doc.locks.get(text);
            let held = layer.locks.get(&key);
            if !self.declared(&key, &raw, Layer::Instance) {
                continue;
            }
            let same_value = layer.values.values.get(&key) == Some(&raw);
            match (wants_lock, held) {
                (Some(l), Some(h)) if same_value && h.reason == l.reason => self.plan.same += 1,
                (_, Some(_)) => self.plan.refused.push(conflict(&key, "locked")),
                (Some(l), None) => self.plan.ops.push(Op::Lock {
                    key,
                    raw,
                    reason: l.reason.clone(),
                }),
                (None, None) if same_value => self.plan.same += 1,
                (None, None) => self.plan.ops.push(Op::Instance { key, raw }),
            }
        }
        Ok(())
    }
}

/// Reads the whole text into a plan; `Err` = the text is broken (nothing may be changed).
pub(super) fn plan(
    doc: &Doc,
    store: &dyn SettingsStore,
    instance: Option<&dyn InstanceStore>,
    env: &ImportEnv<'_>,
    file: &Path,
) -> Result<Plan, Coded<SettingCode>> {
    let mut ctx = Ctx {
        file,
        store,
        env,
        plan: Plan::default(),
    };
    ctx.profile(&Target::Base, &doc.base)?;
    for (name, body) in &doc.profiles {
        let n = ProfileName::new(name).map_err(|_| invalid(file, 0, "profile-name"))?;
        ctx.profile(&Target::Named(n), body)?;
    }
    ctx.instance(doc, instance)?;
    Ok(ctx.plan)
}
