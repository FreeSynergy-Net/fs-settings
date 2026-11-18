//! The OS user in the log comes from the operating system, never from an environment variable (`USER`, `LOGNAME`, `USERNAME` can be
//! set by anyone). Its own test binary with ONE test: the environment is set once, in this thread, before anything else runs, and every check
//! follows in the same thread (`set_var` is unsafe because another thread may read the environment at the same moment; there is none).

use fs_settings::{InstanceStore, Stampd};
use fs_settings_server::system_os_user;

mod support;
use support::{at, db_file, iri, key, open_instance, raw_str};

const FORGED: &str = "forged-by-environment";

fn forge() {
    for name in ["USER", "LOGNAME", "USERNAME"] {
        // SAFETY: this binary has one test, run on one thread; nothing reads or writes the environment at the same time.
        unsafe { std::env::set_var(name, FORGED) };
    }
}

/// The OS user ignores a forged environment.
fn os_user_ignores_the_environment() {
    let got = system_os_user()().expect("the OS knows the user of this process");
    assert_ne!(got, "");
    assert_ne!(got, FORGED);
}

/// The instance log carries the OS user of the store's default source, not the forged one.
fn the_log_does_not_carry_a_forged_user() {
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
fn the_command_line_actor_is_the_id_of_the_process_user() {
    use fs_types::BasicValue;
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self").unwrap().uid();
    let actor = fs_settings_server::os_actor().unwrap();
    assert_eq!(actor.canonical(), format!("urn:fs:os-user:{uid}"));
}

/// A forged environment changes nothing: the user, the log, the actor.
#[test]
fn a_forged_environment_changes_nothing() {
    forge();
    os_user_ignores_the_environment();
    the_log_does_not_carry_a_forged_user();
    #[cfg(target_os = "linux")]
    the_command_line_actor_is_the_id_of_the_process_user();
}
