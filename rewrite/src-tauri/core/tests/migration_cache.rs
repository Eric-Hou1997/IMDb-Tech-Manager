use itm_core::{acquisition::*, imdb_cache::*, specs::*, store::Store, *};
use serde_json::json;
use std::fs;
fn setup() -> (tempfile::TempDir, Store, MediaItem, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    fs::write(media.join("电影.nfo"),"<movie><title>Fixture</title><uniqueid type=\"imdb\">tt1234567</uniqueid><tag>External</tag></movie>").unwrap();
    let legacy = root.join("legacy");
    fs::create_dir_all(legacy.join("cache")).unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    store
        .configure(
            "roots",
            Configuration {
                roots: vec![LibraryRoot {
                    id: "r".into(),
                    space: Space::Movie,
                    path: media.to_str().unwrap().into(),
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
    let item = store.all_items().unwrap().remove(0);
    (tmp, store, item, legacy)
}
fn request(item: &MediaItem, id: &str, refresh: bool) -> FetchRequest {
    FetchRequest {
        operation_id: id.into(),
        item_id: item.id.clone(),
        expected_hash: item.source_hash.clone(),
        refresh,
    }
}
fn old(age: i64, status: &str) -> serde_json::Value {
    json!({"cache_version":8,"parser_version":1,"imdb":"tt1234567","fetched_at":(chrono::Utc::now()-chrono::Duration::seconds(age)).to_rfc3339(),"ok":status=="ok","status":status,"specs":if status=="ok" {json!({"Camera":["Arri Alexa"]})} else {json!({})},"parser":"legacy-v1"})
}
fn save(legacy: &std::path::Path, value: &serde_json::Value) -> Vec<u8> {
    let bytes = serde_json::to_vec_pretty(value).unwrap();
    fs::write(legacy.join("cache/tt1234567.json"), &bytes).unwrap();
    bytes
}
#[test]
fn imported_cache_serves_real_request_then_preview_write_and_undo_without_network() {
    let (tmp, store, item, legacy) = setup();
    let raw = save(&legacy, &old(60, "ok"));
    let original = fs::read(&item.path).unwrap();
    let mtime = fs::metadata(&item.path).unwrap().modified().unwrap();
    let plan = store
        .prepare_migration("import", &legacy, "itm-engine")
        .unwrap();
    assert_eq!(plan.cache_entries[0].state, "source");
    let receipt = store.apply_migration("import", &plan.fingerprint).unwrap();
    assert_eq!(receipt.applied_cache_entries, 1);
    assert_eq!(
        store
            .apply_migration("import", &plan.fingerprint)
            .unwrap()
            .applied_cache_entries,
        1
    );
    assert_eq!(fs::read(&item.path).unwrap(), original);
    assert_eq!(fs::metadata(&item.path).unwrap().modified().unwrap(), mtime);
    assert_eq!(
        store
            .legacy_artifact("import", "cache/tt1234567.json")
            .unwrap(),
        raw
    );
    let (record, send) = store.begin_fetch(request(&item, "fetch", false)).unwrap();
    assert!(!send && record.cached);
    assert_eq!(record.source.unwrap().specs["Camera"], vec!["Arri Alexa"]);
    let p = store.preview_source("write", "fetch").unwrap();
    let journal = tmp.path().canonicalize().unwrap().join("journal");
    store
        .apply_specs("write", &p.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(store.item(&item.id).unwrap().tags, item.tags);
    let undo = store.preview_undo("undo", "write", &journal).unwrap();
    store
        .apply_specs("undo", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&item.path).unwrap(), original);
    drop(store);
    let reopened = Store::open(&tmp.path().canonicalize().unwrap().join("state.sqlite")).unwrap();
    assert!(
        !reopened
            .begin_fetch(request(&item, "again", false))
            .unwrap()
            .1
    );
}
#[test]
fn original_positive_empty_and_transient_expiry_boundaries_are_retained() {
    for (age, status, send, phase) in [
        (29 * 86400, "ok", false, "completed"),
        (31 * 86400, "ok", true, "requested"),
        (6 * 86400, "no-tech", false, "completed"),
        (8 * 86400, "no-tech", true, "requested"),
        (60, "timeout", false, "failed"),
        (3601, "timeout", true, "requested"),
    ] {
        let (_tmp, store, item, legacy) = setup();
        save(&legacy, &old(age, status));
        let plan = store
            .prepare_migration("import", &legacy, "itm-engine")
            .unwrap();
        store.apply_migration("import", &plan.fingerprint).unwrap();
        let (record, actual) = store.begin_fetch(request(&item, "fetch", false)).unwrap();
        assert_eq!(actual, send, "{status} age={age}");
        assert_eq!(record.phase, phase);
        if status == "no-tech" && !send {
            assert_eq!(record.source.unwrap().status, SourceStatus::Empty);
        }
        assert!(
            store
                .begin_fetch(request(&item, "refresh", true))
                .unwrap()
                .1
        );
    }
}
#[test]
fn cached_failures_keep_identity_and_do_not_extend_the_original_cooldown() {
    let (_tmp, store, item, legacy) = setup();
    let value = old(3500, "timeout");
    save(&legacy, &value);
    let plan = store
        .prepare_migration("import", &legacy, "itm-engine")
        .unwrap();
    store.apply_migration("import", &plan.fingerprint).unwrap();
    let (record, send) = store.begin_fetch(request(&item, "cached", false)).unwrap();
    assert!(!send);
    let error = record.error.unwrap();
    assert_eq!(error.code, "imdb-timeout");
    assert_eq!(error.path.as_deref(), Some(item.path.as_str()));
    assert_eq!(error.operation_id.as_deref(), Some("cached"));
    let failure: Failure =
        serde_json::from_value(store.preferences("imdb-failure:tt1234567").unwrap()).unwrap();
    assert_eq!(failure.fetched_at, value["fetched_at"]);
    let plan = store
        .plan_batch(batch::BatchRequest {
            operation_id: "batch".into(),
            space: Space::Movie,
            item_ids: vec![item.id.clone()],
            root_ids: vec![],
            retry_failed: false,
            engine: batch::BatchEngine::Specs,
            mode: batch::BatchMode::Generate,
        })
        .unwrap();
    store
        .approve_batch_scope("batch", &plan.batch.unwrap().plan_hash)
        .unwrap();
    store
        .run_batch_next(
            || false,
            |_| {},
            |task, row| {
                assert!(
                    store
                        .begin_batch_fetch(request(&item, &row.request_id, false), &task.id)?
                        .1
                );
                Ok(batch::Outcome {
                    phase: "fixture-finished".into(),
                    error: None,
                    candidate_hash: None,
                })
            },
        )
        .unwrap();
}
#[test]
fn changed_destination_rolls_back_import_and_newer_cache_wins() {
    let (_tmp, store, item, legacy) = setup();
    save(&legacy, &old(600, "ok"));
    let plan = store
        .prepare_migration("import", &legacy, "itm-engine")
        .unwrap();
    store.begin_fetch(request(&item, "live", true)).unwrap();
    let source = SourceSpecs {
        status: SourceStatus::Ok,
        imdb: item.imdb.clone(),
        specs: Specs::from([("Camera".into(), vec!["New live value".into()])]),
        fetched_at: chrono::Utc::now().to_rfc3339(),
        parser: "live".into(),
    };
    store.finish_fetch("live", Ok(source)).unwrap();
    assert_eq!(
        store
            .apply_migration("import", &plan.fingerprint)
            .unwrap_err()
            .code,
        "migration-cache-conflict"
    );
    assert!(store
        .legacy_artifact("import", "cache/tt1234567.json")
        .is_err());
    let plan = store
        .prepare_migration("replan", &legacy, "itm-engine")
        .unwrap();
    assert_eq!(plan.cache_entries[0].state, "kept-current");
    assert_eq!(
        store
            .apply_migration("replan", &plan.fingerprint)
            .unwrap()
            .applied_cache_entries,
        0
    );
    assert_eq!(
        store
            .begin_fetch(request(&item, "result", false))
            .unwrap()
            .0
            .source
            .unwrap()
            .specs["Camera"],
        vec!["New live value"]
    );
}
#[test]
fn unknown_versions_identity_and_invalid_data_are_archived_without_becoming_facts() {
    for change in [
        json!({"cache_version":99}),
        json!({"imdb":"tt7654321"}),
        json!({"specs":{"Unknown field":["value"]}}),
    ] {
        let (_tmp, store, item, legacy) = setup();
        let mut value = old(60, "ok");
        for (k, v) in change.as_object().unwrap() {
            value[k] = v.clone();
        }
        let raw = save(&legacy, &value);
        let plan = store
            .prepare_migration("import", &legacy, "itm-engine")
            .unwrap();
        assert_eq!(plan.cache_entries[0].state, "archived-incompatible");
        assert_eq!(
            store
                .apply_migration("import", &plan.fingerprint)
                .unwrap()
                .applied_cache_entries,
            0
        );
        assert_eq!(
            store
                .legacy_artifact("import", "cache/tt1234567.json")
                .unwrap(),
            raw
        );
        assert!(store.begin_fetch(request(&item, "fetch", false)).unwrap().1);
    }
}
