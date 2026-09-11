use itm_core::{
    ai::{
        self,
        job::{Request, Settings},
        legacy_cache,
    },
    migration::ai_profile,
    store::Store,
    *,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
const RAW: &str = "<movie><title>缓存迁移</title><tag>External</tag><technicalspecs source=\"IMDb\" imdbid=\"tt1234567\"><section name=\"Camera\"><item>Camera Model</item></section></technicalspecs></movie>";
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    old: PathBuf,
    store: Store,
    item: MediaItem,
    source: String,
}
fn fixture(change: impl FnOnce(&mut Value), nested: bool) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let old = root.join("legacy");
    let media = root.join("电影");
    fs::create_dir(&media).unwrap();
    fs::write(media.join("缓存.nfo"), RAW).unwrap();
    let profile = json!({"library_roots":{"movies":[media]},"ai":{"enabled":true,"provider":"openai-compatible","base_url":"https://provider.example/v1","model":"requested-model","prompt":"  自定义\n提示词  ","temperature":0.0,"top_p":1.0,"extra_body":"{ \"metadata\": {\"name\": \"中文\"} }"}});
    let settings = ai_profile::adapt(&profile).unwrap().unwrap();
    let specs = Specs::from([("Camera".into(), vec!["Camera Model".into()])]);
    let key = legacy_cache::key(
        &settings,
        &specs,
        &[],
        profile["ai"]["extra_body"].as_str().unwrap(),
    )
    .unwrap();
    let prefix = if nested { "data/" } else { "" };
    fs::create_dir_all(old.join(format!("{prefix}ai-cache"))).unwrap();
    fs::write(
        old.join(format!("{prefix}config.json")),
        serde_json::to_vec(&profile).unwrap(),
    )
    .unwrap();
    let source = format!("{prefix}ai-cache/{key}.json");
    let mut cache = json!({"cache_schema":2,"created_at":"2024-01-02T03:04:05+00:00","model":"resolved-model-2024","prompt_hash":&hash(format!("{}\n\n{}",settings.config.prompt.trim(),ai::LANGUAGE_BOUNDARY).as_bytes())[..16],"spec_hash":"a".repeat(64),"output_language":"zh-CN","usage":{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120,"completion_tokens_details":{"reasoning_tokens":7}},"cost":0.25,"result":{"tags":[{"value":"Camera Model","field":"Camera","source_indexes":[0],"confidence":"medium"}],"warnings":[]}});
    change(&mut cache);
    fs::write(old.join(&source), serde_json::to_vec(&cache).unwrap()).unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let plan = store
        .prepare_migration("import", &old, "itm-engine")
        .unwrap();
    store.apply_migration("import", &plan.fingerprint).unwrap();
    let config = store.configuration().unwrap();
    store
        .submit(ScanRequest {
            operation_id: "scan".into(),
            space: Space::Movie,
            root_ids: vec![config.roots[0].id.clone()],
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
        source,
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

#[test]
fn imported_cache_survives_restart_source_offline_and_preserves_historical_accounting() {
    let f = fixture(|_| {}, false);
    let mtime = fs::metadata(&f.item.path).unwrap().modified().unwrap();
    let (value, execute) = f.store.begin_ai(request(&f.item, "cached")).unwrap();
    assert!(!execute);
    assert!(value.cached && ai::job::needs_review(&value));
    assert_eq!(value.meter.attempts, 0);
    assert_eq!(value.meter.successful_http, 0);
    assert_eq!(value.meter.current.total, 0);
    assert!(value.meter.raw_usage.is_empty());
    assert_eq!(value.meter.historical_cache.total, 120);
    assert_eq!(value.cost, 0.0);
    assert_eq!(value.historical_cost, 0.25);
    assert_eq!(
        value.legacy_cache.as_ref().unwrap().raw_usage["completion_tokens_details"]
            ["reasoning_tokens"],
        7
    );
    assert!(ai::job::next(&value).unwrap().is_none());
    fs::rename(&f.old, f.root.join("offline")).unwrap();
    drop(f.store);
    let store = Store::open(&f.root.join("state.sqlite")).unwrap();
    let (replay, execute) = store.begin_ai(request(&f.item, "cached")).unwrap();
    assert!(!execute);
    assert_eq!(
        serde_json::to_value(replay).unwrap(),
        serde_json::to_value(value).unwrap()
    );
    assert!(store.begin_ai(request(&f.item, "second")).unwrap().0.cached);
    assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
    assert_eq!(
        fs::metadata(&f.item.path).unwrap().modified().unwrap(),
        mtime
    );
}

#[test]
fn non_tag_specs_prices_and_review_policy_do_not_invalidate_legacy_identity() {
    let f = fixture(|_| {}, true);
    let mut settings = f.store.ai_settings().unwrap();
    settings.input_price_per_million = 999.0;
    settings.warning_policy = "accept".into();
    settings.run_request_limit = 1;
    f.store.save_ai_settings("prices", settings).unwrap();
    fs::write(
        &f.item.path,
        RAW.replace(
            "</technicalspecs>",
            "<section name=\"Runtime\"><item>900 min</item></section></technicalspecs>",
        ),
    )
    .unwrap();
    let current = f.store.inspect_item(&f.item.id).unwrap();
    let (value, execute) = f.store.begin_ai(request(&current, "cached")).unwrap();
    assert!(!execute);
    assert!(!ai::job::needs_review(&value));
    assert_eq!(value.historical_cost, 0.25);
    assert_eq!(value.legacy_cache.unwrap().source, f.source);
}

#[test]
fn changed_request_fields_and_force_cannot_reuse_old_result() {
    let f = fixture(|_| {}, false);
    let baseline = f.store.ai_settings().unwrap();
    let mutations: Vec<fn(&mut Settings)> = vec![
        |s| s.config.model.push('2'),
        |s| s.config.provider.push('2'),
        |s| s.config.base_url.push_str("/other"),
        |s| s.config.protocol = ai::Protocol::Anthropic,
        |s| s.config.prompt.push('!'),
        |s| s.config.temperature = 0.5,
        |s| s.config.top_p = 0.7,
        |s| s.config.max_tokens = 3000,
        |s| s.json_mode = "off".into(),
        |s| s.config.thinking_mode = "on".into(),
        |s| s.config.prompt_cache_mode = "off".into(),
        |s| {
            s.config.extra_body.insert("new".into(), json!(true));
        },
    ];
    for (i, change) in mutations.into_iter().enumerate() {
        let mut settings = baseline.clone();
        change(&mut settings);
        f.store
            .save_ai_settings(&format!("config-{i}"), settings)
            .unwrap();
        let (record, execute) = f
            .store
            .begin_ai(request(&f.item, &format!("different-{i}")))
            .unwrap();
        assert!(execute && !record.cached, "mutation {i}");
        f.store
            .end_ai(
                &record.request.operation_id,
                AppError::new("cancelled", "Test ended before HTTP"),
            )
            .unwrap();
    }
    f.store.save_ai_settings("restore", baseline).unwrap();
    let mut force = request(&f.item, "force");
    force.force = true;
    assert!(f.store.begin_ai(force).unwrap().1);
}

#[test]
fn changed_language_specs_and_owned_tags_are_part_of_identity() {
    let f = fixture(|_| {}, false);
    let settings = f.store.ai_settings().unwrap();
    let specs = Specs::from([("Camera".into(), vec!["Camera Model".into()])]);
    let extra = serde_json::to_string(&settings.config.extra_body).unwrap();
    let before = legacy_cache::key(&settings, &specs, &[], &extra).unwrap();
    assert_ne!(
        before,
        legacy_cache::key(
            &settings,
            &specs,
            &[json!({"value":"Owned","source":"ai"})],
            &extra
        )
        .unwrap()
    );
    let mut other = settings.clone();
    other.config.output_language = "en-US".into();
    assert_ne!(
        before,
        legacy_cache::key(&other, &specs, &[], &extra).unwrap()
    );
    fs::write(
        &f.item.path,
        RAW.replace("Camera Model", "Different camera"),
    )
    .unwrap();
    let current = f.store.inspect_item(&f.item.id).unwrap();
    assert!(
        f.store
            .begin_ai(request(&current, "changed-specs"))
            .unwrap()
            .1
    );
}

#[test]
fn invalid_imported_result_fails_closed_and_explicit_force_bypasses_it() {
    for change in [
        (|v: &mut Value| v["result"]["tags"][0]["source_indexes"] = json!([99])) as fn(&mut Value),
        |v| v["cache_schema"] = json!(9),
        |v| v["prompt_hash"] = json!("wrong"),
        |v| v["cost"] = json!(-1),
        |v| v["usage"]["prompt_tokens"] = json!(-3),
        |v| v["created_at"] = json!("invalid"),
        |v| v["output_language"] = json!("en-US"),
    ] {
        let f = fixture(change, false);
        let error = f.store.begin_ai(request(&f.item, "bad-cache")).unwrap_err();
        assert_eq!(error.code, "legacy-ai-cache-invalid");
        assert_eq!(error.path, Some(f.source.clone()));
        assert!(f.store.operation_result("bad-cache").is_err());
        let mut force = request(&f.item, "force");
        force.force = true;
        assert!(f.store.begin_ai(force).unwrap().1);
        assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
    }
}

#[test]
fn anthropic_historical_usage_includes_cache_creation_and_read_tokens() {
    let f = fixture(
        |v| v["usage"] = json!({"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":7}),
        false,
    );
    let (value, execute) = f.store.begin_ai(request(&f.item, "hit")).unwrap();
    assert!(!execute);
    assert_eq!(
        value.meter.historical_cache,
        ai::Usage {
            input: 60,
            output: 7,
            total: 67
        }
    );
    assert_eq!(value.cost, 0.0);
}

#[test]
fn newer_import_is_selected_without_rewriting_old_history_and_archive_corruption_is_visible() {
    let f = fixture(|_| {}, false);
    let original = f.store.begin_ai(request(&f.item, "first")).unwrap().0;
    let mut cache: Value =
        serde_json::from_slice(&fs::read(f.old.join(&f.source)).unwrap()).unwrap();
    cache["cost"] = json!(0.75);
    cache["model"] = json!("later-response-model");
    fs::write(f.old.join(&f.source), serde_json::to_vec(&cache).unwrap()).unwrap();
    let plan = f
        .store
        .prepare_migration("second-import", &f.old, "itm-engine")
        .unwrap();
    f.store
        .apply_migration("second-import", &plan.fingerprint)
        .unwrap();
    let newer = f.store.begin_ai(request(&f.item, "second")).unwrap().0;
    assert_eq!(newer.historical_cost, 0.75);
    assert_eq!(
        newer.legacy_cache.as_ref().unwrap().import_id,
        "second-import"
    );
    assert_eq!(
        serde_json::to_value(f.store.ai_record("first").unwrap()).unwrap(),
        serde_json::to_value(original).unwrap()
    );
    let db = rusqlite::Connection::open(f.root.join("state.sqlite")).unwrap();
    db.execute(
        "UPDATE legacy_artifacts SET body=?1 WHERE import_id='second-import' AND path=?2",
        rusqlite::params![b"{}".as_slice(), f.source],
    )
    .unwrap();
    let error = f.store.begin_ai(request(&f.item, "corrupt")).unwrap_err();
    assert_eq!(error.code, "legacy-ai-cache-invalid");
    // The already persisted result remains queryable; a new request cannot
    // silently select the older imported cache after corruption of the latest.
    assert_eq!(f.store.ai_record("second").unwrap().historical_cost, 0.75);
}

#[test]
fn reviewed_legacy_candidate_uses_original_model_and_existing_transaction_undo() {
    let f = fixture(|_| {}, false);
    f.store.begin_ai(request(&f.item, "hit")).unwrap();
    let preview = f.store.preview_ai("write", "hit").unwrap();
    assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
    if cfg!(feature = "write-prototype") {
        f.store
            .apply_specs(
                "write",
                &preview.after_hash,
                &f.root.join("journal"),
                || false,
            )
            .unwrap();
        let item = f.store.item(&f.item.id).unwrap();
        assert_eq!(item.tags[0].ownership, Ownership::External);
        assert_eq!(item.inspection.tag_model, "resolved-model-2024");
        let undo = f
            .store
            .preview_undo("undo", "write", &f.root.join("journal"))
            .unwrap();
        f.store
            .apply_specs("undo", &undo.after_hash, &f.root.join("journal"), || false)
            .unwrap();
        assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
    }
}
