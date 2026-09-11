use itm_core::library;
#[test]
fn unicode_ownership_and_repeated_specs_match_legacy_python_semantics() {
    let raw = r#"<movie><tag>STRASSE</tag><technicalspecs source="IMDb"><section name="Camera"><item>ARRI  Alexa</item></section><section name="Camera"><item>Panavision</item></section><section name="Unknown"><item>Ignored</item></section><generatedtags owner="IMDb Tech Manager" engine="local"><tag>Straße</tag></generatedtags></technicalspecs></movie>"#;
    let root = itm_core::LibraryRoot {
        id: "r".into(),
        space: itm_core::Space::Movie,
        path: "/fixtures".into(),
    };
    let item = library::parse(
        &root,
        std::path::Path::new("/fixtures/movie.nfo"),
        raw.as_bytes(),
    )
    .unwrap();
    assert_eq!(item.tags[0].ownership, itm_core::Ownership::Generated);
    assert_eq!(item.specs["Camera"], vec!["ARRI Alexa", "Panavision"]);
    assert!(!item.specs.contains_key("Unknown"));
}
