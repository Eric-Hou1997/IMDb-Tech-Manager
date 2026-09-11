use itm_core::{
    ai::{
        self,
        job::{Request, Settings},
        legacy_failure::{self, Failure},
    },
    store::Store,
    *,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
const RAW:&str="<movie><title>失败队列测试</title><technicalspecs source=\"IMDb\" imdbid=\"tt1234567\"><section name=\"Camera\"><item>Camera Model</item></section></technicalspecs></movie>";
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    old: PathBuf,
    store: Store,
    item: MediaItem,
}
fn fixture(unfingerprinted: bool) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let old = root.join("old");
    let media = root.join("media");
    fs::create_dir(&old).unwrap();
    fs::create_dir(&media).unwrap();
    let path = media.join("电影.nfo");
    fs::write(&path, RAW).unwrap();
    let profile = json!({"library_roots":{"movies":[media]},"ai":{"enabled":true,"provider":"openai-compatible","base_url":"https://provider.example/v1","model":"fixture","prompt":"中文提示词","extra_body":"{ \"metadata\": {\"fixture\": true} }"}});
    let settings = itm_core::migration::ai_profile::adapt(&profile)
        .unwrap()
        .unwrap();
    let specs = Specs::from([("Camera".into(), vec!["Camera Model".into()])]);
    let mut entry = json!({"path":path,"title":"失败队列测试","kind":"schema-invalid","message":"Original fixture error","retryable":true,"at":"2024-01-02T03:04:05+00:00","task_id":"old-task","fingerprint":legacy_failure::fingerprint(path.to_str().unwrap(),&specs,&settings,"schema-invalid",profile["ai"]["extra_body"].as_str().unwrap()).unwrap()});
    if unfingerprinted {
        entry.as_object_mut().unwrap().remove("fingerprint");
    }
    fs::write(
        old.join("config.json"),
        serde_json::to_vec(&profile).unwrap(),
    )
    .unwrap();
    fs::write(
        old.join("ai-failure-queue.json"),
        serde_json::to_vec(&json!({"schema":2,"items":[entry]})).unwrap(),
    )
    .unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let plan = store
        .prepare_migration("import", &old, "itm-engine")
        .unwrap();
    assert!(plan
        .adapters
        .iter()
        .any(|a| a.target.starts_with("legacy-ai-failure:")));
    store.apply_migration("import", &plan.fingerprint).unwrap();
    store
        .submit(ScanRequest {
            operation_id: "scan".into(),
            space: Space::Movie,
            root_ids: vec![store.configuration().unwrap().roots[0].id.clone()],
        })
        .unwrap();
    store.run_next(|| false, |_| {}).unwrap();
    let item = store.all_items().unwrap().remove(0);
    Fixture {
        _temp: temp,
        root,
        old,
        store,
        item,
    }
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
fn succeed(store: &Store, id: &str) {
    let value = store.ai_record(id).unwrap();
    let (body, _) = ai::job::next(&value).unwrap().unwrap();
    store.reserve_ai_attempt(id, body).unwrap();
    let content=json!({"tags":[{"value":"Camera Model","field":"Camera","source_indexes":[0]}],"warnings":[]}).to_string();
    let response=ai::HttpResponse{status:200,body:serde_json::to_vec(&json!({"choices":[{"finish_reason":"stop","message":{"content":content}}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}})).unwrap()};
    assert_eq!(
        store
            .observe_ai_attempt(id, Ok(response), 0.0)
            .unwrap()
            .phase,
        "review-ready"
    );
}
#[test]
fn unchanged_failure_import_skips_without_http_or_fallback_and_survives_restart() {
    let f = fixture(false);
    let mtime = fs::metadata(&f.item.path).unwrap().modified().unwrap();
    let mut settings = f.store.ai_settings().unwrap();
    settings.fallback_mode = "local-rules".into();
    settings.input_price_per_million = 99.0;
    f.store.save_ai_settings("policy", settings).unwrap();
    let (record, execute) = f.store.begin_ai(request(&f.item, "skip")).unwrap();
    assert!(!execute);
    assert_eq!(record.phase, "skipped-legacy-failure");
    assert!(record.result.is_none());
    assert_eq!(record.meter.attempts, 0);
    assert_eq!(record.cost, 0.0);
    assert_eq!(record.error.as_ref().unwrap().code, "schema-invalid");
    assert_eq!(
        record.legacy_failure.as_ref().unwrap().entry["task_id"],
        "old-task"
    );
    assert!(ai::job::next(&record).unwrap().is_none());
    let mut force = request(&f.item, "force-alone");
    force.force = true;
    assert!(!f.store.begin_ai(force).unwrap().1);
    fs::rename(&f.old, f.root.join("offline")).unwrap();
    drop(f.store);
    let store = Store::open(&f.root.join("state.sqlite")).unwrap();
    assert_eq!(
        serde_json::to_value(store.begin_ai(request(&f.item, "skip")).unwrap().0).unwrap(),
        serde_json::to_value(record).unwrap()
    );
    assert_eq!(
        store
            .begin_ai(request(&f.item, "after-restart"))
            .unwrap()
            .0
            .phase,
        "skipped-legacy-failure"
    );
    assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
    assert_eq!(
        fs::metadata(&f.item.path).unwrap().modified().unwrap(),
        mtime
    );
}
#[test]
fn changed_prompt_endpoint_parameters_and_full_specs_allow_new_requests() {
    let f = fixture(false);
    let baseline = f.store.ai_settings().unwrap();
    let changes: Vec<fn(&mut Settings)> = vec![
        |s| s.config.prompt.push('!'),
        |s| s.config.base_url.push_str("/new"),
        |s| s.config.model.push('2'),
        |s| s.output_token_cap = 12000,
        |s| s.config.temperature = 0.1,
        |s| s.json_mode = "off".into(),
    ];
    for (i, change) in changes.into_iter().enumerate() {
        let mut settings = baseline.clone();
        change(&mut settings);
        f.store
            .save_ai_settings(&format!("settings-{i}"), settings)
            .unwrap();
        let id = format!("request-{i}");
        assert!(f.store.begin_ai(request(&f.item, &id)).unwrap().1);
        f.store
            .end_ai(&id, AppError::new("cancelled", "No HTTP dispatched"))
            .unwrap();
    }
    f.store.save_ai_settings("restore", baseline).unwrap();
    fs::write(
        &f.item.path,
        RAW.replace(
            "</technicalspecs>",
            "<section name=\"Runtime\"><item>100 min</item></section></technicalspecs>",
        ),
    )
    .unwrap();
    let item = f.store.inspect_item(&f.item.id).unwrap();
    assert!(
        f.store
            .begin_ai(request(&item, "changed-full-specs"))
            .unwrap()
            .1
    );
}
#[test]
fn explicit_retry_is_required_for_unfingerprinted_records_and_cancellation_does_not_clear_them() {
    let f = fixture(true);
    let (record, execute) = f.store.begin_ai(request(&f.item, "unknown")).unwrap();
    assert!(!execute);
    assert_eq!(record.phase, "skipped-legacy-unverified");
    let mut retry = request(&f.item, "retry");
    retry.retry_failed = true;
    assert!(f.store.begin_ai(retry).unwrap().1);
    f.store
        .end_ai("retry", AppError::new("cancelled", "No HTTP dispatched"))
        .unwrap();
    assert_eq!(
        f.store
            .begin_ai(request(&f.item, "still-protected"))
            .unwrap()
            .0
            .phase,
        "skipped-legacy-unverified"
    );
    let mut retry = request(&f.item, "success");
    retry.retry_failed = true;
    f.store.begin_ai(retry).unwrap();
    succeed(&f.store, "success");
    let saved = f
        .store
        .preferences(&Failure::preference_key(&f.item.path))
        .unwrap();
    assert_eq!(saved["active"], false);
    assert!(saved["resolved_at"].is_string());
    assert_eq!(saved["entry"]["message"], "Original fixture error");
    let (cached, execute) = f.store.begin_ai(request(&f.item, "after-success")).unwrap();
    assert!(!execute);
    assert!(cached.cached);
    assert!(!cached.legacy_failure.unwrap().active);
}
#[test]
fn migration_preview_conflicts_with_a_newly_resolved_failure_and_rolls_back() {
    let f = fixture(false);
    let plan = f
        .store
        .prepare_migration("second-import", &f.old, "itm-engine")
        .unwrap();
    let mut retry = request(&f.item, "retry");
    retry.retry_failed = true;
    f.store.begin_ai(retry).unwrap();
    succeed(&f.store, "retry");
    assert_eq!(
        f.store
            .apply_migration("second-import", &plan.fingerprint)
            .unwrap_err()
            .code,
        "migration-configuration-conflict"
    );
    assert!(f
        .store
        .legacy_artifact("second-import", "ai-failure-queue.json")
        .is_err());
    assert_eq!(
        f.store
            .preferences(&Failure::preference_key(&f.item.path))
            .unwrap()["active"],
        false
    );
}
#[test]
fn invalid_or_duplicate_queue_entries_are_not_silently_activated() {
    let f = fixture(false);
    let path = f.old.join("ai-failure-queue.json");
    let mut queue: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    queue["items"][0]["fingerprint"] = json!("not-a-hash");
    fs::write(&path, serde_json::to_vec(&queue).unwrap()).unwrap();
    let plan = f
        .store
        .prepare_migration("bad", &f.old, "itm-engine")
        .unwrap();
    assert!(!plan
        .adapters
        .iter()
        .any(|a| a.target.starts_with("legacy-ai-failure:")));
    assert!(plan
        .warnings
        .iter()
        .any(|v| v.contains("Invalid historical failure")));
    queue["items"][0]
        .as_object_mut()
        .unwrap()
        .remove("fingerprint");
    let entry = queue["items"][0].clone();
    queue["items"].as_array_mut().unwrap().push(entry);
    fs::write(&path, serde_json::to_vec(&queue).unwrap()).unwrap();
    assert_eq!(
        f.store
            .prepare_migration("ambiguous", &f.old, "itm-engine")
            .unwrap_err()
            .code,
        "migration-ambiguous-ai-failure"
    );
}

#[test]
fn batch_preserves_skip_reason_and_retries_only_after_new_scope_approval() {
    use itm_core::batch::{BatchEngine, BatchMode, BatchRequest, Outcome};
    let f = fixture(false);
    for (id, retry) in [("batch", false), ("retry-batch", true)] {
        let task = f
            .store
            .plan_batch(BatchRequest {
                operation_id: id.into(),
                space: Space::Movie,
                item_ids: vec![f.item.id.clone()],
                root_ids: vec![],
                retry_failed: retry,
                engine: BatchEngine::Ai,
                mode: BatchMode::Rebuild,
            })
            .unwrap();
        assert!(f
            .store
            .run_batch_next(|| false, |_| {}, |_, _| panic!("Unapproved batch executed"))
            .unwrap()
            .is_none());
        f.store
            .approve_batch_scope(id, &task.batch.unwrap().plan_hash)
            .unwrap();
        f.store
            .run_batch_next(
                || false,
                |_| {},
                |task, row| {
                    let (record, execute) = f.store.begin_batch_ai(
                        Request {
                            operation_id: row.request_id.clone(),
                            item_id: row.item.id.clone(),
                            expected_hash: row.item.source_hash.clone(),
                            force: true,
                            retry_failed: retry,
                        },
                        &task.id,
                    )?;
                    assert_eq!(execute, retry);
                    if retry {
                        f.store.end_ai(
                            &row.request_id,
                            AppError::new("cancelled", "Stopped before transport"),
                        )?;
                        Ok(Outcome::failed(AppError::new(
                            "cancelled",
                            "Stopped before transport",
                        )))
                    } else {
                        assert_eq!(record.phase, "skipped-legacy-failure");
                        Ok(Outcome {
                            phase: record.phase,
                            error: record.error,
                            candidate_hash: None,
                        })
                    }
                },
            )
            .unwrap();
    }
    let rows = f.store.batch_items("batch").unwrap();
    assert_eq!(rows[0].phase, "skipped-legacy-failure");
    assert_eq!(
        rows[0].error.as_ref().unwrap().path.as_deref(),
        Some(f.item.path.as_str())
    );
    assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
}
