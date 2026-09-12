use super::*;
use crate::{imdb_cache::*, migration::*};
pub(super) fn negative_key(imdb: &str) -> String {
    format!("imdb-failure:{imdb}")
}
fn current(db: &Connection, imdb: &str) -> Result<(Option<String>, Option<String>)> {
    Ok((
        db.query_row("SELECT body FROM imdb_cache WHERE imdb=?1", [imdb], |r| {
            r.get(0)
        })
        .optional()?,
        db.query_row(
            "SELECT body FROM preferences WHERE key=?1",
            [negative_key(imdb)],
            |r| r.get(0),
        )
        .optional()?,
    ))
}
fn snapshot(value: &(Option<String>, Option<String>)) -> Result<Option<String>> {
    if value.0.is_none() && value.1.is_none() {
        Ok(None)
    } else {
        Ok(Some(hash(&serde_json::to_vec(value)?)))
    }
}
impl Store {
    pub(super) fn prepare_cache_migration(&self, plan: &mut MigrationPlan) -> Result<()> {
        if !plan.source_kind.starts_with("itm-") {
            return Ok(());
        }
        let mut seen = std::collections::HashSet::new();
        for file in plan.files.iter().filter(|f| f.category == "imdb-cache") {
            let Some(imdb) = legacy_id(&file.relative) else {
                continue;
            };
            if !seen.insert(imdb.clone()) {
                return Err(AppError::new(
                    "ambiguous-cache-source",
                    "Choose one historical IMDb cache directory",
                ));
            }
            let raw = read_snapshot(Path::new(&plan.source), file)?;
            let existing = current(&*self.db()?, &imdb)?;
            let before_hash = snapshot(&existing)?;
            let (state, detail) = match parse_legacy(&imdb, &raw) {
                Ok(ref entry) => {
                    let (kind, at) = match entry {
                        LegacyCache::Source(s) => ("source", &s.fetched_at),
                        LegacyCache::Failure(f) => ("failure", &f.fetched_at),
                    };
                    let old = chrono::DateTime::parse_from_rfc3339(at)
                        .map_err(|e| AppError::new("legacy-cache-time", e))?;
                    let newer = [existing.0.as_ref(), existing.1.as_ref()]
                        .into_iter()
                        .flatten()
                        .any(|body| {
                            serde_json::from_str::<serde_json::Value>(body)
                                .ok()
                                .and_then(|v| {
                                    v["fetched_at"]
                                        .as_str()
                                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                })
                                .is_some_and(|t| t >= old)
                        });
                    if newer {
                        ("kept-current".into(),"Current cache is at least as recent; historical bytes are retained without replacing it".into())
                    } else {
                        (kind.into(),format!("Original fetch time {at}; runtime expiry and explicit refresh remain in effect"))
                    }
                }
                Err(error) => (
                    "archived-incompatible".into(),
                    format!("{}: {}", error.code, error.message),
                ),
            };
            plan.cache_entries.push(CacheMigrationItem {
                source: file.relative.clone(),
                body_source: None,
                imdb,
                state,
                before_hash,
                detail,
            });
        }
        self.prepare_raw_cache_migration(plan)?;
        Ok(())
    }
}
pub(super) fn apply(
    db: &Connection,
    import_id: &str,
    entries: &[CacheMigrationItem],
) -> Result<u32> {
    let mut count = 0;
    for entry in entries
        .iter()
        .filter(|e| ["source", "failure"].contains(&e.state.as_str()))
    {
        if snapshot(&current(db, &entry.imdb)?)? != entry.before_hash {
            return Err(AppError::new(
                "migration-cache-conflict",
                "IMDb cache changed after the reviewed migration plan",
            ));
        }
        let raw: Vec<u8> = db.query_row(
            "SELECT body FROM legacy_artifacts WHERE import_id=?1 AND path=?2",
            params![import_id, entry.source],
            |r| r.get(0),
        )?;
        match parse_legacy(&entry.imdb, &raw)? {
            LegacyCache::Source(source) => {
                db.execute("INSERT INTO imdb_cache VALUES(?1,1,?2) ON CONFLICT(imdb) DO UPDATE SET parser_version=1,body=excluded.body",params![entry.imdb,serde_json::to_string(&source)?])?;
                db.execute(
                    "DELETE FROM preferences WHERE key=?1",
                    [negative_key(&entry.imdb)],
                )?;
            }
            LegacyCache::Failure(failure) => {
                db.execute("INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET body=excluded.body",params![negative_key(&entry.imdb),serde_json::to_string(&failure)?])?;
            }
        }
        count += 1;
    }
    Ok(count + super::raw_cache::apply(db, import_id, entries)?)
}
