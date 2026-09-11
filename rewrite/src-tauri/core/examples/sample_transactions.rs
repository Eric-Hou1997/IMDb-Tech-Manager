//! Exercise the write prototype ONLY on temporary copies of supplied read results.
#[cfg(not(feature = "write-prototype"))]
fn main() {
    eprintln!("Enable write-prototype to test temporary copies");
    std::process::exit(2);
}
#[cfg(feature = "write-prototype")]
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use itm_core::{hash, specs, transaction::Writer};
    use std::{fs, path::Path};
    let input = std::env::args()
        .nth(1)
        .ok_or("usage: sample_transactions READ_REPORT_JSON")?;
    let report: serde_json::Value = serde_json::from_slice(&fs::read(input)?)?;
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    let media = base.join("copies");
    fs::create_dir(&media)?;
    let writer = Writer::new(&base.join("journal"), vec![media.clone()])?;
    let mut passed = 0;
    let mut attempted = 0;
    let mut failures = vec![];
    for space in report["spaces"].as_array().ok_or("missing spaces")? {
        for item in space["items"].as_array().ok_or("missing items")? {
            let current = attempted;
            attempted += 1;
            let original = Path::new(item["path"].as_str().ok_or("missing path")?);
            let raw = fs::read(original)?;
            let result = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
                let path = media.join(format!("sample-{current}.nfo"));
                fs::write(&path, &raw)?;
                fs::set_permissions(&path, fs::metadata(original)?.permissions())?;
                let mut changed: itm_core::Specs = serde_json::from_value(item["specs"].clone())?;
                changed
                    .entry("Camera".into())
                    .or_default()
                    .push("Acceptance & temporary copy".into());
                let candidate = specs::manual_candidate(&raw, &changed)?;
                specs::validate_specs_only(&raw, &candidate)?;
                let id = format!("write-{current}");
                let receipt = writer.commit(&id, &path, &hash(&raw), &candidate, |_| Ok(()))?;
                assert_eq!(fs::read(&path)?, candidate);
                assert_eq!(
                    writer.commit(&id, &path, &hash(&raw), &candidate, |_| panic!(
                        "Duplicate write"
                    ))?,
                    receipt
                );
                writer.undo(&format!("undo-{current}"), &id)?;
                assert_eq!(fs::read(&path)?, raw);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        fs::metadata(&path)?.permissions().mode(),
                        fs::metadata(original)?.permissions().mode()
                    );
                }
                Ok(())
            })();
            match result {
                Ok(()) => passed += 1,
                Err(e) => failures.push(serde_json::json!({"path":original,"error":e.to_string()})),
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"temporary_copies_passed":passed,"failures":failures,"checks":["candidate protects outside Specs","commit","duplicate write replay","undo restores exact bytes and mode"],"source_access":"read-only"})
    );
    if !failures.is_empty() {
        return Err("Some temporary-copy transactions failed; see JSON report".into());
    }
    Ok(())
}
