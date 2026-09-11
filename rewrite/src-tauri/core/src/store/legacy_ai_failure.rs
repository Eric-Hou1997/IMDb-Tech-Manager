use super::*;
use crate::ai::{
    job::Record,
    legacy_failure::{self, Failure},
};
use serde_json::Value;

pub(super) fn inspect(db: &Connection, value: &mut Record) -> Result<()> {
    let body: Option<String> = db
        .query_row(
            "SELECT body FROM preferences WHERE key=?1",
            [Failure::preference_key(&value.path)],
            |r| r.get(0),
        )
        .optional()?;
    let Some(body) = body else { return Ok(()) };
    let failure: Failure = serde_json::from_str(&body)?;
    failure.validate()?;
    let stored = failure.entry["fingerprint"].as_str().unwrap_or("");
    let mut matches = stored.is_empty();
    if failure.active && !value.request.retry_failed && !matches {
        for extra in super::legacy_ai_cache::extra_candidates(db, &value.settings)? {
            if legacy_failure::fingerprint(
                &value.path,
                &value.specs,
                &value.settings,
                failure.kind(),
                &extra,
            )
            .is_ok_and(|v| v == stored)
            {
                matches = true;
                break;
            }
        }
    }
    if failure.active && !value.request.retry_failed && matches {
        value.phase = if stored.is_empty() {
            "skipped-legacy-unverified"
        } else {
            "skipped-legacy-failure"
        }
        .into();
        let message = if stored.is_empty() {
            "Historical failure has no request fingerprint; explicitly retry this item to proceed"
                .into()
        } else {
            failure.entry["message"]
                .as_str()
                .unwrap_or("Unchanged historical AI failure; explicitly retry to proceed")
                .to_owned()
        };
        let mut error = AppError::new(failure.kind(), message).at(&value.path);
        error.retryable = failure.entry["retryable"].as_bool().unwrap_or(true);
        error.operation_id = Some(value.request.operation_id.clone());
        value.error = Some(error);
        value.finished_at = Some(crate::ai::job::now());
    }
    value.legacy_failure = Some(failure);
    Ok(())
}
pub(super) fn resolve(db: &Connection, path: &str) -> Result<()> {
    db.execute("UPDATE preferences SET body=json_set(body,'$.active',json('false'),'$.resolved_at',?2) WHERE key=?1 AND json_extract(body,'$.active')=1",params![Failure::preference_key(path),crate::ai::job::now()])?;
    Ok(())
}
impl Store {
    pub(super) fn prepare_failure_migration(
        &self,
        plan: &mut crate::migration::MigrationPlan,
    ) -> Result<()> {
        if !plan.source_kind.starts_with("itm-") {
            return Ok(());
        }
        let db = self.db()?;
        let mut entries = std::collections::BTreeMap::new();
        for file in &plan.files {
            if !["ai-failure-queue.json", "data/ai-failure-queue.json"]
                .contains(&file.relative.as_str())
            {
                continue;
            }
            let raw = crate::migration::read_snapshot(Path::new(&plan.source), file)?;
            let queue: Value = match serde_json::from_slice(&raw) {
                Ok(v) => v,
                Err(_) => {
                    plan.warnings.push(format!(
                        "Malformed AI failure queue retained without activation: {}",
                        file.relative
                    ));
                    continue;
                }
            };
            let Some(items) = queue["items"].as_array() else {
                plan.warnings.push(format!(
                    "Missing AI failure items retained: {}",
                    file.relative
                ));
                continue;
            };
            if items.len() > 100_000 {
                return Err(AppError::new(
                    "migration-size",
                    "Historical failure queue exceeds 100000 records",
                ));
            }
            for entry in items {
                let failure = Failure {
                    path: entry["path"].as_str().unwrap_or("").into(),
                    import_id: plan.id.clone(),
                    source: file.relative.clone(),
                    entry: entry.clone(),
                    active: true,
                    resolved_at: None,
                };
                if failure.validate().is_err() {
                    plan.warnings.push(format!(
                        "Invalid historical failure retained without activation: {}",
                        failure.path
                    ));
                    continue;
                }
                let key = Failure::preference_key(&failure.path);
                if entries.contains_key(&key) {
                    return Err(AppError::new(
                        "migration-ambiguous-ai-failure",
                        "Multiple historical failures target the same path; choose one queue",
                    ));
                }
                let before: Option<String> = db
                    .query_row("SELECT body FROM preferences WHERE key=?1", [&key], |r| {
                        r.get(0)
                    })
                    .optional()?;
                entries.insert(key.clone(),crate::migration::AdapterPlan{source:format!("{} · {}",file.relative,failure.path),target:key,before_hash:before.map(|v|hash(v.as_bytes())),value:serde_json::to_value(failure)?,warnings:vec!["No request is resumed by import; unchanged failures remain skipped until an explicit retry. Unfingerprinted failures also require explicit retry.".into()]});
            }
        }
        plan.adapters.extend(entries.into_values());
        Ok(())
    }
}
