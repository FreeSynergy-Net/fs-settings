//! Helpers of the tests. Nothing here touches the real user folders: every path lies in a `tempfile` folder (never /tmp by hand,
//! never `$HOME`). Windows-safe: paths are joined, no Unix-only assumption except where a test says `#[cfg(unix)]`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use fs_types::coded::Coded;
use fs_settings::{
    Catalog, Declaration, GroupId, Kind, ProgramName, Raw, Scope, SettingCode, SettingKey, Value,
};
use fs_settings_server::exchange::ImportEnv;
use fs_settings_server::{DbPlace, DbSettingsStore, NoInstance, fixed_clock};
use fs_types::{BasicValue, Reference, Utc};

pub fn at(ms: i64) -> Utc {
    Utc::from_millis(ms)
}

pub fn key(text: &str) -> SettingKey {
    SettingKey::parse(text).unwrap()
}

pub fn program(name: &str) -> Scope {
    Scope::Program(ProgramName::new(name).unwrap())
}

pub fn iri(text: &str) -> Reference {
    Reference::parse(text).unwrap()
}

pub fn db_file(dir: &Path) -> PathBuf {
    dir.join("settings.db")
}

/// A person store on `dir/settings.db`, clock fixed at `ms`.
pub fn open_at(file: &Path, ms: i64) -> DbSettingsStore {
    DbSettingsStore::open(
        &DbPlace::File(file.to_path_buf()),
        false,
        Box::new(NoInstance),
        fixed_clock(at(ms)),
    )
    .unwrap()
}

pub fn open(file: &Path) -> DbSettingsStore {
    open_at(file, 1_700_000_000_000)
}

/// The instance store (instance tables active) on `file`.
pub fn open_instance(file: &Path, ms: i64) -> DbSettingsStore {
    DbSettingsStore::open(
        &DbPlace::File(file.to_path_buf()),
        true,
        Box::new(NoInstance),
        fixed_clock(at(ms)),
    )
    .unwrap()
}

/// What proves "the file stayed as it was": all bytes, the length and the modification time.
pub fn fingerprint(file: &Path) -> (Vec<u8>, u64, SystemTime) {
    let meta = std::fs::metadata(file).unwrap();
    (
        std::fs::read(file).unwrap(),
        meta.len(),
        meta.modified().unwrap(),
    )
}

/// The names in a folder, sorted (no new file may appear).
pub fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The `cause` or any text parameter of a code.
pub fn text_of(code: &Coded<SettingCode>, name: &str) -> Option<String> {
    code.params()
        .find(|(k, _)| *k == name)
        .and_then(|(_, v)| match v {
            fs_types::coded::Param::Text(t) => Some(t.clone()),
            _ => None,
        })
}

pub fn is_code(code: &Coded<SettingCode>, expected: SettingCode) -> bool {
    code.code() == expected
}

pub fn raw_str(text: &str) -> Raw {
    Raw::Str(text.to_owned())
}

/// The declarations of the shell keys the tests use (text values).
pub fn catalog() -> Catalog {
    let group = GroupId::new("shell").unwrap();
    let decl = |k: &str| {
        Declaration::new(
            key(k),
            Kind::Text { max_chars: 64 },
            Value::Text(String::new()),
            group.clone(),
        )
    };
    Catalog::of(
        ["language", "frontend", "scheme", "style"]
            .iter()
            .map(|l| decl(&format!("fs.shell.{l}")))
            .collect(),
    )
    .unwrap()
}

pub fn env<'a>(by: Option<&'a fs_types::Reference>, catalog: &'a Catalog) -> ImportEnv<'a> {
    ImportEnv {
        catalog,
        by,
        device: true,
    }
}
