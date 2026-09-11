use super::*;
use crate::{legacy_undo::*, writing::WriteIntent};
// Keep the archive for audit while consuming the old one-shot undo right.
// An interrupted write must be recovered under its original operation ID.
pub(super) fn restore_state(
    db: &Connection,
    archive_hash: &str,
    except_id: &str,
) -> Result<Option<String>> {
    Ok(db.query_row(
        "SELECT CASE WHEN json_extract(result,'$.result.phase') LIKE 'committed%' THEN 'consumed' ELSE 'restore-pending' END FROM operations WHERE json_extract(result,'$.kind')='write' AND json_extract(result,'$.result.intent.kind')='legacy-undo' AND json_extract(result,'$.result.intent.proof.archive_hash')=?1 AND id<>?2 AND json_extract(result,'$.result.phase') NOT IN ('preview','failed','not-applied','unchanged') ORDER BY CASE WHEN json_extract(result,'$.result.phase') LIKE 'committed%' THEN 0 ELSE 1 END LIMIT 1",
        params![archive_hash,except_id], |r| r.get(0),
    ).optional()?)
}
fn journal(db: &Connection, import_id: &str, source: &str) -> Result<(Journal, String)> {
    let (expected, category, raw): (String, String, Vec<u8>) = db.query_row(
        "SELECT sha256,category,body FROM legacy_artifacts WHERE import_id=?1 AND path=?2",
        params![import_id, source],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if category != "backup" || hash(&raw) != expected {
        return Err(AppError::new(
            "legacy-undo-archive",
            "Historical undo archive failed its category/checksum check",
        ));
    }
    Ok((serde_json::from_slice(&raw)?, expected))
}
pub(super) fn validate_proof(
    db: &Connection,
    proof: &LegacyUndoProof,
    candidate: &[u8],
) -> Result<()> {
    let (record, archive_hash) = journal(db, &proof.import_id, &proof.source)?;
    if archive_hash != proof.archive_hash
        || record.before_hash != proof.before_hash
        || record.after_hash != proof.after_hash
        || record.expires_at != proof.expires_at
        || record.validate(&proof.source)? != candidate
    {
        return Err(AppError::new(
            "legacy-undo-archive",
            "Reviewed restore no longer matches the imported historical journal",
        ));
    }
    Ok(())
}
impl Store {
    pub fn legacy_undo_entries(&self, item_id: &str, offset: u32) -> Result<LegacyUndoPage> {
        let item = self.item(item_id)?;
        let config = self.configuration()?;
        let root = config
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Root no longer configured"))?;
        let current = library::read(root, Path::new(&item.path))?;
        let db = self.db()?;
        let clause = format!(
            "category='backup' AND ({}=?1 OR {}=?2)",
            crate::legacy_undo::PATH_EXPR,
            crate::legacy_undo::HASH_EXPR
        );
        let total = db.query_row(
            &format!("SELECT COUNT(*) FROM legacy_artifacts WHERE {clause}"),
            params![current.path, current.source_hash],
            |r| r.get(0),
        )?;
        let mut statement=db.prepare(&format!("SELECT import_id,path FROM legacy_artifacts WHERE {clause} ORDER BY rowid DESC LIMIT 20 OFFSET ?3"))?;
        let rows = statement
            .query_map(params![current.path, current.source_hash, offset], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
        let mut entries = vec![];
        for row in rows {
            let (import_id, source) = row?;
            let mut entry = LegacyUndoEntry {
                import_id,
                source,
                old_path: String::new(),
                operation: String::new(),
                expires_at: String::new(),
                state: "invalid".into(),
                error: None,
            };
            let result =
                journal(&db, &entry.import_id, &entry.source).and_then(|(record, archive_hash)| {
                    entry.old_path = record.path.clone();
                    entry.operation = record.operation.clone();
                    entry.expires_at = record.expires_at.clone();
                    record.validate(&entry.source)?;
                    entry.state = if let Some(state) = restore_state(&db, &archive_hash, "")? {
                        state
                    } else if chrono::DateTime::parse_from_rfc3339(&record.expires_at)
                        .is_ok_and(|t| t < chrono::Utc::now())
                    {
                        "expired".into()
                    } else if record.after_hash != current.source_hash {
                        "source-conflict".into()
                    } else if record.path != current.path {
                        "path-confirmation-required".into()
                    } else {
                        "available".into()
                    };
                    Ok(())
                });
            if let Err(error) = result {
                entry.error = Some(error);
            }
            entries.push(entry);
        }
        Ok(LegacyUndoPage { total, entries })
    }
    pub fn preview_legacy_undo(
        &self,
        request: LegacyUndoRequest,
    ) -> Result<crate::writing::WritePreview> {
        valid_id(&request.operation_id)?;
        let fingerprint = hash(&serde_json::to_vec(&("legacy-undo", &request))?);
        if let Some(body) = self.operation(&request.operation_id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::Write(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "Operation belongs to another action",
                )),
            };
        }
        let item = self.item(&request.item_id)?;
        let config = self.configuration()?;
        let root = config
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Root no longer configured"))?;
        let (_, raw) = library::read_bytes(root, Path::new(&item.path))?;
        let (record, archive_hash) = journal(&*self.db()?, &request.import_id, &request.source)?;
        if let Some(state) = restore_state(&*self.db()?, &archive_hash, &request.operation_id)? {
            return Err(AppError::new("legacy-undo-unavailable", state));
        }
        let backup = record.validate(&request.source)?;
        if record.after_hash != request.expected_hash || hash(&raw) != request.expected_hash {
            return Err(AppError::new(
                "source-conflict",
                "Current NFO differs from the historical operation; later edits are protected",
            )
            .at(&item.path));
        }
        if record.path != item.path && !request.confirm_path_mapping {
            return Err(AppError::new(
                "legacy-path-confirmation",
                "Explicitly confirm the historical path mapping before restoring to this file",
            )
            .at(&item.path));
        }
        let proof = LegacyUndoProof {
            import_id: request.import_id,
            source: request.source,
            archive_hash,
            before_hash: record.before_hash,
            after_hash: record.after_hash,
            expires_at: record.expires_at,
            destination: item.path.clone(),
        };
        proof.validate_candidate(Path::new(&item.path), &request.expected_hash, &backup)?;
        self.save_write_preview(
            &request.operation_id,
            &fingerprint,
            &item,
            root,
            (&raw, &backup),
            WriteIntent::LegacyUndo {
                proof: Box::new(proof),
            },
        )
    }
}
