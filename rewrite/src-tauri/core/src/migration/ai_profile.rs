use crate::{
    ai::{self, job::Settings, Protocol},
    AppError, Result,
};
use serde_json::Value;
pub const LEGACY_ACCOUNT: &str = "legacy-itm-macos-api-key";
/// The v4.1 Go config is flat under `ai`; runtime settings separate provider
/// request parameters from retry/accounting policy. Preserve custom prompt bytes.
pub fn adapt(root: &Value) -> Result<Option<Settings>> {
    let Some(old) = root.get("ai") else {
        return Ok(None);
    };
    let old = old
        .as_object()
        .ok_or_else(|| AppError::new("legacy-ai-config", "Expected an AI configuration object"))?;
    let mut value = serde_json::to_value(Settings::default())?;
    for key in [
        "enabled",
        "timeout_seconds",
        "retry_count",
        "output_token_cap",
        "input_price_per_million",
        "output_price_per_million",
        "run_request_limit",
        "run_token_limit",
        "run_cost_limit",
        "json_mode",
        "fallback_mode",
        "legacy_cleanup_mode",
        "warning_policy",
    ] {
        if let Some(v) = old.get(key) {
            value[key] = v.clone();
        }
    }
    for key in [
        "provider",
        "base_url",
        "model",
        "prompt",
        "temperature",
        "top_p",
        "max_tokens",
        "thinking_mode",
        "prompt_cache_mode",
    ] {
        if let Some(v) = old.get(key) {
            value["config"][key] = v.clone();
        }
    }
    let provider = value["config"]["provider"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let url = value["config"]["base_url"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let explicit = old
        .get("api_protocol")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let protocol = match explicit.as_str() {
        "openai" => Protocol::Openai,
        "anthropic" => Protocol::Anthropic,
        _ if provider.contains("anthropic")
            || url.contains("/apps/anthropic")
            || url.trim_end_matches('/').ends_with("/v1/messages") =>
        {
            Protocol::Anthropic
        }
        _ if [
            "",
            "openai",
            "openai-compatible",
            "bailian",
            "dashscope",
            "aliyun",
            "qwen",
        ]
        .contains(&provider.as_str())
            || url.contains("/compatible-mode/v1")
            || url.trim_end_matches('/').ends_with("/chat/completions") =>
        {
            Protocol::Openai
        }
        _ => {
            return Err(AppError::new(
                "legacy-ai-protocol-unresolved",
                "Historical provider needs an explicit protocol selection",
            ))
        }
    };
    value["config"]["protocol"] = serde_json::to_value(protocol)?;
    if let Some(extra) = old.get("extra_body") {
        value["config"]["extra_body"] = if let Some(text) = extra.as_str() {
            if text.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(text).map_err(|e| AppError::new("legacy-ai-extra-body", e))?
            }
        } else {
            extra.clone()
        };
    }
    // Match the old loader's documented defaults and corrections.
    if value["config"]["prompt"]
        .as_str()
        .is_some_and(|s| s.trim().is_empty())
    {
        value["config"]["prompt"] = ai::DEFAULT_PROMPT.into();
    }
    if provider.is_empty() {
        value["config"]["provider"] = "openai-compatible".into();
    }
    if value["config"]["top_p"]
        .as_f64()
        .is_some_and(|v| v <= 0.0 || v > 1.0)
    {
        value["config"]["top_p"] = 1.0.into();
    }
    if value["config"]["max_tokens"] == 1800 && value["output_token_cap"] == 8192 {
        value["config"]["max_tokens"] = 2000.into();
        value["output_token_cap"] = 10000.into();
    }
    for (key, min, default) in [("max_tokens", 128, 2000)] {
        if value["config"][key].as_i64().is_some_and(|v| v < min) {
            value["config"][key] = default.into();
        }
    }
    for (key, min, default) in [
        ("output_token_cap", 4096, 10000),
        ("timeout_seconds", 10, 90),
    ] {
        if value[key].as_i64().is_some_and(|v| v < min) {
            value[key] = default.into();
        }
    }
    for (key, default) in [
        ("json_mode", "auto"),
        ("fallback_mode", "abort"),
        ("legacy_cleanup_mode", "strict"),
        ("warning_policy", "review"),
    ] {
        if value[key].as_str() == Some("") {
            value[key] = default.into();
        }
    }
    for (key, default) in [("thinking_mode", "off"), ("prompt_cache_mode", "auto")] {
        if value["config"][key].as_str() == Some("") {
            value["config"][key] = default.into();
        }
    }
    if value["retry_count"]
        .as_i64()
        .is_some_and(|v| !(0..=5).contains(&v))
    {
        value["retry_count"] = 2.into();
    }
    for key in [
        "input_price_per_million",
        "output_price_per_million",
        "run_request_limit",
        "run_token_limit",
        "run_cost_limit",
    ] {
        if value[key].as_f64().is_some_and(|v| v < 0.0) {
            value[key] = 0.into();
        }
    }
    value["credential_account"] = LEGACY_ACCOUNT.into();
    let settings: Settings =
        serde_json::from_value(value).map_err(|e| AppError::new("legacy-ai-config", e))?;
    settings.validate()?;
    Ok(Some(settings))
}
