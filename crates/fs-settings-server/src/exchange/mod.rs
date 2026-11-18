//! Exchange: TOML 1.0 and JSON (RFC 8259) with the SAME structure, only to carry settings from one place to another (Q4).
//! The store is the database; these texts are never read by a program at run time.
//!
//! Structure (flat keys, one text): `[base]` = the person's base profile, `[profiles."<Name>"]` = named profiles (by NAME, never by
//! id), each with `general` and `programs.<program>` tables of `key = value` and `device` (the same two tables; only with `--device`);
//! `[instance]` = the instance defaults; `[locks."<key>"]` = `{reason, by, at}` (the locked value lies in `[instance]`).
//! Deterministic: keys sorted, so the same DB gives the same bytes.
//!
//! Pattern: Adapter (database <-> text) over the port, so the same code runs over the DB, the memory store and the RPC client.

mod doc;
mod export;
mod format;
mod import;
mod ops;
mod plan;

pub use doc::{Body, Doc, LockDoc, Scopes, V};
pub use export::{ExportScope, export};
pub use format::Format;
pub use import::{ImportEnv, ImportMode, ImportReport, import};
pub use ops::{Op, Target};
