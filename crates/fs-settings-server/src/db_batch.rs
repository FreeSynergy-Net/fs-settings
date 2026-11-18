//! The batch of the database: an import's writes in ONE transaction (`SettingRows::apply`), and the list of programs for export.
//! Time and OS user are the store's own, never the caller's.
//!
//! Pattern: Adapter (the command list of the exchange -> the rows' batch).

use crate::db_store::DbSettingsStore;
use crate::error_map::map;
use crate::exchange::{Op, Target};
use crate::ports::{
    Actor, Coded, IndexChange, Outcome, ProfileName, ProfileRef, ProgramName, Reference, Scope,
    SettingCode, SettingOp, Writer,
};
use crate::prim::{place_row, scope_row, to_prim};

impl DbSettingsStore {
    /// True when this file family is the instance database (`serve --instance`): instance writes are possible.
    #[must_use]
    pub fn is_instance(&self) -> bool {
        self.instance_mode
    }

    /// The programs that have a value (export names them; the rows list them).
    ///
    /// # Errors
    /// The store's code (an emergency store has no programs: `Ok(empty)`).
    pub fn known_programs(&self) -> Result<Vec<ProgramName>, Coded<SettingCode>> {
        let Some(store) = &self.store else {
            return Ok(Vec::new());
        };
        let read = async {
            let rows = self.rows(store).await?;
            rows.programs().await.map_err(|e| map(&e, &self.file, None))
        };
        // A stored name that is no program name cannot be written by this program; it is left out, never guessed.
        Ok(self
            .run(read)?
            .iter()
            .filter_map(|n| ProgramName::new(n).ok())
            .collect())
    }

    /// Runs `ops` in ONE transaction: new named profiles are created in it (ids `top + 1`, ...), then every write. The first failing
    /// operation rolls everything back. Returns how many operations were run (0 when the batch changed nothing).
    ///
    /// # Errors
    /// `read-only` in emergency mode; the rows' error of the failing operation (`actor-missing` for an instance operation without
    /// `by`); nothing is written then.
    pub fn apply_ops(&self, ops: &[Op], by: Option<&Reference>) -> Result<u32, Coded<SettingCode>> {
        let store = self.writable("person")?;
        let (set, _) = self.index();
        let top = set.get(&ProfileRef::Base).copied().unwrap_or(0);
        let mut known: Vec<(ProfileName, i64)> = set
            .names()
            .filter_map(|n| {
                set.get(&ProfileRef::Named(n.clone()))
                    .map(|id| (n.clone(), i64::try_from(*id).unwrap_or(0)))
            })
            .collect();
        let mut next = i64::try_from(top).unwrap_or(0);
        let mut batch: Vec<SettingOp<'_>> = Vec::new();
        for op in ops {
            if let Op::Value {
                target: Target::Named(n),
                ..
            } = op
                && !known.iter().any(|(k, _)| k.same(n))
            {
                next += 1;
                known.push((n.clone(), next));
                let name = n.as_str().to_owned();
                batch.push(SettingOp::Index(IndexChange::Create {
                    id: next,
                    name_key: name.to_lowercase(),
                    name,
                }));
            }
        }
        for op in ops {
            batch.push(match op {
                Op::Value {
                    target,
                    scope,
                    place,
                    key,
                    raw,
                } => {
                    let profile = match target {
                        Target::Base => 0,
                        Target::Named(n) => known
                            .iter()
                            .find(|(k, _)| k.same(n))
                            .map_or(0, |(_, id)| *id),
                    };
                    let via = match scope {
                        Scope::General => None,
                        Scope::Program(p) => Some(p.as_str()),
                    };
                    SettingOp::Put {
                        profile,
                        scope: scope_row(scope),
                        place: place_row(*place),
                        key: key.as_str(),
                        v: Some(to_prim(raw)),
                        w: Writer { by: None, via },
                    }
                }
                Op::Instance { key, raw } => SettingOp::PutInstance {
                    key: key.as_str(),
                    v: Some(to_prim(raw)),
                },
                Op::Lock { key, raw, reason } => SettingOp::Lock {
                    key: key.as_str(),
                    v: to_prim(raw),
                    reason,
                },
            });
        }
        let (at, os_user) = ((self.now)(), (self.os_user)());
        let actor = by.map(|by| Actor {
            by,
            os_user: os_user.as_deref(),
        });
        let run = async {
            let rows = self.rows(store).await?;
            rows.apply(&batch, actor, at)
                .await
                .map_err(|e| map(&e, &self.file, None))
        };
        let outcome = self.run(run)?;
        Ok(match outcome {
            Outcome::Changed { .. } => u32::try_from(ops.len()).unwrap_or(u32::MAX),
            _ => 0,
        })
    }
}
