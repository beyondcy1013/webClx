use std::{collections::BTreeMap, time::Instant};

use futures_util::StreamExt;
use reqwest::header;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{
    ApiPresetLlmTarget, ChatCompletionReply, ConversationOptions, ConversationStreamEvent,
    LlmCallError, LlmProtocol, LlmTokenUsage, UPSTREAM_PRESET_ID_HEADER,
    anthropic_conversation_request_body, anthropic_messages_url, anthropic_token_usage,
    chat_completions_url, completed_responses_value, enable_chat_streaming,
    extract_anthropic_tool_calls, extract_responses_tool_calls, extract_text_from_responses_value,
    openai_token_usage, responses_conversation_request_body, responses_token_usage, responses_url,
    truncate_body,
};

#[derive(Default)]
struct ChatToolDelta {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct StreamAccumulator {
    content: String,
    reasoning_content: String,
    finish_reason: String,
    usage: Option<LlmTokenUsage>,
    chat_tools: BTreeMap<usize, ChatToolDelta>,
    anthropic_tools: BTreeMap<usize, Value>,
    completed_value: Option<Value>,
}

pub async fn call_conversation_stream(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
    events: mpsc::Sender<ConversationStreamEvent>,
) -> Result<ChatCompletionReply, LlmCallError> {
    let options = ConversationOptions::default();
    let result = match target.protocol {
        LlmProtocol::ChatCompletions => {
            stream_chat_completions(client, target, model, messages, tools, options, &events).await
        }
        LlmProtocol::Responses => {
            stream_responses(client, target, model, messages, tools, options, &events).await
        }
        LlmProtocol::AnthropicMessages => {
            stream_anthropic(client, target, model, messages, tools, options, &events).await
        }
    }?;
    let _ = events
        .send(ConversationStreamEvent::Completed(result.clone()))
        .await;
    Ok(result)
}

async fn stream_chat_completions(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
    options: ConversationOptions,
    events: &mpsc::Sender<ConversationStreamEvent>,
) -> Result<ChatCompletionReply, LlmCallError> {
    let endpoint = chat_completions_url(&target.base_url);
    let mut body = json!({"model": model, "messages": messages});
    enable_chat_streaming(&mut body);
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = Value::String("auto".to_string());
    }
    if let Some(limit) = options.max_output_tokens {
        body["max_tokens"] = Value::from(limit);
    }
    let mut request = client
        .post(&endpoint)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "text/event-stream")
        .header(header::AUTHORIZATION, format!("Bearer {}", target.api_key))
        .json(&body);
    if let Some(preset_id) = target.upstream_preset_id.as_deref() {
        request = request.header(UPSTREAM_PRESET_ID_HEADER, preset_id);
    }
    stream_response(request, endpoint, LlmProtocol::ChatCompletions, events).await
}

async fn stream_responses(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
    options: ConversationOptions,
    events: &mpsc::Sender<ConversationStreamEvent>,
) -> Result<ChatCompletionReply, LlmCallError> {
    let endpoint = responses_url(&target.base_url);
    let body = responses_conversation_request_body(model, &messages, &tools, options);
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
    stream_response(request, endpoint, LlmProtocol::Responses, events).await
}

async fn stream_anthropic(
    client: &reqwest::Client,
    target: &ApiPresetLlmTarget,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Value>,
    options: ConversationOptions,
    events: &mpsc::Sender<ConversationStreamEvent>,
) -> Result<ChatCompletionReply, LlmCallError> {
    let endpoint = anthropic_messages_url(&target.base_url);
    let mut body = anthropic_conversation_request_body(model, &messages, &tools, options);
    body["stream"] = Value::Bool(true);
    let request = client
        .post(&endpoint)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "text/event-stream")
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", &target.api_key)
        .header(header::AUTHORIZATION, format!("Bearer {}", target.api_key))
        .json(&body);
    stream_response(request, endpoint, LlmProtocol::AnthropicMessages, events).await
}

async fn stream_response(
    request: reqwest::RequestBuilder,
    endpoint: String,
    protocol: LlmProtocol,
    events: &mpsc::Sender<ConversationStreamEvent>,
) -> Result<ChatCompletionReply, LlmCallError> {
    let started = Instant::now();
    let response = request.send().await.map_err(|error| LlmCallError {
        endpoint: endpoint.clone(),
        status: None,
        latency_ms: started.elapsed().as_millis(),
        message: format!("LLM 请求失败: {error}"),
        response_body: String::new(),
    })?;
    let status = response.status();
    if !status.is_success() {
        let response_body = response.text().await.unwrap_or_default();
        return Err(LlmCallError {
            endpoint,
            status: Some(status.as_u16()),
            latency_ms: started.elapsed().as_millis(),
            message: format!("LLM 返回错误 {status}: {}", truncate_body(&response_body)),
            response_body,
        });
    }

    let mut stream = response.bytes_stream();
    let mut pending = String::new();
    let mut response_body = String::new();
    let mut accumulator = StreamAccumulator::default();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| LlmCallError {
            endpoint: endpoint.clone(),
            status: Some(status.as_u16()),
            latency_ms: started.elapsed().as_millis(),
            message: format!("读取 LLM 流失败: {error}"),
            response_body: response_body.clone(),
        })?;
        let text = String::from_utf8_lossy(&chunk);
        response_body.push_str(&text);
        pending.push_str(&text);
        while let Some(newline) = pending.find('\n') {
            let line = pending[..newline].trim_end_matches('\r').to_string();
            pending.drain(..=newline);
            if let Some(data) = line.trim().strip_prefix("data:") {
                process_sse_data(data.trim(), protocol, &mut accumulator, events).await;
            }
        }
    }
    if !pending.trim().is_empty() {
        if let Some(data) = pending.trim().strip_prefix("data:") {
            process_sse_data(data.trim(), protocol, &mut accumulator, events).await;
        }
    }

    if accumulator.completed_value.is_none()
        && let Ok(value) = serde_json::from_str::<Value>(&response_body)
    {
        process_json_event(&value, protocol, &mut accumulator, events).await;
        accumulator.completed_value = Some(value);
    }
    finish_reply(
        endpoint,
        status.as_u16(),
        started.elapsed().as_millis(),
        response_body,
        protocol,
        accumulator,
    )
}

async fn process_sse_data(
    data: &str,
    protocol: LlmProtocol,
    accumulator: &mut StreamAccumulator,
    events: &mpsc::Sender<ConversationStreamEvent>,
) {
    if data.is_empty() || data == "[DONE]" {
        return;
    }
    if let Ok(value) = serde_json::from_str::<Value>(data) {
        process_json_event(&value, protocol, accumulator, events).await;
    }
}

async fn emit_delta(
    delta: &str,
    accumulator: &mut StreamAccumulator,
    events: &mpsc::Sender<ConversationStreamEvent>,
) {
    if delta.is_empty() {
        return;
    }
    accumulator.content.push_str(delta);
    let _ = events
        .send(ConversationStreamEvent::TextDelta(delta.to_string()))
        .await;
}

async fn emit_reasoning_delta(
    delta: &str,
    accumulator: &mut StreamAccumulator,
    events: &mpsc::Sender<ConversationStreamEvent>,
) {
    if delta.is_empty() {
        return;
    }
    accumulator.reasoning_content.push_str(delta);
    let _ = events
        .send(ConversationStreamEvent::ReasoningDelta(delta.to_string()))
        .await;
}

async fn process_json_event(
    value: &Value,
    protocol: LlmProtocol,
    accumulator: &mut StreamAccumulator,
    events: &mpsc::Sender<ConversationStreamEvent>,
) {
    match protocol {
        LlmProtocol::ChatCompletions => {
            if let Some(usage) = value.get("usage").and_then(openai_token_usage) {
                accumulator.usage = Some(usage);
            }
            let Some(choice) = value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|v| v.first())
            else {
                return;
            };
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                accumulator.finish_reason = reason.to_string();
            }
            let delta = choice.get("delta").or_else(|| choice.get("message"));
            if let Some(delta) = delta {
                if let Some(reasoning) = delta.get("reasoning_content").and_then(Value::as_str) {
                    emit_reasoning_delta(reasoning, accumulator, events).await;
                }
                if let Some(text) = delta.get("content").and_then(Value::as_str) {
                    emit_delta(text, accumulator, events).await;
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                    for tool in tool_calls {
                        let index = tool.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        let target = accumulator.chat_tools.entry(index).or_default();
                        if let Some(id) = tool.get("id").and_then(Value::as_str) {
                            target.id.push_str(id);
                        }
                        if let Some(function) = tool.get("function") {
                            if let Some(name) = function.get("name").and_then(Value::as_str) {
                                target.name.push_str(name);
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(Value::as_str)
                            {
                                target.arguments.push_str(arguments);
                            }
                        }
                    }
                }
            }
        }
        LlmProtocol::Responses => match value.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    emit_delta(delta, accumulator, events).await;
                }
            }
            Some("response.reasoning_summary_text.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    emit_reasoning_delta(delta, accumulator, events).await;
                }
            }
            Some("response.reasoning_text.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    emit_reasoning_delta(delta, accumulator, events).await;
                }
            }
            Some("response.completed") => {
                if let Some(response) = value.get("response") {
                    accumulator.usage = response.get("usage").and_then(responses_token_usage);
                    accumulator.completed_value = Some(response.clone());
                }
            }
            _ => {}
        },
        LlmProtocol::AnthropicMessages => match value.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                accumulator.usage = value
                    .get("message")
                    .and_then(|message| message.get("usage"))
                    .and_then(anthropic_token_usage);
            }
            Some("content_block_start") => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(block) = value.get("content_block") {
                    if block.get("type").and_then(Value::as_str) == Some("text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            emit_delta(text, accumulator, events).await;
                        }
                    } else if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                        accumulator.anthropic_tools.insert(index, block.clone());
                    }
                }
            }
            Some("content_block_delta") => {
                let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(delta) = value.get("delta") {
                    if let Some(text) = delta.get("text").and_then(Value::as_str) {
                        emit_delta(text, accumulator, events).await;
                    }
                    if let Some(thinking) = delta.get("thinking").and_then(Value::as_str) {
                        emit_reasoning_delta(thinking, accumulator, events).await;
                    }
                    if let Some(json_delta) = delta.get("partial_json").and_then(Value::as_str) {
                        let block = accumulator
                            .anthropic_tools
                            .entry(index)
                            .or_insert_with(|| json!({"type": "tool_use"}));
                        let existing = block
                            .get("input_json")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        block["input_json"] = Value::String(format!("{existing}{json_delta}"));
                    }
                }
            }
            Some("message_delta") => {
                if let Some(reason) = value
                    .get("delta")
                    .and_then(|delta| delta.get("stop_reason"))
                    .and_then(Value::as_str)
                {
                    accumulator.finish_reason = reason.to_string();
                }
                if let Some(output_tokens) = value
                    .get("usage")
                    .and_then(|usage| usage.get("output_tokens"))
                    .and_then(Value::as_u64)
                {
                    let usage = accumulator.usage.get_or_insert_with(LlmTokenUsage::default);
                    usage.output_tokens = output_tokens;
                    usage.total_tokens = usage.input_tokens.saturating_add(output_tokens);
                }
            }
            _ => {}
        },
    }
}

fn finish_reply(
    endpoint: String,
    status: u16,
    latency_ms: u128,
    response_body: String,
    protocol: LlmProtocol,
    mut accumulator: StreamAccumulator,
) -> Result<ChatCompletionReply, LlmCallError> {
    let mut tool_calls = None;
    match protocol {
        LlmProtocol::ChatCompletions => {
            let calls = accumulator
                .chat_tools
                .into_values()
                .map(|tool| {
                    json!({
                        "id": if tool.id.is_empty() { "call_unknown" } else { &tool.id },
                        "type": "function",
                        "function": {"name": tool.name, "arguments": tool.arguments}
                    })
                })
                .collect::<Vec<_>>();
            if !calls.is_empty() {
                tool_calls = Some(calls);
            }
        }
        LlmProtocol::Responses => {
            if let Some(value) = accumulator.completed_value.as_ref() {
                tool_calls = extract_responses_tool_calls(value);
                if accumulator.content.is_empty() {
                    accumulator.content =
                        extract_text_from_responses_value(value).unwrap_or_default();
                }
            } else if let Some(value) = completed_responses_value(&response_body) {
                tool_calls = extract_responses_tool_calls(&value);
                if accumulator.content.is_empty() {
                    accumulator.content =
                        extract_text_from_responses_value(&value).unwrap_or_default();
                }
                accumulator.usage = value.get("usage").and_then(responses_token_usage);
            }
        }
        LlmProtocol::AnthropicMessages => {
            let content = accumulator
                .anthropic_tools
                .into_values()
                .map(|mut block| {
                    if let Some(input_json) = block.get("input_json").and_then(Value::as_str) {
                        block["input"] =
                            serde_json::from_str(input_json).unwrap_or_else(|_| json!({}));
                    }
                    block
                })
                .collect::<Vec<_>>();
            if !content.is_empty() {
                tool_calls = extract_anthropic_tool_calls(&json!({"content": content}));
            }
        }
    }
    if accumulator.finish_reason.is_empty() {
        accumulator.finish_reason = if tool_calls.is_some() {
            "tool_calls"
        } else {
            "stop"
        }
        .to_string();
    }
    Ok(ChatCompletionReply {
        endpoint,
        status,
        latency_ms,
        content: (!accumulator.content.is_empty()).then_some(accumulator.content),
        reasoning_content: (!accumulator.reasoning_content.is_empty())
            .then_some(accumulator.reasoning_content),
        tool_calls,
        finish_reason: accumulator.finish_reason,
        response_body,
        usage: accumulator.usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chat_stream_accumulates_text_tools_and_usage() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut accumulator = StreamAccumulator::default();
        process_json_event(
            &json!({"choices": [{"delta": {"content": "hel", "reasoning_content": "think "}}]}),
            LlmProtocol::ChatCompletions,
            &mut accumulator,
            &tx,
        )
        .await;
        process_json_event(
            &json!({
                "choices": [{"delta": {"content": "lo", "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "function": {"name": "run_command", "arguments": "{\"command\":\"pwd\"}"}
                }]}, "finish_reason": "tool_calls"}],
                "usage": {"prompt_tokens": 12, "completion_tokens": 4, "total_tokens": 16}
            }),
            LlmProtocol::ChatCompletions,
            &mut accumulator,
            &tx,
        )
        .await;
        let reply = finish_reply(
            "http://test".to_string(),
            200,
            1,
            String::new(),
            LlmProtocol::ChatCompletions,
            accumulator,
        )
        .unwrap();

        assert_eq!(reply.content.as_deref(), Some("hello"));
        assert_eq!(reply.reasoning_content.as_deref(), Some("think "));
        assert_eq!(reply.tool_calls.as_ref().unwrap()[0]["function"]["name"], "run_command");
        assert_eq!(reply.usage.unwrap().input_tokens, 12);
        assert!(
            matches!(rx.recv().await, Some(ConversationStreamEvent::ReasoningDelta(delta)) if delta == "think ")
        );
        assert!(
            matches!(rx.recv().await, Some(ConversationStreamEvent::TextDelta(delta)) if delta == "hel")
        );
        assert!(
            matches!(rx.recv().await, Some(ConversationStreamEvent::TextDelta(delta)) if delta == "lo")
        );
    }

    #[tokio::test]
    async fn responses_and_anthropic_reasoning_deltas_are_streamed() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut accumulator = StreamAccumulator::default();
        process_json_event(
            &json!({"type": "response.reasoning_summary_text.delta", "delta": "resp "}),
            LlmProtocol::Responses,
            &mut accumulator,
            &tx,
        )
        .await;
        assert!(
            matches!(rx.recv().await, Some(ConversationStreamEvent::ReasoningDelta(delta)) if delta == "resp ")
        );

        let (tx, mut rx) = mpsc::channel(8);
        let mut accumulator = StreamAccumulator::default();
        process_json_event(
            &json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking"}}),
            LlmProtocol::AnthropicMessages,
            &mut accumulator,
            &tx,
        )
        .await;
        process_json_event(
            &json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "deep "}}),
            LlmProtocol::AnthropicMessages,
            &mut accumulator,
            &tx,
        )
        .await;
        assert!(
            matches!(rx.recv().await, Some(ConversationStreamEvent::ReasoningDelta(delta)) if delta == "deep ")
        );
        let reply = finish_reply(
            "http://test".to_string(),
            200,
            1,
            String::new(),
            LlmProtocol::AnthropicMessages,
            accumulator,
        )
        .unwrap();
        assert_eq!(reply.reasoning_content.as_deref(), Some("deep "));
    }

    #[tokio::test]
    async fn responses_stream_reads_completed_usage() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut accumulator = StreamAccumulator::default();
        process_json_event(
            &json!({"type": "response.output_text.delta", "delta": "done"}),
            LlmProtocol::Responses,
            &mut accumulator,
            &tx,
        )
        .await;
        process_json_event(
            &json!({"type": "response.completed", "response": {
                "status": "completed",
                "usage": {"input_tokens": 20, "output_tokens": 5, "total_tokens": 25},
                "output": []
            }}),
            LlmProtocol::Responses,
            &mut accumulator,
            &tx,
        )
        .await;
        let reply = finish_reply(
            "http://test".to_string(),
            200,
            1,
            String::new(),
            LlmProtocol::Responses,
            accumulator,
        )
        .unwrap();

        assert_eq!(reply.content.as_deref(), Some("done"));
        assert_eq!(reply.usage.unwrap().total_tokens, 25);
        assert!(
            matches!(rx.recv().await, Some(ConversationStreamEvent::TextDelta(delta)) if delta == "done")
        );
    }

    #[tokio::test]
    async fn anthropic_stream_accumulates_tool_json_and_usage() {
        let (tx, _rx) = mpsc::channel(8);
        let mut accumulator = StreamAccumulator::default();
        for event in [
            json!({"type": "message_start", "message": {"usage": {"input_tokens": 30, "output_tokens": 0}}}),
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "tool_1", "name": "read_file", "input": {}}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"path\":\"Cargo.toml\"}"}}),
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 7}}),
        ] {
            process_json_event(&event, LlmProtocol::AnthropicMessages, &mut accumulator, &tx).await;
        }
        let reply = finish_reply(
            "http://test".to_string(),
            200,
            1,
            String::new(),
            LlmProtocol::AnthropicMessages,
            accumulator,
        )
        .unwrap();

        assert_eq!(reply.tool_calls.as_ref().unwrap()[0]["function"]["name"], "read_file");
        assert_eq!(reply.usage.unwrap().total_tokens, 37);
    }
}
