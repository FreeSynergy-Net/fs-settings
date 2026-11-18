//! The OS user in the log comes from the operating system, never from an environment variable (`USER`, `LOGNAME`, `USERNAME` can be
//! set by anyone). Its own test binary: it sets environment variables, which must not race with other tests.

use fs_settings::{InstanceStore, Stampd};
use fs_settings_server::system_os_user;

mod support;
use support::{at, db_file, iri, key, open_instance, raw_str};

const FORGED: &str = "forged-by-environment";

fn forge() {
    for name in ["USER", "LOGNAME", "USERNAME"] {
        // SAFETY: the only test of this binary; nothing else reads or writes the environment while it runs.
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
