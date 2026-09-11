use crate::desktop::Desktop;
use product_core::{
    acquisition::{FetchRecord, FetchRequest},
    specs::SourceSpecs,
    writing::WritePreview,
    AppError, Result,
};
use std::time::Duration;
use tauri::{Emitter, Manager};
fn network(error: reqwest::Error) -> AppError {
    let code = if error.is_timeout() {
        "imdb-timeout"
    } else if error.is_connect() {
        "imdb-connection-failed"
    } else if error.is_decode() {
        "imdb-response-invalid"
    } else {
        "imdb-network"
    };
    let mut value = AppError::new(code, error);
    value.retryable = true;
    value
}
async fn fetch_page(imdb: &str) -> Result<SourceSpecs> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("IMDb-Tech-Manager/4.1.0")
        .build()
        .map_err(network)?;
    let mut response = client
        .get(product_core::specs::imdb_url(imdb)?)
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(network)?;
    product_core::acquisition::http_status(response.status().as_u16())?;
    if response
        .content_length()
        .is_some_and(|size| size > 8 * 1024 * 1024)
    {
        return Err(AppError::new(
            "imdb-response-too-large",
            "IMDb response exceeds 8 MiB",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err(AppError::new(
                "imdb-response-too-large",
                "IMDb response exceeds 8 MiB",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let page =
        std::str::from_utf8(&bytes).map_err(|e| AppError::new("imdb-response-encoding", e))?;
    product_core::specs::parse_page(imdb, page)
}
#[tauri::command]
pub async fn fetch_specs(request: FetchRequest, app: tauri::AppHandle) -> Result<FetchRecord> {
    tauri::async_runtime::spawn_blocking(move || {
        let desktop = app.state::<Desktop>();
        if desktop.stopping() {
            return Err(AppError::new("shutting-down", "Application is stopping"));
        }
        let (record, execute) = desktop.store.begin_fetch(request)?;
        if !execute {
            return Ok(record);
        }
        let id = &record.request.operation_id;
        // Reserve the operation before waiting for the owner, so a queued
        // request remains queryable and cancellable from another Inspector.
        let _owner = desktop
            .write_gate
            .lock()
            .map_err(|e| AppError::new("fetch-owner", e))?;
        if desktop.stopping() {
            return desktop.store.cancel_fetch(id);
        }
        let current = desktop.store.fetch_record(id)?;
        if current.phase != "requested" {
            return Ok(current);
        }
        if let Err(error) = app.emit("fetch-changed", &record) {
            eprintln!("fetch-event: {error}");
        }
        let mut result = tauri::async_runtime::block_on(async {
            let mut work = Box::pin(fetch_page(&record.imdb));
            loop {
                if desktop.stopping() || desktop.store.fetch_record(id)?.phase != "requested" {
                    return Err(AppError::new("cancelled", "IMDb request cancelled"));
                }
                match tokio::time::timeout(Duration::from_millis(100), &mut work).await {
                    Ok(result) => return result,
                    Err(_) => continue,
                }
            }
        });
        desktop
            .store
            .record_fetch_attempt(id, "http", result.as_ref().err().cloned())?;
        if result.as_ref().is_err_and(|error| {
            matches!(
                error.code.as_str(),
                "imdb-waf-challenge"
                    | "imdb-http-forbidden"
                    | "imdb-layout-unrecognized"
                    | "imdb-json-invalid"
                    | "imdb-payload-missing"
                    | "imdb-connection-failed"
                    | "imdb-timeout"
                    | "imdb-network"
            )
        }) {
            if let Err(error) = app.emit("fetch-browser-fallback", &record) {
                eprintln!("fetch-event: {error}");
            }
            result = crate::imdb_webview::fetch(&app, id, &record.imdb);
            desktop.store.record_fetch_attempt(
                id,
                "system-webview",
                result.as_ref().err().cloned(),
            )?;
        }
        let record = desktop.store.finish_fetch(id, result)?;
        if let Err(error) = app.emit("fetch-changed", &record) {
            eprintln!("fetch-event: {error}");
        }
        Ok(record)
    })
    .await
    .map_err(|e| AppError::new("fetch-worker", e))?
}
#[tauri::command]
pub fn fetch_record(id: String, state: tauri::State<'_, Desktop>) -> Result<FetchRecord> {
    state.store.fetch_record(&id)
}
#[tauri::command]
pub fn cancel_fetch(id: String, state: tauri::State<'_, Desktop>) -> Result<FetchRecord> {
    state.store.cancel_fetch(&id)
}
#[tauri::command]
pub async fn preview_source(
    id: String,
    fetch_id: String,
    app: tauri::AppHandle,
) -> Result<WritePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>().store.preview_source(&id, &fetch_id)
    })
    .await
    .map_err(|e| AppError::new("fetch-worker", e))?
}

#[tauri::command]
pub fn fetch_history(
    item_id: String,
    state: tauri::State<'_, Desktop>,
) -> Result<Vec<FetchRecord>> {
    state.store.fetch_history(&item_id)
}
