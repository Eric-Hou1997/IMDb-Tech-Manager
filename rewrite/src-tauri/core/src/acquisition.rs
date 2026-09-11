use crate::{specs::SourceSpecs, AppError, Locale, Result};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FetchRequest {
    pub operation_id: String,
    pub item_id: String,
    pub expected_hash: String,
    pub refresh: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FetchAttempt {
    pub transport: String,
    pub finished_at: String,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FetchRecord {
    pub request: FetchRequest,
    pub imdb: String,
    pub path: String,
    pub locale: Locale,
    pub phase: String,
    pub cached: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub source: Option<SourceSpecs>,
    pub error: Option<AppError>,
    #[serde(default)]
    pub attempts: Vec<FetchAttempt>,
}
pub fn http_status(status: u16) -> Result<()> {
    let (code, retryable) = match status {
        200 => return Ok(()),
        202 => ("imdb-waf-challenge", true),
        401 => ("imdb-authentication", false),
        403 => ("imdb-http-forbidden", false),
        404 => ("imdb-title-not-found", false),
        429 => ("imdb-rate-limit", true),
        500..=599 => ("imdb-server-error", true),
        300..=399 => ("imdb-redirect-rejected", false),
        _ => ("imdb-http-error", false),
    };
    let mut error = AppError::new(code, format!("IMDb returned HTTP {status}"));
    error.retryable = retryable;
    Err(error)
}
