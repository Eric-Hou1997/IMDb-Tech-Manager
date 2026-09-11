use crate::{credentials::AiCredentials, desktop::Desktop};
use product_core::{
    ai::{
        self,
        job::{Record, Request, Settings},
        HttpResponse,
    },
    services::CredentialStore,
    writing::WritePreview,
    AppError, Result,
};
use std::time::Duration;
use tauri::{Emitter, Manager};
fn credentials(app: &tauri::AppHandle) -> AiCredentials {
    AiCredentials::new(app.config().identifier.clone())
}
#[tauri::command]
pub async fn ai_settings(app: tauri::AppHandle) -> Result<(Settings, bool)> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = app.state::<Desktop>().store.ai_settings()?;
        let configured = !settings.credential_account.is_empty()
            && credentials(&app)
                .get(&settings.credential_account)?
                .is_some();
        Ok((settings, configured))
    })
    .await
    .map_err(|e| AppError::new("ai-worker", e))?
}
#[tauri::command]
pub async fn save_ai_settings(
    id: String,
    settings: Settings,
    secret: Option<String>,
    app: tauri::AppHandle,
) -> Result<Settings> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>().store.save_ai_profile(
            &id,
            settings,
            secret.as_deref(),
            &credentials(&app),
        )
    })
    .await
    .map_err(|e| AppError::new("ai-worker", e))?
}
fn network(error: reqwest::Error) -> AppError {
    AppError::new(
        "transient",
        if error.is_timeout() {
            "Provider request timed out"
        } else {
            "Provider network request failed"
        },
    )
}
fn client(settings: &Settings) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(settings.timeout_seconds.into()))
        .connect_timeout(Duration::from_secs(15))
        .user_agent("IMDb-Tech-Manager/4.1.0")
        .build()
        .map_err(network)
}
async fn transport(
    client: &reqwest::Client,
    settings: &Settings,
    secret: &str,
    body: &serde_json::Value,
) -> Result<(HttpResponse, f64)> {
    let mut request = client
        .post(ai::job::endpoint(&settings.config)?)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(body)?);
    request = match settings.config.protocol {
        ai::Protocol::Openai => request.bearer_auth(secret),
        ai::Protocol::Anthropic => request
            .header("x-api-key", secret)
            .header("anthropic-version", "2023-06-01"),
    };
    let mut response = request.send().await.map_err(network)?;
    let status = response.status().as_u16();
    let retry = response
        .headers()
        .get("Retry-After")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err(AppError::new(
                "provider-response-too-large",
                "Provider response exceeds 8 MiB",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((
        HttpResponse {
            status,
            body: bytes,
        },
        retry,
    ))
}
fn emit(app: &tauri::AppHandle, value: &Record) {
    if let Err(e) = app.emit("ai-changed", value) {
        eprintln!("ai-event: {e}");
    }
}
#[tauri::command]
pub async fn generate_ai(request: Request, app: tauri::AppHandle) -> Result<Record> {
    run_ai(request, None, app).await
}
pub(crate) async fn run_ai(
    request: Request,
    batch_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<Record> {
    tauri::async_runtime::spawn_blocking(move || {
        let desktop = app.state::<Desktop>();
        if desktop.stopping() {
            return Err(AppError::new("shutting-down", "Application is stopping"));
        }
        let (mut value, execute) = match batch_id {
            Some(id) => desktop.store.begin_batch_ai(request, &id)?,
            None => desktop.store.begin_ai(request)?,
        };
        if !execute {
            return Ok(value);
        }
        let id = value.request.operation_id.clone();
        emit(&app, &value);
        let _owner = desktop
            .write_gate
            .lock()
            .map_err(|e| AppError::new("ai-owner", e))?;
        if desktop.stopping() {
            return desktop
                .store
                .end_ai(&id, AppError::new("cancelled", "Application is stopping"));
        }
        value = desktop.store.ai_record(&id)?;
        if value.phase != "requested" {
            return Ok(value);
        }
        let secret = match credentials(&app).get(&value.settings.credential_account) {
            Ok(Some(secret)) => secret,
            Ok(None) => {
                return desktop.store.end_ai(
                    &id,
                    AppError::new(
                        "credential-missing",
                        "Configure the provider credential first",
                    ),
                )
            }
            Err(error) => return desktop.store.end_ai(&id, error),
        };
        let client = match client(&value.settings) {
            Ok(client) => client,
            Err(error) => return desktop.store.end_ai(&id, error),
        };
        loop {
            let next = match ai::job::next(&value) {
                Ok(next) => next,
                Err(error) => return desktop.store.end_ai(&id, error),
            };
            let Some((body, delay)) = next else {
                return Ok(value);
            };
            let deadline = std::time::Instant::now() + Duration::from_secs_f64(delay);
            loop {
                if desktop.stopping() || desktop.store.ai_record(&id)?.phase == "cancelled" {
                    return desktop
                        .store
                        .end_ai(&id, AppError::new("cancelled", "AI request cancelled"));
                }
                if std::time::Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            value = match desktop.store.reserve_ai_attempt(&id, body.clone()) {
                Ok(value) => value,
                Err(error) => return desktop.store.end_ai(&id, error),
            };
            emit(&app, &value);
            let outcome = tauri::async_runtime::block_on(async {
                let mut pending = Box::pin(transport(&client, &value.settings, &secret, &body));
                loop {
                    if desktop.stopping() || desktop.store.ai_record(&id)?.phase == "cancelled" {
                        return Err(AppError::new(
                            "cancelled",
                            "AI request cancelled; provider usage may be unavailable",
                        ));
                    }
                    match tokio::time::timeout(Duration::from_millis(100), &mut pending).await {
                        Ok(result) => return result,
                        Err(_) => continue,
                    }
                }
            });
            let (response, retry) = match outcome {
                Ok((response, retry)) => (Ok(response), retry),
                Err(error) => (Err(error), 0.0),
            };
            value = desktop.store.observe_ai_attempt(&id, response, retry)?;
            emit(&app, &value);
        }
    })
    .await
    .map_err(|e| AppError::new("ai-worker", e))?
}
#[tauri::command]
pub fn ai_record(id: String, state: tauri::State<'_, Desktop>) -> Result<Record> {
    state.store.ai_record(&id)
}
#[tauri::command]
pub fn cancel_ai(id: String, state: tauri::State<'_, Desktop>) -> Result<Record> {
    state
        .store
        .end_ai(&id, AppError::new("cancelled", "AI request cancelled"))
}
#[tauri::command]
pub fn ai_history(
    item_id: Option<String>,
    state: tauri::State<'_, Desktop>,
) -> Result<Vec<Record>> {
    state.store.ai_history(item_id.as_deref())
}
#[tauri::command]
pub async fn preview_ai(id: String, ai_id: String, app: tauri::AppHandle) -> Result<WritePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>().store.preview_ai(&id, &ai_id)
    })
    .await
    .map_err(|e| AppError::new("ai-worker", e))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    #[test]
    fn real_http_transport_sends_protocol_headers_and_does_not_follow_redirects() {
        for protocol in [ai::Protocol::Openai, ai::Protocol::Anthropic] {
            let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = server.local_addr().unwrap();
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = server.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = vec![];
                let mut chunk = [0_u8; 4096];
                loop {
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..pos]);
                        let length = headers
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|v| v.parse::<usize>().ok())
                            })
                            .unwrap();
                        if bytes.len() >= pos + 4 + length {
                            break;
                        }
                    }
                }
                stream.write_all(b"HTTP/1.1 302 Found\r\nContent-Length: 2\r\nConnection: close\r\nLocation: http://127.0.0.1:1/credential-trap\r\nRetry-After: 2\r\n\r\n{}").unwrap();
                String::from_utf8(bytes).unwrap()
            });
            let mut settings = Settings::default();
            settings.config.base_url = format!("http://{address}/v1");
            settings.config.protocol = protocol.clone();
            settings.timeout_seconds = 5;
            let body = serde_json::json!({"model":"contract","messages":[]});
            let (response, retry) = tauri::async_runtime::block_on(transport(
                &client(&settings).unwrap(),
                &settings,
                "non-secret-test-sentinel",
                &body,
            ))
            .unwrap();
            assert_eq!(response.status, 302);
            assert_eq!(retry, 2.0);
            let received = worker.join().unwrap();
            let lower = received.to_ascii_lowercase();
            match protocol {
                ai::Protocol::Openai => {
                    assert!(received.starts_with("POST /v1/chat/completions HTTP/1.1"));
                    assert!(lower.contains("authorization: bearer non-secret-test-sentinel"));
                    assert!(!lower.contains("x-api-key:"));
                }
                ai::Protocol::Anthropic => {
                    assert!(received.starts_with("POST /v1/messages HTTP/1.1"));
                    assert!(lower.contains("x-api-key: non-secret-test-sentinel"));
                    assert!(lower.contains("anthropic-version: 2023-06-01"));
                    assert!(!lower.contains("authorization:"));
                }
            }
            assert!(received.ends_with(&serde_json::to_string(&body).unwrap()));
        }
    }
}
