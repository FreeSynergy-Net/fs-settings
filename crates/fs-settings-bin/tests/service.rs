//! The built `fs-settings` with a REAL service: KC15 (write, stop, start), Y4 (`export`/`import` refuse while a service holds the file),
//! Y5 (the OS user the service logs), and the import through the real command line (undeclared keys are never written).
//! [TESTISOLATION]: every service lives in its own temp folders (socket, data, `HOME`, every `XDG_*`) and is ended by a guard, also
//! when a test fails. Unix only: the socket is a file (a named pipe on Windows has no file to wait for).
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use fs_settings::{Place, ProgramName, Raw, Scope, SettingKey, SettingsStore};
use fs_settings_rpc::{ClientTimes, RpcSettingsStore};

fn bin(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fs-settings"));
    for (name, sub) in [
        ("HOME", "home"),
        ("XDG_RUNTIME_DIR", "run"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let dir = home.join(sub);
        std::fs::create_dir_all(&dir).unwrap();
        cmd.env(name, dir);
    }
    cmd
}

/// A running service; ended (stop, else kill) when dropped.
struct Service {
    child: Option<Child>,
    socket: PathBuf,
}

impl Service {
    fn start(home: &Path, data: &Path, instance: bool) -> Self {
        let socket = home.join("settings.sock");
        let mut cmd = bin(home);
        cmd.args(["serve", "--socket"])
            .arg(&socket)
            .arg("--data")
            .arg(data);
        if instance {
            cmd.arg("--instance");
        }
        let child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let service = Self {
            child: Some(child),
            socket,
        };
        let end = Instant::now() + Duration::from_secs(10);
        while !service.socket.exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(service.socket.exists(), "the service did not appear");
        service
    }

    /// `Control.stop`, then the exit code.
    fn stop(mut self) -> i32 {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            fs_rpc::stop_server(&self.socket).await.unwrap()
        });
        let mut child = self.child.take().unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status.code().unwrap_or(-1);
            }
            assert!(Instant::now() < end, "the service did not stop");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn times() -> ClientTimes {
    ClientTimes {
        call: Duration::from_secs(10),
        retry: Duration::from_secs(1),
    }
}

/// KC15: the built `fs-settings serve --socket S --data D` and an `RpcSettingsStore`: a value is written, `Control.stop` ends it with
/// exit 0, and after a new start the value is there.
#[test]
fn kc15_serve_write_stop_start() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("db");
    let tasks = ProgramName::new("tasks").unwrap();
    let key = SettingKey::parse("fs.shell.frontend").unwrap();

    let service = Service::start(home.path(), &data, false);
    let client = RpcSettingsStore::open(&service.socket, &tasks, times());
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
    assert_eq!(service.stop(), 0);

    let service = Service::start(home.path(), &data, false);
    let client = RpcSettingsStore::open(&service.socket, &tasks, times());
    let value = client
        .profile(&Scope::Program(tasks), Place::Profile, 0)
        .values
        .get(&key)
        .cloned();
    assert_eq!(value, Some(Raw::Str("day".to_owned())));
    drop(client);
    assert_eq!(service.stop(), 0);
}

/// Y4: while a service answers on the socket, `export` and `import` refuse with `store-busy{socket}` and leave the file as it was.
#[test]
fn y4_export_and_import_refuse_while_a_service_holds_the_file() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("db");
    let service = Service::start(home.path(), &data, false);
    let file = data.join("settings.db");
    let before = (
        std::fs::read(&file).unwrap(),
        std::fs::metadata(&file).unwrap().modified().unwrap(),
    );
    let out_file = home.path().join("out.toml");
    let text = home.path().join("in.toml");
    std::fs::write(
        &text,
        "[base.values.general]\n\"fs.shell.language\" = \"de\"\n",
    )
    .unwrap();
    for words in [
        vec![
            "export",
            "--format",
            "toml",
            "--out",
            out_file.to_str().unwrap(),
        ],
        vec!["import", text.to_str().unwrap()],
    ] {
        let out = bin(home.path())
            .args(&words)
            .args([
                "--data",
                data.to_str().unwrap(),
                "--socket",
                service.socket.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{words:?}");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.starts_with("setting-error-store-busy socket="),
            "{said}"
        );
    }
    assert!(!out_file.exists());
    assert_eq!(
        (
            std::fs::read(&file).unwrap(),
            std::fs::metadata(&file).unwrap().modified().unwrap()
        ),
        before
    );
    drop(service);
}

/// Y5: the instance log carries the OS user as the SERVICE found it for the connection (B2: `getuid` of the service, the same user),
/// not the name the CLI-side function would give; and the actor is the OS user of the caller, not a word of the command line.
#[cfg(target_os = "linux")]
#[test]
fn y5_the_service_logs_its_own_os_user() {
    use std::os::unix::fs::MetadataExt;
    let home = tempfile::tempdir().unwrap();
    let service = Service::start(home.path(), &home.path().join("db"), true);
    let socket = service.socket.to_str().unwrap().to_owned();
    let set = bin(home.path())
        .args([
            "instance",
            "set",
            "fs.shell.style",
            "calm",
            "--socket",
            &socket,
        ])
        .output()
        .unwrap();
    assert_eq!(
        set.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let log = bin(home.path())
        .args(["instance", "log", "--socket", &socket])
        .output()
        .unwrap();
    let line = String::from_utf8_lossy(&log.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    let uid = std::fs::metadata("/proc/self").unwrap().uid();
    assert!(line.contains(&format!("os_user={uid} ")), "{line}");
    assert!(line.contains("by=urn:fs:os-user:"), "{line}");
    drop(service);
}

/// The real command line: `import` without declarations refuses every key (`import-conflict{cause=not-declared}`, exit 1) and writes
/// NOTHING; the file it opened holds no value afterwards.
#[test]
fn import_on_the_command_line_never_writes_an_undeclared_key() {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("db");
    let socket = home.path().join("none.sock");
    let text = home.path().join("in.toml");
    std::fs::write(
        &text,
        "[base.values.general]\n\"fs.shell.language\" = \"de\"\n",
    )
    .unwrap();
    let out = bin(home.path())
        .args([
            "import",
            text.to_str().unwrap(),
            "--data",
            data.to_str().unwrap(),
            "--socket",
            socket.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("setting-error-import-conflict") && said.contains("cause=not-declared"),
        "{said}"
    );
    let exported = home.path().join("out.toml");
    let done = bin(home.path())
        .args([
            "export",
            "--format",
            "toml",
            "--out",
            exported.to_str().unwrap(),
        ])
        .args([
            "--data",
            data.to_str().unwrap(),
            "--socket",
            socket.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        done.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    assert!(
        !std::fs::read_to_string(&exported)
            .unwrap()
            .contains("fs.shell.language")
    );
}
