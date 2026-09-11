use itm_core::{inspector::*, store::Store, *};
use std::fs;
fn fixture() -> (tempfile::TempDir, Store, MediaItem) {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    let nfo = media.join("电影.nfo");
    fs::write(&nfo,b"\xef\xbb\xbf<movie>\r\n<title>Fixture</title><uniqueid type=\"imdb\">tt1234567</uniqueid><tag>External</tag><tag>external</tag>\r\n</movie>\r\n").unwrap();
    let s = Store::open(&root.join("state.sqlite")).unwrap();
    s.configure(
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
    s.submit(ScanRequest {
        operation_id: "scan".into(),
        space: Space::Movie,
        root_ids: vec!["r".into()],
    })
    .unwrap();
    s.run_next(|| false, |_| {}).unwrap();
    let item = s.all_items().unwrap().remove(0);
    (t, s, item)
}
fn request(i: &MediaItem, id: &str, action: AnnotationAction) -> AnnotationRequest {
    AnnotationRequest {
        operation_id: id.into(),
        item_id: i.id.clone(),
        expected_hash: i.source_hash.clone(),
        action,
    }
}
#[test]
fn ignored_issues_and_status_are_hash_bound_and_never_write_media() {
    let (t, s, i) = fixture();
    let raw = fs::read(&i.path).unwrap();
    let mtime = fs::metadata(&i.path).unwrap().modified().unwrap();
    assert_eq!(i.inspection.issues, vec!["spec-missing", "duplicate-tag"]);
    assert_eq!(i.inspection.lifecycle, "spec-missing");
    assert!(i.inspection.bom);
    assert_eq!(i.inspection.newline, "CRLF");
    let ignore = request(
        &i,
        "ignore",
        AnnotationAction::Ignore {
            issue: "spec-missing".into(),
        },
    );
    let saved = s.annotate(ignore.clone()).unwrap();
    assert_eq!(s.annotate(ignore).unwrap(), saved);
    s.annotate(request(
        &i,
        "status",
        AnnotationAction::SetStatus {
            value: "ai-complete".into(),
        },
    ))
    .unwrap();
    let detail = s.inspect_item(&i.id).unwrap();
    assert_eq!(detail.inspection.lifecycle, "ai-complete");
    assert_eq!(detail.inspection.ignored_issues, vec!["spec-missing"]);
    assert_eq!(detail.inspection.issues, vec!["duplicate-tag"]);
    assert_eq!(s.all_items().unwrap()[0].inspection, detail.inspection);
    assert_eq!(fs::read(&i.path).unwrap(), raw);
    assert_eq!(fs::metadata(&i.path).unwrap().modified().unwrap(), mtime);
    drop(s);
    let s = Store::open(&t.path().canonicalize().unwrap().join("state.sqlite")).unwrap();
    assert_eq!(s.inspect_item(&i.id).unwrap().inspection, detail.inspection);
    fs::write(
        &i.path,
        String::from_utf8(raw)
            .unwrap()
            .replace("Fixture", "Changed"),
    )
    .unwrap();
    let changed = s.inspect_item(&i.id).unwrap();
    assert_eq!(changed.inspection.lifecycle, "spec-missing");
    assert!(changed.inspection.ignored_issues.is_empty());
    assert_eq!(
        s.annotate(request(
            &i,
            "stale",
            AnnotationAction::SetStatus {
                value: "ai-complete".into()
            }
        ))
        .unwrap_err()
        .code,
        "source-conflict"
    );
}
#[test]
fn restore_issues_and_clear_status_preserve_the_other_annotation() {
    let (_t, s, i) = fixture();
    s.annotate(request(
        &i,
        "ignore",
        AnnotationAction::Ignore {
            issue: "duplicate-tag".into(),
        },
    ))
    .unwrap();
    s.annotate(request(
        &i,
        "status",
        AnnotationAction::SetStatus {
            value: "review".into(),
        },
    ))
    .unwrap();
    s.annotate(request(&i, "restore", AnnotationAction::RestoreIssues))
        .unwrap();
    let current = s.inspect_item(&i.id).unwrap();
    assert_eq!(current.inspection.lifecycle, "review");
    assert_eq!(current.inspection.issues.len(), 2);
    s.annotate(request(
        &i,
        "clear",
        AnnotationAction::SetStatus {
            value: String::new(),
        },
    ))
    .unwrap();
    assert_eq!(
        s.inspect_item(&i.id).unwrap().inspection.lifecycle,
        "spec-missing"
    );
    assert_eq!(
        s.annotate(request(
            &i,
            "invalid",
            AnnotationAction::SetStatus {
                value: "made-up".into()
            }
        ))
        .unwrap_err()
        .code,
        "invalid-annotation"
    );
}
#[test]
fn safety_issues_cannot_be_ignored_and_bad_xml_stays_inspectable() {
    let (_t, s, i) = fixture();
    for issue in ["xml-error", "read-error", "library-type-mismatch", "absent"] {
        assert_eq!(
            s.annotate(request(
                &i,
                issue,
                AnnotationAction::Ignore {
                    issue: issue.into()
                }
            ))
            .unwrap_err()
            .code,
            "protected-or-missing-issue"
        );
    }
    fs::write(&i.path, b"<movie>broken").unwrap();
    let current = s.inspect_item(&i.id).unwrap();
    assert!(current.error.is_some());
    assert_eq!(current.inspection.lifecycle, "xml-error");
    assert_eq!(current.inspection.issues, vec!["xml-error"]);
    assert_eq!(fs::read(&i.path).unwrap(), b"<movie>broken");
}
#[test]
fn tag_status_priority_and_effective_source_snapshots_match_original_rules() {
    let root = LibraryRoot {
        id: "r".into(),
        space: Space::Movie,
        path: "/media".into(),
    };
    let specs = Specs::from([("Camera".into(), vec!["Arri Alexa".into()])]);
    let digest = specs::fingerprint(&specs).unwrap();
    for (engine, hash, state, tag, want) in [
        ("ai", digest.as_str(), "current", true, "ai-complete"),
        (
            "local-rules",
            digest.as_str(),
            "current",
            true,
            "local-complete",
        ),
        ("ai", "old", "review", true, "stale"),
        ("ai", digest.as_str(), "review", true, "review"),
        ("ai", "old", "current", false, "tag-missing"),
    ] {
        let xml=format!("<movie><uniqueid type=\"imdb\">tt1234567</uniqueid>{}<technicalspecs source=\"IMDb\" imdbid=\"tt1234567\" fetched=\"old\"><section name=\"Camera\"><item>Arri Alexa</item></section><sourcesnapshot fetched=\"new\"><section name=\"Camera\"><item>Source Camera</item></section></sourcesnapshot><generatedtags owner=\"IMDb Tech Manager\" schema=\"2\" engine=\"{engine}\" specHash=\"{hash}\" state=\"{state}\"><tag>Camera: Arri Alexa</tag></generatedtags></technicalspecs></movie>",if tag{"<tag>Camera: Arri Alexa</tag>"}else{""});
        let i =
            library::parse(&root, std::path::Path::new("/media/a.nfo"), xml.as_bytes()).unwrap();
        assert_eq!(i.inspection.lifecycle, want);
        assert_eq!(i.inspection.source_specs["Camera"], vec!["Source Camera"]);
        assert_eq!(i.inspection.source_fetched_at, "new");
    }
}
#[test]
fn wrong_library_category_blocks_generation_even_with_manual_success_status() {
    let (_t, s, i) = fixture();
    let xml=b"<tvshow><uniqueid type=\"imdb\">tt1234567</uniqueid><technicalspecs source=\"IMDb\" imdbid=\"tt1234567\"><section name=\"Camera\"><item>Arri Alexa</item></section></technicalspecs></tvshow>";
    fs::write(&i.path, xml).unwrap();
    let current = s.inspect_item(&i.id).unwrap();
    assert!(current
        .inspection
        .issues
        .contains(&"library-type-mismatch".into()));
    s.annotate(request(
        &current,
        "override",
        AnnotationAction::SetStatus {
            value: "ai-complete".into(),
        },
    ))
    .unwrap();
    assert_eq!(
        s.preview_rules("rules", &i.id, &current.source_hash)
            .unwrap_err()
            .code,
        "library-type-mismatch"
    );
    assert_eq!(
        s.begin_fetch(acquisition::FetchRequest {
            operation_id: "fetch".into(),
            item_id: i.id.clone(),
            expected_hash: current.source_hash.clone(),
            refresh: false
        })
        .unwrap_err()
        .code,
        "library-type-mismatch"
    );
    let mut missing = current;
    missing.spec_status = "missing".into();
    missing.modified_at = 100;
    assert!(automatic::candidates(&[missing], 200, |_| false).is_empty());
    assert_eq!(fs::read(&i.path).unwrap(), xml);
}
#[test]
fn legacy_ack_and_status_keep_independent_revision_hashes_and_destination_cas() {
    let (t, s, i) = fixture();
    let dir = t.path().canonicalize().unwrap().join("legacy");
    fs::create_dir(&dir).unwrap();
    let old_hash = "0".repeat(64);
    let a = serde_json::json!({&i.path:{"source_hash":i.source_hash,"kinds":["duplicate-tag"],"updated_at":"2026-09-11T00:00:00Z"}});
    let b = serde_json::json!({&i.path:{"source_hash":old_hash,"value":"ai-complete","updated_at":"2026-09-11T00:00:00Z"}});
    fs::write(
        dir.join("issue-acknowledgements.json"),
        serde_json::to_vec(&a).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("status-overrides.json"),
        serde_json::to_vec(&b).unwrap(),
    )
    .unwrap();
    let p = s.prepare_migration("import", &dir, "itm-engine").unwrap();
    assert_eq!(p.adapters.len(), 1);
    let before = fs::read(&i.path).unwrap();
    let mtime = fs::metadata(&i.path).unwrap().modified().unwrap();
    s.apply_migration("import", &p.fingerprint).unwrap();
    s.apply_migration("import", &p.fingerprint).unwrap();
    let current = s.inspect_item(&i.id).unwrap();
    assert_eq!(current.inspection.ignored_issues, vec!["duplicate-tag"]);
    assert_eq!(current.inspection.lifecycle, "spec-missing");
    let again = s.prepare_migration("again", &dir, "itm-engine").unwrap();
    s.annotate(request(
        &i,
        "manual",
        AnnotationAction::SetStatus {
            value: "review".into(),
        },
    ))
    .unwrap();
    assert_eq!(
        s.apply_migration("again", &again.fingerprint)
            .unwrap_err()
            .code,
        "migration-configuration-conflict"
    );
    assert_eq!(
        s.inspect_item(&i.id).unwrap().inspection.lifecycle,
        "review"
    );
    assert_eq!(fs::read(&i.path).unwrap(), before);
    assert_eq!(fs::metadata(&i.path).unwrap().modified().unwrap(), mtime);
}
#[test]
fn invalid_legacy_safety_ack_is_archived_without_suppressing_the_issue() {
    let (t, s, i) = fixture();
    let dir = t.path().canonicalize().unwrap().join("legacy");
    fs::create_dir(&dir).unwrap();
    fs::write(
        dir.join("issue-acknowledgements.json"),
        serde_json::to_vec(
            &serde_json::json!({&i.path:{"source_hash":i.source_hash,"kinds":["xml-error"]}}),
        )
        .unwrap(),
    )
    .unwrap();
    let p = s.prepare_migration("import", &dir, "itm-engine").unwrap();
    assert!(p.adapters.is_empty());
    assert!(p
        .warnings
        .iter()
        .any(|w| w.contains("Unverified Inspector")));
    s.apply_migration("import", &p.fingerprint).unwrap();
    assert!(s
        .inspect_item(&i.id)
        .unwrap()
        .inspection
        .ignored_issues
        .is_empty());
}
#[test]
fn list_filters_use_the_same_effective_status_and_issues_as_inspector() {
    let (_t, s, i) = fixture();
    s.annotate(request(
        &i,
        "status",
        AnnotationAction::SetStatus {
            value: "ai-complete".into(),
        },
    ))
    .unwrap();
    let mut view = ui::LibraryView {
        lifecycle: "ai-complete".into(),
        issues: true,
        ..Default::default()
    };
    assert_eq!(s.browse(Space::Movie, view.clone()).unwrap().total, 1);
    for (n, kind) in ["spec-missing", "duplicate-tag"].iter().enumerate() {
        s.annotate(request(
            &i,
            &format!("ignore-{n}"),
            AnnotationAction::Ignore {
                issue: (*kind).into(),
            },
        ))
        .unwrap();
    }
    assert_eq!(s.browse(Space::Movie, view.clone()).unwrap().total, 0);
    view.issues = false;
    assert_eq!(s.browse(Space::Movie, view).unwrap().total, 1);
}
#[test]
fn misplaced_nfo_is_indexed_and_locatable_instead_of_silently_hidden() {
    let (_t, s, i) = fixture();
    let path = std::path::Path::new(&i.path)
        .parent()
        .unwrap()
        .join("tvshow.nfo");
    fs::write(
        &path,
        b"<tvshow><title>Misplaced</title><uniqueid type=\"imdb\">tt1234567</uniqueid></tvshow>",
    )
    .unwrap();
    s.submit(ScanRequest {
        operation_id: "rescan".into(),
        space: Space::Movie,
        root_ids: vec!["r".into()],
    })
    .unwrap();
    let task = s.run_next(|| false, |_| {}).unwrap().unwrap();
    assert_eq!(task.processed, 2);
    assert_eq!(task.errors, 1);
    let all = s.all_items().unwrap();
    assert_eq!(all.len(), 2);
    let wrong = all.iter().find(|i| i.title == "Misplaced").unwrap();
    assert_eq!(wrong.space, Space::Movie);
    assert!(wrong
        .inspection
        .issues
        .contains(&"library-type-mismatch".into()));
    assert!(s
        .inspect_item(&wrong.id)
        .unwrap()
        .inspection
        .issues
        .contains(&"library-type-mismatch".into()));
}
#[test]
fn old_index_is_not_misreported_as_corrupt_xml_before_refresh() {
    let (_t, _s, mut i) = fixture();
    i.parser_revision = 3;
    *i.inspection = Default::default();
    apply(&mut i, None);
    assert_eq!(i.inspection.lifecycle, "index-refresh-required");
    assert!(!i.inspection.issues.contains(&"xml-error".into()));
}
#[test]
fn initially_broken_file_keeps_a_queryable_inspector_id_and_recovery_does_not_duplicate_rows() {
    let (_t, s, i) = fixture();
    let root = std::path::Path::new(&i.path).parent().unwrap();
    let broken = root.join("broken.nfo");
    let other = root.join("other.nfo");
    fs::write(&broken, b"<movie>bad").unwrap();
    fs::write(&other, b"<movie>bad").unwrap();
    s.submit(ScanRequest {
        operation_id: "bad-scan".into(),
        space: Space::Movie,
        root_ids: vec!["r".into()],
    })
    .unwrap();
    s.run_next(|| false, |_| {}).unwrap();
    let first = s
        .all_items()
        .unwrap()
        .into_iter()
        .find(|i| i.path == broken.to_str().unwrap())
        .unwrap();
    let detail = s.inspect_item(&first.id).unwrap();
    assert_eq!(detail.id, first.id);
    assert!(s.inspect_item(&detail.id).unwrap().error.is_some());
    fs::write(&broken, b"<movie><title>Recovered</title></movie>").unwrap();
    s.submit(ScanRequest {
        operation_id: "recovery-scan".into(),
        space: Space::Movie,
        root_ids: vec!["r".into()],
    })
    .unwrap();
    s.run_next(|| false, |_| {}).unwrap();
    let all = s.all_items().unwrap();
    assert_eq!(
        all.iter()
            .filter(|i| i.path == broken.to_str().unwrap())
            .count(),
        1
    );
    let recovered = all.iter().find(|i| i.title == "Recovered").unwrap();
    assert!(s.inspect_item(&recovered.id).unwrap().error.is_none());
    assert!(all
        .iter()
        .any(|i| i.path == other.to_str().unwrap() && i.error.is_some()));
}
#[test]
fn changing_library_category_after_review_still_blocks_generation_at_commit() {
    let (t, s, i) = fixture();
    fs::write(&i.path,b"<movie><uniqueid type=\"imdb\">tt1234567</uniqueid><technicalspecs source=\"IMDb\" imdbid=\"tt1234567\"><section name=\"Camera\"><item>Arri Alexa</item></section></technicalspecs></movie>").unwrap();
    let i = s.inspect_item(&i.id).unwrap();
    let raw = fs::read(&i.path).unwrap();
    let preview = s.preview_rules("rules", &i.id, &i.source_hash).unwrap();
    let mut config = s.configuration().unwrap();
    config.roots[0].space = Space::Tv;
    s.configure("reclassify", config).unwrap();
    assert_eq!(
        s.apply_specs(
            "rules",
            &preview.after_hash,
            &t.path().canonicalize().unwrap().join("journal"),
            || false
        )
        .unwrap_err()
        .code,
        "library-type-mismatch"
    );
    assert_eq!(fs::read(&i.path).unwrap(), raw);
}
