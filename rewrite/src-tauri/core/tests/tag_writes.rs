use itm_core::{
    tags::{self, Action, GeneratedEntry, Plan},
    transaction::{Phase, Writer},
    writing::WriteIntent,
    *,
};
use std::fs;
const RAW:&str="\u{feff}<movie>\r\n<title>中文电影</title><tag>External</tag><tag keep=\"yes\">Manual</tag><tag>Old</tag><technicalspecs source=\"IMDb\" formatVersion=\"21\"><section name=\"Camera\"><item>New</item><item>External</item><item>Manual</item></section><!-- retain facts --><sourcesnapshot><section name=\"Camera\"><item>Original</item></section></sourcesnapshot><generatedtags owner=\"IMDb Tech Manager\" schema=\"2\" engine=\"ai\" state=\"stale\"><tag id=\"old-id\" origin=\"generated\" field=\"Camera\">Old</tag></generatedtags><manualtags owner=\"IMDb Tech Manager\" schema=\"1\"><tag id=\"manual-id\" origin=\"manual-add\">Manual</tag></manualtags></technicalspecs><actor><name>Preserve</name></actor>\r\n</movie>";
fn plan(action: Action) -> Plan {
    Plan {
        action,
        timestamp: "2026-09-11T00:00:00Z".into(),
    }
}
fn generate(raw: &[u8]) -> Plan {
    let entries = rules::entries(&tags::effective_specs(raw).unwrap())
        .into_iter()
        .map(|e| GeneratedEntry {
            value: e.value,
            field: e.field,
            source_indexes: e.source_indexes,
            confidence: "high".into(),
            operation: String::new(),
        })
        .collect();
    plan(Action::Generate {
        entries,
        engine: "local-rules".into(),
        model: String::new(),
        prompt_hash: String::new(),
    })
}
fn string(raw: &[u8]) -> String {
    String::from_utf8(raw.to_vec()).unwrap()
}
#[test]
fn regeneration_protects_external_manual_facts_and_stays_byte_stable() {
    let p = generate(RAW.as_bytes());
    let result = tags::candidate(RAW.as_bytes(), &p).unwrap();
    let s = string(&result);
    assert!(s.starts_with("\u{feff}<movie>\r\n<title>中文电影</title><tag>External</tag><tag keep=\"yes\">Manual</tag><tag>New</tag>"));
    assert!(s.contains("<section name=\"Camera\"><item>New</item><item>External</item><item>Manual</item></section><!-- retain facts --><sourcesnapshot><section name=\"Camera\"><item>Original</item></section></sourcesnapshot>"));
    assert!(s.contains("<manualtags owner=\"IMDb Tech Manager\" schema=\"1\"><tag id=\"manual-id\" origin=\"manual-add\">Manual</tag></manualtags>"));
    let doc = roxmltree::Document::parse(s.trim_start_matches('\u{feff}')).unwrap();
    let generated = doc
        .descendants()
        .find(|n| n.has_tag_name("generatedtags"))
        .unwrap();
    assert_eq!(generated.children().filter(|n| n.is_element()).count(), 1);
    assert_eq!(generated.attribute("state"), Some("current"));
    assert_eq!(tags::candidate(&result, &p).unwrap(), result);
}
#[test]
fn manual_edit_of_generated_is_protected_from_future_generation() {
    let edited = tags::candidate(
        RAW.as_bytes(),
        &plan(Action::Edit {
            root_index: 2,
            value: "My & Camera".into(),
        }),
    )
    .unwrap();
    let rebuilt = tags::candidate(&edited, &generate(&edited)).unwrap();
    let s = string(&rebuilt);
    assert!(s.contains("<tag>My &amp; Camera</tag>"));
    assert!(s.contains("origin=\"manual-edit\""));
    assert!(s.contains("replaces=\"old-id\""));
    assert!(s.contains("<tag>New</tag>"));
    let unchanged = tags::candidate(
        &edited,
        &plan(Action::SetOwnership {
            root_index: 2,
            ownership: "manual".into(),
        }),
    )
    .unwrap();
    assert_eq!(unchanged, edited);
}
#[test]
fn external_edit_keeps_ownership_and_deletion_requires_confirmation() {
    let p = plan(Action::Edit {
        root_index: 0,
        value: "Changed".into(),
    });
    let edited = tags::candidate(RAW.as_bytes(), &p).unwrap();
    assert_eq!(
        string(&edited),
        RAW.replacen("<tag>External</tag>", "<tag>Changed</tag>", 1)
    );
    assert_eq!(
        tags::candidate(
            RAW.as_bytes(),
            &plan(Action::Delete {
                root_index: 0,
                confirm_external: false
            })
        )
        .unwrap_err()
        .code,
        "external-confirmation-required"
    );
    assert_eq!(
        string(
            &tags::candidate(
                RAW.as_bytes(),
                &plan(Action::Delete {
                    root_index: 0,
                    confirm_external: true
                })
            )
            .unwrap()
        ),
        RAW.replacen("<tag>External</tag>", "", 1)
    );
    assert_eq!(
        tags::candidate(
            RAW.as_bytes(),
            &plan(Action::Edit {
                root_index: 0,
                value: "Old".into()
            })
        )
        .unwrap_err()
        .code,
        "unsafe-skip"
    );
}
#[test]
fn ambiguous_ownership_unknown_schema_and_missing_root_fail_closed() {
    for bad in [
        RAW.replace("<tag>Old</tag>", "<tag>Old</tag><tag>OLD</tag>"),
        RAW.replace("id=\"old-id\"", "id=\"manual-id\""),
        RAW.replace("schema=\"2\"", "schema=\"3\""),
        RAW.replace("<tag>Old</tag>", ""),
        RAW.replace(
            "owner=\"IMDb Tech Manager\" schema=\"2\"",
            "owner=\"Another Tool\" schema=\"2\"",
        ),
    ] {
        assert_eq!(
            tags::candidate(bad.as_bytes(), &generate(bad.as_bytes()))
                .unwrap_err()
                .code,
            "unsafe-skip"
        );
    }
}
#[test]
fn explicit_ownership_and_ai_clear_use_individual_engine_and_keep_roots() {
    let claimed = tags::candidate(
        RAW.as_bytes(),
        &plan(Action::SetOwnership {
            root_index: 0,
            ownership: "local-rules".into(),
        }),
    )
    .unwrap();
    assert!(string(&claimed).contains("<tag>External</tag>"));
    let cleared = tags::candidate(&claimed, &plan(Action::ClearAi { confirmed: true })).unwrap();
    let s = string(&cleared);
    assert!(s.contains("<tag>External</tag>"));
    assert!(!s.contains("<tag>Old</tag>"));
    assert!(s.contains("<tag keep=\"yes\">Manual</tag>"));
    let released = tags::candidate(
        &cleared,
        &plan(Action::SetOwnership {
            root_index: 0,
            ownership: "external".into(),
        }),
    )
    .unwrap();
    assert!(string(&released).contains("<tag>External</tag>"));
    assert_eq!(
        tags::candidate(
            RAW.as_bytes(),
            &plan(Action::AddManual {
                value: "External".into()
            })
        )
        .unwrap_err()
        .code,
        "unsafe-skip"
    );
}
#[test]
fn transaction_binds_plan_backup_and_undo_and_rejects_extra_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let media = base.join("media");
    fs::create_dir(&media).unwrap();
    let path = media.join("电影.nfo");
    fs::write(&path, RAW).unwrap();
    let writer = Writer::new(&base.join("journal"), vec![media]).unwrap();
    let p = generate(RAW.as_bytes());
    let candidate = tags::candidate(RAW.as_bytes(), &p).unwrap();
    let intent = WriteIntent::Tags { plan: p };
    let bad = string(&candidate).replace("中文电影", "tampered");
    assert_eq!(
        writer
            .commit_with_intent(
                "bad",
                &path,
                &hash(RAW.as_bytes()),
                bad.as_bytes(),
                &intent,
                |_| Ok(())
            )
            .unwrap_err()
            .code,
        "unsafe-candidate"
    );
    assert_eq!(fs::read(&path).unwrap(), RAW.as_bytes());
    writer
        .commit_with_intent(
            "write",
            &path,
            &hash(RAW.as_bytes()),
            &candidate,
            &intent,
            |phase| {
                if matches!(phase, Phase::Replaced) {
                    return Err(AppError::new("crash", "injected"));
                }
                Ok(())
            },
        )
        .unwrap_err();
    assert_eq!(writer.inspect("write").unwrap().state, "committed");
    writer
        .commit_with_intent(
            "write",
            &path,
            &hash(RAW.as_bytes()),
            &candidate,
            &intent,
            |_| panic!(),
        )
        .unwrap();
    writer.undo("undo", "write").unwrap();
    assert_eq!(fs::read(&path).unwrap(), RAW.as_bytes());
}
#[test]
fn store_review_mirror_failure_recovery_and_undo_are_one_nfo_write() {
    use itm_core::{store::Store, writing::TagEdit};
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let media = base.join("media");
    fs::create_dir(&media).unwrap();
    let path = media.join("电影.nfo");
    fs::write(&path, RAW).unwrap();
    let db = base.join("state.sqlite");
    let journal = base.join("journal");
    let store = Store::open(&db).unwrap();
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
    let item = store.all_items().unwrap().remove(0);
    let preview = store
        .preview_rules("rules", &item.id, &item.source_hash)
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), RAW.as_bytes());
    // Force a mirror failure after NFO replacement, without relying on root/admin permissions.
    fs::write(base.join("ownership"), b"blocked-directory").unwrap();
    assert_eq!(
        store
            .apply_specs("rules", &preview.after_hash, &journal, || false)
            .unwrap_err()
            .code,
        "ownership-mirror"
    );
    let after = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(hash(&after), preview.after_hash);
    assert_eq!(
        store.write_history().unwrap()[0].phase,
        "committed-mirror-pending"
    );
    drop(store);
    fs::remove_file(base.join("ownership")).unwrap();
    let store = Store::open(&db).unwrap();
    store
        .apply_specs("rules", &preview.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    let mirror = base
        .join("ownership")
        .join(format!("{}.json", hash(path.to_string_lossy().as_bytes())));
    let mirrored: serde_json::Value = serde_json::from_slice(&fs::read(&mirror).unwrap()).unwrap();
    assert_eq!(mirrored["entries"].as_array().unwrap().len(), 1);
    assert_eq!(mirrored["entries"][0]["value"], "New");
    assert_eq!(mirrored["manual_entries"][0]["id"], "manual-id");
    let current = store.item(&item.id).unwrap();
    let edit = store
        .preview_tags(TagEdit {
            operation_id: "edit".into(),
            item_id: item.id.clone(),
            expected_hash: current.source_hash,
            action: Action::Edit {
                root_index: 2,
                value: "Protected".into(),
            },
        })
        .unwrap();
    store
        .apply_specs("edit", &edit.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(
        store.item(&item.id).unwrap().tags[2].ownership,
        Ownership::Manual
    );
    let undo = store.preview_undo("undo-edit", "edit", &journal).unwrap();
    store
        .apply_specs("undo-edit", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), after);
    let undo = store.preview_undo("undo-rules", "rules", &journal).unwrap();
    store
        .apply_specs("undo-rules", &undo.after_hash, &journal, || false)
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), RAW.as_bytes());
    let mirrored: serde_json::Value = serde_json::from_slice(&fs::read(&mirror).unwrap()).unwrap();
    assert_eq!(mirrored["entries"][0]["id"], "old-id");
}
