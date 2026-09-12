use itm_core::specs::{parse_page, source_candidate, validate_specs_only, SourceStatus};

const IDENTITY: &str =
    "<link rel='canonical' href='https://www.imdb.com/title/tt1234567/technical/'>";
#[test]
fn old_visual_text_fixtures_preserve_values_order_and_safe_write() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/rewrite/fixtures/imdb-html-lines.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let page = format!("{IDENTITY}{}", case["body"].as_str().unwrap());
        let source = parse_page("tt1234567", &page).unwrap();
        assert_eq!(
            source.parser,
            case["parser"].as_str().unwrap_or("html-lines")
        );
        assert_eq!(source.status, SourceStatus::Ok);
        let actual: itm_core::Specs = source
            .specs
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case["expected"],
            "{}",
            case["name"]
        );
        let before = b"\xef\xbb\xbf<movie>\r\n<title>Original</title><tag>External</tag><unknown>keep</unknown>\r\n</movie>\r\n";
        let after = source_candidate(before, &source).unwrap();
        validate_specs_only(before, &after).unwrap();
    }
}
#[test]
fn fallback_requires_consistent_identity_and_never_infers_empty() {
    let body = "<h3>Camera</h3><p>Actual</p>";
    assert_eq!(
        parse_page("tt1234567", body).unwrap_err().code,
        "imdb-identity-unconfirmed"
    );
    for url in [
        "https://www.imdb.com/title/tt9999999/technical/",
        "https://example.com/title/tt1234567/technical/",
        "https://evil@www.imdb.com/title/tt1234567/technical/",
    ] {
        let page = format!("{IDENTITY}<meta property='og:url' content='{url}'>{body}");
        assert_eq!(
            parse_page("tt1234567", &page).unwrap_err().code,
            "imdb-title-mismatch"
        );
    }
    assert_eq!(
        parse_page("tt1234567", &format!("{IDENTITY}<h3>Camera</h3>"))
            .unwrap_err()
            .code,
        "imdb-layout-unrecognized"
    );
    let mismatch = format!("{IDENTITY}<script id='__NEXT_DATA__'>{{\"id\":\"tt9999999\",\"runtimes\":{{\"edges\":[]}},\"technicalSpecifications\":{{}}}}</script>{body}");
    assert_eq!(
        parse_page("tt1234567", &mismatch).unwrap_err().code,
        "imdb-title-mismatch"
    );
}
#[test]
fn structured_positive_wins_and_explicit_empty_can_use_visible_facts() {
    let page = |items: &str| {
        format!("<script id='__NEXT_DATA__'>{{\"id\":\"tt1234567\",\"runtimes\":{{\"edges\":[]}},\"technicalSpecifications\":{{\"cameras\":{{\"items\":[{items}]}}}}}}</script><h3>Camera</h3><p>Visible</p>")
    };
    let source = parse_page("tt1234567", &page("{\"camera\":\"Structured\"}")).unwrap();
    assert_eq!(source.parser, "next-data");
    assert_eq!(source.specs["Camera"], ["Structured"]);
    let source = parse_page("tt1234567", &page("")).unwrap();
    assert_eq!(source.parser, "html-lines");
    assert_eq!(source.specs["Camera"], ["Visible"]);
}
