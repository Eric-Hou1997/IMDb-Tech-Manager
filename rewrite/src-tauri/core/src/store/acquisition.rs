use super::*;
use crate::acquisition::{FetchRecord, FetchRequest};
impl Store {
    pub fn begin_fetch(&self, request: FetchRequest) -> Result<(FetchRecord, bool)> {
        self.begin_fetch_context(request, None)
    }
    pub fn begin_batch_fetch(
        &self,
        request: FetchRequest,
        task_id: &str,
    ) -> Result<(FetchRecord, bool)> {
        let task = self.batch_context(
            task_id,
            &request.operation_id,
            &request.item_id,
            &request.expected_hash,
            crate::batch::BatchEngine::Specs,
        )?;
        self.begin_fetch_context(request, Some(task.locale))
    }
    fn begin_fetch_context(
        &self,
        request: FetchRequest,
        locale: Option<Locale>,
    ) -> Result<(FetchRecord, bool)> {
        valid_id(&request.operation_id)?;
        let fingerprint = hash(&serde_json::to_vec(&("imdb-fetch", &request))?);
        if let Some(body) = self.operation(&request.operation_id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::Fetch(value) => Ok((value, false)),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "Operation belongs to another action",
                )),
            };
        }
        let item = self.item(&request.item_id)?;
        let configuration = self.configuration()?;
        let root = configuration
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Item root is no longer configured"))?;
        let (_, raw) = library::read_bytes(root, Path::new(&item.path))?;
        if hash(&raw) != request.expected_hash {
            return Err(
                AppError::new("source-conflict", "NFO changed since inspection").at(&item.path),
            );
        }
        let current = library::parse(root, Path::new(&item.path), &raw)?;
        crate::specs::imdb_url(&current.imdb)?;
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [&request.operation_id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(AppError::new("operation-conflict", "ID belongs to a task"));
        }
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
                    "ID belongs to different input",
                ));
            }
            return match serde_json::from_str(&body)? {
                OperationResult::Fetch(value) => Ok((value, false)),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "Operation belongs to another action",
                )),
            };
        }
        let cached: Option<crate::specs::SourceSpecs> = if request.refresh {
            None
        } else {
            tx.query_row(
                "SELECT body FROM imdb_cache WHERE imdb=?1 AND parser_version=1",
                [&current.imdb],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(|body| serde_json::from_str(&body))
            .transpose()?
        };
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let value = FetchRecord {
            request,
            imdb: current.imdb,
            path: current.path,
            locale: locale.unwrap_or(configuration.locale),
            phase: if cached.is_some() {
                "completed"
            } else {
                "requested"
            }
            .into(),
            cached: cached.is_some(),
            started_at: now.clone(),
            finished_at: cached.as_ref().map(|_| now),
            source: cached,
            error: None,
            attempts: Vec::new(),
        };
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                value.request.operation_id,
                fingerprint,
                serde_json::to_string(&OperationResult::Fetch(value.clone()))?
            ],
        )?;
        tx.commit()?;
        Ok((value.clone(), !value.cached))
    }
    pub fn fetch_record(&self, id: &str) -> Result<FetchRecord> {
        match self.operation_result(id)? {
            OperationResult::Fetch(value) => Ok(value),
            _ => Err(AppError::new(
                "operation-conflict",
                "Operation is not an IMDb request",
            )),
        }
    }
    pub fn finish_fetch(
        &self,
        id: &str,
        result: Result<crate::specs::SourceSpecs>,
    ) -> Result<FetchRecord> {
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        let body: String =
            tx.query_row("SELECT result FROM operations WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        let mut value = match serde_json::from_str(&body)? {
            OperationResult::Fetch(value) => value,
            _ => {
                return Err(AppError::new(
                    "operation-conflict",
                    "Operation is not an IMDb request",
                ))
            }
        };
        if value.phase != "requested" {
            return Ok(value);
        }
        value.finished_at =
            Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        match result {
            Ok(source) => {
                if source.imdb != value.imdb {
                    return Err(AppError::new(
                        "imdb-title-mismatch",
                        "Fetched data belongs to a different title",
                    ));
                }
                tx.execute("INSERT INTO imdb_cache VALUES(?1,1,?2) ON CONFLICT(imdb) DO UPDATE SET parser_version=1,body=excluded.body",params![source.imdb,serde_json::to_string(&source)?])?;
                value.source = Some(source);
                value.phase = "completed".into();
            }
            Err(mut error) => {
                error.path = Some(value.path.clone());
                error.operation_id = Some(id.into());
                value.phase = if error.code == "cancelled" {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                value.error = Some(error);
            }
        }
        tx.execute(
            "UPDATE operations SET result=?2 WHERE id=?1",
            params![
                id,
                serde_json::to_string(&OperationResult::Fetch(value.clone()))?
            ],
        )?;
        tx.commit()?;
        Ok(value)
    }
    pub fn fetch_history(&self, item_id: &str) -> Result<Vec<FetchRecord>> {
        let db = self.db()?;
        let mut query=db.prepare("SELECT result FROM operations WHERE json_extract(result,'$.kind')='fetch' AND json_extract(result,'$.result.request.item_id')=?1 ORDER BY rowid DESC LIMIT 20")?;
        let mut out = Vec::new();
        for row in query.query_map([item_id], |r| r.get::<_, String>(0))? {
            if let OperationResult::Fetch(record) = serde_json::from_str(&row?)? {
                out.push(record);
            }
        }
        Ok(out)
    }
    pub fn record_fetch_attempt(
        &self,
        id: &str,
        transport: &str,
        error: Option<AppError>,
    ) -> Result<()> {
        let db = self.db()?;
        self.writable()?;
        let body: String =
            db.query_row("SELECT result FROM operations WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        let mut value = match serde_json::from_str(&body)? {
            OperationResult::Fetch(value) => value,
            _ => {
                return Err(AppError::new(
                    "operation-conflict",
                    "Operation is not an IMDb request",
                ))
            }
        };
        value.attempts.push(crate::acquisition::FetchAttempt {
            transport: transport.into(),
            finished_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            error,
        });
        db.execute(
            "UPDATE operations SET result=?2 WHERE id=?1",
            params![id, serde_json::to_string(&OperationResult::Fetch(value))?],
        )?;
        Ok(())
    }
    pub fn cancel_fetch(&self, id: &str) -> Result<FetchRecord> {
        self.finish_fetch(
            id,
            Err(AppError::new(
                "cancelled",
                "IMDb request cancelled; no NFO was changed",
            )),
        )
    }
}
