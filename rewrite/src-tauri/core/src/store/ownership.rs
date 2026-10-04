use super::*;
use serde_json::Value;
use std::io::Read;

/// New mirrors take precedence, including a cleared record. Never resurrect an
/// older archive when an existing local mirror is unreadable or invalid.
pub(super) fn record(db: &Connection, path: &Path) -> Result<Option<Value>> {
    let key = hash(path.to_string_lossy().as_bytes());
    if let Some(directory) = db.path().and_then(|p| Path::new(p).parent()) {
        let target = directory.join("ownership").join(format!("{key}.json"));
        match std::fs::symlink_metadata(&target) {
            Ok(meta) => {
                paths::checked(&target)?;
                if !meta.is_file() || meta.len() > library::MAX_NFO_BYTES {
                    return Err(AppError::new(
                        "unsafe-skip",
                        "Ownership sidecar exceeds read limit",
                    ));
                }
                let mut bytes = vec![];
                std::fs::File::open(&target)
                    .and_then(|f| f.take(library::MAX_NFO_BYTES + 1).read_to_end(&mut bytes))
                    .map_err(|e| AppError::new("unsafe-skip", e).at(target.display()))?;
                if bytes.len() as u64 > library::MAX_NFO_BYTES {
                    return Err(AppError::new(
                        "unsafe-skip",
                        "Ownership sidecar grew beyond read limit",
                    ));
                }
                let value: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| AppError::new("unsafe-skip", "Invalid ownership sidecar JSON"))?;
                return if value["engine"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty())
                {
                    Ok(Some(value))
                } else {
                    Ok(None)
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(AppError::new("unsafe-skip", e).at(target.display())),
        }
    }
    let mut query = db.prepare("SELECT sha256,body FROM legacy_artifacts WHERE category='ownership' AND path IN (?1,?2) ORDER BY rowid DESC")?;
    let mut found = None;
    for row in query.query_map(
        params![
            format!("ownership/{key}.json"),
            format!("data/ownership/{key}.json")
        ],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)),
    )? {
        let (expected, raw) = row?;
        if raw.len() as u64 > library::MAX_NFO_BYTES {
            return Err(AppError::new(
                "unsafe-skip",
                "Ownership archive exceeds read limit",
            ));
        }
        if hash(&raw) != expected {
            return Err(AppError::new(
                "migration-archive-corrupt",
                "Ownership archive checksum failed",
            ));
        }
        let value: Value = serde_json::from_slice(&raw)
            .map_err(|_| AppError::new("unsafe-skip", "Invalid archived ownership JSON"))?;
        if found.as_ref().is_some_and(|previous| previous != &value) {
            return Err(AppError::new(
                "unsafe-skip",
                "Conflicting imported ownership records",
            ));
        }
        found = Some(value);
    }
    Ok(found)
}
pub(super) fn overlay(db: &Connection, item: &mut MediaItem) {
    if item.error.is_some()
        || !item.inspection.tag_engine.is_empty()
        || item.spec_status == "missing"
    {
        return;
    }
    let result = record(db, Path::new(&item.path)).and_then(|record| {
        if let Some(record) = record {
            crate::tags::recovery::overlay(item, &record)?;
        }
        Ok(())
    });
    if result.is_err()
        && !item
            .inspection
            .issues
            .contains(&"ownership-mismatch".into())
    {
        item.inspection.issues.push("ownership-mismatch".into());
    }
}
impl Store {
    pub(super) fn recovered_nfo(
        &self,
        path: &Path,
        raw: &[u8],
    ) -> Result<Option<(Value, Vec<u8>)>> {
        if !crate::tags::recovery::needed(raw)? {
            return Ok(None);
        }
        record(&*self.db()?, path)?
            .map(|value| {
                let recovered = crate::tags::recovery::restore(raw, path, &value)?;
                Ok((value, recovered))
            })
            .transpose()
    }
}
