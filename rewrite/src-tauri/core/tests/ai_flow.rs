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
    let mut settings = Settings {
        enabled: true,
        ..Default::default()
    };
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

#[test]
fn price_budget_review_timeout_and_non_tag_specs_changes_reuse_current_cache() {
    let (_temp, store, item, _) = setup();
    store.begin_ai(request(&item, "generate")).unwrap();
    let result = send(&store, "generate", response("stop", &good(), 100, 20));
    let mut settings = store.ai_settings().unwrap();
    settings.input_price_per_million = 999.0;
    settings.output_price_per_million = 555.0;
    settings.run_request_limit = 1;
    settings.run_token_limit = 1;
    settings.run_cost_limit = 0.01;
    settings.warning_policy = "accept".into();
    settings.timeout_seconds = 120;
    settings.retry_count = 5;
    settings.output_token_cap = 12000;
    store.save_ai_settings("policy", settings).unwrap();
    fs::write(
        &item.path,
        RAW.replace(
            "</technicalspecs>",
            "<section name=\"Runtime\"><item>900 min</item></section></technicalspecs>",
        ),
    )
    .unwrap();
    let changed = store.inspect_item(&item.id).unwrap();
    let (cached, execute) = store.begin_ai(request(&changed, "cached")).unwrap();
    assert!(!execute);
    assert!(cached.cached);
    assert_eq!(cached.meter.attempts, 0);
    assert_eq!(cached.cost, 0.0);
    assert_eq!(cached.historical_cost, result.cost);
    assert_eq!(cached.meter.historical_cache.total, 120);
    assert_eq!(cached.fingerprint, result.fingerprint);
}

#[test]
fn current_failure_survives_accounting_changes_but_new_credentials_and_input_allow_retry() {
    let (_temp, store, item, _) = setup();
    let mut settings = store.ai_settings().unwrap();
    settings.retry_count = 0;
    store.save_ai_settings("no-retry", settings).unwrap();
    store.begin_ai(request(&item, "fail")).unwrap();
    assert_eq!(
        send(
            &store,
            "fail",
            HttpResponse {
                status: 418,
                body: b"fixture".to_vec()
            }
        )
        .phase,
        "failed"
    );
    let mut settings = store.ai_settings().unwrap();
    settings.input_price_per_million = 999.0;
    settings.retry_count = 4;
    settings.timeout_seconds = 100;
    settings.run_request_limit = 10;
    store
        .save_ai_settings("accounting", settings.clone())
        .unwrap();
    let (skipped, execute) = store.begin_ai(request(&item, "skip")).unwrap();
    assert!(!execute);
    assert_eq!(skipped.phase, "skipped-unchanged-failure");
    let peer = std::path::Path::new(&item.path).with_file_name("same-facts.nfo");
    fs::write(&peer, RAW).unwrap();
    store
        .submit(ScanRequest {
            operation_id: "scan-peer".into(),
            space: Space::Movie,
            root_ids: vec![item.root_id.clone()],
        })
        .unwrap();
    store.run_next(|| false, |_| {}).unwrap();
    let other = store
        .all_items()
        .unwrap()
        .into_iter()
        .find(|v| v.path == peer.to_string_lossy())
        .unwrap();
    assert!(store.begin_ai(request(&other, "peer")).unwrap().1);
    store
        .end_ai(
            "peer",
            AppError::new("cancelled", "Test ends before transport"),
        )
        .unwrap();
    settings.credential_account = "new-credential-reference".into();
    store.save_ai_settings("new-credential", settings).unwrap();
    assert!(store.begin_ai(request(&item, "new-account")).unwrap().1);
    store
        .end_ai(
            "new-account",
            AppError::new("cancelled", "Test ends before transport"),
        )
        .unwrap();
    let mut settings = store.ai_settings().unwrap();
    settings.config.model.push_str("-changed");
    store.save_ai_settings("new-model", settings).unwrap();
    assert!(store.begin_ai(request(&item, "new-input")).unwrap().1);
}

// Recreate the immediately preceding rewrite database using actual request
// records. The archived source bytes below must survive the v8 migration.
fn simulate_v7(root: &std::path::Path) {
    let mut db = rusqlite::Connection::open(root.join("state.sqlite")).unwrap();
    let tx = db.transaction().unwrap();
    let rows: Vec<(String, String)> = tx
        .prepare("SELECT id,result FROM operations WHERE json_extract(result,'$.kind')='ai'")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    let mut failures = std::collections::BTreeMap::new();
    for (id, body) in rows {
        let OperationResult::Ai(mut record) = serde_json::from_str(&body).unwrap() else {
            unreachable!()
        };
        let new_failure = ai::identity::record_keys(&record).unwrap().failure;
        let old = hash(
            &serde_json::to_vec(&(
                "ai-runtime-1",
                ai::failure_fingerprint(&record.settings.config, &record.specs, &record.existing)
                    .unwrap(),
                &record.settings,
            ))
            .unwrap(),
        );
        failures.insert(new_failure, old.clone());
        record.fingerprint = old;
        tx.execute(
            "UPDATE operations SET result=?2 WHERE id=?1",
            rusqlite::params![
                id,
                serde_json::to_string(&OperationResult::Ai(record)).unwrap()
            ],
        )
        .unwrap();
    }
    let caches: Vec<String> = tx
        .prepare("SELECT body FROM ai_cache")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    tx.execute("DELETE FROM ai_cache", []).unwrap();
    for body in caches {
        let mut record: job::Record = serde_json::from_str(&body).unwrap();
        record.fingerprint = hash(
            &serde_json::to_vec(&(
                "ai-runtime-1",
                ai::failure_fingerprint(&record.settings.config, &record.specs, &record.existing)
                    .unwrap(),
                &record.settings,
            ))
            .unwrap(),
        );
        tx.execute(
            "INSERT INTO ai_cache VALUES(?1,?2)",
            rusqlite::params![record.fingerprint, serde_json::to_string(&record).unwrap()],
        )
        .unwrap();
    }
    for (new, old) in failures {
        tx.execute(
            "UPDATE ai_failures SET fingerprint=?2 WHERE fingerprint=?1",
            rusqlite::params![new, old],
        )
        .unwrap();
    }
    tx.execute_batch(
        "DROP TABLE ai_cache_v1_archive; DROP TABLE ai_failures_v1_archive; PRAGMA user_version=7;",
    )
    .unwrap();
    tx.commit().unwrap();
}

#[test]
fn v7_rekey_preserves_original_cache_and_history_and_keeps_failure_protection() {
    let (_temp, store, item, root) = setup();
    store.begin_ai(request(&item, "good")).unwrap();
    let good = send(&store, "good", response("stop", &good(), 10, 5));
    let mut settings = store.ai_settings().unwrap();
    settings.config.model = "failed-model".into();
    settings.retry_count = 0;
    store
        .save_ai_settings("failure-settings", settings)
        .unwrap();
    store.begin_ai(request(&item, "failure")).unwrap();
    send(
        &store,
        "failure",
        HttpResponse {
            status: 418,
            body: b"fixture".to_vec(),
        },
    );
    drop(store);
    simulate_v7(&root);
    let db = rusqlite::Connection::open(root.join("state.sqlite")).unwrap();
    let history: String = db
        .query_row("SELECT result FROM operations WHERE id='good'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let cache: String = db
        .query_row("SELECT body FROM ai_cache", [], |r| r.get(0))
        .unwrap();
    let mut older: job::Record = serde_json::from_str(&cache).unwrap();
    older.settings.input_price_per_million = 1.0;
    older.cost = 9.9;
    older.finished_at = Some("2000-01-01T00:00:00.000Z".into());
    older.fingerprint = hash(
        &serde_json::to_vec(&(
            "ai-runtime-1",
            ai::failure_fingerprint(&older.settings.config, &older.specs, &older.existing).unwrap(),
            &older.settings,
        ))
        .unwrap(),
    );
    db.execute(
        "INSERT INTO ai_cache VALUES(?1,?2)",
        rusqlite::params![older.fingerprint, serde_json::to_string(&older).unwrap()],
    )
    .unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        8
    );
    assert_eq!(
        db.query_row("SELECT result FROM operations WHERE id='good'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        history
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM ai_cache_v1_archive WHERE body=?1",
            [&cache],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM ai_cache", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    let mut settings = store.ai_settings().unwrap();
    settings.input_price_per_million = 999.0;
    store
        .save_ai_settings("price-only", settings.clone())
        .unwrap();
    assert_eq!(
        store
            .begin_ai(request(&item, "still-failed"))
            .unwrap()
            .0
            .phase,
        "skipped-unchanged-failure"
    );
    settings.config.model = "model".into();
    store.save_ai_settings("original-model", settings).unwrap();
    let (hit, execute) = store.begin_ai(request(&item, "hit")).unwrap();
    assert!(!execute);
    assert!(hit.cached);
    assert_eq!(hit.historical_cost, good.cost);
    drop(store);
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    assert!(
        store
            .begin_ai(request(&item, "second-open"))
            .unwrap()
            .0
            .cached
    );
    assert_eq!(fs::read(&item.path).unwrap(), RAW.as_bytes());
}

#[test]
fn failed_v7_rekey_is_atomic_and_can_retry_after_destination_failure_is_removed() {
    let (_temp, store, item, root) = setup();
    store.begin_ai(request(&item, "good")).unwrap();
    send(&store, "good", response("stop", &good(), 10, 5));
    drop(store);
    simulate_v7(&root);
    let db = rusqlite::Connection::open(root.join("state.sqlite")).unwrap();
    let original: String = db
        .query_row("SELECT body FROM ai_cache", [], |r| r.get(0))
        .unwrap();
    db.execute_batch("CREATE TRIGGER reject_rekey BEFORE INSERT ON ai_cache BEGIN SELECT RAISE(ABORT,'injected destination failure'); END;").unwrap();
    assert_eq!(
        Store::open(&root.join("state.sqlite")).err().unwrap().code,
        "ai-identity-migration"
    );
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        7
    );
    assert_eq!(
        db.query_row("SELECT body FROM ai_cache", [], |r| r.get::<_, String>(0))
            .unwrap(),
        original
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='ai_cache_v1_archive'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER reject_rekey;").unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    assert!(store.begin_ai(request(&item, "hit")).unwrap().0.cached);
}

#[test]
fn unmappable_v7_failure_blocks_migration_without_erasing_the_original_gate() {
    let (_temp, store, _item, root) = setup();
    drop(store);
    simulate_v7(&root);
    let db = rusqlite::Connection::open(root.join("state.sqlite")).unwrap();
    let error = serde_json::to_string(&AppError::new("auth", "Historical error fixture")).unwrap();
    db.execute("INSERT INTO ai_failures VALUES('unmapped',?1)", [&error])
        .unwrap();
    assert_eq!(
        Store::open(&root.join("state.sqlite")).err().unwrap().code,
        "ai-identity-migration"
    );
    assert_eq!(
        db.query_row(
            "SELECT error FROM ai_failures WHERE fingerprint='unmapped'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        error
    );
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        7
    );
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
    assert_eq!(skipped.phase, "paused");
    assert_eq!(skipped.error.unwrap().code, "auth");
    assert_eq!(skipped.meter.attempts, 0);
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

#[test]
fn disabled_ai_blocks_new_requests_and_batches_but_keeps_prior_history_queryable() {
    let (_temp, store, item, _) = setup();
    store.begin_ai(request(&item, "previous")).unwrap();
    store
        .end_ai("previous", AppError::new("cancelled", "test finished"))
        .unwrap();
    let mut settings = store.ai_settings().unwrap();
    settings.enabled = false;
    store.save_ai_settings("disable", settings).unwrap();
    assert_eq!(
        store.begin_ai(request(&item, "disabled")).unwrap_err().code,
        "ai-disabled"
    );
    assert_eq!(store.ai_record("previous").unwrap().phase, "cancelled");
    let scope = batch::BatchRequest {
        operation_id: "disabled-batch".into(),
        space: Space::Movie,
        item_ids: vec![item.id],
        root_ids: vec![],
        retry_failed: false,
        engine: batch::BatchEngine::Ai,
        mode: batch::BatchMode::Generate,
    };
    assert_eq!(store.plan_batch(scope).unwrap_err().code, "ai-disabled");
    assert!(store.ai_record("disabled").is_err());
}

fn import_runtime(store: &Store, root: &std::path::Path, id: &str, state: Value) {
    let legacy = root.join(id);
    fs::create_dir(&legacy).unwrap();
    fs::write(
        legacy.join("ai-runtime.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let plan = store.prepare_migration(id, &legacy, "itm-engine").unwrap();
    assert!(plan.adapters.iter().any(|a| a.target == "ai-runtime"));
    store.apply_migration(id, &plan.fingerprint).unwrap();
}
fn paused(kind: &str) -> Value {
    json!({"paused":true,"reason_kind":kind,"reason":"旧错误原因","paused_at":"2026-09-01T12:00:00+00:00","last_success_at":"2026-08-31T12:00:00+00:00","last_error_at":"2026-09-01T12:00:00+00:00","updated_at":"2026-09-01T12:00:00+00:00"})
}
fn connection_good() -> String {
    serde_json::to_string(&json!({"tags":[{"value":"Dolby Atmos","field":"Sound mix","source_indexes":[1],"confidence":"high"}],"warnings":[]})).unwrap()
}
#[test]
fn imported_pause_precedes_cache_survives_settings_and_resumes_only_explicitly() {
    let (_temp, store, item, root) = setup();
    let before = fs::metadata(&item.path).unwrap().modified().unwrap();
    store.begin_ai(request(&item, "cached-source")).unwrap();
    send(&store, "cached-source", response("stop", &good(), 10, 5));
    import_runtime(&store, &root, "import-pause", paused("auth"));
    assert_eq!(
        serde_json::to_value(store.ai_runtime().unwrap()).unwrap(),
        paused("auth")
    );
    let mut settings = store.ai_settings().unwrap();
    settings.config.model = "changed".into();
    store
        .save_ai_settings("save-while-paused", settings)
        .unwrap();
    assert!(store.ai_runtime().unwrap().paused);
    let (blocked, execute) = store.begin_ai(request(&item, "blocked")).unwrap();
    assert!(!execute);
    assert_eq!(blocked.phase, "paused");
    assert_eq!(blocked.meter.attempts, 0);
    let mut settings = store.ai_settings().unwrap();
    settings.config.model = "model".into();
    store
        .save_ai_settings("restore-settings", settings)
        .unwrap();
    assert_eq!(
        store
            .begin_ai(request(&item, "blocked-cache"))
            .unwrap()
            .0
            .phase,
        "paused"
    );
    store.resume_ai_runtime("resume").unwrap();
    let cached = store.begin_ai(request(&item, "after-resume")).unwrap().0;
    assert!(cached.cached);
    assert_eq!(cached.meter.attempts, 0);
    import_runtime(&store, &root, "pause-again", paused("quota"));
    // Re-delivery of an old resume receipt cannot resume a newer pause.
    assert!(!store.resume_ai_runtime("resume").unwrap().paused);
    assert!(store.ai_runtime().unwrap().paused);
    assert_eq!(
        store.resume_ai_runtime("scan").unwrap_err().code,
        "operation-conflict"
    );
    drop(store);
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    assert_eq!(store.ai_runtime().unwrap().reason_kind, "quota");
    assert_eq!(fs::read(&item.path).unwrap(), RAW.as_bytes());
    assert_eq!(
        fs::metadata(&item.path).unwrap().modified().unwrap(),
        before
    );
}
#[test]
fn malformed_runtime_and_changed_import_destination_fail_closed() {
    let (_temp, store, item, root) = setup();
    import_runtime(
        &store,
        &root,
        "malformed",
        json!({"paused":"not-a-boolean"}),
    );
    assert_eq!(
        store.ai_runtime().unwrap().reason_kind,
        "legacy-runtime-unverified"
    );
    assert!(!store.begin_ai(request(&item, "guarded")).unwrap().1);
    let legacy = root.join("next-import");
    fs::create_dir(&legacy).unwrap();
    fs::write(
        legacy.join("ai-runtime.json"),
        serde_json::to_vec(&paused("auth")).unwrap(),
    )
    .unwrap();
    let plan = store
        .prepare_migration("next-import", &legacy, "itm-engine")
        .unwrap();
    store.resume_ai_runtime("manual-resume").unwrap();
    assert_eq!(
        store
            .apply_migration("next-import", &plan.fingerprint)
            .unwrap_err()
            .code,
        "migration-configuration-conflict"
    );
    assert!(store
        .legacy_artifact("next-import", "ai-runtime.json")
        .is_err());
    assert!(!store.ai_runtime().unwrap().paused);
}
#[test]
fn connection_test_works_without_enabled_ai_or_library_and_never_becomes_a_write() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let mut settings = Settings::default();
    settings.config.model = "alias".into();
    settings.config.base_url = "https://provider.example/v1".into();
    store.save_ai_settings("profile", settings).unwrap();
    assert!(!store.ai_settings().unwrap().enabled);
    import_runtime(&store, &root, "old-runtime", paused("auth"));
    let (test, execute) = store.begin_ai_test("test").unwrap();
    assert!(execute);
    assert_eq!(test.purpose, job::Purpose::ConnectionTest);
    assert!(test.path.is_empty());
    assert_eq!(test.specs.len(), 10);
    assert_eq!(test.specs["Sound mix"], vec!["DTS (DTS: X)", "Dolby Atmos"]);
    let mut reply: Value =
        serde_json::from_slice(&response("stop", &connection_good(), 10, 5).body).unwrap();
    reply["model"] = "resolved-model-202609".into();
    let done = send(
        &store,
        "test",
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&reply).unwrap(),
        },
    );
    assert_eq!(done.phase, "review-ready");
    assert_eq!(done.meter.current.total, 15);
    assert_eq!(done.resolved_model, "resolved-model-202609");
    assert_eq!(
        done.attempts[0].model.as_deref(),
        Some("resolved-model-202609")
    );
    assert!(!store.ai_runtime().unwrap().paused);
    assert!(!store.ai_runtime().unwrap().last_success_at.is_empty());
    assert_eq!(
        store.preview_ai("unsafe-write", "test").unwrap_err().code,
        "ai-test-not-writable"
    );
    assert!(store.all_items().unwrap().is_empty());
    assert!(!store.begin_ai_test("test").unwrap().1);
    let (fresh, execute) = store.begin_ai_test("test-again").unwrap();
    assert!(execute);
    assert!(!fresh.cached);
    store
        .end_ai("test-again", AppError::new("cancelled", "fixture"))
        .unwrap();
    assert_eq!(
        store
            .ai_history(Some(job::CONNECTION_TEST_ITEM))
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn failed_cancelled_or_interrupted_connection_tests_cannot_clear_pause_or_use_rule_fallback() {
    let (_temp, store, _item, root) = setup();
    let mut settings = store.ai_settings().unwrap();
    settings.retry_count = 0;
    settings.fallback_mode = "local-rules".into();
    store.save_ai_settings("fallback", settings).unwrap();
    import_runtime(&store, &root, "old-runtime", paused("quota"));
    store.begin_ai_test("bad-json").unwrap();
    send(&store, "bad-json", response("stop", "not-json", 10, 5));
    let failed = send(&store, "bad-json", response("stop", "not-json", 10, 5));
    assert_eq!(failed.phase, "failed");
    assert!(failed.result.is_none());
    assert_eq!(failed.engine, "ai");
    assert_eq!(failed.meter.current.total, 30);
    assert!(store.ai_runtime().unwrap().paused);
    store.begin_ai_test("cancel").unwrap();
    let (body, _) = job::next(&store.ai_record("cancel").unwrap())
        .unwrap()
        .unwrap();
    store.reserve_ai_attempt("cancel", body).unwrap();
    store
        .end_ai("cancel", AppError::new("cancelled", "fixture"))
        .unwrap();
    let cancelled = store
        .observe_ai_attempt(
            "cancel",
            Ok(response("stop", &connection_good(), 10, 5)),
            0.0,
        )
        .unwrap();
    assert_eq!(cancelled.meter.current.total, 15);
    assert!(cancelled.result.is_none());
    assert!(store.ai_runtime().unwrap().paused);
    store.begin_ai_test("interrupted").unwrap();
    let (body, _) = job::next(&store.ai_record("interrupted").unwrap())
        .unwrap()
        .unwrap();
    store.reserve_ai_attempt("interrupted", body).unwrap();
    drop(store);
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let (interrupted, execute) = store.begin_ai_test("interrupted").unwrap();
    assert!(!execute);
    assert_eq!(interrupted.phase, "interrupted");
    assert_eq!(interrupted.meter.attempts, 1);
    assert!(store.ai_runtime().unwrap().paused);
}
#[test]
fn connection_test_preserves_budget_and_unknown_pause_while_normal_success_preserves_auth() {
    let (_temp, store, item, root) = setup();
    for (i, kind) in ["budget", "legacy-runtime-unverified"].iter().enumerate() {
        import_runtime(&store, &root, &format!("import-{i}"), paused(kind));
        let id = format!("test-{i}");
        store.begin_ai_test(&id).unwrap();
        send(&store, &id, response("stop", &connection_good(), 10, 5));
        assert_eq!(store.ai_runtime().unwrap().reason_kind, *kind);
        assert!(store.ai_runtime().unwrap().paused);
    }
    import_runtime(&store, &root, "import-auth", paused("auth"));
    let mut req = request(&item, "normal-explicit-retry");
    req.retry_failed = true;
    store.begin_ai(req).unwrap();
    send(
        &store,
        "normal-explicit-retry",
        response("stop", &good(), 10, 5),
    );
    assert!(store.ai_runtime().unwrap().paused);
}
#[test]
fn bounded_rate_limit_retries_finish_before_global_pause_and_do_not_fall_back() {
    let (_temp, store, item, root) = setup();
    let mut settings = store.ai_settings().unwrap();
    settings.retry_count = 1;
    settings.fallback_mode = "local-rules".into();
    store.save_ai_settings("retry-once", settings).unwrap();
    store.begin_ai(request(&item, "limited")).unwrap();
    let limited = || HttpResponse {
        status: 429,
        body: br#"{"error":{"type":"rate_limit_error","message":"fixture"}}"#.to_vec(),
    };
    let first = send(&store, "limited", limited());
    assert_eq!(first.phase, "running");
    assert!(!store.ai_runtime().unwrap().paused);
    let exhausted = send(&store, "limited", limited());
    assert_eq!(exhausted.phase, "failed");
    assert_eq!(exhausted.error.unwrap().code, "rate-limit");
    assert!(exhausted.result.is_none());
    assert_eq!(exhausted.meter.attempts, 2);
    assert_eq!(
        store.begin_ai(request(&item, "blocked")).unwrap().0.phase,
        "paused"
    );
    drop(store);
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    assert_eq!(store.ai_runtime().unwrap().reason_kind, "rate-limit");
    store.resume_ai_runtime("resume").unwrap();
    assert!(store.begin_ai(request(&item, "fresh")).unwrap().1);
}
#[test]
fn response_model_survives_cache_and_generated_ownership_without_inventing_missing_model() {
    let (_temp, store, item, root) = setup();
    store.begin_ai(request(&item, "model-response")).unwrap();
    let mut reply: Value = serde_json::from_slice(&response("stop", &good(), 10, 5).body).unwrap();
    reply["model"] = "resolved-version".into();
    send(
        &store,
        "model-response",
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&reply).unwrap(),
        },
    );
    let cached = store.begin_ai(request(&item, "cached-model")).unwrap().0;
    assert_eq!(cached.resolved_model, "resolved-version");
    let preview = store.preview_ai("write-model", "cached-model").unwrap();
    assert!(preview.after_xml.contains("resolved-version"));
    assert!(!preview
        .after_xml
        .contains("&quot;model&quot;:&quot;model&quot;"));
    store
        .apply_specs(
            "write-model",
            &preview.after_hash,
            &root.join("journal"),
            || false,
        )
        .unwrap();
    let changed = store.item(&item.id).unwrap();
    let mut req = request(&changed, "missing-model");
    req.force = true;
    store.begin_ai(req).unwrap();
    let unknown = send(&store, "missing-model", response("stop", &good(), 10, 5));
    assert!(unknown.resolved_model.is_empty());
    assert!(unknown.attempts[0].model.is_none());
}
#[test]
fn inaccessible_credentials_do_not_hide_the_saved_ai_settings() {
    struct Denied;
    impl itm_core::services::CredentialStore for Denied {
        fn get(&self, account: &str) -> Result<Option<String>> {
            assert_eq!(account, "denied-account");
            Err(AppError::new(
                "credential-read",
                "fixture permission denied",
            ))
        }
        fn put(&self, _: &str, _: &str) -> Result<()> {
            panic!("read-only profile")
        }
        fn delete(&self, _: &str) -> Result<()> {
            panic!("read-only profile")
        }
    }
    let (_temp, store, _item, _root) = setup();
    let mut settings = store.ai_settings().unwrap();
    settings.credential_account = "denied-account".into();
    store
        .save_ai_settings("credential-reference", settings)
        .unwrap();
    let profile = store.ai_profile(&Denied).unwrap();
    assert_eq!(profile.settings.config.model, "model");
    assert!(!profile.credential_ready);
    assert_eq!(profile.credential_error.unwrap().code, "credential-read");
}

#[test]
fn stale_profile_save_preserves_current_settings_and_does_not_create_credentials() {
    use std::{collections::BTreeMap, sync::Mutex};
    struct Vault(Mutex<BTreeMap<String, String>>);
    impl services::CredentialStore for Vault {
        fn get(&self, a: &str) -> Result<Option<String>> {
            Ok(self.0.lock().unwrap().get(a).cloned())
        }
        fn put(&self, a: &str, s: &str) -> Result<()> {
            self.0.lock().unwrap().insert(a.into(), s.into());
            Ok(())
        }
        fn delete(&self, a: &str) -> Result<()> {
            self.0.lock().unwrap().remove(a);
            Ok(())
        }
    }
    let (_temp, store, _, _) = setup();
    let vault = Vault(Mutex::new(BTreeMap::new()));
    let old = store.ai_profile(&vault).unwrap();
    let mut newer = old.settings.clone();
    newer.config.model = "other-view-model".into();
    let saved = store
        .save_ai_profile_checked(
            "new-view",
            newer.clone(),
            Some("fixture"),
            &vault,
            &old.revision,
        )
        .unwrap();
    let err = store
        .save_ai_profile_checked(
            "stale-view",
            old.settings.clone(),
            Some("unused-fixture"),
            &vault,
            &old.revision,
        )
        .unwrap_err();
    assert_eq!(err.code, "ai-settings-conflict");
    assert_eq!(vault.0.lock().unwrap().len(), 1);
    assert_eq!(
        store.ai_settings().unwrap().config.model,
        "other-view-model"
    );
    // A known successful request remains queryable/replayable even after another save.
    let mut third = store.ai_settings().unwrap();
    third.config.model = "third-model".into();
    store.save_ai_settings("third", third).unwrap();
    assert_eq!(
        store
            .save_ai_profile_checked("new-view", newer, Some("fixture"), &vault, &old.revision)
            .unwrap()
            .credential_account,
        saved.credential_account
    );
    assert_eq!(store.ai_settings().unwrap().config.model, "third-model");
    assert!(!store.ai_runtime().unwrap().paused);
}

#[test]
fn profile_changed_during_keychain_write_rejects_stale_commit_and_cleans_new_key() {
    use std::{collections::BTreeMap, sync::Mutex};
    struct RacingVault<'a> {
        store: &'a Store,
        keys: Mutex<BTreeMap<String, String>>,
    }
    impl services::CredentialStore for RacingVault<'_> {
        fn get(&self, a: &str) -> Result<Option<String>> {
            Ok(self.keys.lock().unwrap().get(a).cloned())
        }
        fn put(&self, a: &str, s: &str) -> Result<()> {
            self.keys.lock().unwrap().insert(a.into(), s.into());
            let mut newer = self.store.ai_settings()?;
            newer.config.model = "changed-while-keychain-open".into();
            self.store.save_ai_settings("concurrent-save", newer)?;
            Ok(())
        }
        fn delete(&self, a: &str) -> Result<()> {
            self.keys.lock().unwrap().remove(a);
            Ok(())
        }
    }
    let (_temp, store, _, _) = setup();
    let vault = RacingVault {
        store: &store,
        keys: Mutex::new(BTreeMap::new()),
    };
    let old = store.ai_profile(&vault).unwrap();
    let err = store
        .save_ai_profile_checked(
            "stale-keychain",
            old.settings,
            Some("fixture"),
            &vault,
            &old.revision,
        )
        .unwrap_err();
    assert_eq!(err.code, "ai-settings-conflict");
    assert!(vault.keys.lock().unwrap().is_empty());
    assert_eq!(
        store.ai_settings().unwrap().config.model,
        "changed-while-keychain-open"
    );
    assert!(store.ai_settings().unwrap().credential_account.is_empty());
    assert_eq!(
        store.operation_result("stale-keychain").unwrap_err().code,
        "operation-not-found"
    );
}
