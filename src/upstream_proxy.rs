use std::{convert::Infallible, net::SocketAddr};

use auth_core::{
    ANTHROPIC_UPSTREAM_PROXY_BASE_PATH, ApiAccessMode, ApiResponsesProxyMode,
    OPENAI_UPSTREAM_PROXY_BASE_PATH, effective_api_responses_proxy,
    local_proxy_api_preset_id_from_api_key, local_proxy_claude_preset_id_from_token,
};
use axum::{
    body::{Body, Bytes, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderName, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use codex_proxy_core::{
    chat_response_to_responses_payload, degrade_resume_chat_request_for_minimax, gateway_message,
    response_payload_to_sse_chunks, responses_request_to_chat_request,
    sanitize_chat_request_for_deepseek, sanitize_chat_request_for_minimax,
    strip_chat_request_reasoning_content,
};
use futures_util::stream;
use serde_json::{Value, json};
use tracing::warn;

use crate::{ApiResult, AppError, AppState};

mod transform;

use transform::{
    anthropic_messages_request_to_openai_chat, anthropic_messages_request_to_openai_responses,
    openai_chat_response_to_anthropic_messages_response,
    openai_responses_payload_to_anthropic_messages_response,
};

const PROXY_TIMEOUT_SECS: u64 = 300;
const MAX_PROXY_BODY_BYTES: usize = 32 * 1024 * 1024;
const UPSTREAM_PRESET_ID_HEADER: &str = "x-webclx-upstream-preset-id";
const TRANSPARENT_KIND: &str = "透明跳转";
const OPENAI_RESPONSES_KIND: &str = "OpenAI Responses→OpenAI Responses";
const ANTHROPIC_CHAT_KIND: &str = "Anthropic Messages→OpenAI Chat";
const ANTHROPIC_RESPONSES_KIND: &str = "Anthropic Messages→OpenAI Responses";
const CLIENT_CREDENTIAL_KIND: &str = "客户端凭据透传";

pub async fn openai_upstream_proxy(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
) -> ApiResult<Response> {
    ensure_addr_allowed(&state, addr)?;
    let (parts, body) = request.into_parts();
    let settings = state.auth_manager.upstream_proxy_settings();
    let legacy_bare_token = openai_proxy_uses_legacy_bare_token(&parts.headers);
    let scoped_preset_id = upstream_preset_id_from_headers(&parts.headers)
        .or_else(|| openai_proxy_preset_id_from_credentials(&parts.headers));
    let used_active_fallback = legacy_bare_token && scoped_preset_id.is_none();
    let preset_id = scoped_preset_id
        .or(settings.active_api_proxy_preset_id.as_deref())
        .ok_or_else(|| {
            AppError::bad_request(gateway_message(
                TRANSPARENT_KIND,
                "尚未应用任何 Codex_API 预设。",
            ))
        })?;
    if used_active_fallback {
        warn_active_fallback(
            &state,
            "OpenAI",
            &preset_id,
            parts
                .headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok()),
        );
    }
    let preset = state
        .auth_manager
        .api_presets_snapshot()
        .into_iter()
        .find(|preset| preset.id == preset_id)
        .ok_or_else(|| {
            AppError::not_found(gateway_message(
                TRANSPARENT_KIND,
                "当前 Codex_API 上游预设已不存在。",
            ))
        })?;

    let body = to_bytes(body, MAX_PROXY_BODY_BYTES)
        .await
        .map_err(|error| {
            AppError::bad_request(gateway_message(
                TRANSPARENT_KIND,
                format!("读取代理请求体失败: {error}"),
            ))
        })?;
    let suffix = upstream_suffix(&parts.uri, OPENAI_UPSTREAM_PROXY_BASE_PATH)?;

    if parts.method == Method::POST
        && suffix == "/responses"
        && effective_api_responses_proxy(&preset).is_some()
    {
        return proxy_openai_responses_conversion(state, &preset, &parts.headers, body).await;
    }
    let body = if parts.method == Method::POST && suffix == "/chat/completions" {
        sanitize_openai_chat_request_body(&preset, body)?
    } else {
        body
    };

    // OAuth access-token preset: route to ChatGPT backend and inject account_id header
    let is_chatgpt_oauth =
        preset.access_mode == Some(ApiAccessMode::ChatgptOauth) && !preset.access_token.is_empty();
    if is_chatgpt_oauth {
        let url = join_upstream_url(&preset.base_url, &suffix, parts.uri.query());
        let mut oauth_headers = parts.headers.clone();
        if !preset.account_id.is_empty() {
            oauth_headers.insert(
                "chatgpt-account-id",
                preset.account_id.as_str().try_into().map_err(|error| {
                    AppError::internal(gateway_message(
                        TRANSPARENT_KIND,
                        format!("account_id header 编码失败: {error}"),
                    ))
                })?,
            );
        }
        let credential = Credential::Bearer(preset.access_token.clone());
        return forward_request(
            &state,
            &preset.provider_name,
            parts.method,
            url,
            oauth_headers,
            body,
            credential,
        )
        .await;
    }

    let url = join_upstream_url(&preset.base_url, &suffix, parts.uri.query());
    let credential = client_provided_credential(&parts.headers, false)
        .inspect(|_| {
            tracing::info!(
                "{}",
                gateway_message(
                    CLIENT_CREDENTIAL_KIND,
                    format!(
                        "OpenAI 上游使用客户端提供的凭据转发，预设 {} 仅提供 base_url",
                        preset.name
                    ),
                )
            );
        })
        .unwrap_or_else(|| Credential::from_preset(&preset.api_key, false));
    forward_request(
        &state,
        &preset.provider_name,
        parts.method,
        url,
        parts.headers,
        body,
        credential,
    )
    .await
}

pub async fn anthropic_upstream_proxy(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
) -> ApiResult<Response> {
    ensure_addr_allowed(&state, addr)?;
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, MAX_PROXY_BODY_BYTES)
        .await
        .map_err(|error| {
            AppError::bad_request(gateway_message(
                TRANSPARENT_KIND,
                format!("读取代理请求体失败: {error}"),
            ))
        })?;
    let settings = state.auth_manager.upstream_proxy_settings();
    let presets = state.auth_manager.claude_presets_snapshot();
    let legacy_bare_token = anthropic_proxy_uses_legacy_bare_token(&parts.headers);
    let scoped_preset = upstream_preset_id_from_headers(&parts.headers)
        .or_else(|| anthropic_proxy_scoped_preset_id_from_credentials(&parts.headers))
        .and_then(|preset_id| presets.iter().find(|preset| preset.id == preset_id));
    let model_matched_preset = claude_preset_from_request_model(&body, &presets);
    let active_preset = settings
        .active_claude_proxy_preset_id
        .as_deref()
        .and_then(|preset_id| presets.iter().find(|preset| preset.id == preset_id));
    let used_active_fallback =
        legacy_bare_token && scoped_preset.is_none() && model_matched_preset.is_none();
    let preset = scoped_preset
        .or(model_matched_preset)
        .or(active_preset)
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(gateway_message(TRANSPARENT_KIND, "当前 Claude 上游预设已不存在。"))
        })?;
    if used_active_fallback {
        warn_active_fallback(
            &state,
            "Claude",
            &preset.id,
            parts.headers.get("x-api-key").and_then(|v| v.to_str().ok()),
        );
    }

    let suffix = upstream_suffix(&parts.uri, ANTHROPIC_UPSTREAM_PROXY_BASE_PATH)?;
    if parts.method == Method::POST
        && suffix.ends_with("/messages")
        && auth_core::effective_claude_access_mode(&preset)
            == auth_core::ClaudeAccessMode::OpenaiChat
    {
        return proxy_anthropic_messages_to_openai_chat(&state, &preset, &parts.headers, body)
            .await;
    }
    if parts.method == Method::POST
        && suffix.ends_with("/messages")
        && auth_core::effective_claude_access_mode(&preset)
            == auth_core::ClaudeAccessMode::OpenaiResponses
    {
        return proxy_anthropic_messages_to_openai_responses(&state, &preset, &parts.headers, body)
            .await;
    }
    let url = join_upstream_url(&preset.base_url, &suffix, parts.uri.query());
    let openai_shape = auth_core::effective_claude_access_mode(&preset)
        == auth_core::ClaudeAccessMode::OpenaiChat
        || auth_core::effective_claude_access_mode(&preset)
            == auth_core::ClaudeAccessMode::OpenaiResponses;
    let credential = client_provided_credential(&parts.headers, !openai_shape)
        .inspect(|_| {
            tracing::info!(
                "{}",
                gateway_message(
                    CLIENT_CREDENTIAL_KIND,
                    format!(
                        "Claude 上游使用客户端提供的凭据转发，预设 {} 仅提供 base_url",
                        preset.name
                    ),
                )
            );
        })
        .unwrap_or_else(|| Credential::from_preset(&preset.auth_token, !openai_shape));

    forward_request(
        &state,
        &preset.provider_name,
        parts.method,
        url,
        parts.headers,
        body,
        credential,
    )
    .await
}

fn claude_preset_from_request_model<'a>(
    body: &[u8],
    presets: &'a [auth_core::StoredClaudePreset],
) -> Option<&'a auth_core::StoredClaudePreset> {
    let model = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|payload| {
            payload
                .get("model")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })?;
    let mut matches = presets
        .iter()
        .filter(|preset| claude_preset_has_model(preset, &model));
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}

fn claude_preset_has_model(preset: &auth_core::StoredClaudePreset, model: &str) -> bool {
    [
        preset.default_haiku_model.as_deref(),
        preset.default_sonnet_model.as_deref(),
        preset.default_opus_model.as_deref(),
        preset.third_party_model.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|candidate| candidate == model)
}

fn upstream_preset_id_from_headers(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(UPSTREAM_PRESET_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Emit a warn log + global toast when an upstream-proxy request could not be
/// pinned to a preset via scoped token/header and fell back to the global
/// `active_*_proxy_preset_id`. This is expected only for legacy bare placeholder
/// tokens (`webclx-local-api-proxy` / `webclx-local-claude-proxy` without an id
/// suffix); new sessions always carry a scoped token and never reach here.
/// Surfacing it lets operators discover stale processes still on bare tokens.
fn warn_active_fallback(
    state: &AppState,
    channel: &str,
    preset_id: &str,
    credential_header: Option<&str>,
) {
    let masked = credential_header
        .map(|value| {
            let len = value.trim().len();
            if len > 12 {
                format!("{}...{}", &value[..6], &value[len - 4..])
            } else {
                "(短凭据)".to_string()
            }
        })
        .unwrap_or_else(|| "(无凭据)".to_string());
    warn!(
        channel,
        preset_id,
        credential = %masked,
        "{} upstream proxy request had no preset-scoped identity; fell back to global active preset {}. Usually means a process is still using a legacy bare placeholder token.",
        channel,
        preset_id,
    );
    state.terminal_manager.broadcast_toast(
        format!(
            "{} 上游请求回退到全局 active 预设「{}」，可能来自仍在使用旧通用占位 token 的进程。",
            channel, preset_id,
        ),
        "warn",
    );
}

fn openai_proxy_preset_id_from_credentials(headers: &HeaderMap) -> Option<&str> {
    bearer_token_from_headers(headers)
        .and_then(local_proxy_api_preset_id_from_api_key)
        .or_else(|| {
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
                .and_then(local_proxy_api_preset_id_from_api_key)
        })
}

fn openai_proxy_uses_legacy_bare_token(headers: &HeaderMap) -> bool {
    bearer_token_from_headers(headers)
        .is_some_and(|value| value.trim() == auth_core::LOCAL_PROXY_API_KEY)
        || headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.trim() == auth_core::LOCAL_PROXY_API_KEY)
}

fn anthropic_proxy_scoped_preset_id_from_credentials(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .and_then(local_proxy_claude_preset_id_from_token)
        .or_else(|| {
            bearer_token_from_headers(headers).and_then(local_proxy_claude_preset_id_from_token)
        })
}

fn anthropic_proxy_uses_legacy_bare_token(headers: &HeaderMap) -> bool {
    headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == auth_core::LOCAL_PROXY_CLAUDE_TOKEN)
        || bearer_token_from_headers(headers)
            .is_some_and(|value| value.trim() == auth_core::LOCAL_PROXY_CLAUDE_TOKEN)
}

fn bearer_token_from_headers(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or(value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn sanitize_openai_chat_request_body(
    preset: &auth_core::StoredApiPreset,
    body: Bytes,
) -> ApiResult<Bytes> {
    let request: Value = serde_json::from_slice(&body).map_err(|error| {
        AppError::bad_request(gateway_message(
            TRANSPARENT_KIND,
            format!("OpenAI Chat 请求不是有效 JSON: {error}"),
        ))
    })?;
    let request = match effective_api_responses_proxy(preset) {
        Some(ApiResponsesProxyMode::DeepseekChat) => sanitize_chat_request_for_deepseek(request),
        _ => strip_chat_request_reasoning_content(request),
    };
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| {
            AppError::internal(gateway_message(
                TRANSPARENT_KIND,
                format!("序列化 OpenAI Chat 请求失败: {error}"),
            ))
        })
}

async fn proxy_openai_responses_conversion(
    state: AppState,
    preset: &auth_core::StoredApiPreset,
    headers: &HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let payload: Value = serde_json::from_slice(&body).map_err(|error| {
        AppError::bad_request(gateway_message(
            OPENAI_RESPONSES_KIND,
            format!("Responses 请求不是有效 JSON: {error}"),
        ))
    })?;
    let missing_previous_response = payload
        .get("previous_response_id")
        .and_then(Value::as_str)
        .is_some_and(|response_id| !state.codex_proxy_history.contains(response_id));
    let chat_request = state
        .codex_proxy_history
        .chat_request_with_previous_response(
            &payload,
            responses_request_to_chat_request(&payload).map_err(|error| {
                AppError::bad_request(gateway_message(OPENAI_RESPONSES_KIND, error.to_string()))
            })?,
        );
    let chat_request = match effective_api_responses_proxy(preset) {
        Some(ApiResponsesProxyMode::Direct) => chat_request,
        Some(ApiResponsesProxyMode::OpenaiChat) => chat_request,
        Some(ApiResponsesProxyMode::AnthropicChat) => chat_request,
        Some(ApiResponsesProxyMode::MinimaxChat) => {
            if missing_previous_response {
                degrade_resume_chat_request_for_minimax(chat_request)
            } else {
                sanitize_chat_request_for_minimax(chat_request)
            }
        }
        Some(ApiResponsesProxyMode::DeepseekChat) => {
            sanitize_chat_request_for_deepseek(chat_request)
        }
        None => chat_request,
    };
    let chat_request = match effective_api_responses_proxy(preset) {
        Some(ApiResponsesProxyMode::DeepseekChat) => chat_request,
        _ => strip_chat_request_reasoning_content(chat_request),
    };
    let stream_response = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("webclx-proxy")
        .to_string();
    let url = join_upstream_url(&preset.base_url, "/chat/completions", None);
    let client = state
        .proxy_manager
        .build_app_client(PROXY_TIMEOUT_SECS)
        .map_err(|error| {
            AppError::internal(gateway_message(
                OPENAI_RESPONSES_KIND,
                format!("创建上游代理客户端失败: {error}"),
            ))
        })?;
    let upstream_bearer = client_provided_credential(headers, false)
        .and_then(|credential| match credential {
            Credential::Bearer(token) => Some(token),
            _ => None,
        })
        .unwrap_or_else(|| preset.api_key.clone());
    let mut request = client
        .post(url)
        .header(header::CONTENT_TYPE, "application/json")
        .bearer_auth(&upstream_bearer);
    if let Some(value) = headers.get(header::ACCEPT) {
        request = request.header(header::ACCEPT, value.clone());
    }
    let upstream = request.json(&chat_request).send().await.map_err(|error| {
        AppError::internal(gateway_message(
            OPENAI_RESPONSES_KIND,
            format!("{} 代理请求失败: {error}", preset.provider_name),
        ))
    })?;
    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let upstream_body = upstream.bytes().await.map_err(|error| {
        AppError::internal(gateway_message(
            OPENAI_RESPONSES_KIND,
            format!("读取 {} 响应失败: {error}", preset.provider_name),
        ))
    })?;

    if !status.is_success() {
        // Capture the quota reset time from a Zhipu quota-exceeded 429. Codex
        // discards the response body and only prints the status line, so the
        // terminal tail never contains the "限额将在 {time} 重置" text the
        // auto-continue time patterns rely on. Record it here keyed by preset
        // id; the terminal error auto-continue scanner consumes it.
        if status == StatusCode::TOO_MANY_REQUESTS
            && crate::quota_reset_cache::base_url_is_zhipu_upstream(&preset.base_url)
            && let Ok(body_str) = std::str::from_utf8(&upstream_body)
            && let Some(reset_at) = crate::quota_reset_cache::parse_zhipu_quota_reset(body_str)
        {
            state.quota_reset_cache.record_for_preset(
                &preset.id,
                &preset.base_url,
                reset_at.clone(),
            );
            tracing::info!(
                "{}",
                gateway_message(
                    OPENAI_RESPONSES_KIND,
                    format!(
                        "{} 捕获到智谱额度重置时间 {}，将在重置后由无人值守自动续跑消费。",
                        preset.provider_name, reset_at
                    ),
                )
            );
        }
        warn!(
            "{}",
            gateway_message(
                OPENAI_RESPONSES_KIND,
                format!(
                    "{} 上游状态 {}: {}",
                    preset.provider_name,
                    status,
                    String::from_utf8_lossy(&upstream_body)
                )
            )
        );
        return Ok(build_response(status, &upstream_headers, upstream_body));
    }

    let chat_response: Value = serde_json::from_slice(&upstream_body).map_err(|error| {
        AppError::internal(gateway_message(
            OPENAI_RESPONSES_KIND,
            format!(
                "{} 响应不是有效 JSON: {error}; {}",
                preset.provider_name,
                String::from_utf8_lossy(&upstream_body)
            ),
        ))
    })?;
    let response_payload = chat_response_to_responses_payload(&chat_response, &model);
    state
        .codex_proxy_history
        .record_response(&response_payload, &chat_request, &chat_response);

    if stream_response {
        Ok(sse_response(response_payload))
    } else {
        Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            serde_json::to_vec(&response_payload).unwrap_or_default(),
        )
            .into_response())
    }
}

async fn proxy_anthropic_messages_to_openai_chat(
    state: &AppState,
    preset: &auth_core::StoredClaudePreset,
    headers: &HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let payload: Value = serde_json::from_slice(&body).map_err(|error| {
        AppError::bad_request(gateway_message(
            ANTHROPIC_CHAT_KIND,
            format!("Anthropic Messages 请求不是有效 JSON: {error}"),
        ))
    })?;
    let stream_response = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("webclx-claude-proxy")
        .to_string();
    let chat_request = anthropic_messages_request_to_openai_chat(&payload)?;
    let url = join_upstream_url(&preset.base_url, "/chat/completions", None);
    let client = state
        .proxy_manager
        .build_app_client(PROXY_TIMEOUT_SECS)
        .map_err(|error| {
            AppError::internal(gateway_message(
                ANTHROPIC_CHAT_KIND,
                format!("创建上游代理客户端失败: {error}"),
            ))
        })?;
    let upstream_bearer = client_provided_credential(headers, false)
        .and_then(|credential| match credential {
            Credential::Bearer(token) => Some(token),
            _ => None,
        })
        .unwrap_or_else(|| preset.auth_token.clone());
    let mut request = client
        .post(url)
        .header(header::CONTENT_TYPE, "application/json")
        .bearer_auth(&upstream_bearer);
    if let Some(value) = headers.get(header::ACCEPT) {
        request = request.header(header::ACCEPT, value.clone());
    }
    let upstream = request.json(&chat_request).send().await.map_err(|error| {
        AppError::internal(gateway_message(
            ANTHROPIC_CHAT_KIND,
            format!("{} 代理请求失败: {error}", preset.provider_name),
        ))
    })?;
    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let upstream_body = upstream.bytes().await.map_err(|error| {
        AppError::internal(gateway_message(
            ANTHROPIC_CHAT_KIND,
            format!("读取 {} 响应失败: {error}", preset.provider_name),
        ))
    })?;

    if !status.is_success() {
        warn!(
            "{}",
            gateway_message(
                ANTHROPIC_CHAT_KIND,
                format!(
                    "{} 上游状态 {}: {}",
                    preset.provider_name,
                    status,
                    String::from_utf8_lossy(&upstream_body)
                )
            )
        );
        return Ok(build_response(status, &upstream_headers, upstream_body));
    }

    let chat_response: Value = serde_json::from_slice(&upstream_body).map_err(|error| {
        AppError::internal(gateway_message(
            ANTHROPIC_CHAT_KIND,
            format!(
                "{} 响应不是有效 JSON: {error}; {}",
                preset.provider_name,
                String::from_utf8_lossy(&upstream_body)
            ),
        ))
    })?;
    let anthropic_response =
        openai_chat_response_to_anthropic_messages_response(&chat_response, &model);
    if stream_response {
        Ok(anthropic_sse_response(anthropic_response))
    } else {
        Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            serde_json::to_vec(&anthropic_response).unwrap_or_default(),
        )
            .into_response())
    }
}

async fn proxy_anthropic_messages_to_openai_responses(
    state: &AppState,
    preset: &auth_core::StoredClaudePreset,
    headers: &HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let payload: Value = serde_json::from_slice(&body).map_err(|error| {
        AppError::bad_request(gateway_message(
            ANTHROPIC_RESPONSES_KIND,
            format!("Anthropic Messages 请求不是有效 JSON: {error}"),
        ))
    })?;
    let stream_response = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("webclx-claude-proxy")
        .to_string();
    let responses_request = anthropic_messages_request_to_openai_responses(&payload)?;
    let url = join_upstream_url(&preset.base_url, "/responses", None);
    let client = state
        .proxy_manager
        .build_app_client(PROXY_TIMEOUT_SECS)
        .map_err(|error| {
            AppError::internal(gateway_message(
                ANTHROPIC_RESPONSES_KIND,
                format!("创建上游代理客户端失败: {error}"),
            ))
        })?;
    let upstream_bearer = client_provided_credential(headers, false)
        .and_then(|credential| match credential {
            Credential::Bearer(token) => Some(token),
            _ => None,
        })
        .unwrap_or_else(|| preset.auth_token.clone());
    let mut request = client
        .post(url)
        .header(header::CONTENT_TYPE, "application/json")
        .bearer_auth(&upstream_bearer);
    if let Some(value) = headers.get(header::ACCEPT) {
        request = request.header(header::ACCEPT, value.clone());
    }
    let upstream = request
        .json(&responses_request)
        .send()
        .await
        .map_err(|error| {
            AppError::internal(gateway_message(
                ANTHROPIC_RESPONSES_KIND,
                format!("{} 代理请求失败: {error}", preset.provider_name),
            ))
        })?;
    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let upstream_body = upstream.bytes().await.map_err(|error| {
        AppError::internal(gateway_message(
            ANTHROPIC_RESPONSES_KIND,
            format!("读取 {} 响应失败: {error}", preset.provider_name),
        ))
    })?;

    if !status.is_success() {
        warn!(
            "{}",
            gateway_message(
                ANTHROPIC_RESPONSES_KIND,
                format!(
                    "{} 上游状态 {}: {}",
                    preset.provider_name,
                    status,
                    String::from_utf8_lossy(&upstream_body)
                )
            )
        );
        return Ok(build_response(status, &upstream_headers, upstream_body));
    }

    let responses_payload: Value = serde_json::from_slice(&upstream_body).map_err(|error| {
        AppError::internal(gateway_message(
            ANTHROPIC_RESPONSES_KIND,
            format!(
                "{} 响应不是有效 JSON: {error}; {}",
                preset.provider_name,
                String::from_utf8_lossy(&upstream_body)
            ),
        ))
    })?;
    let anthropic_response =
        openai_responses_payload_to_anthropic_messages_response(&responses_payload, &model);
    if stream_response {
        Ok(anthropic_sse_response(anthropic_response))
    } else {
        Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            serde_json::to_vec(&anthropic_response).unwrap_or_default(),
        )
            .into_response())
    }
}

fn anthropic_sse_response(message: Value) -> Response {
    let content = message
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut start = message.clone();
    if let Some(root) = start.as_object_mut() {
        root.insert("content".to_string(), Value::Array(Vec::new()));
    }
    let mut chunks = Vec::new();
    chunks.push(format!(
        "event: message_start\ndata: {}\n\n",
        json!({"type":"message_start","message": start})
    ));
    for (index, block) in content.iter().enumerate() {
        let start_block = match block.get("type").and_then(Value::as_str).unwrap_or("") {
            "text" => json!({"type":"text","text":""}),
            _ => block.clone(),
        };
        chunks.push(format!(
            "event: content_block_start\ndata: {}\n\n",
            json!({
                "type": "content_block_start",
                "index": index,
                "content_block": start_block,
            })
        ));
        if block.get("type").and_then(Value::as_str) == Some("text") {
            chunks.push(format!(
                "event: content_block_delta\ndata: {}\n\n",
                json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {
                        "type": "text_delta",
                        "text": block.get("text").and_then(Value::as_str).unwrap_or(""),
                    }
                })
            ));
        }
        chunks.push(format!(
            "event: content_block_stop\ndata: {}\n\n",
            json!({
                "type": "content_block_stop",
                "index": index,
            })
        ));
    }
    chunks.push(format!(
        "event: message_delta\ndata: {}\n\n",
        json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": message.get("stop_reason").cloned().unwrap_or(Value::Null),
                "stop_sequence": message.get("stop_sequence").cloned().unwrap_or(Value::Null),
            },
            "usage": {
                "output_tokens": message
                    .get("usage")
                    .and_then(|usage| usage.get("output_tokens"))
                    .cloned()
                    .unwrap_or_else(|| json!(0)),
            }
        })
    ));
    chunks.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string());
    let stream = stream::iter(
        chunks
            .into_iter()
            .map(|chunk| Ok::<Bytes, Infallible>(Bytes::from(chunk))),
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

enum Credential {
    Bearer(String),
    Anthropic(String),
}

impl Credential {
    /// 用预设里保存的上游凭据构造（本机终端路径）。
    fn from_preset(token: &str, anthropic: bool) -> Self {
        if anthropic {
            Credential::Anthropic(token.to_string())
        } else {
            Credential::Bearer(token.to_string())
        }
    }
}

/// 从客户端请求头里读出"真实上游凭据"，用于对外网关透传。
///
/// 只接受非占位的真实 Bearer / x-api-key；占位 token
/// （`webclx-local-api-proxy:` / `webclx-local-claude-proxy:`）是预设身份标识，
/// 必须回到预设解析路径，不能当作客户端凭据。详见
/// docs/codex/tasks/api-preset-routing-boundaries.md。
fn client_provided_credential(headers: &HeaderMap, anthropic: bool) -> Option<Credential> {
    let bearer =
        bearer_token_from_headers(headers).filter(|token| !is_local_proxy_placeholder_token(token));
    let x_api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|token| !token.is_empty() && !is_local_proxy_placeholder_token(token));
    if anthropic {
        x_api_key
            .map(|token| Credential::Anthropic(token.to_string()))
            .or_else(|| bearer.map(|token| Credential::Anthropic(token.to_string())))
    } else {
        bearer
            .map(|token| Credential::Bearer(token.to_string()))
            .or_else(|| x_api_key.map(|token| Credential::Bearer(token.to_string())))
    }
}

fn is_local_proxy_placeholder_token(token: &str) -> bool {
    let trimmed = token.trim();
    // preset-scoped 占位 token (webclx-local-api-proxy:<id> / webclx-local-claude-proxy:<id>)
    // 和旧通用兼容 token (webclx-local-api-proxy / webclx-local-claude-proxy 无 id 后缀)
    // 都是 webClx 预设身份标识, 绝不能当作客户端凭据透传。前者由 preset-id 解析判定,
    // 后者是边界文档定义的兼容路径, 必须回到预设解析。
    auth_core::local_proxy_api_preset_id_from_api_key(trimmed).is_some()
        || auth_core::local_proxy_claude_preset_id_from_token(trimmed).is_some()
        || trimmed == auth_core::LOCAL_PROXY_API_KEY
        || trimmed == auth_core::LOCAL_PROXY_CLAUDE_TOKEN
}

async fn forward_request(
    state: &AppState,
    provider_name: &str,
    method: Method,
    url: String,
    headers: HeaderMap,
    body: Bytes,
    credential: Credential,
) -> ApiResult<Response> {
    let client = state
        .proxy_manager
        .build_app_client(PROXY_TIMEOUT_SECS)
        .map_err(|error| {
            AppError::internal(gateway_message(
                TRANSPARENT_KIND,
                format!("创建上游代理客户端失败: {error}"),
            ))
        })?;
    let mut request = client.request(method, url);
    for (name, value) in headers.iter() {
        if should_forward_request_header(name) {
            request = request.header(name, value);
        }
    }
    request = match credential {
        Credential::Bearer(token) => request.bearer_auth(&token),
        Credential::Anthropic(token) => request
            .header("x-api-key", &token)
            .header(header::AUTHORIZATION, format!("Bearer {token}")),
    };

    let upstream = request.body(body).send().await.map_err(|error| {
        AppError::internal(gateway_message(
            TRANSPARENT_KIND,
            format!("{provider_name} 代理请求失败: {error}"),
        ))
    })?;
    let status = upstream.status();
    let headers = upstream.headers().clone();
    let body = upstream.bytes().await.map_err(|error| {
        AppError::internal(gateway_message(
            TRANSPARENT_KIND,
            format!("读取 {provider_name} 响应失败: {error}"),
        ))
    })?;
    Ok(build_response(status, &headers, body))
}

fn ensure_addr_allowed(state: &AppState, addr: SocketAddr) -> ApiResult<()> {
    // loopback 永远放行（本机终端行为不变）；非 loopback 仅在设置页开启了
    // gateway_listen_non_loopback 对外网关开关时放行。详见
    // docs/codex/tasks/api-preset-routing-boundaries.md。
    if addr.ip().is_loopback() || state.workspace_settings.gateway_listen_non_loopback() {
        Ok(())
    } else {
        Err(AppError {
            status: StatusCode::FORBIDDEN,
            message: gateway_message(
                TRANSPARENT_KIND,
                "上游 API 本机代理只允许本机访问；如需对外，请在设置页开启对外网关开关。",
            ),
        })
    }
}

fn upstream_suffix(uri: &Uri, prefix: &str) -> ApiResult<String> {
    let path = uri.path();
    let suffix = path.strip_prefix(prefix).ok_or_else(|| {
        AppError::bad_request(gateway_message(TRANSPARENT_KIND, "代理路径前缀无效。"))
    })?;
    Ok(if suffix.is_empty() {
        "/".to_string()
    } else {
        suffix.to_string()
    })
}

fn join_upstream_url(base_url: &str, suffix: &str, query: Option<&str>) -> String {
    let mut url = format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        if suffix.starts_with('/') {
            suffix.to_string()
        } else {
            format!("/{suffix}")
        }
    );
    if let Some(query) = query.filter(|value| !value.is_empty()) {
        url.push('?');
        url.push_str(query);
    }
    url
}

fn should_forward_request_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str(),
        "host"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
            | "authorization"
            | "x-api-key"
            | UPSTREAM_PRESET_ID_HEADER
    )
}

fn should_forward_response_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
    )
}

fn build_response(status: reqwest::StatusCode, headers: &HeaderMap, body: Bytes) -> Response {
    let mut builder = Response::builder().status(status);
    if let Some(target_headers) = builder.headers_mut() {
        for (name, value) in headers.iter() {
            if should_forward_response_header(name) {
                target_headers.insert(name.clone(), value.clone());
            }
        }
    }
    builder.body(Body::from(body)).unwrap_or_else(|error| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("构造代理响应失败: {error}")).into_response()
    })
}

fn sse_response(response_payload: Value) -> Response {
    let chunks = response_payload_to_sse_chunks(&response_payload);
    let stream = stream::iter(
        chunks
            .into_iter()
            .map(|chunk| Ok::<Bytes, Infallible>(Bytes::from(chunk))),
    );
    (
        [
            (header::CONTENT_TYPE, "text/event-stream; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

#[cfg(test)]
mod tests;
