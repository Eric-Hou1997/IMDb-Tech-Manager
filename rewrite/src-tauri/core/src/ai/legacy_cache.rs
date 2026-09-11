//! Exact v4 Python cache identity; no legacy interpreter in the runtime.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "LegacyAiCacheOrigin")]
pub struct Origin {
    pub import_id: String,
    pub source: String,
    pub archive_hash: String,
    pub created_at: String,
    pub model: String,
    pub raw_usage: Value,
}

// Python json.dumps uses Python's float representation, including .0 for
// integral floats and two-digit signed exponents. Only temperature/top_p are
// floats in this identity; extra_body is the original JSON *string*.
fn python_json(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Number(n) if n.is_f64() => {
            let v = n.as_f64().expect("finite JSON number");
            if v != 0.0 && (v.abs() < 1e-4 || v.abs() >= 1e16) {
                let raw = format!("{v:e}");
                let (mantissa, exponent) = raw.split_once('e').expect("scientific float");
                let exponent: i32 = exponent.parse().expect("numeric exponent");
                format!("{mantissa}e{exponent:+03}")
            } else {
                let raw = v.to_string();
                if raw.contains('.') {
                    raw
                } else {
                    format!("{raw}.0")
                }
            }
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_json)
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
        Value::Object(values) => {
            let sorted: BTreeMap<_, _> = values.iter().collect();
            format!(
                "{{{}}}",
                sorted
                    .into_iter()
                    .map(|(k, v)| Ok(format!("{}:{}", serde_json::to_string(k)?, python_json(v)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(",")
            )
        }
        _ => serde_json::to_string(value)?,
    })
}

pub fn key(
    settings: &job::Settings,
    specs: &Specs,
    existing: &[Value],
    extra_body: &str,
) -> Result<String> {
    let cfg = &settings.config;
    // Formatting is significant to the old key, but its parsed request must
    // also equal the current settings before we consider any historical key.
    let parsed: BTreeMap<String, Value> = serde_json::from_str(extra_body)?;
    if parsed != cfg.extra_body {
        return Err(AppError::new(
            "legacy-ai-cache-config",
            "Historical request parameters differ",
        ));
    }
    let input: Specs = TAG_SECTIONS
        .into_iter()
        .filter_map(|k| {
            specs
                .get(k)
                .filter(|v| !v.is_empty())
                .map(|v| (k.into(), v.clone()))
        })
        .collect();
    let stable = json!({
        "schema":2,"existing":existing,"api_protocol":cfg.protocol,
        "provider":cfg.provider,"base_url":cfg.base_url,"model":cfg.model,
        "prompt":format!("{}\n\n{}",cfg.prompt.trim(),LANGUAGE_BOUNDARY),
        "temperature":cfg.temperature,"top_p":cfg.top_p,"max_tokens":cfg.max_tokens,
        "json_mode":settings.json_mode,"thinking_mode":cfg.thinking_mode,
        "prompt_cache_mode":cfg.prompt_cache_mode,"extra_body":extra_body,
        "output_language":cfg.output_language,"language_contract":1,"specs":input
    });
    Ok(hash(python_json(&stable)?.as_bytes()))
}

pub struct Hit {
    pub result: Value,
    pub usage: Usage,
    pub cost: f64,
    pub created_at: String,
    pub model: String,
    pub raw_usage: Value,
}
pub fn validate(raw: &[u8], settings: &job::Settings, specs: &Specs) -> Result<Hit> {
    let invalid = || {
        AppError::new(
            "legacy-ai-cache-invalid",
            "Historical AI cache failed validation; force rebuild to bypass it",
        )
    };
    if raw.len() > 8 * 1024 * 1024 {
        return Err(invalid());
    }
    let value: Value = serde_json::from_slice(raw).map_err(|_| invalid())?;
    let text = |key: &str| value[key].as_str().ok_or_else(invalid);
    let created_at = text("created_at")?;
    chrono::DateTime::parse_from_rfc3339(created_at).map_err(|_| invalid())?;
    let prompt_hash =
        hash(format!("{}\n\n{}", settings.config.prompt.trim(), LANGUAGE_BOUNDARY).as_bytes());
    let spec_hash = text("spec_hash")?;
    if value["cache_schema"] != 2
        || text("prompt_hash")? != &prompt_hash[..16]
        || text("output_language")? != settings.config.output_language
        || spec_hash.len() != 64
        || !spec_hash.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(invalid());
    }
    // The old key deliberately excludes non-tag fields. Its recorded full
    // spec_hash can differ after Runtime/Color/etc. edits and is not a new gate.
    let cost = value["cost"]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or_else(invalid)?;
    let raw_usage = value
        .get("usage")
        .filter(|v| v.is_object())
        .ok_or_else(invalid)?;
    let count = |k: &str| -> Result<u64> {
        match raw_usage.get(k) {
            None | Some(Value::Null) => Ok(0),
            Some(v) => v
                .as_u64()
                .filter(|n| *n <= 9_007_199_254_740_991)
                .ok_or_else(invalid),
        }
    };
    let input = if !raw_usage["prompt_tokens"].is_null() {
        count("prompt_tokens")?
    } else {
        count("input_tokens")?
            .checked_add(count("cache_creation_input_tokens")?)
            .and_then(|v| v.checked_add(count("cache_read_input_tokens").ok()?))
            .ok_or_else(invalid)?
    };
    let output = count("completion_tokens")?;
    let output = if output == 0 {
        count("output_tokens")?
    } else {
        output
    };
    let total = count("total_tokens")?;
    let total = if total == 0 {
        input.checked_add(output).ok_or_else(invalid)?
    } else {
        total
    };
    let result = super::validate(&value["result"], specs).map_err(|_| invalid())?;
    Ok(Hit {
        result,
        usage: Usage {
            input,
            output,
            total,
        },
        cost,
        created_at: created_at.into(),
        model: text("model")?.into(),
        raw_usage: raw_usage.clone(),
    })
}
