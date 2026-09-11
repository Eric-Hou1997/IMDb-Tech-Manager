use crate::desktop::Desktop;
use product_core::{batch::*, writing::WritePreview, *};
use tauri::{Emitter, Manager};
#[tauri::command]
pub async fn plan_batch(request: BatchRequest, app: tauri::AppHandle) -> Result<Task> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Desktop>().store.plan_batch(request))
        .await
        .map_err(|e| AppError::new("batch-worker", e))?
}
#[tauri::command]
pub fn approve_batch_scope(
    id: String,
    reviewed_hash: String,
    state: tauri::State<'_, Desktop>,
) -> Result<Task> {
    state.store.approve_batch_scope(&id, &reviewed_hash)
}
fn skip(phase: &str) -> Outcome {
    Outcome {
        phase: phase.into(),
        error: None,
        candidate_hash: None,
    }
}
pub fn execute(app: &tauri::AppHandle, task: &Task, row: &BatchItem) -> Result<Outcome> {
    let desktop = app.state::<Desktop>();
    let batch = task
        .batch
        .as_ref()
        .ok_or_else(|| AppError::new("invalid-task", "Missing batch plan"))?;
    // Recover a previously saved candidate/receipt before consulting the changed
    // NFO. The original write operation may already have committed before exit.
    let existing = match desktop.store.operation_result(&row.write_id) {
        Ok(OperationResult::Write(p)) => Some(p),
        Err(e) if e.code == "operation-not-found" => None,
        Err(e) => return Err(e),
        _ => {
            return Err(AppError::new(
                "operation-conflict",
                "Batch write ID has another owner",
            ))
        }
    };
    let mut review_required = false;
    let candidate = if let Some(p) = existing {
        p
    } else {
        if let Some(reason) = product_core::batch::skip_reason(task, row)? {
            return Ok(skip(&reason));
        }
        match batch.engine {
            BatchEngine::Rules => {
                desktop
                    .store
                    .preview_rules(&row.write_id, &row.item.id, &row.item.source_hash)?
            }
            BatchEngine::Ai => {
                let result = tauri::async_runtime::block_on(crate::ai_jobs::run_ai(
                    ai::job::Request {
                        operation_id: row.request_id.clone(),
                        item_id: row.item.id.clone(),
                        expected_hash: row.item.source_hash.clone(),
                        force: batch.mode == BatchMode::Rebuild,
                        retry_failed: batch.retry_failed,
                    },
                    Some(task.id.clone()),
                    app.clone(),
                ))?;
                if result.phase != "review-ready" {
                    return Ok(Outcome {
                        phase: if [
                            "skipped-unchanged-failure",
                            "skipped-legacy-failure",
                            "skipped-legacy-unverified",
                        ]
                        .contains(&result.phase.as_str())
                        {
                            result.phase
                        } else {
                            "failed".into()
                        },
                        error: result.error,
                        candidate_hash: None,
                    });
                }
                review_required = ai::job::needs_review(&result);
                desktop.store.preview_ai(&row.write_id, &row.request_id)?
            }
            BatchEngine::Specs => {
                let result = tauri::async_runtime::block_on(crate::acquisition::run_fetch(
                    acquisition::FetchRequest {
                        operation_id: row.request_id.clone(),
                        item_id: row.item.id.clone(),
                        expected_hash: row.item.source_hash.clone(),
                        refresh: batch.mode == BatchMode::Rebuild,
                    },
                    Some(task.id.clone()),
                    app.clone(),
                ))?;
                if result.phase != "completed" {
                    return Err(result.error.unwrap_or_else(|| {
                        AppError::new(
                            "interrupted",
                            "IMDb request did not complete; explicit retry is required",
                        )
                    }));
                }
                desktop
                    .store
                    .preview_source(&row.write_id, &row.request_id)?
            }
        }
    };
    if batch.engine == BatchEngine::Ai {
        review_required = ai::job::needs_review(&desktop.store.ai_record(&row.request_id)?);
    }
    if candidate.phase == "committed" || candidate.phase == "unchanged" {
        return Ok(Outcome {
            phase: candidate.phase,
            error: None,
            candidate_hash: Some(candidate.after_hash),
        });
    }
    if batch.mode == BatchMode::Preview || review_required {
        return Ok(Outcome {
            phase: "review-ready".into(),
            error: None,
            candidate_hash: Some(candidate.after_hash),
        });
    }
    let _owner = desktop
        .write_gate
        .lock()
        .map_err(|e| AppError::new("write-owner", e))?;
    if desktop.stopping() {
        return Err(AppError::new(
            "interrupted",
            "Application stopped before the candidate was applied",
        ));
    }
    let journal = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::new("data-directory", e))?
        .join("nfo-transactions");
    let receipt: WritePreview =
        desktop
            .store
            .apply_specs(&row.write_id, &candidate.after_hash, &journal, || {
                desktop.stopping()
            })?;
    let _ = app.emit("nfo-written", &receipt);
    Ok(Outcome {
        phase: receipt.phase,
        error: receipt.error,
        candidate_hash: Some(receipt.after_hash),
    })
}

#[tauri::command]
pub async fn apply_batch_item(
    task_id: String,
    write_id: String,
    reviewed_hash: String,
    app: tauri::AppHandle,
) -> Result<WritePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        let desktop = app.state::<Desktop>();
        let _owner = desktop
            .write_gate
            .lock()
            .map_err(|e| AppError::new("write-owner", e))?;
        if desktop.stopping() {
            return Err(AppError::new("shutting-down", "Application is stopping"));
        }
        let journal = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::new("data-directory", e))?
            .join("nfo-transactions");
        desktop
            .store
            .apply_batch_item(&task_id, &write_id, &reviewed_hash, &journal, || {
                desktop.stopping()
            })
    })
    .await
    .map_err(|e| AppError::new("batch-worker", e))?
}

#[tauri::command]
pub async fn batch_detail(id: String, app: tauri::AppHandle) -> Result<Task> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Desktop>().store.task(&id))
        .await
        .map_err(|e| AppError::new("batch-worker", e))?
}
