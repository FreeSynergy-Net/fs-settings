//! KC12 and KC13: the `instance` commands over the instance database (in process; the binary's own tests are in fs-settings-bin).

use fs_settings_server::{Args, InstanceCmd, Reply, instance_cmd, run_instance};

mod support;
use support::{at, db_file, open_instance};

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

fn run(store: &fs_settings_server::DbSettingsStore, text: &str) -> Reply {
    match instance_cmd(&words(text)) {
        Ok(cmd) => run_instance(&cmd, store, at(0)),
        Err(usage) => Reply::usage(&usage.word),
    }
}

const BY: &str = "--by https://example.org/admin";

/// KC12: `lock` without `--reason` is usage (exit 2, word `--reason`); with a blank reason `reason-missing` (exit 1); without `--by`
/// usage; the same `set` twice is `same` (exit 0, one log row); `unlock --reason x` frees; the log shows the OS user of the service.
#[test]
fn kc12_words_and_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_instance(&db_file(dir.path()), 1_700_000_000_000).with_os_user(std::sync::Arc::new(|| Some("service-user".to_owned())));

    let no_reason = run(&store, &format!("lock fs.shell.style calm {BY}"));
    assert_eq!((no_reason.exit, no_reason.err), (2, vec!["usage word=--reason".to_owned()]));

    let mut blank = instance_cmd(&words(&format!("lock fs.shell.style calm --reason x {BY}"))).unwrap();
    if let InstanceCmd::Lock { reason, .. } = &mut blank {
        *reason = "  ".to_owned();
    }
    let blank = run_instance(&blank, &store, at(0));
    assert_eq!(blank.exit, 1);
    assert!(blank.err[0].starts_with("setting-error-reason-missing key=fs.shell.style"), "{:?}", blank.err);

    let no_by = run(&store, "set fs.shell.style calm");
    assert_eq!((no_by.exit, no_by.err), (2, vec!["usage word=--by".to_owned()]));

    assert_eq!(run(&store, &format!("set fs.shell.style calm {BY}")).out, vec!["set fs.shell.style"]);
    let same = run(&store, &format!("set fs.shell.style calm {BY}"));
    assert_eq!((same.exit, same.out), (0, vec!["same fs.shell.style".to_owned()]));

    assert_eq!(run(&store, &format!("lock fs.shell.style calm --reason policy {BY}")).exit, 0);
    let locked = run(&store, &format!("clear fs.shell.style {BY}"));
    assert_eq!(locked.exit, 1);
    assert!(locked.err[0].starts_with("setting-error-locked key=fs.shell.style"));
    assert_eq!(run(&store, &format!("unlock fs.shell.style --reason maintenance-over {BY}")).exit, 0);

    let log = run(&store, "log");
    assert_eq!(log.out.len(), 3, "set (once), lock, unlock: the repeated set left no row: {:?}", log.out);
    assert!(log.out.iter().all(|l| l.contains("os_user=service-user") && l.contains("at=2023-11-14T22:13:20.000Z")), "{:?}", log.out);
}

/// KC13: `log --after` pages without gaps and in the order of time.
#[test]
fn kc13_log_pages_without_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_instance(&db_file(dir.path()), 1_700_000_000_000);
    for n in 0..5 {
        assert_eq!(run(&store, &format!("set fs.shell.slot-{n} {n} {BY}")).exit, 0);
    }
    let all = run(&store, "log --size 100").out;
    assert_eq!(all.len(), 5);
    let mut seen = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let cmd = after.as_ref().map_or_else(|| "log --size 2".to_owned(), |id| format!("log --size 2 --after {id}"));
        let page = run(&store, &cmd).out;
        if page.is_empty() {
            break;
        }
        after = page.last().and_then(|l| l.split_whitespace().next()).map(str::to_owned);
        seen.extend(page);
    }
    assert_eq!(seen, all, "pages joined = the whole log, no row twice, none missing");
}

/// The words of `Args` (no database): an option without a value is usage of that option.
#[test]
fn args_without_value_is_usage() {
    let err = Args::parse(&words("--by")).unwrap_err();
    assert_eq!(err.word, "--by");
}
