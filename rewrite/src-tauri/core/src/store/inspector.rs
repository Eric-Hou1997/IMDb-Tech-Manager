use super::*;
use crate::inspector::{self, Annotation, AnnotationAction, AnnotationRequest};
use std::collections::BTreeMap;
pub(super) fn annotations(db: &Connection) -> Result<BTreeMap<String, Annotation>> {
    let mut statement =
        db.prepare("SELECT key,body FROM preferences WHERE key LIKE 'inspector:%'")?;
    let mut values = BTreeMap::new();
    for row in statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (key, body) = row?;
        values.insert(key, serde_json::from_str(&body)?);
    }
    Ok(values)
}
fn annotation(db: &Connection, id: &str) -> Result<Annotation> {
    let body: Option<String> = db
        .query_row(
            "SELECT body FROM preferences WHERE key=?1",
            [format!("inspector:{id}")],
            |r| r.get(0),
        )
        .optional()?;
    body.map(|s| serde_json::from_str(&s).map_err(Into::into))
        .unwrap_or(Ok(Annotation::default()))
}
impl Store {
    pub fn inspect_item(&self, id: &str) -> Result<MediaItem> {
        let indexed = self.item(id)?;
        let config = self.configuration()?;
        let root = config
            .roots
            .iter()
            .find(|r| r.id == indexed.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Root is no longer configured"))?;
        // Parse errors remain inspectable. No arbitrary path from the frontend is accepted.
        let mut item = match library::read(root, Path::new(&indexed.path)) {
            Ok(value) => value,
            Err(error) => {
                let mut value = library::empty(root, Path::new(&indexed.path));
                value.error = Some(error);
                value
            }
        };
        item.id = indexed.id;
        let db = self.db()?;
        db.execute(
            "UPDATE items SET body=?2 WHERE id=?1",
            params![id, serde_json::to_string(&item)?],
        )?;
        let value = annotation(&db, &hash(item.path.as_bytes()))?;
        inspector::apply(&mut item, Some(&value));
        Ok(item)
    }
    pub fn annotate(&self, request: AnnotationRequest) -> Result<Annotation> {
        valid_id(&request.operation_id)?;
        let fingerprint = hash(&serde_json::to_vec(&("inspector-annotation", &request))?);
        if let Some(body) = self.operation(&request.operation_id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::Annotation(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                )),
            };
        }
        let item = self.inspect_item(&request.item_id)?;
        if item.source_hash != request.expected_hash || item.source_hash.is_empty() {
            return Err(AppError::new(
                "source-conflict",
                "NFO changed; refresh before confirming its status or issues",
            )
            .at(&item.path));
        }
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        let mut value = annotation(&tx, &hash(item.path.as_bytes()))?;
        if value.issue_hash.is_empty() {
            value.issue_hash = value.source_hash.clone();
        }
        if value.status_hash.is_empty() {
            value.status_hash = value.source_hash.clone();
        }
        value.source_hash = item.source_hash.clone();
        match request.action {
            AnnotationAction::Ignore { issue } => {
                if inspector::protected(&issue) || !item.inspection.issues.contains(&issue) {
                    return Err(AppError::new(
                        "protected-or-missing-issue",
                        "File safety issues cannot be ignored; refresh if the issue changed",
                    )
                    .at(&item.path));
                }
                if value.issue_hash != item.source_hash {
                    value.ignored_kinds.clear();
                }
                value.issue_hash = item.source_hash.clone();
                if !value.ignored_kinds.contains(&issue) {
                    value.ignored_kinds.push(issue);
                    value.ignored_kinds.sort();
                }
            }
            AnnotationAction::RestoreIssues => {
                value.ignored_kinds.clear();
                value.issue_hash = item.source_hash.clone();
            }
            AnnotationAction::SetStatus { value: status } => {
                value.status_override = status;
                value.status_hash = item.source_hash.clone();
            }
        }
        value.updated_at = chrono::Utc::now().to_rfc3339();
        value.validate()?;
        tx.execute("INSERT INTO preferences(key,body) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET body=excluded.body",params![format!("inspector:{}",hash(item.path.as_bytes())),serde_json::to_string(&value)?])?;
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                request.operation_id,
                fingerprint,
                serde_json::to_string(&OperationResult::Annotation(value.clone()))?
            ],
        )?;
        tx.commit()?;
        Ok(value)
    }
}
impl Store {
    pub(super) fn prepare_annotation_migration(
        &self,
        plan: &mut crate::migration::MigrationPlan,
    ) -> Result<()> {
        if !plan.source_kind.starts_with("itm-") {
            return Ok(());
        }
        let db = self.db()?;
        let mut prepared: BTreeMap<String, crate::migration::AdapterPlan> = BTreeMap::new();
        for file in &plan.files {
            let name = file.relative.rsplit('/').next().unwrap_or("");
            if !["issue-acknowledgements.json", "status-overrides.json"].contains(&name) {
                continue;
            }
            let raw = crate::migration::read_snapshot(Path::new(&plan.source), file)?;
            let value: serde_json::Value = serde_json::from_slice(&raw)?;
            let Some(records) = value.as_object() else {
                plan.warnings.push(format!(
                    "Inspector state requires review: {}",
                    file.relative
                ));
                continue;
            };
            if records.len() > 100_000 {
                return Err(AppError::new(
                    "migration-size",
                    "Inspector annotation count exceeds 100000",
                ));
            }
            for (path, record) in records {
                let key = format!("inspector:{}", hash(path.as_bytes()));
                let before: Option<String> = db
                    .query_row("SELECT body FROM preferences WHERE key=?1", [&key], |r| {
                        r.get(0)
                    })
                    .optional()?;
                let mut annotation: Annotation = if let Some(adapter) = prepared.get(&key) {
                    serde_json::from_value(adapter.value.clone())?
                } else {
                    before
                        .as_ref()
                        .map(|s| serde_json::from_str(s))
                        .transpose()?
                        .unwrap_or_default()
                };
                if annotation.issue_hash.is_empty() {
                    annotation.issue_hash = annotation.source_hash.clone();
                }
                if annotation.status_hash.is_empty() {
                    annotation.status_hash = annotation.source_hash.clone();
                }
                let source_hash = record
                    .get("source_hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                annotation.source_hash = source_hash.clone();
                annotation.updated_at = record
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .into();
                if name == "issue-acknowledgements.json" {
                    let Some(kinds) = record.get("kinds").and_then(|v| v.as_array()) else {
                        plan.warnings
                            .push(format!("Invalid issue acknowledgement retained: {path}"));
                        continue;
                    };
                    annotation.ignored_kinds = kinds
                        .iter()
                        .map(|v| v.as_str().unwrap_or("").to_owned())
                        .collect();
                    annotation.issue_hash = source_hash;
                } else {
                    annotation.status_override = record
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into();
                    annotation.status_hash = source_hash;
                }
                if annotation.validate().is_err() {
                    plan.warnings.push(format!(
                        "Unverified Inspector annotation retained without applying: {path}"
                    ));
                    continue;
                }
                let sources = prepared
                    .get(&key)
                    .map(|a| a.source.clone())
                    .unwrap_or_else(|| path.clone());
                prepared.insert(key.clone(),crate::migration::AdapterPlan{source:format!("{sources} · {}",file.relative),target:key,before_hash:before.map(|s|hash(s.as_bytes())),value:serde_json::to_value(annotation)?,warnings:vec!["Applies only to the recorded path and exact NFO revision; it never changes facts, ownership or generation permission".into()]});
            }
        }
        plan.adapters.extend(prepared.into_values());
        Ok(())
    }
}
