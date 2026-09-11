use super::*;
use crate::{
    automatic::{Settings, Status},
    batch::*,
};

fn read(db: &Connection) -> Result<Status> {
    let body: Option<String> = db
        .query_row(
            "SELECT body FROM preferences WHERE key='automatic'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    body.map(|b| serde_json::from_str(&b).map_err(Into::into))
        .unwrap_or_else(|| Ok(Status::default()))
}
fn write(db: &Connection, status: &Status) -> Result<()> {
    db.execute("INSERT INTO preferences VALUES('automatic',?1) ON CONFLICT(key) DO UPDATE SET body=excluded.body", [serde_json::to_string(status)?])?;
    Ok(())
}
pub(super) fn enabled(db: &Connection) -> Result<bool> {
    Ok(read(db)?.enabled)
}
impl Store {
    pub fn automatic_status(&self) -> Result<Status> {
        let mut status = read(&*self.db()?)?;
        status.task_ids = self
            .tasks()?
            .into_iter()
            .filter(|t| t.automatic && !t.state.terminal())
            .map(|t| t.id)
            .collect();
        Ok(status)
    }
    /// Startup preference is independent of the previous process's run state.
    pub fn automatic_startup(&self, now: i64) -> Result<Status> {
        let status = self.automatic_status()?;
        let enabled = status.settings.on_app_start && !self.configuration()?.roots.is_empty();
        let mut db = self.db()?;
        let tx = db.transaction()?;
        let mut status = read(&tx)?;
        status.enabled = enabled;
        status.next_due = now;
        write(&tx, &status)?;
        let rows: Vec<(String, String)> = tx
            .prepare("SELECT id,body FROM tasks")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (id, body) in rows {
            let mut task: Task = serde_json::from_str(&body)?;
            if task.automatic && !task.state.terminal() {
                if enabled && task.state == TaskState::Interrupted {
                    task.state = TaskState::Requested;
                } else if !enabled {
                    task.state = TaskState::Cancelled;
                }
                tx.execute(
                    "UPDATE tasks SET body=?2 WHERE id=?1",
                    params![id, serde_json::to_string(&task)?],
                )?;
            }
        }
        tx.commit()?;
        drop(db);
        self.automatic_status()
    }
    pub fn set_automatic(
        &self,
        id: &str,
        settings: Settings,
        enabled: bool,
        now: i64,
    ) -> Result<Status> {
        valid_id(id)?;
        settings.validate()?;
        let fingerprint = hash(&serde_json::to_vec(&("automatic", &settings, enabled))?);
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        if let Some((old, body)) = tx
            .query_row(
                "SELECT fingerprint,result FROM operations WHERE id=?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if old != fingerprint {
                return Err(AppError::new(
                    "operation-conflict",
                    "Automatic operation ID has different input",
                ));
            }
            return match serde_json::from_str(&body)? {
                OperationResult::Automatic(s) => Ok(s),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "Operation belongs to another action",
                )),
            };
        }
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(AppError::new("operation-conflict", "ID belongs to a task"));
        }
        if enabled {
            let body: Option<String> = tx
                .query_row("SELECT body FROM configuration WHERE id=1", [], |r| {
                    r.get(0)
                })
                .optional()?;
            if body
                .map(|b| serde_json::from_str::<Configuration>(&b))
                .transpose()?
                .is_none_or(|c| c.roots.is_empty())
            {
                return Err(AppError::new(
                    "empty-scope",
                    "Confirm library roots before starting automatic mode",
                ));
            }
        }
        let mut status = read(&tx)?;
        if enabled && !status.enabled {
            status.next_due = now;
        }
        status.enabled = enabled;
        status.settings = settings;
        if !enabled {
            let rows: Vec<(String, String)> = tx
                .prepare("SELECT id,body FROM tasks")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?;
            for (id, body) in rows {
                let mut task: Task = serde_json::from_str(&body)?;
                if task.automatic && !task.state.terminal() {
                    if let Some(batch) = task.batch.as_mut() {
                        if task.state == TaskState::Running {
                            batch.cancel_requested = true;
                        } else {
                            task.state = TaskState::Cancelled;
                        }
                    } else {
                        task.state = TaskState::Cancelled;
                    }
                    tx.execute(
                        "UPDATE tasks SET body=?2 WHERE id=?1",
                        params![id, serde_json::to_string(&task)?],
                    )?;
                }
            }
        }
        write(&tx, &status)?;
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                id,
                fingerprint,
                serde_json::to_string(&OperationResult::Automatic(status.clone()))?
            ],
        )?;
        tx.commit()?;
        Ok(status)
    }
    /// Called by the sole desktop worker. Planning reads the catalog, never a
    /// directory walk. Live path/hash checks still run before every fetch/write.
    pub fn automatic_tick(&self, now: i64) -> Result<Vec<Task>> {
        let _owner = self
            .worker
            .try_lock()
            .map_err(|_| AppError::new("worker-busy", "Another task owns the worker"))?;
        let status = self.automatic_status()?;
        if !status.enabled {
            return Ok(vec![]);
        }
        let tasks = self.tasks()?;
        if tasks.iter().any(|t| !t.state.terminal()) {
            return Ok(vec![]);
        }
        if status.next_due == 0 {
            let mut db = self.db()?;
            let tx = db.transaction()?;
            let mut current = read(&tx)?;
            current.next_due = now.saturating_add(current.settings.interval_seconds as i64);
            write(&tx, &current)?;
            tx.commit()?;
            return Ok(vec![]);
        }
        if now < status.next_due {
            return Ok(vec![]);
        }
        let config = self.configuration()?;
        let items: Vec<MediaItem> = self
            .all_items()?
            .into_iter()
            .filter(|i| {
                i.parser_revision == library::PARSER_REVISION
                    && config.roots.iter().any(|r| r.id == i.root_id)
            })
            .collect();
        let mut cooling = {
            let db = self.db()?;
            let mut statement = db.prepare("SELECT json_extract(result,'$.result.imdb'),COALESCE(json_extract(result,'$.result.finished_at'),json_extract(result,'$.result.started_at')) FROM operations WHERE rowid IN (SELECT MAX(rowid) FROM operations WHERE json_extract(result,'$.kind')='fetch' AND json_extract(result,'$.result.cached')=0 GROUP BY json_extract(result,'$.result.imdb')) AND json_extract(result,'$.result.phase')!='completed'")?;
            let rows = statement
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.into_iter()
                .filter(|(_, at)| {
                    chrono::DateTime::parse_from_rfc3339(at)
                        .is_ok_and(|t| now.saturating_sub(t.timestamp()) < 3600)
                })
                .map(|(imdb, _)| imdb)
                .collect::<std::collections::HashSet<_>>()
        };
        {
            let db = self.db()?;
            let mut stmt =
                db.prepare("SELECT body FROM preferences WHERE key LIKE 'imdb-failure:%'")?;
            for body in stmt.query_map([], |r| r.get::<_, String>(0))? {
                let failure: crate::imdb_cache::Failure = serde_json::from_str(&body?)?;
                if crate::imdb_cache::fresh(&failure.fetched_at, now, 3600) {
                    cooling.insert(failure.imdb);
                }
            }
        }
        let selected = crate::automatic::candidates(&items, now, |i| cooling.contains(&i.imdb));
        let cycle = {
            let mut db = self.db()?;
            let tx = db.transaction()?;
            let mut current = read(&tx)?;
            if !current.enabled {
                return Ok(vec![]);
            }
            current.cycle = current.cycle.checked_add(1).ok_or_else(|| {
                AppError::new("cycle-overflow", "Automatic cycle counter exhausted")
            })?;
            current.next_due = now.saturating_add(current.settings.interval_seconds as i64);
            write(&tx, &current)?;
            tx.commit()?;
            current.cycle
        };
        let indexed: std::collections::HashMap<_, _> = items.iter().map(|i| (&i.id, i)).collect();
        let mut planned = vec![];
        if items.is_empty() {
            for space in [Space::Movie, Space::Tv] {
                let root_ids: Vec<String> = config
                    .roots
                    .iter()
                    .filter(|r| r.space == space)
                    .map(|r| r.id.clone())
                    .collect();
                if root_ids.is_empty() {
                    continue;
                }
                let request = ScanRequest {
                    operation_id: format!(
                        "auto-index-{cycle}-{}",
                        if space == Space::Movie { "movie" } else { "tv" }
                    ),
                    space,
                    root_ids,
                };
                match self.submit_context(request, true) {
                    Ok(task) => planned.push(task),
                    Err(e) if e.code == "automatic-stopped" => break,
                    Err(e) => return Err(e),
                }
            }
            return Ok(planned);
        }
        for (space, recent) in [
            (Space::Movie, true),
            (Space::Tv, true),
            (Space::Movie, false),
            (Space::Tv, false),
        ] {
            let item_ids: Vec<String> = selected
                .iter()
                .filter(|id| {
                    indexed.get(*id).is_some_and(|i| {
                        i.space == space
                            && (90..=900).contains(&now.saturating_sub(i.modified_at)) == recent
                    })
                })
                .take(10000)
                .cloned()
                .collect();
            if item_ids.is_empty() {
                continue;
            }
            let request = BatchRequest {
                operation_id: format!(
                    "auto-{cycle}-{}-{}",
                    if space == Space::Movie { "movie" } else { "tv" },
                    if recent { "recent" } else { "backfill" }
                ),
                space,
                item_ids,
                root_ids: vec![],
                retry_failed: false,
                engine: BatchEngine::Specs,
                mode: BatchMode::Generate,
            };
            match self.plan_batch_context(request, true) {
                Ok(task) => planned.push(task),
                Err(e) if e.code == "automatic-stopped" => break,
                Err(e) => return Err(e),
            }
        }
        Ok(planned)
    }
}

pub(super) fn planned(db: &Connection) -> Result<()> {
    let mut status = read(db)?;
    if !status.enabled {
        return Err(AppError::new(
            "automatic-stopped",
            "Automatic mode stopped before planning completed",
        ));
    }
    status.next_due = 0;
    write(db, &status)
}
pub(super) fn import_settings(db: &Connection, settings: Settings) -> Result<()> {
    settings.validate()?;
    let mut status = read(db)?;
    if status.enabled {
        return Err(AppError::new(
            "automatic-active",
            "Stop automatic mode before importing its settings",
        ));
    }
    status.settings = settings;
    write(db, &status)
}
