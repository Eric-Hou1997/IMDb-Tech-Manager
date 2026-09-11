use itm_core::{ai::*, *};
use serde_json::{json, Value};
fn specs() -> Specs {
    Specs::from([
        ("Camera".into(), vec!["ARRI".into()]),
        ("Runtime".into(), vec!["124 min".into()]),
    ])
}
fn http(body: Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: serde_json::to_vec(&body).unwrap(),
    }
}
#[test]
fn truncated_output_is_classified_before_content_and_usage_is_kept() {
    let mut meter = Meter::default();
    let result=meter.attempt(&Protocol::Openai,&specs(),||Ok(http(json!({"choices":[{"finish_reason":"length","message":{"content":"{broken"}}],"usage":{"prompt_tokens":17,"completion_tokens":30}}))));
    assert_eq!(result.unwrap_err().code, "output-truncated");
    assert_eq!(meter.attempts, 1);
    assert_eq!(meter.current.input, 17);
    assert_eq!(meter.current.output, 30);
    assert_eq!(larger_limit(2000, 8000), Some(4000));
    assert_eq!(larger_limit(8000, 8000), None);
}
#[test]
fn malformed_json_schema_and_provider_response_are_distinct() {
    for (body, code) in [
        (
            json!({"choices":[{"finish_reason":"stop","message":{"content":"{broken"}}]}),
            "malformed-json",
        ),
        (
            json!({"choices":[{"finish_reason":"stop","message":{"content":"{}"}}]}),
            "schema-invalid",
        ),
        (json!({"choices":[]}), "provider-response"),
    ] {
        let mut meter = Meter::default();
        assert_eq!(
            meter
                .attempt(&Protocol::Openai, &specs(), || Ok(http(body)))
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(meter.attempts, 1);
    }
}
#[test]
fn transport_error_counts_attempt_but_not_success() {
    let mut meter = Meter::default();
    assert!(meter
        .attempt(&Protocol::Openai, &specs(), || Err(AppError::new(
            "network-timeout",
            "injected"
        )))
        .is_err());
    assert_eq!(meter.attempts, 1);
    assert_eq!(meter.successful_http, 0);
}
#[test]
fn cached_usage_never_charges_current_run() {
    let mut meter = Meter::default();
    meter.cache_hit(Usage {
        input: 100,
        output: 30,
        total: 130,
    });
    assert_eq!(meter.attempts, 0);
    assert_eq!(meter.current, Usage::default());
    assert_eq!(meter.historical_cache.total, 130);
}
#[test]
fn provider_http_failures_remain_distinct() {
    for (status, body, expected) in [
        (401, "", "auth"),
        (429, "insufficient_quota", "quota"),
        (429, "", "rate-limit"),
        (503, "", "transient"),
        (403, "", "request"),
    ] {
        assert_eq!(http_failure(status, body).code, expected);
    }
}
#[test]
fn qwen_default_request_is_compact_non_thinking_and_keeps_prompt() {
    let cfg = Config {
        provider: "qwen".into(),
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
        model: "qwen-plus".into(),
        ..Config::default()
    };
    let body = request(&cfg, &specs(), &[]).unwrap();
    assert_eq!(body["enable_thinking"], false);
    let user: Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert!(user["technical_specs"].get("Runtime").is_none());
    let prompt = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(prompt.starts_with(DEFAULT_PROMPT.trim()));
    assert!(prompt.ends_with(LANGUAGE_BOUNDARY));
}
#[test]
fn provider_extensions_do_not_leak_and_custom_prompt_is_preserved() {
    let cfg = Config {
        provider: "other".into(),
        base_url: "https://example.test/v1".into(),
        model: "model".into(),
        prompt: "我的自定义 prompt".into(),
        ..Config::default()
    };
    let body = request(&cfg, &specs(), &[]).unwrap();
    assert!(body.get("enable_thinking").is_none());
    assert_eq!(
        body["messages"][0]["content"],
        format!("我的自定义 prompt\n\n{LANGUAGE_BOUNDARY}")
    );
}
#[test]
fn provenance_coverage_is_recorded_before_deduplication() {
    let input = Specs::from([
        ("Negative Format".into(), vec!["35 mm".into()]),
        ("Printed Film Format".into(), vec!["35 mm".into()]),
    ]);
    let output=validate(&json!({"tags":[{"value":"35 mm","field":"Negative Format","source_indexes":[0]},{"value":"35 mm","field":"Printed Film Format","source_indexes":[0]}]}),&input).unwrap();
    assert_eq!(output["tags"].as_array().unwrap().len(), 1);
    assert_eq!(output["requires_review"], false);
    let empty = validate(&json!({"tags":[]}), &input).unwrap();
    assert_eq!(empty["requires_review"], true);
}
#[test]
fn invalid_source_index_is_not_writable() {
    assert_eq!(
        validate(
            &json!({"tags":[{"value":"ARRI","field":"Camera","source_indexes":[1]}]}),
            &specs()
        )
        .unwrap_err()
        .code,
        "schema-invalid"
    );
}
#[test]
fn anthropic_usage_includes_cache_tokens_before_validation() {
    let mut meter = Meter::default();
    let result=meter.attempt(&Protocol::Anthropic,&specs(),||Ok(http(json!({"stop_reason":"max_tokens","usage":{"input_tokens":5,"cache_read_input_tokens":10,"cache_creation_input_tokens":7,"output_tokens":9},"content":[]}))));
    assert_eq!(result.unwrap_err().code, "output-truncated");
    assert_eq!(meter.current.input, 22);
    assert_eq!(meter.current.output, 9);
}
#[test]
fn explicit_retry_or_changed_configuration_bypasses_failure_skip() {
    let mut cfg = Config::default();
    let first = failure_fingerprint(&cfg, &specs(), &[]).unwrap();
    assert!(skip_unchanged(Some(&first), &first, false));
    assert!(!skip_unchanged(Some(&first), &first, true));
    cfg.model = "another-model".into();
    let next = failure_fingerprint(&cfg, &specs(), &[]).unwrap();
    assert_ne!(first, next);
    assert!(!skip_unchanged(Some(&first), &next, false));
}
