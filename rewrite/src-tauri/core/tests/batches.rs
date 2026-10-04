use itm_core::{batch::*, store::Store, *};
use std::{cell::Cell, fs};
fn setup() -> (tempfile::TempDir, Store, Vec<MediaItem>) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().canonicalize().unwrap();
    let media = path.join("media");
    fs::create_dir(&media).unwrap();
    for name in ["a", "b"] {
        fs::write(media.join(format!("{name}.nfo")),format!(r#"<movie><title>{name}</title><uniqueid type="imdb">tt1234567</uniqueid><tag>External</tag><technicalspecs source="IMDb" imdbid="tt1234567"><section name="Camera"><item>Arri Alexa 65</item></section></technicalspecs></movie>"#)).unwrap();
    }
    let store = Store::open(&path.join("state.sqlite")).unwrap();
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
    let items = store.all_items().unwrap();
    (tmp, store, items)
}
fn request(items: &[MediaItem], id: &str) -> BatchRequest {
    BatchRequest {
        operation_id: id.into(),
        space: Space::Movie,
        item_ids: items.iter().map(|i| i.id.clone()).collect(),
        root_ids: vec![],
        engine: BatchEngine::Rules,
        mode: BatchMode::Generate,
        retry_failed: false,
    }
}
fn control(store: &Store, id: &str, state: TaskState) {
    store
        .control(TaskControl {
            operation_id: format!("{id}-{state:?}"),
            task_id: id.into(),
            state,
        })
        .unwrap();
}
fn success() -> Outcome {
    Outcome {
        phase: "committed".into(),
        error: None,
        candidate_hash: None,
    }
}
#[test]
fn original_selected_preflight_reads_fresh_bytes_without_creating_or_expanding_a_task() {
    let (_tmp, store, items) = setup();
    let item = &items[0];
    let bytes = fs::read_to_string(&item.path)
        .unwrap()
        .replace("<title>a</title>", "<title>Changed by TMM</title>")
        .replace("<title>b</title>", "<title>Changed by TMM</title>");
    fs::write(&item.path, &bytes).unwrap();
    let modified = fs::metadata(&item.path).unwrap().modified().unwrap();
    let tasks = store.tasks().unwrap();
    let scope = store
        .preflight_items(Space::Movie, vec![item.id.clone(), item.id.clone()])
        .unwrap();
    assert_eq!(scope.len(), 1);
    assert_eq!(scope[0].title, "Changed by TMM");
    assert_ne!(scope[0].source_hash, item.source_hash);
    assert_eq!(scope[0].tags[0].ownership, Ownership::External);
    assert_eq!(store.tasks().unwrap(), tasks);
    assert_eq!(fs::read(&item.path).unwrap(), bytes.as_bytes());
    assert_eq!(
        fs::metadata(&item.path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(
        store
            .preflight_items(Space::Tv, vec![item.id.clone()])
            .unwrap_err()
            .code,
        "invalid-scope"
    );
    assert_eq!(
        store
            .preflight_items(Space::Movie, vec![])
            .unwrap_err()
            .code,
        "invalid-scope"
    );
    assert_eq!(
        store
            .preflight_items(Space::Movie, vec!["missing".into()])
            .unwrap_err()
            .code,
        "invalid-scope"
    );
    fs::remove_file(&item.path).unwrap();
    assert!(store
        .preflight_items(Space::Movie, vec![item.id.clone()])
        .is_err());
    assert_eq!(store.tasks().unwrap(), tasks);
}

#[test]
fn a_confirmed_new_scope_supersedes_stopped_scans_and_same_engine_without_erasing_results() {
    let (_tmp, store, items) = setup();
    let old = store.plan_batch(request(&items, "old-plan")).unwrap();
    store
        .submit(ScanRequest {
            operation_id: "old-index".into(),
            space: Space::Movie,
            root_ids: vec!["r".into()],
        })
        .unwrap();
    store
        .control(TaskControl {
            operation_id: "pause-index".into(),
            task_id: "old-index".into(),
            state: TaskState::Paused,
        })
        .unwrap();
    let new = store.plan_batch(request(&items, "new-plan")).unwrap();
    assert_eq!(store.task("old-plan").unwrap(), old);
    assert_eq!(store.task("old-index").unwrap().state, TaskState::Paused);
    store
        .approve_batch_scope(&new.id, &new.batch.unwrap().plan_hash)
        .unwrap();
    assert_eq!(store.task("old-plan").unwrap().state, TaskState::Cancelled);
    assert_eq!(
        store.task("old-plan").unwrap().batch.unwrap().items.len(),
        items.len()
    );
    assert_eq!(store.task("old-index").unwrap().state, TaskState::Cancelled);
    assert!(store
        .task_job("old-plan")
        .unwrap()
        .log
        .contains("原记录与已完成结果保留"));
    let new_task = store.task("new-plan").unwrap();
    store
        .run_batch_next(
            || false,
            |_| {},
            |_, _| {
                Ok(Outcome {
                    phase: "skipped-current".into(),
                    error: None,
                    candidate_hash: None,
                })
            },
        )
        .unwrap();
    assert_eq!(
        store.task(&new_task.id).unwrap().state,
        TaskState::Completed
    );
    assert_eq!(store.all_items().unwrap().len(), items.len());
}

#[test]
fn a_new_read_only_scan_preserves_the_paused_ai_queue_and_pending_write_blocks_superseding() {
    let (tmp, store, items) = setup();
    let mut settings = itm_core::ai::job::Settings {
        enabled: true,
        ..Default::default()
    };
    settings.config.base_url = "https://provider.example/v1".into();
    settings.config.model = "fixture".into();
    store.save_ai_settings("ai-settings", settings).unwrap();
    let mut req = request(&items, "ai-paused");
    req.engine = BatchEngine::Ai;
    let ai = store.plan_batch(req).unwrap();
    store
        .submit(ScanRequest {
            operation_id: "new-index".into(),
            space: Space::Movie,
            root_ids: vec!["r".into()],
        })
        .unwrap();
    assert_eq!(store.task("ai-paused").unwrap(), ai);
    store
        .control(TaskControl {
            operation_id: "pause".into(),
            task_id: "new-index".into(),
            state: TaskState::Paused,
        })
        .unwrap();
    let mut pending = store
        .preview_rules("pending-write", &items[0].id, &items[0].source_hash)
        .unwrap();
    pending.phase = "writing".into();
    let db = rusqlite::Connection::open(tmp.path().join("state.sqlite")).unwrap();
    db.execute(
        "UPDATE operations SET result=?1 WHERE id='pending-write'",
        [serde_json::to_string(&OperationResult::Write(pending)).unwrap()],
    )
    .unwrap();
    let new = store.plan_batch(request(&items, "guarded")).unwrap();
    assert_eq!(
        store
            .approve_batch_scope(&new.id, &new.batch.unwrap().plan_hash)
            .unwrap_err()
            .code,
        "recovery-required"
    );
    assert_eq!(store.task("new-index").unwrap().state, TaskState::Paused);
    assert!(!store.task("guarded").unwrap().batch.unwrap().approved);
    assert_eq!(store.task("ai-paused").unwrap(), ai);
}
#[test]
fn scope_approval_pause_boundary_resume_and_cancel_preserve_completed_items() {
    let (_tmp, store, items) = setup();
    let plan = store.plan_batch(request(&items, "batch")).unwrap();
    assert!(store
        .run_batch_next(|| false, |_| {}, |_, _| panic!("unapproved scope ran"))
        .unwrap()
        .is_none());
    assert_eq!(
        store
            .approve_batch_scope("batch", "wrong")
            .unwrap_err()
            .code,
        "review-mismatch"
    );
    assert_eq!(
        store
            .control(TaskControl {
                operation_id: "bypass".into(),
                task_id: "batch".into(),
                state: TaskState::Requested
            })
            .unwrap_err()
            .code,
        "scope-unapproved"
    );
    store
        .approve_batch_scope("batch", &plan.batch.unwrap().plan_hash)
        .unwrap();
    let first = Cell::new(0);
    let t = store
        .run_batch_next(
            || false,
            |_| {},
            |_, _| {
                first.set(first.get() + 1);
                control(&store, "batch", TaskState::Paused);
                let current = store.task("batch").unwrap();
                assert_eq!(current.state, TaskState::Running);
                assert!(current.batch.unwrap().pause_requested);
                Ok(success())
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(first.get(), 1);
    assert_eq!(t.state, TaskState::Paused);
    assert_eq!(t.processed, 1);
    control(&store, "batch", TaskState::Requested);
    let t = store
        .run_batch_next(
            || false,
            |_| {},
            |_, row| {
                assert_eq!(row.item.id, items[1].id);
                control(&store, "batch", TaskState::Cancelled);
                Ok(success())
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(t.state, TaskState::Cancelled);
    assert_eq!(t.processed, 2);
    assert_eq!(
        store.plan_batch(request(&items, "batch")).unwrap().state,
        TaskState::Cancelled
    );
}
#[test]
fn full_scope_is_explicit_and_movie_tv_cannot_mix() {
    let (_tmp, store, items) = setup();
    let mut r = request(&[], "all");
    assert_eq!(
        store.plan_batch(r.clone()).unwrap_err().code,
        "invalid-scope"
    );
    r.root_ids = vec!["r".into()];
    let t = store.plan_batch(r).unwrap();
    assert_eq!(t.batch.unwrap().items.len(), 2);
    let mut r = request(&items, "mixed");
    r.space = Space::Tv;
    assert_eq!(store.plan_batch(r).unwrap_err().code, "invalid-scope");
    let mut r = request(&items, "duplicate");
    r.item_ids.push(items[0].id.clone());
    assert_eq!(store.plan_batch(r).unwrap_err().code, "invalid-scope");
}
#[test]
fn preview_approval_reuses_exact_candidate_and_external_edit_blocks_write() {
    let (tmp, store, items) = setup();
    let mut req = request(&items, "preview");
    req.mode = BatchMode::Preview;
    let t = store.plan_batch(req).unwrap();
    store
        .approve_batch_scope("preview", &t.batch.unwrap().plan_hash)
        .unwrap();
    let t = store
        .run_batch_next(
            || false,
            |_| {},
            |_, row| {
                let p = store.preview_rules(&row.write_id, &row.item.id, &row.item.source_hash)?;
                Ok(Outcome {
                    phase: "review-ready".into(),
                    error: None,
                    candidate_hash: Some(p.after_hash),
                })
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(t.state, TaskState::Completed);
    let batch = t.batch.unwrap();
    let a = &batch.items[0];
    let b = &batch.items[1];
    assert_eq!(hash(&fs::read(&a.item.path).unwrap()), a.item.source_hash);
    let journal = tmp.path().canonicalize().unwrap().join("journal");
    let result = store
        .apply_batch_item(
            "preview",
            &a.write_id,
            a.candidate_hash.as_ref().unwrap(),
            &journal,
            || false,
        )
        .unwrap();
    assert_eq!(result.phase, "committed");
    assert_eq!(
        store.item(&a.item.id).unwrap().tags[0].ownership,
        Ownership::External
    );
    let undo = store.preview_undo("undo", &a.write_id, &journal).unwrap();
    store
        .apply_specs("undo", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(hash(&fs::read(&a.item.path).unwrap()), a.item.source_hash);
    fs::write(
        &b.item.path,
        b"<movie><title>external changed</title></movie>",
    )
    .unwrap();
    assert_eq!(
        store
            .apply_batch_item(
                "preview",
                &b.write_id,
                b.candidate_hash.as_ref().unwrap(),
                &journal,
                || false
            )
            .unwrap_err()
            .code,
        "source-conflict"
    );
}

#[test]
fn reviewed_adoption_is_an_owned_job_that_reuses_writes_preserves_preview_history_and_continues_after_conflict(
) {
    for engine in [BatchEngine::Rules, BatchEngine::Ai] {
        let (tmp, store, items) = setup();
        if engine == BatchEngine::Ai {
            let mut settings = itm_core::ai::job::Settings {
                enabled: true,
                ..Default::default()
            };
            settings.config.base_url = "https://provider.example/v1".into();
            settings.config.model = "fixture".into();
            store.save_ai_settings("settings", settings).unwrap();
        }
        let mut req = request(&items, "review");
        req.mode = BatchMode::Preview;
        req.engine = engine.clone();
        let planned = store.plan_batch(req).unwrap();
        store
            .approve_batch_scope(&planned.id, &planned.batch.unwrap().plan_hash)
            .unwrap();
        let preview = store
            .run_batch_next(
                || false,
                |_| {},
                |_, row| {
                    let p =
                        store.preview_rules(&row.write_id, &row.item.id, &row.item.source_hash)?;
                    Ok(Outcome {
                        phase: "review-ready".into(),
                        candidate_hash: Some(p.after_hash),
                        error: None,
                    })
                },
            )
            .unwrap()
            .unwrap();
        let rows = &preview.batch.as_ref().unwrap().items;
        let adoption = PreviewAdoption {
            operation_id: "adopt".into(),
            task_id: preview.id.clone(),
            items: rows
                .iter()
                .map(|r| ReviewedCandidate {
                    write_id: r.write_id.clone(),
                    reviewed_hash: r.candidate_hash.clone().unwrap(),
                })
                .collect(),
        };
        let planned = store.adopt_preview(adoption.clone()).unwrap();
        assert_eq!(
            planned.batch.as_ref().unwrap().mode,
            BatchMode::AdoptPreview
        );
        assert_eq!(store.adopt_preview(adoption.clone()).unwrap(), planned);
        let mut changed = adoption.clone();
        changed.items[0].reviewed_hash = "other".into();
        assert_eq!(
            store.adopt_preview(changed).unwrap_err().code,
            "operation-conflict"
        );
        let external = b"<movie><title>Changed externally</title><tag>Protected</tag></movie>";
        fs::write(&rows[0].item.path, external).unwrap();
        let journal = tmp.path().canonicalize().unwrap().join("journal");
        let mut applied = 0;
        let completed = store
            .run_batch_next(
                || false,
                |_| {},
                |task, row| {
                    let candidate = store.adoption_candidate(task, row)?;
                    assert_eq!(candidate.after_hash, row.candidate_hash.as_deref().unwrap());
                    let receipt = store.apply_specs(
                        &row.write_id,
                        &candidate.after_hash,
                        &journal,
                        || false,
                    )?;
                    applied += 1;
                    Ok(Outcome {
                        phase: receipt.phase,
                        error: receipt.error,
                        candidate_hash: Some(receipt.after_hash),
                    })
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(completed.state, TaskState::Failed);
        assert_eq!(completed.processed, 2);
        assert_eq!(completed.errors, 1);
        assert_eq!(applied, 1);
        assert_eq!(fs::read(&rows[0].item.path).unwrap(), external);
        assert_eq!(
            store.item(&rows[1].item.id).unwrap().tags[0].ownership,
            Ownership::External
        );
        assert_eq!(store.task("review").unwrap(), preview);
        assert!(store.ai_history(None).unwrap().is_empty());
        assert_eq!(
            store.task_job("adopt").unwrap().action,
            if engine == BatchEngine::Ai {
                "ai-approve-selected"
            } else {
                "local-approve-selected"
            }
        );
        assert!(store
            .task_job("adopt")
            .unwrap()
            .log
            .contains("source-conflict"));
        assert_eq!(store.job_history().unwrap().len(), 3);
        assert_eq!(
            store.adopt_preview(adoption).unwrap().state,
            TaskState::Failed
        );
    }
}

#[test]
fn adoption_rejects_unreviewed_hashes_and_cannot_be_requested_as_new_generation() {
    let (_tmp, store, items) = setup();
    let mut req = request(&items, "forged");
    req.mode = BatchMode::AdoptPreview;
    assert_eq!(store.plan_batch(req).unwrap_err().code, "review-mismatch");
    assert!(store
        .adopt_preview(PreviewAdoption {
            operation_id: "empty".into(),
            task_id: "missing".into(),
            items: vec![]
        })
        .is_err());
    assert_eq!(store.tasks().unwrap().len(), 1);
    assert!(store.write_history().unwrap().is_empty());
}
#[test]
fn crash_after_write_replays_receipt_without_rewriting_or_duplicating_backup() {
    let (tmp, store, items) = setup();
    let req = request(&items, "recover");
    let t = store.plan_batch(req).unwrap();
    store
        .approve_batch_scope("recover", &t.batch.unwrap().plan_hash)
        .unwrap();
    let journal = tmp.path().canonicalize().unwrap().join("journal");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store
            .run_batch_next(
                || false,
                |_| {},
                |_, row| {
                    let p =
                        store.preview_rules(&row.write_id, &row.item.id, &row.item.source_hash)?;
                    store.apply_specs(&row.write_id, &p.after_hash, &journal, || false)?;
                    panic!("simulated process loss after commit")
                },
            )
            .unwrap();
    }));
    assert!(result.is_err());
    drop(store);
    let store = Store::open(&tmp.path().canonicalize().unwrap().join("state.sqlite")).unwrap();
    assert_eq!(store.task("recover").unwrap().state, TaskState::Interrupted);
    control(&store, "recover", TaskState::Requested);
    let mut recovered = 0;
    let t = store
        .run_batch_next(
            || false,
            |_| {},
            |_, row| {
                let p = store.preview_rules(&row.write_id, &row.item.id, &row.item.source_hash)?;
                if p.phase == "committed" {
                    recovered += 1;
                }
                let done = store.apply_specs(&row.write_id, &p.after_hash, &journal, || false)?;
                Ok(Outcome {
                    phase: done.phase,
                    error: None,
                    candidate_hash: Some(done.after_hash),
                })
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(recovered, 1);
    assert_eq!(t.processed, 2);
    assert_eq!(t.state, TaskState::Completed);
}
#[test]
fn ai_batch_locks_language_and_settings_and_counts_budget_across_files() {
    let (_tmp, store, items) = setup();
    let mut settings = ai::job::Settings {
        enabled: true,
        ..Default::default()
    };
    settings.config.model = "original-model".into();
    settings.config.base_url = "https://provider.example/v1".into();
    settings.run_request_limit = 1;
    store
        .save_ai_settings("ai-config", settings.clone())
        .unwrap();
    let mut req = request(&items, "ai-batch");
    req.engine = BatchEngine::Ai;
    req.mode = BatchMode::Rebuild;
    let t = store.plan_batch(req).unwrap();
    store
        .approve_batch_scope("ai-batch", &t.batch.unwrap().plan_hash)
        .unwrap();
    settings.config.model = "changed-model".into();
    store.save_ai_settings("changed-config", settings).unwrap();
    let mut calls = 0;
    let t=store.run_batch_next(||false,|_|{},|task,row|{
  let (r,execute)=store.begin_batch_ai(ai::job::Request{operation_id:row.request_id.clone(),item_id:row.item.id.clone(),expected_hash:row.item.source_hash.clone(),force:true,retry_failed:false},&task.id)?;
  assert!(execute);assert_eq!(r.settings.config.model,"original-model");assert_eq!(r.settings.config.output_language,"zh-CN");
  let body=ai::job::next(&r)?.unwrap().0;
  match store.reserve_ai_attempt(&row.request_id,body){Ok(_)=>{},Err(e)=>{assert_eq!(e.code,"budget-exhausted");return Err(e);}}
  calls+=1;
  let content=serde_json::json!({"tags":[{"value":"Arri Alexa 65","field":"Camera","source_indexes":[0],"confidence":"high"}],"warnings":[]}).to_string();
  let response=ai::HttpResponse{status:200,body:serde_json::to_vec(&serde_json::json!({"choices":[{"finish_reason":"stop","message":{"content":content}}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}))?};
  store.observe_ai_attempt(&row.request_id,Ok(response),0.0)?;Ok(success())
 }).unwrap().unwrap();
    assert_eq!(calls, 1);
    assert_eq!(t.state, TaskState::Paused);
    assert_eq!(t.errors, 1);
}

#[test]
fn item_checkpoints_are_separate_rows_and_early_snapshots_migrate_atomically() {
    let (tmp, store, items) = setup();
    let plan = store.plan_batch(request(&items, "storage")).unwrap();
    let path = tmp.path().canonicalize().unwrap().join("state.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    let metadata: String = db
        .query_row("SELECT body FROM tasks WHERE id='storage'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(metadata["batch"]["items"], serde_json::json!([]));
    assert_eq!(metadata["batch"]["total"], 2);
    db.execute_batch("CREATE TABLE checkpoint_audit(ordinal INTEGER);CREATE TRIGGER count_batch_changes AFTER UPDATE ON batch_items BEGIN INSERT INTO checkpoint_audit VALUES(NEW.ordinal);END;").unwrap();
    store
        .approve_batch_scope("storage", &plan.batch.as_ref().unwrap().plan_hash)
        .unwrap();
    let task = store
        .run_batch_next(
            || false,
            |_| {},
            |_, _| {
                control(&store, "storage", TaskState::Paused);
                Ok(success())
            },
        )
        .unwrap()
        .unwrap();
    let changed: Vec<u32> = db
        .prepare("SELECT ordinal FROM checkpoint_audit")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(changed, vec![0, 0]);
    assert_eq!(task.processed, 1);
    drop(store);
    // Recreate the previous in-body snapshot format and verify lossless upgrade.
    db.execute(
        "UPDATE tasks SET body=?1 WHERE id='storage'",
        [serde_json::to_string(&task).unwrap()],
    )
    .unwrap();
    db.execute("DELETE FROM batch_items WHERE task_id='storage'", [])
        .unwrap();
    db.pragma_update(None, "user_version", 6).unwrap();
    db.execute_batch("DROP TABLE ai_cache_v1_archive; DROP TABLE ai_failures_v1_archive;")
        .unwrap();
    drop(db);
    let store = Store::open(&path).unwrap();
    let migrated = store.task("storage").unwrap();
    assert_eq!(migrated, task);
    assert_eq!(
        store
            .tasks()
            .unwrap()
            .iter()
            .find(|t| t.id == "storage")
            .unwrap()
            .batch
            .as_ref()
            .unwrap()
            .items
            .len(),
        0
    );
}
