use super::{
    ApiResponsesProxyMode, AuthFile, AuthTokens, CodexOAuthManager, CodexOAuthSessionStatus,
    CodexRateLimit, CodexUsageResponse, CodexUsageWindow, CurrentAuthMode, CurrentAuthState,
    PresetConfigOverride, PresetTerminalEnvVar, ResolvedConfigTarget, SaveApiPresetRequest,
    StoredApiPreset, StoredAuthPreset, StoredClaudePreset, UpstreamProxySettings,
    api_preset_enables_local_upstream_proxy_on_apply, api_preset_prefers_local_upstream_proxy,
    api_preset_summary, api_preset_summary_with_proxy_state, api_provider_base_url,
    api_provider_base_url_for_mode, api_provider_options,
    claude_preset_summary_with_effective_proxy_state, claude_preset_summary_with_proxy_state,
    claude_preset_supports_direct_anthropic_endpoint, claude_provider_base_url_for_mode,
    clear_inactive_managed_config_entries_in_content,
    clear_provider_and_set_config_entry_in_config_content,
    clear_provider_and_set_model_in_config_content, clear_provider_in_config_content,
    default_api_provider_options, derive_current_api_state, derive_current_claude_state,
    derive_current_mode, effective_api_responses_proxy, effective_claude_use_local_proxy,
    effective_preset_config_overrides, extract_account_id_from_auth,
    local_proxy_api_key_for_preset_id, local_proxy_api_preset_id_from_api_key,
    local_proxy_claude_token_for_preset_id, merge_codex_snapshot_projects_in_config_content,
    merge_refreshed_auth_preset_details, normalize_api_preset, normalize_claude_preset,
    parse_claude_settings_document, parse_codex_device_poll_interval, preset_summary,
    read_current_config_provider_from_content, resolve_api_management_url, resolve_api_preset_name,
    resolve_auth_preset_name, resolve_claude_preset_name,
    resolve_effective_claude_config_overrides, resolve_effective_preset_config_targets,
    sanitize_preset_config_override, set_api_provider_and_config_entry_in_config_content,
    set_api_provider_and_model_in_config_content, set_api_provider_in_config_content,
    set_claude_settings_in_value, set_claude_settings_in_value_with_endpoint,
    set_local_proxy_auth_header_in_config_content, summarize_remote_body, sync_api_model_catalog,
    touch_auth_last_refresh, upsert_model_catalog_entry_in_value, validate_api_auth_file_sync,
    validate_auth_file_sync, validate_claude_code_endpoint_compatibility,
    validate_claude_model_selection, write_claude_settings_file,
};
use base64::{Engine as _, engine::general_purpose};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

mod claude;

fn sample_auth() -> AuthFile {
    AuthFile {
        openai_api_key: None,
        last_refresh: "2026-04-05T04:00:00Z".to_string(),
        tokens: AuthTokens {
            access_token: "access".to_string(),
            account_id: "100f021d-33a9-46ca-bc96-2d26bcccb2e5".to_string(),
            id_token: "id".to_string(),
            refresh_token: "refresh".to_string(),
        },
    }
}

fn sample_api_preset(base_url: &str, model: Option<&str>) -> StoredApiPreset {
    StoredApiPreset {
        id: "api-1".to_string(),
        name: "Example".to_string(),
        saved_at: 1,
        provider_name: "Example API".to_string(),
        base_url: base_url.to_string(),
        management_url: None,
        wire_api: None,
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: model
            .map(|model| {
                vec![PresetConfigOverride {
                    key: Some("model".to_string()),
                    value: Some(model.to_string()),
                }]
            })
            .unwrap_or_default(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "sk-example".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    }
}

#[test]
fn codex_device_poll_interval_accepts_string_and_number() {
    assert_eq!(parse_codex_device_poll_interval(&json!("7")), 7);
    assert_eq!(parse_codex_device_poll_interval(&json!(9)), 9);
    assert_eq!(parse_codex_device_poll_interval(&Value::Null), 5);
}

#[test]
fn codex_oauth_manager_marks_pending_session_expired() {
    let manager = CodexOAuthManager::new();
    let session = manager.insert_pending(
        "https://auth.openai.com/codex/device",
        "https://auth.openai.com/codex/device?code=ABCD-EFGH",
        "ABCD-EFGH",
        5,
    );

    {
        let mut sessions = manager
            .sessions
            .write()
            .expect("codex oauth session manager poisoned");
        let stored = sessions
            .get_mut(&session.id)
            .expect("session should exist after insertion");
        stored.expires_at = stored.created_at.saturating_sub(1);
    }

    let expired = manager
        .get(&session.id)
        .expect("session should still exist after expiring");

    assert_eq!(expired.status, CodexOAuthSessionStatus::Expired);
    assert!(expired.error.is_some());
}

#[test]
fn reject_empty_token_fields() {
    let mut auth = sample_auth();
    auth.tokens.access_token.clear();
    assert!(validate_auth_file_sync(&auth).is_err());
}

#[test]
fn auth_file_accepts_refresh_time_aliases() {
    let auth: AuthFile = serde_json::from_value(json!({
        "OPENAI_API_KEY": null,
        "refresh_time": "2026-04-20T08:00:00Z",
        "tokens": {
            "access_token": "access",
            "account_id": "acct-1",
            "id_token": "id",
            "refresh_token": "refresh"
        }
    }))
    .expect("refresh_time alias should deserialize");

    assert_eq!(auth.last_refresh, "2026-04-20T08:00:00Z");
}

#[test]
fn auth_file_accepts_flat_cpa_export() {
    let auth: AuthFile = serde_json::from_value(json!({
        "id_token": "",
        "access_token": "access",
        "refresh_token": "",
        "account_id": "acct-cpa",
        "last_refresh": "2026-05-02T19:30:52.000Z",
        "email": "user@example.com",
        "type": "codex",
        "expired": "2026-05-12T19:30:52.000Z"
    }))
    .expect("flat CPA export should deserialize as auth.json");

    assert_eq!(auth.openai_api_key, None);
    assert_eq!(auth.last_refresh, "2026-05-02T19:30:52.000Z");
    assert_eq!(auth.tokens.access_token, "access");
    assert_eq!(auth.tokens.id_token, "");
    assert_eq!(auth.tokens.refresh_token, "");
    assert_eq!(auth.tokens.account_id, "acct-cpa");
    validate_auth_file_sync(&auth).expect("access-only CPA auth should remain usable");
}

#[test]
fn auth_file_accepts_access_token_only_flat_export() {
    let auth: AuthFile = serde_json::from_value(json!({
        "id_token": "",
        "access_token": "access-only",
        "refresh_token": "",
        "account_id": "acct-access-only",
        "last_refresh": "2026-07-14T09:00:00Z",
        "email": "user@example.com",
        "type": "codex",
        "expired": "2026-07-24T09:00:00Z"
    }))
    .expect("access-token-only Codex export should deserialize as auth.json");

    validate_auth_file_sync(&auth)
        .expect("access-token-only Codex export should be accepted for use until expiry");
    assert_eq!(auth.tokens.access_token, "access-only");
    assert_eq!(auth.tokens.account_id, "acct-access-only");
    assert!(auth.tokens.id_token.is_empty());
    assert!(auth.tokens.refresh_token.is_empty());

    let serialized = serde_json::to_value(&auth).expect("access-only auth should serialize");
    let serialized_tokens = serialized["tokens"]
        .as_object()
        .expect("serialized auth should contain tokens");
    // AuthTokens always serializes id_token and refresh_token (strict contract).
    assert!(serialized_tokens.contains_key("id_token"));
    assert!(serialized_tokens.contains_key("refresh_token"));
}

#[test]
fn auth_manager_load_accepts_cpa_array_as_presets() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("webclx-auth-cpa-{unique}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    fs::write(
        dir.join("webclx-auth-presets.json"),
        serde_json::to_vec_pretty(&json!([{
            "id_token": "id",
            "access_token": "access",
            "refresh_token": "refresh",
            "account_id": "acct-cpa",
            "last_refresh": "2026-05-02T19:30:52.000Z",
            "email": "user@example.com",
            "type": "codex",
            "expired": "2026-05-12T19:30:52.000Z"
        }]))
        .expect("fixture should encode"),
    )
    .expect("fixture should be written");

    let manager = super::AuthPresetManager::load(&dir).expect("CPA array should load as presets");
    let presets = manager.auth_presets_snapshot();

    fs::remove_dir_all(&dir).ok();

    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].details.email.as_deref(), Some("user@example.com"));
    assert_eq!(presets[0].auth.tokens.account_id, "acct-cpa");
    assert_eq!(presets[0].auth.last_refresh, "2026-05-02T19:30:52.000Z");
}

#[cfg(unix)]
#[test]
fn auth_manager_secures_runtime_preset_files() {
    use std::os::unix::fs::PermissionsExt;

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("webclx-preset-permissions-{unique}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");

    super::AuthPresetManager::load(&dir).expect("manager should create runtime preset files");

    for file_name in [
        "webclx-auth-presets.json",
        "webclx-api-presets.json",
        "webclx-claude-presets.json",
        "webclx-upstream-proxy.json",
    ] {
        let mode = fs::metadata(dir.join(file_name))
            .expect("runtime preset file should exist")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "{file_name} should be owner-only");
    }

    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn async_preset_write_restores_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");
    runtime.block_on(async {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("webclx-async-permissions-{unique}"));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let manager = super::AuthPresetManager::load(&dir).expect("manager should load");
        let preset_file = manager.preset_file();
        fs::set_permissions(&preset_file, fs::Permissions::from_mode(0o644))
            .expect("test should relax permissions");

        super::persist_auth_presets_async(&manager, &[])
            .await
            .expect("async preset write should succeed");

        let mode = fs::metadata(&preset_file)
            .expect("auth preset file should exist")
            .permissions()
            .mode()
            & 0o777;
        fs::remove_dir_all(&dir).ok();

        assert_eq!(mode, 0o600);
    });
}

#[test]
fn upstream_proxy_settings_default_to_disabled() {
    let settings = UpstreamProxySettings::default();

    assert!(!settings.codex_api_proxy_enabled);
    assert!(!settings.claude_proxy_enabled);
    assert_eq!(settings.active_api_proxy_preset_id, None);
    assert_eq!(settings.active_claude_proxy_preset_id, None);
}

#[test]
fn auth_manager_persists_upstream_proxy_settings() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("webclx-upstream-proxy-{unique}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");

    let manager = super::AuthPresetManager::load(&dir).expect("manager should load");
    let mut settings = manager.upstream_proxy_settings();
    settings.codex_api_proxy_enabled = true;
    settings.active_api_proxy_preset_id = Some("api-1".to_string());
    super::persist_upstream_proxy_settings(&manager, settings.clone())
        .expect("settings should persist");

    let reloaded = super::AuthPresetManager::load(&dir).expect("manager should reload");
    fs::remove_dir_all(&dir).ok();

    assert_eq!(reloaded.upstream_proxy_settings(), settings);
}

#[test]
fn touch_auth_last_refresh_updates_timestamp() {
    let mut auth = sample_auth();
    let previous_refresh = auth.last_refresh.clone();

    touch_auth_last_refresh(&mut auth).expect("touch should succeed");

    assert_ne!(auth.last_refresh, previous_refresh);
    assert!(auth.last_refresh.contains('T'));
    assert!(auth.last_refresh.ends_with('Z'));
}

#[test]
fn remote_error_summary_extracts_json_message() {
    let summary = summarize_remote_body(
        r#"{"error":{"message":"Your refresh token has already been used to generate a new access token.","type":"invalid_request_error"}}"#,
    );

    assert_eq!(
        summary,
        ": Your refresh token has already been used to generate a new access token."
    );
}

#[test]
fn remote_error_summary_handles_detail_message() {
    assert_eq!(summarize_remote_body(r#"{"detail":"Unauthorized"}"#), ": Unauthorized");
}

#[test]
fn reject_empty_api_key() {
    assert!(
        validate_api_auth_file_sync(&super::ApiAuthFile {
            openai_api_key: "   ".to_string(),
        })
        .is_err()
    );
}

#[test]
fn auto_name_uses_account_id_suffix() {
    let name = resolve_auth_preset_name("", &sample_auth(), &[], None);
    assert_eq!(name, "账号 ccb2e5");
}

#[test]
fn api_name_uses_base_url() {
    let name = resolve_api_preset_name("", "https://api.openai.com/v1", &[], None);
    assert_eq!(name, "API api.openai.comv1");
}

#[test]
fn api_name_strips_url_scheme_and_slashes_when_saved() {
    let name = resolve_api_preset_name(
        "https://api.openai.com/v1/",
        "https://api.openai.com/v1",
        &[],
        None,
    );
    assert_eq!(name, "api.openai.comv1");
}

#[test]
fn minimax_codex_model_uses_responses_proxy() {
    let preset = sample_api_preset("https://api.minimaxi.com/v1", Some("codex-MiniMax-M2.7"));

    assert_eq!(effective_api_responses_proxy(&preset), Some(ApiResponsesProxyMode::MinimaxChat));
    assert_eq!(api_provider_options(&preset).wire_api, "responses");
    assert!(api_provider_base_url(&preset).contains("/api/codex-proxy/minimax/v1"));
}

#[test]
fn explicit_direct_mode_keeps_deepseek_v4_flash_on_responses() {
    let mut preset = sample_api_preset("https://api.deepseek.com/v1", Some("deepseek-v4-flash"));
    preset.responses_proxy = Some(
        serde_json::from_str("\"direct\"").expect("direct must be a valid responses proxy mode"),
    );

    normalize_api_preset(&mut preset).expect("preset should normalize");

    assert_eq!(effective_api_responses_proxy(&preset), None);
    assert_eq!(api_provider_base_url(&preset), "https://api.deepseek.com/v1");
    assert_eq!(
        serde_json::to_value(api_preset_summary(&preset, CurrentAuthMode::None, None))
            .expect("summary should serialize")["responses_proxy"],
        "direct"
    );
}

#[test]
fn legacy_empty_mode_keeps_deepseek_v4_flash_on_responses() {
    let preset = sample_api_preset("https://api.deepseek.com/v1", Some("deepseek-v4-flash"));

    assert_eq!(effective_api_responses_proxy(&preset), None);
    assert_eq!(api_provider_base_url(&preset), "https://api.deepseek.com/v1");
}

#[test]
fn anthropic_relay_with_claude_model_uses_responses_proxy() {
    let preset =
        sample_api_preset("https://api.deepseek.com/anthropic", Some("claude-3-5-sonnet-20241022"));

    assert_eq!(
        effective_api_responses_proxy(&preset),
        Some(ApiResponsesProxyMode::AnthropicChat)
    );
    assert!(api_provider_base_url(&preset).contains("/api/codex-proxy/anthropic/v1"));
}

#[test]
fn anthropic_relay_without_claude_model_keeps_original_url() {
    let preset = sample_api_preset("https://api.deepseek.com/anthropic", Some("deepseek-chat"));

    assert_eq!(effective_api_responses_proxy(&preset), None);
    assert_eq!(api_provider_base_url(&preset), "https://api.deepseek.com/anthropic");
}

#[test]
fn explicit_anthropic_chat_responses_proxy_rewrites_base_url() {
    let mut preset = sample_api_preset(
        "https://relay.example.com/anthropic",
        Some("claude-3-5-sonnet-20241022"),
    );
    preset.responses_proxy = Some(ApiResponsesProxyMode::AnthropicChat);

    assert_eq!(
        effective_api_responses_proxy(&preset),
        Some(ApiResponsesProxyMode::AnthropicChat)
    );
    assert!(api_provider_base_url(&preset).contains("/api/codex-proxy/anthropic/v1"));
}

#[test]
fn minimax_non_codex_model_keeps_original_provider_url() {
    let preset = sample_api_preset("https://api.minimaxi.com/v1", Some("MiniMax-Text-01"));

    assert_eq!(effective_api_responses_proxy(&preset), None);
    assert_eq!(api_provider_base_url(&preset), "https://api.minimaxi.com/v1");
}

#[test]
fn api_provider_base_url_uses_local_proxy_when_enabled() {
    let preset = sample_api_preset("https://api.example.com/v1", Some("gpt-5.4"));

    let base_url = api_provider_base_url_for_mode(&preset, true);

    assert!(base_url.ends_with("/api/upstream/openai/v1"));
    assert_ne!(base_url, preset.base_url);
}

#[test]
fn api_provider_base_url_keeps_existing_special_proxy_when_disabled() {
    let preset = sample_api_preset("https://api.minimaxi.com/v1", Some("codex-MiniMax-M2.7"));

    let base_url = api_provider_base_url_for_mode(&preset, false);

    assert!(base_url.contains("/api/codex-proxy/minimax/v1"));
}

#[test]
fn glm_deepseek_and_minimax_presets_prefer_local_upstream_proxy() {
    let glm = sample_api_preset("https://open.bigmodel.cn/api/paas/v4", Some("glm-5.1"));
    let deepseek = sample_api_preset("https://api.deepseek.com/v1", Some("deepseek-v4-pro"));
    let minimax = sample_api_preset("https://api.minimaxi.com/v1", Some("MiniMax-M2.7"));
    let openai = sample_api_preset("https://api.openai.com/v1", Some("gpt-5.4"));

    assert!(api_preset_prefers_local_upstream_proxy(&glm));
    assert!(api_preset_prefers_local_upstream_proxy(&deepseek));
    assert!(api_preset_prefers_local_upstream_proxy(&minimax));
    assert!(!api_preset_prefers_local_upstream_proxy(&openai));
}

#[test]
fn glm_zhipu_presets_use_responses_proxy() {
    let mut preset = sample_api_preset("https://open.bigmodel.cn/api/paas/v4", Some("glm-5.1"));
    preset.wire_api = Some("chat".to_string());

    assert_eq!(effective_api_responses_proxy(&preset), Some(ApiResponsesProxyMode::OpenaiChat));
    assert_eq!(api_provider_options(&preset).wire_api, "responses");

    let summary = api_preset_summary(&preset, CurrentAuthMode::None, None);
    assert_eq!(summary.wire_api.as_deref(), Some("responses"));
    assert_eq!(summary.responses_proxy.as_ref(), Some(&ApiResponsesProxyMode::OpenaiChat));
}

#[test]
fn glm_zhipu_provider_config_writer_uses_supported_responses_wire_api() {
    let preset = sample_api_preset("https://open.bigmodel.cn/api/paas/v4", Some("glm-5.1"));
    let next = set_api_provider_in_config_content(
        "",
        &preset.provider_name,
        &api_provider_base_url(&preset),
        &api_provider_options(&preset),
    )
    .expect("config should update");

    let current = read_current_config_provider_from_content(&next)
        .expect("config should parse")
        .expect("provider should exist");
    assert_eq!(current.wire_api.as_deref(), Some("responses"));
    assert!(!next.contains("wire_api = \"chat\""));
}

#[test]
fn local_zhipu_responses_proxy_keeps_responses_wire_api() {
    let preset =
        sample_api_preset("http://127.0.0.1:11111/api/codex-proxy/zhipu/v1", Some("glm-5.1"));

    assert_eq!(api_provider_options(&preset).wire_api, "responses");
}

#[test]
fn api_preset_apply_proxy_flag_defaults_to_false_for_legacy_presets() {
    let mut preset: StoredApiPreset = serde_json::from_str(
        r#"{
              "id":"api-legacy",
              "name":"旧 API",
              "saved_at":1775363902,
              "base_url":"https://api.example.com/v1/",
              "api_key":"sk-example"
            }"#,
    )
    .expect("old api preset format should deserialize");

    normalize_api_preset(&mut preset).expect("old api preset should normalize");

    assert!(!preset.apply_upstream_proxy_on_switch);
}

#[test]
fn api_preset_request_accepts_apply_proxy_flag() {
    let request: SaveApiPresetRequest = serde_json::from_value(json!({
        "name": "Example",
        "provider_name": "Example",
        "api_key": "sk-example",
        "base_url": "https://api.example.com/v1",
        "apply_upstream_proxy_on_switch": true
    }))
    .expect("request should deserialize");

    assert!(request.apply_upstream_proxy_on_switch);
}

#[test]
fn api_preset_apply_proxy_flag_enables_local_proxy_for_regular_upstreams() {
    let mut preset = sample_api_preset("https://api.openai.com/v1", Some("gpt-5.4"));
    preset.apply_upstream_proxy_on_switch = true;

    assert!(api_preset_enables_local_upstream_proxy_on_apply(&preset));
}

#[test]
fn api_preset_apply_proxy_helper_preserves_saved_direct_choice() {
    let glm = sample_api_preset("https://open.bigmodel.cn/api/paas/v4", Some("glm-5.1"));
    let deepseek = sample_api_preset("https://api.deepseek.com/v1", Some("deepseek-v4-pro"));
    let minimax = sample_api_preset("https://api.minimaxi.com/v1", Some("MiniMax-M2.7"));

    assert!(api_preset_prefers_local_upstream_proxy(&glm));
    assert!(api_preset_prefers_local_upstream_proxy(&deepseek));
    assert!(api_preset_prefers_local_upstream_proxy(&minimax));
    assert!(!api_preset_enables_local_upstream_proxy_on_apply(&glm));
    assert!(!api_preset_enables_local_upstream_proxy_on_apply(&deepseek));
    assert!(!api_preset_enables_local_upstream_proxy_on_apply(&minimax));
}

#[test]
fn api_preset_summary_reports_saved_direct_choice() {
    let minimax = sample_api_preset("https://api.minimaxi.com/v1", Some("MiniMax-M3"));

    let summary = api_preset_summary(&minimax, CurrentAuthMode::None, None);

    assert!(!summary.apply_upstream_proxy_on_switch);
    assert_eq!(summary.provider_base_url, None);
}

#[test]
fn api_preset_summary_describes_chatgpt_oauth_in_shared_table() {
    let mut preset = sample_api_preset("https://chatgpt.com/backend-api/codex", None);
    preset.api_key = "webclx-local-api-proxy:api-oauth".to_string();
    preset.access_token = "oauth-secret-token".to_string();
    preset.account_id = "acct-123456789".to_string();
    preset.access_mode = Some(super::ApiAccessMode::ChatgptOauth);
    preset.apply_upstream_proxy_on_switch = true;

    let summary = api_preset_summary(&preset, CurrentAuthMode::None, None);

    assert_eq!(summary.access_mode, super::ApiAccessMode::ChatgptOauth);
    assert_eq!(summary.account_id.as_deref(), Some("acct-123456789"));
    assert_eq!(summary.masked_access_token.as_deref(), Some("oauth-***oken"));
    assert_eq!(summary.masked_api_key, "OAuth Token");
}

#[test]
fn auth_name_can_reuse_same_value_when_editing_self() {
    let presets = vec![StoredAuthPreset {
        id: "auth-1".to_string(),
        name: "账号 ccb2e5".to_string(),
        saved_at: 1,
        details: Default::default(),
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth: sample_auth(),
        switch_count: 0,
    }];

    let name = resolve_auth_preset_name("账号 ccb2e5", &sample_auth(), &presets, Some("auth-1"));
    assert_eq!(name, "账号 ccb2e5");
}

#[test]
fn api_name_can_reuse_same_value_when_editing_self() {
    let presets = vec![StoredApiPreset {
        id: "api-1".to_string(),
        name: "API api.openai.comv1".to_string(),
        saved_at: 1,
        provider_name: "OpenAI".to_string(),
        base_url: "https://api.openai.com/v1".to_string(),
        management_url: None,
        wire_api: None,
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "sk-example".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    }];

    let name = resolve_api_preset_name(
        "API api.openai.com/v1",
        "https://api.openai.com/v1",
        &presets,
        Some("api-1"),
    );
    assert_eq!(name, "API api.openai.comv1");
}

#[test]
fn preset_without_details_is_backward_compatible() {
    let presets: Vec<StoredAuthPreset> = serde_json::from_str(
        r#"[{
              "id":"auth-1",
              "name":"旧预设",
              "saved_at":1775363902,
              "auth":{
                "OPENAI_API_KEY":null,
                "last_refresh":"2026-04-05T04:00:00Z",
                "tokens":{
                  "access_token":"access",
                  "account_id":"100f021d-33a9-46ca-bc96-2d26bcccb2e5",
                  "id_token":"id",
                  "refresh_token":"refresh"
                }
              }
            }]"#,
    )
    .expect("old preset format should deserialize");

    assert_eq!(presets.len(), 1);
    assert!(presets[0].details.email.is_none());
}

fn fake_jwt(payload: Value) -> String {
    let encoded = general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("header.{encoded}.signature")
}

fn sample_claude_preset() -> StoredClaudePreset {
    StoredClaudePreset {
        id: "claude-1".to_string(),
        name: "Claude example".to_string(),
        saved_at: 1,
        provider_name: "Claude Mirror".to_string(),
        base_url: "https://new.aicode.us.com".to_string(),
        management_url: Some("https://new.aicode.us.com/manage".to_string()),
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-ant-example".to_string(),
        default_haiku_model: Some("glm-4.5-air".to_string()),
        default_sonnet_model: Some("glm-5-turbo".to_string()),
        default_opus_model: Some("glm-5.1".to_string()),
        third_party_model: None,
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    }
}

fn sample_third_party_claude_preset() -> StoredClaudePreset {
    StoredClaudePreset {
        id: "claude-2".to_string(),
        name: "Claude third party".to_string(),
        saved_at: 2,
        provider_name: "Claude Mirror".to_string(),
        base_url: "https://new.aicode.us.com".to_string(),
        management_url: Some("https://new.aicode.us.com/manage".to_string()),
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-ant-example".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("glm-5.1".to_string()),
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    }
}

#[test]
fn summary_can_derive_basic_details_from_tokens() {
    let preset = StoredAuthPreset {
        id: "auth-2".to_string(),
        name: "旧预设".to_string(),
        saved_at: 1775363902,
        details: Default::default(),
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth: AuthFile {
            openai_api_key: None,
            last_refresh: "2026-04-05T04:00:00Z".to_string(),
            tokens: AuthTokens {
                access_token: fake_jwt(json!({
                    "https://api.openai.com/profile": {
                        "email": "be02@739964.xyz"
                    },
                    "https://api.openai.com/auth": {
                        "chatgpt_plan_type": "team",
                    }
                })),
                account_id: "100f021d-33a9-46ca-bc96-2d26bcccb2e5".to_string(),
                id_token: fake_jwt(json!({
                    "auth_provider": "password"
                })),
                refresh_token: "refresh".to_string(),
            },
        },
        switch_count: 0,
    };

    let summary = preset_summary(&preset, None);
    assert_eq!(summary.details.email.as_deref(), Some("be02@739964.xyz"));
    assert_eq!(summary.details.plan_type.as_deref(), Some("TEAM"));
    assert_eq!(summary.details.login_method.as_deref(), Some("Password"));
}

#[test]
fn refreshed_quota_overrides_plan_and_windows() {
    let auth = AuthFile {
        openai_api_key: None,
        last_refresh: "2026-04-05T04:00:00Z".to_string(),
        tokens: AuthTokens {
            access_token: fake_jwt(json!({
                "https://api.openai.com/profile": {
                    "email": "be02@739964.xyz"
                }
            })),
            account_id: "100f021d-33a9-46ca-bc96-2d26bcccb2e5".to_string(),
            id_token: fake_jwt(json!({
                "auth_provider": "password",
                "https://api.openai.com/auth": {
                    "chatgpt_plan_type": "team",
                }
            })),
            refresh_token: "refresh".to_string(),
        },
    };

    let details = merge_refreshed_auth_preset_details(
        &Default::default(),
        &auth,
        &CodexUsageResponse {
            rate_limit: Some(CodexRateLimit {
                primary_window: Some(CodexUsageWindow {
                    used_percent: Some(35.5),
                    reset_at: Some(1_712_643_600),
                }),
                secondary_window: Some(CodexUsageWindow {
                    used_percent: Some(74.6),
                    reset_at: Some(1_713_248_400),
                }),
            }),
            plan_type: Some("Plus".to_string()),
        },
    );

    assert_eq!(details.email.as_deref(), Some("be02@739964.xyz"));
    assert_eq!(details.plan_type.as_deref(), Some("PLUS"));
    assert_eq!(details.login_method.as_deref(), Some("Password"));
    assert_eq!(details.hourly_percentage, Some(36));
    assert_eq!(details.hourly_reset_time, Some(1_712_643_600));
    assert_eq!(details.weekly_percentage, Some(75));
    assert_eq!(details.weekly_reset_time, Some(1_713_248_400));
}

#[test]
fn account_id_can_be_recovered_from_refreshed_tokens() {
    let auth = AuthFile {
        openai_api_key: None,
        last_refresh: "2026-04-05T04:00:00Z".to_string(),
        tokens: AuthTokens {
            access_token: fake_jwt(json!({
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": "acct-from-access"
                }
            })),
            account_id: String::new(),
            id_token: fake_jwt(json!({
                "https://api.openai.com/auth": {
                    "chatgpt_account_id": "acct-from-id"
                }
            })),
            refresh_token: "refresh".to_string(),
        },
    };

    assert_eq!(extract_account_id_from_auth(&auth).as_deref(), Some("acct-from-id"));
}

#[test]
fn old_api_preset_format_is_backward_compatible() {
    let mut preset: StoredApiPreset = serde_json::from_str(
        r#"{
              "id":"api-1",
              "name":"旧 API",
              "saved_at":1775363902,
              "base_url":"https://api.example.com/v1/",
              "api_key":"sk-example"
            }"#,
    )
    .expect("old api preset format should deserialize");

    normalize_api_preset(&mut preset).expect("old api preset should normalize");

    assert_eq!(preset.provider_name, "api.example.comv1");
    assert_eq!(preset.base_url, "https://api.example.com/v1");
    assert!(preset.management_url.is_none());
    assert!(!preset.apply_upstream_proxy_on_switch);
    assert!(preset.config_overrides.is_empty());
}

#[test]
fn preset_config_override_trims_and_validates_key() {
    let (config_key, config_value) =
        sanitize_preset_config_override(Some(" model ".to_string()), Some(" glm-5.1 ".to_string()))
            .expect("config override should be valid");

    assert_eq!(config_key.as_deref(), Some("model"));
    assert_eq!(config_value.as_deref(), Some("glm-5.1"));

    let error =
        sanitize_preset_config_override(Some("bad key".to_string()), Some("glm-5.1".to_string()))
            .expect_err("invalid config key should fail");
    assert_eq!(error.to_string(), "config 键名每段只能包含字母、数字、_ 或 -。");
}

#[test]
fn api_management_url_can_follow_base_url_flag() {
    let management_url = resolve_api_management_url(None, true, "https://example.com/v1")
        .expect("same-as-base should be accepted");

    assert_eq!(management_url.as_deref(), Some("https://example.com/v1"));
}

#[test]
fn old_claude_preset_format_is_backward_compatible() {
    let mut preset: StoredClaudePreset = serde_json::from_str(
        r#"{
              "id":"claude-1",
              "name":"旧 Claude",
              "saved_at":1775363902,
              "base_url":"https://new.aicode.us.com/",
              "auth_token":"sk-ant-example"
            }"#,
    )
    .expect("old claude preset format should deserialize");

    normalize_claude_preset(&mut preset).expect("old claude preset should normalize");

    assert_eq!(preset.provider_name, "new.aicode.us.com");
    assert_eq!(preset.base_url, "https://new.aicode.us.com");
    assert!(preset.management_url.is_none());
    assert!(preset.default_haiku_model.is_none());
    assert!(preset.default_sonnet_model.is_none());
    assert!(preset.default_opus_model.is_none());
    assert!(preset.third_party_model.is_none());
}

#[test]
fn legacy_small_fast_model_maps_to_default_haiku_model() {
    let mut preset: StoredClaudePreset = serde_json::from_str(
        r#"{
              "id":"claude-2",
              "name":"旧 Claude 模型",
              "saved_at":1775363902,
              "base_url":"https://new.aicode.us.com/",
              "auth_token":"sk-ant-example",
              "small_fast_model":"glm-4.5-air"
            }"#,
    )
    .expect("legacy claude preset should deserialize");

    normalize_claude_preset(&mut preset).expect("legacy claude preset should normalize");

    assert_eq!(preset.default_haiku_model.as_deref(), Some("glm-4.5-air"));
    assert!(preset.default_sonnet_model.is_none());
    assert!(preset.default_opus_model.is_none());
    assert!(preset.third_party_model.is_none());
}

#[test]
fn normalize_claude_preset_keeps_explicit_local_proxy_for_anthropic_compatible_urls() {
    let mut preset: StoredClaudePreset = serde_json::from_str(
        r#"{
              "id":"claude-minimax",
              "name":"MiniMax Claude",
              "saved_at":1775363902,
              "base_url":"https://api.minimaxi.com/anthropic",
              "auth_token":"sk-ant-example",
              "third_party_model":"MiniMax-M2.7",
              "use_local_proxy":true
            }"#,
    )
    .expect("claude preset should deserialize");

    normalize_claude_preset(&mut preset).expect("claude preset should normalize");

    assert_eq!(preset.base_url, "https://api.minimaxi.com/anthropic");
    assert!(preset.use_local_proxy);
    assert!(claude_preset_supports_direct_anthropic_endpoint(&preset));
    assert!(effective_claude_use_local_proxy(&preset));
}

#[test]
fn config_reader_can_extract_provider() {
    let config = r#"
model_provider = "crs"

[model_providers.crs]
name = "crs"
base_url = "https://example.com/v1"
wire_api = "responses"
"#;
    let current = read_current_config_provider_from_content(config)
        .expect("config should parse")
        .expect("provider should exist");
    assert_eq!(current.provider_id, "crs");
    assert_eq!(current.provider_name.as_deref(), Some("crs"));
    assert_eq!(current.base_url.as_deref(), Some("https://example.com/v1"));
    assert_eq!(current.wire_api.as_deref(), Some("responses"));
}

#[test]
fn clear_provider_only_removes_selector() {
    let config = r#"
model_provider = "crs"
model = "gpt-5.4"

[model_providers.crs]
name = "crs"
base_url = "https://example.com/v1"
"#;
    let next = clear_provider_in_config_content(config).expect("config should update");
    let current = read_current_config_provider_from_content(&next).expect("config should parse");
    assert!(current.is_none());
    assert!(next.contains("base_url = \"https://example.com/v1\""));
    assert!(next.contains("model = \"gpt-5.4\""));
}

#[test]
fn api_provider_writer_sets_fixed_provider() {
    let next = set_api_provider_in_config_content(
        "",
        "Example API",
        "https://api.example.com/v1",
        &default_api_provider_options(),
    )
    .expect("config should update");
    let current = read_current_config_provider_from_content(&next)
        .expect("config should parse")
        .expect("provider should exist");
    assert_eq!(current.provider_id, "webclx_api");
    assert_eq!(current.provider_name.as_deref(), Some("Example API"));
    assert_eq!(current.base_url.as_deref(), Some("https://api.example.com/v1"));
    assert_eq!(current.wire_api.as_deref(), Some("responses"));
}

#[test]
fn local_proxy_provider_uses_environment_backed_local_token_header() {
    let local = set_local_proxy_auth_header_in_config_content(
        r#"
model_provider = "webclx_api"

[model_providers.webclx_api]
name = "DeepSeek"
base_url = "http://127.0.0.1:11111/api/upstream/openai/v1"
wire_api = "responses"
"#,
        true,
    )
    .expect("local proxy auth header should be configured");

    assert!(local.contains("[model_providers.webclx_api.env_http_headers]"));
    assert!(local.contains("X-WebClx-Local-Token = \"WEBCLX_LOCAL_API_TOKEN\""));

    let direct = set_local_proxy_auth_header_in_config_content(&local, false)
        .expect("direct provider should remove local proxy auth header");
    assert!(!direct.contains("X-WebClx-Local-Token"));
    assert!(!direct.contains("WEBCLX_LOCAL_API_TOKEN"));
}

#[test]
fn auth_preset_config_writer_sets_expected_model() {
    let next = clear_provider_and_set_model_in_config_content(
        r#"
model_provider = "crs"
model = "gpt-5.4"

[model_providers.crs]
name = "crs"
base_url = "https://example.com/v1"
"#,
        "glm-5.1",
    )
    .expect("config should update");

    let current = read_current_config_provider_from_content(&next).expect("config should parse");
    assert!(current.is_none());
    assert!(next.contains("model = \"glm-5.1\""));
    assert!(next.contains("base_url = \"https://example.com/v1\""));
}

#[test]
fn auth_preset_config_writer_sets_custom_key_and_value() {
    let next = clear_provider_and_set_config_entry_in_config_content(
        r#"
model_provider = "crs"
model = "gpt-5.4"
"#,
        "model_reasoning_effort",
        "high",
    )
    .expect("config should update");

    let current = read_current_config_provider_from_content(&next).expect("config should parse");
    assert!(current.is_none());
    assert!(next.contains("model_reasoning_effort = \"high\""));
}

#[test]
fn auth_preset_config_writer_preserves_multiple_config_entries() {
    let next = clear_provider_and_set_config_entry_in_config_content("", "model", "gpt-5.4")
        .and_then(|updated| {
            clear_provider_and_set_config_entry_in_config_content(
                &updated,
                "model_reasoning_effort",
                "xhigh",
            )
        })
        .expect("config should update");

    assert!(next.contains("model = \"gpt-5.4\""));
    assert!(next.contains("model_reasoning_effort = \"xhigh\""));
}

#[test]
fn api_preset_config_cleanup_removes_only_inactive_managed_entries() {
    let next = clear_inactive_managed_config_entries_in_content(
        r#"
model = "gpt-5.6-sol"
model_context_window = 1000000
user_custom_setting = "keep"

[features]
goals = true
custom = "keep"
"#,
        &[
            "model".to_string(),
            "model_context_window".to_string(),
            "features.goals".to_string(),
        ],
        &["model".to_string(), "features.goals".to_string()],
    )
    .expect("managed config should clean up");

    assert!(next.contains("model = \"gpt-5.6-sol\""));
    assert!(!next.contains("model_context_window"));
    assert!(next.contains("user_custom_setting = \"keep\""));
    assert!(next.contains("goals = true"));
    assert!(next.contains("custom = \"keep\""));
}

#[test]
fn codex_snapshot_project_merge_persists_only_snapshot_project_changes() {
    let baseline = r#"
model = "gpt-5.4"
model_provider = "webclx_api"

[projects."/home/codes/existing"]
trust_level = "trusted"

[model_providers.webclx_api]
base_url = "https://old.example/v1"
wire_api = "responses"
"#;
    let snapshot = r#"
model = "snapshot-model"
model_provider = "snapshot-provider"

[projects."/home/codes/existing"]
trust_level = "trusted"

[projects."/home/codes/newly-trusted"]
trust_level = "trusted"

[model_providers.snapshot-provider]
base_url = "https://snapshot.example/v1"
wire_api = "responses"
"#;
    let shared = r#"
model = "current-model"
model_provider = "webclx_api"

[projects."/home/codes/existing"]
trust_level = "trusted"

[projects."/home/codes/concurrent"]
trust_level = "trusted"

[model_providers.webclx_api]
base_url = "https://current.example/v1"
wire_api = "responses"
"#;

    let merged = merge_codex_snapshot_projects_in_config_content(baseline, snapshot, shared)
        .expect("snapshot project trust should merge");

    assert!(merged.contains("model = \"current-model\""), "{merged}");
    assert!(merged.contains("base_url = \"https://current.example/v1\""), "{merged}");
    assert!(merged.contains("[projects.\"/home/codes/existing\"]"), "{merged}");
    assert!(merged.contains("[projects.\"/home/codes/concurrent\"]"), "{merged}");
    assert!(merged.contains("[projects.\"/home/codes/newly-trusted\"]"), "{merged}");
    assert!(!merged.contains("snapshot-model"), "{merged}");
    assert!(!merged.contains("snapshot-provider"), "{merged}");
}

/// Regression: switching to a preset that does NOT declare a `model` (no
/// config_overrides and an empty default model value) must clear the stale
/// `model` left by the previous preset, so the new provider is not paired with
/// an unrelated model. `model` stays in managed_keys (it is a default entry)
/// but is absent from active_keys, so cleanup removes it.
#[test]
fn applying_preset_without_model_clears_stale_model() {
    let next = clear_inactive_managed_config_entries_in_content(
        r#"
model = "gpt-5.6-sol"
model_reasoning_effort = "xhigh"
"#,
        &["model".to_string(), "model_reasoning_effort".to_string()],
        &["model_reasoning_effort".to_string()],
    )
    .expect("cleanup should drop inactive model");

    assert!(
        !next.contains("model ="),
        "stale model must be removed when the active preset does not declare it: {next}",
    );
    assert!(next.contains("model_reasoning_effort = \"xhigh\""));
}

#[test]
fn api_model_catalog_updates_existing_entry_from_context_window_target() {
    let mut catalog = json!({
        "models": [{
            "slug": "gpt-5.6-sol",
            "context_window": 256000,
            "max_context_window": 256000
        }]
    });
    assert!(
        upsert_model_catalog_entry_in_value(&mut catalog, "gpt-5.6-sol", Some(1_000_000))
            .expect("model catalog should update")
    );
    let model = catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["slug"] == "gpt-5.6-sol")
        .unwrap();
    assert_eq!(model["context_window"], 1_000_000);
    assert_eq!(model["max_context_window"], 1_000_000);
    assert_eq!(model["auto_compact_token_limit"], 800_000);
}

#[test]
fn api_model_catalog_initializes_from_bundled_catalog_when_config_has_no_catalog() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");
    runtime.block_on(async {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let codex_dir =
            std::env::temp_dir().join(format!("webclx-model-catalog-init-{unique}/.codex"));
        fs::create_dir_all(&codex_dir).expect("codex dir should be created");
        let config_path = codex_dir.join("config.toml");
        let catalog_path = codex_dir.join("model_catalog.json");
        fs::write(&config_path, "model = \"GLM-5.2\"\n").expect("config fixture should be written");
        fs::write(
            &catalog_path,
            serde_json::to_vec_pretty(&json!({
                "models": [{
                    "slug": "private-model",
                    "display_name": "Private Model",
                    "context_window": 128000,
                    "max_context_window": 128000
                }]
            }))
            .expect("catalog fixture should encode"),
        )
        .expect("catalog fixture should be written");
        let bundled_catalog = json!({
            "models": [{
                "slug": "gpt-next-bundled",
                "display_name": "GPT Next Bundled",
                "context_window": 256000,
                "max_context_window": 256000
            }]
        });
        let targets = vec![ResolvedConfigTarget {
            key: "model".to_string(),
            value: "GLM-5.2".to_string(),
        }];

        sync_api_model_catalog(&config_path, &targets, Some(&bundled_catalog))
            .await
            .expect("missing catalog config should initialize from bundled models");

        let config = fs::read_to_string(&config_path).expect("config should remain readable");
        let catalog: Value = serde_json::from_slice(
            &fs::read(&catalog_path).expect("initialized catalog should be readable"),
        )
        .expect("initialized catalog should be valid JSON");
        let slugs = catalog["models"]
            .as_array()
            .expect("catalog should contain models")
            .iter()
            .filter_map(|entry| entry["slug"].as_str())
            .collect::<Vec<_>>();
        fs::remove_dir_all(codex_dir.parent().unwrap()).ok();

        assert!(config.contains("model_catalog_json = \"model_catalog.json\""));
        assert!(slugs.contains(&"gpt-next-bundled"));
        assert!(slugs.contains(&"private-model"));
        assert!(slugs.contains(&"GLM-5.2"));
    });
}

#[test]
fn api_model_catalog_refreshes_bundled_models_and_preserves_custom_path_and_entries() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");
    runtime.block_on(async {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let codex_dir =
            std::env::temp_dir().join(format!("webclx-model-catalog-refresh-{unique}/.codex"));
        let catalog_path = codex_dir.join("catalogs/custom-models.json");
        fs::create_dir_all(catalog_path.parent().unwrap()).expect("catalog dir should be created");
        let config_path = codex_dir.join("config.toml");
        fs::write(
            &config_path,
            "model = \"GLM-5.2\"\nmodel_catalog_json = \"catalogs/custom-models.json\"\n",
        )
        .expect("config fixture should be written");
        fs::write(
            &catalog_path,
            serde_json::to_vec_pretty(&json!({
                "models": [
                    {"slug": "old-bundled-model", "context_window": 128000},
                    {"slug": "private-model", "context_window": 64000}
                ]
            }))
            .expect("catalog fixture should encode"),
        )
        .expect("catalog fixture should be written");
        let bundled_catalog = json!({
            "models": [{
                "slug": "new-bundled-model",
                "display_name": "New Bundled Model",
                "context_window": 256000,
                "max_context_window": 256000
            }]
        });
        let targets = vec![ResolvedConfigTarget {
            key: "model".to_string(),
            value: "GLM-5.2".to_string(),
        }];

        sync_api_model_catalog(&config_path, &targets, Some(&bundled_catalog))
            .await
            .expect("configured catalog should refresh from bundled models");

        let config = fs::read_to_string(&config_path).expect("config should remain readable");
        let catalog: Value = serde_json::from_slice(
            &fs::read(&catalog_path).expect("refreshed catalog should be readable"),
        )
        .expect("refreshed catalog should be valid JSON");
        let slugs = catalog["models"]
            .as_array()
            .expect("catalog should contain models")
            .iter()
            .filter_map(|entry| entry["slug"].as_str())
            .collect::<Vec<_>>();
        fs::remove_dir_all(codex_dir.parent().unwrap()).ok();

        assert!(config.contains("model_catalog_json = \"catalogs/custom-models.json\""));
        assert!(slugs.contains(&"new-bundled-model"));
        assert!(slugs.contains(&"private-model"));
        assert!(slugs.contains(&"GLM-5.2"));
    });
}

#[test]
fn api_model_catalog_backfills_required_reasoning_summary_capability() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");
    runtime.block_on(async {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let codex_dir = std::env::temp_dir().join(format!(
            "webclx-model-catalog-reasoning-summary-{unique}/.codex"
        ));
        fs::create_dir_all(&codex_dir).expect("codex dir should be created");
        let config_path = codex_dir.join("config.toml");
        let catalog_path = codex_dir.join("model_catalog.json");
        fs::write(
            &config_path,
            "model = \"gpt-next-bundled\"\nmodel_catalog_json = \"model_catalog.json\"\n",
        )
        .expect("config fixture should be written");
        let bundled_catalog = json!({
            "models": [
                {
                    "slug": "gpt-next-bundled",
                    "display_name": "GPT Next Bundled",
                    "supported_reasoning_levels": [{"effort": "high", "description": "Deep reasoning"}],
                    "default_reasoning_summary": "none"
                },
                {
                    "slug": "plain-model",
                    "display_name": "Plain Model",
                    "supported_reasoning_levels": []
                }
            ]
        });
        let targets = vec![ResolvedConfigTarget {
            key: "model".to_string(),
            value: "gpt-next-bundled".to_string(),
        }];

        sync_api_model_catalog(&config_path, &targets, Some(&bundled_catalog))
            .await
            .expect("catalog sync should accept Codex bundled output");

        let catalog: Value = serde_json::from_slice(
            &fs::read(&catalog_path).expect("synced catalog should be readable"),
        )
        .expect("synced catalog should be valid JSON");
        let model = catalog["models"]
            .as_array()
            .expect("catalog should contain models")
            .iter()
            .find(|entry| entry["slug"] == "gpt-next-bundled")
            .expect("bundled model should be present");
        let plain_model = catalog["models"]
            .as_array()
            .expect("catalog should contain models")
            .iter()
            .find(|entry| entry["slug"] == "plain-model")
            .expect("plain model should be present");
        fs::remove_dir_all(codex_dir.parent().unwrap()).ok();

        assert_eq!(model["supports_reasoning_summaries"], true);
        assert_eq!(plain_model["supports_reasoning_summaries"], false);
    });
}

#[test]
fn api_model_catalog_upsert_deduplicates_model_slugs_case_insensitively() {
    let mut catalog = json!({
        "models": [
            {"slug": "glm-5.2", "display_name": "glm-5.2", "context_window": 128000},
            {"slug": "GLM-5.2", "display_name": "GLM-5.2", "context_window": 256000}
        ]
    });

    assert!(
        upsert_model_catalog_entry_in_value(&mut catalog, "GLM-5.2", None)
            .expect("model catalog should deduplicate")
    );

    let matching = catalog["models"]
        .as_array()
        .expect("catalog should contain models")
        .iter()
        .filter(|entry| {
            entry["slug"]
                .as_str()
                .is_some_and(|slug| slug.eq_ignore_ascii_case("GLM-5.2"))
        })
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0]["slug"], "GLM-5.2");
}

#[test]
fn api_model_catalog_custom_entry_clears_inherited_official_upgrade() {
    let mut catalog = json!({
        "models": [
            {
                "slug": "gpt-5.4",
                "display_name": "GPT-5.4",
                "description": "Official model",
                "upgrade": {"model": "gpt-5.6-terra"}
            },
            {
                "slug": "GLM-5.2",
                "display_name": "GLM-5.2",
                "description": "Custom API model routed through WebClx.",
                "upgrade": {"model": "gpt-5.6-terra"}
            }
        ]
    });

    assert!(
        upsert_model_catalog_entry_in_value(&mut catalog, "GLM-5.2", None)
            .expect("custom model catalog entry should be refreshed")
    );

    let models = catalog["models"]
        .as_array()
        .expect("catalog should contain models");
    let official = models
        .iter()
        .find(|entry| entry["slug"] == "gpt-5.4")
        .expect("official model should remain");
    let custom = models
        .iter()
        .find(|entry| entry["slug"] == "GLM-5.2")
        .expect("custom model should remain");
    assert_eq!(official["upgrade"]["model"], "gpt-5.6-terra");
    assert_eq!(custom["upgrade"], Value::Null);
}

#[test]
fn api_model_catalog_new_custom_entry_drops_template_upgrade() {
    let mut catalog = json!({
        "models": [{
            "slug": "gpt-5.4",
            "display_name": "GPT-5.4",
            "description": "Official model",
            "upgrade": {"model": "gpt-5.6-terra"}
        }]
    });

    assert!(
        upsert_model_catalog_entry_in_value(&mut catalog, "GLM-5.2", Some(1_000_000))
            .expect("custom model catalog entry should be created")
    );

    let custom = catalog["models"]
        .as_array()
        .expect("catalog should contain models")
        .iter()
        .find(|entry| entry["slug"] == "GLM-5.2")
        .expect("custom model should be created");
    assert_eq!(custom["upgrade"], Value::Null);
}

#[test]
fn api_model_catalog_initialization_requires_bundled_catalog() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");
    runtime.block_on(async {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let codex_dir =
            std::env::temp_dir().join(format!("webclx-model-catalog-required-{unique}/.codex"));
        fs::create_dir_all(&codex_dir).expect("codex dir should be created");
        let config_path = codex_dir.join("config.toml");
        fs::write(&config_path, "model = \"GLM-5.2\"\n").expect("config fixture should be written");
        let targets = vec![ResolvedConfigTarget {
            key: "model".to_string(),
            value: "GLM-5.2".to_string(),
        }];

        let error = sync_api_model_catalog(&config_path, &targets, None)
            .await
            .expect_err("initialization without bundled models should fail");
        let config = fs::read_to_string(&config_path).expect("config should remain readable");
        fs::remove_dir_all(codex_dir.parent().unwrap()).ok();

        assert!(error.to_string().contains("bundled"));
        assert!(!config.contains("model_catalog_json"));
    });
}

#[test]
fn blank_default_model_config_target_is_skipped() {
    let targets = resolve_effective_preset_config_targets(
        &[("model", ""), ("model_reasoning_effort", "xhigh")],
        &[],
    )
    .expect("blank model default should be skipped");

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].key, "model_reasoning_effort");
    assert_eq!(targets[0].value, "xhigh");
}

#[test]
fn preset_model_override_can_use_default_model_key() {
    let targets = resolve_effective_preset_config_targets(
        &[("model", ""), ("model_reasoning_effort", "xhigh")],
        &[PresetConfigOverride {
            key: None,
            value: Some("glm-5.1".to_string()),
        }],
    )
    .expect("preset model value should use the first default key");

    assert!(
        targets
            .iter()
            .any(|target| { target.key == "model_reasoning_effort" && target.value == "xhigh" })
    );
    assert!(
        targets
            .iter()
            .any(|target| target.key == "model" && target.value == "glm-5.1")
    );
}

#[test]
fn preset_config_overrides_accept_dotted_second_level_keys() {
    let overrides = effective_preset_config_overrides(
        vec![PresetConfigOverride {
            key: Some("features.goals".to_string()),
            value: Some("true".to_string()),
        }],
        None,
        None,
        None,
        None,
    )
    .expect("dotted config key should be accepted");

    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].key.as_deref(), Some("features.goals"));
    assert_eq!(overrides[0].value.as_deref(), Some("true"));
}

#[test]
fn config_targets_keep_default_secondary_when_preset_lacks_it() {
    let targets = resolve_effective_preset_config_targets(
        &[("model", "gpt-5.4"), ("model_reasoning_effort", "high")],
        &[PresetConfigOverride {
            key: Some("model".to_string()),
            value: Some("glm-5.1".to_string()),
        }],
    )
    .expect("config targets should resolve");

    assert_eq!(
        targets,
        vec![
            ResolvedConfigTarget {
                key: "model".to_string(),
                value: "glm-5.1".to_string(),
            },
            ResolvedConfigTarget {
                key: "model_reasoning_effort".to_string(),
                value: "high".to_string(),
            },
        ]
    );
}

#[test]
fn config_targets_write_defaults_when_preset_has_no_overrides() {
    let targets = resolve_effective_preset_config_targets(
        &[("model", "gpt-5.4"), ("model_reasoning_effort", "high")],
        &[],
    )
    .expect("config targets should resolve");

    assert!(
        targets
            .iter()
            .any(|target| target.key == "model" && target.value == "gpt-5.4")
    );
    assert!(
        targets
            .iter()
            .any(|target| { target.key == "model_reasoning_effort" && target.value == "high" })
    );
}

#[test]
fn config_targets_include_added_default_rows() {
    let targets = resolve_effective_preset_config_targets(
        &[
            ("model", "gpt-5.4"),
            ("model_reasoning_effort", "high"),
            ("features.goals", "true"),
        ],
        &[],
    )
    .expect("config targets should resolve");

    assert!(
        targets
            .iter()
            .any(|target| target.key == "features.goals" && target.value == "true")
    );
}

#[test]
fn config_targets_auto_inject_compact_limit_from_context_window() {
    let targets =
        resolve_effective_preset_config_targets(&[("model_context_window", "1000000")], &[])
            .expect("config targets should resolve");

    assert!(
        targets
            .iter()
            .any(|target| { target.key == "model_context_window" && target.value == "1000000" })
    );
    assert!(targets.iter().any(|target| {
        target.key == "model_auto_compact_token_limit" && target.value == "800000"
    }));
}

#[test]
fn config_targets_respect_explicit_compact_limit() {
    let targets = resolve_effective_preset_config_targets(
        &[("model_context_window", "1000000")],
        &[PresetConfigOverride {
            key: Some("model_auto_compact_token_limit".to_string()),
            value: Some("600000".to_string()),
        }],
    )
    .expect("config targets should resolve");

    let compact = targets
        .iter()
        .find(|target| target.key == "model_auto_compact_token_limit")
        .unwrap();
    assert_eq!(compact.value, "600000");
}

#[test]
fn config_targets_skip_compact_limit_without_context_window() {
    let targets = resolve_effective_preset_config_targets(&[("model", "gpt-5.4")], &[])
        .expect("config targets should resolve");

    assert!(
        targets
            .iter()
            .all(|target| target.key != "model_auto_compact_token_limit")
    );
}

#[test]
fn api_preset_config_writer_sets_provider_and_expected_model() {
    let next = set_api_provider_and_model_in_config_content(
        r#"
model = "gpt-5.4"
"#,
        "Example API",
        "https://api.example.com/v1",
        "glm-5.1",
    )
    .expect("config should update");

    let current = read_current_config_provider_from_content(&next)
        .expect("config should parse")
        .expect("provider should exist");
    assert_eq!(current.provider_id, "webclx_api");
    assert_eq!(current.provider_name.as_deref(), Some("Example API"));
    assert_eq!(current.base_url.as_deref(), Some("https://api.example.com/v1"));
    assert_eq!(current.wire_api.as_deref(), Some("responses"));
    assert!(next.contains("model = \"glm-5.1\""));
}

#[test]
fn api_preset_config_writer_sets_provider_and_custom_key() {
    let next = set_api_provider_and_config_entry_in_config_content(
        r#"
model = "gpt-5.4"
"#,
        "Example API",
        "https://api.example.com/v1",
        &default_api_provider_options(),
        "model_reasoning_effort",
        "high",
    )
    .expect("config should update");

    let current = read_current_config_provider_from_content(&next)
        .expect("config should parse")
        .expect("provider should exist");
    assert_eq!(current.provider_id, "webclx_api");
    assert!(next.contains("model_reasoning_effort = \"high\""));
}

#[test]
fn api_preset_config_writer_supports_dotted_table_key_with_toml_value() {
    let next = set_api_provider_and_config_entry_in_config_content(
        "",
        "Example API",
        "https://api.example.com/v1",
        &default_api_provider_options(),
        "features.goals",
        "true",
    )
    .expect("config should update");

    let current = read_current_config_provider_from_content(&next)
        .expect("config should parse")
        .expect("provider should exist");
    assert_eq!(current.provider_id, "webclx_api");
    assert!(next.contains("[features]"));
    assert!(next.contains("goals = true"));
}

#[test]
fn api_preset_config_writer_preserves_multiple_config_entries() {
    let next = set_api_provider_and_config_entry_in_config_content(
        "",
        "Example API",
        "https://api.example.com/v1",
        &default_api_provider_options(),
        "model",
        "gpt-5.4",
    )
    .and_then(|updated| {
        set_api_provider_and_config_entry_in_config_content(
            &updated,
            "Example API",
            "https://api.example.com/v1",
            &default_api_provider_options(),
            "model_reasoning_effort",
            "xhigh",
        )
    })
    .expect("config should update");

    assert!(next.contains("model = \"gpt-5.4\""));
    assert!(next.contains("model_reasoning_effort = \"xhigh\""));
    assert!(next.contains("model_provider = \"webclx_api\""));
}

#[test]
fn api_preset_switches_keep_fixed_provider_id() {
    let first = set_api_provider_and_config_entry_in_config_content(
        "",
        "First API",
        "https://first.example.com/v1",
        &default_api_provider_options(),
        "model",
        "gpt-5.4",
    )
    .expect("first config should update");

    let second = set_api_provider_and_config_entry_in_config_content(
        &first,
        "Second API",
        "https://second.example.com/v1",
        &default_api_provider_options(),
        "model",
        "glm-5.1",
    )
    .expect("second config should update");

    let current = read_current_config_provider_from_content(&second)
        .expect("config should parse")
        .expect("provider should exist");
    assert_eq!(current.provider_id, "webclx_api");
    assert_eq!(current.provider_name.as_deref(), Some("Second API"));
    assert_eq!(current.base_url.as_deref(), Some("https://second.example.com/v1"));
    assert!(second.contains("model_provider = \"webclx_api\""));
    assert!(!second.contains("model_provider = \"openai\""));
}

#[test]
fn current_mode_prefers_api_when_provider_exists() {
    let auth_state = CurrentAuthState::Login(sample_auth());
    let provider = super::ConfigProviderState {
        provider_id: "crs".to_string(),
        provider_name: Some("crs".to_string()),
        base_url: Some("https://example.com/v1".to_string()),
        wire_api: Some("responses".to_string()),
        config_values: BTreeMap::new(),
    };

    assert_eq!(derive_current_mode(Some(&auth_state), Some(&provider)), CurrentAuthMode::Api);
}

#[test]
fn api_preset_can_be_marked_active() {
    let preset = StoredApiPreset {
        id: "api-1".to_string(),
        name: "Example".to_string(),
        saved_at: 1,
        provider_name: "Example API".to_string(),
        base_url: "https://example.com/v1".to_string(),
        management_url: Some("https://example.com/keys".to_string()),
        wire_api: None,
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "sk-example".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let auth_state = CurrentAuthState::Api(super::ApiAuthFile {
        openai_api_key: "sk-example".to_string(),
    });
    let provider = super::ConfigProviderState {
        provider_id: "webclx_api".to_string(),
        provider_name: Some("Example API".to_string()),
        base_url: Some("https://example.com/v1".to_string()),
        wire_api: Some("responses".to_string()),
        config_values: BTreeMap::new(),
    };
    let current_api =
        derive_current_api_state(Some(&provider), Some(&auth_state), &[preset.clone()]);

    assert_eq!(
        current_api
            .as_ref()
            .and_then(|item| item.preset_id.as_deref()),
        Some(preset.id.as_str())
    );
    let summary = api_preset_summary(&preset, CurrentAuthMode::Api, current_api.as_ref());
    assert!(summary.active);
    assert_eq!(
        current_api
            .as_ref()
            .and_then(|item| item.management_url.as_deref()),
        Some("https://example.com/keys")
    );
}

#[test]
fn api_preset_summary_is_active_from_proxy_state_when_proxy_enabled() {
    let preset = sample_api_preset("https://api.example.com/v1", Some("gpt-5.4"));
    let settings = UpstreamProxySettings {
        codex_api_proxy_enabled: true,
        active_api_proxy_preset_id: Some(preset.id.clone()),
        ..Default::default()
    };

    let summary =
        api_preset_summary_with_proxy_state(&preset, CurrentAuthMode::None, None, &settings);

    assert!(summary.active);
}

#[test]
fn api_preset_summary_is_active_from_preset_proxy_option_without_global_toggle() {
    let mut preset = sample_api_preset("https://api.example.com/v1", Some("gpt-5.4"));
    preset.apply_upstream_proxy_on_switch = true;
    let settings = UpstreamProxySettings {
        codex_api_proxy_enabled: false,
        active_api_proxy_preset_id: Some(preset.id.clone()),
        ..Default::default()
    };

    let summary =
        api_preset_summary_with_proxy_state(&preset, CurrentAuthMode::None, None, &settings);

    assert!(summary.active);
}

#[test]
fn special_api_presets_do_not_replace_saved_proxy_option_in_summary() {
    let preset = sample_api_preset("https://api.deepseek.com/v1", Some("deepseek-v4-pro"));

    let summary = api_preset_summary(&preset, CurrentAuthMode::None, None);

    assert!(api_preset_prefers_local_upstream_proxy(&preset));
    assert!(!summary.apply_upstream_proxy_on_switch);
}

#[test]
fn api_preset_summary_prefers_current_config_over_saved_active_id() {
    let first_preset = sample_api_preset("https://api.example.com/v1", None);
    let mut second_preset = first_preset.clone();
    second_preset.id = "api-2".to_string();
    second_preset.name = "Same endpoint second preset".to_string();

    let auth_state = CurrentAuthState::Api(super::ApiAuthFile {
        openai_api_key: first_preset.api_key.clone(),
    });
    let provider = super::ConfigProviderState {
        provider_id: "webclx_api".to_string(),
        provider_name: Some(first_preset.provider_name.clone()),
        base_url: Some(first_preset.base_url.clone()),
        wire_api: Some("responses".to_string()),
        config_values: BTreeMap::new(),
    };
    let presets = vec![first_preset.clone(), second_preset.clone()];
    let current_api = derive_current_api_state(Some(&provider), Some(&auth_state), &presets);
    let settings = UpstreamProxySettings {
        active_api_proxy_preset_id: Some(second_preset.id.clone()),
        ..Default::default()
    };

    let first_summary = api_preset_summary_with_proxy_state(
        &first_preset,
        CurrentAuthMode::Api,
        current_api.as_ref(),
        &settings,
    );
    let second_summary = api_preset_summary_with_proxy_state(
        &second_preset,
        CurrentAuthMode::Api,
        current_api.as_ref(),
        &settings,
    );

    assert!(first_summary.active);
    assert!(!second_summary.active);
}

#[test]
fn local_proxy_api_key_encodes_api_preset_id() {
    let key = local_proxy_api_key_for_preset_id("api-proxy");

    assert_eq!(local_proxy_api_preset_id_from_api_key(&key), Some("api-proxy"));
    assert_eq!(local_proxy_api_preset_id_from_api_key("real-key"), None);
}

#[test]
fn local_proxy_api_config_marks_current_preset_without_saved_active_state() {
    let mut proxy_preset = sample_api_preset("https://proxy.example.com/v1", Some("gpt-5.4"));
    proxy_preset.id = "api-proxy".to_string();
    proxy_preset.apply_upstream_proxy_on_switch = true;
    let mut direct_preset = sample_api_preset("https://direct.example.com/v1", Some("gpt-5.5"));
    direct_preset.id = "api-direct".to_string();
    let auth_state = CurrentAuthState::Api(super::ApiAuthFile {
        openai_api_key: local_proxy_api_key_for_preset_id(&proxy_preset.id),
    });
    let provider = super::ConfigProviderState {
        provider_id: "webclx_api".to_string(),
        provider_name: Some(proxy_preset.provider_name.clone()),
        base_url: Some(api_provider_base_url_for_mode(&proxy_preset, true)),
        wire_api: Some("responses".to_string()),
        config_values: BTreeMap::new(),
    };
    let presets = vec![proxy_preset.clone(), direct_preset.clone()];
    let current_api = derive_current_api_state(Some(&provider), Some(&auth_state), &presets);
    let settings = UpstreamProxySettings::default();

    let proxy_summary = api_preset_summary_with_proxy_state(
        &proxy_preset,
        CurrentAuthMode::Api,
        current_api.as_ref(),
        &settings,
    );
    let direct_summary = api_preset_summary_with_proxy_state(
        &direct_preset,
        CurrentAuthMode::Api,
        current_api.as_ref(),
        &settings,
    );

    assert!(proxy_summary.active);
    assert!(!direct_summary.active);
}

#[test]
fn api_preset_summary_marks_only_current_name_when_no_saved_active_id_exists() {
    let first_preset = sample_api_preset("https://api.example.com/v1", None);
    let mut second_preset = first_preset.clone();
    second_preset.id = "api-2".to_string();
    second_preset.name = "Same endpoint second preset".to_string();

    let auth_state = CurrentAuthState::Api(super::ApiAuthFile {
        openai_api_key: first_preset.api_key.clone(),
    });
    let provider = super::ConfigProviderState {
        provider_id: "webclx_api".to_string(),
        provider_name: Some(first_preset.provider_name.clone()),
        base_url: Some(first_preset.base_url.clone()),
        wire_api: Some("responses".to_string()),
        config_values: BTreeMap::new(),
    };
    let presets = vec![first_preset.clone(), second_preset.clone()];
    let current_api = derive_current_api_state(Some(&provider), Some(&auth_state), &presets);

    let first_summary =
        api_preset_summary(&first_preset, CurrentAuthMode::Api, current_api.as_ref());
    let second_summary =
        api_preset_summary(&second_preset, CurrentAuthMode::Api, current_api.as_ref());

    assert_eq!(
        current_api
            .as_ref()
            .and_then(|item| item.preset_name.as_deref()),
        Some(first_preset.name.as_str())
    );
    assert!(first_summary.active);
    assert!(!second_summary.active);
}

#[test]
fn api_preset_summary_includes_terminal_startup_fields() {
    let mut preset = sample_api_preset("https://example.com/v1", None);
    preset.terminal_env = vec![PresetTerminalEnvVar {
        key: "CODEX_RESPONSE_STYLE".to_string(),
        value: "caveman".to_string(),
    }];
    preset.terminal_startup_script = Some("echo ready".to_string());

    let summary = api_preset_summary(&preset, CurrentAuthMode::None, None);

    assert_eq!(summary.terminal_env, preset.terminal_env);
    assert_eq!(summary.terminal_startup_script.as_deref(), Some("echo ready"));
}

#[test]
fn normalize_api_preset_rejects_terminal_startup_scripts() {
    let mut preset: StoredApiPreset = serde_json::from_value(json!({
        "id": "api-1",
        "name": "Example",
        "saved_at": 1,
        "provider_name": "Example API",
        "base_url": " https://example.com/v1 ",
        "api_key": "sk-example",
        "terminal_env": [
            { "key": " CODEX_RESPONSE_STYLE ", "value": " caveman " },
            { "key": "HOME", "value": "/tmp/alternate-home" },
            { "key": "CODEX_HOME", "value": "/tmp/alternate-codex" },
            { "key": "CLAUDE_CONFIG_DIR", "value": "/tmp/alternate-claude" },
            { "key": "WEBCLX_USER_HOME", "value": "/tmp/alternate-user" },
            { "key": "bad-key", "value": "ignored" },
            { "key": "EMPTY_VALUE", "value": "\u{0000}ok" }
        ],
        "terminal_startup_script": "echo before\u{0000}\necho after"
    }))
    .expect("preset should deserialize");

    normalize_api_preset(&mut preset).expect("preset should normalize");

    assert_eq!(
        preset.terminal_env,
        vec![
            PresetTerminalEnvVar {
                key: "CODEX_RESPONSE_STYLE".to_string(),
                value: "caveman".to_string(),
            },
            PresetTerminalEnvVar {
                key: "EMPTY_VALUE".to_string(),
                value: "ok".to_string(),
            },
        ]
    );
    assert_eq!(preset.terminal_startup_script, None);
}

#[test]
fn normalize_api_preset_backfills_auto_compact_from_context_window() {
    let mut preset = sample_api_preset("https://api.example.com/v1", None);
    preset.config_overrides.push(PresetConfigOverride {
        key: Some("model_context_window".to_string()),
        value: Some("1000000".to_string()),
    });
    normalize_api_preset(&mut preset).expect("preset should normalize");
    let compact = preset
        .config_overrides
        .iter()
        .find(|item| item.key.as_deref() == Some("model_auto_compact_token_limit"))
        .expect("auto_compact_token_limit should be backfilled");
    assert_eq!(compact.value.as_deref(), Some("800000"));
}

#[test]
fn normalize_api_preset_keeps_explicit_auto_compact() {
    let mut preset = sample_api_preset("https://api.example.com/v1", None);
    preset.config_overrides.push(PresetConfigOverride {
        key: Some("model_context_window".to_string()),
        value: Some("1000000".to_string()),
    });
    preset.config_overrides.push(PresetConfigOverride {
        key: Some("model_auto_compact_token_limit".to_string()),
        value: Some("500000".to_string()),
    });
    normalize_api_preset(&mut preset).expect("preset should normalize");
    let compact = preset
        .config_overrides
        .iter()
        .find(|item| item.key.as_deref() == Some("model_auto_compact_token_limit"))
        .expect("auto_compact_token_limit should exist");
    assert_eq!(compact.value.as_deref(), Some("500000"));
}

#[test]
fn api_preset_distinguished_by_config_overrides_when_credentials_match() {
    // 两个预设共享同一组 base_url+api_key+wire_api，只有 config_overrides 不同，
    // 模拟 "sub2api" 与 "sub2api gpt-5.5 1M" 的情况。
    let mut first_preset = sample_api_preset("https://api.example.com/v1", None);
    first_preset.id = "api-base".to_string();
    first_preset.name = "sub2api".to_string();
    first_preset.config_overrides = vec![
        PresetConfigOverride {
            key: Some("model".to_string()),
            value: Some("gpt-5.5".to_string()),
        },
        PresetConfigOverride {
            key: Some("model_reasoning_effort".to_string()),
            value: Some("high".to_string()),
        },
    ];

    let mut second_preset = first_preset.clone();
    second_preset.id = "api-1m".to_string();
    second_preset.name = "sub2api gpt-5.5 1M".to_string();
    second_preset.config_overrides.push(PresetConfigOverride {
        key: Some("model_context_window".to_string()),
        value: Some("1000000".to_string()),
    });

    let auth_state = CurrentAuthState::Api(super::ApiAuthFile {
        openai_api_key: first_preset.api_key.clone(),
    });

    // 当前 config.toml 实际写入了 1M 预设的全部覆盖取值。
    let mut applied_config = BTreeMap::new();
    applied_config.insert("model".to_string(), "gpt-5.5".to_string());
    applied_config.insert("model_reasoning_effort".to_string(), "high".to_string());
    applied_config.insert("model_context_window".to_string(), "1000000".to_string());

    let provider = super::ConfigProviderState {
        provider_id: "webclx_api".to_string(),
        provider_name: Some(first_preset.provider_name.clone()),
        base_url: Some(first_preset.base_url.clone()),
        wire_api: Some("responses".to_string()),
        config_values: applied_config,
    };
    let presets = vec![first_preset.clone(), second_preset.clone()];
    let current_api = derive_current_api_state(Some(&provider), Some(&auth_state), &presets);

    // 应当选中 1M 预设，而不是排在更前面的同名凭据预设 "sub2api"。
    assert_eq!(
        current_api
            .as_ref()
            .and_then(|item| item.preset_name.as_deref()),
        Some("sub2api gpt-5.5 1M"),
    );
    let first_summary =
        api_preset_summary(&first_preset, CurrentAuthMode::Api, current_api.as_ref());
    let second_summary =
        api_preset_summary(&second_preset, CurrentAuthMode::Api, current_api.as_ref());
    assert!(!first_summary.active);
    assert!(second_summary.active);

    // 当 config.toml 没有写出额外覆盖时，退回首个候选 (sub2api)。
    let provider_no_context = super::ConfigProviderState {
        config_values: BTreeMap::from([
            ("model".to_string(), "gpt-5.5".to_string()),
            ("model_reasoning_effort".to_string(), "high".to_string()),
        ]),
        ..provider
    };
    let current_api_fallback =
        derive_current_api_state(Some(&provider_no_context), Some(&auth_state), &presets);
    assert_eq!(
        current_api_fallback
            .as_ref()
            .and_then(|item| item.preset_name.as_deref()),
        Some("sub2api"),
    );
}

#[test]
fn parse_imported_accounts_handles_sub2api_bundle() {
    let raw = r#"{
        "exported_at": "2026-07-14T08:00:00Z",
        "accounts": [
            {
                "name": "Account A",
                "credentials": {
                    "access_token": "tok-a",
                    "chatgpt_account_id": "acct-a",
                    "email": "a@example.com"
                }
            }
        ]
    }"#;
    let accounts = super::parse_imported_accounts(raw).expect("should parse sub2api bundle");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].access_token, "tok-a");
    assert_eq!(accounts[0].account_id, "acct-a");
    assert_eq!(accounts[0].name, "Account A");
    assert_eq!(accounts[0].email, "a@example.com");
}

#[test]
fn parse_imported_accounts_recovers_account_id_from_jwt() {
    // Minimal JWT: header.payload.signature where payload has the auth claim
    let header = base64_url_no_pad(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = base64_url_no_pad(
        r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"jwt-acct-123"}}"#,
    );
    let jwt = format!("{header}.{payload}.sig");

    let raw = serde_json::json!([{
        "access_token": jwt,
        "chatgpt_account_id": "",
        "email": "jwt@example.com"
    }])
    .to_string();
    let accounts = super::parse_imported_accounts(&raw).expect("should parse CPA array");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].account_id, "jwt-acct-123");
    assert_eq!(accounts[0].access_token, jwt);
}

#[test]
fn parse_imported_accounts_handles_multi_account_bundle() {
    let raw = r#"{
        "accounts": [
            {"credentials": {"access_token": "tok-1", "chatgpt_account_id": "id-1", "email": "e1@x.com"}},
            {"credentials": {"access_token": "tok-2", "chatgpt_account_id": "id-2", "email": "e2@x.com"}}
        ]
    }"#;
    let accounts = super::parse_imported_accounts(raw).expect("should parse multi-account bundle");
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].account_id, "id-1");
    assert_eq!(accounts[1].account_id, "id-2");
}

#[test]
fn parse_imported_accounts_handles_standard_auth_json_tokens() {
    let raw = r#"{
        "tokens": {
            "access_token": "standard-access",
            "account_id": "standard-account",
            "id_token": "standard-id",
            "refresh_token": "standard-refresh"
        },
        "email": "standard@example.com"
    }"#;
    let accounts = super::parse_imported_accounts(raw).expect("should parse auth.json tokens");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].access_token, "standard-access");
    assert_eq!(accounts[0].account_id, "standard-account");
    assert_eq!(accounts[0].email, "standard@example.com");
}

#[test]
fn parse_imported_accounts_handles_json_stream_and_nested_data_bundle() {
    let raw = r#"
        {"tokens":{"access_token":"stream-1","account_id":"account-1"},"email":"one@example.com"}
        {"data":{"accounts":[
            {"name":"Stream Two","credentials":{"access_token":"stream-2","chatgpt_account_id":"account-2"}},
            {"accessToken":"stream-3","accountId":"account-3"}
        ]}}
    "#;
    let accounts = super::parse_imported_accounts(raw).expect("should parse every JSON value");
    assert_eq!(accounts.len(), 3);
    assert_eq!(accounts[0].account_id, "account-1");
    assert_eq!(accounts[1].name, "Stream Two");
    assert_eq!(accounts[2].access_token, "stream-3");
    assert_eq!(accounts[2].account_id, "account-3");
}

#[test]
fn parse_imported_accounts_rejects_empty() {
    let result = super::parse_imported_accounts(r#"{"accounts":[]}"#);
    assert!(result.is_err());
}

fn base64_url_no_pad(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input.as_bytes())
}
