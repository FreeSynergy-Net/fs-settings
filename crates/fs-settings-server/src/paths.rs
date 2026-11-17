//! Where the database lies: by convention, changed ONLY by a start word (c0 decisions 3 and 4).
//!
//! Pattern: pure function over an injected environment (`resolve_in`); `resolve` feeds it the process environment. The Unix rules
//! are those of fs-rpc `default_data_dir("settings")` (a relative `$XDG_DATA_HOME` is ignored); they are repeated here as a pure
//! function so that the Windows places can be tested on every system.

use std::path::PathBuf;

use crate::ports::{Coded, SettingCode, params};

/// The file name inside the folder.
pub const DB_NAME: &str = "settings.db";

/// Where the DB lies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbPlace {
    /// `--settings FILE`: exactly this file.
    File(PathBuf),
    /// `--data DIR`: `DIR/settings.db`.
    Dir(PathBuf),
    /// Neither: the person's or (with `--instance`) the instance's place by convention.
    Convention {
        /// The instance place instead of the person's.
        instance: bool,
    },
}

/// The operating system whose places are meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    /// Unix-like.
    Unix,
    /// Windows.
    Windows,
}

impl Os {
    /// The system this program runs on.
    #[must_use]
    pub const fn here() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }

    const fn sep(self) -> char {
        match self {
            Self::Unix => '/',
            Self::Windows => '\\',
        }
    }

    fn absolute(self, text: &str) -> bool {
        match self {
            Self::Unix => text.starts_with('/'),
            Self::Windows => {
                text.starts_with("\\\\")
                    || text.as_bytes().get(1) == Some(&b':')
                        && text.as_bytes().get(2) == Some(&b'\\')
            }
        }
    }
}

/// The environment variables the convention reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Env {
    /// `$XDG_DATA_HOME`.
    pub xdg_data_home: Option<String>,
    /// `$HOME`.
    pub home: Option<String>,
    /// `%LOCALAPPDATA%`.
    pub local_app_data: Option<String>,
    /// `%ProgramData%`.
    pub program_data: Option<String>,
}

impl Env {
    /// The environment of this process.
    #[must_use]
    pub fn from_process() -> Self {
        let get = |k: &str| std::env::var_os(k).and_then(|v| v.into_string().ok());
        Self {
            xdg_data_home: get("XDG_DATA_HOME"),
            home: get("HOME"),
            local_app_data: get("LOCALAPPDATA"),
            program_data: get("ProgramData"),
        }
    }
}

fn no_folder() -> Coded<SettingCode> {
    Coded::new(SettingCode::NoFolder).with_text(params::WHICH, "data")
}

/// `base` + `parts`, joined with the separator of `os` (the native `PathBuf` join when `os` is this system).
fn join(os: Os, base: &str, parts: &[&str]) -> PathBuf {
    if os == Os::here() {
        let mut p = PathBuf::from(base);
        p.extend(parts);
        return p;
    }
    let mut text = base.trim_end_matches(os.sep()).to_owned();
    for part in parts {
        text.push(os.sep());
        text.push_str(part);
    }
    PathBuf::from(text)
}

/// The file for `place` in the environment `env` of system `os`.
///
/// # Errors
/// `no-folder{which=data}` when no convention applies (no `$HOME`, no `%LOCALAPPDATA%`) and no start word gave the place.
pub fn resolve_in(place: &DbPlace, os: Os, env: &Env) -> Result<PathBuf, Coded<SettingCode>> {
    match place {
        DbPlace::File(file) => Ok(file.clone()),
        DbPlace::Dir(dir) => Ok(dir.join(DB_NAME)),
        DbPlace::Convention { instance: true } => match os {
            Os::Unix => Ok(join(os, "/var/lib", &["freesynergy", "instance", DB_NAME])),
            Os::Windows => {
                let base = env
                    .program_data
                    .as_deref()
                    .filter(|p| os.absolute(p))
                    .ok_or_else(no_folder)?;
                Ok(join(os, base, &["FreeSynergy", "instance", DB_NAME]))
            }
        },
        DbPlace::Convention { instance: false } => match os {
            Os::Unix => {
                let home_share = env.home.as_deref().filter(|h| os.absolute(h)).map(|h| {
                    join(os, h, &[".local", "share"])
                        .to_string_lossy()
                        .into_owned()
                });
                let base = env
                    .xdg_data_home
                    .as_deref()
                    .filter(|x| os.absolute(x))
                    .map(str::to_owned)
                    .or(home_share)
                    .ok_or_else(no_folder)?;
                Ok(join(os, &base, &["freesynergy", "settings", DB_NAME]))
            }
            Os::Windows => {
                let base = env
                    .local_app_data
                    .as_deref()
                    .filter(|p| os.absolute(p))
                    .ok_or_else(no_folder)?;
                Ok(join(os, base, &["FreeSynergy", "settings", DB_NAME]))
            }
        },
    }
}

/// [`resolve_in`] for this system and this process's environment.
///
/// # Errors
/// As [`resolve_in`].
pub fn resolve(place: &DbPlace) -> Result<PathBuf, Coded<SettingCode>> {
    resolve_in(place, Os::here(), &Env::from_process())
}
