use itm_core::{acquisition::*, specs::SourceSpecs, store::Store, *};
use std::fs;
fn setup() -> (tempfile::TempDir, Store, MediaItem, FetchRequest) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    fs::write(media.join("电影.nfo"),b"\xef\xbb\xbf<movie>\r\n<title>Original</title><uniqueid type=\"imdb\">tt1234567</uniqueid><tag>External</tag>\r\n</movie>").unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    store
        .configure(
            "config",
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
    let request = FetchRequest {
        operation_id: "fetch".into(),
        item_id: item.id.clone(),
        expected_hash: item.source_hash.clone(),
        refresh: false,
    };
    (temp, store, item, request)
}
fn source() -> SourceSpecs {
    SourceSpecs {
        status: Default::default(),
        imdb: "tt1234567".into(),
        specs: Specs::from([("Camera".into(), vec!["Example".into()])]),
        fetched_at: chrono::Utc::now().to_rfc3339(),
        parser: "next-data".into(),
    }
}

use itm_core::imdb_cache::{CacheRequest, CacheSettings};
fn clear(store: &Store, id: &str) -> CacheRequest {
    CacheRequest {
        operation_id: id.into(),
        settings: store.imdb_cache_status().unwrap().settings,
        clear: true,
    }
}
#[test]
fn clearing_imdb_cache_preserves_nfo_ai_index_history_and_migration_archives() {
    let (temp, store, item, request) = setup();
    let original = fs::read(&item.path).unwrap();
    store.begin_fetch(request).unwrap();
    store.finish_fetch("fetch", Ok(source())).unwrap();
    let db = rusqlite::Connection::open(temp.path().join("state.sqlite")).unwrap();
    db.execute("INSERT INTO ai_cache VALUES('protected','{}')", [])
        .unwrap();
    db.execute("INSERT INTO ai_failures VALUES('protected','{}')", [])
        .unwrap();
    db.execute("INSERT INTO legacy_artifacts VALUES('migration','raw-tt1234567.html.gz','imdb-cache','kept',?1)",[b"original archive".as_slice()]).unwrap();
    let history = serde_json::to_string(&store.fetch_history(&item.id).unwrap()).unwrap();
    let before = store.imdb_cache_status().unwrap();
    assert_eq!(before.parsed_count, 1);
    assert!(before.used_bytes > 0);
    let request = clear(&store, "clear");
    let result = store.maintain_imdb_cache(request.clone()).unwrap();
    assert_eq!(result.used_bytes, 0);
    assert_eq!(result.removed_count, 1);
    assert_eq!(store.maintain_imdb_cache(request).unwrap(), result);
    assert_eq!(fs::read(&item.path).unwrap(), original);
    assert_eq!(store.all_items().unwrap().len(), 1);
    assert_eq!(
        serde_json::to_string(&store.fetch_history(&item.id).unwrap()).unwrap(),
        history
    );
    for table in ["ai_cache", "ai_failures", "legacy_artifacts"] {
        assert_eq!(
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            1
        );
    }
    assert_eq!(result.archive_bytes, b"original archive".len() as u64);
}
#[test]
fn inflight_refresh_keeps_existing_cache_until_its_owner_finishes() {
    let (_temp, store, _, mut request) = setup();
    store.begin_fetch(request.clone()).unwrap();
    store.finish_fetch("fetch", Ok(source())).unwrap();
    request.operation_id = "refresh".into();
    request.refresh = true;
    store.begin_fetch(request).unwrap();
    let result = store
        .maintain_imdb_cache(clear(&store, "clear-busy"))
        .unwrap();
    assert_eq!(result.state, "busy");
    assert_eq!(result.protected_count, 1);
    assert_eq!(result.removed_count, 0);
    store.finish_fetch("refresh", Ok(source())).unwrap();
    let result = store
        .maintain_imdb_cache(clear(&store, "clear-idle"))
        .unwrap();
    assert_eq!(result.state, "ready");
    assert_eq!(result.used_bytes, 0);
}
#[test]
fn limits_revision_and_operation_identity_are_atomic_and_survive_restart() {
    let (temp, store, _, _) = setup();
    let defaults = store.imdb_cache_status().unwrap();
    assert_eq!(defaults.settings.limit_mb, 2048);
    let mut request = CacheRequest {
        operation_id: "budget".into(),
        settings: CacheSettings {
            revision: 0,
            limit_mb: 63,
        },
        clear: false,
    };
    assert_eq!(
        store.maintain_imdb_cache(request.clone()).unwrap_err().code,
        "invalid-cache-limit"
    );
    request.settings.limit_mb = 64;
    let result = store.maintain_imdb_cache(request.clone()).unwrap();
    assert_eq!(result.settings.revision, 1);
    request.settings.limit_mb = 128;
    assert_eq!(
        store.maintain_imdb_cache(request.clone()).unwrap_err().code,
        "operation-conflict"
    );
    request.operation_id = "stale".into();
    assert_eq!(
        store.maintain_imdb_cache(request).unwrap_err().code,
        "cache-settings-conflict"
    );
    drop(store);
    let store = Store::open(&temp.path().join("state.sqlite")).unwrap();
    assert_eq!(store.imdb_cache_status().unwrap().settings, result.settings);
}
#[test]
fn failed_automatic_cleanup_does_not_erase_completed_fetch_and_can_recover() {
    let (temp, store, _, request) = setup();
    store.begin_fetch(request).unwrap();
    store.finish_fetch("fetch", Ok(source())).unwrap();
    let db = rusqlite::Connection::open(temp.path().join("state.sqlite")).unwrap();
    db.execute("INSERT INTO imdb_cache VALUES('tt9999999',0,'{}')", [])
        .unwrap();
    db.execute_batch("CREATE TRIGGER reject_cache_delete BEFORE DELETE ON imdb_cache BEGIN SELECT RAISE(ABORT,'simulated cache storage failure'); END;").unwrap();
    assert!(store.maintain_imdb_cache_automatically().is_err());
    assert_eq!(store.fetch_record("fetch").unwrap().phase, "completed");
    assert_eq!(store.imdb_cache_status().unwrap().state, "failed");
    db.execute_batch("DROP TRIGGER reject_cache_delete")
        .unwrap();
    let result = store.maintain_imdb_cache_automatically().unwrap();
    assert_eq!(result.state, "ready");
    assert_eq!(result.parsed_count, 1);
    assert!(result.error.is_none());
}

#[test]
fn migrated_budget_is_usable_and_conflicting_destination_rolls_back_import() {
    let (temp, store, _, _) = setup();
    let legacy = temp.path().canonicalize().unwrap().join("legacy");
    fs::create_dir(&legacy).unwrap();
    fs::write(legacy.join("config.json"), r#"{"imdb_cache_max_mb":128}"#).unwrap();
    let plan = store
        .prepare_migration("budget-import", &legacy, "itm-engine")
        .unwrap();
    assert_eq!(
        plan.adapters
            .iter()
            .find(|a| a.target == "imdb-cache-settings")
            .unwrap()
            .value["limit_mb"],
        128
    );
    let request = CacheRequest {
        operation_id: "change-current".into(),
        settings: CacheSettings {
            revision: 0,
            limit_mb: 64,
        },
        clear: false,
    };
    store.maintain_imdb_cache(request).unwrap();
    assert_eq!(
        store
            .apply_migration("budget-import", &plan.fingerprint)
            .unwrap_err()
            .code,
        "migration-configuration-conflict"
    );
    assert_eq!(store.imdb_cache_status().unwrap().settings.limit_mb, 64);
    let plan = store
        .prepare_migration("budget-import-again", &legacy, "itm-engine")
        .unwrap();
    store
        .apply_migration("budget-import-again", &plan.fingerprint)
        .unwrap();
    assert_eq!(store.imdb_cache_status().unwrap().settings.limit_mb, 128);
}
#[test]
fn invalid_legacy_budget_retains_original_bytes_and_uses_the_legacy_default() {
    let (temp, store, _, _) = setup();
    let legacy = temp.path().canonicalize().unwrap().join("legacy");
    fs::create_dir(&legacy).unwrap();
    let original = br#"{"imdb_cache_max_mb":true}"#;
    fs::write(legacy.join("config.json"), original).unwrap();
    let plan = store
        .prepare_migration("invalid-budget", &legacy, "itm-engine")
        .unwrap();
    let adapter = plan
        .adapters
        .iter()
        .find(|a| a.target == "imdb-cache-settings")
        .unwrap();
    assert_eq!(adapter.value["limit_mb"], 2048);
    assert!(!adapter.warnings.is_empty());
    store
        .apply_migration("invalid-budget", &plan.fingerprint)
        .unwrap();
    assert_eq!(
        store
            .legacy_artifact("invalid-budget", "config.json")
            .unwrap(),
        original
    );
    assert_eq!(fs::read(legacy.join("config.json")).unwrap(), original);
}

#[test]
fn incomplete_raw_pair_is_removed_without_deleting_remaining_archive_bytes() {
    let (temp, store, _, _) = setup();
    let db = rusqlite::Connection::open(temp.path().join("state.sqlite")).unwrap();
    let reference = itm_core::imdb_cache::RawReference {
        import_id: "historical".into(),
        metadata: "cache/raw-tt1234567.json".into(),
        body: "cache/raw-tt1234567.html.gz".into(),
        fetched_at: chrono::Utc::now().to_rfc3339(),
    };
    db.execute(
        "INSERT INTO preferences VALUES('imdb-raw:tt1234567',?1)",
        [serde_json::to_string(&reference).unwrap()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO legacy_artifacts VALUES('historical',?1,'imdb-cache','kept',?2)",
        rusqlite::params![reference.metadata, b"remaining original bytes".as_slice()],
    )
    .unwrap();
    assert_eq!(store.imdb_cache_status().unwrap().raw_count, 1);
    let after = store.maintain_imdb_cache_automatically().unwrap();
    assert_eq!(after.raw_count, 0);
    assert_eq!(after.removed_count, 1);
    assert_eq!(
        after.archive_bytes,
        b"remaining original bytes".len() as u64
    );
}

fn downloaded_page() -> String {
    format!(
        "<script id='__NEXT_DATA__'>{}</script>",
        serde_json::json!({"props":{"title":{"id":"tt1234567","runtimes":{"edges":[]},"technicalSpecifications":{"cameras":{"items":[{"camera":"Downloaded Camera"}]}}}}})
    )
}
#[test]
fn downloaded_pages_reparse_after_parser_upgrade_without_http_and_keep_the_original_time() {
    let (temp, store, item, mut request) = setup();
    let original = fs::read(&item.path).unwrap();
    store.begin_fetch(request.clone()).unwrap();
    let page = downloaded_page();
    assert!(store.retain_fetch_page("fetch", &page).unwrap());
    store
        .finish_fetch(
            "fetch",
            Ok(itm_core::specs::parse_page("tt1234567", &page).unwrap()),
        )
        .unwrap();
    let db = rusqlite::Connection::open(temp.path().join("state.sqlite")).unwrap();
    let at: String = db
        .query_row("SELECT fetched_at FROM imdb_raw_cache", [], |r| r.get(0))
        .unwrap();
    db.execute("UPDATE imdb_cache SET parser_version=0", [])
        .unwrap();
    drop(store);
    let store = Store::open(&temp.path().join("state.sqlite")).unwrap();
    request.operation_id = "reparse".into();
    let (record, execute) = store.begin_fetch(request).unwrap();
    assert!(!execute);
    assert!(record.cached && record.attempts.is_empty());
    let parsed = record.source.unwrap();
    assert_eq!(parsed.fetched_at, at);
    assert_eq!(parsed.specs["Camera"], ["Downloaded Camera"]);
    assert_eq!(fs::read(&item.path).unwrap(), original);
    let status = store.imdb_cache_status().unwrap();
    assert_eq!(
        (status.parsed_count, status.raw_count, status.archive_bytes),
        (1, 1, 0)
    );
    assert_eq!(
        store
            .maintain_imdb_cache(clear(&store, "clear-downloaded"))
            .unwrap()
            .removed_count,
        2
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM imdb_raw_cache", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
}
#[test]
fn raw_cache_rejects_challenges_mismatched_titles_and_late_cancelled_responses() {
    let (_temp, store, _, request) = setup();
    store.begin_fetch(request).unwrap();
    let page = downloaded_page();
    store.retain_fetch_page("fetch", &page).unwrap();
    for bad in [
        "<div>awswafintegration</div>".into(),
        page.replace("tt1234567", "tt7654321"),
        "x".repeat(8 * 1024 * 1024 + 1),
    ] {
        assert!(store.retain_fetch_page("fetch", &bad).is_err());
        assert_eq!(store.imdb_cache_status().unwrap().raw_count, 1);
    }
    store.cancel_fetch("fetch").unwrap();
    assert!(!store
        .retain_fetch_page("fetch", "cancelled response")
        .unwrap());
    store
        .maintain_imdb_cache(clear(&store, "clear-cancelled"))
        .unwrap();
    assert_eq!(store.imdb_cache_status().unwrap().raw_count, 0);
}
#[test]
fn raw_cache_write_failure_is_atomic_and_does_not_prevent_recording_fetch_success() {
    let (temp, store, _, request) = setup();
    store.begin_fetch(request).unwrap();
    let page = downloaded_page();
    let db = rusqlite::Connection::open(temp.path().join("state.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_raw BEFORE INSERT ON imdb_raw_cache BEGIN SELECT RAISE(ABORT,'raw disk failure'); END;").unwrap();
    assert!(store.retain_fetch_page("fetch", &page).is_err());
    assert_eq!(
        store
            .finish_fetch(
                "fetch",
                Ok(itm_core::specs::parse_page("tt1234567", &page).unwrap())
            )
            .unwrap()
            .phase,
        "completed"
    );
    assert_eq!(store.imdb_cache_status().unwrap().raw_count, 0);
    assert_eq!(store.imdb_cache_status().unwrap().parsed_count, 1);
}
#[test]
fn version_eight_upgrade_adds_raw_storage_without_rewriting_existing_state() {
    let (temp, store, item, request) = setup();
    store.begin_fetch(request).unwrap();
    store.finish_fetch("fetch", Ok(source())).unwrap();
    let db = rusqlite::Connection::open(temp.path().join("state.sqlite")).unwrap();
    let before: String = db
        .query_row("SELECT result FROM operations WHERE id='fetch'", [], |r| {
            r.get(0)
        })
        .unwrap();
    drop(store);
    db.execute_batch("DROP TABLE imdb_raw_cache; PRAGMA user_version=8;")
        .unwrap();
    let store = Store::open(&temp.path().join("state.sqlite")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        9
    );
    assert_eq!(
        db.query_row("SELECT result FROM operations WHERE id='fetch'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        before
    );
    assert_eq!(store.item(&item.id).unwrap().source_hash, item.source_hash);
    assert_eq!(store.imdb_cache_status().unwrap().parsed_count, 1);
}
#[test]
fn downloaded_raw_cache_detects_corruption_and_expires_without_rewriting_archives() {
    let (temp, store, _, mut request) = setup();
    store.begin_fetch(request.clone()).unwrap();
    store
        .retain_fetch_page("fetch", &downloaded_page())
        .unwrap();
    store.cancel_fetch("fetch").unwrap();
    let db = rusqlite::Connection::open(temp.path().join("state.sqlite")).unwrap();
    db.execute("UPDATE imdb_raw_cache SET body=x'00'", [])
        .unwrap();
    request.operation_id = "corrupt".into();
    assert_eq!(
        store.begin_fetch(request.clone()).unwrap_err().code,
        "legacy-raw-compression"
    );
    request.refresh = true;
    assert!(store.begin_fetch(request).unwrap().1);
    store
        .retain_fetch_page("corrupt", &downloaded_page())
        .unwrap();
    store.cancel_fetch("corrupt").unwrap();
    db.execute(
        "UPDATE imdb_raw_cache SET fetched_at='2000-01-01T00:00:00Z'",
        [],
    )
    .unwrap();
    assert_eq!(
        store
            .maintain_imdb_cache_automatically()
            .unwrap()
            .removed_count,
        1
    );
}

#[test]
fn downloaded_unknown_layout_is_retained_for_later_parsers_but_never_claimed_as_specs() {
    let (_temp, store, _, mut request) = setup();
    store.begin_fetch(request.clone()).unwrap();
    let page="<html><head><title>Future IMDb layout</title></head><body>Unavailable to the current parser</body></html>";
    assert!(store.retain_fetch_page("fetch", page).unwrap());
    let error = itm_core::specs::parse_page("tt1234567", page).unwrap_err();
    store.finish_fetch("fetch", Err(error)).unwrap();
    request.operation_id = "unsupported-retry".into();
    let (record, execute) = store.begin_fetch(request).unwrap();
    assert!(record.source.is_none());
    if !execute {
        assert_eq!(record.phase, "failed");
    }
    assert_eq!(store.imdb_cache_status().unwrap().raw_count, 1);
}
