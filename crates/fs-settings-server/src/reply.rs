//! What a command answers: exit code and lines. Codes are printed as `setting-error-<code> key=value ...`, never as a sentence
//! (ADR 0057: the words belong to the program that shows them).

use crate::ports::{Coded, Param, SettingCode};

/// The answer of a command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    /// 0 done (also `same`), 1 failed, 2 wrong words.
    pub exit: i32,
    /// Lines for stdout.
    pub out: Vec<String>,
    /// Lines for stderr.
    pub err: Vec<String>,
}

/// `setting-error-<code> k=v ...`.
#[must_use]
pub fn code_line(code: &Coded<SettingCode>) -> String {
    let erased = code.clone().erase();
    let mut line = format!("{}{}", erased.prefix, erased.name);
    for (k, v) in &erased.params {
        match v {
            Param::Text(t) => line.push_str(&format!(" {k}={t}")),
            Param::Number(n) => line.push_str(&format!(" {k}={n}")),
            _ => {}
        }
    }
    line
}

impl Reply {
    /// Done, printing `lines`.
    #[must_use]
    pub fn done(lines: Vec<String>) -> Self {
        Self {
            exit: 0,
            out: lines,
            err: Vec::new(),
        }
    }

    /// Failed with a code (exit 1).
    #[must_use]
    pub fn failed(code: &Coded<SettingCode>) -> Self {
        Self {
            exit: 1,
            out: Vec::new(),
            err: vec![code_line(code)],
        }
    }

    /// Wrong words (exit 2): `usage word=<word>`.
    #[must_use]
    pub fn usage(word: &str) -> Self {
        Self {
            exit: 2,
            out: Vec::new(),
            err: vec![format!("usage word={word}")],
        }
    }
}
