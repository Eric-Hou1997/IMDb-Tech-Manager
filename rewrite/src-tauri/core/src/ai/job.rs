use super::*;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(rename = "AiSettings")]
pub struct Settings {
    pub config: Config,
    pub credential_account: String,
    pub enabled: bool,
    pub json_mode: String,
    pub timeout_seconds: u32,
    pub retry_count: u32,
    pub output_token_cap: u32,
    pub input_price_per_million: f64,
    pub output_price_per_million: f64,
    pub warning_policy: String,
    pub fallback_mode: String,
    pub legacy_cleanup_mode: String,
    #[ts(type = "number")]
    pub run_request_limit: u64,
    #[ts(type = "number")]
    pub run_token_limit: u64,
    pub run_cost_limit: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            config: Config::default(),
            credential_account: String::new(),
            enabled: false,
            json_mode: "auto".into(),
            timeout_seconds: 90,
            retry_count: 2,
            output_token_cap: 10000,
            input_price_per_million: 0.0,
            output_price_per_million: 0.0,
            warning_policy: "review".into(),
            fallback_mode: "abort".into(),
            legacy_cleanup_mode: "strict".into(),
            run_request_limit: 0,
            run_token_limit: 0,
            run_cost_limit: 0.0,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        endpoint(&self.config)?;
        request(&self.config, &Specs::new(), &[])?;
        if !(10..=600).contains(&self.timeout_seconds)
            || !(0.0..2.0).contains(&self.config.temperature)
            || self.config.top_p <= 0.0
            || self.config.top_p > 1.0
            || !(128..=32768).contains(&self.config.max_tokens)
            || self.output_token_cap < self.config.max_tokens
            || self.retry_count > 5
            || !(4096..=32768).contains(&self.output_token_cap)
            || !["auto", "on", "off"].contains(&self.json_mode.as_str())
            || !["review", "accept"].contains(&self.warning_policy.as_str())
            || !["abort", "local-rules"].contains(&self.fallback_mode.as_str())
            || !["strict", "inferred"].contains(&self.legacy_cleanup_mode.as_str())
            || [
                self.input_price_per_million,
                self.output_price_per_million,
                self.run_cost_limit,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(AppError::new(
                "invalid-ai-config",
                "Invalid retry, timeout, review or cost setting",
            ));
        }
        Ok(())
    }
    pub fn cost(&self, usage: &Usage) -> f64 {
        usage.input as f64 * self.input_price_per_million / 1_000_000.0
            + usage.output as f64 * self.output_price_per_million / 1_000_000.0
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "AiProfile")]
pub struct Profile {
    pub settings: Settings,
    pub credential_ready: bool,
    pub credential_error: Option<AppError>,
}
pub fn endpoint(config: &Config) -> Result<String> {
    let mut url = url::Url::parse(config.base_url.trim())
        .map_err(|e| AppError::new("invalid-ai-endpoint", e))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || !["https", "http"].contains(&url.scheme())
    {
        return Err(AppError::new(
            "invalid-ai-endpoint",
            "Use an HTTP(S) provider URL without embedded credentials, query or fragment",
        ));
    }
    let suffix = match config.protocol {
        Protocol::Openai => "chat/completions",
        Protocol::Anthropic => "v1/messages",
    };
    let path = url.path().trim_end_matches('/');
    if !path.to_ascii_lowercase().ends_with(&format!("/{suffix}")) {
        let suffix = if config.protocol == Protocol::Anthropic
            && path.to_ascii_lowercase().ends_with("/v1")
        {
            "messages"
        } else {
            suffix
        };
        url.set_path(&format!("{path}/{suffix}"));
    }
    Ok(url.into())
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "AiRequest")]
pub struct Request {
    pub operation_id: String,
    pub item_id: String,
    pub expected_hash: String,
    pub force: bool,
    pub retry_failed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "AiAttempt")]
pub struct Attempt {
    pub sequence: u32,
    pub request: Value,
    pub phase: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub http_status: Option<u16>,
    pub raw_usage: Option<Value>,
    #[serde(default)]
    pub model: Option<String>,
    pub error: Option<AppError>,
    pub retry_after_seconds: f64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "AiPurpose")]
pub enum Purpose {
    #[default]
    Generate,
    ConnectionTest,
}
pub const CONNECTION_TEST_ITEM: &str = "__connection_test__";
pub fn connection_specs() -> Specs {
    let mut specs: Specs =
        serde_json::from_str(include_str!("../../assets/ai-connection-test-specs.json"))
            .expect("verified connection test fixture");
    for section in crate::specs::SECTIONS {
        specs.entry(section.into()).or_default();
    }
    specs
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "AiRecord")]
pub struct Record {
    #[serde(default)]
    pub purpose: Purpose,
    #[serde(default)]
    pub resolved_model: String,
    #[serde(default)]
    pub batch_id: Option<String>,
    #[serde(default = "default_engine")]
    pub engine: String,
    pub request: Request,
    pub path: String,
    pub title: String,
    pub year: String,
    pub imdb: String,
    pub media_kind: String,
    pub locale: crate::Locale,
    pub settings: Settings,
    pub specs: Specs,
    pub existing: Vec<Value>,
    pub fingerprint: String,
    pub phase: String,
    pub cached: bool,
    #[serde(default)]
    pub legacy_cache: Option<super::legacy_cache::Origin>,
    #[serde(default)]
    pub legacy_failure: Option<super::legacy_failure::Failure>,
    pub meter: Meter,
    pub cost: f64,
    pub historical_cost: f64,
    pub result: Option<Value>,
    pub error: Option<AppError>,
    pub attempts: Vec<Attempt>,
    pub started_at: String,
    pub finished_at: Option<String>,
}
fn default_engine() -> String {
    "ai".into()
}
pub fn needs_review(record: &Record) -> bool {
    record.engine == "ai"
        && record.settings.warning_policy == "review"
        && record
            .result
            .as_ref()
            .is_some_and(|v| v["requires_review"] == true)
}
/// Explicitly configured fallback preserves the failed AI request and its cost.
/// Account, budget, pause and cancellation failures never become rule output.
pub fn fallback(record: &mut Record) {
    if record.purpose == Purpose::ConnectionTest {
        return;
    }
    let Some(error) = &record.error else { return };
    if record.settings.fallback_mode != "local-rules"
        || !["failed", "skipped-unchanged-failure"].contains(&record.phase.as_str())
        || [
            "auth",
            "quota",
            "rate-limit",
            "paused",
            "budget-exhausted",
            "cancelled",
            "credential-missing",
            "credential-read",
        ]
        .contains(&error.code.as_str())
    {
        return;
    }
    record.result = Some(
        json!({"tags":crate::rules::entries(&record.specs).into_iter().map(|e|json!({"value":e.value,"field":e.field,"source_indexes":e.source_indexes,"confidence":"high","operation":"local-rule"})).collect::<Vec<_>>(),"warnings":[],"review_reasons":[],"requires_review":false}),
    );
    record.engine = "local-rules".into();
    record.phase = "review-ready".into();
    record.finished_at = Some(now());
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
/// Reconstruct the next request from immutable input and recorded outcomes.
/// Each recovery alters the body; truncation uses the user's promised ceiling once.
pub fn next(record: &Record) -> Result<Option<(Value, f64)>> {
    let s = &record.settings;
    if !["requested", "running"].contains(&record.phase.as_str()) {
        return Ok(None);
    }
    if s.run_request_limit > 0 && record.meter.attempts >= s.run_request_limit
        || s.run_token_limit > 0 && record.meter.current.total >= s.run_token_limit
        || s.run_cost_limit > 0.0 && record.cost >= s.run_cost_limit
    {
        return Err(AppError::new(
            "budget-exhausted",
            "AI request, token or cost limit reached",
        ));
    }
    let mut cfg = s.config.clone();
    cfg.json_mode = s.json_mode != "off";
    let mut recovered = BTreeSet::new();
    let mut transient = 0;
    let mut delay = 0.0;
    let mut instruction = String::new();
    for attempt in &record.attempts {
        delay = 0.0;
        let Some(error) = &attempt.error else {
            return Ok(None);
        };
        let code = error.code.as_str();
        match code {
            "output-truncated"
                if !recovered.contains(code) && s.output_token_cap > cfg.max_tokens =>
            {
                recovered.insert(code);
                cfg.max_tokens = s.output_token_cap;
                instruction =
                    "上次输出被截断。请在新的输出长度内返回完整 JSON，不要省略任何字段或数组结尾。"
                        .into();
                // extra_body must not silently override the larger recovery limit.
                cfg.extra_body.remove("max_tokens");
            }
            "malformed-json" | "schema-invalid" if !recovered.contains(code) => {
                recovered.insert(code);
                cfg.json_mode = true;
                cfg.extra_body.remove("response_format");
                instruction = if code == "malformed-json" {
                    "上次返回的 JSON 语法不完整。请重新生成完整、严格有效的 JSON；只返回 JSON 对象。".into()
                } else {
                    format!(
                        "上次 JSON 未通过标签结构校验：{}。请按既定 schema 完整重生。",
                        error.message.chars().take(800).collect::<String>()
                    )
                };
            }
            "request"
                if matches!(attempt.http_status, Some(400 | 422))
                    && s.config.prompt_cache_mode == "auto"
                    && super::qwen(&s.config)
                    && cfg.prompt_cache_mode != "off" =>
            {
                cfg.prompt_cache_mode = "off".into();
            }
            "request"
                if matches!(attempt.http_status, Some(400 | 422))
                    && s.json_mode == "auto"
                    && cfg.json_mode =>
            {
                cfg.json_mode = false;
                cfg.extra_body.remove("response_format");
                cfg.extra_body.remove("output_config");
            }
            "rate-limit" | "transient" | "provider-response" | "request"
                if transient < s.retry_count =>
            {
                delay = if attempt.retry_after_seconds > 0.0 {
                    attempt.retry_after_seconds.min(600.0)
                } else {
                    (1.2_f64 * 2_f64.powi(transient as i32)).min(8.0)
                };
                transient += 1;
            }
            _ => return Ok(None),
        }
    }
    let mut body = request(&cfg, &record.specs, &record.existing)?;
    if !instruction.is_empty() {
        let user = body["messages"]
            .as_array_mut()
            .expect("request messages")
            .iter_mut()
            .find(|m| m["role"] == "user")
            .expect("user payload");
        let mut payload: Value =
            serde_json::from_str(user["content"].as_str().expect("user JSON"))?;
        payload["recovery_instruction"] = json!(instruction);
        user["content"] = json!(serde_json::to_string(&payload)?);
        body["temperature"] = json!(0);
    }
    // A truncated request can include a user override larger than cfg.max_tokens.
    // Never repeat it unless the actual outgoing limit increased.
    if let Some(last) = record.attempts.last().filter(|a| {
        a.error
            .as_ref()
            .is_some_and(|e| e.code == "output-truncated")
    }) {
        if body["max_tokens"].as_u64().unwrap_or(0)
            <= last.request["max_tokens"].as_u64().unwrap_or(0)
        {
            return Ok(None);
        }
    }
    Ok(Some((body, delay)))
}
