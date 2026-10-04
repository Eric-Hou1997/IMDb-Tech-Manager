use super::*;
use crate::imdb_cache::{CacheRequest, CacheSettings, CacheStatus, Failure, RawReference};
use std::collections::{HashMap, HashSet};
const SETTINGS: &str = "imdb-cache-settings";
const CLEANUP: &str = "imdb-cache-last-cleanup";
const ERROR: &str = "imdb-cache-error";
#[derive(Clone)]
struct Entry {
    key: String,
    imdb: String,
    table: &'static str,
    raw: bool,
    expired: bool,
    bytes: u64,
    accessed: i64,
}
fn timestamp(at: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(at)
        .map(|at| at.timestamp())
        .unwrap_or(0)
}
fn settings(db: &Connection) -> Result<CacheSettings> {
    let body: Option<String> = db
        .query_row(
            "SELECT body FROM preferences WHERE key=?1",
            [SETTINGS],
            |r| r.get(0),
        )
        .optional()?;
    let value = body
        .map(|s| serde_json::from_str::<CacheSettings>(&s))
        .transpose()?
        .unwrap_or_default();
    value.validate()?;
    Ok(value)
}
fn entries(db: &Connection, now: i64) -> Result<(Vec<Entry>, HashSet<String>)> {
    let mut access = HashMap::<String, i64>::new();
    let mut protected = HashSet::new();
    let mut stmt = db.prepare("SELECT json_extract(result,'$.result.imdb'),json_extract(result,'$.result.started_at'),json_extract(result,'$.result.phase') FROM operations WHERE json_extract(result,'$.kind')='fetch'")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (imdb, at, phase) = row?;
        access
            .entry(imdb.clone())
            .and_modify(|old| *old = (*old).max(timestamp(&at)))
            .or_insert(timestamp(&at));
        if phase == "requested" {
            protected.insert(imdb);
        }
    }
    drop(stmt);
    let mut result = Vec::new();
    let mut stmt = db.prepare("SELECT imdb,parser_version,body FROM imdb_cache")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (imdb, version, body) = row?;
        let source = serde_json::from_str::<crate::specs::SourceSpecs>(&body).ok();
        let at = source
            .as_ref()
            .map(|s| timestamp(&s.fetched_at))
            .unwrap_or(0);
        result.push(Entry {
            key: imdb.clone(),
            imdb: imdb.clone(),
            table: "imdb_cache",
            raw: false,
            expired: version != crate::imdb_cache::PARSER_VERSION
                || source
                    .as_ref()
                    .is_none_or(|s| s.imdb != imdb || !crate::imdb_cache::source_fresh(s, now)),
            bytes: body.len() as u64,
            accessed: access.get(&imdb).copied().unwrap_or(at),
        });
    }
    drop(stmt);
    let mut stmt =
        db.prepare("SELECT imdb,fetched_at,length(metadata)+length(body) FROM imdb_raw_cache")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, u64>(2)?,
        ))
    })? {
        let (imdb, at, bytes) = row?;
        result.push(Entry {
            key: imdb.clone(),
            imdb: imdb.clone(),
            table: "imdb_raw_cache",
            raw: true,
            expired: !crate::imdb_cache::fresh(&at, now, 30 * 86400),
            bytes,
            accessed: access.get(&imdb).copied().unwrap_or_else(|| timestamp(&at)),
        });
    }
    drop(stmt);
    let mut stmt = db.prepare(
        "SELECT key,body FROM preferences WHERE key LIKE 'imdb-raw:%' OR key LIKE 'imdb-failure:%'",
    )?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (key, body) = row?;
        let raw = key.starts_with("imdb-raw:");
        let imdb = key
            .split_once(':')
            .map(|(_, value)| value)
            .unwrap_or_default()
            .to_owned();
        let (at, expired, bytes) = if raw {
            match serde_json::from_str::<RawReference>(&body) {
                Ok(value) => {
                    let (count, packed): (u32, u64) = db.query_row("SELECT COUNT(*),COALESCE(SUM(length(body)),0) FROM legacy_artifacts WHERE import_id=?1 AND (path=?2 OR path=?3)",params![value.import_id,value.metadata,value.body],|r|Ok((r.get(0)?,r.get(1)?)))?;
                    (
                        timestamp(&value.fetched_at),
                        count != 2 || !crate::imdb_cache::fresh(&value.fetched_at, now, 30 * 86400),
                        packed.saturating_add(body.len() as u64),
                    )
                }
                Err(_) => (0, true, body.len() as u64),
            }
        } else {
            match serde_json::from_str::<Failure>(&body) {
                Ok(value) => (
                    timestamp(&value.fetched_at),
                    value.imdb != imdb || !crate::imdb_cache::fresh(&value.fetched_at, now, 3600),
                    body.len() as u64,
                ),
                Err(_) => (0, true, body.len() as u64),
            }
        };
        result.push(Entry {
            key,
            imdb: imdb.clone(),
            table: "preferences",
            raw,
            expired,
            bytes,
            accessed: access.get(&imdb).copied().unwrap_or(at),
        });
    }
    Ok((result, protected))
}
fn status(
    db: &Connection,
    settings: CacheSettings,
    values: &[Entry],
    protected: &HashSet<String>,
    removed: u32,
    clear: bool,
) -> Result<CacheStatus> {
    let used = values.iter().map(|e| e.bytes).sum::<u64>();
    let protected_count = values
        .iter()
        .filter(|e| protected.contains(&e.imdb))
        .count() as u32;
    let last_cleanup = db
        .query_row(
            "SELECT body FROM preferences WHERE key=?1",
            [CLEANUP],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .map(|s| serde_json::from_str(&s))
        .transpose()?;
    let archive_bytes = db.query_row(
        "SELECT COALESCE(SUM(length(body)),0) FROM legacy_artifacts",
        [],
        |r| r.get(0),
    )?;
    let error = db
        .query_row("SELECT body FROM preferences WHERE key=?1", [ERROR], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .map(|s| serde_json::from_str::<AppError>(&s))
        .transpose()?;
    Ok(CacheStatus {
        error: error.clone(),
        state: if error.is_some() {
            "failed"
        } else if clear && used > 0 && protected_count > 0 {
            "busy"
        } else if used > u64::from(settings.limit_mb) * 1024 * 1024 {
            "over-limit"
        } else {
            "ready"
        }
        .into(),
        settings,
        used_bytes: used,
        parsed_count: values.iter().filter(|e| !e.raw).count() as u32,
        raw_count: values.iter().filter(|e| e.raw).count() as u32,
        removed_count: removed,
        protected_count,
        last_cleanup,
        archive_bytes,
    })
}
fn eviction(values: &[Entry], protected: &HashSet<String>, limit: u64, clear: bool) -> Vec<usize> {
    let mut order = (0..values.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i| {
        let e = &values[i];
        (
            if clear || e.expired {
                0
            } else if e.raw {
                1
            } else {
                2
            },
            e.accessed,
            e.key.clone(),
        )
    });
    let mut used = values.iter().map(|e| e.bytes).sum::<u64>();
    let over = used > limit;
    let target = if clear { 0 } else { limit * 9 / 10 };
    let mut selected = Vec::new();
    for index in order {
        let entry = &values[index];
        if (clear || entry.expired || over && used > target) && !protected.contains(&entry.imdb) {
            selected.push(index);
            used = used.saturating_sub(entry.bytes);
        }
    }
    selected
}
pub(super) fn trim(db: &Connection, now: i64, clear: bool) -> Result<CacheStatus> {
    let prefs = settings(db)?;
    let (values, protected) = entries(db, now)?;
    let selected = eviction(
        &values,
        &protected,
        u64::from(prefs.limit_mb) * 1024 * 1024,
        clear,
    );
    for &index in &selected {
        let value = &values[index];
        // The table is an internal enum, never provided through IPC or an imported path.
        match value.table {
            "imdb_cache" => {
                db.execute("DELETE FROM imdb_cache WHERE imdb=?1", [&value.key])?;
            }
            "imdb_raw_cache" => {
                db.execute("DELETE FROM imdb_raw_cache WHERE imdb=?1", [&value.key])?;
            }
            "preferences" => {
                db.execute("DELETE FROM preferences WHERE key=?1", [&value.key])?;
            }
            _ => unreachable!(),
        }
    }
    let at = chrono::DateTime::from_timestamp(now, 0)
        .ok_or_else(|| AppError::new("invalid-cache-time", "Cache cleanup time is out of range"))?
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.execute(
        "INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET body=excluded.body",
        params![CLEANUP, serde_json::to_string(&at)?],
    )?;
    db.execute("DELETE FROM preferences WHERE key=?1", [ERROR])?;
    let (remaining, protected) = entries(db, now)?;
    status(
        db,
        prefs,
        &remaining,
        &protected,
        selected.len() as u32,
        clear,
    )
}
impl Store {
    pub(super) fn prepare_cache_settings(
        &self,
        plan: &mut crate::migration::MigrationPlan,
        source: &str,
        value: &serde_json::Value,
    ) -> Result<()> {
        let Some(raw) = value.get("imdb_cache_max_mb") else {
            return Ok(());
        };
        if plan
            .adapters
            .iter()
            .any(|adapter| adapter.target == SETTINGS)
        {
            return Err(AppError::new(
                "ambiguous-legacy-settings",
                "Choose one historical cache settings source",
            ));
        }
        let number = raw
            .as_i64()
            .or_else(|| raw.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
            .or_else(|| {
                raw.as_f64()
                    .filter(|n| n.is_finite())
                    .map(|n| n.trunc() as i64)
            });
        let accepted = number.filter(|value| (64..=65536).contains(value));
        let db = self.db()?;
        let before = db
            .query_row(
                "SELECT body FROM preferences WHERE key=?1",
                [SETTINGS],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        let mut next = settings(&db)?;
        next.revision = next.revision.checked_add(1).ok_or_else(|| {
            AppError::new("revision-overflow", "Cache settings revision exhausted")
        })?;
        next.limit_mb = accepted.unwrap_or(2048) as u32;
        plan.adapters.push(crate::migration::AdapterPlan{source:source.into(),target:SETTINGS.into(),before_hash:before.map(|s|hash(s.as_bytes())),value:serde_json::to_value(next)?,warnings:if accepted.is_none(){vec!["Invalid legacy cache limit uses the original 2048 MiB default; original configuration bytes are retained".into()]}else{vec![]}});
        Ok(())
    }
    /// Opportunistic maintenance has its own transaction, so a failure never
    /// turns a successfully persisted HTTP result into a requested/retried fetch.
    pub fn maintain_imdb_cache_automatically(&self) -> Result<CacheStatus> {
        let result = (|| {
            let mut db = self.db()?;
            self.writable()?;
            let tx = db.transaction()?;
            let value = trim(&tx, chrono::Utc::now().timestamp(), false)?;
            tx.commit()?;
            Ok(value)
        })();
        if let Err(ref error) = result {
            let db = self.db()?;
            if self.writable().is_ok() {
                db.execute("INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET body=excluded.body",params![ERROR,serde_json::to_string(error)?])?;
            }
        }
        result
    }
    pub fn imdb_cache_status(&self) -> Result<CacheStatus> {
        let db = self.db()?;
        let (entries, protected) = entries(&db, chrono::Utc::now().timestamp())?;
        status(&db, settings(&db)?, &entries, &protected, 0, false)
    }
    pub fn maintain_imdb_cache(&self, request: CacheRequest) -> Result<CacheStatus> {
        valid_id(&request.operation_id)?;
        request.settings.validate()?;
        let fingerprint = hash(&serde_json::to_vec(&("imdb-cache-maintenance", &request))?);
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        if let Some((old, body)) = tx
            .query_row(
                "SELECT fingerprint,result FROM operations WHERE id=?1",
                [&request.operation_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if old != fingerprint {
                return Err(AppError::new(
                    "operation-conflict",
                    "Cache operation ID has different input",
                ));
            }
            return match serde_json::from_str(&body)? {
                OperationResult::Cache(status) => Ok(status),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "Operation belongs to another action",
                )),
            };
        }
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [&request.operation_id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(AppError::new(
                "operation-conflict",
                "Operation belongs to a task",
            ));
        }
        let current = settings(&tx)?;
        if current.revision != request.settings.revision {
            return Err(AppError::new(
                "cache-settings-conflict",
                "Reload cache settings before saving",
            ));
        }
        let mut next = request.settings;
        next.revision = next.revision.checked_add(1).ok_or_else(|| {
            AppError::new("revision-overflow", "Cache settings revision exhausted")
        })?;
        tx.execute("INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET body=excluded.body",params![SETTINGS,serde_json::to_string(&next)?])?;
        let result = trim(&tx, chrono::Utc::now().timestamp(), request.clear)?;
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                request.operation_id,
                fingerprint,
                serde_json::to_string(&OperationResult::Cache(result.clone()))?
            ],
        )?;
        tx.commit()?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn entry(key: &str, bytes: u64, raw: bool, expired: bool, at: i64) -> Entry {
        Entry {
            key: key.into(),
            imdb: key.into(),
            table: "preferences",
            bytes,
            raw,
            expired,
            accessed: at,
        }
    }
    #[test]
    fn capacity_preserves_parsed_facts_prefers_old_raw_and_reaches_low_watermark() {
        let values = vec![
            entry("old-raw", 40, true, false, 1),
            entry("new-raw", 40, true, false, 2),
            entry("parsed", 1, false, false, 0),
        ];
        assert_eq!(eviction(&values, &HashSet::new(), 64, false), vec![0]);
        let busy = ["old-raw".into()].into();
        assert_eq!(eviction(&values, &busy, 64, false), vec![1]);
    }
    #[test]
    fn expiry_runs_below_capacity_and_clear_skips_only_inflight_entries() {
        let values = vec![
            entry("expired", 10, false, true, 1),
            entry("fresh", 10, false, false, 2),
            entry("in-use", 10, true, true, 0),
        ];
        let busy = ["in-use".into()].into();
        assert_eq!(eviction(&values, &busy, 64, false), vec![0]);
        assert_eq!(eviction(&values, &busy, 64, true), vec![0, 1]);
    }
}
