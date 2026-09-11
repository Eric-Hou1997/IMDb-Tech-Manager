use itm_core::{rules, Specs};
#[test]
fn camera_series_expansion_preserves_brand_and_source_fact() {
    let value = "Panavision C-, D-, E- and H-Series Lenses, Sony Venice (aerial shots)";
    let specs = Specs::from([("Camera".into(), vec![value.into()])]);
    let entries = rules::entries(&specs);
    assert_eq!(
        entries.iter().map(|e| e.value.as_str()).collect::<Vec<_>>(),
        vec![
            "Panavision C-Series Lenses",
            "Panavision D-Series Lenses",
            "Panavision E-Series Lenses",
            "Panavision H-Series Lenses",
            "Sony Venice"
        ]
    );
    assert!(entries
        .iter()
        .all(|e| e.field == "Camera" && e.source_indexes == [0]));
    assert_eq!(specs["Camera"], [value]);
}
#[test]
fn ambiguous_and_stays_intact_and_notes_do_not_split() {
    assert_eq!(
        rules::camera_tags("Bell and Howell 2709, ARRI Alexa (IMAX, selected scenes)"),
        vec!["Bell and Howell 2709", "ARRI Alexa"]
    );
    assert_eq!(
        rules::camera_tags("Zeiss Ultra Prime and Angenieux Optimo Lenses"),
        vec!["Zeiss Ultra Prime Lenses", "Angenieux Optimo Lenses"]
    );
}
#[test]
fn only_derived_sections_emit_tags_and_duplicate_keeps_first_provenance() {
    let specs = Specs::from([
        ("Camera".into(), vec!["STRASSE, Straße".into()]),
        ("Runtime".into(), vec!["120 min".into()]),
        ("Sound mix".into(), vec!["DTS (DTS:X)".into()]),
        ("Aspect ratio".into(), vec!["2.39 : 1 (theatrical)".into()]),
    ]);
    let tags = rules::entries(&specs);
    assert_eq!(
        tags.iter().map(|e| e.value.as_str()).collect::<Vec<_>>(),
        vec!["DTS:X", "STRASSE", "2.39:1"]
    );
    assert_eq!(tags[1].source_indexes, [0]);
}
