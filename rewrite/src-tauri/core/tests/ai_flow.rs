use itm_core::{
    ai::{
        job::{self, Request, Settings},
        HttpResponse,
    },
    store::Store,
    *,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
const RAW:&str="<movie><title>中文电影</title><tag>External</tag><technicalspecs source=\"IMDb\" imdbid=\"tt1234567\"><section name=\"Camera\"><item>Camera Model</item></section></technicalspecs></movie>";
fn setup() -> (tempfile::TempDir, Store, MediaItem, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    let path = media.join("电影.nfo");
    fs::write(&path, RAW).unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    store
        .configure(
            "config",
            Configuration {
                roots: vec![LibraryRoot {
                    id: "r".into(),
                    space: Space::Movie,
                    path: media.to_string_lossy().into(),
                }],
                ..Default::default()
            },
        )
        .unwrap();
    store
        .submit(ScanRequest {
            operation_id: "scan".into(),
            space: Space::Movie,
            root_ids: vec!["r".into()],
        })
        .unwrap();
    store.run_next(|| false, |_| {}).unwrap();
    let mut settings = Settings::default();
    settings.config.base_url = "https://provider.example/v1".into();
    settings.config.model = "model".into();
    settings.config.temperature = 0.7;
    settings.input_price_per_million = 2.0;
    settings.output_price_per_million = 4.0;
    store.save_ai_settings("settings", settings).unwrap();
    let item = store.all_items().unwrap().remove(0);
    (temp, store, item, root)
}
fn request(item: &MediaItem, id: &str) -> Request {
    Request {
        operation_id: id.into(),
        item_id: item.id.clone(),
        expected_hash: item.source_hash.clone(),
        force: false,
        retry_failed: false,
    }
}
fn response(reason: &str, content: &str, input: u64, output: u64) -> HttpResponse {
    HttpResponse{status:200,body:serde_json::to_vec(&json!({"choices":[{"finish_reason":reason,"message":{"content":content}}],"usage":{"prompt_tokens":input,"completion_tokens":output,"total_tokens":input+output}})).unwrap()}
}
fn good() -> String {
    serde_json::to_string(&json!({"tags":[{"value":"Camera Model","field":"Camera","source_indexes":[0],"confidence":"high"}],"warnings":[]})).unwrap()
}
fn send(store: &Store, id: &str, response: HttpResponse) -> job::Record {
    let current = store.ai_record(id).unwrap();
    let (body, _) = job::next(&current).unwrap().unwrap();
    store.reserve_ai_attempt(id, body).unwrap();
    store.observe_ai_attempt(id, Ok(response), 0.0).unwrap()
}
#[test]
fn truncation_usage_recovery_cache_and_reviewed_write_keep_old_contract() {
    let (_temp, store, item, root) = setup();
    let (first, execute) = store.begin_ai(request(&item, "ai")).unwrap();
    assert!(execute);
    assert!(first.existing.is_empty());
    let truncated = send(&store, "ai", response("length", "{", 10, 20));
    assert_eq!(truncated.meter.current.total, 30);
    assert_eq!(truncated.meter.attempts, 1);
    assert_eq!(
        truncated.attempts[0].error.as_ref().unwrap().code,
        "output-truncated"
    );
    let (body, _) = job::next(&truncated).unwrap().unwrap();
    assert_eq!(body["max_tokens"], 10000);
    assert_eq!(body["temperature"], 0);
    let input: Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert!(input["recovery_instruction"]
        .as_str()
        .unwrap()
        .contains("截断"));
    let done = send(&store, "ai", response("stop", &good(), 30, 40));
    assert_eq!(done.phase, "review-ready");
    assert_eq!(done.meter.current.total, 100);
    assert_eq!(done.meter.raw_usage.len(), 2);
    assert!((done.cost - 0.00032).abs() < 1e-12);
    assert_eq!(fs::read(&item.path).unwrap(), RAW.as_bytes());
    let (cached, execute) = store.begin_ai(request(&item, "cached")).unwrap();
    assert!(!execute);
    assert!(cached.cached);
    assert_eq!(cached.meter.attempts, 0);
    assert_eq!(cached.cost, 0.0);
    assert_eq!(cached.meter.historical_cache.total, 100);
    assert_eq!(cached.historical_cost, done.cost);
    let preview = store.preview_ai("write", "ai").unwrap();
    assert_eq!(fs::read(&item.path).unwrap(), RAW.as_bytes());
    store
        .apply_specs("write", &preview.after_hash, &root.join("journal"), || {
            false
        })
        .unwrap();
    let result = store.item(&item.id).unwrap();
    assert_eq!(result.tags[0].ownership, Ownership::External);
    assert_eq!(result.tags[1].ownership, Ownership::Generated);
    assert_eq!(result.tags[1].engine, "ai");
    let undo = store
        .preview_undo("undo", "write", &root.join("journal"))
        .unwrap();
    store
        .apply_specs("undo", &undo.after_hash, &root.join("journal"), || false)
        .unwrap();
    assert_eq!(fs::read(&item.path).unwrap(), RAW.as_bytes());
}
#[test]
fn failures_skip_unchanged_and_explicit_retry_retains_each_attempt() {
    let (_temp, store, item, _) = setup();
    store.begin_ai(request(&item, "auth")).unwrap();
    let failed = send(
        &store,
        "auth",
        HttpResponse {
            status: 401,
            body: b"unauthorized".to_vec(),
        },
    );
    assert_eq!(failed.phase, "failed");
    assert_eq!(failed.error.unwrap().code, "auth");
    assert_eq!(failed.meter.attempts, 1);
    let (skipped, execute) = store.begin_ai(request(&item, "skip")).unwrap();
    assert!(!execute);
    assert_eq!(skipped.phase, "skipped-unchanged-failure");
    let mut retry = request(&item, "retry");
    retry.retry_failed = true;
    assert!(store.begin_ai(retry.clone()).unwrap().1);
    assert!(!store.begin_ai(retry).unwrap().1);
    let malformed = send(&store, "retry", response("stop", "not-json", 10, 5));
    assert_eq!(
        malformed.attempts[0].error.as_ref().unwrap().code,
        "malformed-json"
    );
    assert!(job::next(&malformed).unwrap().is_some());
    let done = send(&store, "retry", response("stop", &good(), 10, 5));
    assert_eq!(done.meter.attempts, 2);
    assert_eq!(done.meter.current.total, 30);
}
#[test]
fn interruption_never_replays_http_and_cancelled_response_usage_is_retained() {
    let (_temp, store, item, root) = setup();
    store.begin_ai(request(&item, "interrupted")).unwrap();
    let body = job::next(&store.ai_record("interrupted").unwrap())
        .unwrap()
        .unwrap()
        .0;
    store.reserve_ai_attempt("interrupted", body).unwrap();
    drop(store);
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let (interrupted, execute) = store.begin_ai(request(&item, "interrupted")).unwrap();
    assert!(!execute);
    assert_eq!(interrupted.phase, "interrupted");
    assert_eq!(interrupted.meter.attempts, 1);
    assert!(job::next(&interrupted).unwrap().is_none());
    store.begin_ai(request(&item, "cancel")).unwrap();
    let body = job::next(&store.ai_record("cancel").unwrap())
        .unwrap()
        .unwrap()
        .0;
    store.reserve_ai_attempt("cancel", body).unwrap();
    store
        .end_ai("cancel", AppError::new("cancelled", "User cancelled"))
        .unwrap();
    let cancelled = store
        .observe_ai_attempt("cancel", Ok(response("stop", &good(), 10, 5)), 0.0)
        .unwrap();
    assert_eq!(cancelled.phase, "cancelled");
    assert_eq!(cancelled.meter.current.total, 15);
    assert!(cancelled.result.is_none());
    assert!(store.preview_ai("unsafe", "cancel").is_err());
}
#[test]
fn truncation_never_repeats_an_equal_or_smaller_actual_output_limit() {
    let (_temp, store, item, _) = setup();
    let mut settings = store.ai_settings().unwrap();
    settings
        .config
        .extra_body
        .insert("max_tokens".into(), json!(20000));
    store.save_ai_settings("override", settings).unwrap();
    store.begin_ai(request(&item, "override-ai")).unwrap();
    let failed = send(&store, "override-ai", response("length", "{", 10, 20));
    assert_eq!(failed.phase, "failed");
    assert!(job::next(&failed).unwrap().is_none());
    assert_eq!(failed.meter.attempts, 1);
}
#[test]
fn configured_rule_fallback_preserves_ai_failure_cost_and_never_masks_account_failure() {
    let (_temp, store, item, root) = setup();
    let mut settings = store.ai_settings().unwrap();
    settings.fallback_mode = "local-rules".into();
    settings.retry_count = 0;
    store
        .save_ai_settings("fallback-settings", settings)
        .unwrap();
    store.begin_ai(request(&item, "fallback")).unwrap();
    send(&store, "fallback", response("stop", "bad-json", 10, 5));
    let fallback = send(&store, "fallback", response("stop", "bad-json", 10, 5));
    assert_eq!(fallback.phase, "review-ready");
    assert_eq!(fallback.engine, "local-rules");
    assert_eq!(fallback.error.as_ref().unwrap().code, "malformed-json");
    assert_eq!(fallback.meter.current.total, 30);
    assert!(!job::needs_review(&fallback));
    let p = store.preview_ai("fallback-write", "fallback").unwrap();
    store
        .apply_specs(
            "fallback-write",
            &p.after_hash,
            &root.join("journal"),
            || false,
        )
        .unwrap();
    assert_eq!(store.item(&item.id).unwrap().tags[1].engine, "local-rules");
    let updated = store.item(&item.id).unwrap();
    store.begin_ai(request(&updated, "quota")).unwrap();
    let failed = send(
        &store,
        "quota",
        HttpResponse {
            status: 402,
            body: b"insufficient_quota".to_vec(),
        },
    );
    assert_eq!(failed.phase, "failed");
    assert_eq!(failed.engine, "ai");
    assert!(failed.result.is_none());
    let mut forced = request(&updated, "paused");
    forced.force = true;
    let (paused, execute) = store.begin_ai(forced).unwrap();
    assert!(!execute);
    assert_eq!(paused.phase, "paused");
    assert_eq!(paused.meter.attempts, 0);
}
#[test]
fn cancellation_and_budget_limits_do_not_poison_future_requests() {
    let (_temp, store, item, _) = setup();
    let mut settings = store.ai_settings().unwrap();
    settings.run_request_limit = 1;
    settings.fallback_mode = "local-rules".into();
    store.save_ai_settings("budget-settings", settings).unwrap();
    store.begin_ai(request(&item, "budget")).unwrap();
    let stopped = send(&store, "budget", response("length", "{", 10, 20));
    assert_eq!(stopped.phase, "failed");
    assert_eq!(stopped.error.as_ref().unwrap().code, "budget-exhausted");
    assert!(stopped.result.is_none());
    assert!(store.begin_ai(request(&item, "next-budget")).unwrap().1);
    let body = job::next(&store.ai_record("next-budget").unwrap())
        .unwrap()
        .unwrap()
        .0;
    store.reserve_ai_attempt("next-budget", body).unwrap();
    let cancelled = store
        .observe_ai_attempt(
            "next-budget",
            Err(AppError::new("cancelled", "shutdown")),
            0.0,
        )
        .unwrap();
    assert_eq!(cancelled.phase, "cancelled");
    assert!(cancelled.result.is_none());
    assert!(store.begin_ai(request(&item, "after-cancel")).unwrap().1);
}
#[test]
fn credential_and_configuration_commit_failure_rolls_back_only_the_new_key() {
    use itm_core::services::CredentialStore;
    use std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        },
    };
    struct Vault<'a> {
        store: &'a Store,
        freeze: AtomicBool,
        keys: Mutex<BTreeMap<String, String>>,
    }
    impl CredentialStore for Vault<'_> {
        fn get(&self, account: &str) -> Result<Option<String>> {
            Ok(self.keys.lock().unwrap().get(account).cloned())
        }
        fn put(&self, account: &str, secret: &str) -> Result<()> {
            self.keys
                .lock()
                .unwrap()
                .insert(account.into(), secret.into());
            if self.freeze.load(Ordering::SeqCst) {
                self.store.freeze_for_update()?;
            }
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<()> {
            self.keys.lock().unwrap().remove(account);
            Ok(())
        }
    }
    let (_temp, store, _item, _root) = setup();
    let settings = store.ai_settings().unwrap();
    let vault = Vault {
        store: &store,
        freeze: AtomicBool::new(true),
        keys: Mutex::new(BTreeMap::from([("unrelated".into(), "retained".into())])),
    };
    let error = store
        .save_ai_profile(
            "profile",
            settings.clone(),
            Some("non-secret-fixture"),
            &vault,
        )
        .unwrap_err();
    assert_eq!(error.code, "update-in-progress");
    assert_eq!(vault.keys.lock().unwrap().len(), 1);
    assert!(store.ai_settings().unwrap().credential_account.is_empty());
    store.unfreeze_after_update();
    vault.freeze.store(false, Ordering::SeqCst);
    let saved = store
        .save_ai_profile(
            "profile",
            settings.clone(),
            Some("non-secret-fixture"),
            &vault,
        )
        .unwrap();
    assert_eq!(
        store
            .save_ai_profile(
                "profile",
                settings.clone(),
                Some("non-secret-fixture"),
                &vault
            )
            .unwrap()
            .credential_account,
        saved.credential_account
    );
    assert_eq!(vault.keys.lock().unwrap().len(), 2);
    assert_eq!(
        store
            .save_ai_profile("profile", settings, Some("different-test-value"), &vault)
            .unwrap_err()
            .code,
        "operation-conflict"
    );
    assert_eq!(
        vault.get(&saved.credential_account).unwrap().as_deref(),
        Some("non-secret-fixture")
    );
}
