//! The built `fs-settings` with a REAL service: KC15 (write, stop, start), Y4 (`export`/`import` refuse while a service holds the file),
//! Y5 (the OS user the service logs), and the import through the real command line (undeclared keys are never written).
//! [TESTISOLATION]: every service lives in its own temp folders (socket, data, `HOME`, every `XDG_*`) and is ended by a guard, also
//! when a test fails. Unix only: the socket is a file (a named pipe on Windows has no file to wait for).
#![cfg(unix)]

use fs_settings::{Place, ProgramName, Raw, Scope, SettingKey, SettingsStore};
use fs_settings_rpc::RpcSettingsStore;

mod service_kit;
use service_kit::{Service, bin, times};

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

/// Y5: the instance log carries the OS user as the SERVICE names it (the one form of B2: name, else number), and the actor is the OS
/// user of the caller, not a word of the command line. Since B2 and the command line share the form, the mutant "service takes the CLI-side
/// function" is EQUIVALENT in value and cannot be killed.
#[cfg(target_os = "linux")]
#[test]
fn y5_the_service_logs_its_own_os_user() {
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
    let user = fs_settings_rpc::os_user_name();
    assert!(line.contains(&format!("os_user={user} ")), "{line}");
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
