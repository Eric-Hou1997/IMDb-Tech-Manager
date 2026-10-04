use super::*;
use serde_json::{json, Value};
use std::{fs, io::Write, path::Path};
/// Compare the same authoritative entries used by v4.1.0. Metadata and row
/// ordering in an older mirror do not establish an ownership mismatch.
pub fn mirror_match(directory: &Path, path: &Path, raw: &[u8]) -> Result<Option<bool>> {
    let Some(expected) = record(raw, path)? else {
        return Ok(None);
    };
    let target = directory.join(format!(
        "{}.json",
        crate::hash(path.to_string_lossy().as_bytes())
    ));
    if !target
        .try_exists()
        .map_err(|e| AppError::new("ownership-mirror", e))?
    {
        return Ok(None);
    }
    let target = crate::paths::checked(&target)?;
    let metadata = fs::metadata(&target).map_err(|e| AppError::new("ownership-mirror", e))?;
    if metadata.len() > crate::library::MAX_NFO_BYTES {
        return Err(AppError::new(
            "ownership-mirror",
            "Ownership mirror exceeds read limit",
        ));
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(&target)
        .and_then(|f| {
            f.take(crate::library::MAX_NFO_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| AppError::new("ownership-mirror", e))?;
    if bytes.len() as u64 > crate::library::MAX_NFO_BYTES {
        return Err(AppError::new(
            "ownership-mirror",
            "Ownership mirror grew beyond read limit",
        ));
    }
    let actual: Value = serde_json::from_slice(&bytes)?;
    if actual.is_null() || actual.as_object().is_some_and(|o| o.is_empty()) {
        return Ok(None);
    }
    if !actual.is_object() {
        return Ok(Some(false));
    }
    if expected["engine"].as_str().unwrap_or("").trim().is_empty()
        && actual["entries"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    {
        return Ok(None);
    }
    let rows = |value: &Value, key: &str| {
        let mut rows: Vec<_> = value[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                (
                    clean(row["id"].as_str().unwrap_or("")),
                    crate::ownership_key(&clean(row["value"].as_str().unwrap_or(""))),
                )
            })
            .collect();
        rows.sort();
        rows
    };
    Ok(Some(
        rows(&expected, "entries") == rows(&actual, "entries")
            && rows(&expected, "manual_entries") == rows(&actual, "manual_entries"),
    ))
}
fn record(raw: &[u8], path: &Path) -> Result<Option<Value>> {
    let all = std::str::from_utf8(raw).map_err(|e| AppError::new("invalid-encoding", e))?;
    let doc = roxmltree::Document::parse(all.trim_start_matches('\u{feff}'))
        .map_err(|e| AppError::new("invalid-xml", e))?;
    let tech = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("technicalspecs") && n.attribute("source") == Some("IMDb"));
    let generated = manifest(tech, "generatedtags", "2")?;
    let manual = manifest(tech, "manualtags", "1")?;
    if generated.node.is_none() && manual.node.is_none() {
        return Ok(None);
    }
    let entries = |m: &Manifest<'_, '_>| -> Result<Vec<Value>> {
        m.entries
            .iter()
            .map(|e| {
                let mut v = serde_json::to_value(&e.attrs)?;
                v["value"] = json!(e.value);
                if let Some(indexes) = v.as_object_mut().and_then(|v| v.remove("sourceIndexes")) {
                    let indexes = indexes
                        .as_str()
                        .unwrap_or("")
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| {
                            s.parse::<usize>()
                                .map_err(|e| AppError::new("invalid-ownership", e))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    v["source_indexes"] = json!(indexes);
                }
                Ok(v)
            })
            .collect()
    };
    let attr = |key: &str| generated.attrs.get(key).cloned().unwrap_or_default();
    Ok(Some(
        json!({"schema":2,"path":path.to_string_lossy(),"imdb":tech.and_then(|n|n.attribute("imdbid")).unwrap_or(""),"engine":attr("engine"),"model":attr("model"),"prompt_hash":attr("promptHash"),"spec_hash":attr("specHash"),"state":attr("state"),"generated":attr("generated"),"entries":entries(&generated)?,"manual_entries":entries(&manual)?}),
    ))
}
/// Mirror only after the authoritative NFO transaction committed. A failed mirror
/// is retryable independently and must never trigger a second NFO replacement.
pub fn mirror(directory: &Path, path: &Path, expected: &str) -> Result<()> {
    let io = |e| AppError::new("ownership-mirror", e);
    let path = crate::paths::checked(path)?;
    let raw = fs::read(&path).map_err(io)?;
    if crate::hash(&raw) != expected {
        return Err(
            AppError::new("source-conflict", "NFO changed before ownership mirroring")
                .at(path.display()),
        );
    }
    let value = record(&raw, &path)?;
    fs::create_dir_all(directory).map_err(io)?;
    let directory = crate::paths::checked(directory)?;
    let target = directory.join(format!(
        "{}.json",
        crate::hash(path.to_string_lossy().as_bytes())
    ));
    if fs::symlink_metadata(&target).is_ok() {
        crate::paths::checked(&target)?;
    }
    if let Some(value) = value {
        let mut tmp = tempfile::NamedTempFile::new_in(&directory).map_err(io)?;
        tmp.write_all(&serde_json::to_vec(&value)?).map_err(io)?;
        tmp.as_file().sync_all().map_err(io)?;
        if crate::hash(&fs::read(&path).map_err(io)?) != expected {
            return Err(
                AppError::new("source-conflict", "NFO changed during ownership mirroring")
                    .at(path.display()),
            );
        }
        tmp.persist(&target).map_err(|e| io(e.error))?;
    } else if target.exists() {
        fs::remove_file(&target).map_err(io)?;
    }
    #[cfg(unix)]
    fs::File::open(&directory)
        .and_then(|f| f.sync_all())
        .map_err(io)?;
    Ok(())
}
