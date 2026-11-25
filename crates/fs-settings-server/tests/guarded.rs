//! fs-settings under `Guarded` (Stufe 4 C3): `FormSource` next to the hub on one capability, the suite `module_v1` level 1 over the real
//! database store, and the proof that a bot does not see a `human_only` setting and cannot set it BECAUSE of `SettingsRules`
//! (with rules "everything is allowed" both tests are red). Pattern: none (tests).
#![allow(
    clippy::unused_async_trait_impl,
    clippy::unnecessary_literal_bound,
    clippy::result_large_err
)]

mod guarded_support;
mod support;

use std::sync::Arc;

use capnp::capability::FromClientHook;
use fs_interfaces::module::{
    Applied, Ask, Blob, ChangeFeed, ChangePage, ChangesQuery, Context, FeedBreak, Follow,
    HiddenReason, Invocation, ModuleCore, Page, PageQuery, Refusal, RightsRules, StepRef, Verdict,
};
use fs_interfaces::owner::OwnerProblem;
use fs_interfaces::settings_capnp::settings_hub;
use fs_rpc::{Caller, FixedPerson, Guarded, OsUserIsPerson, PerConnection, Ports};
use fs_settings::{Place, Scope, SettingsStore};
use fs_settings_rpc::SettingsCore;
use fs_settings_server::{DbSettingsStore, SettingsWithForm};
use guarded_support::{
    AI, BOT_ID, PERSON_ID, WEEK, catalog, connect, ids, invoke, key, program, run, service,
    the_person, with_form,
};

fn store(dir: &std::path::Path) -> Arc<DbSettingsStore> {
    Arc::new(support::open(&support::db_file(dir)))
}

fn stored(store: &Arc<DbSettingsStore>, name: &str) -> bool {
    let scope = Scope::Program(program());
    // Called OUTSIDE the runtime (the database store is sync over a runtime of its own): after `run` has returned.
    store
        .profile(&scope, Place::Profile, 0)
        .values
        .contains_key(&key(name))
}

/// The rules say yes to everything (the wrong rules of the counter-proof).
struct AllowAll;

impl RightsRules for AllowAll {
    fn may(&self, _: &Ask<'_>) -> Verdict {
        Verdict::Allow
    }
    fn hidden(&self, _: &Context, _: &str, _: &str) -> Option<HiddenReason> {
        None
    }
}

/// Pattern: Decorator - the module `M` with the rules of `AllowAll`; everything else is `M`.
struct Lax<M: ModuleCore> {
    inner: M,
    rules: AllowAll,
}

impl<M: ModuleCore> ModuleCore for Lax<M> {
    fn level(&self) -> u32 {
        self.inner.level()
    }
    fn provider(&self) -> &str {
        self.inner.provider()
    }
    fn rules(&self) -> &dyn RightsRules {
        &self.rules
    }
    async fn page(&self, cx: &Context, query: PageQuery) -> Result<Page, Refusal> {
        self.inner.page(cx, query).await
    }
    async fn blob(&self, cx: &Context, record: &str, field: &str) -> Result<Blob, Refusal> {
        self.inner.blob(cx, record, field).await
    }
    async fn apply(&self, cx: &Context, command: Invocation) -> Result<Applied, Refusal> {
        self.inner.apply(cx, command).await
    }
    async fn revert(&self, cx: &Context, change: &str, step: StepRef) -> Result<Applied, Refusal> {
        self.inner.revert(cx, change, step).await
    }
    async fn changes(&self, cx: &Context, query: ChangesQuery) -> Result<ChangePage, Refusal> {
        self.inner.changes(cx, query).await
    }
    fn feed(&self, sink: ChangeFeed) {
        self.inner.feed(sink);
    }
    fn on_feed_break(&self, sink: FeedBreak) {
        self.inner.on_feed_break(sink);
    }
    fn follows(&self, cx: &Context, form: &str, scope: &str) -> Result<Follow, Refusal> {
        self.inner.follows(cx, form, scope)
    }
}

/// The wrapper with the SAME ports as the program, over a core with the rules of `rules` (the real ones: `SettingsCore`; the wrong ones: `Lax`).
fn lax(store: &Arc<DbSettingsStore>) -> Guarded<Lax<SettingsCore<DbSettingsStore>>> {
    let core = SettingsCore::new(program(), catalog(), store.clone());
    let core = Lax {
        inner: core,
        rules: AllowAll,
    };
    Guarded::new(core, Ports::new(OsUserIsPerson, FixedPerson(the_person())))
}

/// Test 1, one body for the proof and the counter-proof: does the bot see `AI` (and does the person)?
async fn bot_sees(
    bot: &fs_interfaces::formsource_capnp::form_source::Client,
    person: &fs_interfaces::formsource_capnp::form_source::Client,
) -> bool {
    assert!(
        ids(person).await.iter().any(|id| id == AI),
        "a person sees it"
    );
    ids(bot).await.iter().any(|id| id == AI)
}

#[test]
fn a_bot_does_not_see_a_human_only_setting() {
    // The store is opened BEFORE the runtime (it has a runtime of its own).
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    run(async {
        let mut s = with_form(&store);
        assert!(s.serves_form());
        let (bot, person) = (connect(&mut s, BOT_ID), connect(&mut s, PERSON_ID));
        assert!(
            !bot_sees(&bot, &person).await,
            "the bot must not see the setting"
        );
        assert_eq!(
            ids(&bot).await,
            [WEEK],
            "the other setting is there for the bot"
        );
    });
}

#[test]
fn a_bot_cannot_set_a_human_only_setting() {
    // The store is opened BEFORE the runtime (it has a runtime of its own).
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    run(async {
        let mut s = with_form(&store);
        let bot = connect(&mut s, BOT_ID);
        let refused = invoke(&bot, "value.set", AI, &[("value", "true")]).await;
        assert_eq!((refused.failed, refused.status), (true, 403), "{refused:?}");
    });
    assert!(!stored(&store, AI), "the store did not change");
    // a person sets it, the same call
    run(async {
        let mut s = with_form(&store);
        let person = connect(&mut s, PERSON_ID);
        assert!(
            !invoke(&person, "value.set", AI, &[("value", "true")])
                .await
                .failed
        );
    });
    assert!(stored(&store, AI));
}

/// The counter-proof: with rules "everything is allowed" the bot sees it AND sets it (both facts of the two tests above turn around).
#[test]
fn counter_rules_that_allow_everything_let_the_bot_in() {
    // The store is opened BEFORE the runtime (it has a runtime of its own).
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    run(async {
        let mut g = lax(&store);
        let (bot, person) = (
            FromClientHook::new(g.bootstrap(&Caller::new(BOT_ID, None)).hook),
            FromClientHook::new(g.bootstrap(&Caller::new(PERSON_ID, None)).hook),
        );
        assert!(
            bot_sees(&bot, &person).await,
            "with AllowAll the bot sees it"
        );
        let done = invoke(&bot, "value.set", AI, &[("value", "true")]).await;
        assert!(!done.failed, "{done:?}");
    });
    assert!(stored(&store, AI), "with AllowAll the bot sets it");
}

/// The hub of the contract `settings` answers on the same capability, as before.
#[test]
fn the_hub_runs_on_the_same_capability() {
    // The store is opened BEFORE the runtime (it has a runtime of its own).
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    run(async {
        let mut s = with_form(&store);
        let hub: settings_hub::Client =
            FromClientHook::new(s.bootstrap(&Caller::new(PERSON_ID, None)).hook);
        let opened = hub.manage_request().send().promise.await.unwrap();
        let store_cap = opened.get().unwrap().get_store().unwrap();
        store_cap.stamp_request().send().promise.await.unwrap();
        // and the form source on it, too
        let form = connect(&mut s, PERSON_ID);
        assert_eq!(ids(&form).await, [AI, WEEK]);
    });
}

/// Without the person "me" the form source does not start; the hub runs on, and the report names the `owner-*` code.
#[test]
fn without_the_module_person_only_the_form_source_stays_off() {
    // The store is opened BEFORE the runtime (it has a runtime of its own).
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    run(async {
        let told = std::cell::RefCell::new(Vec::<String>::new());
        let no_me: Result<FixedPerson, OwnerProblem> = Err(OwnerProblem::NoMe {
            hint: "create me".into(),
        });
        let mut s =
            SettingsWithForm::new(service(&store), store.clone(), program(), no_me, &|line| {
                told.borrow_mut().push(line.to_owned());
            });
        assert!(!s.serves_form());
        let told = told.borrow().clone();
        assert_eq!(told.len(), 1);
        assert!(told[0].contains("owner-no-me"), "{told:?}");
        let hub: settings_hub::Client =
            FromClientHook::new(s.bootstrap(&Caller::new(PERSON_ID, None)).hook);
        let opened = hub.manage_request().send().promise.await.unwrap();
        opened
            .get()
            .unwrap()
            .get_store()
            .unwrap()
            .stamp_request()
            .send()
            .promise
            .await
            .unwrap();
    });
}
