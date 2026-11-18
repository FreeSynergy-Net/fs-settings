//! A claimed `by` is logged by the service and is never the actor. Unix only (the socket is a file).
#![cfg(unix)]

use std::process::Stdio;

use fs_settings::{InstanceStore, Raw, SettingKey, Stampd};
use fs_settings_rpc::RpcInstanceAdmin;
use fs_types::BasicValue;

mod service_kit;
use service_kit::{Service, bin, times};

/// A `by` that a client claims over the wire is logged by the service (`claimed-by="..." ignored`) and is NEVER the actor: the instance
/// log names the OS user of the connection.
#[test]
fn a_claimed_by_is_logged_and_never_the_actor() {
    let home = tempfile::tempdir().unwrap();
    let log_file = home.path().join("service.log");
    let log = std::fs::File::create(&log_file).unwrap();
    let service =
        Service::start_logged(home.path(), &home.path().join("db"), true, Stdio::from(log));
    let forged = "https://example.org/forged";
    let admin = RpcInstanceAdmin::open(&service.socket, forged, times());
    let key = SettingKey::parse("fs.shell.style").unwrap();
    let who = Stampd {
        by: fs_types::Reference::parse("https://example.org/also-forged").unwrap(),
        at: fs_types::Utc::from_millis(0),
    };
    admin
        .write_instance(&key, Some(&Raw::Str("calm".to_owned())), &who)
        .unwrap();
    drop(admin);
    let socket = service.socket.to_str().unwrap().to_owned();
    let log_out = bin(home.path())
        .args(["instance", "log", "--socket", &socket])
        .output()
        .unwrap();
    let line = String::from_utf8_lossy(&log_out.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    assert!(
        line.contains("by=urn:fs:os-user:") && !line.contains("forged"),
        "{line}"
    );
    drop(service);
    let logged = std::fs::read_to_string(&log_file).unwrap();
    assert!(
        logged.contains("claimed-by=\"https://example.org/forged\" ignored"),
        "{logged}"
    );
}
