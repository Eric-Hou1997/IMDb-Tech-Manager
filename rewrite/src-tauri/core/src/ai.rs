//! Provider request and response policy shared by durable jobs and differential tests.
use crate::{hash, specs::TAG_SECTIONS, AppError, Result, Specs};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use ts_rs::TS;
pub mod job;
pub const DEFAULT_PROMPT: &str = include_str!("../assets/default-ai-prompt.txt");
pub const LANGUAGE_BOUNDARY: &str = include_str!("../assets/language-boundary.txt");
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "AiProtocol")]
pub enum Protocol {
    Openai,
    Anthropic,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "AiConfig")]
pub struct Config {
    pub protocol: Protocol,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub prompt: String,
    pub output_language: String,
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u32,
    pub thinking_mode: String,
    pub prompt_cache_mode: String,
    pub json_mode: bool,
    pub extra_body: BTreeMap<String, Value>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            protocol: Protocol::Openai,
            provider: "openai-compatible".into(),
            base_url: String::new(),
            model: String::new(),
            prompt: DEFAULT_PROMPT.into(),
            output_language: "zh-CN".into(),
            temperature: 0.0,
            top_p: 1.0,
            max_tokens: 2000,
            thinking_mode: "off".into(),
            prompt_cache_mode: "auto".into(),
            json_mode: true,
            extra_body: BTreeMap::new(),
        }
    }
}
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn qwen(cfg: &Config) -> bool {
    let model = cfg.model.to_lowercase();
    let provider = cfg.provider.to_lowercase();
    let base = cfg.base_url.to_lowercase();
    (model.starts_with("qwen") || provider.contains("qwen"))
        && (base.contains("dashscope")
            || base.contains("aliyuncs.com")
            || ["bailian", "dashscope", "aliyun", "qwen"].contains(&provider.as_str()))
}
pub fn request(cfg: &Config, specs: &Specs, existing: &[Value]) -> Result<Value> {
    if cfg.max_tokens == 0
        || cfg.model.trim().is_empty()
        || !cfg.temperature.is_finite()
        || !cfg.top_p.is_finite()
        || !["off", "on", "auto"].contains(&cfg.thinking_mode.as_str())
        || !["off", "on", "auto"].contains(&cfg.prompt_cache_mode.as_str())
    {
        return Err(AppError::new(
            "invalid-ai-config",
            "Invalid model parameters",
        ));
    }
    let input: Specs = TAG_SECTIONS
        .into_iter()
        .filter_map(|key| {
            specs
                .get(key)
                .filter(|v| !v.is_empty())
                .map(|v| (key.into(), v.clone()))
        })
        .collect();
    let mut payload = json!({"technical_specs":input,"output_language":cfg.output_language});
    if !existing.is_empty() {
        payload["existing_tags"] = json!(existing);
    }
    let prompt = format!("{}\n\n{}", cfg.prompt.trim(), LANGUAGE_BOUNDARY);
    let cache = cfg.prompt_cache_mode == "on" || (cfg.prompt_cache_mode == "auto" && qwen(cfg));
    let system = if cache {
        json!([{"type":"text","text":prompt,"cache_control":{"type":"ephemeral"}}])
    } else {
        json!(prompt)
    };
    let mut body = json!({"model":cfg.model,"temperature":cfg.temperature,"top_p":cfg.top_p,"max_tokens":cfg.max_tokens});
    match cfg.protocol {
        Protocol::Openai => {
            body["messages"] = json!([{"role":"system","content":system},{"role":"user","content":serde_json::to_string(&payload)?}]);
            if cfg.json_mode {
                body["response_format"] = json!({"type":"json_object"});
            }
        }
        Protocol::Anthropic => {
            body["system"] = system;
            body["messages"] = json!([{"role":"user","content":serde_json::to_string(&payload)?}]);
            if cfg.json_mode {
                body["output_config"] =
                    json!({"format":{"type":"json_schema","schema":output_schema()}});
            }
        }
    }
    for (key, value) in &cfg.extra_body {
        if ["model", "messages"].contains(&key.as_str())
            || cfg.protocol == Protocol::Anthropic
                && ["system", "max_tokens"].contains(&key.as_str())
        {
            continue;
        }
        body[key] = value.clone();
    }
    if cfg.thinking_mode != "auto" {
        match cfg.protocol {
            Protocol::Openai if qwen(cfg) => {
                body["enable_thinking"] = json!(cfg.thinking_mode == "on")
            }
            Protocol::Anthropic if qwen(cfg) => {
                body["thinking"] = if cfg.thinking_mode == "off" {
                    json!({"type":"disabled"})
                } else {
                    json!({"type":"enabled","budget_tokens":(cfg.max_tokens/2).clamp(1024,4096)})
                };
            }
            Protocol::Anthropic if cfg.thinking_mode == "on" => {
                body["thinking"] = json!({"type":"adaptive"})
            }
            _ => {}
        }
    }
    Ok(body)
}
fn output_schema() -> Value {
    serde_json::from_str(include_str!("../assets/ai-output-schema.json"))
        .expect("verified embedded schema")
}
pub fn failure_fingerprint(cfg: &Config, specs: &Specs, existing: &[Value]) -> Result<String> {
    Ok(hash(
        serde_json::to_string(
            &json!({"contract":"rust-policy-1","config":cfg,"specs":specs,"existing":existing}),
        )?
        .as_bytes(),
    ))
}
pub fn skip_unchanged(previous: Option<&str>, current: &str, explicit_retry: bool) -> bool {
    !explicit_retry && previous == Some(current)
}
pub fn larger_limit(actual_request_limit: u32, ceiling: u32) -> Option<u32> {
    let next = actual_request_limit.saturating_mul(2).min(ceiling);
    (next > actual_request_limit).then_some(next)
}
pub fn http_failure(status: u16, detail: &str) -> AppError {
    let low = detail.to_lowercase();
    let code = if status == 401 || low.contains("invalid api key") || low.contains("unauthorized") {
        "auth"
    } else if status == 402
        || [
            "insufficient_quota",
            "quota exceeded",
            "quota_exceeded",
            "insufficient balance",
            "balance insufficient",
            "余额不足",
            "额度不足",
            "欠费",
            "account balance",
            "billing",
        ]
        .iter()
        .any(|s| low.contains(s))
    {
        "quota"
    } else if status == 429 {
        "rate-limit"
    } else if [408, 409, 425].contains(&status) || status >= 500 {
        "transient"
    } else {
        "request"
    };
    AppError::new(code, format!("Provider HTTP {status}"))
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(rename = "AiUsage")]
pub struct Usage {
    #[ts(type = "number")]
    pub input: u64,
    #[ts(type = "number")]
    pub output: u64,
    #[ts(type = "number")]
    pub total: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(rename = "AiMeter")]
pub struct Meter {
    #[ts(type = "number")]
    pub attempts: u64,
    #[ts(type = "number")]
    pub successful_http: u64,
    pub current: Usage,
    pub historical_cache: Usage,
    pub raw_usage: Vec<Value>,
}
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}
impl Meter {
    // The transport is called once per attempt; failures and malformed output do not erase usage.
    pub fn attempt(
        &mut self,
        protocol: &Protocol,
        specs: &Specs,
        transport: impl FnOnce() -> Result<HttpResponse>,
    ) -> Result<Value> {
        self.attempts += 1;
        let response = transport()?;
        self.observe(protocol, specs, response)
    }
    /// Durable callers reserve an attempt before transport, then record its
    /// response and usage before validating model output.
    pub fn observe(
        &mut self,
        protocol: &Protocol,
        specs: &Specs,
        response: HttpResponse,
    ) -> Result<Value> {
        if !(200..300).contains(&response.status) {
            return Err(http_failure(
                response.status,
                &String::from_utf8_lossy(&response.body),
            ));
        }
        self.successful_http += 1;
        let payload: Value = serde_json::from_slice(&response.body)
            .map_err(|e| AppError::new("provider-response", e))?;
        if let Some(usage) = payload.get("usage").filter(|v| v.is_object()) {
            let count = |key: &str| usage[key].as_u64().unwrap_or(0);
            let input = if usage.get("prompt_tokens").is_some() {
                count("prompt_tokens")
            } else {
                count("input_tokens")
                    .saturating_add(count("cache_creation_input_tokens"))
                    .saturating_add(count("cache_read_input_tokens"))
            };
            let output = usage["completion_tokens"]
                .as_u64()
                .or_else(|| usage["output_tokens"].as_u64())
                .unwrap_or(0);
            self.current.input = self.current.input.saturating_add(input);
            self.current.output = self.current.output.saturating_add(output);
            self.current.total = self.current.total.saturating_add(
                usage["total_tokens"]
                    .as_u64()
                    .unwrap_or(input.saturating_add(output)),
            );
            self.raw_usage.push(usage.clone());
        }
        let text = response_text(protocol, &payload)?;
        validate(&parse_json(&text)?, specs)
    }
    pub fn cache_hit(&mut self, historical: Usage) {
        self.historical_cache.input = self.historical_cache.input.saturating_add(historical.input);
        self.historical_cache.output = self
            .historical_cache
            .output
            .saturating_add(historical.output);
        self.historical_cache.total = self.historical_cache.total.saturating_add(historical.total);
    }
}
pub fn response_text(protocol: &Protocol, payload: &Value) -> Result<String> {
    match protocol {
        Protocol::Openai => {
            let choice = &payload["choices"][0];
            let reason = choice["finish_reason"]
                .as_str()
                .unwrap_or("")
                .to_lowercase();
            if reason == "length" {
                return Err(AppError::new(
                    "output-truncated",
                    "Increase output limit before retrying",
                ));
            }
            if ["content_filter", "content-filter"].contains(&reason.as_str()) {
                return Err(AppError::new("content-filter", "Provider filtered output"));
            }
            if choice["message"]["refusal"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
            {
                return Err(AppError::new(
                    "provider-refusal",
                    "Provider refused request",
                ));
            }
            choice["message"]["content"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| {
                    AppError::new("provider-response", "Missing choices[0].message.content")
                })
        }
        Protocol::Anthropic => {
            let reason = payload["stop_reason"].as_str().unwrap_or("");
            let code = match reason {
                "max_tokens" => Some("output-truncated"),
                "refusal" => Some("provider-refusal"),
                "model_context_window_exceeded" => Some("context-length"),
                "tool_use" | "pause_turn" => Some("provider-response"),
                _ => None,
            };
            if let Some(code) = code {
                return Err(AppError::new(
                    code,
                    "Provider stopped before a usable result",
                ));
            }
            let text = payload["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                Err(AppError::new("provider-response", "No text content blocks"))
            } else {
                Ok(text)
            }
        }
    }
}
pub fn parse_json(text: &str) -> Result<Value> {
    let text = text.trim();
    if let Ok(value) = serde_json::from_str(text) {
        return Ok(value);
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if end > start {
            return serde_json::from_str(&text[start..=end])
                .map_err(|e| AppError::new("malformed-json", e));
        }
    }
    Err(AppError::new(
        "malformed-json",
        "Model content is not a JSON object",
    ))
}
pub fn validate(value: &Value, specs: &Specs) -> Result<Value> {
    let tags = value["tags"]
        .as_array()
        .ok_or_else(|| AppError::new("schema-invalid", "Missing tags array"))?;
    let mut seen = BTreeSet::new();
    let mut coverage: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut out = vec![];
    let mut reviews = vec![];
    let colon = regex::Regex::new(r"\s*:\s*").expect("constant regex");
    for item in tags {
        let raw = item["value"].as_str().unwrap_or("");
        let mut text = clean(raw);
        let field = item["field"].as_str().unwrap_or("");
        if text.is_empty() || text.chars().count() > 320 || !TAG_SECTIONS.contains(&field) {
            return Err(AppError::new(
                "schema-invalid",
                "Invalid tag value or field",
            ));
        }
        let indexes = item["source_indexes"]
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or_else(|| AppError::new("schema-invalid", "Missing source indexes"))?;
        for index in indexes {
            let index = index
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .filter(|i| specs.get(field).is_some_and(|v| *i < v.len()))
                .ok_or_else(|| AppError::new("schema-invalid", "Source index outside input"))?;
            coverage.entry(field.into()).or_default().insert(index);
        }
        if field == "Aspect ratio" {
            text = colon.replace_all(&text, ":").into_owned();
        }
        let confidence = item["confidence"].as_str().unwrap_or("high").to_lowercase();
        let confidence = if ["high", "medium", "low"].contains(&confidence.as_str()) {
            confidence.as_str()
        } else {
            "medium"
        };
        if confidence != "high" {
            reviews.push(json!({"reason":"confidence","value":text,"confidence":confidence}));
        }
        if seen.insert(crate::ownership_key(&text)) {
            out.push(json!({"value":text,"field":field,"source_indexes":indexes,"confidence":confidence,"operation":item["operation"].as_str().unwrap_or("")}));
        }
    }
    for field in TAG_SECTIONS {
        for index in 0..specs.get(field).map_or(0, Vec::len) {
            if !coverage.get(field).is_some_and(|s| s.contains(&index)) {
                reviews
                    .push(json!({"reason":"missing-source-coverage","field":field,"index":index}));
            }
        }
    }
    let warnings = value.get("warnings").cloned().unwrap_or_else(|| json!([]));
    if warnings != json!([]) {
        reviews.push(json!({"reason":"model-warnings","warnings":warnings}));
    }
    Ok(
        json!({"tags":out,"warnings":warnings,"review_reasons":reviews,"requires_review":!reviews.is_empty()}),
    )
}
