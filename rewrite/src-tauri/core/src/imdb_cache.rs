use crate::{
    specs::{SourceSpecs, SourceStatus},
    *,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub fn fresh(at: &str, now: i64, seconds: i64) -> bool {
    chrono::DateTime::parse_from_rfc3339(at)
        .is_ok_and(|t| now.saturating_sub(t.timestamp()) < seconds)
}
pub fn source_fresh(source: &SourceSpecs, now: i64) -> bool {
    source.validate().is_ok()
        && fresh(
            &source.fetched_at,
            now,
            if source.status == SourceStatus::Empty {
                7 * 86400
            } else {
                30 * 86400
            },
        )
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub imdb: String,
    pub fetched_at: String,
    pub error: AppError,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CacheMigrationItem {
    pub source: String,
    pub imdb: String,
    pub state: String,
    pub before_hash: Option<String>,
    pub detail: String,
}
#[derive(Debug, Clone)]
pub enum LegacyCache {
    Source(SourceSpecs),
    Failure(Failure),
}
pub fn legacy_id(relative: &str) -> Option<String> {
    let name = relative
        .rsplit('/')
        .next()?
        .strip_suffix(".json")?
        .to_ascii_lowercase();
    crate::specs::imdb_url(&name).ok().map(|_| name)
}
pub fn parse_legacy(imdb: &str, raw: &[u8]) -> Result<LegacyCache> {
    let value: serde_json::Value = serde_json::from_slice(raw)?;
    if value["cache_version"] != 8 || value.get("parser_version").is_some_and(|p| p != 1) {
        return Err(AppError::new(
            "legacy-cache-version",
            "Historical cache/parser version cannot be reused",
        ));
    }
    if value["imdb"]
        .as_str()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
        != Some(imdb)
    {
        return Err(AppError::new(
            "legacy-cache-identity",
            "Cache filename and payload IMDb differ",
        ));
    }
    let at = value["fetched_at"]
        .as_str()
        .ok_or_else(|| AppError::new("legacy-cache-time", "Cache has no fetch time"))?;
    chrono::DateTime::parse_from_rfc3339(at).map_err(|e| AppError::new("legacy-cache-time", e))?;
    let status = value["status"].as_str().unwrap_or(if value["ok"] == true {
        "ok"
    } else {
        "fetch-error"
    });
    if value["ok"] == true || status == "no-tech" {
        let empty = status == "no-tech" || status == "empty";
        if empty
            && value.get("specs").is_some_and(|v| {
                v.as_object().is_some_and(|o| {
                    o.values().any(|values| {
                        values.as_array().is_some_and(|a| {
                            a.iter()
                                .any(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
                        })
                    })
                })
            })
        {
            return Err(AppError::new(
                "legacy-cache-status",
                "Empty cache contradicts its specifications",
            ));
        }
        let source = SourceSpecs {
            imdb: imdb.into(),
            fetched_at: at.into(),
            parser: value["parser"]
                .as_str()
                .unwrap_or("legacy-parser-v1")
                .into(),
            status: if empty {
                SourceStatus::Empty
            } else {
                SourceStatus::Ok
            },
            specs: if empty {
                Specs::new()
            } else {
                serde_json::from_value(value["specs"].clone())?
            },
        };
        source.validate()?;
        return Ok(LegacyCache::Source(source));
    }
    let code = match status {
        "imdb-waf-challenge" => "imdb-waf-challenge",
        "http-403" => "imdb-http-forbidden",
        "http-429" => "imdb-rate-limit",
        "network-error" => "imdb-connection-failed",
        "timeout" => "imdb-timeout",
        "invalid-response" => "imdb-response-invalid",
        "parse-error" => "imdb-parse-error",
        "partial-or-suspicious-response" => "imdb-response-suspicious",
        "fetch-error" => "imdb-network",
        _ => {
            return Err(AppError::new(
                "legacy-cache-status",
                "Unknown historical failure state",
            ))
        }
    };
    let mut error = AppError::new(
        code,
        format!(
            "Historical IMDb request failed ({status}); refresh explicitly or wait for cooldown"
        ),
    );
    error.retryable = true;
    Ok(LegacyCache::Failure(Failure {
        imdb: imdb.into(),
        fetched_at: at.into(),
        error,
    }))
}
pub fn cacheable(error: &AppError) -> bool {
    [
        "imdb-waf-challenge",
        "imdb-http-forbidden",
        "imdb-rate-limit",
        "imdb-timeout",
        "imdb-connection-failed",
        "imdb-network",
        "imdb-response-invalid",
        "imdb-parse-error",
        "imdb-response-suspicious",
        "imdb-empty-unconfirmed",
        "imdb-layout-unrecognized",
        "imdb-payload-missing",
        "imdb-webview-timeout",
        "imdb-response-encoding",
        "imdb-json-invalid",
    ]
    .contains(&error.code.as_str())
}
