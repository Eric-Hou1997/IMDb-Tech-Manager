use itm_core::{
    migration::MigrationReceipt,
    store::{StartupSources, Store},
    Configuration, Locale, OperationResult, Space,
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    sources: StartupSources,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let sources = StartupSources {
            manager: root.join("IMDb Tech Manager"),
            engine: root.join("tmm-imdb-tech"),
        };
        fs::create_dir(&sources.manager).unwrap();
        fs::create_dir(&sources.engine).unwrap();
        let store = Store::open(&root.join("workspace.sqlite")).unwrap();
        Self {
            _temp: temp,
            root,
            sources,
            store,
        }
    }
    fn save(&self, folder: &Path, name: &str, value: serde_json::Value) {
        fs::write(
            folder.join(name),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
    fn defaults(&self) {
        self.save(&self.sources.manager, "settings.json", json!({"language":"en-US","interval_seconds":60,"auto_mode_on_app_start":true,"auto_mode_on_app_start_configured":true}));
        self.save(&self.sources.engine, "config.json", json!({"ai":{"enabled":false,"base_url":"","model":"","prompt":"  原有提示词\r\n保持原样  "}}));
    }
}

#[test]
fn original_server_layout_restores_split_drawer_columns_and_independent_sorts() {
    use itm_core::ui::{CatalogColumn, Sort};
    let f = Fixture::new();
    f.defaults();
    let value = json!({"schema_version":1,"revision":17,"split_ratio":49.257,"task_center":{"open":true,"cli_height_px":315.25},"catalog":{"movies":{"sort":{"field":"year","direction":"desc"},"columns":{"order":["year","title","tag_status"],"visible":["year","title"],"widthMode":"pixels-current","widths":{"title":415,"year":95},"compact":true}},"tv":{"sort":{"field":"spec_status","direction":"desc"},"columns":{"visible":["title","year"],"widthMode":"old-adaptive","widths":{"title":300}}}}});
    f.save(&f.sources.manager, "ui-layout.json", value.clone());
    let receipts = restore(&f);
    assert!(receipts[1]
        .applied_adapters
        .contains(&"library-view".to_owned()));
    let state = f.store.ui_state().unwrap();
    assert_eq!(state.movie.sort, Sort::Year);
    assert!(state.movie.descending);
    assert_eq!(state.tv.sort, Sort::Title);
    assert!(!state.tv.descending);
    let p = state.presentation.unwrap();
    assert!((p.split_basis_points - 4925.7).abs() < 0.000001);
    assert_eq!(p.task_height, 315.25);
    assert!(p.task_open);
    let columns = p.movie_columns.unwrap();
    assert_eq!(columns.order[0], CatalogColumn::Title);
    assert_eq!(columns.order.len(), 5);
    assert_eq!(columns.widths[&CatalogColumn::Title], 415);
    assert!(columns.compact);
    assert!(p.tv_columns.unwrap().widths.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(f.sources.manager.join("ui-layout.json")).unwrap()
        )
        .unwrap(),
        value
    );
}
fn restore(f: &Fixture) -> Vec<MigrationReceipt> {
    f.store
        .restore_legacy_on_start(&f.sources, &|| false)
        .unwrap()
        .unwrap()
}

#[test]
fn original_split_settings_become_effective_once_without_touching_legacy_or_nfo_bytes() {
    let f = Fixture::new();
    f.defaults();
    let media = f.root.join("Movies");
    fs::create_dir(&media).unwrap();
    let nfo = media.join("movie.nfo");
    let bytes = b"\xef\xbb\xbf<movie><title>Original</title></movie>\r\n";
    fs::write(&nfo, bytes).unwrap();
    f.save(&f.sources.engine,"config.json",json!({"library_roots":{"movies":[media],"tv":[f.root.join("offline-tv")]},"roots":[media,f.root.join("offline-tv"),f.root.join("unassigned")],"language":"zh-CN","ai":{"enabled":false,"base_url":"","model":"","prompt":"  原有提示词\r\n保持原样  "}}));
    let protected: Vec<_> = [
        f.sources.engine.join("config.json"),
        f.sources.manager.join("settings.json"),
        nfo.clone(),
    ]
    .iter()
    .map(|p| {
        (
            p.clone(),
            fs::read(p).unwrap(),
            fs::metadata(p).unwrap().modified().unwrap(),
        )
    })
    .collect();
    let receipts = restore(&f);
    assert_eq!(receipts.len(), 2);
    let config = f.store.configuration().unwrap();
    assert_eq!(config.locale, Locale::English);
    assert_eq!(config.roots.len(), 2);
    assert!(config
        .roots
        .iter()
        .any(|r| r.space == Space::Tv && r.path.ends_with("offline-tv")));
    assert_eq!(f.store.pending_legacy_roots().unwrap().len(), 1);
    assert_eq!(
        f.store.ai_settings().unwrap().config.prompt,
        "  原有提示词\r\n保持原样  "
    );
    assert!(f.store.automatic_status().unwrap().settings.on_app_start);
    assert!(!f.store.automatic_status().unwrap().enabled);
    assert!(f.store.tasks().unwrap().is_empty());
    assert!(f.store.all_items().unwrap().is_empty());
    assert!(f.store.ai_history(None).unwrap().is_empty());
    for (path, bytes, mtime) in protected {
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), mtime);
    }
}

#[test]
fn second_directory_changes_roll_back_both_archives_adapters_and_receipts() {
    let f = Fixture::new();
    f.defaults();
    let plans = f
        .store
        .prepare_startup_migration(&f.sources, &|| false)
        .unwrap();
    fs::write(f.sources.manager.join("settings.json"), b"{changed}").unwrap();
    assert!(f.store.apply_startup_migration(&plans, &|| false).is_err());
    assert_eq!(f.store.configuration().unwrap(), Configuration::default());
    assert_eq!(
        f.store.ai_settings().unwrap().config.prompt,
        itm_core::ai::DEFAULT_PROMPT
    );
    assert!(f.store.startup_import_receipts().unwrap().is_none());
    for plan in &plans {
        assert!(f
            .store
            .legacy_artifact(&plan.id, &plan.files[0].relative)
            .is_err());
        assert!(matches!(
            f.store.operation_result(&plan.id).unwrap(),
            OperationResult::MigrationPlan(_)
        ));
    }
    f.defaults();
    assert_eq!(restore(&f).len(), 2);
}

#[test]
fn failed_primary_settings_retry_uses_repaired_files_and_never_substitutes_defaults() {
    let f = Fixture::new();
    f.defaults();
    fs::write(f.sources.manager.join("settings.json"), b"{interrupted").unwrap();
    assert_eq!(
        f.store
            .restore_legacy_on_start(&f.sources, &|| false)
            .unwrap_err()
            .code,
        "migration-settings-invalid"
    );
    assert_eq!(f.store.configuration().unwrap().revision, 0);
    assert!(f.store.startup_import_receipts().unwrap().is_none());
    f.defaults();
    restore(&f);
    assert_eq!(f.store.configuration().unwrap().locale, Locale::English);
}

#[test]
fn cancellation_during_apply_rolls_back_and_a_new_attempt_remains_possible() {
    let f = Fixture::new();
    f.defaults();
    let plans = f
        .store
        .prepare_startup_migration(&f.sources, &|| false)
        .unwrap();
    let calls = AtomicUsize::new(0);
    assert_eq!(
        f.store
            .apply_startup_migration(&plans, &|| calls.fetch_add(1, Ordering::SeqCst) > 0)
            .unwrap_err()
            .code,
        "startup-cancelled"
    );
    assert_eq!(f.store.configuration().unwrap().revision, 0);
    assert!(f.store.startup_import_receipts().unwrap().is_none());
    assert!(f
        .store
        .legacy_artifact(&plans[0].id, "config.json")
        .is_err());
    restore(&f);
}

#[test]
fn completed_receipt_survives_restart_and_missing_sources_without_overwriting_new_choices() {
    let f = Fixture::new();
    f.defaults();
    restore(&f);
    let mut config = f.store.configuration().unwrap();
    config.locale = Locale::Traditional;
    let saved = f.store.configure("later-choice", config).unwrap();
    fs::remove_dir_all(&f.sources.manager).unwrap();
    fs::remove_dir_all(&f.sources.engine).unwrap();
    let db = f.root.join("workspace.sqlite");
    let sources = f.sources;
    drop(f.store);
    let store = Store::open(&db).unwrap();
    assert_eq!(
        store
            .restore_legacy_on_start(&sources, &|| panic!(
                "Completed import must not read source"
            ))
            .unwrap()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(store.configuration().unwrap(), saved);
}

#[test]
fn used_destination_and_interleaved_user_save_never_receive_automatic_import() {
    let f = Fixture::new();
    f.defaults();
    let plans = f
        .store
        .prepare_startup_migration(&f.sources, &|| false)
        .unwrap();
    let saved = f
        .store
        .configure("user-settings", Configuration::default())
        .unwrap();
    assert_eq!(
        f.store
            .apply_startup_migration(&plans, &|| false)
            .unwrap_err()
            .code,
        "migration-destination-in-use"
    );
    fs::write(f.sources.engine.join("config.json"), b"unreadable settings").unwrap();
    assert!(f
        .store
        .restore_legacy_on_start(&f.sources, &|| panic!(
            "Used workspace must not inspect sources"
        ))
        .unwrap()
        .is_none());
    assert_eq!(f.store.configuration().unwrap(), saved);
}

#[test]
fn conflicting_profiles_fail_before_any_partial_import() {
    let f = Fixture::new();
    f.defaults();
    f.save(
        &f.sources.manager,
        "settings.json",
        json!({"ai":{"enabled":false,"base_url":"","model":"","prompt":"another profile"}}),
    );
    assert_eq!(
        f.store
            .restore_legacy_on_start(&f.sources, &|| false)
            .unwrap_err()
            .code,
        "migration-ambiguous-settings"
    );
    assert_eq!(f.store.configuration().unwrap().revision, 0);
    assert!(f.store.startup_import_receipts().unwrap().is_none());
}

#[test]
fn offline_classified_roots_can_be_retained_or_removed_and_pending_roots_require_confirmation() {
    let f = Fixture::new();
    f.defaults();
    let offline = f.root.join("offline-media");
    f.save(&f.sources.engine,"config.json",json!({"library_roots":{"movies":[offline],"tv":[]},"roots":[offline,f.root.join("old-unassigned")]}));
    restore(&f);
    let mut config = f.store.configuration().unwrap();
    assert_eq!(config.roots.len(), 1);
    assert!(
        !f.store
            .test_library_root(config.roots[0].path.as_str())
            .unwrap()
            .online
    );
    config.locale = Locale::Traditional;
    f.store.configure("locale", config).unwrap();
    assert_eq!(f.store.pending_legacy_roots().unwrap().len(), 1);
    let config = f.store.configuration().unwrap();
    let saved = f
        .store
        .confirm_library_roots("confirm", config.clone())
        .unwrap();
    assert_eq!(saved.roots, config.roots);
    assert!(f.store.pending_legacy_roots().unwrap().is_empty());
    assert_eq!(
        f.store.confirm_library_roots("confirm", config).unwrap(),
        saved
    );
    let mut removed = saved;
    removed.roots.clear();
    f.store.confirm_library_roots("remove", removed).unwrap();
    assert!(f.store.configuration().unwrap().roots.is_empty());
}

#[test]
fn missing_sources_are_a_fresh_start_but_ambiguous_paths_and_secrets_fail_closed() {
    let f = Fixture::new();
    fs::remove_dir(&f.sources.manager).unwrap();
    fs::remove_dir(&f.sources.engine).unwrap();
    assert!(f
        .store
        .restore_legacy_on_start(&f.sources, &|| false)
        .unwrap()
        .is_none());
    fs::create_dir(&f.sources.engine).unwrap();
    fs::write(
        f.sources.engine.join("config.json"),
        br#"{"ai":{"api_key":"fixture-only-secret"}}"#,
    )
    .unwrap();
    let error = f
        .store
        .restore_legacy_on_start(&f.sources, &|| false)
        .unwrap_err();
    assert_eq!(error.code, "migration-credential-boundary");
    assert!(!error.message.contains("fixture-only-secret"));
    assert_eq!(f.store.configuration().unwrap().revision, 0);
}

#[cfg(unix)]
#[test]
fn offline_paths_never_authorize_symlink_ancestors_or_automated_nfo_access() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.defaults();
    let outside = f.root.join("outside");
    fs::create_dir(&outside).unwrap();
    let link = f.root.join("link");
    symlink(&outside, &link).unwrap();
    f.save(
        &f.sources.engine,
        "config.json",
        json!({"library_roots":{"movies":[link.join("not-present")],"tv":[]}}),
    );
    restore(&f);
    assert!(f.store.configuration().unwrap().roots.is_empty());
    assert_eq!(f.store.pending_legacy_roots().unwrap().len(), 1);
    assert_eq!(
        itm_core::paths::checked_or_missing(&link.join("not-present"))
            .unwrap_err()
            .code,
        "ambiguous-path"
    );
    assert!(f.store.tasks().unwrap().is_empty());
}
