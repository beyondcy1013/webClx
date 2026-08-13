use std::{convert::Infallible, net::SocketAddr};

use auth_core::{
    ApiResponsesProxyMode, effective_api_responses_proxy, local_proxy_api_preset_id_from_api_key,
};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderName, StatusCode, header},
    response::{IntoResponse, Response},
};
use codex_proxy_core::{
    anthropic_messages_response_to_chat_response, anthropic_provider_label,
    chat_request_to_anthropic_messages, chat_response_to_responses_payload,
    degrade_resume_chat_request_for_minimax, gateway_message, response_payload_to_sse_chunks,
    responses_request_to_chat_request, sanitize_chat_request_for_deepseek,
    sanitize_chat_request_for_minimax, strip_chat_request_reasoning_content,
};
use futures_util::stream;
use serde_json::Value;
use tracing::warn;

use crate::{ApiResult, AppError, AppState};

/// 校验访问来源是否被允许。
///
/// loopback 永远放行(本机终端行为不变);非 loopback 仅在设置页开启了
/// `gateway_listen_non_loopback` 对外网关开关时放行。详见
/// docs/codex/tasks/api-preset-routing-boundaries.md。
fn ensure_addr_allowed(state: &AppState, addr: SocketAddr, kind: &str) -> ApiResult<()> {
    if addr.ip().is_loopback() || state.workspace_settings.gateway_listen_non_loopback() {
        Ok(())
    } else {
        Err(AppError {
            status: StatusCode::FORBIDDEN,
            message: gateway_message(
                kind,
                "代理只允许本机访问；如需对外，请在设置页开启对外网关开关。".to_string(),
            ),
        })
    }
}

pub use codex_proxy_core::CodexProxyHistory;

const MINIMAX_CHAT_COMPLETIONS_URL: &str = "https://api.minimaxi.com/v1/chat/completions";
const ZHIPU_CHAT_COMPLETIONS_URL: &str =
    "https://open.bigmodel.cn/api/coding/paas/v4/chat/completions";
const DEEPSEEK_CHAT_COMPLETIONS_URL: &str = "https://api.deepseek.com/chat/completions";
const PROXY_TIMEOUT_SECS: u64 = 300;
const OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND: &str = "OpenAI Responses→Chat Completions";
const ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND: &str = "Anthropic Messages→Chat Completions";

#[derive(Clone, Copy)]
struct ProxyProvider {
    name: &'static str,
    upstream_url: &'static str,
    default_model: &'static str,
    client_error_label: &'static str,
    request_error_label: &'static str,
    read_error_label: &'static str,
    invalid_json_label: &'static str,
}

const MINIMAX_PROVIDER: ProxyProvider = ProxyProvider {
    name: "MiniMax",
    upstream_url: MINIMAX_CHAT_COMPLETIONS_URL,
    default_model: "codex-MiniMax-M2.7",
    client_error_label: "创建 MiniMax 代理客户端失败",
    request_error_label: "MiniMax 代理请求失败",
    read_error_label: "读取 MiniMax 响应失败",
    invalid_json_label: "MiniMax 响应不是有效 JSON",
};

const ZHIPU_PROVIDER: ProxyProvider = ProxyProvider {
    name: "Zhipu",
    upstream_url: ZHIPU_CHAT_COMPLETIONS_URL,
    default_model: "GLM-4-Flash",
    client_error_label: "创建智谱代理客户端失败",
    request_error_label: "智谱代理请求失败",
    read_error_label: "读取智谱响应失败",
    invalid_json_label: "智谱响应不是有效 JSON",
};

const DEEPSEEK_PROVIDER: ProxyProvider = ProxyProvider {
    name: "DeepSeek",
    upstream_url: DEEPSEEK_CHAT_COMPLETIONS_URL,
    default_model: "deepseek-chat",
    client_error_label: "创建 DeepSeek 代理客户端失败",
    request_error_label: "DeepSeek 代理请求失败",
    read_error_label: "读取 DeepSeek 响应失败",
    invalid_json_label: "DeepSeek 响应不是有效 JSON",
};

pub async fn minimax_responses(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> ApiResult<Response> {
    proxy_responses(state, addr, headers, payload, MINIMAX_PROVIDER).await
}

pub async fn zhipu_responses(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> ApiResult<Response> {
    proxy_responses(state, addr, headers, payload, ZHIPU_PROVIDER).await
}

pub async fn deepseek_responses(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> ApiResult<Response> {
    proxy_responses(state, addr, headers, payload, DEEPSEEK_PROVIDER).await
}

pub async fn anthropic_responses(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> ApiResult<Response> {
    proxy_anthropic_responses(state, addr, headers, payload).await
}

async fn proxy_responses(
    state: AppState,
    addr: SocketAddr,
    headers: HeaderMap,
    payload: Value,
    provider: ProxyProvider,
) -> ApiResult<Response> {
    ensure_addr_allowed(&state, addr, OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND)?;

    let missing_previous_response = payload
        .get("previous_response_id")
        .and_then(Value::as_str)
        .is_some_and(|response_id| !state.codex_proxy_history.contains(response_id));
    let chat_request = state
        .codex_proxy_history
        .chat_request_with_previous_response(
            &payload,
            responses_request_to_chat_request(&payload).map_err(|error| {
                AppError::bad_request(gateway_message(
                    OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
                    error.to_string(),
                ))
            })?,
        );
    let chat_request = if missing_previous_response {
        degrade_resume_chat_request_for_minimax(chat_request)
    } else {
        sanitize_chat_request_for_minimax(chat_request)
    };
    let chat_request = normalize_provider_chat_request(chat_request, provider);
    let stream_response = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(provider.default_model)
        .to_string();
    if provider.name == "Zhipu" {
        tracing::info!(
            "{}",
            gateway_message(
                OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
                format!("Zhipu 代理收到 model: {model}")
            )
        );
    }

    let client = state
        .proxy_manager
        .build_app_client(PROXY_TIMEOUT_SECS)
        .map_err(|error| {
            AppError::internal(gateway_message(
                OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
                format!("{}: {error}", provider.client_error_label),
            ))
        })?;
    let mut request = client
        .post(provider.upstream_url)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        request = request.header(header::AUTHORIZATION, value.clone());
    }

    let upstream = request.json(&chat_request).send().await.map_err(|error| {
        AppError::internal(gateway_message(
            OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
            format!("{}: {error}", provider.request_error_label),
        ))
    })?;
    let status = upstream.status();
    let upstream_body = upstream.text().await.map_err(|error| {
        AppError::internal(gateway_message(
            OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
            format!("{}: {error}", provider.read_error_label),
        ))
    })?;

    if !status.is_success() {
        warn!(
            "{}",
            gateway_message(
                OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
                format!("{} 上游状态 {}: {}", provider.name, status, upstream_body)
            )
        );
        return Ok((
            status,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            upstream_body,
        )
            .into_response());
    }

    let chat_response: Value = serde_json::from_str(&upstream_body).map_err(|error| {
        AppError::internal(gateway_message(
            OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
            format!("{}: {error}; {upstream_body}", provider.invalid_json_label),
        ))
    })?;
    let response_payload = chat_response_to_responses_payload(&chat_response, &model);
    state
        .codex_proxy_history
        .record_response(&response_payload, &chat_request, &chat_response);

    if stream_response {
        Ok(sse_response(response_payload))
    } else {
        Ok(Json(response_payload).into_response())
    }
}

async fn proxy_anthropic_responses(
    state: AppState,
    addr: SocketAddr,
    headers: HeaderMap,
    payload: Value,
) -> ApiResult<Response> {
    ensure_addr_allowed(&state, addr, ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND)?;

    let preset = resolve_anthropic_preset(&state, &headers).ok_or_else(|| {
        AppError::bad_request(gateway_message(
            ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
            "尚未应用任何 Anthropic 中转站 Codex_API 预设。",
        ))
    })?;

    if !matches!(
        effective_api_responses_proxy(&preset),
        Some(ApiResponsesProxyMode::AnthropicChat)
    ) {
        return Err(AppError::bad_request(gateway_message(
            ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
            "当前 Codex_API 预设未开启 Anthropic 中转站代理。",
        )));
    }

    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("claude-3-5-sonnet-20241022")
        .to_string();
    let stream_response = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let chat_request = state
        .codex_proxy_history
        .chat_request_with_previous_response(
            &payload,
            responses_request_to_chat_request(&payload).map_err(|error| {
                AppError::bad_request(gateway_message(
                    ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
                    error.to_string(),
                ))
            })?,
        );
    // Anthropic-compatible relays accept the standard messages[] shape, so
    // we deliberately do NOT apply the minimax/deepseek sanitizers or the
    // strip-reasoning-content pass — those exist to work around quirks in
    // other Chat-Completions providers, not Anthropic relays.
    let anthropic_payload = chat_request_to_anthropic_messages(&chat_request).map_err(|error| {
        AppError::bad_request(gateway_message(
            ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
            error.to_string(),
        ))
    })?;

    let upstream_url = join_anthropic_messages_url(&preset.base_url);
    let client = state
        .proxy_manager
        .build_app_client(PROXY_TIMEOUT_SECS)
        .map_err(|error| {
            AppError::internal(gateway_message(
                ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
                format!("创建 Anthropic 代理客户端失败: {error}"),
            ))
        })?;
    let mut request = client
        .post(&upstream_url)
        .header(header::CONTENT_TYPE, "application/json")
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", &preset.api_key);
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        // Forward client-supplied Authorization if the upstream expects a
        // Bearer token rather than `x-api-key`. Anthropic's spec uses
        // `x-api-key`, but third-party relays sometimes differ.
        request = request.header(header::AUTHORIZATION, value.clone());
    }

    let upstream = request
        .json(&anthropic_payload)
        .send()
        .await
        .map_err(|error| {
            AppError::internal(gateway_message(
                ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
                format!("Anthropic 代理请求失败: {error}"),
            ))
        })?;
    let status = upstream.status();
    let upstream_body = upstream.text().await.map_err(|error| {
        AppError::internal(gateway_message(
            ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
            format!("读取 Anthropic 响应失败: {error}"),
        ))
    })?;

    if !status.is_success() {
        warn!(
            "{}",
            gateway_message(
                ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
                format!("{} 上游状态 {}: {}", anthropic_provider_label(), status, upstream_body)
            )
        );
        return Ok((
            status,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            upstream_body,
        )
            .into_response());
    }

    let anthropic_response: Value = serde_json::from_str(&upstream_body).map_err(|error| {
        AppError::internal(gateway_message(
            ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
            format!("Anthropic 响应不是有效 JSON: {error}; {upstream_body}"),
        ))
    })?;
    let chat_response = anthropic_messages_response_to_chat_response(&anthropic_response, &model)
        .map_err(|error| {
        AppError::internal(gateway_message(
            ANTHROPIC_MESSAGES_TO_CHAT_COMPLETIONS_KIND,
            error.to_string(),
        ))
    })?;
    let response_payload = chat_response_to_responses_payload(&chat_response, &model);
    state
        .codex_proxy_history
        .record_response(&response_payload, &chat_request, &chat_response);

    if stream_response {
        Ok(sse_response(response_payload))
    } else {
        Ok(Json(response_payload).into_response())
    }
}

fn resolve_anthropic_preset(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<auth_core::StoredApiPreset> {
    let upstream_id_header = HeaderName::from_static("x-webclx-upstream-preset-id");
    if let Some(value) = headers
        .get(&upstream_id_header)
        .and_then(|value| value.to_str().ok())
    {
        let trimmed = value.trim();
        if !trimmed.is_empty()
            && let Some(preset) = state
                .auth_manager
                .api_presets_snapshot()
                .into_iter()
                .find(|preset| preset.id == trimmed)
        {
            return Some(preset);
        }
    }
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or(value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .and_then(local_proxy_api_preset_id_from_api_key)
        && let Some(preset) = state
            .auth_manager
            .api_presets_snapshot()
            .into_iter()
            .find(|preset| preset.id == value)
    {
        return Some(preset);
    }
    if let Some(value) = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .and_then(local_proxy_api_preset_id_from_api_key)
        && let Some(preset) = state
            .auth_manager
            .api_presets_snapshot()
            .into_iter()
            .find(|preset| preset.id == value)
    {
        return Some(preset);
    }
    let settings = state.auth_manager.upstream_proxy_settings();
    settings
        .active_api_proxy_preset_id
        .as_deref()
        .and_then(|id| {
            state
                .auth_manager
                .api_presets_snapshot()
                .into_iter()
                .find(|preset| preset.id == id)
        })
}

fn join_anthropic_messages_url(base_url: &str) -> String {
    // Most third-party anthropic relays expose the endpoint at
    // `${base_url}/v1/messages`. If the operator already includes `/v1` or
    // `/messages`, we still end up with a sensible path because the suffixes
    // are concatenated with a leading slash.
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1/messages") || trimmed.ends_with("/messages") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/messages")
    } else {
        format!("{trimmed}/v1/messages")
    }
}

fn normalize_provider_chat_request(mut chat_request: Value, provider: ProxyProvider) -> Value {
    if provider.name != "DeepSeek" {
        return strip_chat_request_reasoning_content(chat_request);
    }

    chat_request = sanitize_chat_request_for_deepseek(chat_request);
    if let Some(root) = chat_request.as_object_mut()
        && let Some(value) = root.remove("max_completion_tokens")
    {
        root.entry("max_tokens").or_insert(value);
    }
    chat_request
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
