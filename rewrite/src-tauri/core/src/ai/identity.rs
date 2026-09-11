//! Request identity is independent of accounting and queue presentation policy.
use super::*;
pub struct Keys {
    pub cache: String,
    pub failure: String,
}
pub fn keys(
    settings: &job::Settings,
    specs: &Specs,
    existing: &[Value],
    path: &str,
) -> Result<Keys> {
    let mut cfg = settings.config.clone();
    cfg.json_mode = settings.json_mode != "off";
    let body = request(&cfg, specs, existing)?;
    let cache=hash(serde_json::to_string(&json!({"policy":"rust-ai-result-2","protocol":cfg.protocol,"provider":cfg.provider,"endpoint":job::endpoint(&cfg)?,"json_mode":settings.json_mode,"thinking_mode":cfg.thinking_mode,"prompt_cache_mode":cfg.prompt_cache_mode,"request":body}))?.as_bytes());
    // Full specs and the recovery ceiling affect failure retry eligibility.
    // Prices, budgets, timeout and retry count do not change model input.
    let all_specs: Specs = crate::specs::SECTIONS
        .into_iter()
        .map(|k| (k.into(), specs.get(k).cloned().unwrap_or_default()))
        .collect();
    let failure=hash(serde_json::to_string(&json!({"policy":"rust-ai-failure-2","path":path,"request":cache,"specs":all_specs,"output_token_cap":settings.output_token_cap,"credential_account":settings.credential_account}))?.as_bytes());
    Ok(Keys { cache, failure })
}
pub fn record_keys(value: &job::Record) -> Result<Keys> {
    keys(&value.settings, &value.specs, &value.existing, &value.path)
}
