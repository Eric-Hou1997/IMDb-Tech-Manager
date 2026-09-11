use super::*;
use crate::writing::WriteIntent;
fn technical_xml(raw: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(raw)
        .map_err(|e| AppError::new("invalid-encoding", e))?
        .trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(text).map_err(|e| AppError::new("invalid-xml", e))?;
    Ok(doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("technicalspecs"))
        .map(|n| text[n.range()].into())
        .unwrap_or_default())
}

impl Store {
    pub fn preview_specs(
        &self,
        request: crate::writing::SpecsEdit,
    ) -> Result<crate::writing::WritePreview> {
        valid_id(&request.operation_id)?;
        let fingerprint = hash(&serde_json::to_vec(&("specs-edit", &request))?);
        if let Some(result) = self.operation(&request.operation_id, &fingerprint)? {
            return match serde_json::from_str(&result)? {
                OperationResult::Write(value) => Ok(value),
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
        let (path, raw) = library::read_bytes(root, Path::new(&item.path))?;
        if hash(&raw) != request.expected_hash {
            return Err(AppError::new(
                "source-conflict",
                "NFO changed since inspection; reload before editing",
            )
            .at(path.display()));
        }
        let candidate = crate::specs::manual_candidate(&raw, &request.specs)?;
        self.save_write_preview(
            &request.operation_id,
            &fingerprint,
            &item,
            root,
            (&raw, &candidate),
            WriteIntent::Specs,
        )
    }
    pub fn preview_source(&self, id: &str, fetch_id: &str) -> Result<crate::writing::WritePreview> {
        valid_id(id)?;
        let fingerprint = hash(&serde_json::to_vec(&("imdb-write", fetch_id))?);
        if let Some(body) = self.operation(id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::Write(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "Operation belongs to another action",
                )),
            };
        }
        let fetched = self.fetch_record(fetch_id)?;
        if fetched.phase != "completed" {
            return Err(AppError::new(
                "fetch-incomplete",
                "Fetch must complete before previewing a write",
            ));
        }
        let source = fetched
            .source
            .ok_or_else(|| AppError::new("fetch-incomplete", "Fetched source data is missing"))?;
        let item = self.item(&fetched.request.item_id)?;
        let config = self.configuration()?;
        let root = config
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Root is no longer configured"))?;
        let (_, raw) = library::read_bytes(root, Path::new(&item.path))?;
        if hash(&raw) != fetched.request.expected_hash {
            return Err(AppError::new(
                "source-conflict",
                "NFO changed during acquisition; reload before creating another preview",
            )
            .at(&item.path));
        }
        let candidate = crate::specs::source_candidate(&raw, &source)?;
        self.save_write_preview(
            id,
            &fingerprint,
            &item,
            root,
            (&raw, &candidate),
            WriteIntent::Specs,
        )
    }
    pub fn preview_tags(
        &self,
        request: crate::writing::TagEdit,
    ) -> Result<crate::writing::WritePreview> {
        valid_id(&request.operation_id)?;
        let fingerprint = hash(&serde_json::to_vec(&("tag-edit", &request))?);
        if let Some(body) = self.operation(&request.operation_id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::Write(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                )),
            };
        }
        // Generated results must come from an authoritative rule/AI operation, never arbitrary IPC input.
        if matches!(request.action, crate::tags::Action::Generate { .. }) {
            return Err(AppError::new(
                "generation-required",
                "Use a rule or approved AI generation result",
            ));
        }
        self.tag_preview(
            &request.operation_id,
            &fingerprint,
            &request.item_id,
            &request.expected_hash,
            request.action,
        )
    }
    pub fn preview_rules(
        &self,
        id: &str,
        item_id: &str,
        expected_hash: &str,
    ) -> Result<crate::writing::WritePreview> {
        valid_id(id)?;
        let fingerprint = hash(&serde_json::to_vec(&("rule-tags", item_id, expected_hash))?);
        if let Some(body) = self.operation(id, &fingerprint)? {
            return match serde_json::from_str(&body)? {
                OperationResult::Write(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                )),
            };
        }
        let item = self.item(item_id)?;
        let configuration = self.configuration()?;
        let root = configuration
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Item root is no longer configured"))?;
        let (_, raw) = library::read_bytes(root, Path::new(&item.path))?;
        if hash(&raw) != expected_hash {
            return Err(
                AppError::new("source-conflict", "NFO changed since inspection").at(&item.path),
            );
        }
        let entries = crate::rules::entries(&crate::tags::effective_specs(&raw)?)
            .into_iter()
            .map(|e| crate::tags::GeneratedEntry {
                value: e.value,
                field: e.field,
                source_indexes: e.source_indexes,
                confidence: "high".into(),
                operation: "local-rule".into(),
            })
            .collect();
        self.tag_preview(
            id,
            &fingerprint,
            item_id,
            expected_hash,
            crate::tags::Action::Generate {
                entries,
                engine: "local-rules".into(),
                model: "4.0.0".into(),
                prompt_hash: String::new(),
            },
        )
    }
    pub(super) fn tag_preview(
        &self,
        id: &str,
        fingerprint: &str,
        item_id: &str,
        expected_hash: &str,
        action: crate::tags::Action,
    ) -> Result<crate::writing::WritePreview> {
        let item = self.item(item_id)?;
        let config = self.configuration()?;
        let root = config
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Item root is no longer configured"))?;
        let (_, raw) = library::read_bytes(root, Path::new(&item.path))?;
        if hash(&raw) != expected_hash {
            return Err(
                AppError::new("source-conflict", "NFO changed since inspection").at(&item.path),
            );
        }
        let plan = crate::tags::Plan {
            action,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        };
        let candidate = crate::tags::candidate(&raw, &plan)?;
        self.save_write_preview(
            id,
            fingerprint,
            &item,
            root,
            (&raw, &candidate),
            WriteIntent::Tags { plan },
        )
    }
    fn save_write_preview(
        &self,
        id: &str,
        fingerprint: &str,
        item: &MediaItem,
        root: &LibraryRoot,
        bytes: (&[u8], &[u8]),
        intent: WriteIntent,
    ) -> Result<crate::writing::WritePreview> {
        let (original, candidate) = bytes;
        match &intent {
            WriteIntent::Specs => crate::specs::validate_specs_only(original, candidate)?,
            WriteIntent::Tags { plan } => {
                if crate::tags::candidate(original, plan)? != candidate {
                    return Err(AppError::new(
                        "unsafe-candidate",
                        "Tag candidate does not match the plan",
                    ));
                }
            }
            // The sole caller obtains and verifies this backup through Writer::original_bytes.
            // Writer checks its identity and exact bytes again before replacement.
            WriteIntent::Undo { .. } => {}
        }
        let before = library::parse(root, Path::new(&item.path), original)?;
        let after = library::parse(root, Path::new(&item.path), candidate)?;
        let value = crate::writing::WritePreview {
            undo_of: match &intent {
                WriteIntent::Undo { original_id } => Some(original_id.clone()),
                _ => None,
            },
            intent,
            operation_id: id.into(),
            item_id: item.id.clone(),
            path: item.path.clone(),
            title: before.title,
            year: before.year,
            imdb: before.imdb,
            media_kind: before.kind,
            before_hash: hash(original),
            after_hash: hash(candidate),
            before_specs: before.specs,
            after_specs: after.specs,
            phase: if original == candidate {
                "unchanged"
            } else {
                "preview"
            }
            .into(),
            error: None,
            before_xml: technical_xml(original)?,
            after_xml: technical_xml(candidate)?,
            before_tags: before.tags,
            after_tags: after.tags,
        };
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
        if let Some((old, result)) = tx
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
                    "ID belongs to different input",
                ));
            }
            return match serde_json::from_str(&result)? {
                OperationResult::Write(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                )),
            };
        }
        tx.execute(
            "INSERT INTO write_candidates(id,root_id,body) VALUES(?1,?2,?3)",
            params![id, root.id, candidate],
        )?;
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3)",
            params![
                id,
                fingerprint,
                serde_json::to_string(&OperationResult::Write(value.clone()))?
            ],
        )?;
        tx.commit()?;
        Ok(value)
    }
    pub fn apply_specs(
        &self,
        id: &str,
        reviewed_hash: &str,
        journal: &Path,
        cancelled: impl Fn() -> bool,
    ) -> Result<crate::writing::WritePreview> {
        let db = self.db()?;
        self.writable()?;
        let body: String =
            db.query_row("SELECT result FROM operations WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        let mut preview = match serde_json::from_str(&body)? {
            OperationResult::Write(value) => value,
            _ => {
                return Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                ))
            }
        };
        if preview.after_hash != reviewed_hash {
            return Err(AppError::new(
                "review-mismatch",
                "Candidate differs from reviewed change",
            ));
        }
        if preview.phase == "committed" || preview.phase == "unchanged" {
            return Ok(preview);
        }
        if db.query_row("SELECT EXISTS(SELECT 1 FROM tasks WHERE json_extract(body,'$.state') IN ('requested','running','paused','interrupted') AND json_extract(body,'$.batch') IS NULL)",[],|r|r.get::<_,bool>(0))? {return Err(AppError::new("active-task","Finish or cancel scans before editing NFO"));}
        let configuration: Configuration = serde_json::from_str(&db.query_row(
            "SELECT body FROM configuration WHERE id=1",
            [],
            |r| r.get::<_, String>(0),
        )?)?;
        let (root_id, candidate): (String, Vec<u8>) = db.query_row(
            "SELECT root_id,body FROM write_candidates WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if hash(&candidate) != preview.after_hash {
            return Err(AppError::new(
                "candidate-corrupt",
                "Stored candidate no longer matches the reviewed hash",
            ));
        }
        let root = configuration
            .roots
            .iter()
            .find(|r| r.id == root_id)
            .ok_or_else(|| {
                AppError::new("invalid-root", "Reviewed root is no longer configured")
            })?;
        paths::within(Path::new(&root.path), Path::new(&preview.path))?;
        let writer =
            crate::transaction::Writer::new(journal, vec![std::path::PathBuf::from(&root.path)])?;
        preview.phase = "writing".into();
        preview.error = None;
        db.execute(
            "UPDATE operations SET result=?2 WHERE id=?1",
            params![
                id,
                serde_json::to_string(&OperationResult::Write(preview.clone()))?
            ],
        )?;
        let result = writer.commit_with_intent(
            id,
            Path::new(&preview.path),
            &preview.before_hash,
            &candidate,
            &preview.intent,
            |phase| {
                if matches!(phase, crate::transaction::Phase::BeforeReplace) && cancelled() {
                    Err(AppError::new(
                        "cancelled",
                        "Write cancelled before replacement",
                    ))
                } else {
                    Ok(())
                }
            },
        );
        if let Err(mut error) = result {
            error.path = Some(preview.path.clone());
            error.operation_id = Some(id.into());
            preview.phase = writer
                .inspect(id)
                .map(|r| {
                    if r.state == "committed" {
                        "committed-index-pending".into()
                    } else {
                        r.state
                    }
                })
                .unwrap_or_else(|_| "failed".into());
            preview.error = Some(error.clone());
            db.execute(
                "UPDATE operations SET result=?2 WHERE id=?1",
                params![id, serde_json::to_string(&OperationResult::Write(preview))?],
            )?;
            return Err(error);
        }
        let mirror = journal
            .parent()
            .ok_or_else(|| AppError::new("invalid-journal", "Journal has no data directory"))?
            .join("ownership");
        if let Err(mut error) =
            crate::tags::mirror(&mirror, Path::new(&preview.path), &preview.after_hash)
        {
            error.path = Some(preview.path.clone());
            error.operation_id = Some(id.into());
            preview.phase = "committed-mirror-pending".into();
            preview.error = Some(error.clone());
            db.execute(
                "UPDATE operations SET result=?2 WHERE id=?1",
                params![id, serde_json::to_string(&OperationResult::Write(preview))?],
            )?;
            return Err(error);
        }
        preview.phase = "committed".into();
        let item = match library::read(root, Path::new(&preview.path)) {
            Ok(item) => item,
            Err(error) => {
                preview.error = Some(error.clone());
                let mut item = library::empty(root, Path::new(&preview.path));
                item.error = Some(error);
                item
            }
        };
        // This mutex also excludes configuration changes and scans until the
        // committed NFO and its index state have both been recorded.
        db.execute(
            "UPDATE items SET body=?2 WHERE id=?1",
            params![preview.item_id, serde_json::to_string(&item)?],
        )?;
        db.execute(
            "UPDATE operations SET result=?2 WHERE id=?1",
            params![
                id,
                serde_json::to_string(&OperationResult::Write(preview.clone()))?
            ],
        )?;
        Ok(preview)
    }
    pub fn preview_undo(
        &self,
        id: &str,
        original_id: &str,
        journal: &Path,
    ) -> Result<crate::writing::WritePreview> {
        valid_id(id)?;
        let fingerprint = hash(&serde_json::to_vec(&("undo-specs", original_id))?);
        if let Some(result) = self.operation(id, &fingerprint)? {
            return match serde_json::from_str(&result)? {
                OperationResult::Write(value) => Ok(value),
                _ => Err(AppError::new(
                    "operation-conflict",
                    "ID belongs to another action",
                )),
            };
        }
        let original = match self.operation_result(original_id)? {
            OperationResult::Write(value) => value,
            _ => {
                return Err(AppError::new(
                    "unsafe-undo",
                    "Original operation is not an NFO write",
                ))
            }
        };
        let item = self.item(&original.item_id)?;
        let configuration = self.configuration()?;
        let root = configuration
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Root is no longer configured"))?;
        let writer =
            crate::transaction::Writer::new(journal, vec![std::path::PathBuf::from(&root.path)])?;
        let backup = writer.original_bytes(original_id)?;
        let (_, raw) = library::read_bytes(root, Path::new(&original.path))?;
        if hash(&raw) != original.after_hash {
            return Err(AppError::new(
                "source-conflict",
                "NFO changed after this write; undo will not overwrite later edits",
            )
            .at(&original.path));
        }
        self.save_write_preview(
            id,
            &fingerprint,
            &item,
            root,
            (&raw, &backup),
            WriteIntent::Undo {
                original_id: original_id.into(),
            },
        )
    }
    pub fn write_history(&self) -> Result<Vec<crate::writing::WritePreview>> {
        let db = self.db()?;
        let mut query=db.prepare("SELECT result FROM operations WHERE json_extract(result,'$.kind')='write' ORDER BY rowid DESC LIMIT 200")?;
        let mut out = Vec::new();
        for row in query.query_map([], |r| r.get::<_, String>(0))? {
            if let OperationResult::Write(value) = serde_json::from_str(&row?)? {
                out.push(value);
            }
        }
        Ok(out)
    }
}
