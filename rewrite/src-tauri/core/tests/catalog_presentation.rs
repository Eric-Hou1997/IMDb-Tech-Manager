use itm_core::{library, store::Store, ui::*, LibraryRoot, Space};
use std::path::Path;

fn item() -> itm_core::MediaItem {
    let root = LibraryRoot {
        id: "movies".into(),
        space: Space::Movie,
        path: "/library".into(),
    };
    library::parse(
        &root,
        Path::new("/library/movie.nfo"),
        b"<movie><title>Sample</title><uniqueid type=\"imdb\">tt0061452</uniqueid></movie>",
    )
    .unwrap()
}
#[test]
fn complete_product_catalog_and_legacy_paging_share_filters_without_dropping_records() {
    let temp = tempfile::tempdir().unwrap();
    let root_path = temp.path().canonicalize().unwrap();
    let before: Vec<_> = (0..127)
        .map(|index| {
            let path = root_path.join(format!("{index}.nfo"));
            let raw = format!("\u{feff}<movie>\r\n<title>电影 {index:03}</title></movie>\r\n");
            std::fs::write(&path, &raw).unwrap();
            let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
            (path, raw, modified)
        })
        .collect();
    let store = Store::open(&root_path.join("index.sqlite")).unwrap();
    store
        .configure(
            "roots",
            itm_core::Configuration {
                revision: 0,
                locale: itm_core::Locale::Simplified,
                roots: vec![LibraryRoot {
                    id: "movies".into(),
                    space: Space::Movie,
                    path: root_path.to_str().unwrap().into(),
                }],
            },
        )
        .unwrap();
    store
        .submit(itm_core::ScanRequest {
            operation_id: "scan".into(),
            space: Space::Movie,
            root_ids: vec!["movies".into()],
        })
        .unwrap();
    store.run_next(|| false, |_| {}).unwrap();
    let view = LibraryView {
        offset: 100,
        ..Default::default()
    };
    let full = store.browse_complete(Space::Movie, view.clone()).unwrap();
    assert_eq!((full.total, full.items.len()), (127, 127));
    assert_eq!(store.browse(Space::Movie, view).unwrap().items.len(), 27);
    let filtered = LibraryView {
        search: "  电影 126  ".into(),
        ..Default::default()
    };
    assert_eq!(
        store
            .browse_complete(Space::Movie, filtered.clone())
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(
        store.catalog_members(Space::Movie, filtered).unwrap().len(),
        1
    );
    for (path, raw, modified) in before {
        assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
        assert_eq!(
            std::fs::metadata(path).unwrap().modified().unwrap(),
            modified
        );
    }
}
#[test]
fn original_episode_show_name_and_display_title_are_read_from_root_fields_only() {
    let root = LibraryRoot {
        id: "tv".into(),
        space: Space::Tv,
        path: "/library".into(),
    };
    let raw = b"<episodedetails><title>First</title><showtitle>Series</showtitle><season>01</season><episode>2</episode><metadata><showtitle>Wrong</showtitle></metadata></episodedetails>";
    let episode = library::parse(&root, Path::new("/library/e2.nfo"), raw).unwrap();
    assert_eq!(episode.series_key, "Series");
    assert_eq!(episode.title, "Series S01E02 First");
    let view = LibraryView {
        search: "  series  ".into(),
        ..Default::default()
    };
    assert!(matches(&episode, &Space::Tv, &view));
    let untitled = library::parse(&root, Path::new("/library/name.nfo"), b"<movie/>").unwrap();
    assert_eq!(untitled.title, "name");
}
#[test]
fn unchanged_episode_with_an_old_parser_cache_reloads_show_name_without_touching_nfo() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().canonicalize().unwrap();
    let nfo = directory.join("episode.nfo");
    let bytes = b"<episodedetails><title>First</title><showtitle>Series</showtitle><season>1</season><episode>2</episode></episodedetails>";
    std::fs::write(&nfo, bytes).unwrap();
    let modified = std::fs::metadata(&nfo).unwrap().modified().unwrap();
    let db_path = directory.join("index.sqlite");
    let store = Store::open(&db_path).unwrap();
    store
        .configure(
            "roots",
            itm_core::Configuration {
                revision: 0,
                locale: itm_core::Locale::Simplified,
                roots: vec![LibraryRoot {
                    id: "tv".into(),
                    space: Space::Tv,
                    path: directory.to_str().unwrap().into(),
                }],
            },
        )
        .unwrap();
    store
        .submit(itm_core::ScanRequest {
            operation_id: "first".into(),
            space: Space::Tv,
            root_ids: vec!["tv".into()],
        })
        .unwrap();
    store.run_next(|| false, |_| {}).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(&db_path).unwrap();
    let body: String = db
        .query_row("SELECT body FROM items", [], |row| row.get(0))
        .unwrap();
    let mut old: serde_json::Value = serde_json::from_str(&body).unwrap();
    old.as_object_mut().unwrap().remove("series_key");
    old["title"] = "First".into();
    old["parser_revision"] = 5.into();
    db.execute(
        "UPDATE items SET body=?1",
        [serde_json::to_string(&old).unwrap()],
    )
    .unwrap();
    drop(db);
    let store = Store::open(&db_path).unwrap();
    let stale = store
        .browse_complete(Space::Tv, LibraryView::default())
        .unwrap()
        .items
        .remove(0);
    assert_eq!(stale.inspection.lifecycle, "index-refresh-required");
    assert!(!matches(
        &stale,
        &Space::Tv,
        &LibraryView {
            catalog_filter: Some(CatalogFilter::Ready),
            ..Default::default()
        }
    ));
    assert!(matches(
        &stale,
        &Space::Tv,
        &LibraryView {
            catalog_filter: Some(CatalogFilter::Error),
            ..Default::default()
        }
    ));
    store
        .reconcile_on_launch("refresh-launch")
        .unwrap()
        .unwrap();
    store.run_next(|| false, |_| {}).unwrap();
    let episode = store
        .browse_complete(Space::Tv, LibraryView::default())
        .unwrap()
        .items
        .remove(0);
    assert_eq!(episode.series_key, "Series");
    assert_eq!(episode.title, "Series S01E02 First");
    assert_eq!(std::fs::read(&nfo).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(&nfo).unwrap().modified().unwrap(),
        modified
    );
}
#[test]
fn dateadded_uses_only_the_root_nfo_field_and_never_file_mtime() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let root = LibraryRoot {
        id: "movies".into(),
        space: Space::Movie,
        path: dir.to_str().unwrap().into(),
    };
    for (index, (fields, expected)) in [
        ("<dateadded>2026-08-20 13:40:20</dateadded>", "2026-08-20"),
        (
            "<dateadded>prefix 2024-03-01 +08:00</dateadded>",
            "2024-03-01",
        ),
        ("<dateadded>2026/08/20</dateadded>", ""),
        ("<metadata><dateadded>2026-08-20</dateadded></metadata>", ""),
        ("", ""),
    ]
    .into_iter()
    .enumerate()
    {
        let path = dir.join(format!("{index}.nfo"));
        let raw = format!(
            "\u{feff}<movie>\r\n<title>电影</title>{fields}<tag>External</tag>\r\n</movie>\r\n"
        );
        std::fs::write(&path, &raw).unwrap();
        let before = std::fs::metadata(&path).unwrap();
        let result = library::read(&root, &path).unwrap();
        assert_eq!(result.added_date, expected);
        assert_eq!(result.title, "电影");
        assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before.modified().unwrap()
        );
    }
}
#[test]
fn original_status_filters_respect_precedence_and_tv_level() {
    let mut item = item();
    let mut view = LibraryView::default();
    for (lifecycle, filter) in [
        ("ai-complete", CatalogFilter::Ai),
        ("local-complete", CatalogFilter::Local),
        ("spec-missing", CatalogFilter::Nospec),
        ("spec-empty", CatalogFilter::Nospec),
        ("no-tags", CatalogFilter::Ready),
        ("xml-error", CatalogFilter::Error),
    ] {
        item.inspection.lifecycle = lifecycle.into();
        view.catalog_filter = Some(filter);
        assert!(matches(&item, &Space::Movie, &view));
        view.catalog_filter = Some(CatalogFilter::NonOptimal);
        assert_eq!(
            matches(&item, &Space::Movie, &view),
            lifecycle != "ai-complete"
        );
    }
    item.inspection.lifecycle = "not-applicable".into();
    view.catalog_filter = Some(CatalogFilter::Ready);
    assert!(!matches(&item, &Space::Movie, &view));
    view.catalog_filter = Some(CatalogFilter::MissingImdb);
    assert!(!matches(&item, &Space::Movie, &view));
    item.imdb.clear();
    assert!(matches(&item, &Space::Movie, &view));
    item.space = Space::Tv;
    item.kind = "Series".into();
    view.catalog_filter = None;
    view.media_level = Some(MediaLevel::Episode);
    assert!(!matches(&item, &Space::Tv, &view));
    item.kind = "Episode".into();
    assert!(matches(&item, &Space::Tv, &view));
}
#[test]
fn date_and_status_sorting_use_the_original_semantic_order() {
    let mut a = item();
    let mut b = a.clone();
    a.id = "a".into();
    b.id = "b".into();
    a.added_date = "2025-04-01".into();
    b.added_date = "2026-04-01".into();
    a.spec_status = "ready".into();
    b.spec_status = "missing".into();
    a.inspection.tag_status = "ai-current".into();
    b.inspection.tag_status = "stale".into();
    for (field, expected) in [
        (Sort::AddedDate, "a"),
        (Sort::SpecsStatus, "b"),
        (Sort::TagsStatus, "b"),
    ] {
        let mut rows = vec![a.clone(), b.clone()];
        let mut view = LibraryView {
            sort: field,
            ..Default::default()
        };
        sort(&mut rows, &view);
        assert_eq!(rows[0].id, expected);
        view.descending = true;
        sort(&mut rows, &view);
        assert_ne!(rows[0].id, expected);
    }
}
#[test]
fn column_preferences_survive_reopen_and_invalid_updates_leave_them_intact() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("columns.sqlite");
    let store = Store::open(&path).unwrap();
    let mut columns = CatalogColumns::default();
    columns.order.swap(1, 4);
    columns.visible.remove(2);
    columns.widths.insert(CatalogColumn::Year, 91);
    columns.compact = true;
    let mut state = store.ui_state().unwrap();
    state.presentation = Some(PresentationState {
        movie_columns: Some(columns.clone()),
        ..Default::default()
    });
    store.save_ui_state("columns", state).unwrap();
    let original = store.ui_state().unwrap();
    for bad in [
        CatalogColumns {
            order: vec![CatalogColumn::Title; 5],
            ..columns.clone()
        },
        CatalogColumns {
            visible: vec![CatalogColumn::Year],
            ..columns.clone()
        },
        CatalogColumns {
            widths: std::collections::BTreeMap::from([(CatalogColumn::Year, 0)]),
            ..columns.clone()
        },
    ] {
        let mut invalid = original.clone();
        invalid.presentation.as_mut().unwrap().movie_columns = Some(bad);
        assert_eq!(
            store
                .save_ui_state("invalid-columns", invalid)
                .unwrap_err()
                .code,
            "view-column-layout"
        );
        assert_eq!(store.ui_state().unwrap(), original);
    }
    drop(store);
    let restored = Store::open(&path).unwrap().ui_state().unwrap();
    assert_eq!(restored.presentation.unwrap().movie_columns, Some(columns));
}
