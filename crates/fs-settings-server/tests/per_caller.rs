//! The service of EACH connection is built from its caller: the actor is `urn:fs:os-user:<Caller::id>` (the name is display only), two
//! callers never share an identity (H2), a caller the system cannot name is refused and logged, a claimed `by` is logged and never the
//! actor. The callers are injected here (`Caller::new`): the real transport admits only the server's own user today (H1).

use std::sync::{Arc, Mutex};

use fs_rpc::{Caller, CallerError, PerConnection};
use fs_settings_server::{PerCaller, actor_of_caller, os_user_label};
use fs_types::BasicValue;

mod support;
use support::{db_file, open_instance};

fn per_caller(dir: &std::path::Path) -> (PerCaller, Arc<Mutex<Vec<String>>>) {
    let store = Arc::new(open_instance(&db_file(dir), 1));
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    let log: fs_settings_server::LogSink =
        Arc::new(move |l| sink.lock().unwrap().push(l.to_owned()));
    (PerCaller::new(Arc::clone(&store), Some(store), log), lines)
}

/// The actor is made of the id, not of the name; the name is the label of the log only (the id when there is none).
#[test]
fn the_actor_is_the_id_and_the_name_is_display_only() {
    let alice = Caller::new("4242", Some("alice".to_owned()));
    assert_eq!(actor_of_caller(&alice).canonical(), "urn:fs:os-user:4242");
    assert_eq!(os_user_label(&alice), "alice");
    let nameless = Caller::new("S-1-5-21-1", None);
    assert_eq!(
        actor_of_caller(&nameless).canonical(),
        "urn:fs:os-user:S-1-5-21-1"
    );
    assert_eq!(os_user_label(&nameless), "S-1-5-21-1");
}

/// H2: every connection gets a service of its own: two callers give two actors and two labels; the second is never the first.
#[test]
fn each_connection_gets_its_own_identity() {
    let dir = tempfile::tempdir().unwrap();
    let (per, _) = per_caller(dir.path());
    let (a, b) = (
        Caller::new("1001", Some("ada".to_owned())),
        Caller::new("1002", Some("bob".to_owned())),
    );
    let (sa, sb) = (per.service_for(&a), per.service_for(&b));
    assert_eq!(
        (sa.actor_of_connection)().canonical(),
        "urn:fs:os-user:1001"
    );
    assert_eq!(
        (sb.actor_of_connection)().canonical(),
        "urn:fs:os-user:1002"
    );
    assert_eq!((sa.os_user)().as_deref(), Some("ada"));
    assert_eq!((sb.os_user)().as_deref(), Some("bob"));
    // and again for the first caller: still its own, not whoever came last
    assert_eq!(
        (per.service_for(&a).actor_of_connection)().canonical(),
        "urn:fs:os-user:1001"
    );
}

/// A claimed `by` is logged, quoted, and ignored: the hook is wired, the actor stays the caller's.
#[test]
fn a_claim_is_logged_and_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let (per, lines) = per_caller(dir.path());
    let caller = Caller::new("1001", Some("ada".to_owned()));
    let service = per.service_for(&caller);
    (service
        .claimed_by
        .as_ref()
        .expect("the claim hook is wired"))("https://example.org/forged\nfs-settings: injected");
    let logged = lines.lock().unwrap().clone();
    assert_eq!(logged.len(), 1, "{logged:?}");
    assert_eq!(
        logged[0],
        "claimed-by=\"https://example.org/forged\\nfs-settings: injected\" ignored"
    );
    assert_eq!(
        (service.actor_of_connection)().canonical(),
        "urn:fs:os-user:1001"
    );
}

/// A connection whose caller the system does not tell is refused and the program says so (it is never served as nobody).
#[test]
fn a_caller_error_is_logged() {
    let dir = tempfile::tempdir().unwrap();
    let (mut per, lines) = per_caller(dir.path());
    per.refused(&CallerError::Credentials(std::io::Error::other(
        "no credentials",
    )));
    let logged = lines.lock().unwrap().clone();
    assert_eq!(logged.len(), 1, "{logged:?}");
    assert!(
        logged[0].starts_with("refused connection: caller-credentials"),
        "{logged:?}"
    );
}
