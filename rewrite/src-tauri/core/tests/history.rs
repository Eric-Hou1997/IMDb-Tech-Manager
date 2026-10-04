use itm_core::store::Store;
use std::fs;
fn setup(files: &[(&str, Vec<u8>)]) -> (tempfile::TempDir, Store) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let legacy = root.join("legacy");
    fs::create_dir(&legacy).unwrap();
    for (name, bytes) in files {
        let path = legacy.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let plan = store
        .prepare_migration("history-import", &legacy, "itm-manager")
        .unwrap();
    store
        .apply_migration("history-import", &plan.fingerprint)
        .unwrap();
    (temp, store)
}
#[test]
fn imported_history_preserves_original_language_fields_and_never_creates_executable_tasks() {
    let value = serde_json::json!([{"job_id":"historical-active", "running":true,"action":"ai-generate","language":"ja","language_pack_revision":3,"message":"過去の記録", "started_at":"2026-08-01T00:00:00Z", "extra":{"keep":"原字段"}}]);
    let raw = serde_json::to_vec_pretty(&value).unwrap();
    let (temp, store) = setup(&[
        ("task-history.json", raw.clone()),
        ("logs/job.log", b"\xef\xbb\xbfOriginal\r\n".to_vec()),
    ]);
    let archives = store.history_archives(0, 1).unwrap();
    assert_eq!(archives.total, 2);
    assert_eq!(archives.items.len(), 1);
    let page = store
        .history_page("history-import", "task-history.json", 0)
        .unwrap();
    assert_eq!(page.unit, "records");
    assert_eq!(page.tasks[0], value[0]);
    assert!(store.tasks().unwrap().is_empty());
    let jobs = store.job_history().unwrap();
    assert_eq!(jobs[0].language, "ja");
    assert_eq!(jobs[0].language_pack_revision, 3);
    assert_eq!(jobs[0].message, "過去の記録");
    assert!(jobs[0].running);
    assert_eq!(
        store
            .legacy_artifact("history-import", "task-history.json")
            .unwrap(),
        raw
    );
    drop(store);
    let store = Store::open(&temp.path().join("state.sqlite")).unwrap();
    assert_eq!(
        store
            .history_page("history-import", "task-history.json", 0)
            .unwrap()
            .tasks[0],
        value[0]
    );
    assert!(store.tasks().unwrap().is_empty());
}

#[test]
fn current_log_is_persisted_with_state_language_scope_and_errors_without_touching_nfos() {
    use itm_core::*;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    let raw = b"\xef\xbb\xbf<movie><title>\xe4\xb8\xad\xe6\x96\x87</title></movie>\r\n";
    fs::write(media.join("good.nfo"), raw).unwrap();
    fs::write(media.join("bad.nfo"), b"<movie>").unwrap();
    let database = root.join("state.sqlite");
    let store = Store::open(&database).unwrap();
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
            operation_id: "one".into(),
            space: Space::Movie,
            root_ids: vec!["r".into()],
        })
        .unwrap();
    let paused = std::cell::Cell::new(false);
    store
        .run_next(
            || false,
            |task| {
                if task.processed == 1 && !paused.replace(true) {
                    store
                        .control(TaskControl {
                            operation_id: "pause".into(),
                            task_id: "one".into(),
                            state: TaskState::Paused,
                        })
                        .unwrap();
                }
            },
        )
        .unwrap();
    assert_eq!(store.task("one").unwrap().state, TaskState::Paused);
    let paused_job = store.task_job("one").unwrap();
    assert!(paused_job.log.contains("已暂停"));
    assert!(paused_job.ended_at.is_empty());
    let mut config = store.configuration().unwrap();
    config.locale = Locale::English;
    store.configure("language", config).unwrap();
    store
        .control(TaskControl {
            operation_id: "resume".into(),
            task_id: "one".into(),
            state: TaskState::Requested,
        })
        .unwrap();
    store.run_next(|| false, |_| {}).unwrap();
    let job = store.task_job("one").unwrap();
    assert_eq!(job.language, "zh-CN");
    assert_eq!(job.exit_code, 1);
    assert!(!job.ended_at.is_empty());
    assert!(job.log.contains("bad.nfo"));
    assert!(job.log.contains("invalid-xml"));
    assert!(job.log.contains("失败"));
    assert!(store.tasks().unwrap()[0]
        .journal
        .as_ref()
        .unwrap()
        .text
        .is_empty());
    drop(store);
    let store = Store::open(&database).unwrap();
    assert_eq!(store.task_job("one").unwrap().log, job.log);
    assert_eq!(store.job_history().unwrap()[0].ended_at, job.ended_at);
    assert_eq!(fs::read(media.join("good.nfo")).unwrap(), raw);
    assert_eq!(fs::read(media.join("bad.nfo")).unwrap(), b"<movie>");
}

#[test]
fn log_tail_is_bounded_utf8_and_old_tasks_keep_unknown_dates_unknown() {
    use itm_core::*;
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("state.sqlite");
    let store = Store::open(&database).unwrap();
    let value = serde_json::json!({"id":"old","automatic":false,"batch":null,"state":"running","locale":"en-US","space":"movie","roots":[],"processed":0,"errors":0,"current_path":null,"failure":null});
    let db = rusqlite::Connection::open(&database).unwrap();
    db.execute(
        "INSERT INTO tasks VALUES('old','old',?1)",
        [value.to_string()],
    )
    .unwrap();
    drop(store);
    let store = Store::open(&database).unwrap();
    let job = store.task_job("old").unwrap();
    assert!(job.started_at.is_empty());
    assert!(job.ended_at.is_empty());
    assert!(job.log.contains("Interrupted"));
    store
        .control(TaskControl {
            operation_id: "cancel".into(),
            task_id: "old".into(),
            state: TaskState::Cancelled,
        })
        .unwrap();
    let mut task = store.task("old").unwrap();
    task.journal.as_mut().unwrap().text = "中文📝".repeat(6000);
    db.execute(
        "UPDATE tasks SET body=?1 WHERE id='old'",
        [serde_json::to_string(&task).unwrap()],
    )
    .unwrap();
    // The normal state writer bounds the UTF-8 tail on its next transition.
    let mut task = store.task("old").unwrap();
    task.state = TaskState::Running;
    db.execute(
        "UPDATE tasks SET body=?1 WHERE id='old'",
        [serde_json::to_string(&task).unwrap()],
    )
    .unwrap();
    store
        .control(TaskControl {
            operation_id: "pause-long".into(),
            task_id: "old".into(),
            state: TaskState::Paused,
        })
        .unwrap();
    let log = store.task_job("old").unwrap().log;
    assert!(log.len() <= 40_000);
    assert!(log.ends_with('\n'));
    assert!(log.contains("Paused"));
}
#[test]
fn log_pagination_preserves_bom_newlines_and_utf8_without_gaps_or_repeated_bytes() {
    let text = format!("\u{feff}{}\r\n尾部", "中文日志📝\r\n".repeat(16000));
    let (_temp, store) = setup(&[("logs/job.log", text.as_bytes().to_vec())]);
    let mut offset = 0;
    let mut joined = String::new();
    loop {
        let page = store
            .history_page("history-import", "logs/job.log", offset)
            .unwrap();
        assert!(page.text.len() <= 65536);
        joined.push_str(&page.text);
        match page.next_offset {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(joined, text);
    assert_eq!(
        store
            .history_page("history-import", "logs/job.log", 1)
            .unwrap_err()
            .code,
        "history-offset"
    );
}
#[test]
fn task_record_pagination_has_stable_order_and_oversized_or_unknown_json_uses_bounded_text() {
    let tasks = (0..123)
        .map(|i| serde_json::json!({"message":format!("record-{i}")}))
        .collect::<Vec<_>>();
    let (_temp, store) = setup(&[("task-history.json", serde_json::to_vec(&tasks).unwrap())]);
    let mut joined = vec![];
    let mut offset = 0;
    loop {
        let page = store
            .history_page("history-import", "task-history.json", offset)
            .unwrap();
        assert_eq!(page.unit, "records");
        joined.extend(page.tasks);
        match page.next_offset {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(joined, tasks);
    for raw in [
        b"{interrupted legacy JSON".to_vec(),
        serde_json::to_vec(&serde_json::json!([{"log":"x".repeat(70000)}])).unwrap(),
    ] {
        let (_temp, store) = setup(&[("task-history.json", raw.clone())]);
        let page = store
            .history_page("history-import", "task-history.json", 0)
            .unwrap();
        assert_eq!(page.unit, "bytes");
        assert!(page.text.len() <= 65536);
        assert_eq!(page.text.as_bytes(), &raw[..page.text.len()]);
    }
}
#[test]
fn original_history_summaries_keep_chronological_order_across_old_local_and_new_utc_timestamps() {
    let old = serde_json::json!([
        {"job_id":"later-local-text","ended_at":"2026-10-03T15:00:00+08:00","message":"Earlier"},
        {"job_id":"later-in-time","ended_at":"2026-10-03T08:00:00Z","message":"Later"}
    ]);
    let (_tmp, store) = setup(&[("task-history.json", serde_json::to_vec(&old).unwrap())]);
    let history = store.job_history().unwrap();
    assert_eq!(history[0].job_id, "later-in-time");
    assert_eq!(history[1].job_id, "later-local-text");
    assert_eq!(
        store
            .legacy_artifact("history-import", "task-history.json")
            .unwrap(),
        serde_json::to_vec(&old).unwrap()
    );
}
#[test]
fn history_view_never_exposes_other_archives_and_rejects_changed_or_non_utf8_bytes() {
    let (temp, store) = setup(&[
        ("logs/job.log", b"original".to_vec()),
        ("logs/binary.log", vec![255]),
        ("config.json", b"{}".to_vec()),
        ("cache/tt1234567.json", b"{}".to_vec()),
        ("backup/nfo.json", b"{}".to_vec()),
    ]);
    assert_eq!(store.history_archives(0, 100).unwrap().total, 2);
    for path in [
        "config.json",
        "cache/tt1234567.json",
        "backup/nfo.json",
        "../logs/job.log",
    ] {
        assert_eq!(
            store
                .history_page("history-import", path, 0)
                .unwrap_err()
                .code,
            "history-not-found"
        );
    }
    assert_eq!(
        store
            .history_page("history-import", "logs/binary.log", 0)
            .unwrap_err()
            .code,
        "history-encoding"
    );
    assert!(store.history_archives(0, 101).is_err());
    let db = rusqlite::Connection::open(temp.path().join("state.sqlite")).unwrap();
    db.execute(
        "UPDATE legacy_artifacts SET body=?1 WHERE path='logs/job.log'",
        [b"changed".as_slice()],
    )
    .unwrap();
    assert_eq!(
        store
            .history_page("history-import", "logs/job.log", 0)
            .unwrap_err()
            .code,
        "history-integrity"
    );
}
