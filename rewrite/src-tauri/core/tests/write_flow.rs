use itm_core::{store::Store, writing::SpecsEdit, *};
use std::{fs, path::PathBuf};
const RAW:&str="\u{feff}<movie>\r\n<title>电影</title><year>2000</year><uniqueid type=\"imdb\">tt1234567</uniqueid><tag>保护</tag><technicalspecs source=\"IMDb\"><section name=\"Camera\"><item>Before</item></section></technicalspecs>\r\n</movie>";
fn setup() -> (tempfile::TempDir, Store, PathBuf, PathBuf, MediaItem) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    let path = media.join("电影.nfo");
    fs::write(&path, RAW).unwrap();
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
    (temp, store, path, root.join("journal"), item)
}
fn edit(item: &MediaItem) -> SpecsEdit {
    SpecsEdit {
        operation_id: "edit".into(),
        item_id: item.id.clone(),
        expected_hash: item.source_hash.clone(),
        specs: Specs::from([("Camera".into(), vec!["After & 中文".into()])]),
    }
}
#[test]
fn preview_apply_restart_and_undo_keep_original_bytes_and_update_index() {
    let (temp, store, path, journal, item) = setup();
    let request = edit(&item);
    let preview = store.preview_specs(request.clone()).unwrap();
    assert_eq!(preview.phase, "preview");
    assert_eq!(fs::read(&path).unwrap(), RAW.as_bytes());
    let applied = store
        .apply_specs("edit", &preview.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(applied.phase, "committed");
    assert_eq!(
        store.item(&item.id).unwrap().specs["Camera"],
        vec!["After & 中文"]
    );
    assert_eq!(store.preview_specs(request).unwrap().phase, "committed");
    let after = fs::read(&path).unwrap();
    assert_eq!(
        store
            .apply_specs("edit", &preview.after_hash, &journal, || panic!())
            .unwrap()
            .phase,
        "committed"
    );
    assert_eq!(fs::read(&path).unwrap(), after);
    drop(store);
    let store = Store::open(&temp.path().join("state.sqlite")).unwrap();
    let undo = store.preview_undo("undo", "edit", &journal).unwrap();
    assert_eq!(undo.undo_of.as_deref(), Some("edit"));
    store
        .apply_specs("undo", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), RAW.as_bytes());
    assert_eq!(
        store.item(&item.id).unwrap().specs["Camera"],
        vec!["Before"]
    );
    assert_eq!(store.write_history().unwrap().len(), 2);
}
#[test]
fn source_conflicts_and_changed_operation_input_never_write() {
    let (_temp, store, path, journal, item) = setup();
    let request = edit(&item);
    let preview = store.preview_specs(request.clone()).unwrap();
    let mut different = request;
    different.specs.clear();
    assert_eq!(
        store.preview_specs(different).unwrap_err().code,
        "operation-conflict"
    );
    assert_eq!(
        store
            .apply_specs("edit", "wrong-review", &journal, || false)
            .unwrap_err()
            .code,
        "review-mismatch"
    );
    let external = RAW.replace("电影", "外部修改");
    fs::write(&path, &external).unwrap();
    assert_eq!(
        store
            .apply_specs("edit", &preview.after_hash, &journal, || false)
            .unwrap_err()
            .code,
        "source-conflict"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
}
#[test]
fn crash_after_file_commit_is_recovered_without_repeating_write() {
    let (_temp, store, path, journal, item) = setup();
    let request = edit(&item);
    let preview = store.preview_specs(request).unwrap();
    let database = rusqlite::Connection::open_with_flags(
        journal.parent().unwrap().join("state.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let candidate: Vec<u8> = database
        .query_row(
            "SELECT body FROM write_candidates WHERE id='edit'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(database);
    let writer = transaction::Writer::new(&journal, vec![path.parent().unwrap().into()]).unwrap();
    writer
        .commit("edit", &path, &preview.before_hash, &candidate, |phase| {
            if matches!(phase, transaction::Phase::Replaced) {
                Err(AppError::new("interrupted", "crash"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(
        store
            .apply_specs("edit", &preview.after_hash, &journal, || panic!(
                "must replay"
            ))
            .unwrap()
            .phase,
        "committed"
    );
    assert_eq!(store.item(&item.id).unwrap().source_hash, hash(&candidate));
}
#[test]
fn unchanged_specs_do_not_modify_nfo_or_create_transaction_backup() {
    let (_temp, store, path, journal, item) = setup();
    let mut request = edit(&item);
    request.specs = item.specs.clone();
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let preview = store.preview_specs(request).unwrap();
    assert_eq!(preview.phase, "unchanged");
    store
        .apply_specs("edit", &preview.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert!(!journal.exists());
}
#[test]
fn undo_refuses_a_later_external_change_and_cancel_preserves_source() {
    let (_temp, store, path, journal, item) = setup();
    let preview = store.preview_specs(edit(&item)).unwrap();
    assert_eq!(
        store
            .apply_specs("edit", &preview.after_hash, &journal, || true)
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert_eq!(fs::read(&path).unwrap(), RAW.as_bytes());
    let mut request = edit(&item);
    request.operation_id = "second".into();
    let preview = store.preview_specs(request).unwrap();
    store
        .apply_specs("second", &preview.after_hash, &journal, || false)
        .unwrap();
    fs::write(&path, b"external").unwrap();
    assert_eq!(
        store
            .preview_undo("undo", "second", &journal)
            .unwrap_err()
            .code,
        "source-conflict"
    );
    assert_eq!(fs::read(&path).unwrap(), b"external");
}
#[cfg(windows)]
#[test]
fn windows_replacement_preserves_acl_and_named_streams() {
    use std::process::Command;
    let (_temp, store, path, journal, item) = setup();
    let stream = PathBuf::from(format!("{}:acceptance", path.display()));
    fs::write(&stream, b"external metadata").unwrap();
    let acl = || {
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-Acl -LiteralPath $env:ITM_ACL_TEST_PATH).Sddl",
            ])
            .env("ITM_ACL_TEST_PATH", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    let before = acl();
    let preview = store.preview_specs(edit(&item)).unwrap();
    store
        .apply_specs("edit", &preview.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(acl(), before);
    assert_eq!(fs::read(&stream).unwrap(), b"external metadata");
    let undo = store.preview_undo("undo", "edit", &journal).unwrap();
    store
        .apply_specs("undo", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(acl(), before);
    assert_eq!(fs::read(&stream).unwrap(), b"external metadata");
}
