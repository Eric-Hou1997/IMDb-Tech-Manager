#![cfg(feature = "write-prototype")]
#[cfg(unix)]
use itm_core::transaction::Phase;
use itm_core::{specs, transaction::Writer, *};
use std::{fs, path::PathBuf};
const ORIGINAL:&str="\u{feff}<movie>\r\n<title>中文</title><tag>外部</tag><tag>ARRI</tag><unknown a=\"keep\"/><technicalspecs source=\"IMDb\" specHash=\"old\"><section name=\"Camera\"><item>ARRI</item></section><generatedtags owner=\"IMDb Tech Manager\" engine=\"local\" state=\"current\"><tag id=\"g1\">ARRI</tag></generatedtags><manualtags owner=\"IMDb Tech Manager\"><tag>保护</tag></manualtags></technicalspecs>\r\n</movie>";
fn setup() -> (tempfile::TempDir, Writer, PathBuf, Vec<u8>) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let media = root.join("media");
    fs::create_dir(&media).unwrap();
    let path = media.join("电影.nfo");
    fs::write(&path, ORIGINAL).unwrap();
    let writer = Writer::new(&root.join("journal"), vec![media]).unwrap();
    let specs = Specs::from([("Camera".into(), vec!["Sony & ARRI".into()])]);
    let candidate = specs::manual_candidate(ORIGINAL.as_bytes(), &specs).unwrap();
    (temp, writer, path, candidate)
}
#[test]
fn candidate_preserves_protected_bytes_and_marks_stale() {
    let (_tmp, _writer, _path, candidate) = setup();
    specs::validate_specs_only(ORIGINAL.as_bytes(), &candidate).unwrap();
    let value = String::from_utf8(candidate).unwrap();
    assert!(value.starts_with('\u{feff}'));
    assert!(value.contains("state=\"stale\""));
    assert!(value.contains("<tag id=\"g1\">ARRI</tag>"));
    assert!(value.contains("<sourcesnapshot"));
    assert!(value.contains("Sony &amp; ARRI"));
    assert!(!value.replace("\r\n", "").contains('\n'));
}
#[cfg(unix)]
#[test]
fn commit_retry_undo_preserve_bytes_and_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, writer, path, candidate) = setup();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let expected = hash(ORIGINAL.as_bytes());
    let r = writer
        .commit("write1", &path, &expected, &candidate, |_| Ok(()))
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), candidate);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(
        writer
            .commit("write1", &path, &expected, &candidate, |_| panic!(
                "must not repeat mutation"
            ))
            .unwrap(),
        r
    );
    writer.undo("undo1", "write1").unwrap();
    assert_eq!(fs::read(&path).unwrap(), ORIGINAL.as_bytes());
    assert_eq!(
        writer
            .commit("write1", &path, &expected, &candidate, |_| panic!())
            .unwrap(),
        r
    );
    assert_eq!(fs::read(&path).unwrap(), ORIGINAL.as_bytes());
}
#[test]
fn mutation_outside_specs_is_rejected() {
    let (_tmp, writer, path, _candidate) = setup();
    let candidate = ORIGINAL.replace("<tag>外部</tag>", "");
    assert_eq!(
        writer
            .commit(
                "bad",
                &path,
                &hash(ORIGINAL.as_bytes()),
                candidate.as_bytes(),
                |_| Ok(())
            )
            .unwrap_err()
            .code,
        "unsafe-skip"
    );
    assert_eq!(fs::read(&path).unwrap(), ORIGINAL.as_bytes());
}
#[test]
fn ownership_mutation_is_rejected() {
    let (_tmp, writer, path, candidate) = setup();
    let bad = String::from_utf8(candidate)
        .unwrap()
        .replace("id=\"g1\"", "id=\"changed\"");
    assert_eq!(
        writer
            .commit(
                "bad",
                &path,
                &hash(ORIGINAL.as_bytes()),
                bad.as_bytes(),
                |_| Ok(())
            )
            .unwrap_err()
            .code,
        "unsafe-skip"
    );
}
#[cfg(unix)]
#[test]
fn concurrent_external_edit_is_never_replaced() {
    let (_tmp, writer, path, candidate) = setup();
    let external = ORIGINAL.replace("中文", "外部修改");
    let result = writer.commit(
        "race",
        &path,
        &hash(ORIGINAL.as_bytes()),
        &candidate,
        |phase| {
            if matches!(phase, Phase::BeforeReplace) {
                fs::write(&path, &external).unwrap();
            }
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err().code, "source-conflict");
    assert_eq!(fs::read_to_string(path).unwrap(), external);
    assert_eq!(writer.inspect("race").unwrap().state, "conflict");
}
#[cfg(unix)]
#[test]
fn cancelled_before_replace_preserves_source() {
    let (_tmp, writer, path, candidate) = setup();
    let result = writer.commit(
        "cancel",
        &path,
        &hash(ORIGINAL.as_bytes()),
        &candidate,
        |phase| {
            if matches!(phase, Phase::BeforeReplace) {
                return Err(AppError::new("cancelled", "injected cancellation"));
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), ORIGINAL.as_bytes());
    assert_eq!(writer.inspect("cancel").unwrap().state, "not-applied");
}
#[cfg(unix)]
#[test]
fn interruption_after_replace_is_recovered_by_hash() {
    let (_tmp, writer, path, candidate) = setup();
    let result = writer.commit(
        "crash",
        &path,
        &hash(ORIGINAL.as_bytes()),
        &candidate,
        |phase| {
            if matches!(phase, Phase::Replaced) {
                return Err(AppError::new(
                    "interrupted",
                    "injected interruption before receipt",
                ));
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(writer.inspect("crash").unwrap().state, "committed");
    writer.undo("undo-crash", "crash").unwrap();
    assert_eq!(fs::read(&path).unwrap(), ORIGINAL.as_bytes());
}
#[test]
fn stale_hash_and_invalid_candidate_do_not_write() {
    let (_tmp, writer, path, candidate) = setup();
    assert_eq!(
        writer
            .commit("stale", &path, "stale", &candidate, |_| Ok(()))
            .unwrap_err()
            .code,
        "source-conflict"
    );
    assert_eq!(
        writer
            .commit(
                "broken",
                &path,
                &hash(ORIGINAL.as_bytes()),
                b"<movie>",
                |_| Ok(())
            )
            .unwrap_err()
            .code,
        "invalid-candidate"
    );
    assert_eq!(fs::read(&path).unwrap(), ORIGINAL.as_bytes());
}
#[test]
fn structured_imdb_keeps_separate_bullets() {
    let payload = serde_json::json!({"nested":{"runtimes":{"edges":[{"node":{"displayableProperty":{"value":{"plainText":"2h 4m"}},"seconds":7440}}]},"technicalSpecifications":{"cameras":{"items":[{"camera":"ARRI","attributes":[{"text":"IMAX"}]},{"camera":"Sony"},{"camera":"Sony"}]}}}});
    let specs = specs::parse_next_data(&payload).unwrap();
    assert_eq!(specs["Camera"], vec!["ARRI (IMAX)", "Sony"]);
    assert_eq!(specs["Runtime"], vec!["2h 4m (124 min)"]);
    assert_eq!(specs.len(), 10);
    assert!(specs::parse_next_data(&serde_json::json!({"challenge":true})).is_err());
}
