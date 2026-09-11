use crate::desktop::Desktop;
use product_core::{
    writing::{SpecsEdit, WritePreview},
    AppError, Result,
};
use tauri::{Emitter, Manager};
fn journal(app: &tauri::AppHandle) -> Result<std::path::PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::new("data-directory", e))?
        .join("nfo-transactions"))
}
#[tauri::command]
pub async fn preview_specs(request: SpecsEdit, app: tauri::AppHandle) -> Result<WritePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>().store.preview_specs(request)
    })
    .await
    .map_err(|e| AppError::new("write-worker", e))?
}
#[tauri::command]
pub async fn preview_tags(
    request: product_core::writing::TagEdit,
    app: tauri::AppHandle,
) -> Result<WritePreview> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Desktop>().store.preview_tags(request))
        .await
        .map_err(|e| AppError::new("write-worker", e))?
}
#[tauri::command]
pub async fn preview_rules(
    id: String,
    item_id: String,
    expected_hash: String,
    app: tauri::AppHandle,
) -> Result<WritePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>()
            .store
            .preview_rules(&id, &item_id, &expected_hash)
    })
    .await
    .map_err(|e| AppError::new("write-worker", e))?
}
#[tauri::command]
pub async fn apply_specs(
    id: String,
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
        let receipt = desktop
            .store
            .apply_specs(&id, &reviewed_hash, &journal(&app)?, || desktop.stopping())?;
        if let Err(error) = app.emit("nfo-written", &receipt) {
            eprintln!("nfo-event: {error}");
        }
        Ok(receipt)
    })
    .await
    .map_err(|e| AppError::new("write-worker", e))?
}
#[tauri::command]
pub async fn preview_undo(
    id: String,
    original_id: String,
    app: tauri::AppHandle,
) -> Result<WritePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>()
            .store
            .preview_undo(&id, &original_id, &journal(&app)?)
    })
    .await
    .map_err(|e| AppError::new("write-worker", e))?
}
#[tauri::command]
pub fn write_history(state: tauri::State<'_, Desktop>) -> Result<Vec<WritePreview>> {
    state.store.write_history()
}
#[tauri::command]
pub async fn legacy_undo_entries(
    item_id: String,
    offset: u32,
    app: tauri::AppHandle,
) -> Result<product_core::legacy_undo::LegacyUndoPage> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>()
            .store
            .legacy_undo_entries(&item_id, offset)
    })
    .await
    .map_err(|e| AppError::new("legacy-undo-worker", e))?
}
#[tauri::command]
pub async fn preview_legacy_undo(
    request: product_core::legacy_undo::LegacyUndoRequest,
    app: tauri::AppHandle,
) -> Result<WritePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>().store.preview_legacy_undo(request)
    })
    .await
    .map_err(|e| AppError::new("legacy-undo-worker", e))?
}
