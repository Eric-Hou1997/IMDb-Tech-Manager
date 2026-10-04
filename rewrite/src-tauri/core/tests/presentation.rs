use itm_core::{hash, presentation::*, services::LOCALES};
use std::collections::BTreeMap;
fn fixture() -> (Catalog, Pack) {
    let mut catalog = Catalog {
        schema: 2,
        product: PRODUCT.into(),
        app_version: "v5.0.0".into(),
        message_set_hash: String::new(),
        messages: BTreeMap::from([
            (
                "task.complete".into(),
                Message {
                    text: "已处理 {count} 个 NFO：{title}".into(),
                    protected: vec!["NFO".into()],
                },
            ),
            (
                "action.close".into(),
                Message {
                    text: "关闭".into(),
                    protected: vec![],
                },
            ),
        ]),
        external: BTreeMap::new(),
    };
    catalog.message_set_hash = catalog.source_hash().unwrap();
    let pack = Pack {
        schema: 2,
        product: PRODUCT.into(),
        locale: "fr-FR".into(),
        revision: 1,
        message_set_hash: catalog.message_set_hash.clone(),
        messages: BTreeMap::from([
            (
                "task.complete".into(),
                "{count} NFO traités : {title}".into(),
            ),
            ("action.close".into(), "Fermer".into()),
        ]),
    };
    for locale in LOCALES.iter().filter(|l| !l.built_in) {
        catalog.external.insert(
            locale.code.into(),
            Descriptor {
                locale: locale.code.into(),
                revision: 1,
                asset: format!("ITM-Language-{}-r1.json", locale.code),
                released_with: "v4.0.0".into(),
                sha256: "0".repeat(64),
            },
        );
    }
    (catalog, pack)
}
fn bytes(catalog: &mut Catalog, pack: &Pack) -> Vec<u8> {
    let bytes = serde_json::to_vec(pack).unwrap();
    catalog.external.get_mut("fr-FR").unwrap().sha256 = hash(&bytes);
    bytes
}
#[test]
fn exact_catalog_binding_allows_unchanged_earlier_assets_and_preserves_values() {
    let (mut catalog, pack) = fixture();
    let bytes = bytes(&mut catalog, &pack);
    let decoded = catalog.decode("fr-FR", &bytes).unwrap();
    let original = "电影 <tag>Manual</tag> {count} C:\\媒体\\IMDb.nfo";
    let output = render(
        &decoded.messages["task.complete"],
        &BTreeMap::from([
            ("count".into(), "2".into()),
            ("title".into(), original.into()),
        ]),
    )
    .unwrap();
    assert_eq!(output, format!("2 NFO traités : {original}"));
    assert_eq!(
        render(
            "{{{value}}}",
            &BTreeMap::from([("value".into(), "{unchanged}".into())])
        )
        .unwrap(),
        "{{unchanged}}"
    );
    assert!(render("{value}", &BTreeMap::new()).is_err());
}
#[test]
fn hash_identity_revision_and_builtin_replacement_fail_closed() {
    let (mut catalog, pack) = fixture();
    let mut data = bytes(&mut catalog, &pack);
    data.push(b' ');
    assert_eq!(
        catalog.decode("fr-FR", &data).unwrap_err().code,
        "language-hash"
    );
    assert_eq!(
        catalog.decode("en-US", &data).unwrap_err().code,
        "language-not-external"
    );
    for field in ["product", "locale", "revision", "message_set_hash"] {
        let mut changed = pack.clone();
        match field {
            "product" => changed.product = "tcm".into(),
            "locale" => changed.locale = "ru-RU".into(),
            "revision" => changed.revision = 2,
            _ => changed.message_set_hash = "a".repeat(64),
        }
        let data = bytes(&mut catalog, &changed);
        assert_eq!(
            catalog.decode("fr-FR", &data).unwrap_err().code,
            "language-incompatible"
        );
    }
}
#[test]
fn missing_extra_placeholders_protected_tokens_and_wrong_script_are_rejected() {
    let (mut catalog, pack) = fixture();
    for (text, code) in [
        ("{count} films : {title}", "language-protected-token"),
        ("NFO traités : {title}", "language-placeholder-invalid"),
        ("{count} NFO : {path}", "language-placeholder-invalid"),
        ("已处理 {count} 个 NFO：{title}", "language-script"),
        (
            "{count} NFO {title}\u{202e}",
            "language-placeholder-invalid",
        ),
    ] {
        let mut changed = pack.clone();
        changed.messages.insert("task.complete".into(), text.into());
        let data = bytes(&mut catalog, &changed);
        assert_eq!(catalog.decode("fr-FR", &data).unwrap_err().code, code);
    }
    for extra in [true, false] {
        let mut changed = pack.clone();
        if extra {
            changed.messages.insert("extra".into(), "Extra".into());
        } else {
            changed.messages.remove("action.close");
        }
        let data = bytes(&mut catalog, &changed);
        assert_eq!(
            catalog.decode("fr-FR", &data).unwrap_err().code,
            "language-coverage"
        );
    }
}
#[test]
fn catalog_requires_all_external_locales_and_canonical_version_asset_names() {
    let (catalog, _) = fixture();
    catalog.validate().unwrap();
    let mut missing = catalog.clone();
    missing.external.remove("th-TH");
    assert!(missing.validate().is_err());
    for value in ["5.0.0", "v5.0.0-test", "v0.0.0", "v5.0.0+local"] {
        let mut changed = catalog.clone();
        changed.app_version = value.into();
        assert!(changed.validate().is_err());
    }
    let mut future = catalog.clone();
    future.external.get_mut("fr-FR").unwrap().released_with = "v5.1.0".into();
    assert!(future.validate().is_err());
    let mut traversal = catalog.clone();
    traversal.external.get_mut("fr-FR").unwrap().asset = "../settings.json".into();
    assert!(traversal.validate().is_err());
    let mut changed = catalog;
    changed
        .messages
        .get_mut("task.complete")
        .unwrap()
        .text
        .push('!');
    assert!(changed.validate().is_err());
}

#[test]
fn installation_survives_restart_reuses_exact_assets_and_leaves_media_configuration_untouched() {
    use itm_core::store::Store;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let db = root.join("workspace.sqlite");
    let nfo = root.join("movie.nfo");
    let original = b"\xef\xbb\xbf<movie><title>Example</title><tag>Manual</tag></movie>\r\n";
    std::fs::write(&nfo, original).unwrap();
    let before = std::fs::metadata(&nfo).unwrap().modified().unwrap();
    let (mut catalog, pack) = fixture();
    let data = bytes(&mut catalog, &pack);
    let store = Store::open(&db).unwrap();
    let configuration = store.configuration().unwrap();
    assert!(store
        .presentation_pack(&catalog, "fr-FR")
        .unwrap()
        .is_none());
    store
        .install_presentation_pack(&catalog, "fr-FR", &data)
        .unwrap();
    store
        .install_presentation_pack(&catalog, "fr-FR", &data)
        .unwrap();
    assert_eq!(store.configuration().unwrap(), configuration);
    drop(store);
    let store = Store::open(&db).unwrap();
    assert_eq!(
        store
            .presentation_pack(&catalog, "fr-FR")
            .unwrap()
            .unwrap()
            .messages["action.close"],
        "Fermer"
    );
    let mut upgraded = catalog.clone();
    upgraded.app_version = "v5.1.0".into();
    assert!(store
        .presentation_pack(&upgraded, "fr-FR")
        .unwrap()
        .is_some());
    let mut bad = data.clone();
    bad.push(b' ');
    assert!(store
        .install_presentation_pack(&catalog, "fr-FR", &bad)
        .is_err());
    assert!(store
        .presentation_pack(&catalog, "fr-FR")
        .unwrap()
        .is_some());
    assert_eq!(std::fs::read(&nfo).unwrap(), original);
    assert_eq!(std::fs::metadata(&nfo).unwrap().modified().unwrap(), before);
}

#[test]
fn corrupted_cache_is_rejected_until_a_valid_explicit_reinstall_repairs_it() {
    use itm_core::store::Store;
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().canonicalize().unwrap().join("workspace.sqlite");
    let (mut catalog, pack) = fixture();
    let data = bytes(&mut catalog, &pack);
    let store = Store::open(&db).unwrap();
    store
        .install_presentation_pack(&catalog, "fr-FR", &data)
        .unwrap();
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute(
            "UPDATE preferences SET body='corrupt' WHERE key=?1",
            [format!("presentation-asset:{}", hash(&data))],
        )
        .unwrap();
    assert_eq!(
        store.presentation_pack(&catalog, "fr-FR").unwrap_err().code,
        "language-cache-corrupt"
    );
    assert!(store
        .install_presentation_pack(&catalog, "fr-FR", b"untrusted")
        .is_err());
    assert_eq!(
        store.presentation_pack(&catalog, "fr-FR").unwrap_err().code,
        "language-cache-corrupt"
    );
    store
        .install_presentation_pack(&catalog, "fr-FR", &data)
        .unwrap();
    assert!(store
        .presentation_pack(&catalog, "fr-FR")
        .unwrap()
        .is_some());
    store.freeze_for_update().unwrap();
    assert_eq!(
        store
            .install_presentation_pack(&catalog, "fr-FR", &data)
            .unwrap_err()
            .code,
        "update-in-progress"
    );
}

#[test]
fn executable_fields_wrong_script_and_unbounded_assets_are_not_pack_inputs() {
    let (mut catalog, pack) = fixture();
    let mut value = serde_json::to_value(&pack).unwrap();
    value["settings"] = serde_json::json!({"enabled":true});
    let data = serde_json::to_vec(&value).unwrap();
    catalog.external.get_mut("fr-FR").unwrap().sha256 = hash(&data);
    assert_eq!(
        catalog.decode("fr-FR", &data).unwrap_err().code,
        "language-format"
    );
    let mut wrong = pack;
    wrong
        .messages
        .insert("action.close".into(), "Закрыть".into());
    let data = bytes(&mut catalog, &wrong);
    assert_eq!(
        catalog.decode("fr-FR", &data).unwrap_err().code,
        "language-script"
    );
    assert_eq!(
        catalog.decode("fr-FR", &[]).unwrap_err().code,
        "language-size"
    );
    assert_eq!(
        catalog
            .decode("fr-FR", &vec![b'x'; MAX_PACK_BYTES + 1])
            .unwrap_err()
            .code,
        "language-size"
    );
}
