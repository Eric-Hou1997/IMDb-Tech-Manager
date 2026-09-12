use super::*;
use crate::{imdb_cache::*, migration::*};
fn key(imdb: &str) -> String {
    format!("imdb-raw:{imdb}")
}
fn current(db: &Connection, imdb: &str) -> Result<Option<String>> {
    Ok(db
        .query_row(
            "SELECT body FROM preferences WHERE key=?1",
            [key(imdb)],
            |row| row.get(0),
        )
        .optional()?)
}
fn identity(relative: &str, suffix: &str) -> Option<String> {
    let id = relative
        .rsplit('/')
        .next()?
        .strip_prefix("raw-")?
        .strip_suffix(suffix)?;
    crate::specs::imdb_url(id).ok().map(|_| id.into())
}
impl Store {
    pub(super) fn prepare_raw_cache_migration(&self, plan: &mut MigrationPlan) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        let mut paired = std::collections::HashSet::new();
        for file in plan
            .files
            .iter()
            .filter(|file| file.category == "imdb-cache")
        {
            let Some(imdb) = identity(&file.relative, ".json") else {
                continue;
            };
            if !seen.insert(imdb.clone()) {
                return Err(AppError::new(
                    "ambiguous-cache-source",
                    "Choose one historical raw IMDb cache directory",
                ));
            }
            let body_path = format!("{}.html.gz", file.relative.trim_end_matches(".json"));
            paired.insert(body_path.clone());
            let prior = current(&*self.db()?, &imdb)?;
            let before_hash = prior.as_ref().map(|body| hash(body.as_bytes()));
            let decoded = (|| {
                let body = plan
                    .files
                    .iter()
                    .find(|other| other.relative == body_path)
                    .ok_or_else(|| AppError::new("legacy-raw-pair", "Raw cache body is missing"))?;
                decode_raw(
                    &imdb,
                    &read_snapshot(Path::new(&plan.source), file)?,
                    &read_snapshot(Path::new(&plan.source), body)?,
                )
            })();
            let (state, detail) = match decoded {
                Err(error) => (
                    "archived-incompatible",
                    format!("{}: {}", error.code, error.message),
                ),
                Ok((at, page)) => {
                    let newer = prior
                        .as_ref()
                        .and_then(|body| serde_json::from_str::<RawReference>(body).ok())
                        .is_some_and(|previous| {
                            chrono::DateTime::parse_from_rfc3339(&previous.fetched_at).ok()
                                >= chrono::DateTime::parse_from_rfc3339(&at).ok()
                        });
                    if newer {
                        (
                            "kept-current",
                            "A newer or equal raw page is already retained".into(),
                        )
                    } else if !fresh(&at, chrono::Utc::now().timestamp(), 30 * 86400) {
                        (
                            "archived-expired",
                            format!("Original fetch time {at}; raw cache lifetime is 30 days"),
                        )
                    } else {
                        match raw_source(&imdb, &at, &page) {
                        Ok(_) => ("raw-source", format!("Validated raw page fetched {at}; reused only after parsed-cache and failure-cooldown checks")),
                        Err(error) => ("raw-unparsed", format!("Validated raw bytes retained for parser upgrades; current parser cannot reuse them: {}", error.code)),
                    }
                    }
                }
            };
            plan.cache_entries.push(CacheMigrationItem {
                source: file.relative.clone(),
                body_source: Some(body_path),
                imdb,
                state: state.into(),
                before_hash,
                detail,
            });
        }
        for file in plan
            .files
            .iter()
            .filter(|file| file.category == "imdb-cache")
        {
            if !paired.contains(&file.relative) {
                if let Some(imdb) = identity(&file.relative, ".html.gz") {
                    plan.cache_entries.push(CacheMigrationItem {source:file.relative.clone(), body_source:None, imdb, state:"archived-incompatible".into(), before_hash:None, detail:"Raw page metadata is missing; retained without becoming an active cache".into()});
                }
            }
        }
        Ok(())
    }
}
fn artifact(db: &Connection, import_id: &str, path: &str) -> Result<Vec<u8>> {
    let (expected, raw): (String, Vec<u8>) = db.query_row(
        "SELECT sha256,body FROM legacy_artifacts WHERE import_id=?1 AND path=?2",
        params![import_id, path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if hash(&raw) != expected {
        return Err(AppError::new(
            "legacy-raw-integrity",
            "Imported raw cache artifact checksum changed",
        )
        .at(path));
    }
    Ok(raw)
}
pub(super) fn apply(
    db: &Connection,
    import_id: &str,
    entries: &[CacheMigrationItem],
) -> Result<u32> {
    let mut count = 0;
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.state.as_str(), "raw-source" | "raw-unparsed"))
    {
        if current(db, &entry.imdb)?.map(|body| hash(body.as_bytes())) != entry.before_hash {
            return Err(AppError::new(
                "migration-cache-conflict",
                "Raw cache changed after migration review",
            ));
        }
        let body = entry.body_source.as_ref().ok_or_else(|| {
            AppError::new("legacy-raw-pair", "Raw cache body is missing from its plan")
        })?;
        let (at, _) = decode_raw(
            &entry.imdb,
            &artifact(db, import_id, &entry.source)?,
            &artifact(db, import_id, body)?,
        )?;
        let reference = RawReference {
            import_id: import_id.into(),
            metadata: entry.source.clone(),
            body: body.clone(),
            fetched_at: at,
        };
        db.execute("INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET body=excluded.body",params![key(&entry.imdb),serde_json::to_string(&reference)?])?;
        count += 1;
    }
    Ok(count)
}
pub(super) fn source(
    db: &Connection,
    imdb: &str,
    now: i64,
) -> Result<Option<crate::specs::SourceSpecs>> {
    let Some(body) = current(db, imdb)? else {
        return Ok(None);
    };
    let reference: RawReference = serde_json::from_str(&body)?;
    if !fresh(&reference.fetched_at, now, 30 * 86400) {
        return Ok(None);
    };
    let (at, page) = decode_raw(
        imdb,
        &artifact(db, &reference.import_id, &reference.metadata)?,
        &artifact(db, &reference.import_id, &reference.body)?,
    )?;
    if at != reference.fetched_at {
        return Err(AppError::new(
            "legacy-raw-integrity",
            "Raw cache timestamp differs from its reviewed reference",
        ));
    }
    let Ok(source) = raw_source(imdb, &at, &page) else {
        return Ok(None);
    };
    db.execute("INSERT INTO imdb_cache VALUES(?1,?3,?2) ON CONFLICT(imdb) DO UPDATE SET parser_version=excluded.parser_version,body=excluded.body",params![imdb,serde_json::to_string(&source)?,crate::imdb_cache::PARSER_VERSION])?;
    db.execute(
        "DELETE FROM preferences WHERE key=?1",
        [super::cache_migration::negative_key(imdb)],
    )?;
    Ok(Some(source))
}
