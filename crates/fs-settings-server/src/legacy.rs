//! The one-time read of PUBLISHED files (ENTWURF C.7): the language files (ADR 0057) and the frontend choice of fs-tasks. The files are
//! only read: never written, never deleted, byte-identical afterwards. `settings.toml` and `instance.toml` are NOT imported (never
//! published: no extra code, c0), `session.toml` has no file at all.
//!
//! Runs at the first start of a NEW person database (and on `fs-settings import --legacy`). A value is taken only when its key is absent
//! (O10); a value that does not fit is a line in the report, never silent.

use std::path::{Path, PathBuf};

use fs_types::Language;

use crate::ports::{
    Coded, Place, ProgramName, Raw, Scope, SettingCode, SettingKey, SettingsStore, params,
};

/// A language tag is a few dozen bytes: a bigger file is refused (as the words crate does).
const MAX_LANGUAGE_BYTES: u64 = 64;
/// The frontend file of fs-tasks is small, too.
const MAX_FRONTEND_BYTES: u64 = 4096;
/// The values of the frontend choice (fs-tasks `frontend.rs`).
const FRONTENDS: [&str; 3] = ["classic", "board", "day"];

/// Where the old files lie.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LegacyEnv {
    /// `$XDG_CONFIG_HOME` (else `$HOME/.config`): the language files.
    pub config_home: Option<PathBuf>,
    /// `$XDG_DATA_HOME` (else `$HOME/.local/share`): the fs-tasks file.
    pub data_home: Option<PathBuf>,
}

impl LegacyEnv {
    /// The places of this process (Unix; on Windows there are no such old files: nothing to read).
    #[must_use]
    pub fn from_process() -> Self {
        if cfg!(windows) {
            return Self::default();
        }
        let abs = |v: Option<std::ffi::OsString>| v.map(PathBuf::from).filter(|p| p.is_absolute());
        let home = abs(std::env::var_os("HOME"));
        Self {
            config_home: abs(std::env::var_os("XDG_CONFIG_HOME"))
                .or_else(|| home.as_ref().map(|h| h.join(".config"))),
            data_home: abs(std::env::var_os("XDG_DATA_HOME"))
                .or_else(|| home.as_ref().map(|h| h.join(".local").join("share"))),
        }
    }
}

/// One line of the report: what became of one old file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyLine {
    /// Taken over as `key` (in `scope`).
    Imported {
        /// The old file.
        file: PathBuf,
        /// The new key.
        key: String,
    },
    /// The key had a value already: left as it is.
    Kept {
        /// The old file.
        file: PathBuf,
        /// The key.
        key: String,
    },
    /// The file does not fit or could not be read, or the store refused: said, not taken.
    Problem {
        /// The old file.
        file: PathBuf,
        /// What is wrong (`file-broken`, `file-too-large`, `import-conflict`, the store's code).
        code: Coded<SettingCode>,
    },
}

fn read_small(file: &Path, limit: u64) -> Result<Option<String>, Coded<SettingCode>> {
    let shown = file.to_string_lossy();
    let Ok(meta) = std::fs::symlink_metadata(file) else {
        return Ok(None);
    };
    let wrong = |code: SettingCode| Coded::new(code).with_text(params::FILE, shown.clone());
    if !meta.is_file() {
        return Err(wrong(SettingCode::NotRegular));
    }
    if meta.len() > limit {
        return Err(wrong(SettingCode::FileTooLarge));
    }
    let bytes = std::fs::read(file).map_err(|_| wrong(SettingCode::FileUnreadable))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| wrong(SettingCode::FileBroken))
}

/// Takes `raw` into `scope` as `key` unless there is a value; one line either way.
fn take(store: &dyn SettingsStore, scope: &Scope, key: &str, raw: Raw, file: &Path) -> LegacyLine {
    let problem = |code: Coded<SettingCode>| LegacyLine::Problem {
        file: file.to_path_buf(),
        code,
    };
    let Ok(setting) = SettingKey::parse(key) else {
        return problem(Coded::new(SettingCode::KeyInvalid).with_text(params::KEY, key));
    };
    if store
        .profile(scope, Place::Profile, 0)
        .values
        .contains_key(&setting)
    {
        return LegacyLine::Kept {
            file: file.to_path_buf(),
            key: key.to_owned(),
        };
    }
    match store.write(scope, Place::Profile, 0, &setting, Some(&raw)) {
        Ok(()) => LegacyLine::Imported {
            file: file.to_path_buf(),
            key: key.to_owned(),
        },
        Err(code) => problem(code),
    }
}

fn language(store: &dyn SettingsStore, scope: &Scope, file: &Path, out: &mut Vec<LegacyLine>) {
    match read_small(file, MAX_LANGUAGE_BYTES) {
        Ok(None) => {}
        Err(code) => out.push(LegacyLine::Problem {
            file: file.to_path_buf(),
            code,
        }),
        Ok(Some(text)) => match Language::parse(text.trim()) {
            Ok(tag) => out.push(take(
                store,
                scope,
                "fs.shell.language",
                Raw::Str(tag.canonical()),
                file,
            )),
            Err(_) => out.push(LegacyLine::Problem {
                file: file.to_path_buf(),
                code: Coded::new(SettingCode::FileBroken)
                    .with_text(params::FILE, file.to_string_lossy()),
            }),
        },
    }
}

fn frontend(store: &dyn SettingsStore, file: &Path, out: &mut Vec<LegacyLine>) {
    let broken =
        || Coded::new(SettingCode::FileBroken).with_text(params::FILE, file.to_string_lossy());
    let text = match read_small(file, MAX_FRONTEND_BYTES) {
        Ok(None) => return,
        Err(code) => {
            return out.push(LegacyLine::Problem {
                file: file.to_path_buf(),
                code,
            });
        }
        Ok(Some(text)) => text,
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return out.push(LegacyLine::Problem {
            file: file.to_path_buf(),
            code: broken(),
        });
    };
    // `frontend` is the word; `face` the one before the rename (ADR: Frontend, never Gesicht).
    let Some(word) = json
        .get("frontend")
        .or_else(|| json.get("face"))
        .and_then(serde_json::Value::as_str)
    else {
        return; // a file without the choice: nothing to take
    };
    let Ok(tasks) = ProgramName::new("tasks") else {
        return;
    };
    if FRONTENDS.contains(&word) {
        out.push(take(
            store,
            &Scope::Program(tasks),
            "fs.shell.frontend",
            Raw::Str(word.to_owned()),
            file,
        ));
    } else {
        let code = Coded::new(SettingCode::ImportConflict)
            .with_text(params::KEY, "fs.shell.frontend")
            .with_text(params::CAUSE, "wrong-type");
        out.push(LegacyLine::Problem {
            file: file.to_path_buf(),
            code,
        });
    }
}

/// Reads the old files once and takes what is not there yet. One line per file found; nothing for a file that is not there.
#[must_use]
pub fn import_legacy(store: &dyn SettingsStore, env: &LegacyEnv) -> Vec<LegacyLine> {
    let mut out = Vec::new();
    if let Some(config) = &env.config_home {
        let root = config.join("freesynergy");
        language(store, &Scope::General, &root.join("language"), &mut out);
        let mut programs: Vec<(String, PathBuf)> = std::fs::read_dir(&root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok().map(|n| (n, e.path())))
            .collect();
        programs.sort();
        for (name, dir) in programs {
            if let Ok(program) = ProgramName::new(&name) {
                language(
                    store,
                    &Scope::Program(program),
                    &dir.join("language"),
                    &mut out,
                );
            }
        }
    }
    if let Some(data) = &env.data_home {
        frontend(
            store,
            &data.join("freesynergy").join("tasks").join("settings.json"),
            &mut out,
        );
    }
    out
}
