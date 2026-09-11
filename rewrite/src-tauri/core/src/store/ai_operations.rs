use super::*;
use crate::ai::{
    self,
    job::{self, Record, Request, Settings},
    HttpResponse, Meter,
};
use serde_json::{json, Value};

fn record(db: &Connection, id: &str) -> Result<Record> {
    let body: String = db.query_row("SELECT result FROM operations WHERE id=?1", [id], |r| {
        r.get(0)
    })?;
    match serde_json::from_str(&body)? {
        OperationResult::Ai(value) => Ok(*value),
        _ => Err(AppError::new(
            "operation-conflict",
            "Operation is not an AI request",
        )),
    }
}
fn save(db: &Connection, value: &Record) -> Result<()> {
    db.execute(
        "UPDATE operations SET result=?2 WHERE id=?1",
        params![
            value.request.operation_id,
            serde_json::to_string(&OperationResult::Ai(Box::new(value.clone())))?
        ],
    )?;
    Ok(())
}
impl Store {
    pub fn save_ai_profile(
        &self,
        id: &str,
        mut settings: Settings,
        secret: Option<&str>,
        credentials: &dyn crate::services::CredentialStore,
    ) -> Result<Settings> {
        valid_id(id)?;
        settings.validate()?;
        let previous = match self.operation_result(id) {
            Ok(OperationResult::AiSettings(value)) => Some(value),
            Ok(_) => {
                return Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                ))
            }
            Err(error) if error.code == "operation-not-found" => None,
            Err(error) => return Err(error),
        };
        if previous.is_none() {
            self.writable()?;
        }
        let mut created_account = None;
        if let Some(secret) = secret.filter(|s| !s.is_empty()) {
            let account = format!("ai-{}", hash(id.as_bytes()));
            if let Some(existing) = credentials.get(&account)? {
                if existing != secret {
                    return Err(AppError::new(
                        "operation-conflict",
                        "Settings operation already holds a different credential",
                    ));
                }
            } else {
                credentials.put(&account, secret)?;
                created_account = Some(account.clone());
            }
            settings.credential_account = account;
        } else {
            // Never accept an arbitrary keyring account supplied by the UI.
            settings.credential_account = match previous {
                Some(value) => value.credential_account,
                None => self.ai_settings()?.credential_account,
            };
        }
        match self.save_ai_settings(id, settings) {
            Ok(value) => Ok(value),
            Err(mut error) => {
                // Roll back only a newly created key with a proven uncommitted
                // settings operation. Uncertain outcomes retain the same ID for recovery.
                if let Some(account) = created_account {
                    if self
                        .operation_result(id)
                        .is_err_and(|e| e.code == "operation-not-found")
                    {
                        if let Err(cleanup) = credentials.delete(&account) {
                            error = AppError::new(
                                "credential-cleanup",
                                format!("{}; {}", error.code, cleanup.message),
                            );
                        }
                    }
                }
                error.operation_id = Some(id.into());
                Err(error)
            }
        }
    }
    pub fn ai_settings(&self) -> Result<Settings> {
        self.db()?
            .query_row(
                "SELECT body FROM preferences WHERE key='ai-settings'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(|s| serde_json::from_str(&s).map_err(Into::into))
            .unwrap_or_else(|| Ok(Settings::default()))
    }
    pub fn save_ai_settings(&self, id: &str, settings: Settings) -> Result<Settings> {
        valid_id(id)?;
        settings.validate()?;
        let fingerprint = hash(&serde_json::to_vec(&("ai-settings", &settings))?);
        if let Some(body) = self.operation(id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::AiSettings(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                )),
            };
        }
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(AppError::new("operation-conflict", "ID belongs to a task"));
        }
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                id,
                fingerprint,
                serde_json::to_string(&OperationResult::AiSettings(settings.clone()))?
            ],
        )?;
        tx.execute("INSERT INTO preferences VALUES('ai-settings',?1) ON CONFLICT(key) DO UPDATE SET body=excluded.body",[serde_json::to_string(&settings)?])?;
        tx.execute("DELETE FROM preferences WHERE key='ai-paused'", [])?;
        tx.commit()?;
        Ok(settings)
    }
    pub fn ai_record(&self, id: &str) -> Result<Record> {
        record(&*self.db()?, id)
    }
    pub fn begin_ai(&self, request: Request) -> Result<(Record, bool)> {
        self.begin_ai_context(request, None)
    }
    pub fn begin_batch_ai(&self, request: Request, task_id: &str) -> Result<(Record, bool)> {
        let task = self.batch_context(
            task_id,
            &request.operation_id,
            &request.item_id,
            &request.expected_hash,
            crate::batch::BatchEngine::Ai,
        )?;
        self.begin_ai_context(request, Some(task))
    }
    fn begin_ai_context(&self, request: Request, task: Option<Task>) -> Result<(Record, bool)> {
        valid_id(&request.operation_id)?;
        let identity = hash(&serde_json::to_vec(&("ai-generate", &request))?);
        if let Some(body) = self.operation(&request.operation_id, &identity)? {
            return match serde_json::from_str(&body)? {
                OperationResult::Ai(value) => Ok((*value, false)),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                )),
            };
        }
        let mut settings: Settings = if let Some(task) = &task {
            serde_json::from_value(task.batch.as_ref().unwrap().settings.clone())?
        } else {
            self.ai_settings()?
        };
        if !settings.enabled {
            return Err(AppError::new(
                "ai-disabled",
                "Enable AI in settings before starting generation",
            ));
        }
        settings.validate()?;
        let item = self.item(&request.item_id)?;
        let configuration = self.configuration()?;
        let locale = task
            .as_ref()
            .map(|t| t.locale.clone())
            .unwrap_or_else(|| configuration.locale.clone());
        settings.config.output_language = serde_json::to_value(&locale)?
            .as_str()
            .expect("locale string")
            .into();
        let root = configuration
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Item root is no longer configured"))?;
        let (_, raw) = library::read_bytes(root, Path::new(&item.path))?;
        crate::inspector::generation_allowed(&library::parse(root, Path::new(&item.path), &raw)?)?;
        if hash(&raw) != request.expected_hash {
            return Err(
                AppError::new("source-conflict", "NFO changed since inspection").at(&item.path),
            );
        }
        let current = library::parse(root, Path::new(&item.path), &raw)?;
        let specs = crate::tags::effective_specs(&raw)?;
        if !crate::specs::TAG_SECTIONS
            .iter()
            .any(|k| specs.get(*k).is_some_and(|v| !v.is_empty()))
        {
            return Err(AppError::new(
                "no-tag-specs",
                "No effective tag specifications",
            ));
        }
        let existing = current
            .tags
            .iter()
            .filter(|t| t.ownership == Ownership::Generated)
            .map(|t| json!({"value":t.value,"source":if t.engine=="ai" {"ai"}else{"rules"}}))
            .collect::<Vec<_>>();
        let fingerprint = hash(&serde_json::to_vec(&(
            "ai-runtime-1",
            ai::failure_fingerprint(&settings.config, &specs, &existing)?,
            &settings,
        ))?);
        let mut value = Record {
            batch_id: task.map(|t| t.id),
            engine: "ai".into(),
            request,
            path: item.path,
            title: item.title,
            year: item.year,
            imdb: item.imdb,
            media_kind: item.kind,
            locale,
            settings,
            specs,
            existing,
            fingerprint,
            phase: "requested".into(),
            cached: false,
            legacy_cache: None,
            meter: Meter::default(),
            cost: 0.0,
            historical_cost: 0.0,
            result: None,
            error: None,
            attempts: vec![],
            started_at: job::now(),
            finished_at: None,
        };
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [&value.request.operation_id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(AppError::new("operation-conflict", "ID belongs to a task"));
        }
        if !value.request.force {
            if let Some(body) = tx
                .query_row(
                    "SELECT body FROM ai_cache WHERE fingerprint=?1",
                    [&value.fingerprint],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                let cached: Record = serde_json::from_str(&body)?;
                if let Some(result) = cached.result {
                    value.result = Some(ai::validate(&result, &value.specs)?);
                    value.cached = true;
                    value.phase = "review-ready".into();
                    value.meter.cache_hit(cached.meter.current);
                    value.historical_cost = cached.cost;
                    value.finished_at = Some(job::now());
                }
            }
        }
        if !value.request.force && !value.cached {
            super::legacy_ai_cache::reuse(&tx, &mut value)?;
        }
        if !value.cached && !value.request.retry_failed {
            if let Some(error) = tx
                .query_row(
                    "SELECT error FROM ai_failures WHERE fingerprint=?1",
                    [&value.fingerprint],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                value.error = Some(serde_json::from_str(&error)?);
                value.phase = "skipped-unchanged-failure".into();
                value.finished_at = Some(job::now());
            }
        }
        if ["requested", "skipped-unchanged-failure"].contains(&value.phase.as_str())
            && !value.request.retry_failed
        {
            if let Some(error) = tx
                .query_row(
                    "SELECT body FROM preferences WHERE key='ai-paused'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                value.phase = "paused".into();
                value.error = Some(serde_json::from_str(&error)?);
                value.finished_at = Some(job::now());
            }
        }
        job::fallback(&mut value);
        let execute = value.phase == "requested";
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                value.request.operation_id,
                identity,
                serde_json::to_string(&OperationResult::Ai(Box::new(value.clone())))?
            ],
        )?;
        tx.commit()?;
        Ok((value, execute))
    }
    pub fn reserve_ai_attempt(&self, id: &str, body: Value) -> Result<Record> {
        let db = self.db()?;
        self.writable()?;
        let mut value = record(&db, id)?;
        if !value.request.retry_failed {
            if let Some(error) = db
                .query_row(
                    "SELECT body FROM preferences WHERE key='ai-paused'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                return Err(serde_json::from_str(&error)?);
            }
        }
        if let Some(batch_id) = &value.batch_id {
            let (attempts,tokens,cost):(u64,u64,f64)=db.query_row("SELECT COALESCE(SUM(json_extract(result,'$.result.meter.attempts')),0),COALESCE(SUM(json_extract(result,'$.result.meter.current.total')),0),COALESCE(SUM(json_extract(result,'$.result.cost')),0.0) FROM operations WHERE json_extract(result,'$.kind')='ai' AND json_extract(result,'$.result.batch_id')=?1",[batch_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let s = &value.settings;
            if s.run_request_limit > 0 && attempts >= s.run_request_limit
                || s.run_token_limit > 0 && tokens >= s.run_token_limit
                || s.run_cost_limit > 0.0 && cost >= s.run_cost_limit
            {
                return Err(AppError::new(
                    "budget-exhausted",
                    "The complete batch reached its configured budget",
                ));
            }
        }
        let Some((expected, _)) = job::next(&value)? else {
            return Err(AppError::new(
                "ai-not-runnable",
                "AI request cannot run another attempt",
            ));
        };
        if expected != body {
            return Err(AppError::new(
                "ai-request-mismatch",
                "Outgoing body differs from the authoritative retry plan",
            ));
        }
        value.phase = "running".into();
        value.meter.attempts += 1;
        value.attempts.push(job::Attempt {
            sequence: value.attempts.len() as u32 + 1,
            request: body,
            phase: "dispatched".into(),
            started_at: job::now(),
            finished_at: None,
            http_status: None,
            raw_usage: None,
            error: None,
            retry_after_seconds: 0.0,
        });
        save(&db, &value)?;
        Ok(value)
    }
    pub fn observe_ai_attempt(
        &self,
        id: &str,
        response: Result<HttpResponse>,
        retry_after: f64,
    ) -> Result<Record> {
        let mut db = self.db()?;
        let tx = db.transaction()?;
        let mut value = record(&tx, id)?;
        let attempt = value
            .attempts
            .last_mut()
            .filter(|a| a.phase == "dispatched")
            .ok_or_else(|| {
                AppError::new(
                    "ai-attempt-missing",
                    "No dispatched request is awaiting a response",
                )
            })?;
        if let Ok(response) = &response {
            attempt.http_status = Some(response.status);
            attempt.raw_usage = serde_json::from_slice::<Value>(&response.body)
                .ok()
                .and_then(|v| v.get("usage").cloned());
        }
        let parsed = response.and_then(|response| {
            value
                .meter
                .observe(&value.settings.config.protocol, &value.specs, response)
        });
        attempt.finished_at = Some(job::now());
        attempt.phase = "recorded".into();
        attempt.error = parsed.as_ref().err().cloned();
        attempt.retry_after_seconds = if retry_after.is_finite() {
            retry_after.max(0.0)
        } else {
            0.0
        };
        value.cost = value.settings.cost(&value.meter.current);
        // A completed HTTP response still contributes usage when cancellation races its arrival.
        if value.phase != "cancelled" {
            match parsed {
                Ok(result) => {
                    value.result = Some(result);
                    value.error = None;
                    value.phase = "review-ready".into();
                    value.finished_at = Some(job::now());
                    tx.execute("INSERT INTO ai_cache VALUES(?1,?2) ON CONFLICT(fingerprint) DO UPDATE SET body=excluded.body",params![value.fingerprint,serde_json::to_string(&value)?])?;
                    tx.execute(
                        "DELETE FROM ai_failures WHERE fingerprint=?1",
                        [&value.fingerprint],
                    )?;
                    tx.execute("DELETE FROM preferences WHERE key='ai-paused'", [])?;
                }
                Err(error) => {
                    let cancelled = error.code == "cancelled";
                    if ["auth", "quota"].contains(&error.code.as_str()) {
                        tx.execute("INSERT INTO preferences VALUES('ai-paused',?1) ON CONFLICT(key) DO UPDATE SET body=excluded.body",[serde_json::to_string(&error)?])?;
                    }
                    value.error = Some(error);
                    let next = job::next(&value);
                    if cancelled || !matches!(next, Ok(Some(_))) {
                        value.phase = if cancelled { "cancelled" } else { "failed" }.into();
                        value.finished_at = Some(job::now());
                        if let Err(error) = next {
                            value.error = Some(error);
                        }
                        if !cancelled
                            && value
                                .error
                                .as_ref()
                                .is_none_or(|e| e.code != "budget-exhausted")
                        {
                            tx.execute("INSERT INTO ai_failures VALUES(?1,?2) ON CONFLICT(fingerprint) DO UPDATE SET error=excluded.error",params![value.fingerprint,serde_json::to_string(&value.error)?])?;
                        }
                    }
                }
            }
        }
        job::fallback(&mut value);
        save(&tx, &value)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn end_ai(&self, id: &str, error: AppError) -> Result<Record> {
        let db = self.db()?;
        let mut value = record(&db, id)?;
        if ["requested", "running"].contains(&value.phase.as_str()) {
            value.phase = if error.code == "cancelled" {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            value.error = Some(error);
            value.finished_at = Some(job::now());
            save(&db, &value)?;
        }
        Ok(value)
    }
    pub fn ai_history(&self, item_id: Option<&str>) -> Result<Vec<Record>> {
        let db = self.db()?;
        let mut q=db.prepare("SELECT result FROM operations WHERE json_extract(result,'$.kind')='ai' AND (?1 IS NULL OR json_extract(result,'$.result.request.item_id')=?1) ORDER BY rowid DESC LIMIT 100")?;
        let mut out = vec![];
        for row in q.query_map([item_id], |r| r.get::<_, String>(0))? {
            if let OperationResult::Ai(value) = serde_json::from_str(&row?)? {
                out.push(*value);
            }
        }
        Ok(out)
    }
    pub fn preview_ai(&self, id: &str, ai_id: &str) -> Result<crate::writing::WritePreview> {
        valid_id(id)?;
        let value = self.ai_record(ai_id)?;
        if value.phase != "review-ready" {
            return Err(AppError::new(
                "ai-result-unavailable",
                "A completed, validated AI result is required",
            ));
        }
        let result = ai::validate(
            value
                .result
                .as_ref()
                .ok_or_else(|| AppError::new("ai-result-unavailable", "AI result is missing"))?,
            &value.specs,
        )?;
        let entries = result["tags"]
            .as_array()
            .expect("validated tags")
            .iter()
            .map(|v| serde_json::from_value(v.clone()).map_err(Into::into))
            .collect::<Result<Vec<crate::tags::GeneratedEntry>>>()?;
        let fingerprint = hash(&serde_json::to_vec(&("ai-write", ai_id))?);
        if let Some(body) = self.operation(id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::Write(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                )),
            };
        }
        self.tag_preview(
            id,
            &fingerprint,
            &value.request.item_id,
            &value.request.expected_hash,
            crate::tags::Action::Generate {
                entries,
                engine: value.engine.clone(),
                model: if value.engine == "local-rules" {
                    "4.0.0".into()
                } else {
                    value
                        .legacy_cache
                        .as_ref()
                        .map(|c| &c.model)
                        .filter(|m| !m.is_empty())
                        .cloned()
                        .unwrap_or(value.settings.config.model)
                },
                prompt_hash: if value.engine == "local-rules" {
                    String::new()
                } else {
                    hash(
                        format!(
                            "{}\n\n{}",
                            value.settings.config.prompt.trim(),
                            ai::LANGUAGE_BOUNDARY
                        )
                        .as_bytes(),
                    )[..16]
                        .into()
                },
            },
        )
    }
}
