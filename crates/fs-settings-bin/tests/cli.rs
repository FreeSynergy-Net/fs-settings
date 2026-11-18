//! KC18 (`--version`) and KC12 (wrong words end with 2, before any service is asked), on the BUILT binary. [TESTISOLATION]: every
//! run gets its own temp folders for `HOME` and every `XDG_*` variable, so no test touches the real user folders or sockets.

use std::path::Path;
use std::process::Command;

/// The binary with an isolated environment below `home`.
pub fn bin(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fs-settings"));
    for (name, sub) in [
        ("HOME", "home"),
        ("XDG_RUNTIME_DIR", "run"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_STATE_HOME", "state"),
        ("LOCALAPPDATA", "localappdata"),
    ] {
        let dir = home.join(sub);
        std::fs::create_dir_all(&dir).unwrap();
        cmd.env(name, dir);
    }
    cmd
}

/// KC18: `fs-settings --version` prints `fs-settings <version>` and exits 0.
#[test]
fn kc18_version() {
    let home = tempfile::tempdir().unwrap();
    let out = bin(home.path()).arg("--version").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("fs-settings {}\n", env!("CARGO_PKG_VERSION"))
    );
}

/// KC12 (binary): wrong words end with 2 and `usage word=...`, before any service is asked. `--by` is no word: the actor is the OS
/// user of the caller, never an argument.
#[test]
fn kc12_usage_exits_with_2() {
    for (words, word) in [
        (
            vec!["instance", "lock", "fs.shell.style", "calm"],
            "--reason",
        ),
        (
            vec![
                "instance",
                "set",
                "fs.shell.style",
                "calm",
                "--by",
                "https://example.org/forged",
            ],
            "--by",
        ),
        (vec!["nope"], "nope"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let out = bin(home.path()).args(&words).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{words:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr).trim(),
            format!("usage word={word}")
        );
    }
}
