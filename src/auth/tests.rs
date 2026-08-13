use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use auth_core::{
    ApiAccessMode, ApiAuthFile, AuthFile, AuthPresetDetails, AuthTokens, ClaudeAccessMode,
    PresetConfigOverride, ResolvedConfigTarget, StoredApiPreset, StoredAuthPreset,
    StoredClaudePreset, api_preset_enables_local_upstream_proxy_on_apply,
    api_provider_base_url_for_mode, api_provider_options,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use zip::{ZipWriter, write::SimpleFileOptions};

use super::apply::local_codex_config_requires_preset_sync;
use super::preset_tests::test_stored_chatgpt_oauth_api_preset_with_endpoint;
use super::{
    TerminalAuthFiles, build_preset_test_client_from_env, collect_terminal_auth_target_users,
    parse_codex_bundled_model_catalog, reorder_presets_by_ids, sync_api_preset_configs,
    sync_auth_preset_configs, test_stored_api_preset_with_delay,
    test_stored_auth_preset_with_endpoint, verify_api_preset_targets, write_api_auth_files,
    write_claude_preset_to_targets, write_login_auth_files,
};

#[test]
fn project_config_requires_preset_sync_only_for_model_or_provider_overrides() {
    let project_header = r#"project_doc_fallback_filenames = ["AGENTS.MD"]
project_doc_max_bytes = 65536
"#;
    let project_only = format!(
        r#"{project_header}

[features]
goals = true
"#
    );
    assert!(!local_codex_config_requires_preset_sync(&project_only).unwrap());

    assert!(
        local_codex_config_requires_preset_sync(&format!(
            "{project_header}model = \"gpt-5.6-sol\"\n"
        ))
        .unwrap()
    );
    assert!(
        local_codex_config_requires_preset_sync(&format!(
            "{project_header}model_provider = \"webclx_api\"\n"
        ))
        .unwrap()
    );
}

#[test]
fn reorder_presets_by_ids_uses_requested_order() {
    let presets = vec![
        ("auth-a".to_string(), 1),
        ("auth-b".to_string(), 2),
        ("auth-c".to_string(), 3),
    ];
    let ids = vec![
        "auth-c".to_string(),
        "auth-a".to_string(),
        "auth-b".to_string(),
    ];

    let reordered = reorder_presets_by_ids(presets, &ids, |preset| preset.0.as_str(), "auth")
        .expect("all ids should be accepted");

    assert_eq!(
        reordered
            .into_iter()
            .map(|preset| preset.0)
            .collect::<Vec<_>>(),
        ids
    );
}

#[test]
fn reorder_presets_by_ids_rejects_missing_or_duplicate_ids() {
    let presets = vec![("api-a".to_string(), 1), ("api-b".to_string(), 2)];
    let duplicate_ids = vec!["api-a".to_string(), "api-a".to_string()];
    let missing_ids = vec!["api-a".to_string()];

    assert!(
        reorder_presets_by_ids(presets.clone(), &duplicate_ids, |preset| preset.0.as_str(), "API")
            .is_err()
    );
    assert!(
        reorder_presets_by_ids(presets, &missing_ids, |preset| preset.0.as_str(), "API").is_err()
    );
}

#[test]
fn codex_bundled_model_catalog_parser_requires_models_array() {
    let catalog =
        parse_codex_bundled_model_catalog(br#"{"models":[{"slug":"future-bundled-model"}]}"#)
            .expect("bundled model catalog should parse");
    let error = parse_codex_bundled_model_catalog(br#"{"models":{}}"#)
        .expect_err("bundled catalog without a models array should fail");

    assert_eq!(catalog["models"][0]["slug"], "future-bundled-model");
    assert!(error.to_string().contains("models"));
}

#[test]
fn api_account_file_import_recurses_through_nested_archives() {
    let tar_gz = tar_gz_bytes(&[(
        "nested/account.json",
        br#"{"tokens":{"access_token":"nested-token","account_id":"nested-account"}}"#,
    )]);
    let inner_zip = zip_bytes(&[(
        "inner.json",
        br#"{"access_token":"inner-token","account_id":"inner-account"}"#.to_vec(),
    )]);
    let outer_zip = zip_bytes(&[
        (
            "top.json",
            br#"{"accounts":[{"credentials":{"access_token":"top-token","chatgpt_account_id":"top-account"}}]}"#.to_vec(),
        ),
        ("archives/accounts.tar.gz", tar_gz),
        ("archives/deeper.zip", inner_zip),
    ]);

    let imported = super::collect_accounts_from_upload("accounts.zip", &outer_zip)
        .expect("nested archives should import");
    let account_ids = imported
        .accounts
        .iter()
        .map(|account| account.account_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(account_ids, vec!["top-account", "nested-account", "inner-account"]);
    assert!(imported.errors.is_empty());
}

#[test]
fn api_account_file_import_keeps_valid_files_and_reports_invalid_json() {
    let archive = zip_bytes(&[
        ("broken.json", b"not json".to_vec()),
        (
            "valid.json",
            br#"{"access_token":"valid-token","account_id":"valid-account"}"#.to_vec(),
        ),
    ]);

    let imported = super::collect_accounts_from_upload("mixed.zip", &archive)
        .expect("one valid JSON file should make the batch importable");

    assert_eq!(imported.accounts.len(), 1);
    assert_eq!(imported.accounts[0].account_id, "valid-account");
    assert_eq!(imported.errors.len(), 1);
    assert!(imported.errors[0].contains("broken.json"));
}

#[test]
fn api_account_file_import_combines_multiple_cpa_files() {
    let first = br#"{
        "type":"codex",
        "access_token":"cpa-token-1",
        "account_id":"cpa-account-1",
        "email":"cpa-one@example.com",
        "last_refresh":"2026-07-15T12:00:00Z",
        "expired":"2026-07-25T12:00:00Z"
    }"#;
    let second = br#"{
        "type":"codex",
        "access_token":"cpa-token-2",
        "account_id":"cpa-account-2",
        "email":"cpa-two@example.com",
        "last_refresh":"2026-07-15T12:01:00Z",
        "expired":"2026-07-25T12:01:00Z"
    }"#;

    let imported = super::collect_accounts_from_uploads([
        ("cpa-one.json", first.as_slice()),
        ("cpa-two.json", second.as_slice()),
    ])
    .expect("multiple CPA auth files should import as one batch");

    assert_eq!(imported.accounts.len(), 2);
    assert_eq!(imported.accounts[0].account_id, "cpa-account-1");
    assert_eq!(imported.accounts[1].account_id, "cpa-account-2");
    assert!(imported.errors.is_empty());
}

#[test]
fn api_account_file_import_rejects_excessive_archive_depth() {
    let mut payload = br#"{"access_token":"deep-token","account_id":"deep-account"}"#.to_vec();
    for depth in 0..=super::API_ACCOUNT_IMPORT_MAX_ARCHIVE_DEPTH {
        payload = zip_bytes(&[(format!("level-{depth}.zip"), payload)]);
    }

    let error = super::collect_accounts_from_upload("too-deep.zip", &payload)
        .expect_err("excessive nesting must be rejected");
    assert!(error.to_string().contains("嵌套层数"));
}

#[test]
fn api_account_file_import_rejects_unsafe_archive_paths() {
    let archive = zip_bytes(&[(
        "../account.json",
        br#"{"access_token":"unsafe-token","account_id":"unsafe-account"}"#.to_vec(),
    )]);

    let error = super::collect_accounts_from_upload("unsafe.zip", &archive)
        .expect_err("unsafe archive paths must be rejected");
    assert!(error.to_string().contains("不安全路径"));
}

#[test]
fn batch_api_import_suffixes_colliding_preset_ids() {
    let existing = ["api-123", "api-123-2"];

    assert_eq!(super::unique_import_preset_id("api-123", existing.into_iter()), "api-123-3",);
}

fn zip_bytes<N>(entries: &[(N, Vec<u8>)]) -> Vec<u8>
where
    N: AsRef<str>,
{
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        writer
            .start_file(name.as_ref(), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn tar_gz_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for (name, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive.append_data(&mut header, name, *content).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

#[test]
fn collect_terminal_auth_target_users_uses_only_configured_terminal_user() {
    assert_eq!(collect_terminal_auth_target_users("alice"), vec!["alice".to_string()]);
    assert_eq!(
        collect_terminal_auth_target_users(crate::runtime_paths::DEFAULT_USER_NAME),
        vec![crate::runtime_paths::DEFAULT_USER_NAME.to_string()]
    );
}

#[tokio::test]
async fn mirrored_auth_writes_update_every_target_home() {
    let temp_root = test_temp_dir("mirrored-auth-writes");
    let targets = vec![
        test_terminal_auth_files(&temp_root, "alice"),
        test_terminal_auth_files(&temp_root, "root"),
    ];
    let auth = sample_auth_file();

    write_login_auth_files(&targets, &auth).await.unwrap();
    sync_auth_preset_configs(
        &targets,
        &[ResolvedConfigTarget {
            key: "model".to_string(),
            value: "\"gpt-5.5\"".to_string(),
        }],
    )
    .await
    .unwrap();

    for target in &targets {
        let auth_content = fs::read_to_string(&target.auth_file).unwrap();
        assert!(auth_content.contains("\"account_id\": \"acct-123456\""), "{auth_content}");
        let config_content = fs::read_to_string(&target.config_file).unwrap();
        assert!(config_content.contains("model = \"gpt-5.5\""), "{config_content}");
    }

    let _ = fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn api_preset_verification_reads_back_auth_config_and_model() {
    let temp_root = test_temp_dir("api-preset-verification");
    let targets = vec![test_terminal_auth_files(&temp_root, "alice")];
    fs::create_dir_all(targets[0].config_file.parent().unwrap()).unwrap();
    fs::write(
        &targets[0].config_file,
        "model_context_window = 1000000\nuser_custom_setting = \"keep\"\n",
    )
    .unwrap();
    let preset = StoredApiPreset {
        id: "api-verify".to_string(),
        name: "Verify API".to_string(),
        saved_at: 0,
        provider_name: "verify-provider".to_string(),
        base_url: "https://verify.example/v1".to_string(),
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: vec![auth_core::PresetConfigOverride {
            key: Some("model".to_string()),
            value: Some("verify-model".to_string()),
        }],
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "verify-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let config_targets = vec![ResolvedConfigTarget {
        key: "model".to_string(),
        value: "\"verify-model\"".to_string(),
    }];

    write_api_auth_files(
        &targets,
        &ApiAuthFile {
            openai_api_key: preset.api_key.clone(),
        },
    )
    .await
    .unwrap();
    sync_api_preset_configs(
        &targets,
        &preset.provider_name,
        &preset.base_url,
        &api_provider_options(&preset),
        &config_targets,
        &["model".to_string(), "model_context_window".to_string()],
    )
    .await
    .unwrap();

    let applied_config = fs::read_to_string(&targets[0].config_file).unwrap();
    assert!(!applied_config.contains("model_context_window"));
    assert!(applied_config.contains("user_custom_setting = \"keep\""));

    let verification = verify_api_preset_targets(&targets, &preset, &config_targets)
        .await
        .unwrap();

    assert!(verification.matches, "{verification:?}");
    assert_eq!(verification.current_mode, auth_core::CurrentAuthMode::Api);
    assert_eq!(
        verification
            .current_api
            .as_ref()
            .and_then(|current| current.base_url.as_deref()),
        Some("https://verify.example/v1")
    );
    assert_eq!(
        verification.config_values.get("model").map(String::as_str),
        Some("verify-model")
    );

    fs::write(
        &targets[0].config_file,
        r#"model_provider = "verify-provider"
model = "wrong-model"

[model_providers.verify-provider]
name = "verify-provider"
base_url = "https://verify.example/v1"
wire_api = "responses"
"#,
    )
    .unwrap();

    let mismatch = verify_api_preset_targets(&targets, &preset, &config_targets)
        .await
        .unwrap();
    assert!(!mismatch.matches, "{mismatch:?}");
    assert!(
        mismatch
            .mismatches
            .iter()
            .any(|item| item.contains("model")),
        "{mismatch:?}"
    );

    let _ = fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn claude_proxy_apply_updates_onboarding_without_replacing_claude_json() {
    let temp_root = test_temp_dir("claude-proxy-onboarding");
    let targets = vec![test_terminal_auth_files(&temp_root, "alice")];
    let target = &targets[0];
    fs::create_dir_all(target.claude_settings_file.parent().unwrap()).unwrap();
    fs::write(&target.claude_settings_file, r#"{"env":{"KEEP_ME":"1"}}"#).unwrap();
    fs::write(
        temp_root.join("alice/.claude.json"),
        r#"{"hasCompletedOnboarding":false,"projects":{"/home/codes/webClx":{"hasTrustDialogAccepted":true}}}"#,
    )
    .unwrap();
    let preset = StoredClaudePreset {
        id: "claude-relay".to_string(),
        name: "Claude Relay".to_string(),
        saved_at: 1,
        provider_name: "Relay".to_string(),
        base_url: "https://api.minimaxi.com/anthropic".to_string(),
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-ant-example".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: Some("MiniMax-M1".to_string()),
        third_party_model: None,
        use_local_proxy: true,
        access_mode: Some(ClaudeAccessMode::AnthropicRelay),
        switch_count: 0,
    };

    write_claude_preset_to_targets(&targets, &preset)
        .await
        .unwrap();

    let claude_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp_root.join("alice/.claude.json")).unwrap())
            .unwrap();
    assert_eq!(
        claude_json
            .get("hasCompletedOnboarding")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        claude_json
            .pointer("/projects/~1home~1codes~1webClx/hasTrustDialogAccepted")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let _ = fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn api_preset_test_client_honors_terminal_proxy_and_no_proxy_env() {
    let (base_url, paths) = spawn_preset_test_server(2).await;
    let preset = StoredApiPreset {
        id: "api-test-env".to_string(),
        name: "API Test Env".to_string(),
        saved_at: 0,
        provider_name: "test-provider".to_string(),
        base_url,
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "test-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let invalid_proxy_env = vec![
        ("HTTP_PROXY".to_string(), "http://127.0.0.1:9".to_string()),
        ("HTTPS_PROXY".to_string(), "http://127.0.0.1:9".to_string()),
    ];
    let proxied_client =
        build_preset_test_client_from_env(&invalid_proxy_env, std::time::Duration::from_secs(2))
            .unwrap();
    let proxied_result =
        test_stored_api_preset_with_delay(&proxied_client, &preset, &[], std::time::Duration::ZERO)
            .await
            .unwrap();
    assert!(!proxied_result.ok, "{proxied_result:?}");

    let mut bypass_env = invalid_proxy_env;
    bypass_env.push(("NO_PROXY".to_string(), "127.0.0.1".to_string()));
    let bypass_client =
        build_preset_test_client_from_env(&bypass_env, std::time::Duration::from_secs(2)).unwrap();
    let bypass_result =
        test_stored_api_preset_with_delay(&bypass_client, &preset, &[], std::time::Duration::ZERO)
            .await
            .unwrap();

    assert!(bypass_result.ok, "{bypass_result:?}");
    assert_eq!(paths.lock().unwrap().as_slice(), ["/models", "/responses"]);
}

#[tokio::test]
async fn api_preset_test_sends_responses_probe_for_legacy_chat_wire_api() {
    let (base_url, paths) = spawn_preset_test_server(2).await;
    let preset = StoredApiPreset {
        id: "api-test".to_string(),
        name: "API Test".to_string(),
        saved_at: 0,
        provider_name: "test-provider".to_string(),
        base_url,
        management_url: None,
        wire_api: Some("chat".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "test-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result =
        test_stored_api_preset_with_delay(&client, &preset, &[], std::time::Duration::ZERO)
            .await
            .unwrap();

    assert!(result.ok, "{result:?}");
    assert!(result.message.contains("模型列表可读内容：test-model"), "{}", result.message);
    assert!(result.message.contains("对话测试可读内容：hi"), "{}", result.message);
    assert!(!result.message.contains("模型列表服务器回应"), "{}", result.message);
    assert!(!result.message.contains("对话测试服务器回应"), "{}", result.message);
    let paths = paths.lock().unwrap().clone();
    assert_eq!(paths, vec!["/models", "/responses"]);
}

#[tokio::test]
async fn api_preset_test_rejects_responses_stream_without_completion_event() {
    let base_url = spawn_incomplete_responses_preset_test_server(false).await;
    let preset = StoredApiPreset {
        id: "api-test-incomplete-sse".to_string(),
        name: "API Test Incomplete SSE".to_string(),
        saved_at: 0,
        provider_name: "test-provider".to_string(),
        base_url,
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "test-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result =
        test_stored_api_preset_with_delay(&client, &preset, &[], std::time::Duration::ZERO)
            .await
            .unwrap();

    assert!(!result.ok, "{result:?}");
    assert!(result.message.contains("response.completed"), "{}", result.message);
}

#[tokio::test]
async fn api_preset_test_rejects_responses_body_disconnect_after_http_200() {
    let base_url = spawn_incomplete_responses_preset_test_server(true).await;
    let preset = StoredApiPreset {
        id: "api-test-disconnected-sse".to_string(),
        name: "API Test Disconnected SSE".to_string(),
        saved_at: 0,
        provider_name: "test-provider".to_string(),
        base_url,
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "test-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result =
        test_stored_api_preset_with_delay(&client, &preset, &[], std::time::Duration::ZERO)
            .await
            .unwrap();

    assert!(!result.ok, "{result:?}");
    assert!(result.message.contains("读取服务器响应失败"), "{}", result.message);
}

#[tokio::test]
async fn auth_preset_test_uses_saved_oauth_account_for_responses() {
    let (endpoint, request) = spawn_codex_oauth_test_server().await;
    let preset = StoredAuthPreset {
        id: "auth-test".to_string(),
        name: "OAuth Test".to_string(),
        saved_at: 0,
        details: AuthPresetDetails::default(),
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth: sample_auth_file(),
        switch_count: 0,
    };
    let client = reqwest::Client::new();

    let result =
        test_stored_auth_preset_with_endpoint(&client, &preset, "gpt-5.4", &endpoint).await;

    assert!(result.ok, "{result:?}");
    assert_eq!(result.status, Some(200));
    assert!(result.message.contains("hi"), "{}", result.message);
    let request = request.lock().unwrap().clone();
    assert!(request.contains("authorization: bearer access-token"), "{request}");
    assert!(request.contains("chatgpt-account-id: acct-123456"), "{request}");
    assert!(request.contains(r#""model":"gpt-5.4""#), "{request}");
    assert!(request.contains(r#""input":[{"#), "{request}");
    assert!(request.contains(r#""type":"message""#), "{request}");
    assert!(request.contains(r#""role":"user""#), "{request}");
    assert!(request.contains(r#""type":"input_text""#), "{request}");
    assert!(request.contains(r#""text":"hi""#), "{request}");
    assert!(!request.contains("max_output_tokens"), "{request}");
}

#[tokio::test]
async fn chatgpt_oauth_api_preset_test_reuses_oauth_probe_contract() {
    let (endpoint, request) = spawn_codex_oauth_test_server().await;
    let preset = StoredApiPreset {
        id: "api-oauth-test".to_string(),
        name: "Imported OAuth".to_string(),
        saved_at: 0,
        provider_name: "ChatGPT".to_string(),
        base_url: "https://chatgpt.com/backend-api/codex".to_string(),
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: true,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "webclx-local-api-proxy:api-oauth-test".to_string(),
        access_token: "access-token".to_string(),
        account_id: "acct-123456".to_string(),
        access_mode: Some(ApiAccessMode::ChatgptOauth),
        switch_count: 0,
    };
    let client = reqwest::Client::new();

    let result =
        test_stored_chatgpt_oauth_api_preset_with_endpoint(&client, &preset, "gpt-5.4", &endpoint)
            .await;

    assert!(result.ok, "{result:?}");
    let request = request.lock().unwrap().clone();
    assert!(request.starts_with("post /backend-api/codex/responses "), "{request}");
    assert!(request.contains("authorization: bearer access-token"), "{request}");
    assert!(request.contains("chatgpt-account-id: acct-123456"), "{request}");
    assert!(!request.contains("max_output_tokens"), "{request}");
    assert!(!request.contains("get /models"), "{request}");
}

#[tokio::test]
async fn api_preset_test_uses_model_override_for_responses_when_models_is_missing() {
    let (base_url, paths) = spawn_responses_preset_test_server().await;
    let preset = StoredApiPreset {
        id: "api-test".to_string(),
        name: "API Test".to_string(),
        saved_at: 0,
        provider_name: "test-provider".to_string(),
        base_url,
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: vec![auth_core::PresetConfigOverride {
            key: Some("model".to_string()),
            value: Some("gpt-test".to_string()),
        }],
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "test-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result =
        test_stored_api_preset_with_delay(&client, &preset, &[], std::time::Duration::ZERO)
            .await
            .unwrap();

    assert!(result.ok, "{result:?}");
    assert_eq!(result.status, Some(200));
    assert_eq!(result.endpoint, format!("{}/responses", preset.base_url));
    assert!(
        result
            .message
            .contains("模型列表可读内容：未能提取模型名。"),
        "{}",
        result.message
    );
    assert!(result.message.contains("对话测试可读内容：hi"), "{}", result.message);
    let paths = paths.lock().unwrap().clone();
    assert_eq!(paths, vec!["/models", "/responses"]);
}

#[tokio::test]
async fn api_preset_test_uses_default_model_when_preset_has_no_model_override() {
    let (base_url, paths) = spawn_responses_preset_test_server_with_model_check(
        r#"{"data":[{"id":"gpt-5"},{"id":"gpt-5.5"}]}"#,
        "gpt-5.5",
    )
    .await;
    let preset = StoredApiPreset {
        id: "api-test".to_string(),
        name: "API Test".to_string(),
        saved_at: 0,
        provider_name: "test-provider".to_string(),
        base_url,
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: vec![auth_core::PresetConfigOverride {
            key: Some("model_reasoning_effort".to_string()),
            value: Some("high".to_string()),
        }],
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "test-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result = test_stored_api_preset_with_delay(
        &client,
        &preset,
        &[("model", "gpt-5.5"), ("model_reasoning_effort", "high")],
        std::time::Duration::ZERO,
    )
    .await
    .unwrap();

    assert!(result.ok, "{result:?}");
    assert_eq!(result.endpoint, format!("{}/responses", preset.base_url));
    assert!(result.message.contains("对话测试可读内容：hi"), "{}", result.message);
    let paths = paths.lock().unwrap().clone();
    assert_eq!(paths, vec!["/models", "/responses"]);
}

#[test]
fn api_preset_test_uses_local_proxy_chat_probe_when_explicitly_enabled() {
    let preset = StoredApiPreset {
        id: "api-test".to_string(),
        name: "Zhipu 5.1".to_string(),
        saved_at: 0,
        provider_name: "智谱".to_string(),
        base_url: "https://open.bigmodel.cn/api/coding/paas/v4".to_string(),
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: true,
        config_overrides: vec![auth_core::PresetConfigOverride {
            key: Some("model".to_string()),
            value: Some("glm-5.1".to_string()),
        }],
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "test-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };

    let base_url = api_provider_base_url_for_mode(
        &preset,
        api_preset_enables_local_upstream_proxy_on_apply(&preset),
    );
    assert!(base_url.ends_with("/api/upstream/openai/v1"));
    assert_eq!(api_provider_options(&preset).wire_api, "responses");
    assert_eq!(
        super::api_probe_endpoint(&preset, &base_url),
        format!("{base_url}/chat/completions")
    );
}

#[tokio::test]
async fn claude_preset_test_uses_preset_model_when_models_endpoint_is_missing() {
    let (base_url, paths) = spawn_claude_models_404_preset_test_server().await;
    let preset = StoredClaudePreset {
        id: "claude-test".to_string(),
        name: "DeepSeek Claude 官方 Anthropic".to_string(),
        saved_at: 0,
        provider_name: "DeepSeek".to_string(),
        base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "test-key".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("deepseek-v4-pro".to_string()),
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result = super::test_stored_claude_preset_with_delay(
        &client,
        &preset,
        false,
        std::time::Duration::ZERO,
    )
    .await;

    assert!(result.ok, "{result:?}");
    assert_eq!(result.status, Some(200));
    assert!(
        result
            .message
            .contains("模型列表测试失败；已使用预设模型等待 2 秒后对话测试成功。"),
        "{}",
        result.message
    );
    assert!(
        result
            .message
            .contains("模型列表可读内容：未能提取模型名。"),
        "{}",
        result.message
    );
    assert!(result.message.contains("对话测试可读内容：hi"), "{}", result.message);
    let paths = paths.lock().unwrap().clone();
    assert_eq!(paths, vec!["/v1/models", "/v1/messages"]);
}

#[tokio::test]
async fn claude_preset_test_uses_inherited_env_model_when_model_fields_are_empty() {
    let (base_url, paths) = spawn_claude_models_404_preset_test_server().await;
    let preset = StoredClaudePreset {
        id: "claude-global-test".to_string(),
        name: "Claude inherited model".to_string(),
        saved_at: 0,
        provider_name: "Test".to_string(),
        base_url,
        management_url: None,
        config_overrides: vec![PresetConfigOverride {
            key: Some("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string()),
            value: Some("claude-sonnet-global".to_string()),
        }],
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "test-key".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: None,
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result = super::test_stored_claude_preset_with_delay(
        &client,
        &preset,
        false,
        std::time::Duration::ZERO,
    )
    .await;

    assert!(result.ok, "{result:?}");
    assert_eq!(paths.lock().unwrap().as_slice(), ["/v1/models", "/v1/messages"]);
}

#[test]
fn claude_preset_test_target_uses_local_proxy_credentials_when_enabled() {
    let preset = StoredClaudePreset {
        id: "claude-local-test".to_string(),
        name: "Claude Local Test".to_string(),
        saved_at: 0,
        provider_name: "DeepSeek".to_string(),
        base_url: "https://api.deepseek.com/anthropic".to_string(),
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "real-claude-token".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("deepseek-v4-pro".to_string()),
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };

    let target = super::claude_preset_test_target(&preset, true);

    assert!(target.base_url.ends_with("/api/upstream/anthropic"), "{}", target.base_url);
    assert_eq!(target.auth_token, auth_core::local_proxy_claude_token_for_preset_id(&preset.id));
    assert_ne!(target.auth_token, preset.auth_token);

    let request = super::apply_claude_preset_test_headers(
        reqwest::Client::new().get("http://127.0.0.1:11111/api/upstream/anthropic/v1/models"),
        &preset,
        &target,
    )
    .build()
    .unwrap();
    assert_eq!(
        request
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok()),
        Some(target.auth_token.as_str())
    );
    assert_eq!(
        request
            .headers()
            .get(super::UPSTREAM_PRESET_ID_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(preset.id.as_str())
    );
}

#[test]
fn claude_preset_test_target_keeps_minimax_anthropic_direct_when_account_proxy_disabled() {
    let preset = StoredClaudePreset {
        id: "claude-minimax-direct".to_string(),
        name: "MiniMax Claude Direct".to_string(),
        saved_at: 0,
        provider_name: "MiniMax".to_string(),
        base_url: "https://api.minimaxi.com/anthropic".to_string(),
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "real-minimax-token".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("MiniMax-M2.7".to_string()),
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };

    let target = super::claude_preset_test_target(&preset, preset.use_local_proxy);

    assert_eq!(target.base_url, "https://api.minimaxi.com/anthropic");
    assert_eq!(target.auth_token, "real-minimax-token");
    let request = super::apply_claude_preset_test_headers(
        reqwest::Client::new().get(format!("{}/v1/models", target.base_url)),
        &preset,
        &target,
    )
    .build()
    .unwrap();
    assert_eq!(
        request
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok()),
        Some("real-minimax-token")
    );
    assert!(
        request
            .headers()
            .get(super::UPSTREAM_PRESET_ID_HEADER)
            .is_none(),
        "direct Claude presets must not send local proxy routing headers"
    );
}

#[tokio::test]
async fn api_preset_test_retries_v1_when_root_models_is_not_json_models() {
    let (base_url, paths) = spawn_v1_fallback_preset_test_server().await;
    let preset = StoredApiPreset {
        id: "api-test".to_string(),
        name: "API Test".to_string(),
        saved_at: 0,
        provider_name: "test-provider".to_string(),
        base_url,
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "test-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result =
        test_stored_api_preset_with_delay(&client, &preset, &[], std::time::Duration::ZERO)
            .await
            .unwrap();

    assert!(result.ok, "{result:?}");
    assert_eq!(result.endpoint, format!("{}/v1/responses", preset.base_url));
    assert!(result.message.contains("模型列表可读内容：v1-test-model"), "{}", result.message);
    assert!(result.message.contains("对话测试可读内容：hi"), "{}", result.message);
    let paths = paths.lock().unwrap().clone();
    assert_eq!(paths, vec!["/models", "/v1/models", "/v1/responses"]);
}

#[tokio::test]
async fn api_preset_test_routes_through_local_upstream_proxy_when_enabled() {
    // The mock server pretends to be the local webclx upstream proxy
    // (e.g. /api/upstream/openai/v1/...). The real base_url of the
    // zhipu preset should never be reached while
    // the preset targets a domestic chat upstream, so a correct
    // implementation must force the request through the proxy origin
    // even when the saved proxy switch is false.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let auths: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let preset_headers: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    let auths_for_task = Arc::clone(&auths);
    let preset_headers_for_task = Arc::clone(&preset_headers);
    tokio::spawn(async move {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer).await else {
                break;
            };
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let first_line = request.lines().next().unwrap_or("").to_string();
            let auth_header = request
                .lines()
                .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
                .unwrap_or("")
                .to_string();
            let preset_header = request
                .lines()
                .find(|line| {
                    line.to_ascii_lowercase()
                        .starts_with("x-webclx-upstream-preset-id:")
                })
                .unwrap_or("")
                .to_string();
            requests_for_task.lock().unwrap().push(first_line);
            auths_for_task.lock().unwrap().push(auth_header);
            preset_headers_for_task.lock().unwrap().push(preset_header);
            let body = if request.starts_with("GET") {
                r#"{"data":[{"id":"glm-5.1"}]}"#
            } else {
                r#"{"id":"chat-test","choices":[{"message":{"role":"assistant","content":"hi"}}]}"#
            };
            let response = format!(
                "HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {}
Connection: close

{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });

    // Point the proxy origin at the mock listener so the upstream
    // URL resolves to the local TCP server.
    auth_core::set_local_webclx_origin(format!("http://{addr}"));

    let preset = StoredApiPreset {
        id: "api-test-zhipu".to_string(),
        name: "智谱5.1".to_string(),
        saved_at: 0,
        provider_name: "智谱".to_string(),
        base_url: "https://open.bigmodel.cn/api/coding/paas/v4".to_string(),
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: None,
        apply_upstream_proxy_on_switch: true,
        config_overrides: vec![auth_core::PresetConfigOverride {
            key: Some("model".to_string()),
            value: Some("glm-5.1".to_string()),
        }],
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "real-zhipu-key".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let result =
        test_stored_api_preset_with_delay(&client, &preset, &[], std::time::Duration::ZERO)
            .await
            .unwrap();

    assert!(result.ok, "{result:?}");
    assert!(
        result
            .endpoint
            .ends_with("/api/upstream/openai/v1/chat/completions"),
        "expected proxy chat endpoint, got {}",
        result.endpoint
    );
    assert!(
        !result.endpoint.contains("open.bigmodel.cn"),
        "test must not hit the original zhipu upstream: {}",
        result.endpoint
    );

    let requests = requests.lock().unwrap().clone();
    assert_eq!(
        requests,
        vec![
            "GET /api/upstream/openai/v1/models HTTP/1.1".to_string(),
            "POST /api/upstream/openai/v1/chat/completions HTTP/1.1".to_string(),
        ]
    );

    let auths = auths.lock().unwrap().clone();
    assert!(
        auths
            .iter()
            .all(|header| header.contains(auth_core::LOCAL_PROXY_API_KEY)),
        "expected local proxy bearer token, got {auths:?}"
    );
    assert!(
        !auths.iter().any(|header| header.contains("real-zhipu-key")),
        "real zhipu key must not leak through the proxy: {auths:?}"
    );
    let preset_headers = preset_headers.lock().unwrap().clone();
    assert!(
        preset_headers
            .iter()
            .all(|header| header.contains("api-test-zhipu")),
        "expected preset id override header, got {preset_headers:?}"
    );
}

#[test]
fn readable_models_summary_includes_models_after_the_first_eight() {
    let body = r#"{"data":[
            {"id":"gpt-5.4"},
            {"id":"gpt-5.4-2025-12-11"},
            {"id":"gpt-5.5"},
            {"id":"gpt-5.4-codex"},
            {"id":"gpt-5.5-codex"},
            {"id":"gpt-5.4-high"},
            {"id":"gpt-5.4-mini"},
            {"id":"gpt-5.4-thinking"},
            {"id":"gpt-5.4-xhigh"},
            {"id":"gpt-5.5"}
        ]}"#;

    let summary = super::readable_models_summary(body);

    assert!(summary.contains("gpt-5.5"), "{summary}");
}

#[test]
fn readable_chat_summary_extracts_anthropic_thinking_blocks() {
    let body = r#"{
            "id": "msg-test",
            "type": "message",
            "role": "assistant",
            "model": "MiniMax-M3",
            "content": [
                {
                    "thinking": "The user has sent a simple greeting.",
                    "signature": "test-signature",
                    "type": "thinking"
                }
            ]
        }"#;

    let summary = super::readable_chat_summary(body);

    assert_eq!(summary, "The user has sent a simple greeting.");
}

#[test]
fn readable_chat_summary_extracts_deepseek_reasoning_content() {
    let body = r#"{
            "id": "chat-test",
            "object": "chat.completion",
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "",
                        "reasoning_content": "We need to respond to the user's greeting."
                    }
                }
            ]
        }"#;

    let summary = super::readable_chat_summary(body);

    assert_eq!(summary, "We need to respond to the user's greeting.");
}

async fn spawn_preset_test_server(expected_requests: usize) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let paths_for_task = Arc::clone(&paths);
    tokio::spawn(async move {
        for _ in 0..expected_requests {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer).await else {
                break;
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("")
                .to_string();
            paths_for_task.lock().unwrap().push(path.clone());
            let body = if path == "/models" {
                r#"{"data":[{"id":"test-model"}]}"#
            } else {
                "event: response.output_text.done\n\
                 data: {\"type\":\"response.output_text.done\",\"text\":\"hi\"}\n\n\
                 event: response.completed\n\
                 data: {\"type\":\"response.completed\"}\n\n"
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                if path == "/models" {
                    "application/json"
                } else {
                    "text/event-stream"
                },
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    (format!("http://{addr}"), paths)
}

async fn spawn_incomplete_responses_preset_test_server(disconnect_body: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for request_index in 0..2 {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer).await else {
                break;
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("");
            if request_index == 0 || path.ends_with("/models") {
                let body = r#"{"data":[{"id":"test-model"}]}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                continue;
            }

            let body = "event: response.output_text.delta\n\
                        data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n";
            let declared_length = if disconnect_body {
                body.len() + 128
            } else {
                body.len()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {declared_length}\r\nConnection: close\r\n\r\n{body}"
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    format!("http://{addr}")
}

async fn spawn_claude_models_404_preset_test_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let paths_for_task = Arc::clone(&paths);
    tokio::spawn(async move {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer).await else {
                break;
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("")
                .to_string();
            paths_for_task.lock().unwrap().push(path.clone());
            let (status, body) = if path == "/v1/models" {
                ("404 Not Found", r#"{"error":{"message":"not found"}}"#)
            } else {
                (
                    "200 OK",
                    r#"{"id":"msg-test","type":"message","role":"assistant","content":[{"type":"text","text":"hi"}]}"#,
                )
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    (format!("http://{addr}"), paths)
}

async fn spawn_responses_preset_test_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let paths_for_task = Arc::clone(&paths);
    tokio::spawn(async move {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer).await else {
                break;
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("")
                .to_string();
            paths_for_task.lock().unwrap().push(path.clone());
            let (status, body) = if path == "/models" {
                ("404 Not Found", r#"{"error":"Not Found"}"#)
            } else {
                (
                    "200 OK",
                    "event: response.output_text.done\n\
                         data: {\"type\":\"response.output_text.done\",\"text\":\"hi\"}\n\n\
                         event: response.completed\n\
                         data: {\"type\":\"response.completed\"}\n\n",
                )
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    (format!("http://{addr}"), paths)
}

async fn spawn_responses_preset_test_server_with_model_check(
    models_body: &'static str,
    accepted_model: &'static str,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let paths_for_task = Arc::clone(&paths);
    tokio::spawn(async move {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer).await else {
                break;
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("")
                .to_string();
            paths_for_task.lock().unwrap().push(path.clone());
            let (status, body) = if path == "/models" {
                ("200 OK", models_body.to_string())
            } else if request.contains(&format!(r#""model":"{accepted_model}""#)) {
                (
                    "200 OK",
                    "event: response.output_text.done\n\
                         data: {\"type\":\"response.output_text.done\",\"text\":\"hi\"}\n\n\
                         event: response.completed\n\
                         data: {\"type\":\"response.completed\"}\n\n"
                        .to_string(),
                )
            } else {
                ("404 Not Found", format!(r#"{{"error":"model {accepted_model} expected"}}"#))
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    (format!("http://{addr}"), paths)
}

async fn spawn_v1_fallback_preset_test_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let paths = Arc::new(Mutex::new(Vec::new()));
    let paths_for_task = Arc::clone(&paths);
    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 4096];
            let Ok(read) = stream.read(&mut buffer).await else {
                break;
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("")
                .to_string();
            paths_for_task.lock().unwrap().push(path.clone());
            let body = match path.as_str() {
                "/v1/models" => r#"{"data":[{"id":"v1-test-model"}]}"#.to_string(),
                "/v1/responses" => "event: response.output_text.done\n\
                         data: {\"type\":\"response.output_text.done\",\"text\":\"hi\"}\n\n\
                         event: response.completed\n\
                         data: {\"type\":\"response.completed\"}\n\n"
                    .to_string(),
                _ => "<!DOCTYPE html><html><body>docs</body></html>".to_string(),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    (format!("http://{addr}"), paths)
}

async fn spawn_codex_oauth_test_server() -> (String, Arc<Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let request = Arc::new(Mutex::new(String::new()));
    let request_for_task = Arc::clone(&request);
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buffer = vec![0_u8; 16 * 1024];
        let Ok(read) = stream.read(&mut buffer).await else {
            return;
        };
        *request_for_task.lock().unwrap() =
            String::from_utf8_lossy(&buffer[..read]).to_ascii_lowercase();
        let body = "event: response.output_text.delta\n\
                    data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n\
                    event: response.completed\n\
                    data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });
    (format!("http://{addr}/backend-api/codex/responses"), request)
}

fn sample_auth_file() -> AuthFile {
    AuthFile {
        openai_api_key: None,
        last_refresh: "2026-06-02T00:00:00Z".to_string(),
        tokens: AuthTokens {
            access_token: "access-token".to_string(),
            account_id: "acct-123456".to_string(),
            id_token: "id-token".to_string(),
            refresh_token: "refresh-token".to_string(),
        },
    }
}

fn test_terminal_auth_files(root: &Path, user_name: &str) -> TerminalAuthFiles {
    let user_root = root.join(user_name);
    TerminalAuthFiles {
        user_name: user_name.to_string(),
        auth_file: user_root.join(".codex/auth.json"),
        config_file: user_root.join(".codex/config.toml"),
        claude_settings_file: user_root.join(".claude/settings.json"),
    }
}

fn test_temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("webclx-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}
