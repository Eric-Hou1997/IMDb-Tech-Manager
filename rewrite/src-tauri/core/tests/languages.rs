use itm_core::{languages::*, store::Store, *};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
fn fixture(change: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) -> (Catalog, Vec<u8>) {
    let mut catalog = Catalog::previous_installation().unwrap();
    let descriptor = &catalog.languages["fr-FR"];
    let mut files: BTreeMap<String, Vec<u8>> = ["core", "engine", "native", "web"]
        .into_iter()
        .map(|s| {
            (
                format!("{s}.json"),
                r#"{"legacy.0123456789abcdef":"Français"}"#.as_bytes().to_vec(),
            )
        })
        .collect();
    let hashes: BTreeMap<_, _> = files.iter().map(|(k, v)| (k.clone(), hash(v))).collect();
    files.insert("manifest.json".into(),serde_json::to_vec(&serde_json::json!({"schema":1,"product":"itm","locale":"fr-FR","revision":descriptor.revision,"released_with":descriptor.released_with,"catalog_schema":1,"message_set_hash":descriptor.message_set_hash,"files":hashes})).unwrap());
    change(&mut files);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    catalog.languages.get_mut("fr-FR").unwrap().sha256 = hash(&bytes);
    (catalog, bytes)
}
#[test]
fn startup_restores_only_selected_or_previously_installed_external_languages() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let (catalog, bytes) = fixture(|_| {});
    assert!(store
        .language_restore_candidates(&catalog)
        .unwrap()
        .is_empty());
    let old = root.join("old");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("settings.json"), br#"{"language":"fr-FR"}"#).unwrap();
    for path in [
        "Language Packs/ja-JP/r7/manifest.json",
        "Language Packs/ru-RU/r0/manifest.json",
        "Language Packs/es-ES/revision/manifest.json",
    ] {
        let path = old.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"{}").unwrap();
    }
    let plan = store
        .prepare_migration("old-language", &old, "itm-manager")
        .unwrap();
    store
        .apply_migration("old-language", &plan.fingerprint)
        .unwrap();
    assert_eq!(
        store.language_restore_candidates(&catalog).unwrap(),
        vec!["fr-FR", "ja-JP"]
    );
    store
        .install_legacy_language_pack(&catalog, "fr-FR", &bytes)
        .unwrap();
    assert_eq!(store.configuration().unwrap().locale, Locale::French);
    assert_eq!(
        store.language_restore_candidates(&catalog).unwrap(),
        vec!["ja-JP"]
    );
    store.select_ui_language(&catalog, Locale::English).unwrap();
    let key = format!("language-zip:{}", catalog.languages["fr-FR"].sha256);
    store
        .save_preference(&key, &serde_json::json!("damaged"))
        .unwrap();
    assert_eq!(
        store.language_restore_candidates(&catalog).unwrap(),
        vec!["fr-FR", "ja-JP"]
    );
    drop(store);
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    assert_eq!(
        store.language_restore_candidates(&catalog).unwrap(),
        vec!["fr-FR", "ja-JP"]
    );
    assert_eq!(store.configuration().unwrap().locale, Locale::English);
}
#[test]
fn installation_history_and_zip_cache_commit_together() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let (catalog, bytes) = fixture(|_| {});
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_language_history BEFORE INSERT ON preferences WHEN NEW.key='language-install-history:fr-FR' BEGIN SELECT RAISE(ABORT,'fixture write failure'); END;").unwrap();
    assert!(store
        .install_legacy_language_pack(&catalog, "fr-FR", &bytes)
        .is_err());
    assert!(store
        .legacy_language_pack(&catalog, "fr-FR")
        .unwrap()
        .is_none());
    assert!(store
        .language_restore_candidates(&catalog)
        .unwrap()
        .is_empty());
    sql.execute_batch("DROP TRIGGER reject_language_history")
        .unwrap();
    store
        .install_legacy_language_pack(&catalog, "fr-FR", &bytes)
        .unwrap();
    assert_eq!(
        store.preferences("language-install-history:fr-FR").unwrap(),
        serde_json::json!(true)
    );
}
fn extracted(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        files.insert(file.name().into(), bytes);
    }
    files
}
fn write_installed(root: &std::path::Path, prefix: &str, files: &BTreeMap<String, Vec<u8>>) {
    let directory = root.join(prefix);
    std::fs::create_dir_all(&directory).unwrap();
    for (name, bytes) in files {
        std::fs::write(directory.join(name), bytes).unwrap();
    }
}
#[test]
fn imported_installed_files_are_usable_without_the_old_source_and_without_repacking() {
    for prefix in ["Language Packs/fr-FR/r1", "data/Language Packs/fr-FR/r1"] {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().canonicalize().unwrap().join("old");
        std::fs::create_dir(&old).unwrap();
        let (catalog, bytes) = fixture(|_| {});
        let files = extracted(&bytes);
        write_installed(&old, prefix, &files);
        std::fs::write(old.join("settings.json"), br#"{"language":"fr-FR"}"#).unwrap();
        let db = temp.path().canonicalize().unwrap().join("state.sqlite");
        let store = Store::open(&db).unwrap();
        let plan = store
            .prepare_migration("import", &old, "itm-manager")
            .unwrap();
        let result = store.apply_migration("import", &plan.fingerprint).unwrap();
        assert_eq!(result.configuration.locale, Locale::French);
        let expected = catalog.decode_installed_files("fr-FR", &files).unwrap();
        assert_eq!(
            store
                .legacy_language_pack(&catalog, "fr-FR")
                .unwrap()
                .unwrap(),
            expected
        );
        for (name, body) in &files {
            let path = format!("{prefix}/{name}");
            assert_eq!(store.legacy_artifact("import", &path).unwrap(), *body);
            assert_eq!(std::fs::read(old.join(path)).unwrap(), *body);
        }
        std::fs::rename(&old, temp.path().canonicalize().unwrap().join("offline")).unwrap();
        drop(store);
        let store = Store::open(&db).unwrap();
        assert_eq!(
            store
                .legacy_language_pack(&catalog, "fr-FR")
                .unwrap()
                .unwrap(),
            expected
        );
        assert_eq!(
            store
                .select_ui_language(&catalog, Locale::French)
                .unwrap()
                .locale,
            Locale::French
        );
        // Installed-file reuse does not fabricate a verified release ZIP cache.
        assert_eq!(
            store
                .preferences(&format!(
                    "language-zip:{}",
                    catalog.languages["fr-FR"].sha256
                ))
                .unwrap(),
            serde_json::json!({})
        );
    }
}
#[test]
fn incomplete_newer_import_cannot_borrow_files_from_an_older_pack_and_verified_download_can_repair_it(
) {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().canonicalize().unwrap().join("old");
    let (catalog, bytes) = fixture(|_| {});
    let files = extracted(&bytes);
    write_installed(&old, "Language Packs/fr-FR/r1", &files);
    let store = Store::open(&temp.path().canonicalize().unwrap().join("state.sqlite")).unwrap();
    let plan = store
        .prepare_migration("first", &old, "itm-manager")
        .unwrap();
    store.apply_migration("first", &plan.fingerprint).unwrap();
    let partial = temp.path().canonicalize().unwrap().join("partial");
    let mut incomplete = files.clone();
    incomplete.remove("web.json");
    write_installed(&partial, "Language Packs/fr-FR/r1", &incomplete);
    let plan = store
        .prepare_migration("second", &partial, "itm-manager")
        .unwrap();
    store.apply_migration("second", &plan.fingerprint).unwrap();
    assert_eq!(
        store
            .legacy_language_pack(&catalog, "fr-FR")
            .unwrap_err()
            .code,
        "language-archive-incomplete"
    );
    assert!(store.select_ui_language(&catalog, Locale::French).is_err());
    store
        .install_legacy_language_pack(&catalog, "fr-FR", &bytes)
        .unwrap();
    assert!(store
        .legacy_language_pack(&catalog, "fr-FR")
        .unwrap()
        .is_some());
    assert_eq!(
        store
            .legacy_artifact("first", "Language Packs/fr-FR/r1/web.json")
            .unwrap(),
        files["web.json"]
    );
}
#[test]
fn ambiguous_directories_and_corrupted_archive_bytes_are_not_accepted_as_installed() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().canonicalize().unwrap().join("old");
    let (catalog, bytes) = fixture(|_| {});
    let files = extracted(&bytes);
    write_installed(&old, "Language Packs/fr-FR/r1", &files);
    write_installed(&old, "data/Language Packs/fr-FR/r1", &files);
    let path = temp.path().canonicalize().unwrap().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let plan = store
        .prepare_migration("ambiguous", &old, "itm-manager")
        .unwrap();
    store
        .apply_migration("ambiguous", &plan.fingerprint)
        .unwrap();
    assert_eq!(
        store
            .legacy_language_pack(&catalog, "fr-FR")
            .unwrap_err()
            .code,
        "language-archive-ambiguous"
    );
    let clean = temp.path().canonicalize().unwrap().join("clean");
    write_installed(&clean, "Language Packs/fr-FR/r1", &files);
    let plan = store
        .prepare_migration("latest", &clean, "itm-manager")
        .unwrap();
    store.apply_migration("latest", &plan.fingerprint).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute("UPDATE legacy_artifacts SET body=?1 WHERE import_id='latest' AND path='Language Packs/fr-FR/r1/web.json'",[b"{}".as_slice()]).unwrap();
    assert_eq!(
        store
            .legacy_language_pack(&catalog, "fr-FR")
            .unwrap_err()
            .code,
        "language-archive-integrity"
    );
}
#[test]
fn baseline_catalog_binds_five_external_zip_assets_and_the_original_eight_options() {
    let catalog = Catalog::embedded().unwrap();
    assert_eq!(catalog.languages.len(), 5);
    let options = options();
    assert_eq!(options.len(), 8);
    assert_eq!(options[2].native_name, "English (United States)");
    assert_eq!(options[0].flag, options[1].flag);
    for option in options {
        if option.built_in {
            assert!(catalog.url(&option.code).is_err());
        } else {
            assert_eq!(catalog.url(&option.code).unwrap(),format!("https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/download/v5.0.0/ITM-Language-{}-r2.zip",option.code));
        }
    }
}
#[test]
fn zip_identity_hash_file_set_and_each_section_digest_must_match_before_use() {
    let (catalog, bytes) = fixture(|_| {});
    assert_eq!(catalog.decode("fr-FR", &bytes).unwrap().len(), 4);
    let mut corrupt = bytes.clone();
    corrupt[0] ^= 1;
    assert!(catalog.decode("fr-FR", &corrupt).is_err());
    assert!(catalog.decode("ja-JP", &bytes).is_err());
    let (catalog, bytes) = fixture(|files| {
        files.insert("../outside.json".into(), b"{}".to_vec());
    });
    assert!(catalog.decode("fr-FR", &bytes).is_err());
    let (catalog, bytes) = fixture(|files| {
        files.insert("web.json".into(), b"{}".to_vec());
    });
    assert!(catalog.decode("fr-FR", &bytes).is_err());
    let (catalog, bytes) = fixture(|files| {
        let mut value: serde_json::Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
        value["locale"] = serde_json::json!("ja-JP");
        files.insert("manifest.json".into(), serde_json::to_vec(&value).unwrap());
    });
    assert!(catalog.decode("fr-FR", &bytes).is_err());
}
#[test]
fn verified_install_survives_restart_and_failed_reinstall_preserves_previous_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let (catalog, bytes) = fixture(|_| {});
    assert!(store.select_ui_language(&catalog, Locale::French).is_err());
    assert!(store
        .legacy_language_pack(&catalog, "fr-FR")
        .unwrap()
        .is_none());
    let sections = store
        .install_legacy_language_pack(&catalog, "fr-FR", &bytes)
        .unwrap();
    assert_eq!(store.configuration().unwrap().locale, Locale::Simplified);
    store.select_ui_language(&catalog, Locale::French).unwrap();
    assert!(store
        .install_legacy_language_pack(&catalog, "fr-FR", b"broken")
        .is_err());
    assert_eq!(
        store
            .legacy_language_pack(&catalog, "fr-FR")
            .unwrap()
            .unwrap(),
        sections
    );
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.configuration().unwrap().locale, Locale::French);
    assert_eq!(
        store
            .legacy_language_pack(&catalog, "fr-FR")
            .unwrap()
            .unwrap(),
        sections
    );
}
#[test]
fn language_switch_works_with_offline_roots_without_changing_media_or_prior_task_locale() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("media");
    std::fs::create_dir(&root).unwrap();
    let bytes = b"<movie><title>Original</title></movie>";
    std::fs::write(root.join("movie.nfo"), bytes).unwrap();
    let metadata = std::fs::metadata(root.join("movie.nfo")).unwrap();
    let store = Store::open(&temp.path().canonicalize().unwrap().join("state.sqlite")).unwrap();
    let config = store
        .configure(
            "setup",
            Configuration {
                revision: 0,
                locale: Locale::Simplified,
                roots: vec![LibraryRoot {
                    id: "movies".into(),
                    space: Space::Movie,
                    path: root.to_str().unwrap().into(),
                }],
            },
        )
        .unwrap();
    let task = store
        .submit(ScanRequest {
            operation_id: "before-language".into(),
            space: Space::Movie,
            root_ids: vec!["movies".into()],
        })
        .unwrap();
    std::fs::rename(&root, root.with_file_name("offline")).unwrap();
    let catalog = Catalog::embedded().unwrap();
    let saved = store.select_ui_language(&catalog, Locale::English).unwrap();
    assert_eq!(saved.roots, config.roots);
    assert_eq!(saved.revision, config.revision + 1);
    assert_eq!(store.task(&task.id).unwrap().locale, Locale::Simplified);
    assert_eq!(
        store
            .select_ui_language(&catalog, Locale::English)
            .unwrap()
            .revision,
        saved.revision
    );
    let path = root.with_file_name("offline").join("movie.nfo");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(path).unwrap().modified().unwrap(),
        metadata.modified().unwrap()
    );
}

fn published_bytes(locale: &str) -> Vec<u8> {
    let output=std::process::Command::new("python3").args(["-c", "import importlib.util,sys;from pathlib import Path;p=Path(sys.argv[1]);s=importlib.util.spec_from_file_location('packs',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);c=m.load_catalog();sys.stdout.buffer.write(m.build_pack(c,sys.argv[2],c['languages'][sys.argv[2]])[0])", concat!(env!("CARGO_MANIFEST_DIR"),"/../../../tools/build-language-packs.py"),locale]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
#[test]
fn published_v4_packs_decode_with_exact_pinned_hashes_for_all_five_locales() {
    let catalog = Catalog::previous_installation().unwrap();
    for locale in catalog.languages.keys() {
        let sections = catalog.decode(locale, &published_bytes(locale)).unwrap();
        assert_eq!(sections.len(), 4);
        assert!(!sections["web"].is_empty());
        assert!(!sections["native"].is_empty());
    }
}
fn rewrite_bytes(locale: &str) -> Vec<u8> {
    let output = std::process::Command::new("python3").args(["-c", "import importlib.util,json,sys;from pathlib import Path;p=Path(sys.argv[1]);s=importlib.util.spec_from_file_location('packs',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);c=json.loads(m.CATALOG_PATH.read_text());sys.stdout.buffer.write(m.build_pack(c,sys.argv[2])[0])", concat!(env!("CARGO_MANIFEST_DIR"),"/../../../rewrite/tools/build-language-packs.py"), locale]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
#[test]
fn rewrite_packs_extend_each_original_section_without_changing_original_translations() {
    let current = Catalog::embedded().unwrap();
    let previous = Catalog::previous_installation().unwrap();
    for locale in current.languages.keys() {
        let old = previous.decode(locale, &published_bytes(locale)).unwrap();
        let new = current.decode(locale, &rewrite_bytes(locale)).unwrap();
        for (section, messages) in old {
            for (id, text) in messages {
                assert_eq!(new[&section][&id], text);
            }
        }
        for english in [
            "Job",
            "Action",
            "Started",
            "HTTP attempts",
            "Skipped",
            "Interrupted",
            "Cancelled",
        ] {
            assert_ne!(new["engine"][&stable_id(english)], english);
        }
    }
}
#[test]
fn task_language_identity_and_text_survive_ui_switch_and_later_pack_damage() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("media");
    std::fs::create_dir(&root).unwrap();
    let bytes = b"<movie><title>Original</title></movie>";
    std::fs::write(root.join("movie.nfo"), bytes).unwrap();
    let path = temp.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    store
        .configure(
            "root",
            Configuration {
                revision: 0,
                locale: Locale::Simplified,
                roots: vec![LibraryRoot {
                    id: "movies".into(),
                    space: Space::Movie,
                    path: root.to_str().unwrap().into(),
                }],
            },
        )
        .unwrap();
    let catalog = Catalog::embedded().unwrap();
    store
        .install_legacy_language_pack(&catalog, "fr-FR", &rewrite_bytes("fr-FR"))
        .unwrap();
    store.select_ui_language(&catalog, Locale::French).unwrap();
    let task = store
        .submit(ScanRequest {
            operation_id: "frozen-french".into(),
            space: Space::Movie,
            root_ids: vec!["movies".into()],
        })
        .unwrap();
    assert_eq!(task.job().language_pack_revision, 2);
    assert!(task.job().log.contains("Tâche: frozen-french"));
    assert!(task.job().log.contains("Opération: reconcile-index"));
    assert!(!task
        .journal
        .as_ref()
        .unwrap()
        .language_catalog_hash
        .is_empty());
    store.select_ui_language(&catalog, Locale::English).unwrap();
    store
        .save_preference(
            &format!("language-zip:{}", catalog.languages["fr-FR"].sha256),
            &serde_json::json!("damaged"),
        )
        .unwrap();
    while !store.task(&task.id).unwrap().state.terminal() {
        store.run_next(|| false, |_| {}).unwrap();
    }
    let job = store.task_job(&task.id).unwrap();
    assert_eq!(job.language, "fr-FR");
    assert_eq!(job.language_pack_revision, 2);
    assert!(job.log.contains("Spécifications manquantes [spec-missing]"));
    assert_eq!(job.message, "Terminé");
    assert!(job.log.contains("En cours"));
    assert!(job.log.contains("Terminé"));
    assert_eq!(std::fs::read(root.join("movie.nfo")).unwrap(), bytes);
}
#[test]
fn every_external_task_keeps_translated_file_errors_and_actions_after_ui_switch() {
    let catalog = Catalog::embedded().unwrap();
    for code in catalog.languages.keys() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let nfo = root.join("broken.nfo");
        let bytes = b"<movie><title>Untouched";
        std::fs::write(&nfo, bytes).unwrap();
        let path = root.join("state.sqlite");
        let store = Store::open(&path).unwrap();
        store
            .configure(
                "root",
                Configuration {
                    roots: vec![LibraryRoot {
                        id: "movies".into(),
                        space: Space::Movie,
                        path: root.to_str().unwrap().into(),
                    }],
                    ..Configuration::default()
                },
            )
            .unwrap();
        let sections = store
            .install_legacy_language_pack(&catalog, code, &rewrite_bytes(code))
            .unwrap();
        let locale = serde_json::from_value(serde_json::json!(code)).unwrap();
        store.select_ui_language(&catalog, locale).unwrap();
        let task = store
            .submit(ScanRequest {
                operation_id: "frozen-errors".into(),
                space: Space::Movie,
                root_ids: vec!["movies".into()],
            })
            .unwrap();
        store.select_ui_language(&catalog, Locale::English).unwrap();
        while !store.task(&task.id).unwrap().state.terminal() {
            store.run_next(|| false, |_| {}).unwrap();
        }
        let job = store.task_job(&task.id).unwrap();
        assert_eq!(job.language, *code);
        assert!(job.log.contains(
            &sections["engine"][&stable_id(
                "NFO XML is invalid. Repair the file and refresh the index before retrying."
            )]
        ));
        assert!(job.log.contains(nfo.to_str().unwrap()));
        assert!(job.log.contains("invalid-xml"));
        let mut failed = task.clone();
        failed.failure = Some(AppError::new("auth", "Provider HTTP 401").at(nfo.display()));
        assert!(failed.job().message.contains(&sections["engine"][&stable_id("AI authentication failed. Check the API Key and test the connection before resuming.")]));
        assert!(failed.job().message.contains("[auth]"));
        assert_eq!(failed.failure.unwrap().message, "Provider HTTP 401");
        assert_eq!(std::fs::read(&nfo).unwrap(), bytes);
        drop(store);
        assert_eq!(
            Store::open(&path).unwrap().task_job(&task.id).unwrap().log,
            job.log
        );
    }
}
#[test]
fn a_reused_r1_installation_is_reported_as_r1_in_new_task_history() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    store
        .configure(
            "root",
            Configuration {
                roots: vec![LibraryRoot {
                    id: "movies".into(),
                    space: Space::Movie,
                    path: root.to_str().unwrap().into(),
                }],
                ..Configuration::default()
            },
        )
        .unwrap();
    let previous = Catalog::previous_installation().unwrap();
    store
        .install_legacy_language_pack(&previous, "fr-FR", &published_bytes("fr-FR"))
        .unwrap();
    store
        .select_ui_language(&Catalog::embedded().unwrap(), Locale::French)
        .unwrap();
    let task = store
        .submit(ScanRequest {
            operation_id: "reused-old-pack".into(),
            space: Space::Movie,
            root_ids: vec!["movies".into()],
        })
        .unwrap();
    assert_eq!(task.job().language_pack_revision, 1);
    while !store.task(&task.id).unwrap().state.terminal() {
        store.run_next(|| false, |_| {}).unwrap();
    }
    assert!(store.task_job(&task.id).unwrap().log.contains("Terminé"));
}
