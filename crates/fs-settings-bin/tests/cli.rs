//! KC18 (`--version`) and KC12 (wrong words end with 2, before any service is asked), on the BUILT binary. [TESTISOLATION]: every
//! run gets its own temp folders for `HOME` and every `XDG_*` variable, so no test touches the real user folders or sockets.

use std::path::Path;
use std::process::Command;

/// The binary with an isolated environment below `home`.
fn bin(home: &Path) -> Command {
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

/// Ends a service that a command of the test may have started by itself (the command line starts one when a word reaches the service
/// stage; a wrong word must not, but a broken program would): every test leaves NO process behind. [TESTISOLATION]
fn end_stray_services(home: &Path) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    for name in ["settings.sock", "settings-instance.sock"] {
        let socket = home.join("run").join("freesynergy").join(name);
        if socket.exists() {
            let _ = local.block_on(&rt, fs_rpc::stop_server(&socket));
        }
    }
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
        end_stray_services(home.path());
        assert_eq!(out.status.code(), Some(2), "{words:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr).trim(),
            format!("usage word={word}")
        );
    }
}
