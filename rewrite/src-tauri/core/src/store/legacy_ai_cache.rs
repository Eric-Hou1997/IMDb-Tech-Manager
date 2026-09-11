use super::*;
use crate::ai::{job::Record, legacy_cache};
use std::collections::BTreeSet;

pub(super) fn extra_candidates(
    db: &Connection,
    settings: &crate::ai::job::Settings,
) -> Result<BTreeSet<String>> {
    let mut extras = BTreeSet::from([serde_json::to_string(&settings.config.extra_body)?]);
    // Preserve the literal old extra_body string from every imported profile,
    // including whitespace and key order. The key builder checks its semantics.
    let mut statement = db.prepare("SELECT DISTINCT CASE WHEN json_valid(body) THEN CASE WHEN json_type(body,'$.ai.extra_body')='text' THEN json_extract(body,'$.ai.extra_body') END END FROM legacy_artifacts WHERE category='configuration'")?;
    for raw in statement.query_map([], |r| r.get::<_, Option<String>>(0))? {
        if let Some(raw) = raw? {
            extras.insert(raw);
        }
    }
    Ok(extras)
}
pub(super) fn reuse(db: &Connection, record: &mut Record) -> Result<()> {
    if !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM legacy_artifacts WHERE category='ai-cache')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    let extras = extra_candidates(db, &record.settings)?;
    let mut candidates = vec![];
    for extra in extras {
        let Ok(key) = legacy_cache::key(&record.settings, &record.specs, &record.existing, &extra)
        else {
            continue;
        };
        let mut statement = db.prepare("SELECT rowid,import_id,path,sha256 FROM legacy_artifacts WHERE category='ai-cache' AND path IN (?1,?2) ORDER BY rowid DESC LIMIT 1")?;
        if let Some(row) = statement
            .query_row(
                params![
                    format!("ai-cache/{key}.json"),
                    format!("data/ai-cache/{key}.json")
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
        {
            candidates.push(row);
        }
    }
    // Latest matching imported snapshot wins; an invalid latest entry is
    // visible as an error instead of silently falling back to a stale result.
    let Some((rowid, import_id, source, expected)) = candidates.into_iter().max_by_key(|r| r.0)
    else {
        return Ok(());
    };
    let raw: Vec<u8> = db.query_row(
        "SELECT body FROM legacy_artifacts WHERE rowid=?1",
        [rowid],
        |r| r.get(0),
    )?;
    if hash(&raw) != expected {
        return Err(AppError::new(
            "legacy-ai-cache-invalid",
            "Historical AI cache archive checksum changed",
        )
        .at(&source));
    }
    let hit =
        legacy_cache::validate(&raw, &record.settings, &record.specs).map_err(|e| e.at(&source))?;
    record.result = Some(hit.result);
    record.cached = true;
    record.phase = "review-ready".into();
    record.meter.cache_hit(hit.usage);
    record.historical_cost = hit.cost;
    record.legacy_cache = Some(legacy_cache::Origin {
        import_id,
        source,
        archive_hash: expected,
        created_at: hit.created_at,
        model: hit.model,
        raw_usage: hit.raw_usage,
    });
    record.finished_at = Some(crate::ai::job::now());
    Ok(())
}
