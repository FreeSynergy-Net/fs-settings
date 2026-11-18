//! The OS user in the log comes from the operating system, never from an environment variable (`USER`, `LOGNAME`, `USERNAME` can be
//! set by anyone). Its own test binary: it sets environment variables, which must not race with other tests.

use fs_settings::{InstanceStore, Stampd};
use fs_settings_server::system_os_user;

mod support;
use support::{at, db_file, iri, key, open_instance, raw_str};

const FORGED: &str = "forged-by-environment";

fn forge() {
    for name in ["USER", "LOGNAME", "USERNAME"] {
        // SAFETY: every test of this binary sets the SAME values and none reads the environment (the code under test asks the system); the
        // tests run in parallel threads, which is why the values never differ.
        unsafe { std::env::set_var(name, FORGED) };
    }
}

/// The OS user ignores a forged environment.
#[test]
fn os_user_ignores_the_environment() {
    forge();
    let got = system_os_user()().expect("the OS knows the user of this process");
    assert_ne!(got, "");
    assert_ne!(got, FORGED);
}

/// The instance log carries the OS user of the store's default source, not the forged one.
#[test]
fn the_log_does_not_carry_a_forged_user() {
    forge();
    let dir = tempfile::tempdir().unwrap();
    let store = open_instance(&db_file(dir.path()), 5);
    let who = Stampd {
        by: iri("https://example.org/admin"),
        at: at(1),
    };
    assert!(
        store
            .write_instance(&key("fs.shell.style"), Some(&raw_str("calm")), &who)
            .unwrap()
    );
    let log = store.instance_log(None, 10);
    assert_eq!(log.len(), 1);
    let user = log[0]
        .os_user
        .as_deref()
        .expect("the log names the OS user");
    assert_ne!(user, FORGED);
}

/// The actor of a command without a connection is the id of the process user, the same IRI the service derives from a connection
/// (`urn:fs:os-user:<id>`), and a forged environment does not change it.
#[cfg(target_os = "linux")]
#[test]
fn the_command_line_actor_is_the_id_of_the_process_user() {
    use fs_types::BasicValue;
    use std::os::unix::fs::MetadataExt;
    forge();
    let uid = std::fs::metadata("/proc/self").unwrap().uid();
    let actor = fs_settings_server::os_actor().unwrap();
    assert_eq!(actor.canonical(), format!("urn:fs:os-user:{uid}"));
}

/// One form per user: the actor is made of the ID (uid, SID); a name, even one with spaces, is display only and never the actor.
#[test]
fn the_actor_is_the_id_and_never_the_name() {
    use fs_types::BasicValue;
    let unix = fs_settings_server::actor_of_user("1000", Some("ada".to_owned()));
    assert_eq!(unix.canonical(), "urn:fs:os-user:1000");
    let windows = fs_settings_server::actor_of_user(
        "S-1-5-21-111-222-333-1001",
        Some("Ada Lovelace".to_owned()),
    );
    assert_eq!(
        windows.canonical(),
        "urn:fs:os-user:S-1-5-21-111-222-333-1001"
    );
    let nameless = fs_settings_server::actor_of_user("1000", None);
    assert_eq!(nameless.canonical(), "urn:fs:os-user:1000");
}
