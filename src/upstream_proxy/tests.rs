use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use auth_core::{
    ApiAccessMode, ApiResponsesProxyMode, StoredApiPreset, StoredClaudePreset,
    UpstreamProxySettings, local_proxy_api_key_for_preset_id,
    local_proxy_claude_token_for_preset_id,
};
use axum::{
    body::{Body, to_bytes},
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use crate::frpc::{FrpRoleManager, FrpcManager, FrpsManager};

use super::{
    UPSTREAM_PRESET_ID_HEADER, anthropic_proxy_uses_legacy_bare_token, anthropic_upstream_proxy,
    openai_proxy_uses_legacy_bare_token, openai_upstream_proxy,
};
use crate::{AppState, auth, codex_proxy, proxy, settings, terminal};

#[test]
fn active_fallback_warning_only_matches_legacy_bare_tokens() {
    let mut headers = HeaderMap::new();
    headers
        .insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer webclx-local-api-proxy"));
    assert!(openai_proxy_uses_legacy_bare_token(&headers));

    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer webclx-local-api-proxy:preset-a"),
    );
    assert!(!openai_proxy_uses_legacy_bare_token(&headers));
    headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer sk-real-client-key"));
    assert!(!openai_proxy_uses_legacy_bare_token(&headers));

    headers.remove(header::AUTHORIZATION);
    headers.insert("x-api-key", HeaderValue::from_static("webclx-local-claude-proxy"));
    assert!(anthropic_proxy_uses_legacy_bare_token(&headers));
    headers.insert("x-api-key", HeaderValue::from_static("webclx-local-claude-proxy:preset-b"));
    assert!(!anthropic_proxy_uses_legacy_bare_token(&headers));
}

#[tokio::test]
async fn deepseek_chat_proxy_backfills_reasoning_for_synthetic_tool_history() {
    let (base_url, requests) = spawn_openai_chat_mock_server().await;
    let preset = StoredApiPreset {
        id: "api-deepseek-chat".to_string(),
        name: "DeepSeek Chat".to_string(),
        saved_at: 0,
        provider_name: "DeepSeek".to_string(),
        base_url,
        management_url: None,
        wire_api: Some("responses".to_string()),
        responses_proxy: Some(ApiResponsesProxyMode::DeepseekChat),
        apply_upstream_proxy_on_switch: true,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: "sk-deepseek".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_api_proxy_settings(
        preset.clone(),
        UpstreamProxySettings {
            active_api_proxy_preset_id: Some(preset.id.clone()),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/upstream/openai/v1/chat/completions")
        .header("content-type", "application/json")
        .header(UPSTREAM_PRESET_ID_HEADER, &preset.id)
        .body(Body::from(
            r#"{"model":"deepseek-v4-flash","messages":[{"role":"user","content":"load skill"},{"role":"assistant","content":"","tool_calls":[{"id":"skill-1","type":"function","function":{"name":"read_skill","arguments":"{}"}}]},{"role":"tool","tool_call_id":"skill-1","content":"skill loaded"}]}"#,
        ))
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let captured = requests.lock().unwrap().clone();
    assert_eq!(captured.len(), 1);
    assert!(
        captured[0].contains(r#""reasoning_content":"Continuing prior assistant turn.""#),
        "DeepSeek chat request was not sanitized: {}",
        captured[0],
    );
}

#[tokio::test]
async fn codex_proxy_route_keeps_forwarding_when_toggle_disabled_after_apply() {
    let (base_url, requests) = spawn_openai_mock_server().await;
    let preset = StoredApiPreset {
        id: "api-local-proxy".to_string(),
        name: "Local Proxy Preset".to_string(),
        saved_at: 0,
        provider_name: "mock-openai".to_string(),
        base_url,
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
        api_key: "sk-test".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_api_proxy_settings(
        preset.clone(),
        UpstreamProxySettings {
            codex_api_proxy_enabled: false,
            active_api_proxy_preset_id: Some(preset.id),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .body(Body::empty())
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(std::str::from_utf8(&body).unwrap(), r#"{"data":[{"id":"mock-model"}]}"#);
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests, vec!["GET /v1/models HTTP/1.1"]);
}
#[tokio::test]
async fn codex_proxy_route_can_override_active_preset_from_test_header() {
    let (active_base_url, active_requests) = spawn_openai_mock_server().await;
    let (test_base_url, test_requests) = spawn_openai_mock_server().await;
    let active_preset = StoredApiPreset {
        id: "api-active-minimax".to_string(),
        name: "Active MiniMax".to_string(),
        saved_at: 0,
        provider_name: "mock-active".to_string(),
        base_url: active_base_url,
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
        api_key: "sk-active".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let test_preset = StoredApiPreset {
        id: "api-test-deepseek".to_string(),
        name: "Test DeepSeek".to_string(),
        saved_at: 0,
        provider_name: "mock-test".to_string(),
        base_url: test_base_url,
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
        api_key: "sk-test".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_api_presets_and_proxy_settings(
        vec![active_preset.clone(), test_preset.clone()],
        UpstreamProxySettings {
            codex_api_proxy_enabled: false,
            active_api_proxy_preset_id: Some(active_preset.id),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .header(UPSTREAM_PRESET_ID_HEADER, &test_preset.id)
        .body(Body::empty())
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(active_requests.lock().unwrap().len(), 0);
    assert_eq!(test_requests.lock().unwrap().clone(), vec!["GET /v1/models HTTP/1.1"]);
}

#[tokio::test]
async fn codex_proxy_route_uses_preset_id_from_local_proxy_bearer_token() {
    let (old_base_url, old_requests) = spawn_openai_mock_server().await;
    let (new_base_url, new_requests) = spawn_openai_mock_server().await;
    let old_preset = StoredApiPreset {
        id: "api-old-local".to_string(),
        name: "Old Local Proxy".to_string(),
        saved_at: 0,
        provider_name: "mock-old".to_string(),
        base_url: old_base_url,
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
        api_key: "sk-old".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let new_preset = StoredApiPreset {
        id: "api-new-local".to_string(),
        name: "New Local Proxy".to_string(),
        saved_at: 0,
        provider_name: "mock-new".to_string(),
        base_url: new_base_url,
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
        api_key: "sk-new".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_api_presets_and_proxy_settings(
        vec![old_preset.clone(), new_preset.clone()],
        UpstreamProxySettings {
            active_api_proxy_preset_id: Some(old_preset.id.clone()),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .header(
            "authorization",
            format!("Bearer {}", local_proxy_api_key_for_preset_id(&new_preset.id)),
        )
        .body(Body::empty())
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(old_requests.lock().unwrap().len(), 0);
    assert_eq!(new_requests.lock().unwrap().clone(), vec!["GET /v1/models HTTP/1.1"]);
}

#[tokio::test]
async fn claude_proxy_route_keeps_forwarding_when_toggle_disabled_after_apply() {
    let (base_url, requests) = spawn_anthropic_mock_server().await;
    let preset = StoredClaudePreset {
        id: "claude-local-proxy".to_string(),
        name: "Claude Local Proxy Preset".to_string(),
        saved_at: 0,
        provider_name: "mock-claude".to_string(),
        base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-ant-test".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: None,
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_claude_proxy_settings(
        preset.clone(),
        UpstreamProxySettings {
            claude_proxy_enabled: false,
            active_claude_proxy_preset_id: Some(preset.id),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/upstream/anthropic/v1/messages")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"model":"mock","messages":[]}"#))
        .unwrap();

    let response = anthropic_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(std::str::from_utf8(&body).unwrap(), r#"{"content":[]}"#);
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests, vec!["POST /v1/messages HTTP/1.1"]);
}

#[tokio::test]
async fn claude_proxy_route_uses_preset_id_from_local_proxy_x_api_key() {
    let (old_base_url, old_requests) = spawn_anthropic_mock_server().await;
    let (new_base_url, new_requests) = spawn_anthropic_mock_server().await;
    let old_preset = StoredClaudePreset {
        id: "claude-old-local".to_string(),
        name: "Old Claude Local Proxy".to_string(),
        saved_at: 0,
        provider_name: "mock-old-claude".to_string(),
        base_url: old_base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-old-ant".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: None,
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let new_preset = StoredClaudePreset {
        id: "claude-new-local".to_string(),
        name: "New Claude Local Proxy".to_string(),
        saved_at: 0,
        provider_name: "mock-new-claude".to_string(),
        base_url: new_base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-new-ant".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: None,
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_claude_presets_and_proxy_settings(
        vec![old_preset.clone(), new_preset.clone()],
        UpstreamProxySettings {
            active_claude_proxy_preset_id: Some(old_preset.id.clone()),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/upstream/anthropic/v1/messages")
        .header("content-type", "application/json")
        .header("x-api-key", local_proxy_claude_token_for_preset_id(&new_preset.id))
        .body(Body::from(r#"{"model":"mock","messages":[]}"#))
        .unwrap();

    let response = anthropic_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(old_requests.lock().unwrap().len(), 0);
    assert_eq!(new_requests.lock().unwrap().clone(), vec!["POST /v1/messages HTTP/1.1"]);
}

#[tokio::test]
async fn claude_dynamic_relay_token_routes_to_request_model_before_active_preset() {
    let (minimax_base_url, minimax_requests) = spawn_anthropic_mock_server().await;
    let (glm_base_url, glm_requests) = spawn_anthropic_mock_server().await;
    let minimax_preset = StoredClaudePreset {
        id: "claude-minimax".to_string(),
        name: "MiniMax".to_string(),
        saved_at: 0,
        provider_name: "mock-minimax".to_string(),
        base_url: minimax_base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-minimax".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("MiniMax-M2.7".to_string()),
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let glm_preset = StoredClaudePreset {
        id: "claude-glm".to_string(),
        name: "GLM".to_string(),
        saved_at: 0,
        provider_name: "mock-glm".to_string(),
        base_url: glm_base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-glm".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: Some("GLM-5.2".to_string()),
        third_party_model: None,
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_claude_presets_and_proxy_settings(
        vec![minimax_preset, glm_preset.clone()],
        UpstreamProxySettings {
            active_claude_proxy_preset_id: Some(glm_preset.id),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/upstream/anthropic/v1/messages")
        .header("content-type", "application/json")
        .header("x-api-key", auth_core::LOCAL_PROXY_CLAUDE_TOKEN)
        .body(Body::from(
            r#"{"model":"MiniMax-M2.7","messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();

    let response = anthropic_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(glm_requests.lock().unwrap().len(), 0);
    assert_eq!(minimax_requests.lock().unwrap().clone(), vec!["POST /v1/messages HTTP/1.1"]);
}

#[tokio::test]
async fn claude_openai_chat_conversion_routes_messages_to_chat_completions() {
    let (base_url, requests) = spawn_openai_chat_mock_server().await;
    let preset = StoredClaudePreset {
        id: "claude-openai-chat".to_string(),
        name: "OpenAI Chat For Claude".to_string(),
        saved_at: 0,
        provider_name: "mock-openai-chat".to_string(),
        base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-openai-chat".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("gpt-4.1".to_string()),
        use_local_proxy: true,
        access_mode: Some(auth_core::ClaudeAccessMode::OpenaiChat),
        switch_count: 0,
    };
    let state = test_state_with_claude_presets_and_proxy_settings(
        vec![preset.clone()],
        UpstreamProxySettings {
            active_claude_proxy_preset_id: Some(preset.id),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/upstream/anthropic/v1/messages")
        .header("content-type", "application/json")
        .header("x-api-key", auth_core::LOCAL_PROXY_CLAUDE_TOKEN)
        .body(Body::from(
            r#"{"model":"gpt-4.1","max_tokens":128,"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#,
        ))
        .unwrap();

    let response = anthropic_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 2048).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["type"], "message");
    assert_eq!(payload["content"][0]["type"], "text");
    assert_eq!(payload["content"][0]["text"], "hello from chat");
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("POST /v1/chat/completions HTTP/1.1"), "{}", requests[0]);
    assert!(requests[0].contains(r#""role":"user""#), "{}", requests[0]);
    assert!(requests[0].contains(r#""content":"hi""#), "{}", requests[0]);
    assert!(
        requests[0].contains("authorization: Bearer sk-openai-chat")
            || requests[0].contains("Authorization: Bearer sk-openai-chat"),
        "{}",
        requests[0]
    );
}

#[tokio::test]
async fn claude_openai_responses_conversion_routes_messages_to_responses() {
    let (base_url, requests) = spawn_openai_responses_mock_server().await;
    let preset = StoredClaudePreset {
        id: "claude-openai-responses".to_string(),
        name: "OpenAI Responses For Claude".to_string(),
        saved_at: 0,
        provider_name: "mock-openai-responses".to_string(),
        base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-openai-responses".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("gpt-5.1".to_string()),
        use_local_proxy: true,
        access_mode: Some(auth_core::ClaudeAccessMode::OpenaiResponses),
        switch_count: 0,
    };
    let state = test_state_with_claude_presets_and_proxy_settings(
        vec![preset.clone()],
        UpstreamProxySettings {
            active_claude_proxy_preset_id: Some(preset.id),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/upstream/anthropic/v1/messages")
        .header("content-type", "application/json")
        .header("x-api-key", auth_core::LOCAL_PROXY_CLAUDE_TOKEN)
        .body(Body::from(
            r#"{"model":"gpt-5.1","max_tokens":128,"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#,
        ))
        .unwrap();

    let response = anthropic_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 2048).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["type"], "message");
    assert_eq!(payload["content"][0]["type"], "text");
    assert_eq!(payload["content"][0]["text"], "hello from responses");
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("POST /v1/responses HTTP/1.1"), "{}", requests[0]);
    assert!(requests[0].contains(r#""role":"user""#), "{}", requests[0]);
    assert!(requests[0].contains(r#""text":"hi""#), "{}", requests[0]);
    assert!(
        requests[0].contains("authorization: Bearer sk-openai-responses")
            || requests[0].contains("Authorization: Bearer sk-openai-responses"),
        "{}",
        requests[0]
    );
}

#[tokio::test]
async fn claude_proxy_route_uses_request_model_before_active_fallback() {
    let (minimax_base_url, minimax_requests) = spawn_anthropic_mock_server().await;
    let (glm_base_url, glm_requests) = spawn_anthropic_mock_server().await;
    let minimax_preset = StoredClaudePreset {
        id: "claude-minimax".to_string(),
        name: "MiniMax".to_string(),
        saved_at: 0,
        provider_name: "mock-minimax".to_string(),
        base_url: minimax_base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-minimax".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("MiniMax-M2.7".to_string()),
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let glm_preset = StoredClaudePreset {
        id: "claude-glm".to_string(),
        name: "GLM".to_string(),
        saved_at: 0,
        provider_name: "mock-glm".to_string(),
        base_url: glm_base_url,
        management_url: None,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "sk-glm".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: Some("GLM-5.2".to_string()),
        third_party_model: None,
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_claude_presets_and_proxy_settings(
        vec![minimax_preset, glm_preset.clone()],
        UpstreamProxySettings {
            active_claude_proxy_preset_id: Some(glm_preset.id),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/upstream/anthropic/v1/messages")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"model":"MiniMax-M2.7","messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();

    let response = anthropic_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(glm_requests.lock().unwrap().len(), 0);
    assert_eq!(minimax_requests.lock().unwrap().clone(), vec!["POST /v1/messages HTTP/1.1"]);
}

fn test_state_with_api_proxy_settings(
    preset: StoredApiPreset,
    upstream_proxy_settings: UpstreamProxySettings,
) -> AppState {
    test_state_with_api_presets_and_proxy_settings(vec![preset], upstream_proxy_settings)
}

fn test_state_with_api_presets_and_proxy_settings(
    presets: Vec<StoredApiPreset>,
    upstream_proxy_settings: UpstreamProxySettings,
) -> AppState {
    let app_dir = unique_temp_dir("webclx-upstream-proxy-test");
    std::fs::create_dir_all(&app_dir).unwrap();
    let auth_manager = auth::AuthPresetManager::load(&app_dir).unwrap();
    auth_manager.replace_api_presets(presets);
    auth_manager.replace_upstream_proxy_settings(upstream_proxy_settings);

    AppState {
        static_dir: app_dir.join("static"),
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        version: "1.0.531".to_string(),
        app_dir: app_dir.clone(),
        local_api_token: std::sync::Arc::from("test-local-api-token"),
        workspace_settings: settings::SettingsManager::load(&app_dir).unwrap(),
        auth_manager,
        codex_oauth_manager: auth::CodexOAuthManager::new(),
        codex_proxy_history: codex_proxy::CodexProxyHistory::new(),
        proxy_manager: proxy::ProxyManager::load(&app_dir).unwrap(),
        quota_reset_cache: crate::quota_reset_cache::QuotaResetCache::new(),
        quota_manager: crate::quota::QuotaConfigManager::load(&app_dir),
        frpc_manager: FrpcManager::load(&app_dir, 0).unwrap(),
        frps_manager: FrpsManager::load(&app_dir).unwrap(),
        frp_role_manager: FrpRoleManager::load(&app_dir, 0).unwrap(),
        terminal_manager: terminal::TerminalManager::new(
            app_dir.join(".webclx-terminal-sessions.json"),
        ),
        preset_test_scheduler: auth::PresetTestScheduler::new(
            &app_dir.join(".webclx-terminal-sessions.json"),
        ),
        preset_run_lease_manager: auth::PresetRunLeaseManager::new(
            app_dir.join(".webclx-preset-run-lease.json"),
        ),
        agent_manager: crate::agent::AgentManager::new(&app_dir),
        agent_config: crate::agent::AgentConfigManager::new(&app_dir),
    }
}

fn test_state_with_claude_proxy_settings(
    preset: StoredClaudePreset,
    upstream_proxy_settings: UpstreamProxySettings,
) -> AppState {
    test_state_with_claude_presets_and_proxy_settings(vec![preset], upstream_proxy_settings)
}

fn test_state_with_claude_presets_and_proxy_settings(
    presets: Vec<StoredClaudePreset>,
    upstream_proxy_settings: UpstreamProxySettings,
) -> AppState {
    let app_dir = unique_temp_dir("webclx-claude-upstream-proxy-test");
    std::fs::create_dir_all(&app_dir).unwrap();
    let auth_manager = auth::AuthPresetManager::load(&app_dir).unwrap();
    auth_manager.replace_claude_presets(presets);
    auth_manager.replace_upstream_proxy_settings(upstream_proxy_settings);

    AppState {
        static_dir: app_dir.join("static"),
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        version: "1.0.531".to_string(),
        app_dir: app_dir.clone(),
        local_api_token: std::sync::Arc::from("test-local-api-token"),
        workspace_settings: settings::SettingsManager::load(&app_dir).unwrap(),
        auth_manager,
        codex_oauth_manager: auth::CodexOAuthManager::new(),
        codex_proxy_history: codex_proxy::CodexProxyHistory::new(),
        proxy_manager: proxy::ProxyManager::load(&app_dir).unwrap(),
        quota_reset_cache: crate::quota_reset_cache::QuotaResetCache::new(),
        quota_manager: crate::quota::QuotaConfigManager::load(&app_dir),
        frpc_manager: FrpcManager::load(&app_dir, 0).unwrap(),
        frps_manager: FrpsManager::load(&app_dir).unwrap(),
        frp_role_manager: FrpRoleManager::load(&app_dir, 0).unwrap(),
        terminal_manager: terminal::TerminalManager::new(
            app_dir.join(".webclx-terminal-sessions.json"),
        ),
        preset_test_scheduler: auth::PresetTestScheduler::new(
            &app_dir.join(".webclx-terminal-sessions.json"),
        ),
        preset_run_lease_manager: auth::PresetRunLeaseManager::new(
            app_dir.join(".webclx-preset-run-lease.json"),
        ),
        agent_manager: crate::agent::AgentManager::new(&app_dir),
        agent_config: crate::agent::AgentConfigManager::new(&app_dir),
    }
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{label}-{nanos}"))
}

#[tokio::test]
async fn openai_upstream_proxy_forwards_client_bearer_instead_of_preset_key() {
    // 客户端带真实非占位 Bearer → 上游应收到客户端 token，不是预设的 sk-test。
    let (base_url, requests) = spawn_openai_auth_capturing_mock_server().await;
    let preset = StoredApiPreset {
        id: "api-preset-key-test".to_string(),
        name: "Preset Key Test".to_string(),
        saved_at: 0,
        provider_name: "mock-openai".to_string(),
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
        api_key: "sk-preset-fallback".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_api_proxy_settings(
        preset,
        UpstreamProxySettings {
            active_api_proxy_preset_id: Some("api-preset-key-test".to_string()),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .header("Authorization", "Bearer sk-client-provided")
        .body(Body::empty())
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let captured = requests.lock().unwrap().join("\n");
    assert!(
        captured.contains("Bearer sk-client-provided"),
        "上游应收到客户端凭据, 实际: {captured}"
    );
    assert!(
        !captured.contains("sk-preset-fallback"),
        "上游不应收到预设凭据, 实际: {captured}"
    );
}

#[tokio::test]
async fn openai_upstream_proxy_uses_preset_key_when_client_sends_placeholder_token() {
    // 占位 token (webclx-local-api-proxy:<id>) 是预设身份标识, 必须回到预设凭据。
    let (base_url, requests) = spawn_openai_auth_capturing_mock_server().await;
    let preset = StoredApiPreset {
        id: "api-placeholder-test".to_string(),
        name: "Placeholder Test".to_string(),
        saved_at: 0,
        provider_name: "mock-openai".to_string(),
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
        api_key: "sk-preset-fallback".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let preset_id = preset.id.clone();
    let state = test_state_with_api_proxy_settings(
        preset,
        UpstreamProxySettings {
            active_api_proxy_preset_id: Some(preset_id.clone()),
            ..Default::default()
        },
    );
    let placeholder = local_proxy_api_key_for_preset_id(&preset_id);
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .header("Authorization", format!("Bearer {placeholder}"))
        .body(Body::empty())
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let captured = requests.lock().unwrap().join("\n");
    assert!(
        captured.contains("Bearer sk-preset-fallback"),
        "占位 token 应触发预设凭据, 实际: {captured}"
    );
    assert!(!captured.contains(&placeholder), "占位 token 不应透传给上游, 实际: {captured}");
}

#[tokio::test]
async fn openai_upstream_proxy_uses_preset_key_when_client_sends_no_credential() {
    // 无凭据 → 预设兜底 (旧的本机直连客户端行为不变)。
    let (base_url, requests) = spawn_openai_auth_capturing_mock_server().await;
    let preset = StoredApiPreset {
        id: "api-no-cred-test".to_string(),
        name: "No Cred Test".to_string(),
        saved_at: 0,
        provider_name: "mock-openai".to_string(),
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
        api_key: "sk-preset-fallback".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_api_proxy_settings(
        preset,
        UpstreamProxySettings {
            active_api_proxy_preset_id: Some("api-no-cred-test".to_string()),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .body(Body::empty())
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let captured = requests.lock().unwrap().join("\n");
    assert!(
        captured.contains("Bearer sk-preset-fallback"),
        "无凭据应兜底预设, 实际: {captured}"
    );
}

#[tokio::test]
async fn openai_upstream_proxy_injects_account_id_and_access_token_for_oauth_preset() {
    // OAuth 预设 → 使用 preset.base_url 上游 + 注入 access_token + account_id。
    let (base_url, requests) = spawn_openai_auth_capturing_mock_server().await;
    let preset = StoredApiPreset {
        id: "api-oauth-test".to_string(),
        name: "OAuth Account".to_string(),
        saved_at: 0,
        provider_name: "ChatGPT".to_string(),
        base_url: format!("{base_url}/codex"),
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
        api_key: "placeholder".to_string(),
        access_token: "oauth-access-tok".to_string(),
        account_id: "acct-oauth-123".to_string(),
        access_mode: Some(ApiAccessMode::ChatgptOauth),
        switch_count: 0,
    };
    let state = test_state_with_api_proxy_settings(
        preset,
        UpstreamProxySettings {
            active_api_proxy_preset_id: Some("api-oauth-test".to_string()),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .body(Body::empty())
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("127.0.0.1:11111".parse().unwrap()),
        request,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let captured = requests.lock().unwrap().join("\n");
    assert!(
        captured.contains("Bearer oauth-access-tok"),
        "OAuth 预设应使用 access_token 作为 Bearer, 实际: {captured}"
    );
    assert!(
        captured.contains("chatgpt-account-id: acct-oauth-123"),
        "OAuth 预设应注入 ChatGPT-Account-Id, 实际: {captured}"
    );
    // 不应发送 placeholder api_key
    assert!(
        !captured.contains("placeholder"),
        "OAuth 预设不应发送 placeholder api_key, 实际: {captured}"
    );
}

#[tokio::test]
async fn openai_upstream_proxy_rejects_non_loopback_when_gateway_disabled() {
    // gateway_listen_non_loopback=false (默认) + 非 loopback → 403。
    let (base_url, _requests) = spawn_openai_mock_server().await;
    let preset = StoredApiPreset {
        id: "api-gateway-off-test".to_string(),
        name: "Gateway Off Test".to_string(),
        saved_at: 0,
        provider_name: "mock-openai".to_string(),
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
        api_key: "sk-test".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_api_proxy_settings(
        preset,
        UpstreamProxySettings {
            active_api_proxy_preset_id: Some("api-gateway-off-test".to_string()),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .body(Body::empty())
        .unwrap();

    let result = openai_upstream_proxy(
        State(state),
        ConnectInfo("192.168.1.50:44444".parse().unwrap()),
        request,
    )
    .await;
    assert!(result.is_err(), "非 loopback 在网关关闭时应被拒绝");
    let err = result.unwrap_err();
    assert_eq!(err.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn openai_upstream_proxy_allows_non_loopback_when_gateway_enabled() {
    // gateway_listen_non_loopback=true + 非 loopback → 放行。
    let (base_url, requests) = spawn_openai_auth_capturing_mock_server().await;
    let preset = StoredApiPreset {
        id: "api-gateway-on-test".to_string(),
        name: "Gateway On Test".to_string(),
        saved_at: 0,
        provider_name: "mock-openai".to_string(),
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
        api_key: "sk-preset-fallback".to_string(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    let state = test_state_with_gateway_enabled_api_proxy_settings(
        preset,
        UpstreamProxySettings {
            active_api_proxy_preset_id: Some("api-gateway-on-test".to_string()),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/upstream/openai/v1/models")
        .header("Authorization", "Bearer sk-client-provided")
        .body(Body::empty())
        .unwrap();

    let response = openai_upstream_proxy(
        State(state),
        ConnectInfo("192.168.1.50:44444".parse().unwrap()),
        request,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let captured = requests.lock().unwrap().join("\n");
    assert!(
        captured.contains("Bearer sk-client-provided"),
        "网关开启时非 loopback 应放行并透传客户端凭据, 实际: {captured}"
    );
}

async fn spawn_openai_mock_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buffer = vec![0_u8; 4096];
        let Ok(read) = stream.read(&mut buffer).await else {
            return;
        };
        let request = String::from_utf8_lossy(&buffer[..read]);
        let request_line = request.lines().next().unwrap_or("").to_string();
        requests_for_task.lock().unwrap().push(request_line);
        let body = r#"{"data":[{"id":"mock-model"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    (format!("http://{addr}/v1"), requests)
}

async fn spawn_openai_chat_mock_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buffer = vec![0_u8; 8192];
        let Ok(read) = stream.read(&mut buffer).await else {
            return;
        };
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        requests_for_task.lock().unwrap().push(request);
        let body = r#"{"id":"chat_1","object":"chat.completion","model":"gpt-4.1","choices":[{"index":0,"message":{"role":"assistant","content":"hello from chat"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    (format!("http://{addr}/v1"), requests)
}

async fn spawn_openai_responses_mock_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buffer = vec![0_u8; 8192];
        let Ok(read) = stream.read(&mut buffer).await else {
            return;
        };
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        requests_for_task.lock().unwrap().push(request);
        let body = r#"{"id":"resp_1","object":"response","model":"gpt-5.1","output":[{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"output_text","text":"hello from responses"}]}],"usage":{"input_tokens":3,"output_tokens":4,"total_tokens":7}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    (format!("http://{addr}/v1"), requests)
}

async fn spawn_anthropic_mock_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buffer = vec![0_u8; 4096];
        let Ok(read) = stream.read(&mut buffer).await else {
            return;
        };
        let request = String::from_utf8_lossy(&buffer[..read]);
        let request_line = request.lines().next().unwrap_or("").to_string();
        requests_for_task.lock().unwrap().push(request_line);
        let body = r#"{"content":[]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    (format!("http://{addr}"), requests)
}

/// 捕获完整请求(含 Authorization 头)的 OpenAI mock,用于验证客户端凭据透传。
async fn spawn_openai_auth_capturing_mock_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buffer = vec![0_u8; 8192];
        let Ok(read) = stream.read(&mut buffer).await else {
            return;
        };
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        requests_for_task.lock().unwrap().push(request);
        let body = r#"{"data":[{"id":"mock-model"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
    });

    (format!("http://{addr}/v1"), requests)
}

/// 构造一个 gateway_listen_non_loopback=true 的测试 state,用于验证对外网关放行。
fn test_state_with_gateway_enabled_api_proxy_settings(
    preset: StoredApiPreset,
    upstream_proxy_settings: UpstreamProxySettings,
) -> AppState {
    let app_dir = unique_temp_dir("webclx-gateway-enabled-test");
    std::fs::create_dir_all(&app_dir).unwrap();
    // 写入一份含 gateway_listen_non_loopback=true 的 settings 文件,让
    // SettingsManager::load 读到开关为 true。其余字段走 serde default。
    std::fs::write(
        app_dir.join("webclx-settings.json"),
        r#"{"workspace_dir":"/home","terminal_user":"root","gateway_listen_non_loopback":true}"#,
    )
    .unwrap();
    let auth_manager = auth::AuthPresetManager::load(&app_dir).unwrap();
    auth_manager.replace_api_presets(vec![preset]);
    auth_manager.replace_upstream_proxy_settings(upstream_proxy_settings);

    AppState {
        static_dir: app_dir.join("static"),
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        version: "1.0.531".to_string(),
        app_dir: app_dir.clone(),
        local_api_token: std::sync::Arc::from("test-local-api-token"),
        workspace_settings: settings::SettingsManager::load(&app_dir).unwrap(),
        auth_manager,
        codex_oauth_manager: auth::CodexOAuthManager::new(),
        codex_proxy_history: codex_proxy::CodexProxyHistory::new(),
        proxy_manager: proxy::ProxyManager::load(&app_dir).unwrap(),
        quota_reset_cache: crate::quota_reset_cache::QuotaResetCache::new(),
        quota_manager: crate::quota::QuotaConfigManager::load(&app_dir),
        frpc_manager: FrpcManager::load(&app_dir, 0).unwrap(),
        frps_manager: FrpsManager::load(&app_dir).unwrap(),
        frp_role_manager: FrpRoleManager::load(&app_dir, 0).unwrap(),
        terminal_manager: terminal::TerminalManager::new(
            app_dir.join(".webclx-terminal-sessions.json"),
        ),
        preset_test_scheduler: auth::PresetTestScheduler::new(
            &app_dir.join(".webclx-terminal-sessions.json"),
        ),
        preset_run_lease_manager: auth::PresetRunLeaseManager::new(
            app_dir.join(".webclx-preset-run-lease.json"),
        ),
        agent_manager: crate::agent::AgentManager::new(&app_dir),
        agent_config: crate::agent::AgentConfigManager::new(&app_dir),
    }
}
