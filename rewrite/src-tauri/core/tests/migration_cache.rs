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

fn raw_page(legacy: &std::path::Path, age: i64, page: &str) -> (Vec<u8>, Vec<u8>) {
    use std::io::Write;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(page.as_bytes()).unwrap();
    let packed = gzip.finish().unwrap();
    let metadata=serde_json::to_vec(&json!({"url":"https://www.imdb.com/title/tt1234567/technical/","fetched_at":(chrono::Utc::now()-chrono::Duration::seconds(age)).to_rfc3339(),"body_hash":hash(page.as_bytes()),"bytes":page.chars().count()})).unwrap();
    fs::write(legacy.join("cache/raw-tt1234567.json"), &metadata).unwrap();
    fs::write(legacy.join("cache/raw-tt1234567.html.gz"), &packed).unwrap();
    (metadata, packed)
}
fn html() -> String {
    format!(
        "<script id='__NEXT_DATA__'>{}</script>",
        json!({"props":{"title":{"id":"tt1234567","runtimes":{"edges":[]},"technicalSpecifications":{"cameras":{"items":[{"camera":"合成相机"}]}}}}})
    )
}
#[test]
fn historical_dom_cache_is_reused_with_original_archive_and_no_http() {
    let (_tmp, store, item, legacy) = setup();
    let page = "<link rel='canonical' href='https://www.imdb.com/title/tt1234567/technical/'><h3>Camera</h3><li>Historical DOM Camera</li>";
    let (meta, packed) = raw_page(&legacy, 60, page);
    let plan = store
        .prepare_migration("dom-import", &legacy, "itm-engine")
        .unwrap();
    assert_eq!(plan.cache_entries[0].state, "raw-source");
    store
        .apply_migration("dom-import", &plan.fingerprint)
        .unwrap();
    let (record, network) = store
        .begin_fetch(request(&item, "dom-fetch", false))
        .unwrap();
    assert!(!network);
    assert!(record.cached);
    assert!(record.attempts.is_empty());
    let source = record.source.unwrap();
    assert_eq!(source.parser, "html-lines");
    assert_eq!(source.specs["Camera"], ["Historical DOM Camera"]);
    assert_eq!(
        store
            .legacy_artifact("dom-import", "cache/raw-tt1234567.json")
            .unwrap(),
        meta
    );
    assert_eq!(
        store
            .legacy_artifact("dom-import", "cache/raw-tt1234567.html.gz")
            .unwrap(),
        packed
    );
}
#[test]
fn original_gzip_pair_is_reused_without_http_reparsed_after_parser_change_and_undoable() {
    let (tmp, store, item, legacy) = setup();
    let (meta, packed) = raw_page(&legacy, 60, &html());
    let original = fs::read(&item.path).unwrap();
    let plan = store
        .prepare_migration("raw-import", &legacy, "itm-engine")
        .unwrap();
    assert_eq!(plan.cache_entries[0].state, "raw-source");
    assert_eq!(
        store
            .apply_migration("raw-import", &plan.fingerprint)
            .unwrap()
            .applied_cache_entries,
        1
    );
    assert_eq!(
        store
            .legacy_artifact("raw-import", "cache/raw-tt1234567.html.gz")
            .unwrap(),
        packed
    );
    assert_eq!(
        store
            .legacy_artifact("raw-import", "cache/raw-tt1234567.json")
            .unwrap(),
        meta
    );
    assert_eq!(fs::read(&item.path).unwrap(), original);
    let (record, send) = store
        .begin_fetch(request(&item, "from-raw", false))
        .unwrap();
    assert!(!send);
    assert!(record.cached && record.attempts.is_empty());
    assert_eq!(
        record.source.as_ref().unwrap().specs["Camera"],
        vec!["合成相机"]
    );
    assert_eq!(
        record.source.unwrap().fetched_at,
        serde_json::from_slice::<serde_json::Value>(&meta).unwrap()["fetched_at"]
    );
    let journal = tmp.path().canonicalize().unwrap().join("backups");
    let p = store.preview_source("raw-write", "from-raw").unwrap();
    store
        .apply_specs("raw-write", &p.after_hash, &journal, || false)
        .unwrap();
    let p = store
        .preview_undo("raw-undo", "raw-write", &journal)
        .unwrap();
    store
        .apply_specs("raw-undo", &p.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&item.path).unwrap(), original);
    let db = rusqlite::Connection::open(tmp.path().join("state.sqlite")).unwrap();
    db.execute("UPDATE imdb_cache SET parser_version=0", [])
        .unwrap();
    drop(db);
    drop(store);
    let store = Store::open(&tmp.path().join("state.sqlite")).unwrap();
    assert!(
        !store
            .begin_fetch(request(&item, "reparse", false))
            .unwrap()
            .1
    );
    assert!(
        store
            .begin_fetch(request(&item, "refresh-raw", true))
            .unwrap()
            .1
    );
}
#[test]
fn raw_fallback_respects_failure_cooldown_and_explicit_retry() {
    let (_tmp, store, item, legacy) = setup();
    raw_page(&legacy, 60, &html());
    save(&legacy, &old(30, "timeout"));
    let plan = store
        .prepare_migration("import", &legacy, "itm-engine")
        .unwrap();
    store.apply_migration("import", &plan.fingerprint).unwrap();
    let (record, send) = store
        .begin_fetch(request(&item, "ordinary", false))
        .unwrap();
    assert!(!send);
    assert_eq!(record.phase, "failed");
    assert_eq!(record.error.unwrap().code, "imdb-timeout");
    store.save_preference("imdb-failure:tt1234567",&json!({"imdb":"tt1234567","fetched_at":(chrono::Utc::now()-chrono::Duration::hours(2)).to_rfc3339(),"error":AppError::new("imdb-timeout","old timeout")})).unwrap();
    assert!(
        !store
            .begin_fetch(request(&item, "cooled-down", false))
            .unwrap()
            .1
    );
    assert!(
        store
            .begin_fetch(request(&item, "explicit-refresh", true))
            .unwrap()
            .1
    );
}
#[test]
fn raw_corruption_expiry_challenge_and_unsupported_pages_never_become_specs() {
    for kind in [
        "expired",
        "hash",
        "url",
        "gzip",
        "waf",
        "unknown",
        "orphan",
        "oversized",
    ] {
        let (_tmp, store, item, legacy) = setup();
        let page = match kind {
            "waf" => format!("{} AwsWafIntegration", html()),
            "unknown" => "<html>Changed structure</html>".into(),
            "oversized" => "x".repeat(8 * 1024 * 1024 + 1),
            _ => html(),
        };
        let (mut meta, _) = raw_page(
            &legacy,
            if kind == "expired" { 31 * 86400 } else { 60 },
            &page,
        );
        if ["hash", "url"].contains(&kind) {
            let mut value: serde_json::Value = serde_json::from_slice(&meta).unwrap();
            if kind == "hash" {
                value["body_hash"] = json!("0".repeat(64));
            } else {
                value["url"] = json!("https://www.imdb.com/title/tt7654321/technical/");
            }
            meta = serde_json::to_vec(&value).unwrap();
            fs::write(legacy.join("cache/raw-tt1234567.json"), &meta).unwrap();
        }
        if kind == "gzip" {
            fs::write(legacy.join("cache/raw-tt1234567.html.gz"), [31, 139, 8]).unwrap();
        }
        if kind == "orphan" {
            fs::remove_file(legacy.join("cache/raw-tt1234567.json")).unwrap();
        }
        let plan = store
            .prepare_migration("import", &legacy, "itm-engine")
            .unwrap();
        assert_eq!(plan.cache_entries.len(), 1, "{kind}");
        assert_ne!(plan.cache_entries[0].state, "raw-source", "{kind}");
        store.apply_migration("import", &plan.fingerprint).unwrap();
        assert!(
            store
                .begin_fetch(request(&item, "acquire", false))
                .unwrap()
                .1,
            "{kind}"
        );
    }
}
#[test]
fn changed_raw_destination_or_source_rolls_back_and_archived_integrity_is_checked() {
    {
        let (_tmp, store, _item, legacy) = setup();
        raw_page(&legacy, 60, &html());
        let plan = store
            .prepare_migration("changed-source", &legacy, "itm-engine")
            .unwrap();
        fs::write(
            legacy.join("cache/raw-tt1234567.html.gz"),
            b"changed after review",
        )
        .unwrap();
        assert_eq!(
            store
                .apply_migration("changed-source", &plan.fingerprint)
                .unwrap_err()
                .code,
            "migration-source-changed"
        );
        assert!(store
            .legacy_artifact("changed-source", "cache/raw-tt1234567.json")
            .is_err());
    }
    let (tmp, store, item, legacy) = setup();
    let (original_meta, original_body) = raw_page(&legacy, 60, &html());
    let other = store
        .prepare_migration("other", &legacy, "itm-engine")
        .unwrap();
    store.apply_migration("other", &other.fingerprint).unwrap();
    raw_page(&legacy, 0, &html());
    let plan = store
        .prepare_migration("first", &legacy, "itm-engine")
        .unwrap();
    assert_eq!(plan.cache_entries[0].state, "raw-source");
    let mut current = store.preferences("imdb-raw:tt1234567").unwrap();
    current["concurrent_fixture"] = json!(true);
    store
        .save_preference("imdb-raw:tt1234567", &current)
        .unwrap();
    assert_eq!(
        store
            .apply_migration("first", &plan.fingerprint)
            .unwrap_err()
            .code,
        "migration-cache-conflict"
    );
    assert!(store
        .legacy_artifact("first", "cache/raw-tt1234567.json")
        .is_err());
    fs::write(legacy.join("cache/raw-tt1234567.json"), original_meta).unwrap();
    fs::write(legacy.join("cache/raw-tt1234567.html.gz"), original_body).unwrap();
    let replan = store
        .prepare_migration("again", &legacy, "itm-engine")
        .unwrap();
    assert_eq!(replan.cache_entries[0].state, "kept-current");
    assert_eq!(
        store
            .apply_migration("again", &replan.fingerprint)
            .unwrap()
            .applied_cache_entries,
        0
    );
    let db = rusqlite::Connection::open(tmp.path().join("state.sqlite")).unwrap();
    db.execute(
        "UPDATE legacy_artifacts SET body=?1 WHERE import_id='other' AND path LIKE '%.html.gz'",
        [b"changed".to_vec()],
    )
    .unwrap();
    drop(db);
    assert_eq!(
        store
            .begin_fetch(request(&item, "corrupt", false))
            .unwrap_err()
            .code,
        "legacy-raw-integrity"
    );
    assert!(
        store
            .begin_fetch(request(&item, "bypass-corrupt", true))
            .unwrap()
            .1
    );
}

#[test]
fn old_missing_hash_is_compatible_but_malformed_hash_type_cannot_disable_integrity_checks() {
    let (_tmp, _store, _item, legacy) = setup();
    let (meta, packed) = raw_page(&legacy, 60, &html());
    let mut value: serde_json::Value = serde_json::from_slice(&meta).unwrap();
    value.as_object_mut().unwrap().remove("body_hash");
    assert!(decode_raw("tt1234567", &serde_json::to_vec(&value).unwrap(), &packed).is_ok());
    value["body_hash"] = json!(123);
    assert_eq!(
        decode_raw("tt1234567", &serde_json::to_vec(&value).unwrap(), &packed)
            .unwrap_err()
            .code,
        "legacy-raw-integrity"
    );
}
