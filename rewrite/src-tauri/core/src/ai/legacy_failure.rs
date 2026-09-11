//! v4 failure queue identities and immutable source details.
use super::*;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "LegacyAiFailure")]
pub struct Failure {
    pub path: String,
    pub import_id: String,
    pub source: String,
    pub entry: Value,
    pub active: bool,
    pub resolved_at: Option<String>,
}
impl Failure {
    pub fn validate(&self) -> Result<()> {
        let fingerprint = self.entry["fingerprint"].as_str().unwrap_or("");
        if !self.entry.is_object()
            || self.path.is_empty()
            || self.path.len() > 32768
            || self.path.contains('\0')
            || self.entry["path"].as_str() != Some(self.path.as_str())
            || (!fingerprint.is_empty()
                && (fingerprint.len() != 64 || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit())))
        {
            return Err(AppError::new(
                "legacy-ai-failure-invalid",
                "Historical failure identity is invalid",
            ));
        }
        Ok(())
    }
    pub fn preference_key(path: &str) -> String {
        format!("legacy-ai-failure:{}", hash(path.as_bytes()))
    }
    pub fn kind(&self) -> &str {
        self.entry["kind"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or("request")
    }
}
pub fn fingerprint(
    path: &str,
    specs: &Specs,
    settings: &job::Settings,
    kind: &str,
    extra_body: &str,
) -> Result<String> {
    let cfg = &settings.config;
    let parsed: BTreeMap<String, Value> = serde_json::from_str(extra_body)?;
    if parsed != cfg.extra_body {
        return Err(AppError::new(
            "legacy-ai-failure-config",
            "Historical request parameters differ",
        ));
    }
    let params = json!({"temperature":cfg.temperature,"top_p":cfg.top_p,"max_tokens":cfg.max_tokens,"output_token_cap":settings.output_token_cap,"json_mode":settings.json_mode,"thinking_mode":cfg.thinking_mode,"prompt_cache_mode":cfg.prompt_cache_mode,"extra_body":extra_body,"output_language":cfg.output_language,"language_contract":1,"result_schema":2,"failure_policy":1});
    let all_specs: Specs = crate::specs::SECTIONS
        .into_iter()
        .map(|k| (k.into(), specs.get(k).cloned().unwrap_or_default()))
        .collect();
    let payload = json!({"path":path,"spec_hash":hash(serde_json::to_string(&all_specs)?.as_bytes()),"provider":clean(&cfg.provider),"api_protocol":cfg.protocol,"endpoint":job::endpoint(cfg)?,"model":clean(&cfg.model),"prompt_hash":hash(format!("{}\n\n{}",cfg.prompt.trim(),LANGUAGE_BOUNDARY).as_bytes()),"params_hash":hash(legacy_cache::python_json(&params)?.as_bytes()),"failure_kind":clean(kind)});
    Ok(hash(legacy_cache::python_json(&payload)?.as_bytes()))
}
