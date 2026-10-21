use codex_proxy_core::gateway_message;
use serde_json::{Value, json};

use super::ANTHROPIC_CHAT_KIND;
use crate::{ApiResult, AppError};

pub(super) fn anthropic_messages_request_to_openai_responses(payload: &Value) -> ApiResult<Value> {
    let chat_request = anthropic_messages_request_to_openai_chat(payload)?;
    let model = chat_request
        .get("model")
        .cloned()
        .unwrap_or_else(|| json!("webclx-claude-proxy"));
    let mut input = Vec::new();
    if let Some(messages) = chat_request.get("messages").and_then(Value::as_array) {
        for message in messages {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let responses_role = if role == "system" { "developer" } else { role };
            let text = message
                .get("content")
                .map(anthropic_content_to_text)
                .unwrap_or_default();
            if text.is_empty() && role == "tool" {
                continue;
            }
            input.push(json!({
                "type": "message",
                "role": responses_role,
                "content": [{
                    "type": if responses_role == "assistant" { "output_text" } else { "input_text" },
                    "text": text,
                }],
            }));
        }
    }
    if input.is_empty() {
        input.push(json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": ""}],
        }));
    }

    let mut out = json!({
        "model": model,
        "input": input,
        "stream": false,
    });
    if let Some(max_tokens) = payload.get("max_tokens").and_then(Value::as_u64) {
        out["max_output_tokens"] = json!(max_tokens);
    }
    if let Some(temperature) = payload.get("temperature").filter(|value| value.is_number()) {
        out["temperature"] = temperature.clone();
    }
    if let Some(top_p) = payload.get("top_p").filter(|value| value.is_number()) {
        out["top_p"] = top_p.clone();
    }
    if let Some(tools) = chat_request.get("tools").cloned() {
        out["tools"] = tools;
    }
    Ok(out)
}

pub(super) fn anthropic_messages_request_to_openai_chat(payload: &Value) -> ApiResult<Value> {
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::bad_request(gateway_message(
                ANTHROPIC_CHAT_KIND,
                "Anthropic Messages 请求缺少 model。",
            ))
        })?;
    let mut messages = Vec::new();
    if let Some(system) = payload.get("system") {
        let system_text = anthropic_content_to_text(system);
        if !system_text.trim().is_empty() {
            messages.push(json!({
                "role": "system",
                "content": system_text,
            }));
        }
    }
    if let Some(items) = payload.get("messages").and_then(Value::as_array) {
        for message in items {
            append_anthropic_message_as_openai_chat(&mut messages, message);
        }
    }
    if messages.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": "",
        }));
    }

    let mut out = json!({
        "model": model,
        "messages": messages,
        "stream": false,
    });
    if let Some(max_tokens) = payload.get("max_tokens").and_then(Value::as_u64) {
        out["max_tokens"] = json!(max_tokens);
    }
    if let Some(temperature) = payload.get("temperature").filter(|value| value.is_number()) {
        out["temperature"] = temperature.clone();
    }
    if let Some(top_p) = payload.get("top_p").filter(|value| value.is_number()) {
        out["top_p"] = top_p.clone();
    }
    if let Some(tools) = payload.get("tools").and_then(Value::as_array) {
        let converted: Vec<Value> = tools
            .iter()
            .filter_map(anthropic_tool_to_openai_tool)
            .collect();
        if !converted.is_empty() {
            out["tools"] = Value::Array(converted);
        }
    }
    Ok(out)
}

fn append_anthropic_message_as_openai_chat(out: &mut Vec<Value>, message: &Value) {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    let content = message.get("content").unwrap_or(&Value::Null);
    match role {
        "assistant" => {
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();
            if let Some(blocks) = content.as_array() {
                for block in blocks {
                    match block.get("type").and_then(Value::as_str).unwrap_or("") {
                        "tool_use" => {
                            let id = block
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("toolu_unknown");
                            let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                            let input = block.get("input").cloned().unwrap_or(Value::Null);
                            let arguments =
                                serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": arguments,
                                }
                            }));
                        }
                        _ => {
                            let text = anthropic_content_to_text(block);
                            if !text.is_empty() {
                                text_parts.push(text);
                            }
                        }
                    }
                }
            } else {
                let text = anthropic_content_to_text(content);
                if !text.is_empty() {
                    text_parts.push(text);
                }
            }
            let mut chat_message = json!({
                "role": "assistant",
                "content": text_parts.join(""),
            });
            if !tool_calls.is_empty() {
                chat_message["tool_calls"] = Value::Array(tool_calls);
            }
            out.push(chat_message);
        }
        "user" => {
            let mut text_parts = Vec::new();
            let mut emitted_tool_result = false;
            if let Some(blocks) = content.as_array() {
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                        emitted_tool_result = true;
                        out.push(json!({
                            "role": "tool",
                            "tool_call_id": block
                                .get("tool_use_id")
                                .and_then(Value::as_str)
                                .unwrap_or("toolu_unknown"),
                            "content": anthropic_content_to_text(
                                block.get("content").unwrap_or(&Value::Null),
                            ),
                        }));
                    } else {
                        let text = anthropic_content_to_text(block);
                        if !text.is_empty() {
                            text_parts.push(text);
                        }
                    }
                }
            } else {
                let text = anthropic_content_to_text(content);
                if !text.is_empty() {
                    text_parts.push(text);
                }
            }
            if !text_parts.is_empty() || !emitted_tool_result {
                out.push(json!({
                    "role": "user",
                    "content": text_parts.join(""),
                }));
            }
        }
        _ => {
            out.push(json!({
                "role": "user",
                "content": anthropic_content_to_text(content),
            }));
        }
    }
}

fn anthropic_content_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(anthropic_content_to_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return text.to_string();
            }
            if let Some(content) = map.get("content") {
                return anthropic_content_to_text(content);
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn anthropic_tool_to_openai_tool(tool: &Value) -> Option<Value> {
    let name = tool.get("name").and_then(Value::as_str)?;
    Some(json!({
        "type": "function",
        "function": {
            "name": name,
            "description": tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(""),
            "parameters": tool
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false,
                })),
        }
    }))
}

pub(super) fn openai_chat_response_to_anthropic_messages_response(
    chat_response: &Value,
    fallback_model: &str,
) -> Value {
    let choice = chat_response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .unwrap_or(&Value::Null);
    let message = choice.get("message").unwrap_or(&Value::Null);
    let mut content = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str)
        && !text.is_empty()
    {
        content.push(json!({
            "type": "text",
            "text": text,
        }));
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            let function = tool_call.get("function").unwrap_or(&Value::Null);
            let arguments = function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
            content.push(json!({
                "type": "tool_use",
                "id": tool_call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("toolu_unknown"),
                "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
                "input": input,
            }));
        }
    }
    if content.is_empty() {
        content.push(json!({
            "type": "text",
            "text": "",
        }));
    }
    let usage = chat_response
        .get("usage")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let input_tokens = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop");
    let stop_reason = match finish_reason {
        "tool_calls" => "tool_use",
        "length" => "max_tokens",
        _ => "end_turn",
    };
    json!({
        "id": chat_response
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("msg_webclx_openai_chat"),
        "type": "message",
        "role": "assistant",
        "model": chat_response
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(fallback_model),
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
        }
    })
}

pub(super) fn openai_responses_payload_to_anthropic_messages_response(
    response: &Value,
    fallback_model: &str,
) -> Value {
    let mut content = Vec::new();
    if let Some(items) = response.get("output").and_then(Value::as_array) {
        for item in items {
            match item.get("type").and_then(Value::as_str).unwrap_or("") {
                "message" => {
                    if let Some(parts) = item.get("content").and_then(Value::as_array) {
                        for part in parts {
                            if let Some(text) = part
                                .get("text")
                                .or_else(|| part.get("output_text"))
                                .and_then(Value::as_str)
                                && !text.is_empty()
                            {
                                content.push(json!({
                                    "type": "text",
                                    "text": text,
                                }));
                            }
                        }
                    }
                }
                "function_call" => {
                    let arguments = item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("{}");
                    let input: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
                    content.push(json!({
                        "type": "tool_use",
                        "id": item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(Value::as_str)
                            .unwrap_or("toolu_unknown"),
                        "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                        "input": input,
                    }));
                }
                _ => {}
            }
        }
    }
    if content.is_empty() {
        content.push(json!({
            "type": "text",
            "text": response
                .get("output_text")
                .and_then(Value::as_str)
                .unwrap_or(""),
        }));
    }

    let usage = response.get("usage").cloned().unwrap_or_else(|| json!({}));
    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "id": response
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("msg_webclx_openai_responses"),
        "type": "message",
        "role": "assistant",
        "model": response
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(fallback_model),
        "content": content,
        "stop_reason": "end_turn",
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
        }
    })
}
