//! The world of `tests/guarded.rs`: a real database store, a catalog with a `human_only` setting, the service of the program
//! (`SettingsWithForm`) and the calls over the form source. Pattern: Test double helpers (in-process, no socket).
#![allow(dead_code, clippy::cast_possible_truncation)]

use std::future::Future;
use std::sync::Arc;

use capnp::capability::FromClientHook;
use fs_interfaces::formsource_capnp::form_source;
use fs_interfaces::owner::fake::person;
use fs_rpc::{Caller, FixedPerson, PerConnection};
use fs_settings::{Catalog, Declaration, GroupId, Kind, ProgramName, SettingKey, Value};
use fs_settings_rpc::{SettingsCore, SettingsService};
use fs_settings_server::{DbSettingsStore, SettingsWithForm, form_core};
use fs_types::PersonRef;

pub const FORM: &str = "fields-in-groups@1.0.0";
pub const WEEK: &str = "program.tasks.week-start";
pub const AI: &str = "program.tasks.ai-switch";
const PERSON: &str = "urn:uuid:018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9e01";

pub fn run<F: Future>(f: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, f)
}

pub fn key(text: &str) -> SettingKey {
    SettingKey::parse(text).unwrap()
}

pub fn the_person() -> PersonRef {
    person(PERSON)
}

/// A number and a switch only a person may touch.
pub fn catalog() -> Catalog {
    let d = |k: &str, kind, default, group: &str| {
        Declaration::new(key(k), kind, default, GroupId::new(group).unwrap())
    };
    Catalog::of(vec![
        d(
            WEEK,
            Kind::Whole { min: 1, max: 7 },
            Value::Whole(1),
            "calendar",
        ),
        d(AI, Kind::Flag, Value::Flag(false), "privacy").human_only(),
    ])
    .unwrap()
}

pub fn program() -> ProgramName {
    ProgramName::new("tasks").unwrap()
}

/// The service of the program over `store`, as `serve` builds it (the person is the owner, the actor `OsUserIsPerson`).
pub fn service(store: &Arc<DbSettingsStore>) -> SettingsService<DbSettingsStore, DbSettingsStore> {
    let mut svc = SettingsService::new_os_user_is_person(store.clone(), None);
    svc.catalog = catalog();
    svc.refused_note = None;
    svc.claimed_by = None;
    svc
}

/// The core of the form source, made OUTSIDE the runtime (it reads the store once).
pub fn make_core(store: &Arc<DbSettingsStore>) -> SettingsCore<DbSettingsStore> {
    form_core(&service(store), store.clone(), program())
}

pub fn with_form(
    store: &Arc<DbSettingsStore>,
    core: SettingsCore<DbSettingsStore>,
) -> SettingsWithForm {
    let person = Ok(FixedPerson(the_person()));
    SettingsWithForm::new(service(store), core, person, &|_| {})
}

/// A person: the transport names the id of the service's own user. A bot: an id the system could not turn into an actor.
pub const PERSON_ID: &str = "1000";
pub const BOT_ID: &str = "not an id";

pub fn connect(s: &mut impl PerConnection, id: &str) -> form_source::Client {
    FromClientHook::new(s.bootstrap(&Caller::new(id, None)).hook)
}

#[derive(Debug)]
pub struct Wrote {
    pub failed: bool,
    pub status: u16,
}

pub async fn invoke(
    c: &form_source::Client,
    cmd: &str,
    record: &str,
    list: &[(&str, &str)],
) -> Wrote {
    let mut req = c.invoke_request();
    req.get().set_command(cmd);
    req.get().set_record(record);
    let mut out = req.get().init_args(list.len() as u32);
    for (i, (p, t)) in list.iter().enumerate() {
        let mut e = out.reborrow().get(i as u32);
        e.set_path(p);
        e.set_text(t);
    }
    let r = req.send().promise.await.unwrap();
    let a = r.get().unwrap();
    Wrote {
        failed: a.get_failed(),
        status: a.get_problem().unwrap().get_status(),
    }
}

/// The ids of the rows of the form.
pub async fn ids(c: &form_source::Client) -> Vec<String> {
    let mut req = c.page_request();
    req.get().set_form(FORM);
    req.get().set_limit(100);
    let r = req.send().promise.await.unwrap();
    let page = r.get().unwrap().get_page().unwrap();
    page.get_records()
        .unwrap()
        .iter()
        .map(|rec| rec.get_id().unwrap().to_string().unwrap())
        .collect()
}
