use super::*;
use crate::batch::*;
impl Store {
    /// Original current/selected preflight: read explicit NFOs, never expand
    /// to a whole library or create a task before the user confirms.
    pub fn preflight_items(&self, space: Space, ids: Vec<String>) -> Result<Vec<MediaItem>> {
        if ids.is_empty() || ids.len() > 10000 {
            return Err(AppError::new(
                "invalid-scope",
                "Select between 1 and 10000 indexed NFO entries",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        let mut items = vec![];
        for id in ids {
            if !seen.insert(id.clone()) {
                continue;
            }
            if !self.db()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM items WHERE id=?1)",
                [&id],
                |row| row.get::<_, bool>(0),
            )? {
                return Err(AppError::new(
                    "invalid-scope",
                    "Selected NFO is no longer indexed; reload the library",
                ));
            }
            if self.item(&id)?.space != space {
                return Err(AppError::new(
                    "invalid-scope",
                    "Movie and TV scopes must remain separate",
                ));
            }
            let item = self.inspect_item(&id)?;
            if ["Movie", "Series", "Episode"].contains(&item.kind.as_str())
                && !item
                    .inspection
                    .issues
                    .iter()
                    .any(|issue| issue == "library-type-mismatch")
            {
                if let Some(error) = item.error {
                    return Err(error);
                }
                items.push(item);
            }
        }
        if items.is_empty() {
            return Err(AppError::new(
                "invalid-scope",
                "No processable NFO entries in the selected scope",
            ));
        }
        Ok(items)
    }
    pub fn plan_batch(&self, request: BatchRequest) -> Result<Task> {
        self.plan_batch_context(request, false)
    }
    pub(super) fn plan_batch_context(
        &self,
        mut request: BatchRequest,
        automatic: bool,
    ) -> Result<Task> {
        valid_id(&request.operation_id)?;
        if request.mode == BatchMode::AdoptPreview {
            return Err(AppError::new(
                "review-mismatch",
                "Adoption requires the exact reviewed preview candidates",
            ));
        }
        let fingerprint = if automatic {
            hash(&serde_json::to_vec(&("automatic-batch", &request))?)
        } else {
            hash(&serde_json::to_vec(&("batch", &request))?)
        };
        if let Ok(task) = self.task(&request.operation_id) {
            let old: String = self.db()?.query_row(
                "SELECT fingerprint FROM tasks WHERE id=?1",
                [&request.operation_id],
                |r| r.get(0),
            )?;
            if old != fingerprint {
                return Err(AppError::new(
                    "operation-conflict",
                    "Task ID already has different input",
                ));
            }
            return Ok(task);
        }
        if !request.root_ids.is_empty() {
            if !request.item_ids.is_empty() {
                return Err(AppError::new(
                    "invalid-scope",
                    "Choose a selection or explicit full root scope",
                ));
            }
            let config = self.configuration()?;
            for id in &request.root_ids {
                if !config
                    .roots
                    .iter()
                    .any(|r| r.id == *id && r.space == request.space)
                {
                    return Err(AppError::new(
                        "invalid-scope",
                        "Root does not belong to this media space",
                    ));
                }
            }
            request.item_ids = self
                .all_items()?
                .into_iter()
                .filter(|i| i.space == request.space && request.root_ids.contains(&i.root_id))
                .map(|i| i.id)
                .collect();
        }
        if request.item_ids.is_empty() || request.item_ids.len() > 10000 {
            return Err(AppError::new(
                "invalid-scope",
                "Select between 1 and 10000 indexed NFO entries",
            ));
        }
        if request.mode == BatchMode::Preview && request.item_ids.len() > 10 {
            return Err(AppError::new(
                "preview-limit",
                "Preview at most 10 NFO entries at once",
            ));
        }
        let config = self.configuration()?;
        let settings = self.ai_settings()?;
        if request.engine == BatchEngine::Ai {
            if !settings.enabled {
                return Err(AppError::new(
                    "ai-disabled",
                    "Enable AI in settings before planning generation",
                ));
            }
            settings.validate()?;
        }
        let mut roots = vec![];
        let mut items = vec![];
        let mut seen = std::collections::HashSet::new();
        for id in &request.item_ids {
            if !seen.insert(id) {
                return Err(AppError::new("invalid-scope", "Duplicate NFO entry"));
            }
            let item = self.item(id)?;
            if item.space != request.space {
                return Err(AppError::new(
                    "invalid-scope",
                    "Movie and TV scopes must remain separate",
                )
                .at(&item.path));
            }
            let root = config
                .roots
                .iter()
                .find(|r| r.id == item.root_id)
                .ok_or_else(|| AppError::new("invalid-root", "Root no longer configured"))?;
            if !roots.contains(root) {
                roots.push(root.clone());
            }
            items.push(BatchItem {
                item,
                request_id: child_id(&request.operation_id, items.len(), "request"),
                write_id: child_id(&request.operation_id, items.len(), "write"),
                phase: "pending".into(),
                error: None,
                candidate_hash: None,
            });
        }
        let mut batch = Batch {
            engine: request.engine,
            mode: request.mode,
            plan_hash: String::new(),
            approved: automatic,
            retry_failed: request.retry_failed,
            pause_requested: false,
            cancel_requested: false,
            settings: serde_json::to_value(settings)?,
            total: items.len() as u32,
            items,
        };
        batch.plan_hash = hash(&serde_json::to_vec(&(&batch, &config.locale))?);
        let mut task = Task {
            journal: None,
            automatic,
            id: request.operation_id,
            state: if automatic {
                TaskState::Requested
            } else {
                TaskState::Paused
            },
            locale: config.locale.clone(),
            space: request.space,
            roots,
            attempt: 0,
            processed: 0,
            errors: 0,
            current_path: None,
            failure: None,
            batch: Some(batch),
        };
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        Self::begin_task_log(&tx, &mut task)?;
        if automatic {
            super::automatic::planned(&tx)?;
        }
        let latest: String =
            tx.query_row("SELECT body FROM configuration WHERE id=1", [], |r| {
                r.get(0)
            })?;
        if serde_json::from_str::<Configuration>(&latest)? != config {
            return Err(AppError::new(
                "configuration-conflict",
                "Configuration changed while planning",
            ));
        }
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM operations WHERE id=?1)",
            [&task.id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(AppError::new(
                "operation-conflict",
                "ID belongs to another operation",
            ));
        }
        let mut summary = task.clone();
        for (ordinal, row) in summary.batch.as_mut().unwrap().items.drain(..).enumerate() {
            tx.execute(
                "INSERT INTO batch_items VALUES(?1,?2,?3,?4,?5)",
                params![
                    task.id,
                    ordinal as u32,
                    row.request_id,
                    row.phase,
                    serde_json::to_string(&row)?
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO tasks VALUES(?1,?2,?3)",
            params![task.id, fingerprint, serde_json::to_string(&summary)?],
        )?;
        tx.commit()?;
        Ok(task)
    }
    pub fn approve_batch_scope(&self, id: &str, reviewed_hash: &str) -> Result<Task> {
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        let body: String =
            tx.query_row("SELECT body FROM tasks WHERE id=?1", [id], |r| r.get(0))?;
        let mut task: Task = serde_json::from_str(&body)?;
        let batch = task
            .batch
            .as_mut()
            .ok_or_else(|| AppError::new("invalid-task", "Task is not a batch"))?;
        if batch.plan_hash != reviewed_hash {
            return Err(AppError::new(
                "review-mismatch",
                "Batch scope differs from the reviewed plan",
            ));
        }
        if batch.approved {
            return Ok(task);
        }
        if task.state != TaskState::Paused {
            return Err(AppError::new(
                "invalid-transition",
                "Batch cannot start from this state",
            ));
        }
        batch.approved = true;
        let engine = batch.engine.clone();
        task.state = TaskState::Requested;
        if !task.automatic {
            Self::supersede_stopped(&tx, id, Some((&engine, &task.space)))?;
        }
        Self::record_task(&tx, &mut task)?;
        tx.commit()?;
        Ok(task)
    }
    /// Original preview adoption is a new owned job using already reviewed
    /// write operations. It cannot generate a replacement candidate or call AI.
    pub fn adopt_preview(&self, request: PreviewAdoption) -> Result<Task> {
        valid_id(&request.operation_id)?;
        if request.items.is_empty() || request.items.len() > 10 {
            return Err(AppError::new(
                "invalid-scope",
                "Select 1–10 reviewed preview candidates",
            ));
        }
        let fingerprint = hash(&serde_json::to_vec(&("preview-adoption", &request))?);
        if let Ok(task) = self.task(&request.operation_id) {
            let old: String = self.db()?.query_row(
                "SELECT fingerprint FROM tasks WHERE id=?1",
                [&task.id],
                |r| r.get(0),
            )?;
            if old != fingerprint {
                return Err(AppError::new(
                    "operation-conflict",
                    "Adoption ID belongs to another review",
                ));
            }
            return Ok(task);
        }
        let source = self.task(&request.task_id)?;
        let preview = source
            .batch
            .as_ref()
            .filter(|b| {
                b.approved && b.mode == BatchMode::Preview && b.engine != BatchEngine::Specs
            })
            .ok_or_else(|| {
                AppError::new("review-mismatch", "A saved AI or rule preview is required")
            })?;
        if matches!(source.state, TaskState::Requested | TaskState::Running) {
            return Err(AppError::new(
                "active-task",
                "Wait until the preview job stops",
            ));
        }
        let config = self.configuration()?;
        let mut seen = std::collections::HashSet::new();
        let mut items = vec![];
        for reviewed in &request.items {
            if !seen.insert(&reviewed.write_id) {
                return Err(AppError::new(
                    "invalid-scope",
                    "Duplicate reviewed candidate",
                ));
            }
            let row = preview
                .items
                .iter()
                .find(|row| {
                    row.write_id == reviewed.write_id
                        && row.candidate_hash.as_deref() == Some(&reviewed.reviewed_hash)
                        && ["review-ready", "committed", "unchanged"].contains(&row.phase.as_str())
                })
                .ok_or_else(|| {
                    AppError::new(
                        "review-mismatch",
                        "Candidate differs from the saved preview",
                    )
                })?;
            let root = source
                .roots
                .iter()
                .find(|r| r.id == row.item.root_id)
                .ok_or_else(|| AppError::new("invalid-root", "Preview root is missing"))?;
            if !config.roots.contains(root) {
                return Err(AppError::new(
                    "root-identity-changed",
                    "Reviewed root configuration changed",
                )
                .at(&row.item.path));
            }
            let candidate = match self.operation_result(&row.write_id)? {
                OperationResult::Write(p) => p,
                _ => {
                    return Err(AppError::new(
                        "review-mismatch",
                        "Preview operation has another owner",
                    ))
                }
            };
            if candidate.operation_id != row.write_id
                || candidate.item_id != row.item.id
                || candidate.after_hash != reviewed.reviewed_hash
            {
                return Err(AppError::new(
                    "review-mismatch",
                    "Candidate receipt does not match the review",
                )
                .at(&row.item.path));
            }
            let mut row = row.clone();
            row.request_id = child_id(&request.operation_id, items.len(), "adoption");
            row.phase = "pending".into();
            row.error = None;
            items.push(row);
        }
        let mut batch = Batch {
            engine: preview.engine.clone(),
            mode: BatchMode::AdoptPreview,
            plan_hash: String::new(),
            approved: true,
            retry_failed: false,
            pause_requested: false,
            cancel_requested: false,
            settings: preview.settings.clone(),
            total: items.len() as u32,
            items,
        };
        batch.plan_hash = hash(&serde_json::to_vec(&(
            &batch,
            &config.locale,
            &request.task_id,
        ))?);
        let mut task = Task {
            journal: None,
            automatic: false,
            batch: Some(batch),
            id: request.operation_id,
            state: TaskState::Requested,
            locale: config.locale.clone(),
            space: source.space,
            roots: source.roots,
            attempt: 0,
            processed: 0,
            errors: 0,
            current_path: None,
            failure: None,
        };
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        Self::begin_task_log(&tx, &mut task)?;
        let latest: String =
            tx.query_row("SELECT body FROM configuration WHERE id=1", [], |r| {
                r.get(0)
            })?;
        if serde_json::from_str::<Configuration>(&latest)? != config {
            return Err(AppError::new(
                "configuration-conflict",
                "Settings changed while submitting the reviewed candidates",
            ));
        }
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM operations WHERE id=?1) OR EXISTS(SELECT 1 FROM tasks WHERE id=?1)",[&task.id],|r|r.get::<_,bool>(0))?{return Err(AppError::new("operation-conflict","Adoption ID already has an owner"));}
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM tasks WHERE json_extract(body,'$.state') IN ('requested','running'))",[],|r|r.get::<_,bool>(0))?{return Err(AppError::new("active-task","Another task is already running"));}
        Self::supersede_stopped(&tx, &task.id, None)?;
        let mut summary = task.clone();
        for (ordinal, row) in summary.batch.as_mut().unwrap().items.drain(..).enumerate() {
            tx.execute(
                "INSERT INTO batch_items VALUES(?1,?2,?3,?4,?5)",
                params![
                    task.id,
                    ordinal as u32,
                    row.request_id,
                    row.phase,
                    serde_json::to_string(&row)?
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO tasks VALUES(?1,?2,?3)",
            params![task.id, fingerprint, serde_json::to_string(&summary)?],
        )?;
        tx.commit()?;
        Ok(task)
    }
    pub fn adoption_candidate(
        &self,
        task: &Task,
        row: &BatchItem,
    ) -> Result<crate::writing::WritePreview> {
        if !task
            .batch
            .as_ref()
            .is_some_and(|b| b.approved && b.mode == BatchMode::AdoptPreview)
        {
            return Err(AppError::new(
                "review-mismatch",
                "Not a reviewed adoption task",
            ));
        }
        let candidate = match self.operation_result(&row.write_id)? {
            OperationResult::Write(p) => p,
            _ => {
                return Err(AppError::new(
                    "review-mismatch",
                    "Reviewed write operation is missing",
                ))
            }
        };
        if candidate.operation_id != row.write_id
            || candidate.item_id != row.item.id
            || row.candidate_hash.as_deref() != Some(&candidate.after_hash)
        {
            return Err(
                AppError::new("review-mismatch", "Reviewed candidate changed").at(&row.item.path),
            );
        }
        Ok(candidate)
    }
    /// One worker owns the queue; every completed item is persisted before the
    /// next item starts. Re-entering an interrupted item uses its original IDs.
    pub fn run_batch_next(
        &self,
        stop: impl Fn() -> bool,
        progress: impl Fn(&Task),
        mut execute: impl FnMut(&Task, &BatchItem) -> Result<Outcome>,
    ) -> Result<Option<Task>> {
        let _owner = self
            .worker
            .try_lock()
            .map_err(|_| AppError::new("worker-busy", "Another task owns the worker"))?;
        let mut pending = self.tasks()?;
        pending.reverse();
        pending.sort_by_key(|t| t.automatic);
        let selected = pending.into_iter().find(|t| {
            t.state == TaskState::Requested && t.batch.as_ref().is_some_and(|b| b.approved)
        });
        let Some(mut task) = selected else {
            return Ok(None);
        };
        if task.automatic
            && self
                .tasks()?
                .iter()
                .any(|t| !t.state.terminal() && !t.automatic)
        {
            return Ok(None);
        }
        // A paused/interrupted scan still owns its unfinished index scope.
        if self
            .tasks()?
            .iter()
            .any(|t| t.batch.is_none() && !t.state.terminal())
        {
            return Ok(None);
        }
        task = self.update_batch(&task.id, |t, _| {
            if t.state == TaskState::Requested {
                t.state = TaskState::Running;
                t.attempt = t.attempt.checked_add(1).ok_or_else(|| {
                    AppError::new("attempt-overflow", "Task attempt counter exhausted")
                })?;
                t.failure = None;
            }
            Ok(())
        })?;
        if task.state != TaskState::Running {
            return Ok(Some(task));
        }
        progress(&task);
        loop {
            let stopped = stop();
            let yield_manual = task.automatic
                && self
                    .tasks()?
                    .iter()
                    .any(|t| !t.state.terminal() && !t.automatic);
            let mut selected = None;
            task=self.update_batch(&task.id,|t,db|{
                let b=t.batch.as_ref().unwrap();
                if stopped||b.pause_requested||b.cancel_requested {
                    t.state=if b.cancel_requested{TaskState::Cancelled}else if stopped{TaskState::Interrupted}else{TaskState::Paused};t.current_path=None;
                } else if t.automatic && !super::automatic::enabled(db)? {
                    t.state=TaskState::Cancelled;t.current_path=None;
                } else if yield_manual {
                    t.state=TaskState::Requested;t.current_path=None;
                } else {
                    let next:Option<(u32,String)>=db.query_row("SELECT ordinal,body FROM batch_items WHERE task_id=?1 AND phase IN ('pending','running') ORDER BY ordinal LIMIT 1",[&t.id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
                    if let Some((index,body))=next {
                        let mut row:BatchItem=serde_json::from_str(&body)?;row.phase="running".into();t.current_path=Some(row.item.path.clone());
                        db.execute("UPDATE batch_items SET phase=?3,body=?4 WHERE task_id=?1 AND ordinal=?2",params![t.id,index,row.phase,serde_json::to_string(&row)?])?;selected=Some((index,row));
                    } else {t.state=if t.errors==0{TaskState::Completed}else{TaskState::Failed};t.current_path=None;}
                }Ok(())
            })?;
            progress(&task);
            if task.state != TaskState::Running {
                return Ok(Some(self.task(&task.id)?));
            }
            let (index, mut row) = selected.expect("claimed batch item");
            let outcome = execute(&task, &row).unwrap_or_else(Outcome::failed);
            row.phase = outcome.phase;
            row.error = outcome.error;
            row.candidate_hash = outcome.candidate_hash;
            if let Some(e) = row.error.as_mut() {
                e.path = Some(row.item.path.clone());
                e.operation_id = Some(row.request_id.clone());
            }
            task = self.update_batch(&task.id, |t, db| {
                db.execute(
                    "UPDATE batch_items SET phase=?3,body=?4 WHERE task_id=?1 AND ordinal=?2",
                    params![t.id, index, row.phase, serde_json::to_string(&row)?],
                )?;
                t.processed += 1;
                t.log_line(&format!(
                    "[{}] {} · {} · {} · {} · {}{}",
                    t.processed,
                    row.item.title,
                    row.item.year,
                    row.item.imdb,
                    row.item.path,
                    t.result_label(&row.phase),
                    row.error
                        .as_ref()
                        .map(|e| format!(" · {}", t.error_message(e)))
                        .unwrap_or_default()
                ));
                if row.phase == "failed" {
                    t.errors += 1;
                    t.failure = row.error.clone();
                }
                if row.error.as_ref().is_some_and(|e| {
                    [
                        "auth",
                        "quota",
                        "rate-limit",
                        "paused",
                        "budget-exhausted",
                        "interrupted",
                        "legacy-runtime-unverified",
                    ]
                    .contains(&e.code.as_str())
                }) {
                    t.batch.as_mut().unwrap().pause_requested = true;
                }
                Ok(())
            })?;
            progress(&task);
        }
    }
    pub fn batch_items(&self, id: &str) -> Result<Vec<BatchItem>> {
        let db = self.db()?;
        let mut stmt =
            db.prepare("SELECT body FROM batch_items WHERE task_id=?1 ORDER BY ordinal")?;
        let result = stmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .map(|s| Ok(serde_json::from_str(&s?)?))
            .collect();
        result
    }
    pub(super) fn batch_context(
        &self,
        task_id: &str,
        request_id: &str,
        item_id: &str,
        expected_hash: &str,
        engine: BatchEngine,
    ) -> Result<Task> {
        let task = self.task_summary(task_id)?;
        let batch = task
            .batch
            .as_ref()
            .ok_or_else(|| AppError::new("invalid-task", "Not a batch"))?;
        let body: Option<String> = self
            .db()?
            .query_row(
                "SELECT body FROM batch_items WHERE task_id=?1 AND request_id=?2",
                params![task_id, request_id],
                |r| r.get(0),
            )
            .optional()?;
        let row = body
            .map(|s| serde_json::from_str::<BatchItem>(&s))
            .transpose()?;
        if batch.engine != engine
            || !batch.approved
            || task.state != TaskState::Running
            || !row.is_some_and(|i| i.item.id == item_id && i.item.source_hash == expected_hash)
        {
            return Err(AppError::new(
                "invalid-scope",
                "Request does not belong to the running approved batch",
            ));
        }
        Ok(task)
    }
    pub(super) fn task_summary(&self, id: &str) -> Result<Task> {
        let body: String =
            self.db()?
                .query_row("SELECT body FROM tasks WHERE id=?1", [id], |r| r.get(0))?;
        Ok(serde_json::from_str(&body)?)
    }
    pub fn apply_batch_item(
        &self,
        task_id: &str,
        write_id: &str,
        reviewed_hash: &str,
        journal: &Path,
        cancelled: impl Fn() -> bool,
    ) -> Result<crate::writing::WritePreview> {
        let task = self.task(task_id)?;
        let batch = task
            .batch
            .as_ref()
            .ok_or_else(|| AppError::new("invalid-task", "Not a batch"))?;
        if !batch.approved || task.state == TaskState::Running {
            return Err(AppError::new(
                "active-task",
                "Wait until the batch stops before approving individual results",
            ));
        }
        let index = batch
            .items
            .iter()
            .position(|i| {
                i.write_id == write_id && i.candidate_hash.as_deref() == Some(reviewed_hash)
            })
            .ok_or_else(|| {
                AppError::new(
                    "review-mismatch",
                    "Candidate is not in the reviewed batch results",
                )
            })?;
        let receipt = self.apply_specs(write_id, reviewed_hash, journal, cancelled)?;
        self.update_batch(task_id, |t, db| {
            let body: String = db.query_row(
                "SELECT body FROM batch_items WHERE task_id=?1 AND ordinal=?2",
                params![task_id, index as u32],
                |r| r.get(0),
            )?;
            let mut row: BatchItem = serde_json::from_str(&body)?;
            if row.phase == "failed" {
                t.errors = t.errors.saturating_sub(1);
            }
            row.phase = receipt.phase.clone();
            row.error = receipt.error.clone();
            db.execute(
                "UPDATE batch_items SET phase=?3,body=?4 WHERE task_id=?1 AND ordinal=?2",
                params![
                    task_id,
                    index as u32,
                    row.phase,
                    serde_json::to_string(&row)?
                ],
            )?;
            Ok(())
        })?;
        Ok(receipt)
    }
    fn update_batch(
        &self,
        id: &str,
        change: impl FnOnce(&mut Task, &Connection) -> Result<()>,
    ) -> Result<Task> {
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        let body: String =
            tx.query_row("SELECT body FROM tasks WHERE id=?1", [id], |r| r.get(0))?;
        let mut task: Task = serde_json::from_str(&body)?;
        change(&mut task, &tx)?;
        Self::record_task(&tx, &mut task)?;
        tx.commit()?;
        Ok(task)
    }
}
