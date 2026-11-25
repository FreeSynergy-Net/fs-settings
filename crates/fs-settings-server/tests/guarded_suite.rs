//! The suite `module_v1` level 1 through the service of the program (`SettingsWithForm`) over the real database store (Stufe 4 C3).
//! Pattern: none (tests).

mod guarded_support;
mod support;

use std::sync::Arc;

use fs_interfaces::formsource_capnp::form_source;
use fs_module::conformance::run_conformance;
use fs_module::{
    CommandKind, DeclaredCommand, DeclaredForm, FormSourceFixture, Param, Sample, Verify,
};
use guarded_support::{FORM, PERSON_ID, WEEK, connect, make_core, run, with_form};

/// The suite's view of the settings module: one connection of a person (the fixture of fs-settings-rpc, on the database store).
struct Fixture {
    client: form_source::Client,
}

fn param(name: &str, required: bool) -> Param {
    Param {
        name: name.into(),
        required,
    }
}

impl FormSourceFixture for Fixture {
    fn client(&self) -> form_source::Client {
        self.client.clone()
    }
    fn level(&self) -> u32 {
        fs_settings_rpc::MODULE_LEVEL
    }
    fn provider(&self) -> Option<&str> {
        Some("settings")
    }
    fn forms(&self) -> Vec<DeclaredForm> {
        let fields = [
            "label",
            "basic",
            "value",
            "group",
            "hint",
            "default",
            "choices",
            "min",
            "max",
            "layer",
            "locked",
            "lock-reason",
            "order",
        ];
        vec![DeclaredForm {
            key: FORM.into(),
            scope: String::new(),
            fields: fields.map(String::from).to_vec(),
        }]
    }
    fn commands(&self) -> Vec<DeclaredCommand> {
        let optional = [param("key", false), param("program", false)];
        let set = [vec![param("value", true)], optional.to_vec()].concat();
        let command = |key: &str, params| DeclaredCommand {
            key: key.into(),
            kind: CommandKind::Record,
            params,
        };
        vec![
            command("value.set", set),
            command("value.reset", optional.to_vec()),
        ]
    }
    fn samples(&self) -> Vec<Sample> {
        let call = |command: &str, args: Vec<(String, String)>, text: &str| Sample {
            command: command.into(),
            record: WEEK.into(),
            args,
            makes_record: true,
            verify: Verify::Value {
                form: FORM.into(),
                record: Some(WEEK.into()),
                path: "value".into(),
                text: text.into(),
            },
        };
        vec![
            call("value.set", vec![("value".into(), "3".into())], "3"),
            call("value.reset", vec![], "1"),
        ]
    }
    fn strict_scope(&self) -> bool {
        true
    }
}

/// Level 1, the whole suite, through the service of the program over the database store: 0 failed; the rules of level 2 are skipped, never passed.
#[test]
fn the_suite_level_1_passes_over_the_guarded_program() {
    // The store is opened BEFORE the runtime (it has a runtime of its own).
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(support::open(&support::db_file(dir.path())));
    let core = make_core(&store);
    run(async {
        let mut s = with_form(&store, core);
        let fx = Fixture {
            client: connect(&mut s, PERSON_ID),
        };
        let report = run_conformance(&fx).await;
        assert!(report.failures.is_empty(), "failed: {:?}", report.failures);
        assert!(report.checked > 20, "the suite ran: {}", report.checked);
    });
}
