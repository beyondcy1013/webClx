use std::{
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    fmt,
    sync::{Arc, RwLock},
    time::SystemTime,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde_json::{Map, Value, json};
use tracing::warn;

const MAX_HISTORY_ENTRIES: usize = 256;
const GATEWAY_PREFIX: &str = "【webClx大模型网关】";
const OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND: &str = "OpenAI Responses→Chat Completions";
const DEEPSEEK_REASONING_CARRIER_PREFIX: &str = "webclx-deepseek-reasoning:v1:";
const DEEPSEEK_REASONING_PLACEHOLDER: &str = "Continuing prior assistant turn.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexProxyError {
    message: String,
}

impl CodexProxyError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CodexProxyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for CodexProxyError {}

pub fn gateway_message(kind: &str, detail: impl Into<String>) -> String {
    format!("{GATEWAY_PREFIX}【{kind}】{}", detail.into())
}

#[derive(Clone, Default)]
pub struct CodexProxyHistory {
    inner: Arc<RwLock<CodexProxyHistoryInner>>,
}

#[derive(Default)]
struct CodexProxyHistoryInner {
    entries: HashMap<String, Vec<Value>>,
    order: VecDeque<String>,
}

impl CodexProxyHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chat_request_with_previous_response(
        &self,
        payload: &Value,
        mut chat_request: Value,
    ) -> Value {
        let Some(previous_response_id) =
            payload.get("previous_response_id").and_then(Value::as_str)
        else {
            return chat_request;
        };

        let Some(previous_messages) = self.get(previous_response_id) else {
            warn!(
                "{}",
                gateway_message(
                    OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
                    format!("previous_response_id 未找到: {previous_response_id}")
                )
            );
            return chat_request;
        };

        let current_messages = chat_request
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(root) = chat_request.as_object_mut() {
            root.insert(
                "messages".to_string(),
                Value::Array(merge_previous_and_current_messages(
                    previous_messages,
                    current_messages,
                )),
            );
        }
        chat_request
    }

    pub fn record_response(
        &self,
        response_payload: &Value,
        chat_request: &Value,
        chat_response: &Value,
    ) {
        let Some(response_id) = response_payload.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some(mut messages) = chat_request
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
        else {
            return;
        };
        if let Some(assistant_message) = chat_response_to_chat_assistant_message(chat_response) {
            messages.push(assistant_message);
        }
        self.insert(response_id.to_string(), messages);
    }

    fn get(&self, response_id: &str) -> Option<Vec<Value>> {
        let guard = self.inner.read().ok()?;
        guard.entries.get(response_id).cloned()
    }

    pub fn contains(&self, response_id: &str) -> bool {
        self.inner
            .read()
            .is_ok_and(|guard| guard.entries.contains_key(response_id))
    }

    fn insert(&self, response_id: String, messages: Vec<Value>) {
        let Ok(mut guard) = self.inner.write() else {
            return;
        };

        if !guard.entries.contains_key(&response_id) {
            guard.order.push_back(response_id.clone());
        }
        guard.entries.insert(response_id, messages);

        while guard.order.len() > MAX_HISTORY_ENTRIES {
            if let Some(oldest) = guard.order.pop_front() {
                guard.entries.remove(&oldest);
            }
        }
    }
}

pub fn responses_request_to_chat_request(payload: &Value) -> Result<Value, CodexProxyError> {
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| CodexProxyError::bad_request("Responses 请求缺少 model。"))?;

    let mut root = Map::new();
    root.insert("model".to_string(), Value::String(model.to_string()));
    root.insert("stream".to_string(), Value::Bool(false));
    root.insert("messages".to_string(), Value::Array(responses_input_to_chat_messages(payload)));

    copy_optional_number(payload, &mut root, "temperature", "temperature");
    copy_optional_number(payload, &mut root, "top_p", "top_p");
    copy_optional_number(payload, &mut root, "max_output_tokens", "max_completion_tokens");

    let tools = responses_tools_to_chat_tools(payload.get("tools"));
    if !tools.is_empty() {
        root.insert("tools".to_string(), Value::Array(tools));
        if let Some(tool_choice) = payload.get("tool_choice") {
            root.insert("tool_choice".to_string(), tool_choice.clone());
        }
        if let Some(parallel) = payload.get("parallel_tool_calls").and_then(Value::as_bool) {
            root.insert("parallel_tool_calls".to_string(), Value::Bool(parallel));
        }
    }

    Ok(Value::Object(root))
}

fn copy_optional_number(source: &Value, target: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = source.get(from).filter(|value| value.is_number()) {
        target.insert(to.to_string(), value.clone());
    }
}

fn responses_input_to_chat_messages(payload: &Value) -> Vec<Value> {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();
    let mut pending_reasoning_content = None;
    if let Some(instructions) = payload.get("instructions").and_then(Value::as_str)
        && !instructions.trim().is_empty()
    {
        system_parts.push(instructions.to_string());
    }

    match payload.get("input") {
        Some(Value::String(text)) => {
            messages.push(json!({ "role": "user", "content": text }));
        }
        Some(Value::Array(items)) => {
            for item in items {
                append_response_input_item(&mut messages, item, &mut pending_reasoning_content);
            }
        }
        Some(Value::Object(_)) => append_response_input_item(
            &mut messages,
            &payload["input"],
            &mut pending_reasoning_content,
        ),
        _ => {}
    }

    if !system_parts.is_empty() {
        let instructions = system_parts.join("\n\n");
        if let Some(existing) = messages
            .iter_mut()
            .find(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        {
            let previous = existing
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            existing["content"] = if previous.is_empty() {
                Value::String(instructions)
            } else {
                Value::String(format!("{instructions}\n\n{previous}"))
            };
        } else {
            messages.insert(
                0,
                json!({
                    "role": "system",
                    "content": instructions,
                }),
            );
        }
    }

    if messages.is_empty() {
        messages.push(json!({ "role": "user", "content": "" }));
    }
    messages
}

fn append_response_input_item(
    messages: &mut Vec<Value>,
    item: &Value,
    pending_reasoning_content: &mut Option<String>,
) {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    match item_type {
        "reasoning" => {
            if let Some(reasoning_content) = response_reasoning_item_to_text(item) {
                if let Some(last_message) = messages
                    .last_mut()
                    .filter(|message| message_role(message) == Some("assistant"))
                    .filter(|message| !message_has_reasoning_content(message))
                {
                    attach_reasoning_content(last_message, reasoning_content);
                } else {
                    *pending_reasoning_content = Some(reasoning_content);
                }
            }
        }
        "function_call_output" => {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .or_else(|| item.get("id").and_then(Value::as_str))
                .unwrap_or("call_unknown");
            let output = item.get("output").map(content_to_text).unwrap_or_default();
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output,
            }));
        }
        "function_call" => {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .or_else(|| item.get("id").and_then(Value::as_str))
                .unwrap_or("call_unknown");
            let name = item.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = item
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let mut message = json!({
                "role": "assistant",
                "content": Value::Null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments,
                    }
                }]
            });
            attach_pending_reasoning_content(&mut message, pending_reasoning_content);
            messages.push(message);
        }
        _ => {
            let raw_role = item.get("role").and_then(Value::as_str).unwrap_or("user");
            let role = match raw_role {
                "assistant" => "assistant",
                "tool" => "tool",
                _ => "user",
            };
            let content = item.get("content").map(content_to_text).unwrap_or_default();
            if matches!(raw_role, "developer" | "system") {
                if let Some(existing) = messages
                    .iter_mut()
                    .find(|message| message.get("role").and_then(Value::as_str) == Some("system"))
                {
                    let previous = existing
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    existing["content"] = if previous.is_empty() {
                        Value::String(content)
                    } else {
                        Value::String(format!("{previous}\n\n{content}"))
                    };
                } else {
                    messages.push(json!({ "role": "system", "content": content }));
                }
            } else {
                let mut message = json!({ "role": role, "content": content });
                if role == "assistant" {
                    attach_pending_reasoning_content(&mut message, pending_reasoning_content);
                }
                messages.push(message);
            }
        }
    }
}

fn attach_pending_reasoning_content(
    message: &mut Value,
    pending_reasoning_content: &mut Option<String>,
) {
    let Some(reasoning_content) = pending_reasoning_content.take() else {
        return;
    };
    if reasoning_content.trim().is_empty() {
        return;
    }
    attach_reasoning_content(message, reasoning_content);
}

fn attach_reasoning_content(message: &mut Value, reasoning_content: String) {
    if let Some(root) = message.as_object_mut() {
        root.insert("reasoning_content".to_string(), Value::String(reasoning_content));
    }
}

fn response_reasoning_item_to_text(item: &Value) -> Option<String> {
    if let Some(reasoning_content) = item.get("reasoning_content").and_then(Value::as_str)
        && !reasoning_content.trim().is_empty()
    {
        return Some(reasoning_content.to_string());
    }

    if let Some(encrypted_content) = item.get("encrypted_content").and_then(Value::as_str)
        && let Some(reasoning_content) = decode_reasoning_carrier(encrypted_content)
    {
        return Some(reasoning_content);
    }

    let content = item.get("content").map(content_to_text).unwrap_or_default();
    if content.trim().is_empty() {
        None
    } else {
        Some(content)
    }
}

fn merge_previous_and_current_messages(
    mut previous_messages: Vec<Value>,
    current_messages: Vec<Value>,
) -> Vec<Value> {
    let mut has_system = previous_messages
        .iter()
        .any(|message| message_role(message) == Some("system"));

    for message in current_messages {
        if message_role(&message) == Some("system") {
            if !has_system {
                previous_messages.insert(0, message);
                has_system = true;
            }
            continue;
        }
        previous_messages.push(message);
    }

    previous_messages
}

pub fn sanitize_chat_request_for_minimax(mut chat_request: Value) -> Value {
    let messages = chat_request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(root) = chat_request.as_object_mut() {
        root.insert(
            "messages".to_string(),
            Value::Array(sanitize_chat_messages_for_minimax(messages)),
        );
    }
    chat_request
}

pub fn sanitize_chat_request_for_deepseek(mut chat_request: Value) -> Value {
    let messages = chat_request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(root) = chat_request.as_object_mut() {
        let messages = sanitize_chat_messages_for_chat_completions(messages);
        root.insert(
            "messages".to_string(),
            Value::Array(backfill_deepseek_reasoning_content(messages)),
        );
    }
    chat_request
}

pub fn strip_chat_request_reasoning_content(mut chat_request: Value) -> Value {
    if let Some(messages) = chat_request
        .get_mut("messages")
        .and_then(Value::as_array_mut)
    {
        for message in messages {
            if let Some(root) = message.as_object_mut() {
                root.remove("reasoning_content");
            }
        }
    }
    chat_request
}

fn backfill_deepseek_reasoning_content(messages: Vec<Value>) -> Vec<Value> {
    messages
        .into_iter()
        .map(|mut message| {
            if message_role(&message) != Some("assistant") {
                return message;
            }
            if message_has_reasoning_content(&message) || !assistant_message_has_payload(&message) {
                return message;
            }
            if let Some(root) = message.as_object_mut() {
                root.insert(
                    "reasoning_content".to_string(),
                    Value::String(DEEPSEEK_REASONING_PLACEHOLDER.to_string()),
                );
            }
            message
        })
        .collect()
}

fn message_has_reasoning_content(message: &Value) -> bool {
    message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

fn assistant_message_has_payload(message: &Value) -> bool {
    let has_content = message
        .get("content")
        .map(content_to_text)
        .is_some_and(|text| !text.trim().is_empty());
    let has_tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|tool_calls| !tool_calls.is_empty());
    has_content || has_tool_calls
}

pub fn degrade_resume_chat_request_for_minimax(mut chat_request: Value) -> Value {
    let messages = chat_request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(root) = chat_request.as_object_mut() {
        root.insert(
            "messages".to_string(),
            Value::Array(degrade_resume_chat_messages_for_minimax(messages)),
        );
    }
    chat_request
}

fn degrade_resume_chat_messages_for_minimax(messages: Vec<Value>) -> Vec<Value> {
    messages
        .into_iter()
        .map(|message| match message_role(&message) {
            Some("assistant") if message.get("tool_calls").is_some() => {
                assistant_tool_calls_to_text_message(&message)
            }
            Some("tool") => {
                let call_id = message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_unknown")
                    .to_string();
                warn!(
                    "{}",
                    gateway_message(
                        OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
                        format!("已将恢复的 tool 结果转换为 user 消息: {call_id}")
                    )
                );
                orphan_tool_message_to_user_message(&message, &call_id)
            }
            _ => message,
        })
        .collect()
}

fn sanitize_chat_messages_for_minimax(messages: Vec<Value>) -> Vec<Value> {
    sanitize_chat_messages_for_chat_completions(messages)
}

fn sanitize_chat_messages_for_chat_completions(messages: Vec<Value>) -> Vec<Value> {
    let mut sanitized = Vec::with_capacity(messages.len());
    let mut consumed_tool_indices = HashSet::new();

    for index in 0..messages.len() {
        if consumed_tool_indices.contains(&index) {
            continue;
        }
        let message = &messages[index];
        match message_role(message) {
            Some("assistant") => {
                let expected_tool_call_ids = assistant_tool_call_ids_in_order(message);
                if expected_tool_call_ids.is_empty() {
                    sanitized.push(message.clone());
                    continue;
                }

                if let Some(tool_indices) = find_matching_tool_message_indices(
                    &messages,
                    index + 1,
                    &expected_tool_call_ids,
                    &consumed_tool_indices,
                ) {
                    sanitized.push(message.clone());
                    for tool_index in tool_indices {
                        sanitized.push(messages[tool_index].clone());
                        consumed_tool_indices.insert(tool_index);
                    }
                } else {
                    warn!(
                        "{}",
                        gateway_message(
                            OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
                            format!(
                                "已将未匹配的 assistant tool_calls 转为文本: {:?}",
                                expected_tool_call_ids
                            )
                        )
                    );
                    sanitized.push(assistant_tool_calls_to_text_message(message));
                }
            }
            Some("tool") => {
                let call_id = message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call_unknown")
                    .to_string();
                warn!(
                    "{}",
                    gateway_message(
                        OPENAI_RESPONSES_TO_CHAT_COMPLETIONS_KIND,
                        format!("已将孤立 tool 结果转换为 user 消息: {call_id}")
                    )
                );
                sanitized.push(orphan_tool_message_to_user_message(message, &call_id));
            }
            _ => {
                sanitized.push(message.clone());
            }
        }
    }

    sanitized
}

fn find_matching_tool_message_indices(
    messages: &[Value],
    start: usize,
    expected_tool_call_ids: &[String],
    consumed_tool_indices: &HashSet<usize>,
) -> Option<Vec<usize>> {
    let expected: HashSet<&str> = expected_tool_call_ids.iter().map(String::as_str).collect();
    let mut found_by_call_id: HashMap<String, usize> = HashMap::new();

    for (index, message) in messages.iter().enumerate().skip(start) {
        if consumed_tool_indices.contains(&index) {
            continue;
        }

        if message_role(message) != Some("tool") {
            continue;
        }

        let Some(call_id) = message.get("tool_call_id").and_then(Value::as_str) else {
            continue;
        };
        if expected.contains(call_id) && !found_by_call_id.contains_key(call_id) {
            found_by_call_id.insert(call_id.to_string(), index);
            if found_by_call_id.len() == expected_tool_call_ids.len() {
                return expected_tool_call_ids
                    .iter()
                    .map(|call_id| found_by_call_id.get(call_id).copied())
                    .collect();
            }
        }
    }

    None
}

fn assistant_tool_calls_to_text_message(message: &Value) -> Value {
    let mut parts = Vec::new();
    let content = message
        .get("content")
        .map(content_to_text)
        .unwrap_or_default();
    if !content.trim().is_empty() {
        parts.push(content);
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            let call_id = tool_call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("call_unknown");
            let function = tool_call.get("function").unwrap_or(&Value::Null);
            let name = function.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            parts.push(format!("Tool call requested for {call_id}: {name}({arguments})"));
        }
    }

    json!({
        "role": "assistant",
        "content": parts.join("\n"),
    })
}

fn assistant_tool_call_ids_in_order(message: &Value) -> Vec<String> {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|tool_calls| {
            tool_calls
                .iter()
                .filter_map(|tool_call| tool_call.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn orphan_tool_message_to_user_message(message: &Value, call_id: &str) -> Value {
    let output = message
        .get("content")
        .map(content_to_text)
        .unwrap_or_default();
    json!({
        "role": "user",
        "content": format!("Tool result for {call_id}:\n{output}"),
    })
}

fn message_role(message: &Value) -> Option<&str> {
    message.get("role").and_then(Value::as_str)
}

fn chat_response_to_chat_assistant_message(chat_response: &Value) -> Option<Value> {
    let mut message = chat_response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(Value::as_object)
        .cloned()?;

    message.insert("role".to_string(), Value::String("assistant".to_string()));
    if let Some(Value::String(text)) = message.get("content") {
        message.insert("content".to_string(), Value::String(strip_think_blocks(text)));
    }

    Some(Value::Object(message))
}

fn content_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("content").and_then(Value::as_str))
                    .map(str::to_string)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .or_else(|| map.get("content").and_then(Value::as_str))
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn responses_tools_to_chat_tools(tools: Option<&Value>) -> Vec<Value> {
    let Some(Value::Array(items)) = tools else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|tool| {
            if tool.get("type").and_then(Value::as_str) != Some("function") {
                return None;
            }
            let name = tool.get("name").and_then(Value::as_str)?;
            let parameters = tool.get("parameters").cloned().unwrap_or_else(|| {
                json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false,
                })
            });
            Some(json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": tool
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    "parameters": parameters,
                }
            }))
        })
        .collect()
}

pub fn chat_response_to_responses_payload(chat_response: &Value, fallback_model: &str) -> Value {
    let response_id = chat_response
        .get("id")
        .and_then(Value::as_str)
        .map(|id| format!("resp_{id}"))
        .unwrap_or_else(|| "resp_minimax_proxy".to_string());
    let model = chat_response
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(fallback_model);
    let created_at = chat_response
        .get("created")
        .and_then(Value::as_i64)
        .unwrap_or_else(current_unix_timestamp);
    let output = chat_response_output_items(chat_response);

    json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": "completed",
        "model": model,
        "output": output,
        "usage": responses_usage(chat_response.get("usage")),
    })
}

fn chat_response_output_items(chat_response: &Value) -> Vec<Value> {
    let message = chat_response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .unwrap_or(&Value::Null);

    let mut output = Vec::new();
    if let Some(reasoning_content) = message.get("reasoning_content").and_then(Value::as_str)
        && !reasoning_content.trim().is_empty()
    {
        output.push(json!({
            "id": "rs_minimax_proxy",
            "type": "reasoning",
            "summary": [],
            "content": Value::Null,
            "encrypted_content": encode_reasoning_carrier(reasoning_content),
        }));
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, tool_call) in tool_calls.iter().enumerate() {
            let call_id = tool_call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("call_minimax_{index}"));
            let function = tool_call.get("function").unwrap_or(&Value::Null);
            output.push(json!({
                "id": format!("fc_{call_id}"),
                "type": "function_call",
                "status": "completed",
                "call_id": call_id,
                "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
                "arguments": function.get("arguments").and_then(Value::as_str).unwrap_or("{}"),
            }));
        }
    }

    let text = message
        .get("content")
        .map(content_to_text)
        .unwrap_or_default();
    let text = strip_think_blocks(&text);
    if !text.trim().is_empty() || output.is_empty() {
        let message = json!({
            "id": "msg_minimax_proxy",
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": text,
                "annotations": [],
            }],
        });
        let insert_index = output
            .iter()
            .position(|item| item.get("type").and_then(Value::as_str) != Some("reasoning"))
            .unwrap_or(output.len());
        output.insert(insert_index, message);
    }
    output
}

fn encode_reasoning_carrier(reasoning_content: &str) -> String {
    format!(
        "{}{}",
        DEEPSEEK_REASONING_CARRIER_PREFIX,
        BASE64_STANDARD.encode(reasoning_content)
    )
}

fn decode_reasoning_carrier(value: &str) -> Option<String> {
    let encoded = value.strip_prefix(DEEPSEEK_REASONING_CARRIER_PREFIX)?;
    let bytes = BASE64_STANDARD.decode(encoded).ok()?;
    String::from_utf8(bytes).ok()
}

fn strip_think_blocks(text: &str) -> String {
    const OPEN_TAG: &str = "<think>";
    const CLOSE_TAG: &str = "</think>";

    let lower = text.to_ascii_lowercase();
    let mut cursor = 0;
    let mut stripped = String::new();
    let mut changed = false;

    while let Some(relative_start) = lower[cursor..].find(OPEN_TAG) {
        let start = cursor + relative_start;
        stripped.push_str(&text[cursor..start]);

        let content_start = start + OPEN_TAG.len();
        if let Some(relative_end) = lower[content_start..].find(CLOSE_TAG) {
            cursor = content_start + relative_end + CLOSE_TAG.len();
        } else {
            cursor = text.len();
        }
        changed = true;
    }

    if !changed {
        return text.to_string();
    }

    stripped.push_str(&text[cursor..]);
    stripped.trim_start().to_string()
}

fn responses_usage(usage: Option<&Value>) -> Value {
    let input_tokens = usage
        .and_then(|usage| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|usage| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = usage
        .and_then(|usage| usage.get("total_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(input_tokens + output_tokens);
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": total_tokens,
    })
}

pub fn response_payload_to_sse_chunks(response: &Value) -> Vec<String> {
    let response_id = response
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_minimax_proxy");
    let created_at = response
        .get("created_at")
        .cloned()
        .unwrap_or_else(|| json!(0));
    let model = response
        .get("model")
        .cloned()
        .unwrap_or_else(|| json!("MiniMax-M2.7"));

    let mut chunks = Vec::new();
    chunks.push(sse_event(
        "response.created",
        json!({
            "type": "response.created",
            "response": {
                "id": response_id,
                "object": "response",
                "created_at": created_at,
                "status": "in_progress",
                "model": model,
                "output": [],
            }
        }),
    ));

    if let Some(items) = response.get("output").and_then(Value::as_array) {
        for (index, item) in items.iter().enumerate() {
            append_output_item_sse(&mut chunks, index, item);
        }
    }

    chunks.push(sse_event(
        "response.completed",
        json!({
            "type": "response.completed",
            "response": response,
        }),
    ));
    chunks.push("data: [DONE]\n\n".to_string());
    chunks
}

fn append_output_item_sse(chunks: &mut Vec<String>, output_index: usize, item: &Value) {
    let item_id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("item_minimax_proxy");
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");

    let mut in_progress_item = item.clone();
    if let Some(map) = in_progress_item.as_object_mut() {
        map.insert("status".to_string(), json!("in_progress"));
        if item_type == "message" {
            map.insert("content".to_string(), json!([]));
        } else if item_type == "function_call" {
            map.insert("arguments".to_string(), json!(""));
        }
    }

    chunks.push(sse_event(
        "response.output_item.added",
        json!({
            "type": "response.output_item.added",
            "output_index": output_index,
            "item": in_progress_item,
        }),
    ));

    match item_type {
        "message" => append_message_sse(chunks, output_index, item_id, item),
        "function_call" => append_function_call_sse(chunks, output_index, item_id, item),
        _ => {}
    }

    chunks.push(sse_event(
        "response.output_item.done",
        json!({
            "type": "response.output_item.done",
            "output_index": output_index,
            "item": item,
        }),
    ));
}

fn append_message_sse(chunks: &mut Vec<String>, output_index: usize, item_id: &str, item: &Value) {
    let text = item
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|part| part.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let part = json!({
        "type": "output_text",
        "text": "",
        "annotations": [],
    });

    chunks.push(sse_event(
        "response.content_part.added",
        json!({
            "type": "response.content_part.added",
            "item_id": item_id,
            "output_index": output_index,
            "content_index": 0,
            "part": part,
        }),
    ));
    if !text.is_empty() {
        chunks.push(sse_event(
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "item_id": item_id,
                "output_index": output_index,
                "content_index": 0,
                "delta": text,
            }),
        ));
    }
    chunks.push(sse_event(
        "response.output_text.done",
        json!({
            "type": "response.output_text.done",
            "item_id": item_id,
            "output_index": output_index,
            "content_index": 0,
            "text": text,
        }),
    ));
    chunks.push(sse_event(
        "response.content_part.done",
        json!({
            "type": "response.content_part.done",
            "item_id": item_id,
            "output_index": output_index,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": text,
                "annotations": [],
            },
        }),
    ));
}

fn append_function_call_sse(
    chunks: &mut Vec<String>,
    output_index: usize,
    item_id: &str,
    item: &Value,
) {
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    if !arguments.is_empty() {
        chunks.push(sse_event(
            "response.function_call_arguments.delta",
            json!({
                "type": "response.function_call_arguments.delta",
                "item_id": item_id,
                "output_index": output_index,
                "delta": arguments,
            }),
        ));
    }
    chunks.push(sse_event(
        "response.function_call_arguments.done",
        json!({
            "type": "response.function_call_arguments.done",
            "item_id": item_id,
            "output_index": output_index,
            "arguments": arguments,
        }),
    ));
}

fn sse_event(event_type: &str, payload: Value) -> String {
    format!("event: {event_type}\ndata: {payload}\n\n")
}

const ANTHROPIC_DEFAULT_MAX_TOKENS: u64 = 8192;
const ANTHROPIC_PROVIDER_LABEL: &str = "Anthropic-compatible relay";

pub fn chat_request_to_anthropic_messages(chat: &Value) -> Result<Value, CodexProxyError> {
    let root = chat
        .as_object()
        .ok_or_else(|| CodexProxyError::bad_request("Chat 请求不是 JSON 对象。"))?;

    let model = root
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| CodexProxyError::bad_request("Chat 请求缺少 model。"))?;

    let max_tokens = root
        .get("max_completion_tokens")
        .and_then(Value::as_u64)
        .or_else(|| root.get("max_tokens").and_then(Value::as_u64))
        .unwrap_or(ANTHROPIC_DEFAULT_MAX_TOKENS);

    let mut out = Map::new();
    out.insert("model".to_string(), Value::String(model.to_string()));
    out.insert("max_tokens".to_string(), json!(max_tokens));
    out.insert("stream".to_string(), Value::Bool(false));
    if let Some(temp) = root.get("temperature").filter(|value| value.is_number()) {
        out.insert("temperature".to_string(), temp.clone());
    }
    if let Some(top_p) = root.get("top_p").filter(|value| value.is_number()) {
        out.insert("top_p".to_string(), top_p.clone());
    }

    let messages = chat_messages_to_anthropic_messages(root.get("messages"))?;
    out.insert("messages".to_string(), Value::Array(messages));

    if let Some(system) = collect_anthropic_system(root.get("messages"))
        && !system.is_empty()
    {
        out.insert("system".to_string(), Value::String(system));
    }

    if let Some(tools_value) = root.get("tools") {
        let tools = anthropic_tools_from_chat_tools(tools_value);
        if !tools.is_empty() {
            out.insert("tools".to_string(), Value::Array(tools));
            // Anthropic only accepts "auto" / "any" / "tool" / specific tool name.
            // Map OpenAI's "auto"/"none"/null-ish to "auto" so the relay
            // still exercises the tool-calling path; this matches the existing
            // passthrough behavior on the Chat side.
            if let Some(choice) = root.get("tool_choice") {
                out.insert("tool_choice".to_string(), anthropic_tool_choice(choice));
            }
        }
    }

    Ok(Value::Object(out))
}

fn collect_anthropic_system(messages: Option<&Value>) -> Option<String> {
    let items = messages.and_then(Value::as_array)?;
    let mut collected: Vec<String> = Vec::new();
    for message in items {
        if message_role(message) != Some("system") {
            continue;
        }
        let text = message
            .get("content")
            .map(content_to_text)
            .unwrap_or_default();
        if !text.trim().is_empty() {
            collected.push(text);
        }
    }
    if collected.is_empty() {
        None
    } else {
        Some(collected.join("\n\n"))
    }
}

fn chat_messages_to_anthropic_messages(
    messages: Option<&Value>,
) -> Result<Vec<Value>, CodexProxyError> {
    let Some(items) = messages.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(items.len());
    let mut pending_tool_results: Vec<Value> = Vec::new();
    for message in items {
        match message_role(message) {
            Some("system") => {
                // system is hoisted to the top-level `system` field; skip.
            }
            Some("user") => {
                flush_pending_tool_results(&mut out, &mut pending_tool_results);
                let text = message
                    .get("content")
                    .map(content_to_text)
                    .unwrap_or_default();
                out.push(json!({
                    "role": "user",
                    "content": anthropic_text_blocks(&text),
                }));
            }
            Some("assistant") => {
                flush_pending_tool_results(&mut out, &mut pending_tool_results);
                let mut content: Vec<Value> = Vec::new();
                let reasoning = message
                    .get("reasoning_content")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .filter(|value| !value.trim().is_empty());
                if let Some(thinking) = reasoning.as_deref() {
                    content.push(json!({
                        "type": "thinking",
                        "thinking": thinking,
                    }));
                }
                let text = message
                    .get("content")
                    .map(content_to_text)
                    .unwrap_or_default();
                if !text.trim().is_empty() {
                    content.push(json!({
                        "type": "text",
                        "text": text,
                    }));
                }
                if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                    for tool_call in tool_calls {
                        let id = tool_call
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("toolu_unknown")
                            .to_string();
                        let function = tool_call.get("function").unwrap_or(&Value::Null);
                        let name = function
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let arguments = function
                            .get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}");
                        let input: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
                        content.push(json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input,
                        }));
                    }
                }
                if content.is_empty() {
                    // Anthropic requires non-empty content blocks for assistant
                    // turns that follow a tool result, so emit a placeholder
                    // text block when we have nothing else to send.
                    content.push(json!({
                        "type": "text",
                        "text": "",
                    }));
                }
                out.push(json!({
                    "role": "assistant",
                    "content": Value::Array(content),
                }));
            }
            Some("tool") => {
                let call_id = message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("toolu_unknown")
                    .to_string();
                let output = message
                    .get("content")
                    .map(content_to_text)
                    .unwrap_or_default();
                pending_tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": anthropic_tool_result_content(&output),
                }));
            }
            _ => {
                // Unknown roles are coerced to user messages so we never drop
                // history silently. This mirrors how the existing Chat
                // conversion treats unrecognised entries.
                flush_pending_tool_results(&mut out, &mut pending_tool_results);
                let text = message
                    .get("content")
                    .map(content_to_text)
                    .unwrap_or_default();
                out.push(json!({
                    "role": "user",
                    "content": anthropic_text_blocks(&text),
                }));
            }
        }
    }
    flush_pending_tool_results(&mut out, &mut pending_tool_results);
    Ok(out)
}

fn flush_pending_tool_results(out: &mut Vec<Value>, pending: &mut Vec<Value>) {
    if pending.is_empty() {
        return;
    }
    let drained = std::mem::take(pending);
    out.push(json!({
        "role": "user",
        "content": Value::Array(drained),
    }));
}

fn anthropic_text_blocks(text: &str) -> Value {
    if text.is_empty() {
        return Value::Array(vec![json!({"type": "text", "text": ""})]);
    }
    Value::Array(vec![json!({"type": "text", "text": text})])
}

fn anthropic_tool_result_content(text: &str) -> Value {
    if text.is_empty() {
        return Value::String(String::new());
    }
    Value::Array(vec![json!({"type": "text", "text": text})])
}

fn anthropic_tools_from_chat_tools(tools_value: &Value) -> Vec<Value> {
    let Some(items) = tools_value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|tool| {
            if tool.get("type").and_then(Value::as_str) != Some("function") {
                return None;
            }
            let function = tool.get("function").unwrap_or(&Value::Null);
            let name = function.get("name").and_then(Value::as_str)?;
            let description = function
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("");
            let input_schema = function.get("parameters").cloned().unwrap_or_else(|| {
                json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false,
                })
            });
            Some(json!({
                "name": name,
                "description": description,
                "input_schema": input_schema,
            }))
        })
        .collect()
}

fn anthropic_tool_choice(choice: &Value) -> Value {
    match choice {
        Value::String(text) => match text.as_str() {
            "any" | "required" | "tool" => Value::String("any".to_string()),
            "none" => Value::String("none".to_string()),
            _ => Value::String("auto".to_string()),
        },
        Value::Object(map) => {
            if let Some(name) = map
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
            {
                Value::String(name.to_string())
            } else {
                Value::String("auto".to_string())
            }
        }
        _ => Value::String("auto".to_string()),
    }
}

pub fn anthropic_messages_response_to_chat_response(
    response: &Value,
    fallback_model: &str,
) -> Result<Value, CodexProxyError> {
    let response_id = response
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("anthropic_msg")
        .to_string();
    let model = response
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(fallback_model)
        .to_string();
    let created = response
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_else(current_unix_timestamp);
    let stop_reason = response
        .get("stop_reason")
        .and_then(Value::as_str)
        .unwrap_or("end_turn");
    let finish_reason = match stop_reason {
        "end_turn" | "stop_sequence" => "stop",
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        other => other,
    }
    .to_string();

    let mut message = Map::new();
    message.insert("role".to_string(), Value::String("assistant".to_string()));
    let mut text_segments: Vec<String> = Vec::new();
    let mut reasoning_segments: Vec<String> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();

    if let Some(content) = response.get("content").and_then(Value::as_array) {
        for block in content {
            let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(Value::as_str)
                        && !text.is_empty()
                    {
                        text_segments.push(text.to_string());
                    }
                }
                "thinking" => {
                    if let Some(thinking) = block.get("thinking").and_then(Value::as_str)
                        && !thinking.trim().is_empty()
                    {
                        reasoning_segments.push(thinking.to_string());
                    }
                }
                "tool_use" => {
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("toolu_unknown")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
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
                "tool_result" => {
                    // Anthropic bundles tool results in the user role; we
                    // expect them on the request side. If a response ever
                    // echoes one back, surface its text content as plain
                    // assistant text so callers can still see something.
                    if let Some(text) = block_content_to_text(block)
                        && !text.is_empty()
                    {
                        text_segments.push(text);
                    }
                }
                _ => {
                    if let Some(text) = block_content_to_text(block)
                        && !text.is_empty()
                    {
                        text_segments.push(text);
                    }
                }
            }
        }
    }

    if !reasoning_segments.is_empty() {
        message.insert(
            "reasoning_content".to_string(),
            Value::String(reasoning_segments.join("\n\n")),
        );
    }
    let combined_text = text_segments.join("");
    message.insert("content".to_string(), Value::String(combined_text));
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(tool_calls));
    }

    let usage = response.get("usage").cloned().unwrap_or_else(|| json!({}));
    let prompt_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let completion_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = prompt_tokens + completion_tokens;

    Ok(json!({
        "id": response_id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": finish_reason,
            "stop_reason": stop_reason,
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": total_tokens,
        }
    }))
}

fn block_content_to_text(block: &Value) -> Option<String> {
    if let Some(text) = block.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(content) = block.get("content") {
        return Some(content_to_text(content));
    }
    None
}

pub fn anthropic_provider_label() -> &'static str {
    ANTHROPIC_PROVIDER_LABEL
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
