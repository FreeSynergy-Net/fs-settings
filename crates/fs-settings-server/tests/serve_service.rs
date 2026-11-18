//! The service the program builds (`build_service`): a claimed `by` and a refused connection both reach the program's log; neither hook
//! can be left off. The real transport admits only the server's own user today (H1), so a refused caller cannot be produced on the wire:
//! the hooks are called as the service calls them.

use std::sync::{Arc, Mutex};

use fs_settings_server::{LogSink, build_service};

mod support;
use support::{db_file, open_instance};

fn collector() -> (LogSink, Arc<Mutex<Vec<String>>>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    (
        Arc::new(move |l: &str| sink.lock().unwrap().push(l.to_owned())),
        lines,
    )
}

/// A claimed `by` is written to the log (quoted) and a refused connection too: the lines are the ones of fs-settings-rpc.
#[test]
fn a_claim_and_a_refused_connection_are_logged() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(open_instance(&db_file(dir.path()), 1));
    let (log, lines) = collector();
    let service = build_service(Arc::clone(&store), Some(store), log);
    (service
        .claimed_by
        .as_ref()
        .expect("the claim hook is wired"))("https://example.org/forged");
    (service
        .refused_note
        .as_ref()
        .expect("the refused hook is wired"))("caller-credentials");
    let logged = lines.lock().unwrap().clone();
    assert_eq!(
        logged,
        vec![
            fs_settings_rpc::claim_line("https://example.org/forged"),
            fs_settings_rpc::refused_line("caller-credentials"),
        ]
    );
    assert!(
        logged[0].contains("claimed-by=\"https://example.org/forged\" ignored"),
        "{logged:?}"
    );
}
