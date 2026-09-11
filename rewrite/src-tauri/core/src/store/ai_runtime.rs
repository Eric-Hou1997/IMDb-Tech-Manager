use super::*;
use crate::ai::runtime::State;
pub(super) fn read(db: &Connection) -> Result<State> {
    if let Some(body) = db
        .query_row(
            "SELECT body FROM preferences WHERE key='ai-runtime'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(serde_json::from_str(&body)?);
    }
    let old = db
        .query_row(
            "SELECT body FROM preferences WHERE key='ai-paused'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    if let Some(body) = old {
        let error: AppError = serde_json::from_str(&body)?;
        return Ok(State {
            paused: true,
            reason_kind: error.code,
            reason: error.message,
            ..Default::default()
        });
    }
    Ok(State::default())
}
pub(super) fn write(db: &Connection, state: &State) -> Result<()> {
    db.execute("INSERT INTO preferences VALUES('ai-runtime',?1) ON CONFLICT(key) DO UPDATE SET body=excluded.body",[serde_json::to_string(state)?])?;
    db.execute("DELETE FROM preferences WHERE key='ai-paused'", [])?;
    Ok(())
}
pub(super) fn pause(db: &Connection, error: &AppError) -> Result<()> {
    let mut state = read(db)?;
    let now = crate::ai::job::now();
    state.paused = true;
    state.reason_kind = error.code.clone();
    state.reason = error.message.chars().take(1000).collect();
    state.paused_at = now.clone();
    state.last_error_at = now.clone();
    state.updated_at = now;
    write(db, &state)
}
fn clear_account_failures(db: &Connection) -> Result<()> {
    db.execute("DELETE FROM ai_failures WHERE json_extract(error,'$.code') IN ('auth','quota','rate-limit')",[])?;
    Ok(())
}
pub(super) fn success(db: &Connection, connection_test: bool) -> Result<()> {
    let mut state = read(db)?;
    state.last_success_at = crate::ai::job::now();
    state.updated_at = state.last_success_at.clone();
    if ["transient", "rate-limit"].contains(&state.reason_kind.as_str())
        || connection_test && ["auth", "quota"].contains(&state.reason_kind.as_str())
    {
        state.clear();
        clear_account_failures(db)?;
    }
    write(db, &state)
}
impl Store {
    pub fn ai_runtime(&self) -> Result<State> {
        read(&*self.db()?)
    }
    pub fn resume_ai_runtime(&self, id: &str) -> Result<State> {
        valid_id(id)?;
        let fingerprint = hash(b"resume-ai-runtime");
        if let Some(body) = self.operation(id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::AiRuntime(state) => Ok(state),
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
        let mut state = read(&tx)?;
        state.clear();
        write(&tx, &state)?;
        clear_account_failures(&tx)?;
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                id,
                fingerprint,
                serde_json::to_string(&OperationResult::AiRuntime(state.clone()))?
            ],
        )?;
        tx.commit()?;
        Ok(state)
    }
    pub(super) fn prepare_runtime_migration(
        &self,
        plan: &mut crate::migration::MigrationPlan,
    ) -> Result<()> {
        if !plan.source_kind.starts_with("itm-") {
            return Ok(());
        }
        for file in &plan.files {
            if !["ai-runtime.json", "data/ai-runtime.json"].contains(&file.relative.as_str()) {
                continue;
            }
            if plan.adapters.iter().any(|a| a.target == "ai-runtime") {
                return Err(AppError::new(
                    "migration-ambiguous-ai-runtime",
                    "Select one historical AI runtime source",
                ));
            }
            let raw = crate::migration::read_snapshot(Path::new(&plan.source), file)?;
            let value = serde_json::from_slice(&raw).unwrap_or(serde_json::Value::Null);
            let (state, warning) = State::import(value);
            let before: Option<String> = self
                .db()?
                .query_row(
                    "SELECT body FROM preferences WHERE key='ai-runtime'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            let mut warnings=vec!["Historical global pause and timestamps are preserved; importing or saving settings does not resume requests".into()];
            warnings.extend(warning);
            plan.adapters.push(crate::migration::AdapterPlan {
                source: file.relative.clone(),
                target: "ai-runtime".into(),
                before_hash: before.map(|s| hash(s.as_bytes())),
                value: serde_json::to_value(state)?,
                warnings,
            });
        }
        Ok(())
    }
}
