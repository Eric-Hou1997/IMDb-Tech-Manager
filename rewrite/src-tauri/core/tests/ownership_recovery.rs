use itm_core::{store::Store, writing::*, *};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
const RAW: &str = "\u{feff}<movie>\r\n<title>Recovery</title><uniqueid type=\"imdb\">tt1234567</uniqueid><tag>TMM</tag><tag>Old</tag><tag keep=\"yes\">Manual</tag><unknown a=\"retain\"/><technicalspecs source=\"IMDb\" imdbid=\"tt1234567\" formatVersion=\"21\"><section name=\"Camera\"><item>New</item><item>TMM</item><item>Manual</item></section></technicalspecs>\r\n</movie>\r\n";
struct Fixture {
    temp: tempfile::TempDir,
    store: Store,
    item: MediaItem,
    old: PathBuf,
    sidecar: PathBuf,
    value: Value,
}
fn fixture(change: impl FnOnce(&mut Value)) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let media = dir.join("media");
    fs::create_dir(&media).unwrap();
    let nfo = media.join("中文电影.nfo");
    fs::write(&nfo, RAW).unwrap();
    let old = dir.join("old");
    fs::create_dir(&old).unwrap();
    fs::create_dir(old.join("ownership")).unwrap();
    fs::write(
        old.join("config.json"),
        serde_json::to_vec(&json!({"library_roots":{"movies":[media],"tv":[]}})).unwrap(),
    )
    .unwrap();
    let specs = tags::effective_specs(RAW.as_bytes()).unwrap();
    let mut value = json!({"schema":2,"path":nfo,"imdb":"tt1234567","engine":"local-rules","model":"original","spec_hash":specs::fingerprint(&specs).unwrap(),"state":"current","generated":"2026-08-01T00:00:00Z","entries":[{"value":"Old","field":"Camera","source_indexes":[0]}],"manual_entries":[{"id":"m1","value":"Manual","origin":"manual-add","created":"original-time"}]});
    change(&mut value);
    let sidecar = old
        .join("ownership")
        .join(format!("{}.json", hash(nfo.to_string_lossy().as_bytes())));
    fs::write(&sidecar, serde_json::to_vec(&value).unwrap()).unwrap();
    let store = Store::open(&dir.join("state.sqlite")).unwrap();
    let plan = store
        .prepare_migration("old-import", &old, "itm-engine")
        .unwrap();
    store
        .apply_migration("old-import", &plan.fingerprint)
        .unwrap();
    let root = store.configuration().unwrap().roots.remove(0);
    store
        .submit(ScanRequest {
            operation_id: "scan".into(),
            space: Space::Movie,
            root_ids: vec![root.id],
        })
        .unwrap();
    store.run_next(|| false, |_| {}).unwrap();
    let item = store.item(&hash(nfo.to_string_lossy().as_bytes())).unwrap();
    Fixture {
        temp,
        store,
        item,
        old,
        sidecar,
        value,
    }
}
fn check_original(f: &Fixture, modified: std::time::SystemTime) {
    assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
    assert_eq!(
        fs::metadata(&f.item.path).unwrap().modified().unwrap(),
        modified
    );
}
#[test]
fn stripped_ownership_is_read_time_only_restores_status_and_survives_old_folder_offline() {
    let f = fixture(|_| {});
    let modified = fs::metadata(&f.item.path).unwrap().modified().unwrap();
    let old_bytes = fs::read(&f.sidecar).unwrap();
    let old_time = fs::metadata(&f.sidecar).unwrap().modified().unwrap();
    for item in [
        f.store.inspect_item(&f.item.id).unwrap(),
        f.store.all_items().unwrap().remove(0),
    ] {
        assert_eq!(item.source_hash, hash(RAW.as_bytes()));
        assert_eq!(
            item.tags.iter().map(|t| &t.ownership).collect::<Vec<_>>(),
            vec![
                &Ownership::External,
                &Ownership::External,
                &Ownership::External
            ]
        );
        assert_eq!(item.inspection.tag_status, "local-current");
        assert!(item.inspection.tag_engine.is_empty());
        assert!(item.inspection.tag_model.is_empty());
        assert_eq!(item.inspection.manifest_sidecar_match, None);
        assert!(!item
            .inspection
            .issues
            .contains(&"ownership-mismatch".into()));
    }
    // The persisted index remains NFO-only; neither inspector nor list can
    // leave a stale recovery overlay in the indexed bytes.
    assert!(f
        .store
        .item(&f.item.id)
        .unwrap()
        .tags
        .iter()
        .all(|t| t.ownership == Ownership::External));
    check_original(&f, modified);
    assert_eq!(fs::read(&f.sidecar).unwrap(), old_bytes);
    assert_eq!(
        fs::metadata(&f.sidecar).unwrap().modified().unwrap(),
        old_time
    );
    fs::rename(&f.old, f.old.with_file_name("offline")).unwrap();
    assert_eq!(
        f.store
            .inspect_item(&f.item.id)
            .unwrap()
            .inspection
            .tag_status,
        "local-current"
    );
    assert_eq!(f.store.tasks().unwrap().len(), 1);
    assert!(f.store.ai_history(None).unwrap().is_empty());
}
#[test]
fn inspector_keeps_original_embedded_only_badges_noop_and_external_edit_behavior() {
    let f = fixture(|_| {});
    let modified = fs::metadata(&f.item.path).unwrap().modified().unwrap();
    let no_op = f
        .store
        .preview_tags(TagEdit {
            operation_id: "same-tag".into(),
            item_id: f.item.id.clone(),
            expected_hash: f.item.source_hash.clone(),
            action: tags::Action::Edit {
                root_index: 1,
                value: "Old".into(),
            },
        })
        .unwrap();
    assert_eq!(no_op.phase, "unchanged");
    check_original(&f, modified);
    let edited = f
        .store
        .preview_tags(TagEdit {
            operation_id: "edit-tag".into(),
            item_id: f.item.id.clone(),
            expected_hash: f.item.source_hash.clone(),
            action: tags::Action::Edit {
                root_index: 1,
                value: "Edited external".into(),
            },
        })
        .unwrap();
    assert!(matches!(edited.intent, WriteIntent::Tags { .. }));
    assert_eq!(edited.before_tags[1].ownership, Ownership::External);
    assert_eq!(edited.after_tags[1].ownership, Ownership::External);
    let journal = f
        .temp
        .path()
        .canonicalize()
        .unwrap()
        .join("nfo-transactions");
    f.store
        .apply_specs("edit-tag", &edited.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(
        fs::read_to_string(&f.item.path).unwrap(),
        RAW.replace("<tag>Old</tag>", "<tag>Edited external</tag>")
    );
    assert_eq!(
        fs::read(&f.sidecar).unwrap(),
        serde_json::to_vec(&f.value).unwrap()
    );
}
#[test]
fn approved_rules_reuse_original_transaction_mirror_replay_and_exact_undo() {
    let f = fixture(|_| {});
    let mtime = fs::metadata(&f.item.path).unwrap().modified().unwrap();
    let preview = f
        .store
        .preview_rules("rules", &f.item.id, &f.item.source_hash)
        .unwrap();
    assert!(matches!(preview.intent, WriteIntent::RecoveredTags { .. }));
    assert_eq!(preview.before_tags[1].ownership, Ownership::External);
    assert_eq!(
        preview
            .after_tags
            .iter()
            .map(|t| t.value.as_str())
            .collect::<Vec<_>>(),
        vec!["TMM", "New", "Manual"]
    );
    assert_eq!(preview.after_tags[0].ownership, Ownership::External);
    assert_eq!(preview.after_tags[2].ownership, Ownership::Manual);
    check_original(&f, mtime);
    let journal = f
        .temp
        .path()
        .canonicalize()
        .unwrap()
        .join("nfo-transactions");
    let applied = f
        .store
        .apply_specs("rules", &preview.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(applied.phase, "committed");
    let raw = fs::read_to_string(&f.item.path).unwrap();
    assert!(raw.starts_with('\u{feff}'));
    assert!(!raw.replace("\r\n", "").contains('\n'));
    assert!(
        raw.contains("<unknown a=\"retain\"/>") && raw.contains("<tag keep=\"yes\">Manual</tag>")
    );
    assert_eq!(
        f.store
            .apply_specs("rules", &preview.after_hash, &journal, || false)
            .unwrap()
            .after_hash,
        applied.after_hash
    );
    let undo = f.store.preview_undo("undo", "rules", &journal).unwrap();
    f.store
        .apply_specs("undo", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
    assert_eq!(
        f.store
            .inspect_item(&f.item.id)
            .unwrap()
            .inspection
            .tag_status,
        "local-current"
    );
}
#[test]
fn specs_edit_preserves_recovered_manual_generated_and_root_bytes_without_regeneration() {
    let f = fixture(|_| {});
    let journal = f
        .temp
        .path()
        .canonicalize()
        .unwrap()
        .join("nfo-transactions");
    let preview = f
        .store
        .preview_specs(SpecsEdit {
            operation_id: "edit-specs".into(),
            item_id: f.item.id.clone(),
            expected_hash: f.item.source_hash.clone(),
            specs: Specs::from([("Camera".into(), vec!["Human specs".into()])]),
        })
        .unwrap();
    assert!(matches!(preview.intent, WriteIntent::RecoveredSpecs { .. }));
    assert_eq!(
        preview
            .before_tags
            .iter()
            .map(|t| &t.value)
            .collect::<Vec<_>>(),
        preview
            .after_tags
            .iter()
            .map(|t| &t.value)
            .collect::<Vec<_>>()
    );
    f.store
        .apply_specs("edit-specs", &preview.after_hash, &journal, || false)
        .unwrap();
    let item = f.store.inspect_item(&f.item.id).unwrap();
    assert_eq!(item.inspection.tag_status, "stale");
    assert_eq!(item.tags, preview.after_tags);
    assert_eq!(f.store.tasks().unwrap().len(), 1);
    assert!(f.store.ai_history(None).unwrap().is_empty());
    assert_eq!(
        fs::read(&f.sidecar).unwrap(),
        serde_json::to_vec(&f.value).unwrap()
    );
}
#[test]
fn changed_record_after_review_and_cancellation_preserve_original_nfo() {
    let f = fixture(|_| {});
    let mtime = fs::metadata(&f.item.path).unwrap().modified().unwrap();
    let preview = f
        .store
        .preview_rules("rules", &f.item.id, &f.item.source_hash)
        .unwrap();
    let journal = f
        .temp
        .path()
        .canonicalize()
        .unwrap()
        .join("nfo-transactions");
    let local = f.temp.path().canonicalize().unwrap().join("ownership");
    fs::create_dir(&local).unwrap();
    let target = local.join(f.sidecar.file_name().unwrap());
    let mut changed = f.value.clone();
    changed["model"] = json!("changed");
    fs::write(&target, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_eq!(
        f.store
            .apply_specs("rules", &preview.after_hash, &journal, || false)
            .unwrap_err()
            .code,
        "unsafe-skip"
    );
    check_original(&f, mtime);
    fs::write(&target, serde_json::to_vec(&f.value).unwrap()).unwrap();
    assert_eq!(
        f.store
            .apply_specs("rules", &preview.after_hash, &journal, || true)
            .unwrap_err()
            .code,
        "cancelled"
    );
    check_original(&f, mtime);
}
#[test]
fn conflicting_identity_schema_entries_and_duplicate_root_tags_fail_closed() {
    for kind in 0..5 {
        let f = fixture(|v| match kind {
            0 => v["imdb"] = json!("tt9999999"),
            1 => v["path"] = json!("/another/movie.nfo"),
            2 => v["schema"] = json!(99),
            3 => v["entries"] = json!([{"id":"g1","value":"Old"},{"id":"g1","value":"Manual"}]),
            _ => v["engine"] = json!("another-product"),
        });
        let mtime = fs::metadata(&f.item.path).unwrap().modified().unwrap();
        assert!(f
            .store
            .inspect_item(&f.item.id)
            .unwrap()
            .inspection
            .issues
            .contains(&"ownership-mismatch".into()));
        assert_eq!(
            f.store
                .preview_rules("rules", &f.item.id, &f.item.source_hash)
                .unwrap_err()
                .code,
            "unsafe-skip"
        );
        check_original(&f, mtime);
    }
    let f = fixture(|_| {});
    let raw = RAW.replace("<tag>Old</tag>", "<tag>Old</tag><tag>old</tag>");
    fs::write(&f.item.path, &raw).unwrap();
    let item = f.store.inspect_item(&f.item.id).unwrap();
    assert!(item
        .inspection
        .issues
        .contains(&"ownership-mismatch".into()));
    assert_eq!(
        f.store
            .preview_rules("rules", &item.id, &item.source_hash)
            .unwrap_err()
            .code,
        "unsafe-skip"
    );
    assert_eq!(fs::read(&f.item.path).unwrap(), raw.as_bytes());
}
#[test]
fn embedded_manifest_wins_and_ai_uses_recovered_generated_context() {
    let f = fixture(|_| {});
    let mut settings = ai::job::Settings {
        enabled: true,
        ..Default::default()
    };
    settings.config.base_url = "https://provider.example/v1".into();
    settings.config.model = "model".into();
    f.store.save_ai_settings("ai-settings", settings).unwrap();
    let (record, _) = f
        .store
        .begin_ai(ai::job::Request {
            operation_id: "ai".into(),
            item_id: f.item.id.clone(),
            expected_hash: f.item.source_hash.clone(),
            force: false,
            retry_failed: false,
        })
        .unwrap();
    assert_eq!(
        record.existing,
        vec![json!({"value":"Old","source":"rules"})]
    );
    assert!(record.attempts.is_empty());
    let raw=RAW.replace("</technicalspecs>","<generatedtags owner=\"IMDb Tech Manager\" schema=\"2\" engine=\"ai\"><tag id=\"embedded\" field=\"Camera\">TMM</tag></generatedtags></technicalspecs>");
    fs::write(&f.item.path, &raw).unwrap();
    let item = f.store.inspect_item(&f.item.id).unwrap();
    assert_eq!(item.tags[0].ownership, Ownership::Generated);
    assert_eq!(item.tags[1].ownership, Ownership::External);
    assert_eq!(
        f.store
            .preview_rules("rules", &item.id, &item.source_hash)
            .unwrap()
            .before_tags[1]
            .ownership,
        Ownership::External
    );
    assert_eq!(
        fs::read(&f.sidecar).unwrap(),
        serde_json::to_vec(&f.value).unwrap()
    );
}
#[test]
fn local_mirror_change_or_removal_never_persists_a_stale_list_overlay() {
    let f = fixture(|_| {});
    let local = f.temp.path().canonicalize().unwrap().join("ownership");
    fs::create_dir(&local).unwrap();
    let target = local.join(f.sidecar.file_name().unwrap());
    let mut changed = f.value.clone();
    changed["engine"] = json!("ai");
    changed["entries"] = json!([]);
    changed["manual_entries"] = json!([]);
    fs::write(&target, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_eq!(
        f.store.all_items().unwrap()[0].tags[1].ownership,
        Ownership::External
    );
    let displayed = f.store.all_items().unwrap().remove(0);
    assert_eq!(displayed.inspection.tag_status, "ai-current");
    assert!(displayed.inspection.tag_engine.is_empty());
    assert!(!displayed.inspection.issues.contains(&"prompt-stale".into()));
    fs::remove_file(&target).unwrap();
    assert_eq!(
        f.store.all_items().unwrap()[0].inspection.tag_status,
        "local-current"
    );
    // A local cleared record takes precedence and does not resurrect archives.
    fs::write(&target, b"{\"engine\":\"\",\"entries\":[]}").unwrap();
    assert!(f.store.all_items().unwrap()[0]
        .tags
        .iter()
        .all(|t| t.ownership == Ownership::External));
    assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
}
#[test]
fn interrupted_recovered_write_finishes_index_and_mirror_without_replacing_nfo_twice() {
    let f = fixture(|_| {});
    let preview = f
        .store
        .preview_rules("rules", &f.item.id, &f.item.source_hash)
        .unwrap();
    let dir = f.temp.path().canonicalize().unwrap();
    let db = rusqlite::Connection::open(dir.join("state.sqlite")).unwrap();
    let candidate: Vec<u8> = db
        .query_row(
            "SELECT body FROM write_candidates WHERE id='rules'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let journal = dir.join("nfo-transactions");
    let writer = transaction::Writer::new(
        &journal,
        vec![PathBuf::from(
            &f.store.configuration().unwrap().roots[0].path,
        )],
    )
    .unwrap();
    writer
        .commit_with_intent(
            "rules",
            std::path::Path::new(&f.item.path),
            &preview.before_hash,
            &candidate,
            &preview.intent,
            |_| Ok(()),
        )
        .unwrap();
    let mtime = fs::metadata(&f.item.path).unwrap().modified().unwrap();
    // Simulate process loss after the owned file transaction, before the
    // Store's index/mirror completion. The immutable journal is the receipt.
    let result = f
        .store
        .apply_specs("rules", &preview.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(result.phase, "committed");
    assert_eq!(
        fs::metadata(&f.item.path).unwrap().modified().unwrap(),
        mtime
    );
    assert_eq!(
        f.store.inspect_item(&f.item.id).unwrap().tags[1].value,
        "New"
    );
    let mirrored = dir.join("ownership").join(f.sidecar.file_name().unwrap());
    assert!(mirrored.is_file());
    assert_eq!(
        fs::read(&f.sidecar).unwrap(),
        serde_json::to_vec(&f.value).unwrap()
    );
}
#[test]
fn corrupt_archives_conflicting_imports_symlink_mirrors_and_source_change_reject_writes() {
    for conflict in [false, true] {
        let f = fixture(|_| {});
        let db =
            rusqlite::Connection::open(f.temp.path().canonicalize().unwrap().join("state.sqlite"))
                .unwrap();
        if conflict {
            let mut other = f.value.clone();
            other["model"] = json!("different");
            let raw = serde_json::to_vec(&other).unwrap();
            db.execute("INSERT INTO legacy_artifacts(import_id,path,category,sha256,body) VALUES('conflicting-import',?1,'ownership',?2,?3)",rusqlite::params![format!("ownership/{}",f.sidecar.file_name().unwrap().to_str().unwrap()),hash(&raw),raw]).unwrap();
        } else {
            db.execute(
                "UPDATE legacy_artifacts SET body=x'7b7d' WHERE category='ownership'",
                [],
            )
            .unwrap();
        }
        let error = f
            .store
            .preview_rules("rules", &f.item.id, &f.item.source_hash)
            .unwrap_err();
        assert_eq!(
            error.code,
            if conflict {
                "unsafe-skip"
            } else {
                "migration-archive-corrupt"
            }
        );
        assert_eq!(fs::read(&f.item.path).unwrap(), RAW.as_bytes());
    }
    let f = fixture(|_| {});
    let preview = f
        .store
        .preview_rules("rules", &f.item.id, &f.item.source_hash)
        .unwrap();
    let changed = RAW.replace("<tag>TMM</tag>", "<tag>New external</tag>");
    fs::write(&f.item.path, &changed).unwrap();
    let journal = f
        .temp
        .path()
        .canonicalize()
        .unwrap()
        .join("nfo-transactions");
    assert_eq!(
        f.store
            .apply_specs("rules", &preview.after_hash, &journal, || false)
            .unwrap_err()
            .code,
        "source-conflict"
    );
    assert_eq!(fs::read(&f.item.path).unwrap(), changed.as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let local = f.temp.path().canonicalize().unwrap().join("ownership");
        fs::create_dir(&local).unwrap();
        let target = local.join(f.sidecar.file_name().unwrap());
        symlink(&f.sidecar, &target).unwrap();
        let item = f.store.inspect_item(&f.item.id).unwrap();
        assert!(item
            .inspection
            .issues
            .contains(&"ownership-mismatch".into()));
        assert!(f
            .store
            .preview_rules("another-preview", &item.id, &item.source_hash)
            .is_err());
        assert!(fs::symlink_metadata(&target).unwrap().is_symlink());
        assert_eq!(
            fs::read(&f.sidecar).unwrap(),
            serde_json::to_vec(&f.value).unwrap()
        );
    }
}
