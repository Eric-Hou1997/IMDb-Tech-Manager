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
        fetched_at: "2026-09-11T00:00:00Z".into(),
        parser: "next-data".into(),
    }
}
#[test]
fn fetched_source_requires_preview_then_confirmation_and_undo_restores_first_insert() {
    let (temp, store, item, request) = setup();
    let original = fs::read(&item.path).unwrap();
    let (record, execute) = store.begin_fetch(request.clone()).unwrap();
    assert!(execute);
    assert_eq!(record.phase, "requested");
    assert!(!store.begin_fetch(request.clone()).unwrap().1);
    assert_eq!(
        store.preview_source("write", "fetch").unwrap_err().code,
        "fetch-incomplete"
    );
    store.finish_fetch("fetch", Ok(source())).unwrap();
    assert_eq!(fs::read(&item.path).unwrap(), original);
    let preview = store.preview_source("write", "fetch").unwrap();
    assert_eq!(preview.after_specs["Camera"], vec!["Example"]);
    assert_eq!(fs::read(&item.path).unwrap(), original);
    let journal = temp.path().canonicalize().unwrap().join("journal");
    store
        .apply_specs("write", &preview.after_hash, &journal, || false)
        .unwrap();
    let updated = store.item(&item.id).unwrap();
    assert_eq!(updated.tags[0].value, "External");
    assert_eq!(updated.specs["Camera"], vec!["Example"]);
    assert!(!store.begin_fetch(request).unwrap().1);
    assert_eq!(
        store.preview_source("write", "fetch").unwrap().phase,
        "committed"
    );
    let undo = store.preview_undo("undo", "write", &journal).unwrap();
    store
        .apply_specs("undo", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&item.path).unwrap(), original);
}
#[test]
fn cache_refresh_cancel_and_operation_replay_are_authoritative() {
    let (_temp, store, item, request) = setup();
    let original = fs::read(&item.path).unwrap();
    store.begin_fetch(request.clone()).unwrap();
    store.finish_fetch("fetch", Ok(source())).unwrap();
    let mut cached = request.clone();
    cached.operation_id = "cached".into();
    let (record, execute) = store.begin_fetch(cached).unwrap();
    assert!(!execute);
    assert!(record.cached);
    let mut refresh = request.clone();
    refresh.operation_id = "refresh".into();
    refresh.refresh = true;
    assert!(store.begin_fetch(refresh.clone()).unwrap().1);
    store.cancel_fetch("refresh").unwrap();
    assert_eq!(
        store.finish_fetch("refresh", Ok(source())).unwrap().phase,
        "cancelled"
    );
    assert_eq!(
        store.preview_source("wrong", "refresh").unwrap_err().code,
        "fetch-incomplete"
    );
    refresh.refresh = false;
    assert_eq!(
        store.begin_fetch(refresh).unwrap_err().code,
        "operation-conflict"
    );
    assert_eq!(fs::read(&item.path).unwrap(), original);
}
#[test]
fn restart_records_interruption_and_failure_does_not_replace_good_cache() {
    let (temp, store, item, request) = setup();
    store.begin_fetch(request.clone()).unwrap();
    store.finish_fetch("fetch", Ok(source())).unwrap();
    let mut refresh = request.clone();
    refresh.operation_id = "failure".into();
    refresh.refresh = true;
    store.begin_fetch(refresh).unwrap();
    let failed = store
        .finish_fetch("failure", http_status(403).map(|_| source()))
        .unwrap();
    assert_eq!(failed.error.unwrap().code, "imdb-http-forbidden");
    let mut pending = request.clone();
    pending.operation_id = "pending".into();
    pending.refresh = true;
    store.begin_fetch(pending.clone()).unwrap();
    drop(store);
    let store = Store::open(&temp.path().join("state.sqlite")).unwrap();
    let (record, execute) = store.begin_fetch(pending).unwrap();
    assert!(!execute);
    assert_eq!(record.phase, "interrupted");
    let mut cache = request;
    cache.operation_id = "cached".into();
    let (record, execute) = store.begin_fetch(cache).unwrap();
    assert!(!execute);
    assert!(record.cached);
    assert!(record.source.is_some());
    fs::write(&item.path, "<movie><title>External change</title></movie>").unwrap();
    assert_eq!(
        store.preview_source("conflict", "cached").unwrap_err().code,
        "source-conflict"
    );
}
#[test]
fn each_http_failure_retains_its_category() {
    for (status, code, retry) in [
        (202, "imdb-waf-challenge", true),
        (403, "imdb-http-forbidden", false),
        (404, "imdb-title-not-found", false),
        (429, "imdb-rate-limit", true),
        (503, "imdb-server-error", true),
        (302, "imdb-redirect-rejected", false),
    ] {
        let error = http_status(status).unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.retryable, retry);
    }
}

#[test]
fn confirmed_empty_cache_write_restart_and_undo_preserve_original_tags() {
    let (temp, store, item, request) = setup();
    let before = fs::read(&item.path).unwrap();
    store.begin_fetch(request.clone()).unwrap();
    let mut empty = source();
    empty.status = specs::SourceStatus::Empty;
    empty.specs.clear();
    store.finish_fetch("fetch", Ok(empty)).unwrap();
    let preview = store.preview_source("write", "fetch").unwrap();
    let journal = temp.path().canonicalize().unwrap().join("journal");
    store
        .apply_specs("write", &preview.after_hash, &journal, || false)
        .unwrap();
    assert!(fs::read_to_string(&item.path)
        .unwrap()
        .contains("status=\"empty\""));
    assert_eq!(store.item(&item.id).unwrap().tags, item.tags);
    drop(store);
    let store = Store::open(&temp.path().canonicalize().unwrap().join("state.sqlite")).unwrap();
    let (cached, execute) = store
        .begin_fetch(FetchRequest {
            operation_id: "after-restart".into(),
            expected_hash: store.item(&item.id).unwrap().source_hash,
            ..request
        })
        .unwrap();
    assert!(!execute);
    assert!(cached.cached);
    assert_eq!(cached.source.unwrap().status, specs::SourceStatus::Empty);
    let undo = store.preview_undo("undo", "write", &journal).unwrap();
    store
        .apply_specs("undo", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&item.path).unwrap(), before);
}
