use super::*;

#[test]
fn claude_name_uses_base_url() {
    let name = resolve_claude_preset_name("", "https://new.aicode.us.com", &[], None);
    assert_eq!(name, "Claude new.aicode.us.com");
}

#[test]
fn claude_name_strips_url_scheme_and_slashes_when_saved() {
    let name = resolve_claude_preset_name(
        "https://new.aicode.us.com/anthropic/",
        "https://new.aicode.us.com/anthropic",
        &[],
        None,
    );
    assert_eq!(name, "new.aicode.us.comanthropic");
}

#[test]
fn claude_settings_writer_preserves_other_root_fields() {
    let preset = sample_claude_preset();
    let settings = parse_claude_settings_document(
        r#"{
              "model":"glm-5.1",
              "env":{
                "KEEP_ME":"1",
                "ANTHROPIC_BASE_URL":"https://old.example.com",
                "ANTHROPIC_SMALL_FAST_MODEL":"legacy-model"
              }
            }"#,
    )
    .expect("settings should parse");
    let updated = set_claude_settings_in_value(settings, &preset).expect("settings should update");

    assert_eq!(updated.get("model").and_then(Value::as_str), Some("glm-5.1"));
    let env = updated
        .get("env")
        .and_then(Value::as_object)
        .expect("env should exist");
    assert_eq!(env.get("KEEP_ME").and_then(Value::as_str), Some("1"));
    assert_eq!(env.get("ANTHROPIC_API_KEY").and_then(Value::as_str), Some("sk-ant-example"));
    assert!(env.get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
        Some("https://new.aicode.us.com")
    );
    assert_eq!(
        env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
            .and_then(Value::as_str),
        Some("glm-4.5-air")
    );
    assert_eq!(
        env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
            .and_then(Value::as_str),
        Some("glm-5-turbo")
    );
    assert_eq!(
        env.get("ANTHROPIC_DEFAULT_OPUS_MODEL")
            .and_then(Value::as_str),
        Some("glm-5.1")
    );
    assert_eq!(env.get("ANTHROPIC_MODEL").and_then(Value::as_str), None);
    assert!(env.get("ANTHROPIC_SMALL_FAST_MODEL").is_none());
}

#[test]
fn claude_settings_file_writer_preserves_claude_json_trust_state() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");
    runtime.block_on(async {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("webclx-claude-json-preserve-{unique}"));
        let claude_dir = dir.join(".claude");
        fs::create_dir_all(&claude_dir).expect("claude dir should be created");
        let settings_path = claude_dir.join("settings.json");
        fs::write(&settings_path, r#"{"env":{"KEEP_ME":"1"}}"#)
            .expect("settings fixture should be written");
        fs::write(
            dir.join(".claude.json"),
            serde_json::to_vec_pretty(&json!({
                "hasCompletedOnboarding": false,
                "projects": {
                    "/home/codes/webClx": {
                        "trust_level": "trusted"
                    }
                },
                "apiKeyHelper": "existing-helper",
                "customSessionState": {
                    "apiKeyTrusted": true
                }
            }))
            .expect("claude json fixture should encode"),
        )
        .expect("claude json fixture should be written");

        write_claude_settings_file(&settings_path, &sample_claude_preset())
            .await
            .expect("claude settings should be written");

        let claude_json: Value =
            serde_json::from_str(&fs::read_to_string(dir.join(".claude.json")).unwrap())
                .expect("claude json should parse");
        assert_eq!(
            claude_json
                .get("hasCompletedOnboarding")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            claude_json
                .pointer("/projects/~1home~1codes~1webClx/trust_level")
                .and_then(Value::as_str),
            Some("trusted")
        );
        assert_eq!(
            claude_json.get("apiKeyHelper").and_then(Value::as_str),
            Some("existing-helper")
        );
        assert_eq!(
            claude_json
                .pointer("/customSessionState/apiKeyTrusted")
                .and_then(Value::as_bool),
            Some(true)
        );

        fs::remove_dir_all(&dir).ok();
    });
}

#[test]
fn claude_settings_writer_can_use_local_proxy_base_and_placeholder_token() {
    let preset = sample_claude_preset();
    let updated = set_claude_settings_in_value_with_endpoint(
        Value::Object(Map::new()),
        &preset,
        "http://127.0.0.1:11111/api/upstream/anthropic",
        "webclx-local-claude-proxy",
    )
    .expect("settings should update");
    let env = updated
        .get("env")
        .and_then(Value::as_object)
        .expect("env should exist");

    assert_eq!(
        env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
        Some("http://127.0.0.1:11111/api/upstream/anthropic")
    );
    assert_eq!(
        env.get("ANTHROPIC_API_KEY").and_then(Value::as_str),
        Some("webclx-local-claude-proxy")
    );
    assert_eq!(
        env.get("ANTHROPIC_DEFAULT_OPUS_MODEL")
            .and_then(Value::as_str),
        Some("glm-5.1")
    );
}

#[test]
fn claude_settings_writer_applies_extra_env_options() {
    let mut preset = sample_claude_preset();
    preset.config_overrides = vec![PresetConfigOverride {
        key: Some("ANTHROPIC_CUSTOM_HEADER".to_string()),
        value: Some("tenant-a".to_string()),
    }];

    let updated = set_claude_settings_in_value(Value::Object(Map::new()), &preset)
        .expect("settings should update");
    let env = updated
        .get("env")
        .and_then(Value::as_object)
        .expect("env should exist");

    assert_eq!(env.get("ANTHROPIC_CUSTOM_HEADER").and_then(Value::as_str), Some("tenant-a"));
}

#[test]
fn current_claude_state_matches_saved_preset() {
    let preset = sample_claude_preset();
    let settings = json!({
        "model": "glm-5.1",
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-ant-example",
            "ANTHROPIC_BASE_URL": "https://new.aicode.us.com",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-4.5-air",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5-turbo",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5.1"
        }
    });

    let current = derive_current_claude_state(&settings, &[preset.clone()])
        .expect("current state should exist");

    assert_eq!(current.preset_name.as_deref(), Some("Claude example"));
    assert_eq!(current.provider_name.as_deref(), Some("Claude Mirror"));
    assert_eq!(current.management_url.as_deref(), Some("https://new.aicode.us.com/manage"));
}

#[test]
fn current_claude_state_matches_extra_env_options() {
    let mut preset = sample_claude_preset();
    preset.config_overrides = vec![PresetConfigOverride {
        key: Some("ANTHROPIC_CUSTOM_HEADER".to_string()),
        value: Some("tenant-a".to_string()),
    }];
    let settings = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-ant-example",
            "ANTHROPIC_BASE_URL": "https://new.aicode.us.com",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-4.5-air",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5-turbo",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5.1",
            "ANTHROPIC_CUSTOM_HEADER": "tenant-a"
        }
    });

    let current = derive_current_claude_state(&settings, &[preset.clone()])
        .expect("current state should exist");

    assert_eq!(current.preset_name.as_deref(), Some("Claude example"));
    assert_eq!(
        current
            .config_values
            .get("ANTHROPIC_CUSTOM_HEADER")
            .map(String::as_str),
        Some("tenant-a")
    );
}

#[test]
fn current_claude_state_matches_local_proxy_preset_scoped_token() {
    let preset = sample_claude_preset();
    let settings = json!({
        "env": {
            "ANTHROPIC_API_KEY": local_proxy_claude_token_for_preset_id(&preset.id),
            "ANTHROPIC_BASE_URL": claude_provider_base_url_for_mode(&preset, true),
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-4.5-air",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5-turbo",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5.1"
        }
    });

    let current =
        derive_current_claude_state(&settings, &[preset]).expect("current state should exist");

    assert_eq!(current.preset_name.as_deref(), Some("Claude example"));
}

#[test]
fn claude_proxy_summary_prefers_current_scoped_token_over_stale_active_state() {
    let old_preset = sample_claude_preset();
    let mut new_preset = sample_claude_preset();
    new_preset.id = "claude-new".to_string();
    new_preset.name = "Claude new".to_string();
    new_preset.auth_token = "sk-ant-new".to_string();
    let settings = json!({
        "env": {
            "ANTHROPIC_API_KEY": local_proxy_claude_token_for_preset_id(&new_preset.id),
            "ANTHROPIC_BASE_URL": claude_provider_base_url_for_mode(&new_preset, true),
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-4.5-air",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5-turbo",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5.1"
        }
    });
    let current = derive_current_claude_state(&settings, &[old_preset.clone(), new_preset.clone()])
        .expect("current state should exist");
    let upstream_proxy = UpstreamProxySettings {
        claude_proxy_enabled: true,
        active_claude_proxy_preset_id: Some(old_preset.id.clone()),
        ..Default::default()
    };

    let old_summary =
        claude_preset_summary_with_proxy_state(&old_preset, Some(&current), &upstream_proxy);
    let new_summary =
        claude_preset_summary_with_proxy_state(&new_preset, Some(&current), &upstream_proxy);

    assert!(!old_summary.active);
    assert!(new_summary.active);
}

#[test]
fn claude_proxy_summary_uses_dynamic_relay_active_state() {
    let old_preset = sample_claude_preset();
    let mut new_preset = sample_claude_preset();
    new_preset.id = "claude-new".to_string();
    new_preset.name = "Claude new".to_string();
    new_preset.auth_token = "sk-ant-new".to_string();
    let settings = json!({
        "env": {
            "ANTHROPIC_API_KEY": crate::LOCAL_PROXY_CLAUDE_TOKEN,
            "ANTHROPIC_BASE_URL": claude_provider_base_url_for_mode(&new_preset, true),
            "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-4.5-air",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5-turbo",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5.1"
        }
    });
    let current = derive_current_claude_state(&settings, &[old_preset.clone(), new_preset.clone()])
        .expect("current state should exist");
    let upstream_proxy = UpstreamProxySettings {
        claude_proxy_enabled: true,
        active_claude_proxy_preset_id: Some(new_preset.id.clone()),
        ..Default::default()
    };

    let old_summary =
        claude_preset_summary_with_proxy_state(&old_preset, Some(&current), &upstream_proxy);
    let new_summary =
        claude_preset_summary_with_proxy_state(&new_preset, Some(&current), &upstream_proxy);

    assert!(!old_summary.active);
    assert!(new_summary.active);
}

#[test]
fn claude_settings_writer_supports_third_party_model_only() {
    let preset = sample_third_party_claude_preset();
    let updated = set_claude_settings_in_value(Value::Object(Map::new()), &preset)
        .expect("settings should update");

    let env = updated
        .get("env")
        .and_then(Value::as_object)
        .expect("env should exist");
    assert_eq!(env.get("ANTHROPIC_MODEL").and_then(Value::as_str), Some("glm-5.1"));
    assert!(env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").is_none());
    assert!(env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").is_none());
    assert!(env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
}

#[test]
fn mixed_claude_model_selection_is_rejected() {
    let error =
        validate_claude_model_selection(Some("claude-sonnet-4-6"), None, None, Some("glm-5.1"))
            .expect_err("mixed claude model settings should be rejected");

    assert_eq!(error.to_string(), "官方模型设置和第三方模型设置不能同时填写。");
}

#[test]
fn direct_deepseek_endpoint_is_rejected_for_claude_code() {
    let error = validate_claude_code_endpoint_compatibility("https://api.deepseek.com/v1")
        .expect_err("direct DeepSeek endpoint should not be applied to Claude Code");

    let message = error.to_string();
    assert!(message.contains("Anthropic 兼容接口"));
    assert!(message.contains("https://api.deepseek.com/anthropic"));
}

#[test]
fn deepseek_anthropic_endpoint_is_allowed_for_claude_code() {
    validate_claude_code_endpoint_compatibility("https://api.deepseek.com/anthropic")
        .expect("DeepSeek Anthropic endpoint should be allowed for Claude Code");
}

#[test]
fn current_claude_state_reads_legacy_small_fast_model_key() {
    let settings = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-ant-example",
            "ANTHROPIC_BASE_URL": "https://new.aicode.us.com",
            "ANTHROPIC_SMALL_FAST_MODEL": "glm-4.5-air"
        }
    });

    let current = derive_current_claude_state(&settings, &[]).expect("current state should exist");

    assert_eq!(current.default_haiku_model.as_deref(), Some("glm-4.5-air"));
    assert!(current.default_sonnet_model.is_none());
    assert!(current.default_opus_model.is_none());
    assert!(current.third_party_model.is_none());
}

#[test]
fn current_claude_state_reads_third_party_model_key() {
    let settings = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-ant-example",
            "ANTHROPIC_BASE_URL": "https://new.aicode.us.com",
            "ANTHROPIC_MODEL": "glm-5.1"
        }
    });

    let current = derive_current_claude_state(&settings, &[]).expect("current state should exist");

    assert_eq!(current.third_party_model.as_deref(), Some("glm-5.1"));
}

#[test]
fn claude_global_defaults_yield_to_preset_models_and_extra_env() {
    let mut preset = sample_claude_preset();
    preset.config_overrides = vec![PresetConfigOverride {
        key: Some("ANTHROPIC_CUSTOM_HEADER".to_string()),
        value: Some("preset-tenant".to_string()),
    }];
    let defaults = [
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "global-haiku"),
        ("ANTHROPIC_CUSTOM_HEADER", "global-tenant"),
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
    ];

    let effective = resolve_effective_claude_config_overrides(&defaults, &preset)
        .expect("Claude defaults should merge");
    let mut effective_preset = preset.clone();
    effective_preset.config_overrides = effective;
    let updated = set_claude_settings_in_value(Value::Object(Map::new()), &effective_preset)
        .expect("effective Claude settings should render");
    let env = updated.get("env").and_then(Value::as_object).unwrap();

    assert_eq!(
        env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
            .and_then(Value::as_str),
        Some("glm-4.5-air")
    );
    assert_eq!(
        env.get("ANTHROPIC_CUSTOM_HEADER").and_then(Value::as_str),
        Some("preset-tenant")
    );
    assert_eq!(
        env.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
            .and_then(Value::as_str),
        Some("1")
    );

    let current = derive_current_claude_state(&updated, &[effective_preset.clone()])
        .expect("effective Claude preset should match current settings");
    let summary = claude_preset_summary_with_effective_proxy_state(
        &preset,
        &effective_preset,
        Some(&current),
        &UpstreamProxySettings::default(),
    );
    assert!(summary.active);
    assert_eq!(summary.config_overrides, preset.config_overrides);
}

#[test]
fn claude_explicit_extra_env_can_override_dedicated_model() {
    let mut preset = sample_claude_preset();
    preset.config_overrides = vec![PresetConfigOverride {
        key: Some("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string()),
        value: Some("explicit-haiku".to_string()),
    }];

    let effective = resolve_effective_claude_config_overrides(
        &[("ANTHROPIC_DEFAULT_HAIKU_MODEL", "global-haiku")],
        &preset,
    )
    .expect("Claude overrides should merge");
    let mut effective_preset = preset;
    effective_preset.config_overrides = effective;
    let updated = set_claude_settings_in_value(Value::Object(Map::new()), &effective_preset)
        .expect("effective Claude settings should render");

    assert_eq!(
        updated
            .pointer("/env/ANTHROPIC_DEFAULT_HAIKU_MODEL")
            .and_then(Value::as_str),
        Some("explicit-haiku")
    );
}
