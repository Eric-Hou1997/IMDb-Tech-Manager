use itm_core::{specs::*, Specs};
fn fetched() -> SourceSpecs {
    SourceSpecs {
        imdb: "tt1234567".into(),
        specs: Specs::from([("Camera".into(), vec!["New & Camera".into()])]),
        fetched_at: "2026-09-11T01:02:03Z".into(),
        parser: "next-data".into(),
    }
}
#[test]
fn initial_insert_and_removal_preserve_every_protected_byte() {
    let raw=b"\xef\xbb\xbf<movie>\r\n<title>Original</title><uniqueid type=\"imdb\">tt1234567</uniqueid>\r\n<tag>External</tag>\r\n</movie>";
    let after = source_candidate(raw, &fetched()).unwrap();
    validate_specs_only(raw, &after).unwrap();
    validate_specs_only(&after, raw).unwrap();
    let text = std::str::from_utf8(&after).unwrap();
    assert!(text.contains("New &amp; Camera"));
    assert!(text.contains("imdbid=\"tt1234567\""));
    assert!(text.contains("formatVersion=\"21\""));
    let outside = regex::Regex::new(r"(?s)<technicalspecs.*?</technicalspecs>")
        .unwrap()
        .replace(text, "");
    assert_eq!(outside.as_bytes(), raw);
    assert!(!text.replace("\r\n", "").contains('\n'));
}
#[test]
fn refresh_retains_manual_effective_facts_and_updates_source_snapshot_only() {
    let raw=br#"<movie><tag>Owned</tag><tag>External</tag><technicalspecs source="IMDb" imdbid="tt1234567" modified="manual" modifiedAt="2020-01-01"><section name="Camera"><item>Manual</item></section><generatedtags owner="IMDb Tech Manager" state="current"><tag>Owned</tag></generatedtags><sourcesnapshot><section name="Camera"><item>Old source</item></section></sourcesnapshot><custom value="keep"/></technicalspecs></movie>"#;
    let raw_text = String::from_utf8(raw.to_vec()).unwrap().replace(
        "state=\"current\"",
        &format!(
            "state=\"current\" specHash=\"{}\"",
            fingerprint(&Specs::from([("Camera".into(), vec!["Manual".into()])])).unwrap()
        ),
    );
    let raw = raw_text.as_bytes();
    let after = source_candidate(raw, &fetched()).unwrap();
    let text = std::str::from_utf8(&after).unwrap();
    let doc = roxmltree::Document::parse(text).unwrap();
    let tech = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("technicalspecs"))
        .unwrap();
    assert_eq!(tech.attribute("modifiedAt"), Some("2020-01-01"));
    let camera = tech.children().find(|n| n.has_tag_name("section")).unwrap();
    assert_eq!(camera.first_element_child().unwrap().text(), Some("Manual"));
    let snapshot = tech
        .children()
        .find(|n| n.has_tag_name("sourcesnapshot"))
        .unwrap();
    assert_eq!(snapshot.attribute("fetched"), Some("2026-09-11T01:02:03Z"));
    assert!(snapshot
        .descendants()
        .any(|n| n.text() == Some("New & Camera")));
    assert!(text.contains("state=\"current\""));
    assert!(text.contains("<custom value=\"keep\"/>"));
    validate_specs_only(raw, &after).unwrap();
}
#[test]
fn source_change_marks_owned_tags_stale_and_ambiguous_source_fails_closed() {
    let raw=br#"<movie><tag>Owned</tag><technicalspecs source="IMDb"><section name="Camera"><item>Old</item></section><generatedtags owner="IMDb Tech Manager" state="current"><tag>Owned</tag></generatedtags></technicalspecs></movie>"#;
    let after = source_candidate(raw, &fetched()).unwrap();
    assert!(String::from_utf8(after)
        .unwrap()
        .contains("state=\"stale\""));
    for text in [
        "<movie/>",
        "<movie><technicalspecs source=\"Other\"/></movie>",
        "<movie><technicalspecs source=\"IMDb\" formatVersion=\"99\"/></movie>",
        "<movie><technicalspecs source=\"IMDb\"/><technicalspecs source=\"IMDb\"/></movie>",
    ] {
        assert!(source_candidate(text.as_bytes(), &fetched()).is_err());
    }
    assert_eq!(
        source_candidate(
            br#"<movie><technicalspecs source="IMDb" imdbid="tt7654321"/></movie>"#,
            &fetched()
        )
        .unwrap_err()
        .code,
        "imdb-title-mismatch"
    );
}
#[test]
fn structured_page_identity_empty_and_invalid_json_are_distinct() {
    let value = serde_json::json!({"props":{"title":{"id":"tt1234567","runtimes":{"edges":[]},"technicalSpecifications":{"cameras":{"items":[{"camera":" A "},{"camera":"a"}]}}}}});
    let page = format!("<script type='application/json' id='__NEXT_DATA__'>{value}</script>");
    let parsed = parse_page("tt1234567", &page).unwrap();
    assert_eq!(parsed.specs["Camera"], vec!["A"]);
    assert_eq!(
        parse_page("tt7654321", &page).unwrap_err().code,
        "imdb-title-mismatch"
    );
    assert_eq!(
        parse_page("tt1234567", "<script id='__NEXT_DATA__'>{broken}</script>")
            .unwrap_err()
            .code,
        "imdb-json-invalid"
    );
    assert_eq!(
        parse_page("tt1234567", "Access denied").unwrap_err().code,
        "imdb-layout-unrecognized"
    );
    let empty = page.replace("{\"camera\":\" A \"},{\"camera\":\"a\"}", "");
    assert_eq!(
        parse_page("tt1234567", &empty).unwrap_err().code,
        "imdb-no-tech"
    );
}

#[test]
fn malformed_section_cannot_silently_erase_existing_facts() {
    for bad in [
        serde_json::json!({"items":"changed upstream layout"}),
        serde_json::json!({"items":[{"camera":123}]}),
    ] {
        let payload = serde_json::json!({"id":"tt1234567","runtimes":{"edges":[]},"technicalSpecifications":{"cameras":bad,"soundMixes":{"items":[{"text":"Mono"}]}}});
        let page = format!("<script id='__NEXT_DATA__'>{payload}</script>");
        assert_eq!(
            parse_page("tt1234567", &page).unwrap_err().code,
            "imdb-schema-invalid"
        );
    }
}

#[test]
fn tv_metadata_matches_legacy_and_valid_spaced_closing_tags_are_preserved() {
    let raw =
        b"<tvshow><title>Series</title><uniqueid type=\"imdb\">tt1234567</uniqueid></tvshow >";
    let out = source_candidate(raw, &fetched()).unwrap();
    assert!(std::str::from_utf8(&out)
        .unwrap()
        .contains("mediatype=\"tvshow\""));
    assert!(out.ends_with(b"</tvshow >"));
    validate_specs_only(raw, &out).unwrap();
}

#[test]
fn repeated_cached_source_does_not_grow_whitespace_or_create_another_change() {
    let raw=b"<movie><technicalspecs source=\"IMDb\">\r\n    <section name=\"Camera\"><item>Before</item></section>\r\n  </technicalspecs></movie>";
    let first = source_candidate(raw, &fetched()).unwrap();
    let second = source_candidate(&first, &fetched()).unwrap();
    assert_eq!(first, second);
}
