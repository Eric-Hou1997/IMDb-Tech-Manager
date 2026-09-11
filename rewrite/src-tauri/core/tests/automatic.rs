use itm_core::{automatic::*, batch::*, store::Store, *};
use std::{
    cell::Cell,
    fs,
    time::{Duration, SystemTime},
};
fn setup() -> (tempfile::TempDir, Store, i64) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    let now = chrono::Utc::now().timestamp();
    for (name, age, tech) in [
        ("recent-a", 120, ""),
        ("recent-b", 150, ""),
        ("old-a", 2000, ""),
        ("old-b", 3000, ""),
        (
            "ready",
            120,
            "<technicalspecs source=\"IMDb\" status=\"empty\"/>",
        ),
    ] {
        let path = media.join(format!("{name}.nfo"));
        fs::write(&path,format!("<movie><title>{name}</title><uniqueid type=\"imdb\">tt1234567</uniqueid><tag>External</tag>{tech}</movie>")).unwrap();
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs((now - age) as u64);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
    }
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
    (temp, store, now)
}
fn ok() -> Outcome {
    Outcome {
        phase: "committed".into(),
        error: None,
        candidate_hash: None,
    }
}
fn start(store: &Store, now: i64) {
    store
        .set_automatic("start", Settings::default(), true, now)
        .unwrap();
}
#[test]
fn recent_priority_one_backfill_and_ready_protection_match_legacy_selection() {
    let (_temp, store, now) = setup();
    assert!(store.automatic_tick(now).unwrap().is_empty());
    start(&store, now);
    let tasks = store.automatic_tick(now).unwrap();
    assert_eq!(tasks.len(), 2);
    let batch = tasks[0].batch.as_ref().unwrap();
    assert!(tasks[0].automatic && batch.approved);
    assert_eq!(batch.engine, BatchEngine::Specs);
    let titles: Vec<_> = tasks
        .iter()
        .flat_map(|t| t.batch.as_ref().unwrap().items.iter())
        .map(|i| i.item.title.as_str())
        .collect();
    assert_eq!(titles.iter().filter(|s| s.starts_with("recent")).count(), 2);
    assert_eq!(titles.iter().filter(|s| s.starts_with("old")).count(), 1);
    assert!(!titles.contains(&"ready"));
    while store
        .run_batch_next(|| false, |_| {}, |_, _| Ok(ok()))
        .unwrap()
        .is_some()
    {}
    assert!(store.automatic_tick(now + 10).unwrap().is_empty());
    assert_eq!(store.automatic_status().unwrap().next_due, now + 70);
    assert!(store.automatic_tick(now + 69).unwrap().is_empty());
    assert!(!store.automatic_tick(now + 70).unwrap().is_empty());
}
#[test]
fn stop_is_file_boundary_and_does_not_change_startup_preference() {
    let (_temp, store, now) = setup();
    start(&store, now);
    let task = store.automatic_tick(now).unwrap().remove(0);
    let count = Cell::new(0);
    let result = store
        .run_batch_next(
            || false,
            |_| {},
            |_, _| {
                count.set(count.get() + 1);
                store
                    .set_automatic(
                        "stop",
                        Settings {
                            on_app_start: true,
                            ..Default::default()
                        },
                        false,
                        now,
                    )
                    .unwrap();
                assert_eq!(store.task(&task.id).unwrap().state, TaskState::Running);
                assert!(!store.automatic_status().unwrap().task_ids.is_empty());
                Ok(ok())
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(count.get(), 1);
    assert_eq!(result.state, TaskState::Cancelled);
    let status = store.automatic_status().unwrap();
    assert!(!status.enabled && status.settings.on_app_start && status.task_ids.is_empty());
    assert!(store.automatic_tick(now + 3600).unwrap().is_empty());
    assert!(store.automatic_startup(now + 3601).unwrap().enabled);
}
#[test]
fn manual_task_preempts_automatic_between_files_then_resumes_original_ids() {
    let (_temp, store, now) = setup();
    start(&store, now);
    let auto = store.automatic_tick(now).unwrap().remove(0);
    let count = Cell::new(0);
    let yielded = store
        .run_batch_next(
            || false,
            |_| {},
            |_, _| {
                count.set(count.get() + 1);
                store
                    .submit(ScanRequest {
                        operation_id: "manual-scan".into(),
                        space: Space::Movie,
                        root_ids: vec!["r".into()],
                    })
                    .unwrap();
                Ok(ok())
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(count.get(), 1);
    assert_eq!(yielded.state, TaskState::Requested);
    assert!(store
        .run_batch_next(|| false, |_| {}, |_, _| panic!("manual priority bypassed"))
        .unwrap()
        .is_none());
    store.run_next(|| false, |_| {}).unwrap();
    store
        .run_batch_next(
            || false,
            |_| {},
            |_, row| {
                assert_ne!(
                    row.request_id,
                    auto.batch.as_ref().unwrap().items[0].request_id
                );
                Ok(ok())
            },
        )
        .unwrap();
    assert_eq!(store.task(&auto.id).unwrap().processed, 2);
}
#[test]
fn automatic_specs_use_existing_safe_write_and_preserve_external_tags() {
    let (temp, store, now) = setup();
    start(&store, now);
    let task = store.automatic_tick(now).unwrap().remove(0);
    let row = task.batch.as_ref().unwrap().items[0].clone();
    let before = fs::read(&row.item.path).unwrap();
    let journal = temp.path().canonicalize().unwrap().join("journal");
    store
        .run_batch_next(
            || false,
            |_| {},
            |task, row| {
                store.begin_batch_fetch(
                    acquisition::FetchRequest {
                        operation_id: row.request_id.clone(),
                        item_id: row.item.id.clone(),
                        expected_hash: row.item.source_hash.clone(),
                        refresh: false,
                    },
                    &task.id,
                )?;
                store.finish_fetch(
                    &row.request_id,
                    Ok(specs::SourceSpecs {
                        status: Default::default(),
                        imdb: row.item.imdb.clone(),
                        specs: Specs::from([("Camera".into(), vec!["Arri Alexa".into()])]),
                        fetched_at: "2026-09-11T00:00:00Z".into(),
                        parser: "test".into(),
                    }),
                )?;
                let p = store.preview_source(&row.write_id, &row.request_id)?;
                store.apply_specs(&row.write_id, &p.after_hash, &journal, || false)?;
                Ok(ok())
            },
        )
        .unwrap();
    let current = store.item(&row.item.id).unwrap();
    assert_eq!(current.tags, row.item.tags);
    assert_eq!(current.spec_status, "ready");
    let undo = store.preview_undo("undo", &row.write_id, &journal).unwrap();
    store
        .apply_specs("undo", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&row.item.path).unwrap(), before);
}
#[test]
fn failure_cooldown_only_applies_to_legacy_backfill_not_recent_retry_window() {
    let (_temp, store, now) = setup();
    let item = store.all_items().unwrap().remove(0);
    store
        .begin_fetch(acquisition::FetchRequest {
            operation_id: "failed".into(),
            item_id: item.id,
            expected_hash: item.source_hash,
            refresh: false,
        })
        .unwrap();
    store
        .finish_fetch(
            "failed",
            Err(AppError::new("imdb-timeout", "fixture timeout")),
        )
        .unwrap();
    start(&store, now);
    let tasks = store.automatic_tick(now).unwrap();
    assert_eq!(tasks[0].batch.as_ref().unwrap().total, 2);
    store
        .run_batch_next(|| false, |_| {}, |_, _| Ok(ok()))
        .unwrap();
    store.automatic_tick(now + 3600).unwrap();
    let next = store.automatic_tick(now + 3660).unwrap();
    assert_eq!(next[0].batch.as_ref().unwrap().total, 1);
}
#[test]
fn disabled_restart_cancels_automatic_work_without_resurrecting_it() {
    let (temp, store, now) = setup();
    start(&store, now);
    let task = store.automatic_tick(now).unwrap().remove(0);
    store
        .run_batch_next(|| true, |_| {}, |_, _| panic!("stopped worker wrote"))
        .unwrap();
    assert_eq!(store.task(&task.id).unwrap().state, TaskState::Interrupted);
    drop(store);
    let store = Store::open(&temp.path().canonicalize().unwrap().join("state.sqlite")).unwrap();
    assert!(!store.automatic_startup(now + 1).unwrap().enabled);
    assert_eq!(store.task(&task.id).unwrap().state, TaskState::Cancelled);
    assert!(store.automatic_tick(now + 1000).unwrap().is_empty());
    assert_eq!(
        store
            .set_automatic(
                "bad",
                Settings {
                    interval_seconds: 29,
                    on_app_start: false
                },
                true,
                now
            )
            .unwrap_err()
            .code,
        "invalid-interval"
    );
}
#[test]
fn old_preferences_import_without_starting_work_or_changing_login_ownership() {
    let (temp, store, now) = setup();
    let source = temp.path().canonicalize().unwrap().join("old");
    fs::create_dir(&source).unwrap();
    let original=br#"{"interval_seconds":15,"auto_start":true,"auto_start_configured":true,"app_auto_start":true,"app_auto_start_configured":true}"#;
    fs::write(source.join("settings.json"), original).unwrap();
    let before = store.lifecycle_settings().unwrap();
    let plan = store
        .prepare_migration("import", &source, "itm-manager")
        .unwrap();
    assert_eq!(plan.adapters.len(), 1);
    assert_eq!(plan.adapters[0].target, "automatic");
    store.apply_migration("import", &plan.fingerprint).unwrap();
    let status = store.automatic_status().unwrap();
    assert!(!status.enabled && status.settings.on_app_start);
    assert_eq!(status.settings.interval_seconds, 60);
    assert_eq!(store.lifecycle_settings().unwrap(), before);
    assert_eq!(
        store.legacy_artifact("import", "settings.json").unwrap(),
        original
    );
    assert!(store.automatic_tick(now).unwrap().is_empty());
    let explicit = serde_json::json!({"auto_start_configured":true,"auto_start":true,"auto_mode_on_app_start_configured":true,"auto_mode_on_app_start":false});
    assert!(!legacy_settings(&explicit).unwrap().unwrap().on_app_start);
    assert!(
        !legacy_settings(&serde_json::json!({"auto_mode_on_app_start":true}))
            .unwrap()
            .unwrap()
            .on_app_start
    );
}
#[test]
fn cold_index_is_read_only_and_owned_by_automatic_stop() {
    let (temp, original, now) = setup();
    let config = Configuration {
        revision: 0,
        ..original.configuration().unwrap()
    };
    let files: Vec<_> = original
        .all_items()
        .unwrap()
        .into_iter()
        .map(|i| {
            (
                i.path.clone(),
                fs::read(&i.path).unwrap(),
                fs::metadata(&i.path).unwrap().modified().unwrap(),
            )
        })
        .collect();
    let store = Store::open(&temp.path().canonicalize().unwrap().join("cold.sqlite")).unwrap();
    store.configure("roots", config).unwrap();
    start(&store, now);
    let scan = store.automatic_tick(now).unwrap().remove(0);
    assert!(scan.automatic && scan.batch.is_none());
    store.run_next(|| false, |_| {}).unwrap();
    for (path, bytes, mtime) in files {
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), mtime);
    }
    assert!(!store.automatic_tick(now + 60).unwrap().is_empty());
    store
        .set_automatic("stop", Settings::default(), false, now + 60)
        .unwrap();
    assert!(store.automatic_status().unwrap().task_ids.is_empty());
    assert!(store
        .run_batch_next(
            || false,
            |_| {},
            |_, _| panic!("stopped automatic work ran")
        )
        .unwrap()
        .is_none());
}
