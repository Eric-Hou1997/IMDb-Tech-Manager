use itm_core::{
    onboarding,
    store::{StartupSources, Store},
    Configuration, Locale,
};
use serde_json::json;
use std::fs;
#[test]
fn large_original_tmm_documents_keep_datasource_discovery_without_media_changes() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let media = root.join("Movies");
    fs::create_dir(&media).unwrap();
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    let target = data.join("movies.json");
    let mut file = fs::File::create(&target).unwrap();
    file.write_all(b"{\"unrelated_metadata\":\"").unwrap();
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..17 {
        file.write_all(&chunk).unwrap();
    }
    file.write_all(b"\",\"datasource\":").unwrap();
    file.write_all(serde_json::to_string(&media).unwrap().as_bytes())
        .unwrap();
    file.write_all(b"}").unwrap();
    file.sync_all().unwrap();
    let modified = fs::metadata(&target).unwrap().modified().unwrap();
    let candidates = itm_core::onboarding::candidates(&data);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].path, media.to_string_lossy());
    assert_eq!(candidates[0].suggested_space, "movies");
    assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), modified);
    assert!(fs::read_dir(&media).unwrap().next().is_none());
}
#[test]
fn datasource_lists_collect_all_direct_paths_before_descending_into_nested_keys() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().canonicalize().unwrap();
    let movie = data.join("Movies");
    let tv = data.join("TV");
    let second = data.join("Film");
    for path in [&movie, &tv, &second] {
        fs::create_dir(path).unwrap();
    }
    fs::write(
        data.join("movies.json"),
        serde_json::to_vec(&json!({"datasource":[{"datasource":tv},[movie,[second]]]})).unwrap(),
    )
    .unwrap();
    let found = onboarding::candidates(&data);
    assert_eq!(
        found.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(),
        vec![
            movie.to_str().unwrap(),
            second.to_str().unwrap(),
            tv.to_str().unwrap()
        ]
    );
}

#[test]
fn tmm_candidates_are_original_ordered_read_only_suggestions_until_explicit_confirmation() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().canonicalize().unwrap();
    let data = root.join("TMM-data");
    fs::create_dir(&data).unwrap();
    let media: Vec<_> = ["Movie", "TV-Series", "Unclassified"]
        .iter()
        .map(|name| {
            let p = root.join(name);
            fs::create_dir(&p).unwrap();
            p
        })
        .collect();
    let movie = serde_json::to_string(&media[0]).unwrap();
    let tv = serde_json::to_string(&media[1]).unwrap();
    let extra = serde_json::to_string(&media[2]).unwrap();
    // z before a proves the reader did not sort object keys for discovery.
    let bytes=format!(r#"{{"zDataSource":[{tv}],"aDataSource":[{movie},{tv}],"nested":{{"data-source":{extra}}},"credential":"fixture-only","ignored":true}}"#).into_bytes();
    fs::write(data.join("movies.json"), &bytes).unwrap();
    fs::write(
        data.join("tvshows.json"),
        format!(r#"{{"datasources":[{tv},"/offline","relative"]}}"#),
    )
    .unwrap();
    let modified = fs::metadata(data.join("movies.json"))
        .unwrap()
        .modified()
        .unwrap();
    let store = Store::open(&root.join("new.sqlite")).unwrap();
    let info = store.onboarding_info(|| Some(data.clone())).unwrap();
    assert!(info.onboarding_required);
    assert!(!info.library_roots_confirmed);
    assert_eq!(
        info.candidates
            .iter()
            .map(|c| c.suggested_space.as_str())
            .collect::<Vec<_>>(),
        vec!["tv", "movies", "unassigned"]
    );
    assert_eq!(store.configuration().unwrap(), Configuration::default());
    assert!(store.tasks().unwrap().is_empty());
    assert!(store.all_items().unwrap().is_empty());
    assert!(!store.automatic_status().unwrap().enabled);
    assert_eq!(fs::read(data.join("movies.json")).unwrap(), bytes);
    assert_eq!(
        fs::metadata(data.join("movies.json"))
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
}

#[test]
fn missing_malformed_or_partial_optional_settings_allow_manual_setup_without_partial_candidates() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().canonicalize().unwrap();
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    let media = root.join("Movie");
    fs::create_dir(&media).unwrap();
    assert!(onboarding::candidates(&data).is_empty());
    let path = serde_json::to_string(&media).unwrap();
    for bytes in [
        format!(r#"{{"datasources":[{path}],"invalid": "#),
        format!(r#"{{"datasources":[{path}]}} trailing"#),
    ] {
        fs::write(data.join("movies.json"), bytes).unwrap();
        assert!(onboarding::candidates(&data).is_empty());
    }
}

#[test]
fn locale_changes_do_not_confirm_fresh_roots_and_explicit_empty_settings_save_does() {
    let t = tempfile::tempdir().unwrap();
    let store = Store::open(&t.path().join("state.sqlite")).unwrap();
    let mut configuration = store.configuration().unwrap();
    configuration.locale = Locale::English;
    let changed = store.configure("language", configuration).unwrap();
    assert!(!store.library_roots_confirmed().unwrap());
    store
        .confirm_library_roots("explicit-empty", changed)
        .unwrap();
    assert!(store
        .onboarding_info(|| panic!("Confirmed setup must not read optional TMM settings"))
        .unwrap()
        .candidates
        .is_empty());
    assert!(store.tasks().unwrap().is_empty());
}

#[test]
fn returning_legacy_unassigned_and_explicit_empty_installations_keep_confirmation() {
    for value in [
        json!({"roots":["/offline-old"]}),
        json!({"library_roots_confirmed":true,"library_roots":{"movies":[],"tv":[]}}),
    ] {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().canonicalize().unwrap();
        let sources = StartupSources {
            manager: root.join("missing-manager"),
            engine: root.join("engine"),
        };
        fs::create_dir(&sources.engine).unwrap();
        fs::write(
            sources.engine.join("config.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let store = Store::open(&root.join("state.sqlite")).unwrap();
        store.restore_legacy_on_start(&sources, &|| false).unwrap();
        assert!(
            !store
                .onboarding_info(|| panic!("Old confirmation must skip discovery"))
                .unwrap()
                .onboarding_required
        );
        assert!(store.tasks().unwrap().is_empty());
    }
}

#[test]
fn original_confirmation_rejects_one_folder_in_both_spaces_without_marking_setup_complete() {
    use itm_core::{LibraryRoot, Space};
    let t = tempfile::tempdir().unwrap();
    let root = t.path().canonicalize().unwrap();
    let media = root.join("Media");
    fs::create_dir(&media).unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let roots = vec![
        LibraryRoot {
            id: "movie".into(),
            space: Space::Movie,
            path: media.to_string_lossy().into(),
        },
        LibraryRoot {
            id: "tv".into(),
            space: Space::Tv,
            path: media.to_string_lossy().into(),
        },
    ];
    assert_eq!(
        store
            .confirm_library_roots(
                "confirm",
                Configuration {
                    roots,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
        "overlapping-roots"
    );
    assert_eq!(store.configuration().unwrap().revision, 0);
    assert!(!store.library_roots_confirmed().unwrap());
    assert!(store.tasks().unwrap().is_empty());
}
