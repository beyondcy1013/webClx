pub(crate) mod environment;
mod streaming;

use std::time::Instant;

use auth_core::{
    ApiResponsesProxyMode, StoredApiPreset, api_preset_enables_local_upstream_proxy_on_apply,
    api_provider_base_url, api_provider_base_url_for_mode, effective_api_responses_proxy,
    local_proxy_api_key_for_preset_id,
};
use reqwest::header;
use serde_json::{Value, json};

const UPSTREAM_PRESET_ID_HEADER: &str = "x-webclx-upstream-preset-id";
const RESPONSES_PROBE_MAX_OUTPUT_TOKENS: u64 = 128;

#[derive(Debug)]
pub struct LlmCallError {
    pub endpoint: String,
    pub status: Option<u16>,
    pub latency_ms: u128,
    pub message: String,
    pub response_body: String,
}

#[derive(Debug, Clone)]
pub struct ChatCompletionReply {
    pub endpoint: String,
    pub status: u16,
    pub latency_ms: u128,
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Vec<Value>>,
    pub finish_reason: String,
    pub response_body: String,
    pub usage: Option<LlmTokenUsage>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LlmTokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone)]
pub enum ConversationStreamEvent {
    TextDelta(String),
    ReasoningDelta(String),
    Completed(ChatCompletionReply),
}

pub use streaming::call_conversation_stream;

#[derive(Debug)]
pub struct ResponsesReply {
    pub endpoint: String,
    pub status: u16,
    pub latency_ms: u128,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProtocol {
    ChatCompletions,
    Responses,
    AnthropicMessages,
}

#[derive(Debug, Clone)]
pub struct ApiPresetLlmTarget {
    pub protocol: LlmProtocol,
    pub base_url: String,
    pub api_key: String,
    pub upstream_preset_id: Option<String>,
}

pub fn api_preset_llm_target(preset: &StoredApiPreset) -> ApiPresetLlmTarget {
    let use_local_proxy = api_preset_enables_local_upstream_proxy_on_apply(preset);
    let mode = effective_api_responses_proxy(preset);
    let protocol = match mode {
        Some(ApiResponsesProxyMode::Direct) => LlmProtocol::Responses,
        Some(ApiResponsesProxyMode::AnthropicChat) if use_local_proxy => LlmProtocol::Responses,
        Some(ApiResponsesProxyMode::AnthropicChat) => LlmProtocol::AnthropicMessages,
        Some(_) => LlmProtocol::ChatCompletions,
        None => LlmProtocol::Responses,
    };

    ApiPresetLlmTarget {
        protocol,
        base_url: if use_local_proxy && matches!(mode, Some(ApiResponsesProxyMode::AnthropicChat)) {
            api_provider_base_url(preset)
        } else if use_local_proxy {
            api_provider_base_url_for_mode(preset, true)
        } else {
            preset.base_url.clone()
        },
        api_key: if use_local_proxy {
            local_proxy_api_key_for_preset_id(&preset.id)
        } else {
            preset.api_key.clone()
        },
        upstream_preset_id: use_local_proxy.then(|| preset.id.clone()),
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ConversationOptions {
    max_output_tokens: Option<u64>,
}

pub async fn call_conversation(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
) -> Result<ChatCompletionReply, LlmCallError> {
    call_conversation_with_options(
        client,
        target,
        model,
        messages,
        tools,
        ConversationOptions::default(),
    )
    .await
}

pub async fn probe_conversation(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    input: &str,
) -> Result<ChatCompletionReply, LlmCallError> {
    call_conversation_with_options(
        client,
        target,
        model,
        vec![json!({"role": "user", "content": input})],
        Vec::new(),
        ConversationOptions {
            // Reasoning models count hidden reasoning against this budget.
            // A 16-token probe can end as `response.incomplete` before the
            // short visible confirmation is emitted.
            max_output_tokens: Some(RESPONSES_PROBE_MAX_OUTPUT_TOKENS),
        },
    )
    .await
}

async fn call_conversation_with_options(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
    options: ConversationOptions,
) -> Result<ChatCompletionReply, LlmCallError> {
    match target.protocol {
        LlmProtocol::ChatCompletions => {
            call_chat_completions(
                client,
                &target.base_url,
                &target.api_key,
                target.upstream_preset_id.as_deref(),
                model,
                messages,
                tools,
                options,
            )
            .await
        }
        LlmProtocol::Responses => {
            call_responses_conversation(client, target, model, messages, tools, options).await
        }
        LlmProtocol::AnthropicMessages => {
            call_anthropic_conversation(client, target, model, messages, tools, options).await
        }
    }
}

pub async fn call_responses_probe(
    client: &reqwest::Client,
    endpoint: &str,
    bearer_token: &str,
    account_id: Option<&str>,
    model: &str,
    input: &str,
) -> Result<ResponsesReply, LlmCallError> {
    call_responses(client, endpoint, bearer_token, account_id, model, input).await
}

pub fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else if trimmed.contains("/v1/") || trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

pub fn responses_url(base_url: &str) -> String {
    append_protocol_path(base_url, "responses")
}

pub fn anthropic_messages_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1/messages") || trimmed.ends_with("/messages") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/messages")
    } else {
        format!("{trimmed}/v1/messages")
    }
}

fn append_protocol_path(base_url: &str, path: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with(&format!("/{path}")) {
        trimmed.to_string()
    } else {
        format!("{trimmed}/{path}")
    }
}

fn enable_chat_streaming(body: &mut Value) {
    body["stream"] = Value::Bool(true);
    body["stream_options"] = json!({"include_usage": true});
}

async fn call_chat_completions(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    upstream_preset_id: Option<&str>,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
    options: ConversationOptions,
) -> Result<ChatCompletionReply, LlmCallError> {
    let endpoint = chat_completions_url(base_url);
    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": false,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = Value::String("auto".to_string());
    }
    if let Some(max_output_tokens) = options.max_output_tokens {
        body["max_tokens"] = Value::from(max_output_tokens);
    }
    let started = Instant::now();
    let mut request = client
        .post(&endpoint)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
        .json(&body);
    if let Some(preset_id) = upstream_preset_id {
        request = request.header(UPSTREAM_PRESET_ID_HEADER, preset_id);
    }
    let response = request.send().await.map_err(|error| LlmCallError {
        endpoint: endpoint.clone(),
        status: None,
        latency_ms: started.elapsed().as_millis(),
        message: format!("LLM 请求失败: {error}"),
        response_body: String::new(),
    })?;
    let status = response.status();
    let response_text = response.text().await.map_err(|error| LlmCallError {
        endpoint: endpoint.clone(),
        status: Some(status.as_u16()),
        latency_ms: started.elapsed().as_millis(),
        message: format!("读取 LLM 响应失败: {error}"),
        response_body: String::new(),
    })?;
    if !status.is_success() {
        return Err(LlmCallError {
            endpoint,
            status: Some(status.as_u16()),
            latency_ms: started.elapsed().as_millis(),
            message: format!("LLM 返回错误 {status}: {}", truncate_body(&response_text)),
            response_body: response_text,
        });
    }
    let response_json: Value =
        serde_json::from_str(&response_text).map_err(|error| LlmCallError {
            endpoint: endpoint.clone(),
            status: Some(status.as_u16()),
            latency_ms: started.elapsed().as_millis(),
            message: format!("LLM 响应解析失败: {error}; body: {}", truncate_body(&response_text)),
            response_body: response_text.clone(),
        })?;
    let choice = response_json
        .get("choices")
        .and_then(|choices| choices.get(0))
        .ok_or_else(|| LlmCallError {
            endpoint: endpoint.clone(),
            status: Some(status.as_u16()),
            latency_ms: started.elapsed().as_millis(),
            message: "LLM 响应缺少 choices".to_string(),
            response_body: response_text.clone(),
        })?;
    let message = choice.get("message").ok_or_else(|| LlmCallError {
        endpoint: endpoint.clone(),
        status: Some(status.as_u16()),
        latency_ms: started.elapsed().as_millis(),
        message: "LLM 响应缺少 message".to_string(),
        response_body: response_text.clone(),
    })?;
    Ok(ChatCompletionReply {
        endpoint,
        status: status.as_u16(),
        latency_ms: started.elapsed().as_millis(),
        content: message
            .get("content")
            .and_then(Value::as_str)
            .map(str::to_string),
        reasoning_content: message
            .get("reasoning_content")
            .and_then(Value::as_str)
            .map(str::to_string),
        tool_calls: message.get("tool_calls").and_then(Value::as_array).cloned(),
        finish_reason: choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .unwrap_or("stop")
            .to_string(),
        response_body: response_text,
        usage: response_json.get("usage").and_then(openai_token_usage),
    })
}

async fn call_responses_conversation(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
    options: ConversationOptions,
) -> Result<ChatCompletionReply, LlmCallError> {
    let endpoint = responses_url(&target.base_url);
    let body = responses_conversation_request_body(model, &messages, &tools, options);
    let started = Instant::now();
    let mut request = client
        .post(&endpoint)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "text/event-stream")
        .header(header::AUTHORIZATION, format!("Bearer {}", target.api_key))
        .header(header::USER_AGENT, "codex-cli")
        .json(&body);
    if let Some(preset_id) = target.upstream_preset_id.as_deref() {
        request = request.header(UPSTREAM_PRESET_ID_HEADER, preset_id);
    }
    let response = request.send().await.map_err(|error| LlmCallError {
        endpoint: endpoint.clone(),
        status: None,
        latency_ms: started.elapsed().as_millis(),
        message: format!("LLM 请求失败: {error}"),
        response_body: String::new(),
    })?;
    let status = response.status();
    let response_text = response.text().await.map_err(|error| LlmCallError {
        endpoint: endpoint.clone(),
        status: Some(status.as_u16()),
        latency_ms: started.elapsed().as_millis(),
        message: format!("读取 LLM 响应失败: {error}"),
        response_body: String::new(),
    })?;
    let latency_ms = started.elapsed().as_millis();
    if !status.is_success() {
        return Err(LlmCallError {
            endpoint,
            status: Some(status.as_u16()),
            latency_ms,
            message: format!("LLM 返回错误 {status}: {}", truncate_body(&response_text)),
            response_body: response_text,
        });
    }
    let response_value = completed_responses_value(&response_text).ok_or_else(|| LlmCallError {
        endpoint: endpoint.clone(),
        status: Some(status.as_u16()),
        latency_ms,
        message: "LLM Responses 流在收到 response.completed 前结束。".to_string(),
        response_body: response_text.clone(),
    })?;
    let tool_calls = extract_responses_tool_calls(&response_value);
    Ok(ChatCompletionReply {
        endpoint,
        status: status.as_u16(),
        latency_ms,
        content: extract_text_from_responses_value(&response_value),
        reasoning_content: None,
        finish_reason: if tool_calls.is_some() {
            "tool_calls".to_string()
        } else {
            "stop".to_string()
        },
        tool_calls,
        response_body: response_text,
        usage: response_value.get("usage").and_then(responses_token_usage),
    })
}

async fn call_anthropic_conversation(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
    options: ConversationOptions,
) -> Result<ChatCompletionReply, LlmCallError> {
    let endpoint = anthropic_messages_url(&target.base_url);
    let body = anthropic_conversation_request_body(model, &messages, &tools, options);
    let started = Instant::now();
    let response = client
        .post(&endpoint)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json")
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", &target.api_key)
        .header(header::AUTHORIZATION, format!("Bearer {}", target.api_key))
        .json(&body)
        .send()
        .await
        .map_err(|error| LlmCallError {
            endpoint: endpoint.clone(),
            status: None,
            latency_ms: started.elapsed().as_millis(),
            message: format!("LLM 请求失败: {error}"),
            response_body: String::new(),
        })?;
    let status = response.status();
    let response_text = response.text().await.map_err(|error| LlmCallError {
        endpoint: endpoint.clone(),
        status: Some(status.as_u16()),
        latency_ms: started.elapsed().as_millis(),
        message: format!("读取 LLM 响应失败: {error}"),
        response_body: String::new(),
    })?;
    let latency_ms = started.elapsed().as_millis();
    if !status.is_success() {
        return Err(LlmCallError {
            endpoint,
            status: Some(status.as_u16()),
            latency_ms,
            message: format!("LLM 返回错误 {status}: {}", truncate_body(&response_text)),
            response_body: response_text,
        });
    }
    let response_json: Value =
        serde_json::from_str(&response_text).map_err(|error| LlmCallError {
            endpoint: endpoint.clone(),
            status: Some(status.as_u16()),
            latency_ms,
            message: format!("LLM 响应解析失败: {error}; body: {}", truncate_body(&response_text)),
            response_body: response_text.clone(),
        })?;
    let content = response_json
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let tool_calls = extract_anthropic_tool_calls(&response_json);
    Ok(ChatCompletionReply {
        endpoint,
        status: status.as_u16(),
        latency_ms,
        content: (!content.trim().is_empty()).then_some(content),
        reasoning_content: None,
        tool_calls,
        finish_reason: response_json
            .get("stop_reason")
            .and_then(Value::as_str)
            .unwrap_or("stop")
            .to_string(),
        response_body: response_text,
        usage: response_json.get("usage").and_then(anthropic_token_usage),
    })
}

fn token_count(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
}

fn openai_token_usage(value: &Value) -> Option<LlmTokenUsage> {
    let input_tokens = token_count(value, &["prompt_tokens", "input_tokens"]);
    let output_tokens = token_count(value, &["completion_tokens", "output_tokens"]);
    let total_tokens =
        token_count(value, &["total_tokens"]).max(input_tokens.saturating_add(output_tokens));
    (input_tokens > 0 || output_tokens > 0 || total_tokens > 0).then_some(LlmTokenUsage {
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

fn responses_token_usage(value: &Value) -> Option<LlmTokenUsage> {
    let input_tokens = token_count(value, &["input_tokens", "prompt_tokens"]);
    let output_tokens = token_count(value, &["output_tokens", "completion_tokens"]);
    let total_tokens =
        token_count(value, &["total_tokens"]).max(input_tokens.saturating_add(output_tokens));
    (input_tokens > 0 || output_tokens > 0 || total_tokens > 0).then_some(LlmTokenUsage {
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

fn anthropic_token_usage(value: &Value) -> Option<LlmTokenUsage> {
    responses_token_usage(value)
}

fn responses_conversation_request_body(
    model: &str,
    messages: &[Value],
    tools: &[Value],
    options: ConversationOptions,
) -> Value {
    let mut instructions = Vec::new();
    let mut input = Vec::new();
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        if role == "system" {
            let text = value_to_text(message.get("content").unwrap_or(&Value::Null));
            if !text.trim().is_empty() {
                instructions.push(text);
            }
            continue;
        }

        if role == "tool" {
            input.push(json!({
                "type": "function_call_output",
                "call_id": message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_unknown"),
                "output": value_to_text(message.get("content").unwrap_or(&Value::Null)),
            }));
            continue;
        }

        let content =
            responses_message_content(role, message.get("content").unwrap_or(&Value::Null));
        if !content.is_empty() {
            input.push(json!({
                "type": "message",
                "role": role,
                "content": content,
            }));
        }
        if role == "assistant"
            && let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array)
        {
            for tool_call in tool_calls {
                let function = tool_call.get("function").unwrap_or(&Value::Null);
                input.push(json!({
                    "type": "function_call",
                    "call_id": tool_call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("call_unknown"),
                    "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
                    "arguments": function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("{}"),
                }));
            }
        }
    }

    let response_tools = tools
        .iter()
        .filter_map(chat_tool_to_responses_tool)
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": model,
        "instructions": instructions.join("\n\n"),
        "input": input,
        "parallel_tool_calls": false,
        "store": false,
        "stream": true,
        "include": ["reasoning.encrypted_content"],
    });
    if !response_tools.is_empty() {
        body["tools"] = Value::Array(response_tools);
        body["tool_choice"] = Value::String("auto".to_string());
    }
    if let Some(max_output_tokens) = options.max_output_tokens {
        body["max_output_tokens"] = Value::from(max_output_tokens);
    }
    body
}

fn responses_message_content(role: &str, value: &Value) -> Vec<Value> {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .filter_map(|item| match item.get("type").and_then(Value::as_str) {
                Some("text") => item.get("text").and_then(Value::as_str).map(|text| {
                    json!({
                        "type": if role == "assistant" { "output_text" } else { "input_text" },
                        "text": text,
                    })
                }),
                Some("image_url") if role != "assistant" => item
                    .get("image_url")
                    .and_then(|image| image.get("url"))
                    .and_then(Value::as_str)
                    .map(|url| json!({"type": "input_image", "image_url": url})),
                _ => None,
            })
            .collect();
    }
    let text = value_to_text(value);
    (!text.trim().is_empty())
        .then(|| {
            json!({
                "type": if role == "assistant" { "output_text" } else { "input_text" },
                "text": text,
            })
        })
        .into_iter()
        .collect()
}

fn chat_tool_to_responses_tool(tool: &Value) -> Option<Value> {
    let function = tool.get("function")?;
    Some(json!({
        "type": "function",
        "name": function.get("name")?.as_str()?,
        "description": function
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
        "parameters": function
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
    }))
}

fn anthropic_conversation_request_body(
    model: &str,
    messages: &[Value],
    tools: &[Value],
    options: ConversationOptions,
) -> Value {
    let system = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        .map(|message| value_to_text(message.get("content").unwrap_or(&Value::Null)))
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut anthropic_messages = Vec::new();
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        if role == "system" {
            continue;
        }
        let mut content = Vec::new();
        if role == "tool" {
            content.push(json!({
                "type": "tool_result",
                "tool_use_id": message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_unknown"),
                "content": value_to_text(message.get("content").unwrap_or(&Value::Null)),
            }));
        } else {
            content
                .extend(anthropic_message_content(message.get("content").unwrap_or(&Value::Null)));
            if role == "assistant"
                && let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array)
            {
                for tool_call in tool_calls {
                    let function = tool_call.get("function").unwrap_or(&Value::Null);
                    let arguments = function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .and_then(|value| serde_json::from_str::<Value>(value).ok())
                        .unwrap_or_else(|| json!({}));
                    content.push(json!({
                        "type": "tool_use",
                        "id": tool_call
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("call_unknown"),
                        "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
                        "input": arguments,
                    }));
                }
            }
        }
        if !content.is_empty() {
            anthropic_messages.push(json!({
                "role": if role == "assistant" { "assistant" } else { "user" },
                "content": content,
            }));
        }
    }

    let anthropic_tools = tools
        .iter()
        .filter_map(chat_tool_to_anthropic_tool)
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": model,
        "system": system,
        "messages": anthropic_messages,
        "max_tokens": options.max_output_tokens.unwrap_or(4096),
        "stream": false,
    });
    if !anthropic_tools.is_empty() {
        body["tools"] = Value::Array(anthropic_tools);
        body["tool_choice"] = json!({"type": "auto"});
    }
    body
}

fn anthropic_message_content(value: &Value) -> Vec<Value> {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .filter_map(|item| match item.get("type").and_then(Value::as_str) {
                Some("text") => item
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|text| json!({"type": "text", "text": text})),
                Some("image_url") => item
                    .get("image_url")
                    .and_then(|image| image.get("url"))
                    .and_then(Value::as_str)
                    .and_then(parse_image_data_url)
                    .map(|(media_type, data)| {
                        json!({
                            "type": "image",
                            "source": {"type": "base64", "media_type": media_type, "data": data}
                        })
                    }),
                _ => None,
            })
            .collect();
    }
    let text = value_to_text(value);
    (!text.trim().is_empty())
        .then(|| json!({"type": "text", "text": text}))
        .into_iter()
        .collect()
}

fn parse_image_data_url(value: &str) -> Option<(&str, &str)> {
    let body = value.strip_prefix("data:")?;
    let (media_type, data) = body.split_once(";base64,")?;
    media_type
        .starts_with("image/")
        .then_some((media_type, data))
}

fn chat_tool_to_anthropic_tool(tool: &Value) -> Option<Value> {
    let function = tool.get("function")?;
    Some(json!({
        "name": function.get("name")?.as_str()?,
        "description": function
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(""),
        "input_schema": function
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
    }))
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                serde_json::to_string(value).unwrap_or_default()
            } else {
                text
            }
        }
        Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn completed_responses_value(body: &str) -> Option<Value> {
    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("response.completed") {
            return value.get("response").cloned().or(Some(value));
        }
        if matches!(
            value.get("type").and_then(Value::as_str),
            Some("response.failed") | Some("response.incomplete")
        ) {
            return None;
        }
    }

    let value = serde_json::from_str::<Value>(body).ok()?;
    if value.get("type").and_then(Value::as_str) == Some("response.completed") {
        value.get("response").cloned().or(Some(value))
    } else if value.get("status").and_then(Value::as_str) == Some("completed") {
        Some(value)
    } else {
        None
    }
}

fn extract_responses_tool_calls(value: &Value) -> Option<Vec<Value>> {
    let tool_calls = value
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .map(|item| {
            json!({
                "id": item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("id").and_then(Value::as_str))
                    .unwrap_or("call_unknown"),
                "type": "function",
                "function": {
                    "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                    "arguments": item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("{}"),
                }
            })
        })
        .collect::<Vec<_>>();
    (!tool_calls.is_empty()).then_some(tool_calls)
}

fn extract_anthropic_tool_calls(value: &Value) -> Option<Vec<Value>> {
    let tool_calls = value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("tool_use"))
        .map(|item| {
            json!({
                "id": item.get("id").and_then(Value::as_str).unwrap_or("call_unknown"),
                "type": "function",
                "function": {
                    "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                    "arguments": serde_json::to_string(
                        item.get("input").unwrap_or(&Value::Null)
                    ).unwrap_or_else(|_| "{}".to_string()),
                }
            })
        })
        .collect::<Vec<_>>();
    (!tool_calls.is_empty()).then_some(tool_calls)
}

async fn call_responses(
    client: &reqwest::Client,
    endpoint: &str,
    bearer_token: &str,
    account_id: Option<&str>,
    model: &str,
    input: &str,
) -> Result<ResponsesReply, LlmCallError> {
    let body = responses_request_body(model, input);
    let started = Instant::now();
    let mut request = client
        .post(endpoint)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "text/event-stream")
        .header(header::AUTHORIZATION, format!("Bearer {bearer_token}"))
        .header(header::USER_AGENT, "codex-cli")
        .json(&body);
    if let Some(account_id) = account_id.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.header("ChatGPT-Account-Id", account_id);
    }
    let response = request.send().await.map_err(|error| LlmCallError {
        endpoint: endpoint.to_string(),
        status: None,
        latency_ms: started.elapsed().as_millis(),
        message: format!("LLM 请求失败: {error}"),
        response_body: String::new(),
    })?;
    let status = response.status();
    let response_text = response.text().await.map_err(|error| LlmCallError {
        endpoint: endpoint.to_string(),
        status: Some(status.as_u16()),
        latency_ms: started.elapsed().as_millis(),
        message: format!("读取 LLM 响应失败: {error}"),
        response_body: String::new(),
    })?;
    let latency_ms = started.elapsed().as_millis();
    if !status.is_success() {
        return Err(LlmCallError {
            endpoint: endpoint.to_string(),
            status: Some(status.as_u16()),
            latency_ms,
            message: format!("LLM 返回错误 {status}: {}", truncate_body(&response_text)),
            response_body: response_text,
        });
    }
    if !responses_body_has_terminal_completion(&response_text) {
        return Err(LlmCallError {
            endpoint: endpoint.to_string(),
            status: Some(status.as_u16()),
            latency_ms,
            message: "LLM Responses 流在收到 response.completed 前结束。".to_string(),
            response_body: response_text,
        });
    }

    Ok(ResponsesReply {
        endpoint: endpoint.to_string(),
        status: status.as_u16(),
        latency_ms,
        content: extract_responses_text(&response_text),
    })
}

pub(crate) fn responses_request_body(model: &str, input: &str) -> Value {
    let messages = [json!({"role": "user", "content": input})];
    let mut body =
        responses_conversation_request_body(model, &messages, &[], ConversationOptions::default());
    body["instructions"] =
        Value::String("Reply briefly to confirm the model is available.".to_string());
    body
}

pub fn responses_body_has_terminal_completion(body: &str) -> bool {
    let mut saw_sse_data = false;
    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        saw_sse_data = true;
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("response.completed") => return true,
            Some("response.failed") | Some("response.incomplete") => return false,
            _ => {}
        }
    }
    if saw_sse_data {
        return false;
    }

    serde_json::from_str::<Value>(body)
        .ok()
        .is_some_and(|value| {
            value.get("type").and_then(Value::as_str) == Some("response.completed")
                || value.get("status").and_then(Value::as_str) == Some("completed")
        })
}

pub fn extract_responses_text(body: &str) -> Option<String> {
    let mut deltas = String::new();
    let mut completed_text = None;
    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    deltas.push_str(delta);
                }
            }
            Some("response.output_text.done") => {
                completed_text = value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            Some("response.completed") => {
                completed_text = completed_text.or_else(|| {
                    value
                        .get("response")
                        .and_then(extract_text_from_responses_value)
                });
            }
            _ => {}
        }
    }
    if !deltas.trim().is_empty() {
        return Some(deltas);
    }
    if completed_text
        .as_deref()
        .is_some_and(|text| !text.trim().is_empty())
    {
        return completed_text;
    }
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| extract_text_from_responses_value(&value))
}

fn extract_text_from_responses_value(value: &Value) -> Option<String> {
    if let Some(text) = value
        .get("output_text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return Some(text.to_string());
    }
    let output = value.get("output").and_then(Value::as_array)?;
    let text = output
        .iter()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter_map(|content| {
            content
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| content.get("output_text").and_then(Value::as_str))
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then_some(text)
}

fn truncate_body(value: &str) -> String {
    const LIMIT: usize = 2_000;
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(LIMIT).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn sample_api_preset(
        responses_proxy: Option<ApiResponsesProxyMode>,
        use_local_proxy: bool,
    ) -> StoredApiPreset {
        StoredApiPreset {
            id: "api-test".to_string(),
            name: "API Test".to_string(),
            saved_at: 0,
            provider_name: "test-provider".to_string(),
            base_url: "https://example.test/v1".to_string(),
            management_url: None,
            wire_api: Some("responses".to_string()),
            responses_proxy,
            apply_upstream_proxy_on_switch: use_local_proxy,
            config_overrides: Vec::new(),
            legacy_config_key: None,
            legacy_config_value: None,
            legacy_secondary_config_key: None,
            legacy_secondary_config_value: None,
            terminal_env: Vec::new(),
            terminal_startup_script: None,
            api_key: "real-key".to_string(),
            access_token: String::new(),
            account_id: String::new(),
            access_mode: None,
            switch_count: 0,
        }
    }

    #[test]
    fn api_target_uses_scoped_local_relay_for_chat_presets() {
        let preset = sample_api_preset(Some(ApiResponsesProxyMode::OpenaiChat), true);

        let target = api_preset_llm_target(&preset);

        assert_eq!(target.protocol, LlmProtocol::ChatCompletions);
        assert!(target.base_url.ends_with("/api/upstream/openai/v1"));
        assert_eq!(target.api_key, local_proxy_api_key_for_preset_id(&preset.id));
        assert_eq!(target.upstream_preset_id.as_deref(), Some("api-test"));
    }

    #[test]
    fn api_target_routes_local_anthropic_presets_through_responses_conversion() {
        let preset = sample_api_preset(Some(ApiResponsesProxyMode::AnthropicChat), true);

        let target = api_preset_llm_target(&preset);

        assert_eq!(target.protocol, LlmProtocol::Responses);
        assert!(target.base_url.ends_with("/api/codex-proxy/anthropic/v1"));
        assert_eq!(target.api_key, local_proxy_api_key_for_preset_id(&preset.id));
        assert_eq!(target.upstream_preset_id.as_deref(), Some("api-test"));
    }

    #[test]
    fn api_target_keeps_direct_anthropic_presets_on_native_messages() {
        let preset = sample_api_preset(Some(ApiResponsesProxyMode::AnthropicChat), false);

        let target = api_preset_llm_target(&preset);

        assert_eq!(target.protocol, LlmProtocol::AnthropicMessages);
        assert_eq!(target.base_url, preset.base_url);
        assert_eq!(target.api_key, preset.api_key);
        assert_eq!(target.upstream_preset_id, None);
    }

    #[test]
    fn responses_conversation_preserves_tool_calls_and_results() {
        let body = responses_conversation_request_body(
            "gpt-test",
            &[
                json!({"role": "system", "content": "rules"}),
                json!({"role": "user", "content": "list files"}),
                json!({
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "run_command", "arguments": "{\"command\":\"ls\"}"}
                    }]
                }),
                json!({
                    "role": "tool",
                    "tool_call_id": "call_1",
                    "content": {"files": ["Cargo.toml"]}
                }),
            ],
            &[json!({
                "type": "function",
                "function": {
                    "name": "run_command",
                    "description": "run",
                    "parameters": {"type": "object"}
                }
            })],
            ConversationOptions::default(),
        );

        assert_eq!(body["instructions"], "rules");
        assert_eq!(body["input"][1]["type"], "function_call");
        assert_eq!(body["input"][2]["type"], "function_call_output");
        assert_eq!(body["tools"][0]["name"], "run_command");
    }

    #[test]
    fn conversation_probe_omits_empty_tool_configuration() {
        let responses = responses_conversation_request_body(
            "gpt-test",
            &[json!({"role": "user", "content": "hi"})],
            &[],
            ConversationOptions {
                max_output_tokens: Some(RESPONSES_PROBE_MAX_OUTPUT_TOKENS),
            },
        );
        let anthropic = anthropic_conversation_request_body(
            "claude-test",
            &[json!({"role": "user", "content": "hi"})],
            &[],
            ConversationOptions {
                max_output_tokens: Some(16),
            },
        );

        assert!(responses.get("tools").is_none());
        assert!(responses.get("tool_choice").is_none());
        assert_eq!(responses["max_output_tokens"], 128);
        assert!(anthropic.get("tools").is_none());
        assert!(anthropic.get("tool_choice").is_none());
        assert_eq!(anthropic["max_tokens"], 16);
    }

    #[test]
    fn extracts_responses_function_calls() {
        let value = json!({
            "status": "completed",
            "output": [{
                "type": "function_call",
                "call_id": "call_1",
                "name": "run_command",
                "arguments": "{\"command\":\"pwd\"}"
            }]
        });

        let calls = extract_responses_tool_calls(&value).unwrap();

        assert_eq!(calls[0]["id"], "call_1");
        assert_eq!(calls[0]["function"]["name"], "run_command");
    }

    #[test]
    fn anthropic_conversation_maps_tools_and_results() {
        let body = anthropic_conversation_request_body(
            "claude-test",
            &[
                json!({"role": "system", "content": "rules"}),
                json!({"role": "user", "content": "inspect"}),
                json!({
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "run_command", "arguments": "{\"command\":\"pwd\"}"}
                    }]
                }),
                json!({"role": "tool", "tool_call_id": "call_1", "content": "/tmp"}),
            ],
            &[json!({
                "type": "function",
                "function": {
                    "name": "run_command",
                    "parameters": {"type": "object"}
                }
            })],
            ConversationOptions::default(),
        );

        assert_eq!(body["system"], "rules");
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(body["tools"][0]["name"], "run_command");
    }

    #[test]
    fn extracts_responses_sse_deltas() {
        let body = "event: response.output_text.delta\n\
                    data: {\"type\":\"response.output_text.delta\",\"delta\":\"h\"}\n\n\
                    event: response.output_text.delta\n\
                    data: {\"type\":\"response.output_text.delta\",\"delta\":\"i\"}\n\n";

        assert_eq!(extract_responses_text(body).as_deref(), Some("hi"));
    }

    #[test]
    fn extracts_responses_json_output() {
        let body = r#"{"output":[{"content":[{"type":"output_text","text":"hi"}]}]}"#;

        assert_eq!(extract_responses_text(body).as_deref(), Some("hi"));
    }

    #[test]
    fn responses_completion_requires_terminal_event() {
        let complete = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n\
                        data: {\"type\":\"response.completed\"}\n\n";
        let incomplete = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n";

        assert!(responses_body_has_terminal_completion(complete));
        assert!(!responses_body_has_terminal_completion(incomplete));
        assert!(responses_body_has_terminal_completion(r#"{"status":"completed","output":[]}"#));
    }

    async fn spawn_llm_server(response_bodies: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_for_task = Arc::clone(&requests);
        tokio::spawn(async move {
            for body in response_bodies {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0_u8; 4096];
                    let read = stream.read(&mut chunk).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..read]);
                    let Some(header_end) =
                        request.windows(4).position(|window| window == b"\r\n\r\n")
                    else {
                        continue;
                    };
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.split_once(':').and_then(|(name, value)| {
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
                requests_for_task
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&request).to_string());
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (format!("http://{addr}"), requests)
    }

    fn test_target(
        protocol: LlmProtocol,
        base_url: String,
        api_key: &str,
        upstream_preset_id: Option<&str>,
    ) -> ApiPresetLlmTarget {
        ApiPresetLlmTarget {
            protocol,
            base_url,
            api_key: api_key.to_string(),
            upstream_preset_id: upstream_preset_id.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn chat_transport_sends_scoped_headers_and_parses_tool_calls() {
        let (origin, requests) = spawn_llm_server(vec![
            r#"{"choices":[{"message":{"content":"hi","tool_calls":[{"id":"call_1","type":"function","function":{"name":"run_command","arguments":"{\"command\":\"pwd\"}"}}]},"finish_reason":"tool_calls"}]}"#.to_string(),
        ])
        .await;
        let target = test_target(
            LlmProtocol::ChatCompletions,
            origin,
            "webclx-local-api-proxy:api-test",
            Some("api-test"),
        );

        let reply = call_conversation(
            &reqwest::Client::new(),
            &target,
            "gpt-test",
            vec![json!({"role": "user", "content": "hi"})],
            vec![json!({
                "type": "function",
                "function": {
                    "name": "run_command",
                    "parameters": {"type": "object"}
                }
            })],
        )
        .await
        .unwrap();

        assert_eq!(reply.content.as_deref(), Some("hi"));
        assert_eq!(reply.tool_calls.as_ref().unwrap()[0]["function"]["name"], "run_command");
        let request = requests.lock().unwrap()[0].to_ascii_lowercase();
        assert!(request.starts_with("post /v1/chat/completions "));
        assert!(request.contains("authorization: bearer webclx-local-api-proxy:api-test"));
        assert!(request.contains("x-webclx-upstream-preset-id: api-test"));
        assert!(request.contains(r#""tool_choice":"auto""#));
    }

    #[tokio::test]
    async fn responses_transport_requires_completion_and_parses_function_calls() {
        let completed = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"hi\"}]},{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"run_command\",\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\"}]}}\n\n";
        let (origin, requests) = spawn_llm_server(vec![completed.to_string()]).await;
        let target =
            test_target(LlmProtocol::Responses, format!("{origin}/v1"), "response-key", None);

        let reply = call_conversation(
            &reqwest::Client::new(),
            &target,
            "gpt-test",
            vec![json!({"role": "user", "content": "hi"})],
            Vec::new(),
        )
        .await
        .unwrap();

        assert_eq!(reply.content.as_deref(), Some("hi"));
        assert_eq!(reply.finish_reason, "tool_calls");
        assert_eq!(reply.tool_calls.as_ref().unwrap()[0]["function"]["name"], "run_command");
        let request = requests.lock().unwrap()[0].to_ascii_lowercase();
        assert!(request.starts_with("post /v1/responses "));
        assert!(request.contains("accept: text/event-stream"));
        assert!(!request.contains(r#""tool_choice""#));
    }

    #[tokio::test]
    async fn responses_transport_rejects_http_success_without_completion() {
        let (origin, _) = spawn_llm_server(vec![
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n".to_string(),
        ])
        .await;
        let target =
            test_target(LlmProtocol::Responses, format!("{origin}/v1"), "response-key", None);

        let error = probe_conversation(&reqwest::Client::new(), &target, "gpt-test", "hi")
            .await
            .unwrap_err();

        assert_eq!(error.status, Some(200));
        assert!(error.message.contains("response.completed"));
    }

    #[tokio::test]
    async fn anthropic_transport_sends_native_headers_and_parses_tool_use() {
        let (origin, requests) = spawn_llm_server(vec![
            r#"{"content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"tool_1","name":"run_command","input":{"command":"pwd"}}],"stop_reason":"tool_use"}"#.to_string(),
        ])
        .await;
        let target = test_target(LlmProtocol::AnthropicMessages, origin, "anthropic-key", None);

        let reply = call_conversation(
            &reqwest::Client::new(),
            &target,
            "claude-test",
            vec![json!({"role": "user", "content": "hi"})],
            vec![json!({
                "type": "function",
                "function": {
                    "name": "run_command",
                    "parameters": {"type": "object"}
                }
            })],
        )
        .await
        .unwrap();

        assert_eq!(reply.content.as_deref(), Some("hi"));
        assert_eq!(reply.tool_calls.as_ref().unwrap()[0]["function"]["name"], "run_command");
        let request = requests.lock().unwrap()[0].to_ascii_lowercase();
        assert!(request.starts_with("post /v1/messages "));
        assert!(request.contains("x-api-key: anthropic-key"));
        assert!(request.contains("anthropic-version: 2023-06-01"));
        assert!(request.contains("authorization: bearer anthropic-key"));
    }
}
