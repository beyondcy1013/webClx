use std::time::{Duration, Instant};

use auth_core::*;
use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use reqwest::{StatusCode, header};
use serde_json::{Value, json};

use super::{
    ANTHROPIC_API_VERSION, PRESET_CHAT_PROBE_DELAY, PRESET_TEST_TIMEOUT_SECS,
    PresetBatchTestResponse, PresetTestResponse, PresetTestResult, UPSTREAM_PRESET_ID_HEADER,
};
use crate::{
    ApiResult, AppError, AppState, llm,
    llm::environment::{LlmHttpContext, LlmHttpEnvironment},
};

const CODEX_OAUTH_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const DEFAULT_CODEX_OAUTH_TEST_MODEL: &str = "gpt-5.4";

#[cfg(test)]
pub(in crate::auth) use crate::llm::environment::build_llm_client_from_env as build_preset_test_client_from_env;

pub(in crate::auth) struct PresetTestEnvironment(LlmHttpEnvironment);

impl PresetTestEnvironment {
    pub(in crate::auth) async fn capture(
        proxy_manager: &crate::proxy::ProxyManager,
        workspace_settings: &settings_core::SettingsManager,
    ) -> ApiResult<Self> {
        LlmHttpEnvironment::capture(proxy_manager, workspace_settings)
            .await
            .map(Self)
    }

    pub(in crate::auth) fn context_for(
        &self,
        preset_env: &[PresetTerminalEnvVar],
    ) -> ApiResult<LlmHttpContext> {
        self.0
            .context_for(preset_env, Duration::from_secs(PRESET_TEST_TIMEOUT_SECS))
    }
}

pub(in crate::auth) fn annotate_preset_test_result(
    context: &LlmHttpContext,
    mut result: PresetTestResult,
    access_mode: &str,
) -> PresetTestResult {
    result.message = format!(
        "访问模式：{access_mode}\n测试网络：{}\n{}",
        context.environment_summary, result.message
    );
    result
}

pub async fn test_auth_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<PresetTestResponse>> {
    let presets = state.auth_manager.auth_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("找不到指定的 OAuth 预设。"))?;
    let (client, proxy_summary) = require_active_oauth_test_proxy(&state)?;
    let model = resolve_auth_preset_test_model(&state, &preset)?;
    let result = annotate_oauth_proxy_result(
        test_stored_auth_preset_with_endpoint(&client, &preset, &model, CODEX_OAUTH_RESPONSES_URL)
            .await,
        &proxy_summary,
    );
    Ok(Json(PresetTestResponse {
        ok: result.ok,
        result,
    }))
}

pub async fn test_all_auth_presets(
    State(state): State<AppState>,
) -> ApiResult<Json<PresetBatchTestResponse>> {
    let presets = state.auth_manager.auth_presets_snapshot();
    let (client, proxy_summary) = require_active_oauth_test_proxy(&state)?;
    let mut results = Vec::with_capacity(presets.len());
    for preset in &presets {
        let model = resolve_auth_preset_test_model(&state, preset)?;
        results.push(annotate_oauth_proxy_result(
            test_stored_auth_preset_with_endpoint(
                &client,
                preset,
                &model,
                CODEX_OAUTH_RESPONSES_URL,
            )
            .await,
            &proxy_summary,
        ));
    }
    Ok(Json(build_preset_batch_test_response(results)))
}

fn require_active_oauth_test_proxy(state: &AppState) -> ApiResult<(reqwest::Client, String)> {
    let proxy_summary = require_active_oauth_test_proxy_summary(state)?;
    let client = state
        .proxy_manager
        .build_auth_client(PRESET_TEST_TIMEOUT_SECS)
        .map_err(|error| AppError::internal(format!("创建 OAuth 测试客户端失败: {error}")))?;
    Ok((client, proxy_summary))
}

fn require_active_oauth_test_proxy_summary(state: &AppState) -> ApiResult<String> {
    state
        .proxy_manager
        .get_active()
        .map(|proxy| proxy.network_summary())
        .ok_or_else(|| AppError::bad_request("ChatGPT OAuth 测试要求先应用一个程序代理。"))
}

fn build_local_relay_test_client() -> ApiResult<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(PRESET_TEST_TIMEOUT_SECS))
        .build()
        .map_err(|error| AppError::internal(format!("创建本地中继测试客户端失败: {error}")))
}

fn annotate_oauth_proxy_result(
    mut result: PresetTestResult,
    proxy_summary: &str,
) -> PresetTestResult {
    result.message = format!("测试网络：应用代理 {proxy_summary}\n{}", result.message);
    result
}

pub async fn test_api_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<PresetTestResponse>> {
    let presets = state.auth_manager.api_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("找不到指定的 API 预设。"))?;
    if preset.access_mode == Some(ApiAccessMode::ChatgptOauth) {
        let proxy_summary = require_active_oauth_test_proxy_summary(&state)?;
        let client = build_local_relay_test_client()?;
        let model = resolve_chatgpt_oauth_api_test_model(&state, &preset)?;
        let result =
            test_stored_chatgpt_oauth_api_preset_via_local_relay(&client, &preset, &model).await;
        return Ok(Json(PresetTestResponse {
            ok: result.ok,
            result: annotate_chatgpt_oauth_api_result(result, &proxy_summary),
        }));
    }
    let environment =
        PresetTestEnvironment::capture(&state.proxy_manager, &state.workspace_settings).await?;
    let context = environment.context_for(&preset.terminal_env)?;
    let default_config_entries = state.workspace_settings.codex_default_config_entries();
    let default_config_pairs = default_config_entries
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<Vec<_>>();
    let result = test_stored_api_preset(&context.client, &preset, &default_config_pairs).await?;
    let access_mode = if api_preset_enables_local_upstream_proxy_on_apply(&preset) {
        "本地中继"
    } else {
        "直连上游"
    };
    let result = annotate_preset_test_result(&context, result, access_mode);
    Ok(Json(PresetTestResponse {
        ok: result.ok,
        result,
    }))
}

fn resolve_auth_preset_test_model(
    state: &AppState,
    preset: &StoredAuthPreset,
) -> ApiResult<String> {
    let default_config_entries = state.workspace_settings.codex_default_config_entries();
    let default_config_pairs = default_config_entries
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<Vec<_>>();
    let config_targets =
        resolve_effective_preset_config_targets(&default_config_pairs, &preset.config_overrides)
            .map_err(|error| AppError::internal(format!("OAuth 预设 config 覆盖无效: {error}")))?;
    Ok(model_from_config_targets(&config_targets)
        .unwrap_or_else(|| DEFAULT_CODEX_OAUTH_TEST_MODEL.to_string()))
}

fn resolve_chatgpt_oauth_api_test_model(
    state: &AppState,
    preset: &StoredApiPreset,
) -> ApiResult<String> {
    let default_config_entries = state.workspace_settings.codex_default_config_entries();
    let default_config_pairs = default_config_entries
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<Vec<_>>();
    let config_targets =
        resolve_effective_preset_config_targets(&default_config_pairs, &preset.config_overrides)
            .map_err(|error| AppError::internal(format!("API 预设 config 覆盖无效: {error}")))?;
    Ok(model_from_config_targets(&config_targets)
        .unwrap_or_else(|| DEFAULT_CODEX_OAUTH_TEST_MODEL.to_string()))
}

pub(crate) async fn test_stored_auth_preset_with_endpoint(
    client: &reqwest::Client,
    preset: &StoredAuthPreset,
    model: &str,
    endpoint: &str,
) -> PresetTestResult {
    match llm::call_responses_probe(
        client,
        endpoint,
        &preset.auth.tokens.access_token,
        Some(&preset.auth.tokens.account_id),
        model,
        "hi",
    )
    .await
    {
        Ok(reply) => PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: true,
            endpoint: reply.endpoint,
            status: Some(reply.status),
            latency_ms: reply.latency_ms,
            message: format!(
                "Codex OAuth 对话测试成功。可读内容：{}",
                reply.content.as_deref().unwrap_or("服务器未返回文本")
            ),
        },
        Err(error) => PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: false,
            endpoint: error.endpoint,
            status: error.status,
            latency_ms: error.latency_ms,
            message: error.message,
        },
    }
}

#[cfg(test)]
pub(crate) async fn test_stored_chatgpt_oauth_api_preset_with_endpoint(
    client: &reqwest::Client,
    preset: &StoredApiPreset,
    model: &str,
    endpoint: &str,
) -> PresetTestResult {
    match llm::call_responses_probe(
        client,
        endpoint,
        &preset.access_token,
        Some(&preset.account_id),
        model,
        "hi",
    )
    .await
    {
        Ok(reply) => PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: true,
            endpoint: reply.endpoint,
            status: Some(reply.status),
            latency_ms: reply.latency_ms,
            message: format!(
                "ChatGPT OAuth 对话测试成功。可读内容：{}",
                reply.content.as_deref().unwrap_or("服务器未返回文本")
            ),
        },
        Err(error) => PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: false,
            endpoint: error.endpoint,
            status: error.status,
            latency_ms: error.latency_ms,
            message: error.message,
        },
    }
}

async fn test_stored_chatgpt_oauth_api_preset_via_local_relay(
    client: &reqwest::Client,
    preset: &StoredApiPreset,
    model: &str,
) -> PresetTestResult {
    let endpoint = llm::responses_url(&api_provider_base_url_for_mode(preset, true));
    match llm::call_responses_probe(
        client,
        &endpoint,
        &local_proxy_api_key_for_preset_id(&preset.id),
        None,
        model,
        "hi",
    )
    .await
    {
        Ok(reply) => PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: true,
            endpoint: reply.endpoint,
            status: Some(reply.status),
            latency_ms: reply.latency_ms,
            message: format!(
                "ChatGPT OAuth 对话测试成功。可读内容：{}",
                reply.content.as_deref().unwrap_or("服务器未返回文本")
            ),
        },
        Err(error) => PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: false,
            endpoint: error.endpoint,
            status: error.status,
            latency_ms: error.latency_ms,
            message: error.message,
        },
    }
}

fn annotate_chatgpt_oauth_api_result(
    mut result: PresetTestResult,
    proxy_summary: &str,
) -> PresetTestResult {
    result.message = format!(
        "访问模式：本地中继\nLLM 代理：webClx 本地中继\n网络代理：应用代理 {proxy_summary}\n{}",
        result.message
    );
    result
}

pub async fn test_all_api_presets(
    State(state): State<AppState>,
) -> ApiResult<Json<PresetBatchTestResponse>> {
    let presets = state.auth_manager.api_presets_snapshot();
    let mut environment = None;
    let default_config_entries = state.workspace_settings.codex_default_config_entries();
    let default_config_pairs = default_config_entries
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<Vec<_>>();
    let mut results = Vec::with_capacity(presets.len());
    for preset in &presets {
        if preset.access_mode == Some(ApiAccessMode::ChatgptOauth) {
            let proxy_summary = require_active_oauth_test_proxy_summary(&state)?;
            let client = build_local_relay_test_client()?;
            let model = resolve_chatgpt_oauth_api_test_model(&state, preset)?;
            let result =
                test_stored_chatgpt_oauth_api_preset_via_local_relay(&client, preset, &model).await;
            results.push(annotate_chatgpt_oauth_api_result(result, &proxy_summary));
            continue;
        }
        if environment.is_none() {
            environment = Some(
                PresetTestEnvironment::capture(&state.proxy_manager, &state.workspace_settings)
                    .await?,
            );
        }
        let context = environment
            .as_ref()
            .expect("preset test environment initialized")
            .context_for(&preset.terminal_env)?;
        let result = test_stored_api_preset(&context.client, preset, &default_config_pairs).await?;
        let access_mode = if api_preset_enables_local_upstream_proxy_on_apply(preset) {
            "本地中继"
        } else {
            "直连上游"
        };
        results.push(annotate_preset_test_result(&context, result, access_mode));
    }
    Ok(Json(build_preset_batch_test_response(results)))
}

pub async fn test_claude_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<PresetTestResponse>> {
    let presets = state.auth_manager.claude_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("找不到指定的 Claude 预设。"))?;
    let effective_preset =
        super::claude_preset_with_global_defaults(&state.workspace_settings, &preset)?;
    let environment =
        PresetTestEnvironment::capture(&state.proxy_manager, &state.workspace_settings).await?;
    let context = environment.context_for(&[])?;
    let use_local_proxy = effective_claude_use_local_proxy(&effective_preset);
    let result =
        test_stored_claude_preset(&context.client, &effective_preset, use_local_proxy).await;
    let result = annotate_preset_test_result(
        &context,
        result,
        if use_local_proxy {
            "本地中继"
        } else {
            "直连上游"
        },
    );
    Ok(Json(PresetTestResponse {
        ok: result.ok,
        result,
    }))
}

pub async fn test_all_claude_presets(
    State(state): State<AppState>,
) -> ApiResult<Json<PresetBatchTestResponse>> {
    let presets = state.auth_manager.claude_presets_snapshot();
    let environment =
        PresetTestEnvironment::capture(&state.proxy_manager, &state.workspace_settings).await?;
    let mut results = Vec::with_capacity(presets.len());
    for preset in &presets {
        let effective_preset =
            super::claude_preset_with_global_defaults(&state.workspace_settings, preset)?;
        let context = environment.context_for(&[])?;
        let use_local_proxy = effective_claude_use_local_proxy(&effective_preset);
        let result =
            test_stored_claude_preset(&context.client, &effective_preset, use_local_proxy).await;
        results.push(annotate_preset_test_result(
            &context,
            result,
            if use_local_proxy {
                "本地中继"
            } else {
                "直连上游"
            },
        ));
    }
    Ok(Json(build_preset_batch_test_response(results)))
}

async fn test_stored_api_preset(
    client: &reqwest::Client,
    preset: &StoredApiPreset,
    default_config_pairs: &[(&str, &str)],
) -> ApiResult<PresetTestResult> {
    test_stored_api_preset_with_delay(client, preset, default_config_pairs, PRESET_CHAT_PROBE_DELAY)
        .await
}

pub(crate) async fn test_stored_api_preset_with_delay(
    client: &reqwest::Client,
    preset: &StoredApiPreset,
    default_config_pairs: &[(&str, &str)],
    chat_probe_delay: Duration,
) -> ApiResult<PresetTestResult> {
    let mut conversation_target = llm::api_preset_llm_target(preset);
    let local_anthropic_responses = matches!(
        effective_api_responses_proxy(preset),
        Some(ApiResponsesProxyMode::AnthropicChat)
    ) && api_preset_enables_local_upstream_proxy_on_apply(preset);
    let mut probe_base_url = if local_anthropic_responses {
        preset.base_url.clone()
    } else {
        conversation_target.base_url.clone()
    };
    let models_api_key = if local_anthropic_responses {
        preset.api_key.clone()
    } else {
        conversation_target.api_key.clone()
    };
    let models_upstream_preset_id = if local_anthropic_responses {
        None
    } else {
        conversation_target.upstream_preset_id.as_deref()
    };
    let mut models_outcome = send_api_models_probe_request(
        client,
        preset,
        &probe_base_url,
        &models_api_key,
        models_upstream_preset_id,
    )
    .await;
    let config_targets =
        resolve_effective_preset_config_targets(default_config_pairs, &preset.config_overrides)
            .map_err(|error| AppError::internal(format!("API 预设 config 覆盖无效: {error}")))?;
    let model_override = model_from_config_targets(&config_targets);
    let mut model = model_override
        .clone()
        .or_else(|| first_model_name_from_body(&models_outcome.body));

    // The proxy URL already ends with `/v1`, so the fallback never fires
    // when local forwarding is enabled. For the direct path, the original
    // base_url may omit `/v1` while the upstream still exposes `/v1/models`.
    if model.is_none()
        && let Some(fallback_base_url) = openai_v1_fallback_base_url(&probe_base_url)
    {
        let fallback_outcome = send_api_models_probe_request(
            client,
            preset,
            &fallback_base_url,
            &models_api_key,
            models_upstream_preset_id,
        )
        .await;
        let fallback_model = first_model_name_from_body(&fallback_outcome.body);
        if fallback_model.is_some() {
            probe_base_url = fallback_base_url;
            models_outcome = fallback_outcome;
            model = model_override.clone().or(fallback_model);
        }
    }

    let Some(model) = model else {
        if !models_outcome.result.ok {
            return Ok(models_outcome.result);
        }
        return Ok(PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: false,
            endpoint: api_probe_endpoint(preset, &conversation_target.base_url),
            status: None,
            latency_ms: models_outcome.result.latency_ms,
            message: "连接测试通过，但无法从预设或模型列表确定对话测试模型。".to_string(),
        });
    };
    if !models_outcome.result.ok && model_override.is_none() {
        return Ok(models_outcome.result);
    }
    if model_override.is_some()
        && models_outcome.result.ok
        && models_response_has_model(&models_outcome.body, &model) == Some(false)
    {
        return Ok(PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: false,
            endpoint: api_probe_endpoint(preset, &conversation_target.base_url),
            status: models_outcome.result.status,
            latency_ms: models_outcome.result.latency_ms,
            message: format!(
                "模型列表测试成功，但预设模型 `{}` 不在上游模型列表中；已跳过对话测试。",
                model
            ),
        });
    }

    tokio::time::sleep(chat_probe_delay).await;
    if !local_anthropic_responses {
        conversation_target.base_url = probe_base_url;
    }
    let chat_outcome = send_api_probe_request(client, preset, &conversation_target, &model).await;
    Ok(PresetTestResult {
        message: format_probe_result_message(
            "模型列表服务器回应",
            &models_outcome,
            "对话测试服务器回应",
            &chat_outcome,
        ),
        ..chat_outcome.result
    })
}

async fn send_api_models_probe_request(
    client: &reqwest::Client,
    preset: &StoredApiPreset,
    base_url: &str,
    bearer_token: &str,
    upstream_preset_id: Option<&str>,
) -> PresetHttpTestOutcome {
    let is_anthropic = matches!(
        effective_api_responses_proxy(preset),
        Some(ApiResponsesProxyMode::AnthropicChat)
    );
    let endpoint = if is_anthropic {
        let trimmed = base_url.trim_end_matches('/');
        if trimmed.to_ascii_lowercase().ends_with("/v1") {
            append_url_path(trimmed, "models")
        } else {
            append_url_path(trimmed, "v1/models")
        }
    } else {
        append_url_path(base_url, "models")
    };
    let started = Instant::now();
    let mut request = client
        .get(&endpoint)
        .bearer_auth(bearer_token)
        .header(header::ACCEPT, "application/json");
    if is_anthropic {
        request = request
            .header("x-api-key", &preset.api_key)
            .header("anthropic-version", "2023-06-01");
    }
    if let Some(preset_id) = upstream_preset_id {
        request = request.header(UPSTREAM_PRESET_ID_HEADER, preset_id);
    }
    let response = request.send().await;
    preset_test_result_from_response(
        preset.id.clone(),
        preset.name.clone(),
        endpoint,
        started,
        response,
    )
    .await
}

pub(in crate::auth) fn api_probe_endpoint(preset: &StoredApiPreset, base_url: &str) -> String {
    match llm::api_preset_llm_target(preset).protocol {
        llm::LlmProtocol::ChatCompletions => llm::chat_completions_url(base_url),
        llm::LlmProtocol::AnthropicMessages => llm::anthropic_messages_url(base_url),
        llm::LlmProtocol::Responses => llm::responses_url(base_url),
    }
}

async fn send_api_probe_request(
    client: &reqwest::Client,
    preset: &StoredApiPreset,
    target: &llm::ApiPresetLlmTarget,
    model: &str,
) -> PresetHttpTestOutcome {
    match llm::probe_conversation(client, target, model, "hi").await {
        Ok(reply) => {
            let status = StatusCode::from_u16(reply.status).unwrap_or(StatusCode::OK);
            PresetHttpTestOutcome {
                result: PresetTestResult {
                    preset_id: preset.id.clone(),
                    name: preset.name.clone(),
                    ok: true,
                    endpoint: reply.endpoint,
                    status: Some(reply.status),
                    latency_ms: reply.latency_ms,
                    message: format_raw_server_response_message(status, &reply.response_body),
                },
                body: reply.response_body,
            }
        }
        Err(error) => PresetHttpTestOutcome {
            result: PresetTestResult {
                preset_id: preset.id.clone(),
                name: preset.name.clone(),
                ok: false,
                endpoint: error.endpoint,
                status: error.status,
                latency_ms: error.latency_ms,
                message: if upstream_model_not_found(error.status, &error.response_body) {
                    format!(
                        "上游没有该模型 `{}`：{}",
                        model,
                        error
                            .message
                            .replacen("读取 LLM 响应失败", "读取服务器响应失败", 1)
                    )
                } else {
                    error
                        .message
                        .replacen("读取 LLM 响应失败", "读取服务器响应失败", 1)
                },
            },
            body: error.response_body,
        },
    }
}

fn upstream_model_not_found(status: Option<u16>, body: &str) -> bool {
    if status == Some(404) {
        return true;
    }
    let normalized = body.to_ascii_lowercase();
    [
        "model_not_found",
        "model not found",
        "unknown model",
        "model does not exist",
        "model not supported",
        "没有这个大模型",
        "模型不存在",
        "不支持的模型",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

fn openai_v1_fallback_base_url(base_url: &str) -> Option<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.to_ascii_lowercase().ends_with("/v1") {
        return None;
    }
    Some(format!("{trimmed}/v1"))
}

fn format_probe_result_message(
    first_label: &str,
    first: &PresetHttpTestOutcome,
    second_label: &str,
    second: &PresetHttpTestOutcome,
) -> String {
    let status_line = match (first.result.ok, second.result.ok) {
        (true, true) => "模型列表测试成功；等待 2 秒后对话测试成功。",
        (true, false) => "模型列表测试成功；等待 2 秒后对话测试失败。",
        (false, true) => "模型列表测试失败；已使用预设模型等待 2 秒后对话测试成功。",
        (false, false) => "模型列表测试失败；已使用预设模型等待 2 秒后对话测试失败。",
    };
    let mut message = format!(
        "{status_line}\n模型列表可读内容：{}\n对话测试可读内容：{}",
        readable_models_summary(&first.body),
        readable_chat_summary(&second.body)
    );
    if !first.result.ok {
        message.push_str(&format!("\n{first_label}失败详情：{}", first.result.message));
    }
    if !second.result.ok {
        message.push_str(&format!("\n{second_label}失败详情：{}", second.result.message));
    }
    message
}

async fn test_stored_claude_preset(
    client: &reqwest::Client,
    preset: &StoredClaudePreset,
    use_local_proxy: bool,
) -> PresetTestResult {
    test_stored_claude_preset_with_delay(client, preset, use_local_proxy, PRESET_CHAT_PROBE_DELAY)
        .await
}

pub(in crate::auth) struct ClaudePresetTestTarget {
    pub(in crate::auth) base_url: String,
    pub(in crate::auth) auth_token: String,
    use_local_proxy: bool,
}

pub(in crate::auth) fn claude_preset_test_target(
    preset: &StoredClaudePreset,
    use_local_proxy: bool,
) -> ClaudePresetTestTarget {
    if use_local_proxy {
        ClaudePresetTestTarget {
            base_url: claude_provider_base_url_for_mode(preset, true),
            auth_token: local_proxy_claude_token_for_preset_id(&preset.id),
            use_local_proxy,
        }
    } else {
        ClaudePresetTestTarget {
            base_url: preset.base_url.clone(),
            auth_token: preset.auth_token.clone(),
            use_local_proxy,
        }
    }
}

pub(in crate::auth) fn apply_claude_preset_test_headers(
    request: reqwest::RequestBuilder,
    preset: &StoredClaudePreset,
    target: &ClaudePresetTestTarget,
) -> reqwest::RequestBuilder {
    let request = request
        .header("x-api-key", &target.auth_token)
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .header(header::ACCEPT, "application/json");
    if target.use_local_proxy {
        request.header(UPSTREAM_PRESET_ID_HEADER, &preset.id)
    } else {
        request
    }
}

pub(crate) async fn test_stored_claude_preset_with_delay(
    client: &reqwest::Client,
    preset: &StoredClaudePreset,
    use_local_proxy: bool,
    chat_probe_delay: Duration,
) -> PresetTestResult {
    let target = claude_preset_test_target(preset, use_local_proxy);
    let endpoint = append_anthropic_models_path(&target.base_url);
    let started = Instant::now();
    let response = apply_claude_preset_test_headers(client.get(&endpoint), preset, &target)
        .send()
        .await;
    let models_outcome = preset_test_result_from_response(
        preset.id.clone(),
        preset.name.clone(),
        endpoint,
        started,
        response,
    )
    .await;
    let model_from_preset = resolve_claude_preset_configured_model(preset);
    if !models_outcome.result.ok && model_from_preset.is_none() {
        return models_outcome.result;
    }

    let Some(model) =
        model_from_preset.or_else(|| first_model_name_from_body(&models_outcome.body))
    else {
        return PresetTestResult {
            preset_id: preset.id.clone(),
            name: preset.name.clone(),
            ok: false,
            endpoint: append_anthropic_messages_path(&target.base_url),
            status: None,
            latency_ms: models_outcome.result.latency_ms,
            message: "连接测试通过，但无法从预设或模型列表确定对话测试模型。".to_string(),
        };
    };

    tokio::time::sleep(chat_probe_delay).await;
    let endpoint = append_anthropic_messages_path(&target.base_url);
    let started = Instant::now();
    let response = apply_claude_preset_test_headers(client.post(&endpoint), preset, &target)
        .json(&json!({
            "model": model,
            "max_tokens": 16,
            "messages": [
                {
                    "role": "user",
                    "content": "hi"
                }
            ]
        }))
        .send()
        .await;
    let chat_outcome = preset_test_result_from_response(
        preset.id.clone(),
        preset.name.clone(),
        endpoint,
        started,
        response,
    )
    .await;
    PresetTestResult {
        message: format_probe_result_message(
            "模型列表服务器回应",
            &models_outcome,
            "对话测试服务器回应",
            &chat_outcome,
        ),
        ..chat_outcome.result
    }
}

struct PresetHttpTestOutcome {
    result: PresetTestResult,
    body: String,
}

async fn preset_test_result_from_response(
    preset_id: String,
    name: String,
    endpoint: String,
    started: Instant,
    response: Result<reqwest::Response, reqwest::Error>,
) -> PresetHttpTestOutcome {
    let latency_ms = started.elapsed().as_millis();
    match response {
        Ok(response) => {
            let status = response.status();
            let status_code = status.as_u16();
            match response.text().await {
                Ok(body) => PresetHttpTestOutcome {
                    result: PresetTestResult {
                        preset_id,
                        name,
                        ok: status.is_success(),
                        endpoint,
                        status: Some(status_code),
                        latency_ms: started.elapsed().as_millis(),
                        message: format_raw_server_response_message(status, &body),
                    },
                    body,
                },
                Err(error) => PresetHttpTestOutcome {
                    result: PresetTestResult {
                        preset_id,
                        name,
                        ok: false,
                        endpoint,
                        status: Some(status_code),
                        latency_ms: started.elapsed().as_millis(),
                        message: format!("HTTP {}，读取服务器响应失败: {error}", status.as_u16()),
                    },
                    body: String::new(),
                },
            }
        }
        Err(error) => PresetHttpTestOutcome {
            result: PresetTestResult {
                preset_id,
                name,
                ok: false,
                endpoint,
                status: error.status().map(|status| status.as_u16()),
                latency_ms,
                message: error.to_string(),
            },
            body: String::new(),
        },
    }
}

fn format_raw_server_response_message(status: StatusCode, body: &str) -> String {
    format!("HTTP {}，可读内容：{}", status.as_u16(), readable_response_summary(body))
}

fn readable_response_summary(body: &str) -> String {
    let chat = readable_chat_summary(body);
    if chat != "未能提取可读对话内容。" {
        return chat;
    }
    readable_models_summary(body)
}

pub(in crate::auth) fn readable_models_summary(body: &str) -> String {
    first_model_names_from_body(body)
        .map(|names| names.join(", "))
        .unwrap_or_else(|| "未能提取模型名。".to_string())
}

pub(in crate::auth) fn readable_chat_summary(body: &str) -> String {
    if let Some(text) = readable_sse_chat_summary(body) {
        return text;
    }
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| extract_chat_text_from_response(&value))
        .unwrap_or_else(|| "未能提取可读对话内容。".to_string())
}

fn readable_sse_chat_summary(body: &str) -> Option<String> {
    body.lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim))
        .filter(|line| !line.is_empty() && *line != "[DONE]")
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|value| extract_chat_text_from_response(&value))
}

fn build_preset_batch_test_response(results: Vec<PresetTestResult>) -> PresetBatchTestResponse {
    let total = results.len();
    let success_count = results.iter().filter(|result| result.ok).count();
    PresetBatchTestResponse {
        ok: success_count == total,
        total,
        success_count,
        failure_count: total.saturating_sub(success_count),
        results,
    }
}

fn append_url_path(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim().trim_end_matches('/'), path.trim_start_matches('/'))
}

fn append_anthropic_models_path(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.to_ascii_lowercase().ends_with("/v1") {
        append_url_path(trimmed, "models")
    } else {
        append_url_path(trimmed, "v1/models")
    }
}

fn append_anthropic_messages_path(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.to_ascii_lowercase().ends_with("/v1") {
        append_url_path(trimmed, "messages")
    } else {
        append_url_path(trimmed, "v1/messages")
    }
}

fn resolve_claude_preset_configured_model(preset: &StoredClaudePreset) -> Option<String> {
    claude_env_model_override(&preset.config_overrides, "ANTHROPIC_MODEL")
        .or_else(|| {
            preset
                .third_party_model
                .as_deref()
                .and_then(non_empty_model_name)
        })
        .or_else(|| {
            claude_env_model_override(&preset.config_overrides, "ANTHROPIC_DEFAULT_SONNET_MODEL")
        })
        .or_else(|| {
            preset
                .default_sonnet_model
                .as_deref()
                .and_then(non_empty_model_name)
        })
        .or_else(|| {
            claude_env_model_override(&preset.config_overrides, "ANTHROPIC_DEFAULT_HAIKU_MODEL")
        })
        .or_else(|| {
            preset
                .default_haiku_model
                .as_deref()
                .and_then(non_empty_model_name)
        })
        .or_else(|| {
            claude_env_model_override(&preset.config_overrides, "ANTHROPIC_DEFAULT_OPUS_MODEL")
        })
        .or_else(|| {
            preset
                .default_opus_model
                .as_deref()
                .and_then(non_empty_model_name)
        })
        .or_else(|| preset_model_override(&preset.config_overrides))
}

fn claude_env_model_override(
    overrides: &[PresetConfigOverride],
    expected_key: &str,
) -> Option<String> {
    overrides.iter().rev().find_map(|item| {
        let key = item.key.as_deref()?.trim();
        if !key.eq_ignore_ascii_case(expected_key) {
            return None;
        }
        item.value.as_deref().and_then(non_empty_model_name)
    })
}

fn preset_model_override(overrides: &[PresetConfigOverride]) -> Option<String> {
    overrides.iter().find_map(|item| {
        let key = item.key.as_deref()?.trim();
        if key != "model" && key != "ANTHROPIC_MODEL" {
            return None;
        }
        item.value
            .as_deref()
            .and_then(trim_model_override_literal)
            .and_then(|value| non_empty_model_name(&value))
    })
}

fn model_from_config_targets(targets: &[ResolvedConfigTarget]) -> Option<String> {
    targets
        .iter()
        .rev()
        .find(|target| target.key.eq_ignore_ascii_case("model"))
        .and_then(|target| {
            trim_model_override_literal(&target.value)
                .and_then(|value| non_empty_model_name(&value))
        })
}

fn trim_model_override_literal(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return Some(trimmed[1..trimmed.len() - 1].trim().to_string());
    }
    Some(trimmed.to_string())
}

fn first_model_name_from_body(body: &str) -> Option<String> {
    first_model_names_from_body(body).and_then(|mut names| names.drain(..).next())
}

fn models_response_has_model(body: &str, model: &str) -> Option<bool> {
    let names = first_model_names_from_body(body)?;
    let expected = model.trim().to_ascii_lowercase();
    Some(
        names
            .iter()
            .any(|name| name.trim().eq_ignore_ascii_case(expected.as_str())),
    )
}

fn first_model_names_from_body(body: &str) -> Option<Vec<String>> {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| extract_model_names_from_models_response(&value))
        .map(|models| models.names)
}

struct ModelNamesSummary {
    names: Vec<String>,
}

fn extract_model_names_from_models_response(value: &Value) -> Option<ModelNamesSummary> {
    let models = value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.get("models").and_then(Value::as_array))?;
    let names: Vec<String> = models.iter().filter_map(extract_model_name).collect();
    Some(ModelNamesSummary { names })
}

fn extract_model_name(value: &Value) -> Option<String> {
    match value {
        Value::String(name) => non_empty_model_name(name),
        Value::Object(object) => ["id", "name", "model", "display_name"]
            .iter()
            .filter_map(|key| object.get(*key).and_then(Value::as_str))
            .find_map(non_empty_model_name),
        _ => None,
    }
}

fn non_empty_model_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_chat_text_from_response(value: &Value) -> Option<String> {
    [
        value.pointer("/choices/0/message/content"),
        value.pointer("/choices/0/message/reasoning_content"),
        value.pointer("/choices/0/text"),
        value.get("content"),
        value.get("output_text"),
        value.pointer("/message/content"),
        value.pointer("/error/message"),
        value.get("detail"),
        value.get("delta"),
        value.get("text"),
        value.get("message"),
    ]
    .into_iter()
    .flatten()
    .find_map(value_to_readable_text)
}

fn value_to_readable_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => non_empty_model_name(text),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .or_else(|| item.get("text").and_then(Value::as_str).map(str::to_string))
                        .or_else(|| {
                            item.get("content")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .or_else(|| {
                            item.get("thinking")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .or_else(|| {
                            item.get("reasoning_content")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                })
                .collect::<Vec<_>>()
                .join("\n");
            non_empty_model_name(&text)
        }
        Value::Object(object) => [
            "text",
            "content",
            "message",
            "thinking",
            "reasoning_content",
        ]
        .iter()
        .filter_map(|key| object.get(*key))
        .find_map(value_to_readable_text),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn models_response_has_model_matches_case_insensitively() {
        let body = json!({
            "data": [
                {"id": "gpt-5.6-sol"},
                {"id": "gpt-5.6"}
            ]
        })
        .to_string();

        assert_eq!(models_response_has_model(&body, "GPT-5.6-SOL"), Some(true));
        assert_eq!(models_response_has_model(&body, "gpt-5.5"), Some(false));
        assert_eq!(models_response_has_model("not-json", "gpt-5.6"), None);
    }

    #[test]
    fn upstream_model_not_found_distinguishes_model_errors_from_service_errors() {
        assert!(upstream_model_not_found(Some(404), ""));
        assert!(upstream_model_not_found(
            Some(503),
            r#"{"error":{"code":"model_not_found","message":"unknown model gpt-5.6-sol"}}"#
        ));
        assert!(!upstream_model_not_found(
            Some(503),
            r#"{"error":{"message":"Service temporarily unavailable"}}"#
        ));
    }
}
