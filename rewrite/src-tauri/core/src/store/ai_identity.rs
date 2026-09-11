use super::*;
use crate::ai::{identity, job::Record};
/// One transaction retains the original tables, rekeys usable records, and
/// advances the schema. Any incomplete identity rolls back instead of losing
/// an old failure gate and silently dispatching a new paid request.
pub(super) fn migrate(db: &Connection) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE ai_cache_v1_archive AS SELECT * FROM ai_cache;
        CREATE TABLE ai_failures_v1_archive AS SELECT * FROM ai_failures;
        DELETE FROM ai_cache; DELETE FROM ai_failures;
        CREATE INDEX IF NOT EXISTS ai_operation_input ON operations(json_extract(result,'$.result.fingerprint')) WHERE json_extract(result,'$.kind')='ai';")?;
    {
        let mut statement=tx.prepare("SELECT body FROM ai_cache_v1_archive ORDER BY json_extract(body,'$.finished_at'),rowid")?;
        for body in statement.query_map([], |r| r.get::<_, String>(0))? {
            let mut record: Record = serde_json::from_str(&body?)?;
            record.fingerprint = identity::record_keys(&record)?.cache;
            tx.execute("INSERT INTO ai_cache VALUES(?1,?2) ON CONFLICT(fingerprint) DO UPDATE SET body=excluded.body",params![record.fingerprint,serde_json::to_string(&record)?])?;
        }
    }
    let mut mapped = std::collections::BTreeSet::new();
    {
        let mut statement=tx.prepare("SELECT o.result,f.fingerprint,f.error FROM operations o JOIN ai_failures_v1_archive f ON json_extract(o.result,'$.result.fingerprint')=f.fingerprint WHERE json_extract(o.result,'$.kind')='ai' ORDER BY json_extract(o.result,'$.result.finished_at'),o.rowid")?;
        for row in statement.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (body, old_key, error) = row?;
            let OperationResult::Ai(record) = serde_json::from_str(&body)? else {
                unreachable!("AI operations query")
            };
            let cause: AppError = serde_json::from_str(&error)?;
            // Only real failed attempts identify a failure's affected path.
            // A skipped peer with identical specs is not another HTTP failure.
            if record.meter.attempts == 0 || record.error.as_ref() != Some(&cause) {
                continue;
            }
            let key = identity::record_keys(&record)?.failure;
            tx.execute("INSERT INTO ai_failures VALUES(?1,?2) ON CONFLICT(fingerprint) DO UPDATE SET error=excluded.error",params![key,error])?;
            mapped.insert(old_key);
        }
    }
    let expected: u64 = tx.query_row("SELECT COUNT(*) FROM ai_failures_v1_archive", [], |r| {
        r.get(0)
    })?;
    if mapped.len() as u64 != expected {
        return Err(AppError::new("ai-identity-migration","Some existing AI failures have no recoverable request identity; original state is retained"));
    }
    tx.pragma_update(None, "user_version", 8)?;
    tx.commit()?;
    Ok(())
}
