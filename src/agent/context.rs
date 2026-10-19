//! Conversation payloads, context accounting, and history compaction.

use super::{AgentMessage, AgentSession, LlmCredential, tool_definitions};
use crate::{ApiResult, AppError, AppState, llm};
use serde::Serialize;
use serde_json::{Value, json};

pub(super) fn build_api_messages(
    session: &AgentSession,
    agent_instructions: &[String],
) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(system_prompt) = &session.system_prompt
        && !system_prompt.trim().is_empty()
    {
        messages.push(json!({"role": "system", "content": system_prompt}));
    } else {
        messages.push(json!({
            "role": "system",
            "content": "你是 webClx 内置工程智能体，可以独立检查和修改工作区，不需要启动 Codex。\
        优先使用 list_files、search_files、read_file、apply_patch、git_diff 和 run_verification 等结构化工具；\
        只做完成任务所需的最小修改，修改前理解上下文，修改后运行验证并报告证据。\
        用户消息中的 $skill-name 表示显式调用对应 Skill；webClx 会自动插入 read_skill 工具结果，请直接遵循已加载的指令。\
        Codex skills 仅用于读取专项操作说明或执行其中明确需要的脚本，不要通过 Codex CLI 完成普通工程任务。"
        }));
    }
    if !agent_instructions.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": format!(
                "按从宽泛到具体的顺序遵循以下 AGENTS.md 指令；更具体的文件在冲突时优先：\n\n{}",
                agent_instructions.join("\n\n---\n\n")
            )
        }));
    }
    if let Some(summary) = session
        .context_summary
        .as_deref()
        .filter(|summary| !summary.trim().is_empty())
    {
        messages.push(json!({
            "role": "system",
                "content": format!(
                    "以下是此前对话的压缩摘要。把它视为已发生的上下文，并继续处理当前消息：\n\n{summary}"
                )
            }));
    }
    if !session.context_files.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": format!(
                "本会话压缩时保留的相关文件清单（可能不完整，需要时可重新读取）：\n{}",
                session
                    .context_files
                    .iter()
                    .map(|file| format!("- {file}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }));
    }
    let tool_result_ids = session
        .messages
        .iter()
        .filter(|message| message.role == "tool")
        .filter_map(|message| message.tool_call_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let call_ids = session
        .messages
        .iter()
        .filter(|message| message.role == "assistant")
        .filter_map(|message| message.tool_calls.as_ref())
        .flat_map(|calls| calls.iter().map(|call| call.id.clone()))
        .collect::<std::collections::HashSet<_>>();
    for msg in &session.messages {
        let mut entry = json!({
            "role": msg.role,
            "content": api_message_content(msg.content.as_ref()),
        });
        if let Some(tool_calls) = &msg.tool_calls {
            let paired = tool_calls
                .iter()
                .filter(|call| tool_result_ids.contains(&call.id))
                .cloned()
                .collect::<Vec<_>>();
            if !paired.is_empty() {
                entry["tool_calls"] = json!(paired);
            }
        }
        if let Some(reasoning_content) = &msg.reasoning_content {
            entry["reasoning_content"] = json!(reasoning_content);
        }
        if let Some(tool_call_id) = &msg.tool_call_id
            && tool_result_ids.contains(tool_call_id)
            && call_ids.contains(tool_call_id)
        {
            entry["tool_call_id"] = json!(tool_call_id);
        }
        if let Some(name) = &msg.name {
            entry["name"] = json!(name);
        }
        messages.push(entry);
    }
    messages
}

fn api_message_content(content: Option<&Value>) -> Value {
    match content {
        Some(value @ (Value::String(_) | Value::Array(_))) => value.clone(),
        Some(Value::Null) | None => Value::String(String::new()),
        Some(value) => Value::String(serde_json::to_string(value).unwrap_or_default()),
    }
}

fn is_cjk_character(character: char) -> bool {
    let code = character as u32;
    matches!(code,
        0x3000..=0x303F
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0xFF00..=0xFFEF
        | 0x20000..=0x2FA1F
    )
}

pub(super) fn estimate_text_tokens(text: &str) -> u64 {
    let mut tokens = 0u64;
    let mut ascii_chars = 0u64;
    for character in text.chars() {
        if character.is_ascii() {
            ascii_chars += 1;
        } else if is_cjk_character(character) {
            tokens += 1;
        } else {
            tokens += 1;
        }
    }
    tokens + ascii_chars.div_ceil(4)
}

pub(super) fn estimate_json_tokens(value: &Value) -> u64 {
    match value {
        Value::String(text) => estimate_text_tokens(text).saturating_add(2),
        Value::Array(items) => items
            .iter()
            .map(estimate_json_tokens)
            .sum::<u64>()
            .saturating_add(1),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| {
                estimate_text_tokens(key)
                    .saturating_add(2)
                    .saturating_add(estimate_json_tokens(value))
            })
            .sum::<u64>()
            .saturating_add(1),
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(number) => number.to_string().len().div_ceil(4).saturating_add(1) as u64,
    }
}

fn estimate_context_tokens(session: &AgentSession) -> u64 {
    let payload = json!({
        "messages": build_api_messages(session, &[]),
        "tools": tool_definitions(),
    });
    estimate_json_tokens(&payload)
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentContextStatus {
    pub model: String,
    pub used_tokens: u64,
    pub context_window: u64,
    pub used_percent: u16,
    pub compact_threshold: u64,
    pub compacted_messages: u64,
    pub compacted_at: Option<u64>,
    pub context_window_source: String,
    pub context_usage_source: String,
}

pub(super) fn context_status(
    session: &AgentSession,
    credential: &LlmCredential,
) -> AgentContextStatus {
    let api_tokens = session
        .last_token_usage
        .as_ref()
        .map(|usage| usage.input_tokens)
        .filter(|tokens| *tokens > 0);
    let used_tokens = api_tokens.unwrap_or_else(|| estimate_context_tokens(session));
    let used_percent = used_tokens
        .saturating_mul(100)
        .div_ceil(credential.context_limits.context_window)
        .min(999) as u16;
    AgentContextStatus {
        model: credential.model.clone(),
        used_tokens,
        context_window: credential.context_limits.context_window,
        used_percent,
        compact_threshold: credential.context_limits.compact_threshold,
        compacted_messages: session.compacted_messages,
        compacted_at: session.compacted_at,
        context_window_source: credential.context_limits.source.clone(),
        context_usage_source: if api_tokens.is_some() {
            "api".to_string()
        } else {
            "estimated".to_string()
        },
    }
}

pub(super) fn compact_split_index(messages: &[AgentMessage]) -> Option<usize> {
    messages
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, message)| (index > 0 && message.role == "user").then_some(index))
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactSessionResult {
    pub compacted: bool,
    pub removed_messages: u64,
    pub status: AgentContextStatus,
}

pub(super) fn parse_compact_reply(reply: &str) -> (String, Vec<String>) {
    let mut summary = String::new();
    let mut files = Vec::new();
    let mut in_files = false;
    for line in reply.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("摘要：")
            .or_else(|| trimmed.strip_prefix("摘要:"))
        {
            summary = rest.trim().to_string();
            in_files = false;
        } else if trimmed.starts_with("相关文件") || trimmed.starts_with("涉及文件") {
            in_files = true;
        } else if in_files {
            let marker_trimmed = trimmed.trim_start_matches(' ').trim_start_matches('\t');
            if marker_trimmed.starts_with('-')
                || marker_trimmed.starts_with('*')
                || marker_trimmed.starts_with('•')
            {
                let file = marker_trimmed
                    .trim_start_matches(|character| {
                        matches!(character, '-' | '*' | '•' | ' ' | '\t')
                    })
                    .trim();
                if !file.is_empty() {
                    files.push(file.to_string());
                }
            } else if !trimmed.is_empty() && !summary.is_empty() {
                summary.push('\n');
                summary.push_str(trimmed);
                in_files = false;
            }
        } else if !summary.is_empty() && !trimmed.is_empty() {
            summary.push('\n');
            summary.push_str(trimmed);
        }
    }
    if summary.is_empty() {
        summary = reply.trim().to_string();
    }
    files.sort();
    files.dedup();
    (summary, files)
}

pub(super) async fn compact_session_history(
    state: &AppState,
    session_id: &str,
    credential: &LlmCredential,
    client: &reqwest::Client,
) -> ApiResult<CompactSessionResult> {
    let session = state
        .agent_manager
        .get_session(session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let Some(split_index) = compact_split_index(&session.messages) else {
        return Ok(CompactSessionResult {
            compacted: false,
            removed_messages: 0,
            status: context_status(&session, credential),
        });
    };
    let history = json!({
        "previous_summary": session.context_summary.as_deref(),
        "messages": &session.messages[..split_index],
    });
    let reply = llm::call_conversation(
        client,
        &credential.target,
        &credential.model,
        vec![
            json!({
                "role": "system",
                "content": "压缩下面的工程智能体对话。保留用户目标、关键决定、已修改文件、命令与验证结果、错误、未完成事项以及继续工作所需的具体上下文。忽略闲聊和重复内容。不要调用工具。\
                输出格式严格如下（没有相关文件时只输出第一行）：\n\
                摘要：<结构清晰的摘要>\n\
                相关文件：\n\
                - <相对路径>\n\
                - <相对路径>"
            }),
            json!({"role": "user", "content": history.to_string()}),
        ],
        Vec::new(),
    )
    .await
    .map_err(|error| AppError::internal(format!("压缩对话失败: {}", error.message)))?;
    let summary = reply
        .content
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| AppError::internal("压缩对话失败：模型未返回摘要。"))?;
    let (summary, context_files) = parse_compact_reply(&summary);
    if summary.is_empty() {
        return Err(AppError::internal("压缩对话失败：模型未返回摘要。"));
    }
    let updated = state
        .agent_manager
        .replace_compacted_history(session_id, split_index, summary, context_files)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    Ok(CompactSessionResult {
        compacted: true,
        removed_messages: split_index as u64,
        status: context_status(&updated, credential),
    })
}
