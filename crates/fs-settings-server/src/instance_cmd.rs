//! The `instance` commands over ANY `InstanceStore` (the RPC admin client in the program, the DB in tests): one call each, one
//! transaction each. `same` (nothing changed) is exit 0 and no log row. Time and OS user are set by the service; the `now` given here
//! is only filled into `Stampd` and ignored by the service.

use crate::ports::{
    BasicValue, Coded, InstanceStore, LockChange, ObjectId, Raw, Reference, SettingCode, SettingKey, Stampd, Utc, params,
};
use crate::reply::Reply;
use crate::words::InstanceCmd;

/// `true`/`false`, whole numbers, else text.
#[must_use]
pub fn infer_raw(text: &str) -> Raw {
    match text {
        "true" => Raw::Bool(true),
        "false" => Raw::Bool(false),
        _ => text.parse::<i64>().map_or_else(|_| Raw::Str(text.to_owned()), Raw::Int),
    }
}

fn shown(raw: &Raw) -> String {
    match raw {
        Raw::Bool(b) => b.to_string(),
        Raw::Int(n) => n.to_string(),
        Raw::Str(s) => s.clone(),
    }
}

/// An id as hex text (the log's paging word) and back.
#[must_use]
pub fn id_text(id: &ObjectId) -> String {
    id.0.iter().map(|b| format!("{b:02x}")).collect()
}

fn id_parse(text: &str) -> Option<ObjectId> {
    if text.len() != 32 || !text.is_ascii() {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(ObjectId(bytes))
}

fn who(by: &str, now: Utc) -> Result<Stampd, Coded<SettingCode>> {
    let by = Reference::parse(by).map_err(|_| Coded::new(SettingCode::KeyInvalid).with_text(params::BY, by))?;
    Ok(Stampd { by, at: now })
}

fn key(text: &str) -> Result<SettingKey, Coded<SettingCode>> {
    SettingKey::parse(text)
}

fn outcome(changed: Result<bool, Coded<SettingCode>>, what: &str, key: &str) -> Reply {
    match changed {
        Ok(true) => Reply::done(vec![format!("{what} {key}")]),
        Ok(false) => Reply::done(vec![format!("same {key}")]),
        Err(code) => Reply::failed(&code),
    }
}

/// Runs one command.
#[must_use]
pub fn run_instance(cmd: &InstanceCmd, store: &dyn InstanceStore, now: Utc) -> Reply {
    let result = (|| -> Result<Reply, Coded<SettingCode>> {
        Ok(match cmd {
            InstanceCmd::Show => {
                let layer = store.instance();
                let mut lines: Vec<String> = layer.values.values.iter().map(|(k, v)| format!("{}={}", k.as_str(), shown(v))).collect();
                lines.extend(layer.locks.iter().map(|(k, l)| format!("locked {} reason={} by={}", k.as_str(), l.reason, l.locked_by)));
                Reply::done(lines)
            }
            InstanceCmd::Set { key: k, value, by } => {
                outcome(store.write_instance(&key(k)?, Some(&infer_raw(value)), &who(by, now)?), "set", k)
            }
            InstanceCmd::Clear { key: k, by } => outcome(store.write_instance(&key(k)?, None, &who(by, now)?), "cleared", k),
            InstanceCmd::Lock { key: k, value, reason, by } => {
                let change = LockChange::Lock { value: infer_raw(value), reason: reason.clone() };
                outcome(store.write_lock(&key(k)?, &change, &who(by, now)?), "locked", k)
            }
            InstanceCmd::Unlock { key: k, reason, by } => {
                let change = LockChange::Unlock { reason: reason.clone() };
                outcome(store.write_lock(&key(k)?, &change, &who(by, now)?), "unlocked", k)
            }
            InstanceCmd::Log { after, size } => {
                let after = match after {
                    None => None,
                    Some(text) => Some(id_parse(text).ok_or_else(|| Coded::new(SettingCode::KeyInvalid).with_text(params::KEY, text.clone()))?),
                };
                Reply::done(
                    store
                        .instance_log(after, *size)
                        .iter()
                        .map(|e| {
                            format!(
                                "{} at={} by={} os_user={} action={} key={}{}",
                                id_text(&e.id),
                                e.at.to_rfc3339().unwrap_or_default(),
                                e.by,
                                e.os_user.as_deref().unwrap_or("-"),
                                e.action,
                                e.key.as_str(),
                                e.reason.as_ref().map(|r| format!(" reason={r}")).unwrap_or_default()
                            )
                        })
                        .collect(),
                )
            }
        })
    })();
    result.unwrap_or_else(|code| Reply::failed(&code))
}

