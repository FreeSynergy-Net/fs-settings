//! The words of `fs-settings` (parsing only; no side effects). Pattern: Command objects built from the words; the CLI runs them.
//!
//! `fs-settings --version | --help`, `serve [--instance] [--data DIR | --settings FILE] [--socket PATH]`,
//! `instance show | set KEY VALUE | clear KEY | lock KEY VALUE --reason TEXT | unlock KEY --reason TEXT | log [--after ID] [--size N]`
//! (`--by IRI` mandatory for every change), `export`, `import`, `repair`.

use std::path::PathBuf;

/// A wrong word: exit code 2, printed as `usage word=<word>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    /// The word that is wrong or missing (`--by`, `--reason`, an unknown word, `command`).
    pub word: String,
}

fn usage(word: &str) -> Usage {
    Usage { word: word.to_owned() }
}

/// Option values and positional words of one command.
#[derive(Debug, Default)]
pub struct Args {
    /// `--name value` pairs, in order.
    pub options: Vec<(String, String)>,
    /// Flags without a value (`--dry-run`, `--instance`, `--device`, `--legacy`).
    pub flags: Vec<String>,
    /// Positional words.
    pub positional: Vec<String>,
}

const FLAGS: [&str; 5] = ["--dry-run", "--instance", "--device", "--legacy", "--help"];

impl Args {
    /// Splits `words`; an option without its value is `usage{word=<option>}`.
    ///
    /// # Errors
    /// [`Usage`].
    pub fn parse(words: &[String]) -> Result<Self, Usage> {
        let mut out = Self::default();
        let mut it = words.iter();
        while let Some(w) = it.next() {
            if FLAGS.contains(&w.as_str()) {
                out.flags.push(w.clone());
            } else if w.starts_with("--") {
                let v = it.next().filter(|v| !v.starts_with("--")).ok_or_else(|| usage(w))?;
                out.options.push((w.clone(), v.clone()));
            } else {
                out.positional.push(w.clone());
            }
        }
        Ok(out)
    }

    /// The last value of `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.options.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// All values of `name` (`--program` may repeat).
    #[must_use]
    pub fn all(&self, name: &str) -> Vec<&str> {
        self.options.iter().filter(|(k, _)| k == name).map(|(_, v)| v.as_str()).collect()
    }

    /// The flag is there.
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    /// Mandatory option.
    ///
    /// # Errors
    /// `usage{word=name}`.
    pub fn need(&self, name: &str) -> Result<&str, Usage> {
        self.get(name).ok_or_else(|| usage(name))
    }

    /// Refuses an option or flag that is not in `known`.
    ///
    /// # Errors
    /// `usage{word=<the unknown one>}`.
    pub fn only(&self, known: &[&str]) -> Result<(), Usage> {
        let bad = self.options.iter().map(|(k, _)| k).chain(self.flags.iter()).find(|k| !known.contains(&k.as_str()));
        bad.map_or(Ok(()), |k| Err(usage(k)))
    }
}

/// The words of `serve`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServeWords {
    /// The place of the DB.
    pub place: crate::paths::DbPlace,
    /// `--instance`.
    pub instance: bool,
    /// `--socket`.
    pub socket: Option<PathBuf>,
}

/// Reads the words after `serve`.
///
/// # Errors
/// [`Usage`] (also `--data` together with `--settings`).
pub fn serve_words(args: &Args) -> Result<ServeWords, Usage> {
    args.only(&["--data", "--settings", "--socket", "--instance"])?;
    let instance = args.flag("--instance");
    let place = match (args.get("--data"), args.get("--settings")) {
        (Some(_), Some(_)) => return Err(usage("--settings")),
        (Some(dir), None) => crate::paths::DbPlace::Dir(PathBuf::from(dir)),
        (None, Some(file)) => crate::paths::DbPlace::File(PathBuf::from(file)),
        (None, None) => crate::paths::DbPlace::Convention { instance },
    };
    Ok(ServeWords { place, instance, socket: args.get("--socket").map(PathBuf::from) })
}

/// One `instance` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstanceCmd {
    /// Show defaults and locks.
    Show,
    /// Set a default.
    Set { key: String, value: String, by: String },
    /// Clear a default.
    Clear { key: String, by: String },
    /// Set and lock.
    Lock { key: String, value: String, reason: String, by: String },
    /// Unlock (reason mandatory).
    Unlock { key: String, reason: String, by: String },
    /// The log.
    Log { after: Option<String>, size: u32 },
}

/// Reads the words after `instance`.
///
/// # Errors
/// [`Usage`]: unknown command, missing `--by` (every change), missing `--reason` (lock, unlock), wrong count of words.
pub fn instance_cmd(words: &[String]) -> Result<InstanceCmd, Usage> {
    let (Some(cmd), rest) = (words.first(), words.get(1..).unwrap_or_default()) else { return Err(usage("command")) };
    let args = Args::parse(rest)?;
    let pos = |n: usize| -> Result<Vec<String>, Usage> {
        if args.positional.len() == n { Ok(args.positional.clone()) } else { Err(usage("count")) }
    };
    let by = || args.need("--by").map(str::to_owned);
    match cmd.as_str() {
        "show" => {
            args.only(&[])?;
            pos(0).map(|_| InstanceCmd::Show)
        }
        "set" => {
            args.only(&["--by"])?;
            let p = pos(2)?;
            Ok(InstanceCmd::Set { key: p[0].clone(), value: p[1].clone(), by: by()? })
        }
        "clear" => {
            args.only(&["--by"])?;
            Ok(InstanceCmd::Clear { key: pos(1)?[0].clone(), by: by()? })
        }
        "lock" => {
            args.only(&["--by", "--reason"])?;
            let p = pos(2)?;
            Ok(InstanceCmd::Lock { key: p[0].clone(), value: p[1].clone(), reason: args.need("--reason")?.to_owned(), by: by()? })
        }
        "unlock" => {
            args.only(&["--by", "--reason"])?;
            Ok(InstanceCmd::Unlock { key: pos(1)?[0].clone(), reason: args.need("--reason")?.to_owned(), by: by()? })
        }
        "log" => {
            args.only(&["--after", "--size"])?;
            pos(0)?;
            let size = args.get("--size").map_or(Ok(100), |s| s.parse::<u32>().map_err(|_| usage("--size")))?;
            Ok(InstanceCmd::Log { after: args.get("--after").map(str::to_owned), size })
        }
        other => Err(usage(other)),
    }
}
