//! KC12 (words, exit 2 before any service), KC15 (the real service and a client), KC18 (`--version`), on the BUILT binary.
//! No test touches the real user folders: socket and data lie in a `tempfile` folder; no process is left behind.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use fs_settings::{Place, ProgramName, Raw, Scope, SettingKey, SettingsStore};
use fs_settings_rpc::{ClientTimes, RpcSettingsStore};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fs-settings"))
}

/// KC18: `fs-settings --version` prints `fs-settings <version>` and exits 0.
#[test]
fn kc18_version() {
    let out = bin().arg("--version").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("fs-settings {}\n", env!("CARGO_PKG_VERSION"))
    );
}

/// KC12 (binary): wrong words end with 2 and `usage word=...`, before any service is asked.
#[test]
fn kc12_usage_exits_with_2() {
    for (words, word) in [
        (
            vec![
                "instance",
                "lock",
                "fs.shell.style",
                "calm",
                "--by",
                "https://example.org/a",
            ],
            "--reason",
        ),
        (vec!["instance", "set", "fs.shell.style", "calm"], "--by"),
        (vec!["nope"], "nope"),
    ] {
        let out = bin().args(&words).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{words:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr).trim(),
            format!("usage word={word}")
        );
    }
}

fn wait_for(socket: &Path) {
    let end = Instant::now() + Duration::from_secs(10);
    while !socket.exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(socket.exists(), "the service did not appear");
}

fn start(socket: &Path, data: &Path) -> Child {
    let child = bin()
        .args(["serve", "--socket"])
        .arg(socket)
        .arg("--data")
        .arg(data)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(socket);
    child
}

fn stop(socket: &Path, mut child: Child) -> i32 {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async { fs_rpc::stop_server(socket).await.unwrap() });
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code().unwrap_or(-1);
        }
        assert!(Instant::now() < end, "the service did not stop");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// KC15: the built `fs-settings serve --socket S --data D` and an `RpcSettingsStore`: a value is written, `Control.stop` ends it with
/// exit 0, and after a new start the value is there.
#[test]
fn kc15_serve_write_stop_start() {
    let dir = tempfile::tempdir().unwrap();
    let (socket, data) = (dir.path().join("settings.sock"), dir.path().join("data"));
    let times = ClientTimes {
        call: Duration::from_secs(10),
        retry: Duration::from_secs(1),
    };
    let tasks = ProgramName::new("tasks").unwrap();
    let key = SettingKey::parse("fs.shell.frontend").unwrap();

    let child = start(&socket, &data);
    let client = RpcSettingsStore::open(&socket, &tasks, times);
    client
        .write(
            &Scope::Program(tasks.clone()),
            Place::Profile,
            0,
            &key,
            Some(&Raw::Str("day".to_owned())),
        )
        .unwrap();
    drop(client);
    assert_eq!(stop(&socket, child), 0);

    let child = start(&socket, &data);
    let client = RpcSettingsStore::open(&socket, &tasks, times);
    let value = client
        .profile(&Scope::Program(tasks), Place::Profile, 0)
        .values
        .get(&key)
        .cloned();
    assert_eq!(value, Some(Raw::Str("day".to_owned())));
    drop(client);
    assert_eq!(stop(&socket, child), 0);
}
