use itm_core::{ai::Protocol, migration::ai_profile, store::Store};
use serde_json::json;
use std::fs;
fn profile() -> serde_json::Value {
    json!({"ai":{"enabled":true,"provider":"anthropic","base_url":"https://provider.example/v1/messages","model":"existing-model","prompt":"  用户自定义提示词\n保留空白与标点。\r\n","temperature":0.4,"top_p":0.8,"max_tokens":1800,"output_token_cap":8192,"json_mode":"off","thinking_mode":"on","prompt_cache_mode":"on","timeout_seconds":125,"retry_count":4,"extra_body":"{\"metadata\":{\"test\":true}}","input_price_per_million":2.5,"output_price_per_million":7.5,"run_request_limit":30,"run_token_limit":9000,"run_cost_limit":12.0,"warning_policy":"accept","fallback_mode":"local-rules","legacy_cleanup_mode":"inferred"}})
}
#[test]
fn legacy_profile_preserves_custom_prompt_protocol_policy_and_native_credential_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let legacy = root.join("legacy");
    fs::create_dir(&legacy).unwrap();
    let bytes = serde_json::to_vec_pretty(&profile()).unwrap();
    fs::write(legacy.join("config.json"), &bytes).unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let plan = store
        .prepare_migration("migration", &legacy, "itm-engine")
        .unwrap();
    assert_eq!(plan.adapters.len(), 1);
    let adapter = &plan.adapters[0];
    assert_eq!(adapter.target, "ai-settings");
    assert!(adapter.before_hash.is_none());
    assert_eq!(adapter.value["config"]["prompt"], profile()["ai"]["prompt"]);
    assert_eq!(adapter.warnings.len(), 2);
    let receipt = store
        .apply_migration("migration", &plan.fingerprint)
        .unwrap();
    assert_eq!(receipt.applied_adapters, vec!["ai-settings"]);
    let settings = store.ai_settings().unwrap();
    assert_eq!(settings.config.protocol, Protocol::Anthropic);
    assert_eq!(settings.config.max_tokens, 2000);
    assert_eq!(settings.output_token_cap, 10000);
    assert_eq!(settings.config.extra_body["metadata"]["test"], true);
    assert_eq!(settings.credential_account, ai_profile::LEGACY_ACCOUNT);
    assert_eq!(settings.retry_count, 4);
    assert_eq!(settings.run_token_limit, 9000);
    assert_eq!(settings.warning_policy, "accept");
    assert_eq!(
        settings.config.prompt,
        profile()["ai"]["prompt"].as_str().unwrap()
    );
    assert_eq!(fs::read(legacy.join("config.json")).unwrap(), bytes);
    assert_eq!(
        store.legacy_artifact("migration", "config.json").unwrap(),
        bytes
    );
    assert_eq!(
        store
            .apply_migration("migration", &plan.fingerprint)
            .unwrap()
            .applied_adapters,
        receipt.applied_adapters
    );
    drop(store);
    let reopened = Store::open(&root.join("state.sqlite")).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.ai_settings().unwrap()).unwrap(),
        serde_json::to_value(settings).unwrap()
    );
}
#[test]
fn changed_destination_profile_rejects_import_and_rolls_back_archives() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let legacy = root.join("legacy");
    fs::create_dir(&legacy).unwrap();
    fs::write(
        legacy.join("config.json"),
        serde_json::to_vec(&profile()).unwrap(),
    )
    .unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    let plan = store
        .prepare_migration("migration", &legacy, "itm-engine")
        .unwrap();
    let mut settings = ai_profile::adapt(&profile()).unwrap().unwrap();
    settings.config.model = "new-user-choice".into();
    store.save_ai_settings("user-change", settings).unwrap();
    assert_eq!(
        store
            .apply_migration("migration", &plan.fingerprint)
            .unwrap_err()
            .code,
        "migration-configuration-conflict"
    );
    assert_eq!(store.ai_settings().unwrap().config.model, "new-user-choice");
    assert!(store.legacy_artifact("migration", "config.json").is_err());
}
#[test]
fn unknown_protocol_and_invalid_extra_body_remain_reviewable_without_guessing() {
    let mut old = profile();
    old["ai"]["provider"] = "custom-unknown".into();
    old["ai"]["base_url"] = "https://provider.example".into();
    assert_eq!(
        ai_profile::adapt(&old).unwrap_err().code,
        "legacy-ai-protocol-unresolved"
    );
    old["ai"]["api_protocol"] = "openai".into();
    assert_eq!(
        ai_profile::adapt(&old).unwrap().unwrap().config.protocol,
        Protocol::Openai
    );
    old["ai"]["extra_body"] = "{broken".into();
    assert_eq!(
        ai_profile::adapt(&old).unwrap_err().code,
        "legacy-ai-extra-body"
    );
}

#[test]
fn credentials_embedded_in_legacy_extra_body_do_not_enter_the_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let legacy = root.join("legacy");
    fs::create_dir(&legacy).unwrap();
    let store = Store::open(&root.join("state.sqlite")).unwrap();
    for (i, body) in [
        r#"{"authorization":"fixture-not-a-secret"}"#,
        r#"{"api_key":"unfinished"#,
    ]
    .iter()
    .enumerate()
    {
        let mut old = profile();
        old["ai"]["extra_body"] = (*body).into();
        fs::write(
            legacy.join("config.json"),
            serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        assert_eq!(
            store
                .prepare_migration(&format!("boundary-{i}"), &legacy, "itm-engine")
                .unwrap_err()
                .code,
            "migration-credential-boundary"
        );
    }
}
