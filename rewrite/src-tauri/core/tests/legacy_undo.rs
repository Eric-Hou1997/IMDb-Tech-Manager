use base64::{engine::general_purpose::STANDARD, Engine};
use itm_core::{legacy_undo::*, store::Store, *};
use serde_json::json;
use std::fs;
struct Fixture {
    temp: tempfile::TempDir,
    store: Store,
    item: MediaItem,
    source: String,
    before: Vec<u8>,
    after: Vec<u8>,
}
fn setup(expired: bool, mapped: bool, corrupt: bool) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    let nfo = media.join("电影.nfo");
    let before=b"\xef\xbb\xbf<movie>\r\n<title>Fixture</title><uniqueid type=\"imdb\">tt1234567</uniqueid><tag>External original</tag><unknown>keep</unknown>\r\n</movie>\r\n".to_vec();
    let after = String::from_utf8(before.clone())
        .unwrap()
        .replace("External original", "External edited")
        .into_bytes();
    fs::write(&nfo, &after).unwrap();
    let old_path = if mapped {
        "/Volumes/OldLibrary/电影.nfo".to_owned()
    } else {
        nfo.to_str().unwrap().into()
    };
    let source = format!("undo/{}.json", hash(old_path.as_bytes()));
    let legacy = root.join("legacy");
    fs::create_dir_all(legacy.join("undo")).unwrap();
    let created = chrono::Utc::now() - chrono::Duration::minutes(if expired { 40 } else { 1 });
    let record = json!({"schema":1,"path":old_path,"operation":"edit-tag","created_at":created.to_rfc3339(),"expires_at":(created+chrono::Duration::minutes(30)).to_rfc3339(),"before":STANDARD.encode(&before),"before_hash":if corrupt {"0".repeat(64)} else {hash(&before)},"after_hash":hash(&after)});
    fs::write(legacy.join(&source), serde_json::to_vec(&record).unwrap()).unwrap();
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
    let plan = store
        .prepare_migration("import", &legacy, "itm-engine")
        .unwrap();
    store.apply_migration("import", &plan.fingerprint).unwrap();
    Fixture {
        temp,
        store,
        item,
        source,
        before,
        after,
    }
}
fn request(f: &Fixture, id: &str) -> LegacyUndoRequest {
    LegacyUndoRequest {
        operation_id: id.into(),
        item_id: f.item.id.clone(),
        import_id: "import".into(),
        source: f.source.clone(),
        expected_hash: hash(&f.after),
        confirm_path_mapping: false,
    }
}
#[test]
fn imported_undo_restores_exact_bytes_and_new_transaction_can_itself_be_undone() {
    let f = setup(false, false, false);
    let page = f.store.legacy_undo_entries(&f.item.id, 0).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.entries[0].state, "available");
    let p = f.store.preview_legacy_undo(request(&f, "restore")).unwrap();
    assert_eq!(fs::read(&f.item.path).unwrap(), f.after);
    assert!(p.before_xml.contains("<unknown>keep</unknown>"));
    assert!(p.after_xml.contains("External original"));
    let journal = f.temp.path().canonicalize().unwrap().join("journal");
    f.store
        .apply_specs("restore", &p.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&f.item.path).unwrap(), f.before);
    let mtime = fs::metadata(&f.item.path).unwrap().modified().unwrap();
    f.store
        .apply_specs("restore", &p.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(
        fs::metadata(&f.item.path).unwrap().modified().unwrap(),
        mtime
    );
    assert_eq!(
        f.store.legacy_undo_entries(&f.item.id, 0).unwrap().entries[0].state,
        "consumed"
    );
    let undo = f
        .store
        .preview_undo("undo-restoration", "restore", &journal)
        .unwrap();
    f.store
        .apply_specs("undo-restoration", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&f.item.path).unwrap(), f.after);
    assert_eq!(
        f.store.legacy_undo_entries(&f.item.id, 0).unwrap().entries[0].state,
        "consumed"
    );
    assert_eq!(
        f.store
            .preview_legacy_undo(request(&f, "cannot-reuse"))
            .unwrap_err()
            .code,
        "legacy-undo-unavailable"
    );
}
#[test]
fn moved_path_requires_explicit_mapping_even_when_full_hash_matches() {
    let f = setup(false, true, false);
    assert_eq!(
        f.store.legacy_undo_entries(&f.item.id, 0).unwrap().entries[0].state,
        "path-confirmation-required"
    );
    assert_eq!(
        f.store
            .preview_legacy_undo(request(&f, "blocked"))
            .unwrap_err()
            .code,
        "legacy-path-confirmation"
    );
    let mut r = request(&f, "mapped");
    r.confirm_path_mapping = true;
    let p = f.store.preview_legacy_undo(r).unwrap();
    f.store
        .apply_specs(
            "mapped",
            &p.after_hash,
            &f.temp.path().canonicalize().unwrap().join("journal"),
            || false,
        )
        .unwrap();
    assert_eq!(fs::read(&f.item.path).unwrap(), f.before);
}
#[test]
fn expired_and_corrupt_journals_are_visible_but_never_restore() {
    for (expired, corrupt, state, code) in [
        (true, false, "expired", "legacy-undo-expired"),
        (false, true, "invalid", "legacy-undo-hash"),
    ] {
        let f = setup(expired, false, corrupt);
        assert_eq!(
            f.store.legacy_undo_entries(&f.item.id, 0).unwrap().entries[0].state,
            state
        );
        assert_eq!(
            f.store
                .preview_legacy_undo(request(&f, "restore"))
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(fs::read(&f.item.path).unwrap(), f.after);
    }
}
#[test]
fn concurrent_edit_cancellation_and_candidate_tampering_preserve_current_file() {
    let f = setup(false, false, false);
    let p = f.store.preview_legacy_undo(request(&f, "restore")).unwrap();
    let journal = f.temp.path().canonicalize().unwrap().join("journal");
    assert_eq!(
        f.store
            .apply_specs("restore", "wrong-reviewed-hash", &journal, || false)
            .unwrap_err()
            .code,
        "review-mismatch"
    );
    assert_eq!(
        f.store
            .apply_specs("restore", &p.after_hash, &journal, || true)
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert_eq!(fs::read(&f.item.path).unwrap(), f.after);
    let p = f.store.preview_legacy_undo(request(&f, "second")).unwrap();
    let changed = String::from_utf8(f.after.clone())
        .unwrap()
        .replace("External edited", "New external edit")
        .into_bytes();
    fs::write(&f.item.path, &changed).unwrap();
    assert_eq!(
        f.store
            .apply_specs("second", &p.after_hash, &journal, || false)
            .unwrap_err()
            .code,
        "source-conflict"
    );
    assert_eq!(fs::read(&f.item.path).unwrap(), changed);
}
#[test]
fn archive_tampering_after_preview_prevents_write_and_preserves_nfo() {
    let f = setup(false, false, false);
    let p = f.store.preview_legacy_undo(request(&f, "restore")).unwrap();
    let db = rusqlite::Connection::open(f.temp.path().canonicalize().unwrap().join("state.sqlite"))
        .unwrap();
    db.execute(
        "UPDATE legacy_artifacts SET body=?1 WHERE import_id='import'",
        [b"{}".as_slice()],
    )
    .unwrap();
    assert_eq!(
        f.store
            .apply_specs(
                "restore",
                &p.after_hash,
                &f.temp.path().canonicalize().unwrap().join("journal"),
                || false
            )
            .unwrap_err()
            .code,
        "legacy-undo-archive"
    );
    assert_eq!(fs::read(&f.item.path).unwrap(), f.after);
}
#[test]
fn two_reviewed_restores_cannot_reuse_one_legacy_journal() {
    let f = setup(false, false, false);
    let first = f.store.preview_legacy_undo(request(&f, "first")).unwrap();
    let second = f.store.preview_legacy_undo(request(&f, "second")).unwrap();
    let journal = f.temp.path().canonicalize().unwrap().join("journal");
    f.store
        .apply_specs("first", &first.after_hash, &journal, || false)
        .unwrap();
    let reverse = f.store.preview_undo("reverse", "first", &journal).unwrap();
    f.store
        .apply_specs("reverse", &reverse.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(
        f.store
            .apply_specs("second", &second.after_hash, &journal, || false)
            .unwrap_err()
            .code,
        "legacy-undo-unavailable"
    );
    assert_eq!(fs::read(&f.item.path).unwrap(), f.after);
}
#[test]
fn interrupted_restore_resumes_only_under_original_operation_id() {
    let f = setup(false, false, false);
    let first = f.store.preview_legacy_undo(request(&f, "first")).unwrap();
    let db = rusqlite::Connection::open(f.temp.path().canonicalize().unwrap().join("state.sqlite"))
        .unwrap();
    // The persisted state immediately before Writer is entered, including a crash before any write.
    db.execute(
        "UPDATE operations SET result=json_set(result,'$.result.phase','writing') WHERE id='first'",
        [],
    )
    .unwrap();
    assert_eq!(
        f.store.legacy_undo_entries(&f.item.id, 0).unwrap().entries[0].state,
        "restore-pending"
    );
    assert_eq!(
        f.store
            .preview_legacy_undo(request(&f, "second"))
            .unwrap_err()
            .code,
        "legacy-undo-unavailable"
    );
    f.store
        .apply_specs(
            "first",
            &first.after_hash,
            &f.temp.path().canonicalize().unwrap().join("journal"),
            || false,
        )
        .unwrap();
    assert_eq!(fs::read(&f.item.path).unwrap(), f.before);
}
