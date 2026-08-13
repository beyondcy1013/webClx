//! Agent subsystem: LLM conversation loop with tool calling.
//!
//! Reuses the selected or currently applied Codex_API preset through the shared
//! LLM target resolver. Core tools are skill discovery and execution, enabling
//! the webClx UI to drive operations like stockScreener full-market testing
//! through natural language.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    fs,
    process::Command,
    sync::{RwLock, mpsc, watch},
};
use tracing::warn;

use crate::{ApiResult, AppError, AppState, builtin_skills, filesystem, llm, runtime_paths};
use auth_core::{
    AUTH_FILE_RELATIVE_PATH, CONFIG_FILE_RELATIVE_PATH, PresetConfigOverride, api_preset_model,
    derive_current_api_state, read_current_auth_state, read_current_config_provider,
};

mod background_commands;
mod engineering_tools;
mod extended_tools;
mod preset_exec;
pub use preset_exec::exec_with_preset;

const AGENT_SESSIONS_FILE: &str = ".webclx-agent-sessions.json";
const AGENT_CONFIG_FILE: &str = ".webclx-agent-config.json";
const SKILLS_DIR_RELATIVE: &str = ".codex/skills";
const LLM_TIMEOUT_SECS: u64 = 300;
const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_CONTEXT_WINDOW: u64 = 128_000;
const DEFAULT_COMPACT_PERCENT: u64 = 80;
const MAX_TOOL_ITERATIONS: u8 = 15;
const MAX_SKILL_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_TERMINAL_AGENT_PROFILES: usize = 64;
const LLM_RETRY_ATTEMPTS: u8 = 2;
const LLM_RETRY_DELAYS_SECS: [u64; 2] = [1, 3];

// ---------------------------------------------------------------------------
// Session management
// ---------------------------------------------------------------------------

pub type AgentSessionStore = Arc<RwLock<HashMap<String, AgentSession>>>;

#[derive(Clone)]
pub struct AgentManager {
    sessions: AgentSessionStore,
    file_path: PathBuf,
    background_commands: background_commands::BackgroundCommandManager,
    chat_runs: Arc<RwLock<HashMap<String, ActiveChatRun>>>,
    approvals: Arc<RwLock<HashMap<String, ApprovalState>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingApproval {
    pub id: String,
    pub tool: String,
    pub summary: String,
    pub status: String,
    pub created_at: u64,
}

#[derive(Debug, Default)]
struct ApprovalState {
    pending: Vec<PendingApproval>,
    approved_keys: std::collections::HashSet<String>,
}

#[derive(Clone)]
struct ActiveChatRun {
    id: String,
    started_at: u64,
    cancel: watch::Sender<bool>,
    queued_messages: mpsc::UnboundedSender<AgentMessage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRunStatus {
    pub running: bool,
    pub run_id: Option<String>,
    pub started_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub title: String,
    pub model: String,
    #[serde(default)]
    pub api_preset_id: String,
    #[serde(default)]
    pub profile_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default = "default_sandbox_mode")]
    pub sandbox_mode: String,
    #[serde(default = "default_approval_policy")]
    pub approval_policy: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_files: Vec<String>,
    #[serde(default)]
    pub compacted_messages: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_token_usage: Option<llm::LlmTokenUsage>,
    #[serde(default = "default_context_usage_source")]
    pub context_usage_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_run: Option<AgentRunMarker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_interrupted_at: Option<u64>,
    pub messages: Vec<AgentMessage>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunMarker {
    pub run_id: String,
    pub started_at: u64,
}

fn default_context_usage_source() -> String {
    "estimated".to_string()
}

fn default_sandbox_mode() -> String {
    "default".to_string()
}

fn default_approval_policy() -> String {
    "ask_once".to_string()
}

fn normalize_approval_policy(policy: &str) -> &str {
    match policy.trim() {
        "ask_each" | "allow_all" => policy.trim(),
        _ => "ask_once",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: String,
    pub content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

impl AgentManager {
    pub fn new(app_dir: &Path) -> Self {
        let file_path = app_dir.join(AGENT_SESSIONS_FILE);
        let mut sessions = Self::load_sessions(&file_path);
        let repaired = Self::repair_message_pairs(&mut sessions);
        let interrupted = Self::interrupt_stale_runs(&mut sessions);
        if interrupted || repaired {
            if let Ok(encoded) = serde_json::to_vec_pretty(&sessions) {
                let _ = std::fs::write(&file_path, encoded);
            }
        }
        let manager = Self {
            sessions: Arc::new(RwLock::new(sessions)),
            file_path,
            background_commands: background_commands::BackgroundCommandManager::new(app_dir),
            chat_runs: Arc::new(RwLock::new(HashMap::new())),
            approvals: Arc::new(RwLock::new(HashMap::new())),
        };
        manager
    }

    fn repair_message_pairs(sessions: &mut HashMap<String, AgentSession>) -> bool {
        let mut changed = false;
        for session in sessions.values_mut() {
            let result_ids = session
                .messages
                .iter()
                .filter(|message| message.role == "tool")
                .filter_map(|message| message.tool_call_id.clone())
                .collect::<std::collections::HashSet<_>>();
            let mut mutated = false;
            for message in session.messages.iter_mut() {
                if message.role == "assistant"
                    && let Some(tool_calls) = &mut message.tool_calls
                {
                    let original_len = tool_calls.len();
                    tool_calls.retain(|call| result_ids.contains(&call.id));
                    mutated |= tool_calls.len() != original_len;
                }
            }
            let call_ids = session
                .messages
                .iter()
                .filter(|message| message.role == "assistant")
                .filter_map(|message| message.tool_calls.as_ref())
                .flat_map(|calls| calls.iter().map(|call| call.id.clone()))
                .collect::<std::collections::HashSet<_>>();
            let original_len = session.messages.len();
            session.messages.retain(|message| {
                message.role != "tool"
                    || message
                        .tool_call_id
                        .as_ref()
                        .is_some_and(|id| call_ids.contains(id))
            });
            mutated |= session.messages.len() != original_len;
            if mutated {
                session.updated_at = current_timestamp();
                changed = true;
            }
        }
        changed
    }

    fn load_sessions(path: &Path) -> HashMap<String, AgentSession> {
        match std::fs::read_to_string(path) {
            Ok(content) if !content.trim().is_empty() => {
                serde_json::from_str(&content).unwrap_or_default()
            }
            _ => HashMap::new(),
        }
    }

    fn persist_sessions(&self, sessions: &HashMap<String, AgentSession>) {
        if let Ok(encoded) = serde_json::to_vec_pretty(sessions) {
            if let Err(error) = std::fs::write(&self.file_path, encoded) {
                warn!("failed to persist agent sessions: {error}");
            }
        }
    }

    fn interrupt_stale_runs(sessions: &mut HashMap<String, AgentSession>) -> bool {
        let mut changed = false;
        let now = current_timestamp();
        for session in sessions.values_mut() {
            if session.active_run.is_some() {
                session.active_run = None;
                session.run_interrupted_at = Some(now);
                session.updated_at = now;
                changed = true;
            }
        }
        changed
    }

    pub async fn list_sessions(&self) -> Vec<AgentSession> {
        let sessions = self.sessions.read().await;
        let mut list: Vec<AgentSession> = sessions.values().cloned().collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    pub async fn get_session(&self, id: &str) -> Option<AgentSession> {
        self.sessions.read().await.get(id).cloned()
    }

    pub async fn create_session(
        &self,
        title: &str,
        model: &str,
        api_preset_id: &str,
        profile_id: &str,
        cwd: &str,
        system_prompt: Option<String>,
    ) -> AgentSession {
        let session = AgentSession {
            id: generate_id(),
            title: title.to_string(),
            model: model.to_string(),
            api_preset_id: api_preset_id.to_string(),
            profile_id: profile_id.to_string(),
            cwd: cwd.to_string(),
            sandbox_mode: "default".to_string(),
            approval_policy: "ask_once".to_string(),
            system_prompt,
            context_summary: None,
            context_files: Vec::new(),
            compacted_messages: 0,
            compacted_at: None,
            last_token_usage: None,
            context_usage_source: "estimated".to_string(),
            active_run: None,
            run_interrupted_at: None,
            messages: Vec::new(),
            created_at: current_timestamp(),
            updated_at: current_timestamp(),
        };
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());
        self.persist_sessions(&sessions);
        session
    }

    pub async fn update_session_settings(
        &self,
        id: &str,
        title: &str,
        model: Option<&str>,
        api_preset_id: Option<&str>,
        sandbox_mode: Option<&str>,
        approval_policy: Option<&str>,
    ) -> Option<AgentSession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(id)?;
        session.title = title.to_string();
        if let Some(model) = model {
            session.model = model.to_string();
        }
        if let Some(api_preset_id) = api_preset_id {
            session.api_preset_id = api_preset_id.to_string();
        }
        if let Some(sandbox_mode) = sandbox_mode.filter(|mode| !mode.trim().is_empty()) {
            session.sandbox_mode = normalize_sandbox_mode(sandbox_mode).to_string();
        }
        if let Some(approval_policy) = approval_policy.filter(|policy| !policy.trim().is_empty()) {
            session.approval_policy = normalize_approval_policy(approval_policy).to_string();
        }
        session.updated_at = current_timestamp();
        let result = session.clone();
        self.persist_sessions(&sessions);
        Some(result)
    }

    pub async fn delete_session(&self, id: &str) -> bool {
        let removed = {
            let mut sessions = self.sessions.write().await;
            let removed = sessions.remove(id).is_some();
            if removed {
                self.persist_sessions(&sessions);
            }
            removed
        };
        if removed {
            self.background_commands.terminate_all(id).await;
            let mut approvals = self.approvals.write().await;
            approvals.remove(id);
        }
        removed
    }

    pub async fn append_messages(
        &self,
        session_id: &str,
        messages: Vec<AgentMessage>,
    ) -> Option<AgentSession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(session_id)?;
        session.messages.extend(messages);
        session.updated_at = current_timestamp();
        let result = session.clone();
        self.persist_sessions(&sessions);
        Some(result)
    }

    pub async fn clear_messages(&self, session_id: &str) -> Option<AgentSession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(session_id)?;
        session.messages.clear();
        session.context_summary = None;
        session.context_files.clear();
        session.compacted_messages = 0;
        session.compacted_at = None;
        session.last_token_usage = None;
        session.context_usage_source = "estimated".to_string();
        session.updated_at = current_timestamp();
        let result = session.clone();
        self.persist_sessions(&sessions);
        Some(result)
    }

    pub async fn replace_compacted_history(
        &self,
        session_id: &str,
        split_index: usize,
        summary: String,
        context_files: Vec<String>,
    ) -> Option<AgentSession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(session_id)?;
        let removed = split_index.min(session.messages.len());
        session.messages.drain(..removed);
        session.context_summary = Some(summary);
        session.context_files = context_files;
        session.compacted_messages = session.compacted_messages.saturating_add(removed as u64);
        session.compacted_at = Some(current_timestamp());
        session.updated_at = current_timestamp();
        let result = session.clone();
        self.persist_sessions(&sessions);
        Some(result)
    }

    pub async fn update_context_summary(
        &self,
        session_id: &str,
        summary: &str,
        context_files: &[String],
    ) -> Option<AgentSession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(session_id)?;
        let trimmed = summary.trim();
        session.context_summary = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        let mut files = context_files
            .iter()
            .map(|file| file.trim().to_string())
            .filter(|file| !file.is_empty())
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        session.context_files = files;
        session.updated_at = current_timestamp();
        let result = session.clone();
        self.persist_sessions(&sessions);
        Some(result)
    }

    pub async fn update_token_usage(
        &self,
        session_id: &str,
        usage: llm::LlmTokenUsage,
    ) -> Option<AgentSession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(session_id)?;
        session.last_token_usage = Some(usage);
        session.context_usage_source = "api".to_string();
        session.updated_at = current_timestamp();
        let result = session.clone();
        self.persist_sessions(&sessions);
        Some(result)
    }

    pub async fn begin_chat_run(
        &self,
        session_id: &str,
    ) -> ApiResult<(String, watch::Receiver<bool>, mpsc::UnboundedReceiver<AgentMessage>)> {
        let mut runs = self.chat_runs.write().await;
        if runs.contains_key(session_id) {
            return Err(AppError::bad_request("当前会话已有一轮对话正在运行。"));
        }
        let id = format!("run-{}", generate_id());
        let started_at = current_timestamp();
        let (cancel, receiver) = watch::channel(false);
        let (queued_messages, queued_receiver) = mpsc::unbounded_channel();
        runs.insert(
            session_id.to_string(),
            ActiveChatRun {
                id: id.clone(),
                started_at,
                cancel,
                queued_messages,
            },
        );
        drop(runs);
        self.mark_run_active(session_id, &id, started_at).await;
        Ok((id, receiver, queued_receiver))
    }

    pub async fn mark_run_active(&self, session_id: &str, run_id: &str, started_at: u64) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.active_run = Some(AgentRunMarker {
                run_id: run_id.to_string(),
                started_at,
            });
            session.run_interrupted_at = None;
            session.updated_at = current_timestamp();
            let sessions_snapshot = sessions.clone();
            self.persist_sessions(&sessions_snapshot);
        }
    }

    pub async fn clear_run_marker(&self, session_id: &str, run_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id)
            && session
                .active_run
                .as_ref()
                .is_some_and(|marker| marker.run_id == run_id)
        {
            session.active_run = None;
            session.updated_at = current_timestamp();
            let sessions_snapshot = sessions.clone();
            self.persist_sessions(&sessions_snapshot);
        }
    }

    pub async fn active_run_status(&self, session_id: &str) -> AgentRunStatus {
        let runs = self.chat_runs.read().await;
        match runs.get(session_id) {
            Some(run) => AgentRunStatus {
                running: true,
                run_id: Some(run.id.clone()),
                started_at: Some(run.started_at),
            },
            None => AgentRunStatus {
                running: false,
                run_id: None,
                started_at: None,
            },
        }
    }

    pub async fn queue_chat_message(&self, session_id: &str, message: AgentMessage) -> bool {
        let runs = self.chat_runs.read().await;
        runs.get(session_id)
            .is_some_and(|run| run.queued_messages.send(message).is_ok())
    }

    pub async fn cancel_chat_run(&self, session_id: &str) -> bool {
        let runs = self.chat_runs.read().await;
        let Some(run) = runs.get(session_id) else {
            return false;
        };
        run.cancel.send(true).is_ok()
    }

    async fn finish_chat_run(&self, session_id: &str, run_id: &str) {
        let mut runs = self.chat_runs.write().await;
        if runs.get(session_id).is_some_and(|run| run.id == run_id) {
            runs.remove(session_id);
        }
        drop(runs);
        self.clear_run_marker(session_id, run_id).await;
    }

    pub async fn request_approval(
        &self,
        session_id: &str,
        tool: &str,
        summary: &str,
    ) -> PendingApproval {
        let approval = PendingApproval {
            id: format!("aprv-{}", generate_id()),
            tool: tool.to_string(),
            summary: summary.to_string(),
            status: "pending".to_string(),
            created_at: current_timestamp(),
        };
        let mut approvals = self.approvals.write().await;
        let state = approvals.entry(session_id.to_string()).or_default();
        if state.pending.len() >= 50 {
            state.pending.retain(|item| item.status != "pending");
        }
        state.pending.push(approval.clone());
        approval
    }

    fn approval_key(tool: &str, summary: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(tool.as_bytes());
        hasher.update(b"\n");
        hasher.update(summary.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub async fn is_approved(&self, session_id: &str, tool: &str, summary: &str) -> bool {
        let key = Self::approval_key(tool, summary);
        let approvals = self.approvals.read().await;
        approvals
            .get(session_id)
            .is_some_and(|state| state.approved_keys.contains(&key))
    }

    pub async fn find_pending_approval(
        &self,
        session_id: &str,
        tool: &str,
        summary: &str,
    ) -> Option<PendingApproval> {
        let approvals = self.approvals.read().await;
        approvals
            .get(session_id)?
            .pending
            .iter()
            .find(|item| item.status == "pending" && item.tool == tool && item.summary == summary)
            .cloned()
    }

    pub async fn approve_approval(
        &self,
        session_id: &str,
        approval_id: &str,
    ) -> Option<PendingApproval> {
        let mut approvals = self.approvals.write().await;
        let state = approvals.get_mut(session_id)?;
        let approval = state
            .pending
            .iter_mut()
            .find(|item| item.id == approval_id)?;
        if approval.status != "pending" {
            return None;
        }
        approval.status = "approved".to_string();
        state
            .approved_keys
            .insert(Self::approval_key(&approval.tool, &approval.summary));
        let result = approval.clone();
        drop(approvals);
        self.promote_ask_once_to_allow_all(session_id).await;
        Some(result)
    }

    pub async fn approve_all_pending(&self, session_id: &str) -> usize {
        let mut approved = 0usize;
        {
            let mut approvals = self.approvals.write().await;
            if let Some(state) = approvals.get_mut(session_id) {
                for item in state.pending.iter_mut() {
                    if item.status != "pending" {
                        continue;
                    }
                    item.status = "approved".to_string();
                    state
                        .approved_keys
                        .insert(Self::approval_key(&item.tool, &item.summary));
                    approved += 1;
                }
            }
        }
        self.promote_ask_once_to_allow_all(session_id).await;
        approved
    }

    async fn promote_ask_once_to_allow_all(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id)
            && session.approval_policy == "ask_once"
        {
            session.approval_policy = "allow_all".to_string();
            session.updated_at = current_timestamp();
            let sessions_snapshot = sessions.clone();
            self.persist_sessions(&sessions_snapshot);
        }
    }

    pub async fn deny_approval(
        &self,
        session_id: &str,
        approval_id: &str,
    ) -> Option<PendingApproval> {
        let mut approvals = self.approvals.write().await;
        let state = approvals.get_mut(session_id)?;
        let approval = state
            .pending
            .iter_mut()
            .find(|item| item.id == approval_id)?;
        if approval.status != "pending" {
            return None;
        }
        approval.status = "denied".to_string();
        Some(approval.clone())
    }

    pub async fn list_approvals(&self, session_id: &str) -> Vec<PendingApproval> {
        let approvals = self.approvals.read().await;
        approvals
            .get(session_id)
            .map(|state| state.pending.clone())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Agent configuration (skill management)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AgentConfigManager {
    config: Arc<RwLock<AgentConfig>>,
    file_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub api_preset_id: String,
    #[serde(default)]
    pub disabled_skills: Vec<String>,
    #[serde(default)]
    pub extra_skill_dirs: Vec<String>,
    #[serde(default)]
    pub system_prompt_override: Option<String>,
    #[serde(default = "default_terminal_agent_profiles")]
    pub terminal_agent_profiles: Vec<TerminalAgentProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalAgentProfile {
    pub id: String,
    pub name: String,
    #[serde(default = "default_terminal_agent_type")]
    pub agent_type: String,
    #[serde(default)]
    pub description: String,
    pub preset_selector: String,
    pub preset_match: String,
    pub cwd: String,
    pub project_path: String,
    pub skill_name: String,
    pub initial_task: String,
    pub terminal_name: String,
}

fn default_terminal_agent_type() -> String {
    "codex".to_string()
}

fn default_terminal_agent_profiles() -> Vec<TerminalAgentProfile> {
    vec![
        TerminalAgentProfile {
            id: "proxy_settings_agent".to_string(),
            name: "代理设置".to_string(),
            agent_type: default_terminal_agent_type(),
            description: "检查并配置本机 Mihomo 代理，处理节点、连通性和代理环境问题。".to_string(),
            preset_selector: "miniMax".to_string(),
            preset_match: "unique_contains".to_string(),
            cwd: "/home/system".to_string(),
            project_path: "/home/system".to_string(),
            skill_name: "mihomo-proxy-ops".to_string(),
            initial_task: "请检查当前代理配置，并根据当前环境完成代理设置。".to_string(),
            terminal_name: "代理设置".to_string(),
        },
        TerminalAgentProfile {
            id: "work_agent".to_string(),
            name: "工作代理".to_string(),
            agent_type: default_terminal_agent_type(),
            description: "在 /home/third_party 中接收并处理通用开发与运维任务。".to_string(),
            preset_selector: "miniMax".to_string(),
            preset_match: "unique_contains".to_string(),
            cwd: "/home/third_party".to_string(),
            project_path: "/home/third_party".to_string(),
            skill_name: "autopilot".to_string(),
            initial_task: "请等待并接收用户接下来输入的工作任务。".to_string(),
            terminal_name: "工作代理".to_string(),
        },
    ]
}

fn normalize_terminal_agent_profile(
    profile: &TerminalAgentProfile,
) -> ApiResult<TerminalAgentProfile> {
    let clean = |value: &str| value.trim().to_string();
    let normalized = TerminalAgentProfile {
        id: clean(&profile.id),
        name: clean(&profile.name),
        agent_type: clean(&profile.agent_type).to_ascii_lowercase(),
        description: clean(&profile.description),
        preset_selector: clean(&profile.preset_selector),
        preset_match: clean(&profile.preset_match),
        cwd: clean(&profile.cwd),
        project_path: clean(&profile.project_path),
        skill_name: clean(&profile.skill_name),
        initial_task: clean(&profile.initial_task),
        terminal_name: clean(&profile.terminal_name),
    };
    if normalized.id.is_empty()
        || normalized.id.len() > 64
        || !normalized
            .id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(AppError::bad_request("智能体 ID 只能包含字母、数字、下划线和连字符。"));
    }
    if normalized.name.is_empty() || normalized.name.chars().count() > 64 {
        return Err(AppError::bad_request("智能体名称不能为空或超过 64 个字符。"));
    }
    if !matches!(normalized.agent_type.as_str(), "native" | "codex" | "claude") {
        return Err(AppError::bad_request("智能体类型只能是 native、codex 或 claude。"));
    }
    if normalized.description.chars().count() > 240
        || normalized.description.chars().any(char::is_control)
    {
        return Err(AppError::bad_request("智能体说明不能超过 240 个字符或包含控制字符。"));
    }
    if normalized.preset_selector.is_empty() || normalized.preset_selector.chars().count() > 128 {
        return Err(AppError::bad_request("智能体必须指定有效的 API 预设。"));
    }
    if !matches!(normalized.preset_match.as_str(), "id" | "exact_name" | "unique_contains") {
        return Err(AppError::bad_request("智能体预设匹配方式无效。"));
    }
    if !Path::new(&normalized.cwd).is_absolute()
        || !Path::new(&normalized.project_path).is_absolute()
    {
        return Err(AppError::bad_request("智能体工作目录和项目路径必须是绝对路径。"));
    }
    if normalized.skill_name.is_empty()
        || normalized.skill_name.chars().count() > 128
        || normalized.skill_name.chars().any(char::is_whitespace)
    {
        return Err(AppError::bad_request("智能体必须指定一个有效的 skill 名称。"));
    }
    if normalized.initial_task.chars().count() > 4096
        || normalized.terminal_name.is_empty()
        || normalized.terminal_name.chars().count() > 64
    {
        return Err(AppError::bad_request("智能体任务或终端名称无效。"));
    }
    Ok(normalized)
}

fn normalize_terminal_agent_profiles(
    profiles: &[TerminalAgentProfile],
) -> ApiResult<Vec<TerminalAgentProfile>> {
    if profiles.len() > MAX_TERMINAL_AGENT_PROFILES {
        return Err(AppError::bad_request("智能体不能超过 64 个。"));
    }
    let mut seen = std::collections::HashSet::new();
    profiles
        .iter()
        .map(normalize_terminal_agent_profile)
        .map(|result| {
            let profile = result?;
            if !seen.insert(profile.id.clone()) {
                return Err(AppError::bad_request("智能体 ID 不能重复。"));
            }
            Ok(profile)
        })
        .collect()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default_model: String::new(),
            api_preset_id: String::new(),
            disabled_skills: Vec::new(),
            extra_skill_dirs: Vec::new(),
            system_prompt_override: None,
            terminal_agent_profiles: default_terminal_agent_profiles(),
        }
    }
}

impl AgentConfigManager {
    pub fn new(app_dir: &Path) -> Self {
        let file_path = app_dir.join(AGENT_CONFIG_FILE);
        let config = Self::load_config(&file_path);
        Self {
            config: Arc::new(RwLock::new(config)),
            file_path,
        }
    }

    fn load_config(path: &Path) -> AgentConfig {
        match std::fs::read_to_string(path) {
            Ok(content) if !content.trim().is_empty() => {
                serde_json::from_str(&content).unwrap_or_default()
            }
            _ => AgentConfig::default(),
        }
    }

    fn persist(&self, config: &AgentConfig) {
        if let Ok(encoded) = serde_json::to_vec_pretty(config) {
            if let Err(error) = std::fs::write(&self.file_path, encoded) {
                warn!("failed to persist agent config: {error}");
            }
        }
    }

    pub async fn get(&self) -> AgentConfig {
        self.config.read().await.clone()
    }

    pub async fn update<F>(&self, updater: F) -> AgentConfig
    where
        F: FnOnce(&mut AgentConfig),
    {
        let mut config = self.config.write().await;
        updater(&mut config);
        let result = config.clone();
        self.persist(&result);
        result
    }
}

fn generate_id() -> String {
    use rand::Rng;
    let timestamp = current_timestamp();
    let random: u32 = rand::thread_rng().r#gen();
    format!("{timestamp:x}{random:08x}")
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// LLM credential resolution (reuses current Codex_API preset)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct LlmCredential {
    target: llm::ApiPresetLlmTarget,
    model: String,
    terminal_env: Vec<auth_core::PresetTerminalEnvVar>,
    context_limits: ContextLimits,
}

#[derive(Debug, Clone)]
struct ContextLimits {
    context_window: u64,
    compact_threshold: u64,
    source: String,
    llm_timeout_secs: u64,
}

fn parse_positive_token_limit(value: &str) -> Option<u64> {
    let normalized = value.trim().trim_matches(['"', '\'']).replace('_', "");
    normalized.parse::<u64>().ok().filter(|value| *value > 0)
}

fn context_limits_from_overrides(overrides: &[PresetConfigOverride]) -> ContextLimits {
    let lookup = |key: &str| {
        overrides.iter().find_map(|item| {
            item.key
                .as_deref()
                .filter(|candidate| candidate.eq_ignore_ascii_case(key))
                .and_then(|_| item.value.as_deref())
                .and_then(parse_positive_token_limit)
        })
    };
    let configured_window = lookup("model_context_window");
    let context_window = configured_window.unwrap_or(DEFAULT_CONTEXT_WINDOW);
    let compact_threshold = lookup("model_auto_compact_token_limit")
        .unwrap_or_else(|| context_window.saturating_mul(DEFAULT_COMPACT_PERCENT) / 100)
        .min(context_window);
    let llm_timeout_secs = overrides
        .iter()
        .find_map(|item| {
            item.key
                .as_deref()
                .filter(|candidate| candidate.eq_ignore_ascii_case("llm_timeout_secs"))
                .and_then(|_| item.value.as_deref())
                .and_then(|value| value.trim().parse::<u64>().ok())
        })
        .unwrap_or(LLM_TIMEOUT_SECS)
        .clamp(30, 1800);
    ContextLimits {
        context_window,
        compact_threshold,
        source: if configured_window.is_some() {
            "preset"
        } else {
            "default"
        }
        .to_string(),
        llm_timeout_secs,
    }
}

async fn resolve_llm_credential(
    state: &AppState,
    requested_model: &str,
    session_api_preset_id: &str,
) -> ApiResult<LlmCredential> {
    let config = state.agent_config.get().await;
    let api_preset_id = if session_api_preset_id.is_empty() {
        config.api_preset_id.as_str()
    } else {
        session_api_preset_id
    };
    resolve_llm_credential_with_preset(state, requested_model, api_preset_id).await
}

/// Resolve the effective protocol, route and credentials for Agent calls.
/// A pinned preset and the currently applied preset use the same resolver.
async fn resolve_llm_credential_with_preset(
    state: &AppState,
    requested_model: &str,
    api_preset_id: &str,
) -> ApiResult<LlmCredential> {
    let api_presets = state.auth_manager.api_presets_snapshot();

    let (target, terminal_env, context_limits) = if !api_preset_id.is_empty() {
        let preset = api_presets
            .iter()
            .find(|p| p.id == api_preset_id)
            .ok_or_else(|| {
                AppError::bad_request(format!(
                    "Agent 指定的 API 预设已不存在（id={}）。请在设置页重新选择。",
                    api_preset_id
                ))
            })?;
        if preset.base_url.is_empty() || preset.api_key.is_empty() {
            return Err(AppError::bad_request(format!(
                "Agent 指定的 API 预设缺少 base_url 或 api_key。",
            )));
        }
        (
            llm::api_preset_llm_target(preset),
            preset.terminal_env.clone(),
            context_limits_from_overrides(&preset.config_overrides),
        )
    } else {
        let user = state.workspace_settings.terminal_user();
        let auth_file = runtime_paths::resolve_user_file(&user, AUTH_FILE_RELATIVE_PATH)
            .map_err(|e| AppError::internal(format!("解析用户路径失败: {e}")))?;
        let config_file = runtime_paths::resolve_user_file(&user, CONFIG_FILE_RELATIVE_PATH)
            .map_err(|e| AppError::internal(format!("解析用户路径失败: {e}")))?;

        let current_auth = read_current_auth_state(&auth_file).await.ok().flatten();
        let current_config = read_current_config_provider(&config_file)
            .await
            .ok()
            .flatten();

        let current_api =
            derive_current_api_state(current_config.as_ref(), current_auth.as_ref(), &api_presets);

        let preset_id = current_api
            .as_ref()
            .and_then(|api| api.preset_id.as_deref())
            .ok_or_else(|| {
                AppError::bad_request(
                    "当前 Codex_API 配置无法匹配到已保存预设。请在 Agent 设置选择一个 API 预设，或重新应用预设。",
                )
            })?;
        let preset = api_presets
            .iter()
            .find(|preset| preset.id == preset_id)
            .ok_or_else(|| AppError::bad_request("当前应用的 Codex_API 预设已不存在。"))?;
        (
            llm::api_preset_llm_target(preset),
            preset.terminal_env.clone(),
            context_limits_from_overrides(&preset.config_overrides),
        )
    };

    let model = if requested_model.is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        requested_model.to_string()
    };

    Ok(LlmCredential {
        target,
        model,
        terminal_env,
        context_limits,
    })
}

// ---------------------------------------------------------------------------
// Tool definitions (OpenAI function-calling schema)
// ---------------------------------------------------------------------------

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_skills",
                "description": "列出所有可用的 Codex skills（来自 ~/.codex/skills/）。返回每个 skill 的名称、描述和路径。",
                "parameters": {"type": "object", "properties": {}}
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_skill",
                "description": "读取指定 skill 的 SKILL.md 全文，了解其用法、命令、参数和评估标准。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "skill_name": {
                            "type": "string",
                            "description": "skill 名称（目录名），例如 stockdata-full-market-auction-alert-test"
                        }
                    },
                    "required": ["skill_name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_skill_script",
                "description": "执行指定 skill 目录下的脚本。脚本路径相对于 skill 目录。工作目录由 skill 或用户指定。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "skill_name": {"type": "string", "description": "skill 名称（目录名）"},
                        "script_path": {
                            "type": "string",
                            "description": "脚本相对于 skill 目录的路径，例如 scripts/run_full_market_test.py"
                        },
                        "args": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "命令行参数列表"
                        },
                        "cwd": {
                            "type": "string",
                            "description": "工作目录（可选）。如果不指定，默认从 skill 文档推断。"
                        }
                    },
                    "required": ["skill_name", "script_path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "在允许的工作区目录内执行有超时和输出上限的 shell 命令。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "要执行的命令（通过 bash -c 执行）"},
                        "cwd": {"type": "string", "description": "工作目录（可选，必须位于允许的工作区范围）"},
                        "timeout_secs": {"type": "integer", "minimum": 1, "maximum": 600, "description": "超时秒数，默认 60"}
                    },
                    "required": ["command"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "start_background_command",
                "description": "启动可持续运行的后台 shell 命令会话，立即返回 command_id；之后可读取输出、写入 stdin 或终止。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "要在后台执行的 shell 命令"},
                        "cwd": {"type": "string", "description": "工作目录（可选）"},
                        "rows": {"type": "integer", "minimum": 2, "maximum": 500},
                        "cols": {"type": "integer", "minimum": 20, "maximum": 1000}
                    },
                    "required": ["command"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_background_command",
                "description": "读取后台命令会话的状态、退出码以及当前累计的 stdout/stderr。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command_id": {"type": "string", "description": "start_background_command 返回的 command_id"}
                    },
                    "required": ["command_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_background_command",
                "description": "向仍在运行的后台命令会话写入 stdin。需要换行时在 input 中包含 \\n。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command_id": {"type": "string"},
                        "input": {"type": "string"}
                    },
                    "required": ["command_id", "input"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "terminate_background_command",
                "description": "终止仍在运行的后台命令会话；已结束的会话保持可查询。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command_id": {"type": "string"}
                    },
                    "required": ["command_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "列出工作区目录中的文件，结果有数量上限。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "工作区目录（可选）"},
                        "max_results": {"type": "integer", "minimum": 1, "maximum": 1000}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "search_files",
                "description": "使用 ripgrep 在工作区文本文件中搜索，不经过 shell 拼接。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "正则搜索内容"},
                        "path": {"type": "string", "description": "工作区目录（可选）"},
                        "glob": {"type": "string", "description": "可选 glob，例如 *.rs"},
                        "max_results": {"type": "integer", "minimum": 1, "maximum": 1000}
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "读取工作区内的 UTF-8 文本文件，可按行分页。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "相对工作区根目录的文件路径"},
                        "start_line": {"type": "integer", "minimum": 1},
                        "line_count": {"type": "integer", "minimum": 1, "maximum": 2000}
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "apply_patch",
                "description": "检查并原子应用标准 unified diff；所有文件必须位于当前工作目录内。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "patch": {"type": "string", "description": "完整 unified diff"},
                        "cwd": {"type": "string", "description": "Git 工作目录（可选）"}
                    },
                    "required": ["patch"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "git_diff",
                "description": "读取当前 Git 状态和未提交差异。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "cwd": {"type": "string", "description": "Git 工作目录（可选）"},
                        "path": {"type": "string", "description": "可选的仓库相对路径"}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "create_checkpoint",
                "description": "把当前已跟踪文件的二进制 Git 差异保存到 .git 内的 Agent 检查点。不会修改提交或工作树。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "cwd": {"type": "string", "description": "Git 工作目录（可选）"}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_checkpoints",
                "description": "列出 .git/webclx-agent-checkpoints 中保存的检查点（可撤销的修改快照）。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "cwd": {"type": "string", "description": "Git 工作目录（可选）"}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "restore_checkpoint",
                "description": "把指定检查点的二进制补丁反向应用，撤销该检查点包含的全部修改，工作区回到创建检查点之前的状态。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "cwd": {"type": "string", "description": "Git 工作目录（可选）"},
                        "checkpoint_id": {
                            "type": "string",
                            "description": "create_checkpoint 或 list_checkpoints 返回的 checkpoint_id"
                        }
                    },
                    "required": ["checkpoint_id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_verification",
                "description": "运行有界验证命令并返回 passed、退出码、超时状态和输出证据。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "验证命令"},
                        "cwd": {"type": "string", "description": "工作目录（可选）"},
                        "timeout_secs": {"type": "integer", "minimum": 1, "maximum": 600, "description": "超时秒数，默认 60"}
                    },
                    "required": ["command"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_mcp_tools",
                "description": "列出 Codex config.toml 中已启用 MCP 服务器提供的工具。",
                "parameters": {"type": "object", "properties": {}}
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "call_mcp_tool",
                "description": "调用指定已启用 MCP 服务器的工具。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "server": {"type": "string"},
                        "tool": {"type": "string"},
                        "arguments": {"type": "object"}
                    },
                    "required": ["server", "tool"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "搜索公开网页并返回可供进一步读取的结果文本。",
                "parameters": {
                    "type": "object",
                    "properties": {"query": {"type": "string"}, "max_chars": {"type": "integer"}},
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "web_fetch",
                "description": "读取 HTTP/HTTPS 网页，提取正文文本并返回状态和最终 URL。",
                "parameters": {
                    "type": "object",
                    "properties": {"url": {"type": "string"}, "max_chars": {"type": "integer"}},
                    "required": ["url"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "view_image",
                "description": "读取工作目录中的图片并返回模型可识别的 data URL。",
                "parameters": {
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_browser_actions",
                "description": "通过 Playwright Chromium 执行 goto、click、fill、press、wait_for、text 和 screenshot 动作。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"},
                        "actions": {"type": "array", "items": {"type": "object"}},
                        "timeout_secs": {"type": "integer", "minimum": 1, "maximum": 120}
                    },
                    "required": ["actions"]
                }
            }
        }),
    ]
}

// ---------------------------------------------------------------------------
// Skill tools implementation
// ---------------------------------------------------------------------------

fn skills_dir_for_user(user: &str) -> ApiResult<PathBuf> {
    runtime_paths::resolve_user_home_preferring_env(user)
        .map(|home| home.join(SKILLS_DIR_RELATIVE))
        .map_err(|e| AppError::internal(format!("解析 skills 目录失败: {e}")))
}

fn is_single_normal_path_component(value: &str) -> bool {
    let path = Path::new(value);
    let mut components = path.components();
    matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(component)), None)
            if component == path.as_os_str()
    )
}

fn resolve_skill_manifest_path(skill_dir: &Path) -> ApiResult<PathBuf> {
    let canonical_skill_dir = skill_dir
        .canonicalize()
        .map_err(|_| AppError::not_found("skill 目录不存在。"))?;
    let manifest = skill_dir
        .join("SKILL.md")
        .canonicalize()
        .map_err(|_| AppError::not_found("skill 不存在或无 SKILL.md。"))?;
    if !manifest.starts_with(&canonical_skill_dir) || !manifest.is_file() {
        return Err(AppError::bad_request("SKILL.md 必须位于 skill 目录内。"));
    }
    Ok(manifest)
}

fn resolve_skill_dir_from_roots(
    primary_dir: &Path,
    extra_dirs: &[String],
    disabled_skills: &[String],
    skill_name: &str,
) -> ApiResult<PathBuf> {
    if !is_single_normal_path_component(skill_name) {
        return Err(AppError::bad_request("skill_name 必须是单个目录名。"));
    }
    if disabled_skills.iter().any(|name| name == skill_name) {
        return Err(AppError::bad_request(format!("skill `{skill_name}` 已禁用。")));
    }

    let roots =
        std::iter::once(primary_dir.to_path_buf()).chain(extra_dirs.iter().map(PathBuf::from));
    for root in roots {
        let Ok(canonical_root) = root.canonicalize() else {
            continue;
        };
        if !canonical_root.is_dir() {
            continue;
        }
        let Ok(skill_dir) = canonical_root.join(skill_name).canonicalize() else {
            continue;
        };
        if !skill_dir.is_dir() || !skill_dir.starts_with(&canonical_root) {
            continue;
        }
        if resolve_skill_manifest_path(&skill_dir).is_ok() {
            return Ok(skill_dir);
        }
    }

    Err(AppError::not_found(format!("skill `{skill_name}` 不存在或无 SKILL.md。")))
}

fn resolve_skill_script_path(skill_dir: &Path, script_path: &str) -> ApiResult<PathBuf> {
    let relative = Path::new(script_path);
    let mut components = relative.components();
    let starts_in_scripts = matches!(
        components.next(),
        Some(std::path::Component::Normal(component)) if component == "scripts"
    );
    if relative.is_absolute()
        || !starts_in_scripts
        || components.any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(AppError::bad_request(
            "script_path 必须是 skill 的 scripts 目录内的相对路径。",
        ));
    }

    let canonical_skill_dir = skill_dir
        .canonicalize()
        .map_err(|_| AppError::not_found("skill 目录不存在。"))?;
    let scripts_dir = canonical_skill_dir
        .join("scripts")
        .canonicalize()
        .map_err(|_| AppError::not_found("skill 不包含 scripts 目录。"))?;
    if !scripts_dir.is_dir() || !scripts_dir.starts_with(&canonical_skill_dir) {
        return Err(AppError::bad_request("skill 的 scripts 目录无效。"));
    }
    let script = canonical_skill_dir
        .join(relative)
        .canonicalize()
        .map_err(|_| AppError::not_found(format!("脚本 `{script_path}` 不存在。")))?;
    if !script.starts_with(&scripts_dir) || !script.is_file() {
        return Err(AppError::bad_request("脚本必须是 skill 的 scripts 目录内的文件。"));
    }
    Ok(script)
}

async fn tool_list_skills(state: &AppState) -> ApiResult<Value> {
    let skills = scan_all_skills(&state).await?;
    Ok(json!({"skills": skills}))
}

/// Scan both the user's ~/.codex/skills and any configured extra dirs.
/// Filters out disabled skills. Each skill entry includes `source` and `disabled`.
async fn scan_all_skills(state: &AppState) -> ApiResult<Vec<Value>> {
    let user = state.workspace_settings.terminal_user();
    let primary_dir = skills_dir_for_user(&user)?;
    let agent_config = state.agent_config.get().await;
    let disabled: Vec<String> = agent_config.disabled_skills.clone();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut skills: Vec<Value> = Vec::new();

    // Primary skills dir
    if primary_dir.exists() {
        scan_skills_dir(&primary_dir, "user", &disabled, &mut seen_names, &mut skills).await?;
    }
    // Extra skill dirs
    for extra_dir in &agent_config.extra_skill_dirs {
        if extra_dir.trim().is_empty() {
            continue;
        }
        let path = PathBuf::from(extra_dir);
        if path.exists() {
            scan_skills_dir(&path, "extra", &disabled, &mut seen_names, &mut skills).await?;
        }
    }
    let builtin_dir = builtin_skills::root_dir(&state.app_dir);
    if builtin_dir.exists() {
        scan_skills_dir(&builtin_dir, "builtin", &disabled, &mut seen_names, &mut skills).await?;
    }
    // Also include disabled skills as entries so the UI can show them
    for name in &disabled {
        if !seen_names.contains(name) {
            skills.push(json!({
                "name": name,
                "description": "",
                "path": "",
                "source": "disabled",
                "disabled": true,
            }));
        }
    }
    skills.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    Ok(skills)
}

async fn scan_skills_dir(
    dir: &Path,
    source: &str,
    disabled: &[String],
    seen_names: &mut std::collections::HashSet<String>,
    skills: &mut Vec<Value>,
) -> ApiResult<()> {
    let mut entries = fs::read_dir(dir).await.map_err(|e| {
        AppError::internal(format!("读取 skills 目录失败 ({}) : {e}", dir.display()))
    })?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::internal(format!("读取目录项失败: {e}")))?
    {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if seen_names.contains(&name) {
            continue;
        }
        seen_names.insert(name.clone());
        let content = fs::read_to_string(&skill_md).await.unwrap_or_default();
        let description = parse_skill_description(&content);
        let is_disabled = disabled.contains(&name);
        skills.push(json!({
            "name": name,
            "description": description,
            "path": path.to_string_lossy(),
            "source": source,
            "disabled": is_disabled,
        }));
    }
    Ok(())
}

/// Parse YAML front-matter `description:` (or `name:`) from SKILL.md.
fn parse_skill_description(content: &str) -> String {
    if let Some(desc) = parse_yaml_front_matter_field(content, "description") {
        return desc;
    }
    if let Some(desc) = parse_yaml_front_matter_field(content, "name") {
        return desc;
    }
    // Fallback: first non-empty paragraph after front-matter + headings
    let mut in_front_matter = false;
    let mut after_front_matter = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !after_front_matter {
                in_front_matter = !in_front_matter;
                if !in_front_matter {
                    after_front_matter = true;
                }
            }
            continue;
        }
        if in_front_matter || !after_front_matter {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return trimmed.to_string();
    }
    String::new()
}

fn parse_yaml_front_matter_field(content: &str, field: &str) -> Option<String> {
    let mut lines = content.lines();
    let first = lines.next()?.trim();
    if first != "---" {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix(&format!("{field}:")) {
            let value = rest.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

async fn tool_read_skill(state: &AppState, skill_name: &str) -> ApiResult<Value> {
    let user = state.workspace_settings.terminal_user();
    let primary_dir = skills_dir_for_user(&user)?;
    let config = state.agent_config.get().await;
    let mut extra_dirs = config.extra_skill_dirs.clone();
    extra_dirs.push(
        builtin_skills::root_dir(&state.app_dir)
            .to_string_lossy()
            .into_owned(),
    );
    let skill_dir = resolve_skill_dir_from_roots(
        &primary_dir,
        &extra_dirs,
        &config.disabled_skills,
        skill_name,
    )?;
    let skill_md = resolve_skill_manifest_path(&skill_dir)?;
    let content = fs::read_to_string(&skill_md)
        .await
        .map_err(|e| AppError::internal(format!("读取 SKILL.md 失败: {e}")))?;
    Ok(json!({
        "skill_name": skill_name,
        "path": skill_dir.to_string_lossy(),
        "content": content,
    }))
}

async fn tool_run_skill_script(
    state: &AppState,
    skill_name: &str,
    script_path: &str,
    args: &[String],
    cwd_override: Option<&str>,
) -> ApiResult<Value> {
    let user = state.workspace_settings.terminal_user();
    let primary_dir = skills_dir_for_user(&user)?;
    let config = state.agent_config.get().await;
    let mut extra_dirs = config.extra_skill_dirs.clone();
    extra_dirs.push(
        builtin_skills::root_dir(&state.app_dir)
            .to_string_lossy()
            .into_owned(),
    );
    let skill_dir = resolve_skill_dir_from_roots(
        &primary_dir,
        &extra_dirs,
        &config.disabled_skills,
        skill_name,
    )?;
    let script = resolve_skill_script_path(&skill_dir, script_path)?;
    let cwd = resolve_execution_cwd(state, &skill_dir, cwd_override, Some(skill_name)).await?;
    execute_script(&script, args, &cwd).await
}

async fn resolve_execution_cwd(
    state: &AppState,
    skill_dir: &Path,
    cwd_override: Option<&str>,
    skill_name: Option<&str>,
) -> ApiResult<PathBuf> {
    if let Some(cwd) = cwd_override
        && !cwd.is_empty()
    {
        let path = PathBuf::from(cwd);
        if path.is_absolute() {
            return path
                .canonicalize()
                .map_err(|e| AppError::bad_request(format!("工作目录无效: {e}")));
        }
    }
    // Infer project directory from SKILL.md content (e.g. "Run from `/home/codes/...`")
    if let Some(_skill_name) = skill_name {
        let skill_md = skill_dir.join("SKILL.md");
        if let Ok(content) = fs::read_to_string(&skill_md).await
            && let Some(project_dir) = infer_project_dir_from_skill(&content)
            && PathBuf::from(&project_dir).exists()
        {
            return Ok(PathBuf::from(project_dir));
        }
    }
    Ok(state.workspace_root())
}

/// Extract a project path like `/home/codes/stockScreener` from skill text
/// patterns such as "Run this only from `/path`".
fn infer_project_dir_from_skill(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(idx) = trimmed.find("`/") {
            if let Some(end) = trimmed[idx + 1..].find('`') {
                let path = &trimmed[idx + 1..idx + 1 + end];
                if path.starts_with('/') && path.split('/').count() >= 3 {
                    return Some(path.to_string());
                }
            }
        }
    }
    None
}

fn truncate_output(output: &str) -> String {
    if output.len() <= MAX_SKILL_OUTPUT_BYTES {
        return output.to_string();
    }
    let half = MAX_SKILL_OUTPUT_BYTES / 2;
    format!(
        "{}\n\n... [output truncated, {} bytes omitted] ...\n\n{}",
        &output[..half],
        output.len() - MAX_SKILL_OUTPUT_BYTES,
        &output[output.len() - half..]
    )
}

async fn execute_script(script: &Path, args: &[String], cwd: &Path) -> ApiResult<Value> {
    let script_str = script.to_string_lossy().to_string();
    let (program, mut cmd_args) = if script_str.ends_with(".py") {
        ("python3".to_string(), vec![script_str])
    } else if script_str.ends_with(".sh") {
        ("bash".to_string(), vec![script_str])
    } else {
        (script_str, vec![])
    };
    cmd_args.extend(args.iter().cloned());
    let mut cmd = Command::new(&program);
    cmd.args(&cmd_args);
    cmd.current_dir(cwd);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::internal(format!("执行脚本失败: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok(json!({
        "exit_code": output.status.code().unwrap_or(-1),
        "stdout": truncate_output(&stdout),
        "stderr": truncate_output(&stderr),
        "cwd": cwd.to_string_lossy(),
        "command": format!("{} {}", program, cmd_args.join(" ")),
    }))
}

// ---------------------------------------------------------------------------
// Tool dispatch
// ---------------------------------------------------------------------------

fn session_tool_cwd<'a>(session: &'a AgentSession, arguments: &'a Value) -> Option<&'a str> {
    arguments
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|cwd| !cwd.trim().is_empty())
        .or_else(|| (!session.cwd.is_empty()).then_some(session.cwd.as_str()))
}

fn session_tool_path<'a>(session: &'a AgentSession, arguments: &'a Value) -> Option<&'a str> {
    arguments
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .or_else(|| (!session.cwd.is_empty()).then_some(session.cwd.as_str()))
}

fn normalize_sandbox_mode(mode: &str) -> &str {
    match mode.trim() {
        "read_only" | "full_access" => mode.trim(),
        _ => "default",
    }
}

fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "list_skills"
            | "read_skill"
            | "list_files"
            | "search_files"
            | "read_file"
            | "git_diff"
            | "list_checkpoints"
            | "read_background_command"
            | "list_mcp_tools"
            | "web_search"
            | "web_fetch"
            | "view_image"
    )
}

fn normalize_command_text(command: &str) -> String {
    command
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_dangerous_command(tool: &str, command: &str) -> bool {
    if !matches!(tool, "run_command" | "run_verification" | "start_background_command") {
        return false;
    }
    let normalized = normalize_command_text(command);
    let patterns = [
        "rm -rf",
        "rm -fr",
        "rm -r -f",
        "rm --recursive --force",
        "rm --force --recursive",
        "mkfs.",
        "mkfs ",
        "fdisk",
        "dd if=",
        "dd of=",
        "shutdown",
        "reboot",
        "halt",
        "poweroff",
        "git reset --hard",
        "git reset -hard",
        "git clean -f",
        "git checkout --",
        "git checkout .",
        "git stash drop",
        "git push --force",
        "git push -f",
        "git branch -d",
        "chmod -r 777 /",
        "chmod -r 0 /",
        "chown -r root",
        ":(){",
        "fork bomb",
        "\"$(curl",
        "\"$(wget",
        "<(curl",
        "<(wget",
    ];
    if patterns.iter().any(|pattern| normalized.contains(pattern)) {
        return true;
    }
    let pipe_to_shell = [
        "| sh",
        "| bash",
        "| zsh",
        "| fish",
        "| pwsh",
        "| powershell",
    ];
    let downloads_code =
        normalized.contains("curl") || normalized.contains("wget") || normalized.contains("eval");
    downloads_code && pipe_to_shell.iter().any(|pipe| normalized.contains(pipe))
}

async fn sandbox_block_write(
    _state: &AppState,
    session: &AgentSession,
    tool: &str,
) -> ApiResult<()> {
    if normalize_sandbox_mode(&session.sandbox_mode) == "read_only" && !is_read_only_tool(tool) {
        return Err(AppError::bad_request(format!(
            "只读沙箱禁止执行工具 `{tool}`。如需执行，请切换为默认或全开沙箱。"
        )));
    }
    Ok(())
}

async fn sandbox_check_command(
    state: &AppState,
    session: &AgentSession,
    tool: &str,
    command: &str,
) -> ApiResult<()> {
    let mode = normalize_sandbox_mode(&session.sandbox_mode);
    if mode == "read_only" {
        return Err(AppError::bad_request(format!(
            "只读沙箱禁止执行命令。如需执行，请切换为默认或全开沙箱。"
        )));
    }
    if mode == "default" && is_dangerous_command(tool, command) {
        if session.approval_policy == "allow_all" {
            return Ok(());
        }
        if state
            .agent_manager
            .is_approved(&session.id, tool, command)
            .await
        {
            return Ok(());
        }
        let approval = match state
            .agent_manager
            .find_pending_approval(&session.id, tool, command)
            .await
        {
            Some(existing) => existing,
            None => {
                state
                    .agent_manager
                    .request_approval(&session.id, tool, command)
                    .await
            }
        };
        return Err(AppError::bad_request(format!(
            "命令需要人工批准（批准编号 {}）。请打开页面顶部的「待批准」，允许后会自动重试上一轮。\n命令：{}",
            approval.id, command
        )));
    }
    Ok(())
}

async fn sandbox_check_restore(
    state: &AppState,
    session: &AgentSession,
    tool: &str,
    summary: &str,
) -> ApiResult<()> {
    let mode = normalize_sandbox_mode(&session.sandbox_mode);
    if mode == "read_only" {
        return Err(AppError::bad_request("只读沙箱禁止恢复检查点。请切换为默认或全开沙箱。"));
    }
    if mode == "default" {
        if session.approval_policy == "allow_all" {
            return Ok(());
        }
        if state
            .agent_manager
            .is_approved(&session.id, tool, summary)
            .await
        {
            return Ok(());
        }
        let approval = match state
            .agent_manager
            .find_pending_approval(&session.id, tool, summary)
            .await
        {
            Some(existing) => existing,
            None => {
                state
                    .agent_manager
                    .request_approval(&session.id, tool, summary)
                    .await
            }
        };
        return Err(AppError::bad_request(format!(
            "恢复检查点需要人工批准（批准编号 {}）。请打开页面顶部的「待批准」，允许后会自动重试上一轮。\n检查点：{}",
            approval.id, summary
        )));
    }
    Ok(())
}

async fn execute_tool(
    state: &AppState,
    session: &AgentSession,
    name: &str,
    arguments: &Value,
) -> ApiResult<Value> {
    match name {
        "list_skills" => tool_list_skills(state).await,
        "read_skill" => {
            let skill_name = arguments
                .get("skill_name")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 skill_name 参数"))?;
            tool_read_skill(state, skill_name).await
        }
        "run_skill_script" => {
            sandbox_block_write(state, session, "run_skill_script").await?;
            let skill_name = arguments
                .get("skill_name")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 skill_name 参数"))?;
            let script_path = arguments
                .get("script_path")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 script_path 参数"))?;
            let args: Vec<String> = arguments
                .get("args")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let cwd = session_tool_cwd(session, arguments);
            tool_run_skill_script(state, skill_name, script_path, &args, cwd).await
        }
        "run_command" => {
            let command = arguments
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 command 参数"))?;
            sandbox_check_command(state, session, "run_command", command).await?;
            let cwd = session_tool_cwd(session, arguments);
            let timeout_secs = arguments.get("timeout_secs").and_then(Value::as_u64);
            engineering_tools::run_command(state, command, cwd, timeout_secs).await
        }
        "start_background_command" => {
            let command = arguments
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 command 参数"))?;
            sandbox_check_command(state, session, "start_background_command", command).await?;
            let cwd = engineering_tools::resolve_cwd(state, session_tool_cwd(session, arguments))?;
            let command_session = state
                .agent_manager
                .background_commands
                .start(
                    &session.id,
                    command,
                    &cwd,
                    arguments
                        .get("rows")
                        .and_then(Value::as_u64)
                        .map(|value| value as u16),
                    arguments
                        .get("cols")
                        .and_then(Value::as_u64)
                        .map(|value| value as u16),
                )
                .await?;
            Ok(json!(command_session))
        }
        "read_background_command" => {
            let command_id = arguments
                .get("command_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 command_id 参数"))?;
            let command_session = state
                .agent_manager
                .background_commands
                .get(&session.id, command_id)
                .await?;
            Ok(json!(command_session))
        }
        "write_background_command" => {
            sandbox_block_write(state, session, "write_background_command").await?;
            let command_id = arguments
                .get("command_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 command_id 参数"))?;
            let input = arguments
                .get("input")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 input 参数"))?;
            let command_session = state
                .agent_manager
                .background_commands
                .write_stdin(&session.id, command_id, input)
                .await?;
            Ok(json!(command_session))
        }
        "terminate_background_command" => {
            sandbox_block_write(state, session, "terminate_background_command").await?;
            let command_id = arguments
                .get("command_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 command_id 参数"))?;
            let command_session = state
                .agent_manager
                .background_commands
                .terminate(&session.id, command_id)
                .await?;
            Ok(json!(command_session))
        }
        "list_files" => {
            engineering_tools::list_files(
                state,
                session_tool_path(session, arguments),
                arguments.get("max_results").and_then(Value::as_u64),
            )
            .await
        }
        "search_files" => {
            let query = arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 query 参数"))?;
            engineering_tools::search_files(
                state,
                query,
                session_tool_path(session, arguments),
                arguments.get("glob").and_then(Value::as_str),
                arguments.get("max_results").and_then(Value::as_u64),
            )
            .await
        }
        "read_file" => {
            let path = arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 path 参数"))?;
            engineering_tools::read_file(
                state,
                path,
                (!session.cwd.is_empty()).then_some(session.cwd.as_str()),
                arguments.get("start_line").and_then(Value::as_u64),
                arguments.get("line_count").and_then(Value::as_u64),
            )
            .await
        }
        "apply_patch" => {
            sandbox_block_write(state, session, "apply_patch").await?;
            let patch = arguments
                .get("patch")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 patch 参数"))?;
            engineering_tools::apply_patch(state, patch, session_tool_cwd(session, arguments)).await
        }
        "git_diff" => {
            engineering_tools::git_diff(
                state,
                session_tool_cwd(session, arguments),
                arguments.get("path").and_then(Value::as_str),
            )
            .await
        }
        "create_checkpoint" => {
            sandbox_block_write(state, session, "create_checkpoint").await?;
            engineering_tools::create_checkpoint(state, session_tool_cwd(session, arguments)).await
        }
        "list_checkpoints" => {
            engineering_tools::list_checkpoints(state, session_tool_cwd(session, arguments)).await
        }
        "restore_checkpoint" => {
            let checkpoint_id = arguments
                .get("checkpoint_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 checkpoint_id 参数"))?;
            sandbox_check_restore(state, session, "restore_checkpoint", checkpoint_id).await?;
            engineering_tools::restore_checkpoint(
                state,
                session_tool_cwd(session, arguments),
                checkpoint_id,
            )
            .await
        }
        "run_verification" => {
            let command = arguments
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 command 参数"))?;
            sandbox_check_command(state, session, "run_verification", command).await?;
            engineering_tools::run_verification(
                state,
                command,
                session_tool_cwd(session, arguments),
                arguments.get("timeout_secs").and_then(Value::as_u64),
            )
            .await
        }
        "list_mcp_tools" => extended_tools::list_mcp_tools(state).await,
        "call_mcp_tool" => {
            sandbox_block_write(state, session, "call_mcp_tool").await?;
            let server = arguments
                .get("server")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 server 参数"))?;
            let tool = arguments
                .get("tool")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 tool 参数"))?;
            extended_tools::call_mcp_tool(
                state,
                server,
                tool,
                arguments
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            )
            .await
        }
        "web_search" => {
            let query = arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 query 参数"))?;
            extended_tools::web_search(query, arguments.get("max_chars").and_then(Value::as_u64))
                .await
        }
        "web_fetch" => {
            let url = arguments
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 url 参数"))?;
            extended_tools::web_fetch(url, arguments.get("max_chars").and_then(Value::as_u64)).await
        }
        "view_image" => {
            let path = arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::bad_request("缺少 path 参数"))?;
            let image_path = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                engineering_tools::resolve_cwd(
                    state,
                    (!session.cwd.is_empty()).then_some(session.cwd.as_str()),
                )?
                .join(path)
            };
            extended_tools::view_image(&image_path).await
        }
        "run_browser_actions" => {
            sandbox_block_write(state, session, "run_browser_actions").await?;
            let cwd = engineering_tools::resolve_cwd(state, session_tool_cwd(session, arguments))?;
            let actions = arguments
                .get("actions")
                .and_then(Value::as_array)
                .ok_or_else(|| AppError::bad_request("缺少 actions 参数"))?;
            extended_tools::run_browser_actions(
                &cwd,
                arguments.get("url").and_then(Value::as_str),
                actions,
                arguments.get("timeout_secs").and_then(Value::as_u64),
            )
            .await
        }
        _ => Err(AppError::bad_request(format!("未知工具: {name}"))),
    }
}

// ---------------------------------------------------------------------------
// LLM chat loop helpers
// ---------------------------------------------------------------------------

async fn load_hierarchical_agent_instructions(
    state: &AppState,
    session: &AgentSession,
) -> Vec<String> {
    let mut candidates = Vec::new();
    let user = state.workspace_settings.terminal_user();
    if let Ok(home) = runtime_paths::resolve_user_home_preferring_env(&user) {
        candidates.push(home.join(".codex/AGENTS.md"));
    }
    let cwd = PathBuf::from(&session.cwd);
    let canonical_cwd = cwd.canonicalize().unwrap_or(cwd);
    let mut ancestors = canonical_cwd
        .ancestors()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    ancestors.reverse();
    candidates.extend(ancestors.into_iter().map(|path| path.join("AGENTS.md")));

    let mut loaded = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for candidate in candidates {
        let identity = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if !seen.insert(identity) {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&candidate).await
            && !content.trim().is_empty()
        {
            loaded.push(format!("{}\n{}", candidate.display(), content.trim()));
        }
    }
    loaded
}

fn build_api_messages(session: &AgentSession, agent_instructions: &[String]) -> Vec<Value> {
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

fn estimate_text_tokens(text: &str) -> u64 {
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

fn estimate_json_tokens(value: &Value) -> u64 {
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

fn context_status(session: &AgentSession, credential: &LlmCredential) -> AgentContextStatus {
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

fn compact_split_index(messages: &[AgentMessage]) -> Option<usize> {
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

fn parse_compact_reply(reply: &str) -> (String, Vec<String>) {
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

async fn compact_session_history(
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

fn is_skill_name_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '-' | '_')
}

fn extract_explicit_skill_invocations(message: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (offset, _) in message.match_indices('$') {
        if message[..offset]
            .chars()
            .next_back()
            .is_some_and(is_skill_name_character)
        {
            continue;
        }
        let name = message[offset + 1..]
            .chars()
            .take_while(|character| is_skill_name_character(*character))
            .collect::<String>();
        if name.is_empty()
            || !name
                .chars()
                .any(|character| character.is_alphabetic() || character == '_')
            || names.contains(&name)
        {
            continue;
        }
        names.push(name);
    }
    names
}

fn skill_invocation_tool_call(skill_name: &str, ordinal: usize) -> ToolCall {
    ToolCall {
        id: format!("skill-invocation-{ordinal}-{}", generate_id()),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "read_skill".to_string(),
            arguments: json!({"skill_name": skill_name}).to_string(),
        },
    }
}

async fn resolve_explicit_skill_invocations(
    state: &AppState,
    message: &str,
) -> ApiResult<Vec<String>> {
    let requested = extract_explicit_skill_invocations(message);
    if requested.is_empty() {
        return Ok(Vec::new());
    }
    let available = scan_all_skills(state).await?;
    let mut resolved = Vec::with_capacity(requested.len());
    for requested_name in requested {
        let normalized = requested_name.to_lowercase();
        let skill = available.iter().find(|skill| {
            skill["name"]
                .as_str()
                .is_some_and(|name| name.to_lowercase() == normalized)
        });
        let Some(skill) = skill else {
            return Err(AppError::bad_request(format!(
                "Skill `${requested_name}` 不存在；请在输入框使用 $ 按钮查看完整列表。"
            )));
        };
        if skill["disabled"].as_bool().unwrap_or(false) {
            return Err(AppError::bad_request(format!(
                "Skill `${requested_name}` 已禁用，请先在 Agent 设置中启用。"
            )));
        }
        if let Some(name) = skill["name"].as_str()
            && !resolved.iter().any(|existing| existing == name)
        {
            resolved.push(name.to_string());
        }
    }
    Ok(resolved)
}

struct LlmChoice {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
    #[allow(dead_code)]
    finish_reason: String,
    usage: Option<llm::LlmTokenUsage>,
    cancelled: bool,
    streamed: bool,
}

fn is_retryable_llm_error(error: &llm::LlmCallError) -> bool {
    match error.status {
        None => true,
        Some(status) => matches!(status, 408 | 429 | 500 | 502 | 503 | 504),
    }
}

async fn call_llm(
    client: &reqwest::Client,
    credential: &LlmCredential,
    messages: Vec<Value>,
    tx: &mpsc::Sender<AgentEvent>,
    cancel: &mut watch::Receiver<bool>,
) -> ApiResult<LlmChoice> {
    let mut attempt = 0u8;
    loop {
        attempt += 1;
        let mut cancel_attempt = cancel.clone();
        match call_llm_once(client, credential, messages.clone(), tx, &mut cancel_attempt).await {
            Ok(choice) => return Ok(choice),
            Err(error) if attempt <= LLM_RETRY_ATTEMPTS && is_retryable_llm_error(&error) => {
                let delay = LLM_RETRY_DELAYS_SECS
                    .get(attempt.saturating_sub(1) as usize)
                    .copied()
                    .unwrap_or(3);
                let _ = tx
                    .send(AgentEvent::Notice {
                        message: format!(
                            "LLM 请求失败（{}/{} 次后重试，{} 秒后）：{}",
                            attempt, LLM_RETRY_ATTEMPTS, delay, error.message
                        ),
                    })
                    .await;
                let mut cancel_sleep = cancel.clone();
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {}
                    changed = cancel_sleep.changed() => {
                        if changed.is_err() || *cancel_sleep.borrow() {
                            return Err(AppError::internal(error.message));
                        }
                    }
                }
            }
            Err(error) => return Err(AppError::internal(error.message)),
        }
    }
}

async fn call_llm_once(
    client: &reqwest::Client,
    credential: &LlmCredential,
    messages: Vec<Value>,
    tx: &mpsc::Sender<AgentEvent>,
    cancel: &mut watch::Receiver<bool>,
) -> Result<LlmChoice, llm::LlmCallError> {
    let (stream_tx, mut stream_rx) = mpsc::channel(64);
    let request = llm::call_conversation_stream(
        client,
        &credential.target,
        &credential.model,
        messages,
        tool_definitions(),
        stream_tx,
    );
    tokio::pin!(request);
    let mut streamed_content = String::new();
    let mut streamed_reasoning = String::new();
    let mut received_delta = false;
    let reply = loop {
        tokio::select! {
            result = &mut request => {
                break result?;
            }
            event = stream_rx.recv() => {
                match event {
                    Some(llm::ConversationStreamEvent::TextDelta(delta)) => {
                        received_delta = true;
                        streamed_content.push_str(&delta);
                        // 客户端断开时忽略事件发送失败，任务继续在服务端运行。
                        let _ = tx.send(AgentEvent::AssistantDelta { content: delta }).await;
                    }
                    Some(llm::ConversationStreamEvent::ReasoningDelta(delta)) => {
                        received_delta = true;
                        streamed_reasoning.push_str(&delta);
                        let _ = tx
                            .send(AgentEvent::ReasoningDelta { content: delta })
                            .await;
                    }
                    Some(llm::ConversationStreamEvent::Completed(reply)) => drop(reply),
                    None => {}
                }
            }
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return Ok(LlmChoice {
                        content: (!streamed_content.is_empty()).then_some(streamed_content),
                        reasoning_content: (!streamed_reasoning.is_empty())
                            .then_some(streamed_reasoning),
                        tool_calls: None,
                        finish_reason: "cancelled".to_string(),
                        usage: None,
                        cancelled: true,
                        streamed: received_delta,
                    });
                }
            }
        }
    };
    let tool_calls = reply.tool_calls.map(|items| {
        items
            .iter()
            .filter_map(|tool_call| {
                let id = tool_call.get("id")?.as_str()?.to_string();
                let function = tool_call.get("function")?;
                let name = function.get("name")?.as_str()?.to_string();
                let arguments = function.get("arguments")?.as_str()?.to_string();
                Some(ToolCall {
                    id,
                    call_type: "function".to_string(),
                    function: ToolFunction { name, arguments },
                })
            })
            .collect()
    });
    Ok(LlmChoice {
        content: reply.content,
        reasoning_content: reply.reasoning_content,
        tool_calls,
        finish_reason: reply.finish_reason,
        usage: reply.usage,
        cancelled: false,
        streamed: received_delta,
    })
}

// ---------------------------------------------------------------------------
// SSE events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum AgentEvent {
    #[serde(rename = "assistant_delta")]
    AssistantDelta { content: String },
    #[serde(rename = "reasoning_delta")]
    ReasoningDelta { content: String },
    #[serde(rename = "assistant_message")]
    AssistantMessage { content: String },
    #[serde(rename = "notice")]
    Notice { message: String },
    #[serde(rename = "tool_call_start")]
    ToolCallStart {
        id: String,
        name: String,
        arguments: String,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        id: String,
        name: String,
        result: Value,
        is_error: bool,
    },
    #[serde(rename = "context_status")]
    ContextStatus { status: AgentContextStatus },
    #[serde(rename = "compact_start")]
    CompactStart { automatic: bool },
    #[serde(rename = "compact_done")]
    CompactDone {
        automatic: bool,
        compacted: bool,
        removed_messages: u64,
        status: AgentContextStatus,
    },
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "stopped")]
    Stopped,
    #[serde(rename = "error")]
    Error { message: String },
}

fn agent_event_to_sse(event: &AgentEvent) -> Result<Event, std::convert::Infallible> {
    let json_str = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
    Ok(Event::default().data(json_str))
}

// ---------------------------------------------------------------------------
// Chat handler with agentic loop (SSE stream via channel)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub attachments: Vec<AgentImageAttachment>,
}

#[derive(Debug, Deserialize)]
pub struct AgentImageAttachment {
    #[serde(default)]
    pub name: String,
    pub mime_type: String,
    pub data_url: String,
}

fn user_message_content(message: &str, attachments: &[AgentImageAttachment]) -> ApiResult<Value> {
    if attachments.is_empty() {
        return Ok(Value::String(message.to_string()));
    }
    if attachments.len() > 8 {
        return Err(AppError::bad_request("每条消息最多附加 8 张图片。"));
    }
    let mut content = vec![json!({"type": "text", "text": message})];
    for attachment in attachments {
        if !attachment.mime_type.starts_with("image/")
            || !attachment
                .data_url
                .starts_with(&format!("data:{};base64,", attachment.mime_type))
            || attachment.data_url.len() > 16 * 1024 * 1024
        {
            return Err(AppError::bad_request(format!(
                "图片附件 `{}` 格式无效或超过大小限制。",
                attachment.name
            )));
        }
        content.push(json!({
            "type": "image_url",
            "image_url": {"url": attachment.data_url},
            "name": attachment.name,
        }));
    }
    Ok(Value::Array(content))
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_preset_id: String,
    #[serde(default)]
    pub profile_id: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionRequest {
    pub title: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub api_preset_id: Option<String>,
    #[serde(default)]
    pub sandbox_mode: Option<String>,
    #[serde(default)]
    pub approval_policy: Option<String>,
}

/// Convert an mpsc::Receiver<AgentEvent> into an SSE-compatible async stream.
fn receiver_to_event_stream(
    rx: mpsc::Receiver<AgentEvent>,
) -> impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>> + Send {
    futures_util::StreamExt::fuse(futures_util::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Some(event) => Some((agent_event_to_sse(&event), rx)),
            None => None,
        }
    }))
}

pub async fn chat(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<ChatRequest>,
) -> ApiResult<Response> {
    let user_message = payload.message.trim();
    if user_message.is_empty() && payload.attachments.is_empty() {
        return Err(AppError::bad_request("消息不能为空"));
    }
    let message_content = user_message_content(user_message, &payload.attachments)?;
    let explicit_skills = resolve_explicit_skill_invocations(&state, user_message).await?;
    let session = state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let credential = resolve_llm_credential(&state, &session.model, &session.api_preset_id).await?;
    let environment = llm::environment::LlmHttpEnvironment::capture(
        &state.proxy_manager,
        &state.workspace_settings,
    )
    .await?;
    let http_context = environment.context_for(
        &credential.terminal_env,
        Duration::from_secs(credential.context_limits.llm_timeout_secs),
    )?;
    let queued_message = AgentMessage {
        role: "user".to_string(),
        content: Some(message_content),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
        created_at: Some(current_timestamp()),
    };
    let (run_id, cancel, queued_messages) =
        match state.agent_manager.begin_chat_run(&session_id).await {
            Ok(run) => run,
            Err(_) => {
                if state
                    .agent_manager
                    .queue_chat_message(&session_id, queued_message)
                    .await
                {
                    return Ok(Json(json!({"queued": true})).into_response());
                }
                return Err(AppError::bad_request("当前会话已有一轮对话正在运行。"));
            }
        };
    state
        .agent_manager
        .append_messages(&session_id, vec![queued_message])
        .await;

    let (tx, rx) = mpsc::channel::<AgentEvent>(64);
    let state_clone = state.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        run_agent_loop(
            state_clone.clone(),
            session_id_clone.clone(),
            credential,
            http_context.client,
            explicit_skills,
            tx,
            cancel,
            queued_messages,
        )
        .await;
        state_clone
            .agent_manager
            .finish_chat_run(&session_id_clone, &run_id)
            .await;
    });

    let stream = receiver_to_event_stream(rx);
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

pub async fn stop_chat(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let stopped = state.agent_manager.cancel_chat_run(&session_id).await;
    Ok(Json(json!({"stopped": stopped})))
}

async fn preload_explicit_skills(
    state: &AppState,
    session_id: &str,
    skill_names: &[String],
    tx: &mpsc::Sender<AgentEvent>,
) -> bool {
    for (ordinal, skill_name) in skill_names.iter().enumerate() {
        let tool_call = skill_invocation_tool_call(skill_name, ordinal);
        let _ = tx
            .send(AgentEvent::ToolCallStart {
                id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                arguments: tool_call.function.arguments.clone(),
            })
            .await;
        let (result, is_error, error_message) = match tool_read_skill(state, skill_name).await {
            Ok(result) => (result, false, None),
            Err(error) => {
                let message = error.message;
                (json!({"error": message.clone()}), true, Some(message))
            }
        };
        state
            .agent_manager
            .append_messages(
                session_id,
                vec![
                    AgentMessage {
                        role: "assistant".to_string(),
                        content: None,
                        reasoning_content: None,
                        tool_calls: Some(vec![tool_call.clone()]),
                        tool_call_id: None,
                        name: None,
                        created_at: Some(current_timestamp()),
                    },
                    AgentMessage {
                        role: "tool".to_string(),
                        content: Some(result.clone()),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(tool_call.id.clone()),
                        name: Some(tool_call.function.name.clone()),
                        created_at: Some(current_timestamp()),
                    },
                ],
            )
            .await;
        let _ = tx
            .send(AgentEvent::ToolResult {
                id: tool_call.id,
                name: tool_call.function.name,
                result,
                is_error,
            })
            .await;
        if let Some(message) = error_message {
            let _ = tx.send(AgentEvent::Error { message }).await;
            return false;
        }
    }
    true
}

async fn run_agent_loop(
    state: AppState,
    session_id: String,
    credential: LlmCredential,
    client: reqwest::Client,
    explicit_skills: Vec<String>,
    tx: mpsc::Sender<AgentEvent>,
    mut cancel: watch::Receiver<bool>,
    mut queued_messages: mpsc::UnboundedReceiver<AgentMessage>,
) {
    if !preload_explicit_skills(&state, &session_id, &explicit_skills, &tx).await {
        return;
    }
    let mut iterations = 0u8;
    let mut auto_compact_attempted = false;
    let initial_session = match state.agent_manager.get_session(&session_id).await {
        Some(session) => session,
        None => return,
    };
    let agent_instructions = load_hierarchical_agent_instructions(&state, &initial_session).await;
    'agent_loop: loop {
        iterations += 1;
        if iterations > MAX_TOOL_ITERATIONS {
            let _ = tx
                .send(AgentEvent::Error {
                    message: format!("达到最大工具调用次数限制 ({MAX_TOOL_ITERATIONS})。"),
                })
                .await;
            break;
        }
        let mut session = match state.agent_manager.get_session(&session_id).await {
            Some(s) => s,
            None => {
                let _ = tx
                    .send(AgentEvent::Error {
                        message: "会话已不存在。".to_string(),
                    })
                    .await;
                break;
            }
        };
        let mut queued = Vec::new();
        while let Ok(message) = queued_messages.try_recv() {
            queued.push(message);
        }
        if !queued.is_empty() {
            state
                .agent_manager
                .append_messages(&session_id, queued)
                .await;
            session = match state.agent_manager.get_session(&session_id).await {
                Some(session) => session,
                None => break,
            };
        }
        let status = context_status(&session, &credential);
        let _ = tx
            .send(AgentEvent::ContextStatus {
                status: status.clone(),
            })
            .await;
        if !auto_compact_attempted && status.used_tokens >= status.compact_threshold {
            auto_compact_attempted = true;
            let _ = tx.send(AgentEvent::CompactStart { automatic: true }).await;
            match compact_session_history(&state, &session_id, &credential, &client).await {
                Ok(result) => {
                    let _ = tx
                        .send(AgentEvent::CompactDone {
                            automatic: true,
                            compacted: result.compacted,
                            removed_messages: result.removed_messages,
                            status: result.status,
                        })
                        .await;
                    if let Some(updated) = state.agent_manager.get_session(&session_id).await {
                        session = updated;
                    }
                }
                Err(error) => {
                    let _ = tx
                        .send(AgentEvent::Error {
                            message: error.message,
                        })
                        .await;
                    break;
                }
            }
        }
        let messages = build_api_messages(&session, &agent_instructions);
        let choice = match call_llm(&client, &credential, messages, &tx, &mut cancel).await {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AgentEvent::Error { message: e.message }).await;
                break;
            }
        };
        if let Some(usage) = choice.usage.clone() {
            if let Some(updated) = state
                .agent_manager
                .update_token_usage(&session_id, usage)
                .await
            {
                session = updated;
            }
        }
        if choice.cancelled {
            if let Some(content) = choice
                .content
                .as_ref()
                .filter(|content| !content.is_empty())
            {
                state
                    .agent_manager
                    .append_messages(
                        &session_id,
                        vec![AgentMessage {
                            role: "assistant".to_string(),
                            content: Some(Value::String(content.clone())),
                            reasoning_content: None,
                            tool_calls: None,
                            tool_call_id: None,
                            name: None,
                            created_at: Some(current_timestamp()),
                        }],
                    )
                    .await;
            }
            let _ = tx.send(AgentEvent::Stopped).await;
            break;
        }
        if let Some(content) = &choice.content
            && !content.is_empty()
            && !choice.streamed
        {
            let _ = tx
                .send(AgentEvent::AssistantMessage {
                    content: content.clone(),
                })
                .await;
        }
        let assistant_msg = AgentMessage {
            role: "assistant".to_string(),
            content: choice.content.as_ref().and_then(|c| {
                if c.is_empty() {
                    None
                } else {
                    Some(Value::String(c.clone()))
                }
            }),
            reasoning_content: choice.reasoning_content.clone(),
            tool_calls: choice.tool_calls.clone(),
            tool_call_id: None,
            name: None,
            created_at: Some(current_timestamp()),
        };
        state
            .agent_manager
            .append_messages(&session_id, vec![assistant_msg])
            .await;
        // No tool calls → conversation turn complete
        let Some(tool_calls) = &choice.tool_calls else {
            let mut queued = Vec::new();
            while let Ok(message) = queued_messages.try_recv() {
                queued.push(message);
            }
            if !queued.is_empty() {
                state
                    .agent_manager
                    .append_messages(&session_id, queued)
                    .await;
                continue 'agent_loop;
            }
            let _ = tx.send(AgentEvent::Done).await;
            break;
        };
        if tool_calls.is_empty() {
            let _ = tx.send(AgentEvent::Done).await;
            break;
        }
        // Execute each tool call
        for tc in tool_calls {
            let _ = tx
                .send(AgentEvent::ToolCallStart {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                })
                .await;
            let arguments: Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(json!({}));
            let tool_future = execute_tool(&state, &session, &tc.function.name, &arguments);
            tokio::pin!(tool_future);
            let result = tokio::select! {
                result = &mut tool_future => result,
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        let _ = tx.send(AgentEvent::Stopped).await;
                        let result_ids = state
                            .agent_manager
                            .get_session(&session_id)
                            .await
                            .map(|session| {
                                session
                                    .messages
                                    .iter()
                                    .filter(|message| message.role == "tool")
                                    .filter_map(|message| message.tool_call_id.clone())
                                    .collect::<std::collections::HashSet<_>>()
                            })
                            .unwrap_or_default();
                        let cancelled = tool_calls
                            .iter()
                            .filter(|call| !result_ids.contains(&call.id))
                            .map(|call| AgentMessage {
                                role: "tool".to_string(),
                                content: Some(json!({"error": "用户已停止，工具未执行。"})),
                                reasoning_content: None,
                                tool_calls: None,
                                tool_call_id: Some(call.id.clone()),
                                name: Some(call.function.name.clone()),
                                created_at: Some(current_timestamp()),
                            })
                            .collect::<Vec<_>>();
                        if !cancelled.is_empty() {
                            state
                                .agent_manager
                                .append_messages(&session_id, cancelled)
                                .await;
                        }
                        break 'agent_loop;
                    }
                    continue;
                }
            };
            let (result_value, is_error) = match result {
                Ok(v) => (v, false),
                Err(e) => (json!({"error": e.message}), true),
            };
            let _ = tx
                .send(AgentEvent::ToolResult {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    result: result_value.clone(),
                    is_error,
                })
                .await;
            state
                .agent_manager
                .append_messages(
                    &session_id,
                    vec![AgentMessage {
                        role: "tool".to_string(),
                        content: Some(result_value),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(tc.id.clone()),
                        name: Some(tc.function.name.clone()),
                        created_at: Some(current_timestamp()),
                    }],
                )
                .await;
        }
        // Loop continues: LLM gets tool results and decides next step
    }
}

// ---------------------------------------------------------------------------
// REST API handlers (session CRUD)
// ---------------------------------------------------------------------------

pub async fn list_sessions(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let sessions = state.agent_manager.list_sessions().await;
    Ok(Json(json!({"sessions": sessions})))
}

fn session_preset_model(state: &AppState, api_preset_id: &str) -> ApiResult<Option<String>> {
    let presets = state.auth_manager.api_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == api_preset_id)
        .ok_or_else(|| {
            AppError::bad_request(format!(
                "Agent 会话指定的 API 预设已不存在（id={}）。",
                api_preset_id
            ))
        })?;
    Ok(api_preset_model(preset).map(str::to_string))
}

async fn resolve_session_preset_id(
    state: &AppState,
    requested_api_preset_id: &str,
) -> ApiResult<String> {
    let requested = requested_api_preset_id.trim();
    if !requested.is_empty() {
        session_preset_model(state, requested)?;
        return Ok(requested.to_string());
    }

    let config = state.agent_config.get().await;
    if !config.api_preset_id.is_empty() {
        session_preset_model(state, &config.api_preset_id)?;
        return Ok(config.api_preset_id);
    }

    let user = state.workspace_settings.terminal_user();
    Ok(resolve_current_api_state(state, &user)
        .await
        .and_then(|current| current.preset_id)
        .unwrap_or_default())
}

fn resolve_profile_api_preset_id(
    state: &AppState,
    profile: &TerminalAgentProfile,
) -> ApiResult<String> {
    let presets = state.auth_manager.api_presets_snapshot();
    let selector = profile.preset_selector.trim();
    let expected = selector.to_lowercase();
    let matching = match profile.preset_match.as_str() {
        "id" => presets
            .iter()
            .filter(|preset| preset.id == selector)
            .collect::<Vec<_>>(),
        "exact_name" => presets
            .iter()
            .filter(|preset| preset.name.trim().to_lowercase() == expected)
            .collect::<Vec<_>>(),
        "unique_contains" => {
            let exact = presets
                .iter()
                .filter(|preset| preset.name.trim().to_lowercase() == expected)
                .collect::<Vec<_>>();
            if exact.len() == 1 {
                exact
            } else {
                presets
                    .iter()
                    .filter(|preset| preset.name.trim().to_lowercase().contains(&expected))
                    .collect::<Vec<_>>()
            }
        }
        _ => Vec::new(),
    };
    match matching.as_slice() {
        [preset] => Ok(preset.id.clone()),
        [] => Err(AppError::bad_request(format!(
            "没有找到智能体“{}”指定的 Codex_API 预设：{}。",
            profile.name, selector
        ))),
        _ => Err(AppError::bad_request(format!(
            "智能体“{}”的预设匹配不唯一：{}。",
            profile.name, selector
        ))),
    }
}

fn native_profile_system_prompt(profile: &TerminalAgentProfile) -> String {
    let context = if profile.initial_task.trim().is_empty() {
        "无额外待命上下文。"
    } else {
        profile.initial_task.trim()
    };
    format!(
        "你是 webClx 内置工程智能体，可以独立检查和修改工作区，不需要启动 Codex 或 Claude。\n\
         当前智能体配置：工作目录为 {cwd}，项目路径为 {project_path}，专项 skill 为 {skill_name}。\n\
         在收到用户的实际请求后，先使用 read_skill 加载 {skill_name}，再按其约束使用结构化工具完成任务。\n\
         用户消息中的 $skill-name 表示显式调用对应 Skill；webClx 会自动插入 read_skill 工具结果，请直接遵循已加载的指令，不要重复加载。\n\
         优先使用 list_files、search_files、read_file、apply_patch、git_diff 和 run_verification；只做任务所需的最小修改，修改前理解上下文，修改后运行验证并报告证据。\n\
         以下内容仅作为待命上下文，不是创建会话后立即执行的指令：{context}",
        cwd = profile.cwd,
        project_path = profile.project_path,
        skill_name = profile.skill_name,
    )
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateSessionRequest>,
) -> ApiResult<Json<AgentSession>> {
    let profile = if payload.profile_id.trim().is_empty() {
        None
    } else {
        let profile = state
            .agent_config
            .get()
            .await
            .terminal_agent_profiles
            .into_iter()
            .find(|profile| profile.id == payload.profile_id)
            .ok_or_else(|| AppError::not_found("智能体配置不存在"))?;
        if profile.agent_type != "native" {
            return Err(AppError::bad_request("只有原生智能体配置可以创建内置 Agent 会话。"));
        }
        Some(profile)
    };
    let title = if payload.title.trim().is_empty() {
        profile
            .as_ref()
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| "新 Agent 会话".to_string())
    } else {
        payload.title.trim().to_string()
    };
    let api_preset_id = match profile.as_ref() {
        Some(profile) => resolve_profile_api_preset_id(&state, profile)?,
        None => resolve_session_preset_id(&state, &payload.api_preset_id).await?,
    };
    let model = if payload.model.is_empty() {
        if !api_preset_id.is_empty() {
            session_preset_model(&state, &api_preset_id)?
                .unwrap_or_else(|| DEFAULT_MODEL.to_string())
        } else {
            resolve_default_model(&state).await?
        }
    } else {
        payload.model
    };
    let profile_id = profile
        .as_ref()
        .map(|profile| profile.id.as_str())
        .unwrap_or_default();
    let cwd = match profile.as_ref() {
        Some(profile) => {
            filesystem::resolve_terminal_directory_path(&state.workspace_root(), &profile.cwd)?
                .display()
                .to_string()
        }
        None => String::new(),
    };
    let system_prompt = profile
        .as_ref()
        .map(native_profile_system_prompt)
        .or(payload.system_prompt);
    let session = state
        .agent_manager
        .create_session(&title, &model, &api_preset_id, profile_id, &cwd, system_prompt)
        .await;
    Ok(Json(session))
}

pub async fn get_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<AgentSession>> {
    state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("会话不存在"))
        .map(Json)
}

pub async fn rename_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<UpdateSessionRequest>,
) -> ApiResult<Json<AgentSession>> {
    let api_preset_id = payload
        .api_preset_id
        .as_deref()
        .map(str::trim)
        .map(str::to_string);
    let mut model = payload
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string);
    let sandbox_mode = payload
        .sandbox_mode
        .as_deref()
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .map(normalize_sandbox_mode)
        .map(str::to_string);
    let approval_policy = payload
        .approval_policy
        .as_deref()
        .map(str::trim)
        .filter(|policy| !policy.is_empty())
        .map(normalize_approval_policy)
        .map(str::to_string);
    if let Some(api_preset_id) = api_preset_id.as_deref()
        && !api_preset_id.is_empty()
    {
        let preset_model = session_preset_model(&state, api_preset_id)?;
        if model.is_none() {
            model = preset_model;
        }
    }
    state
        .agent_manager
        .update_session_settings(
            &session_id,
            &payload.title,
            model.as_deref(),
            api_preset_id.as_deref(),
            sandbox_mode.as_deref(),
            approval_policy.as_deref(),
        )
        .await
        .ok_or_else(|| AppError::not_found("会话不存在"))
        .map(Json)
}

pub async fn delete_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let removed = state.agent_manager.delete_session(&session_id).await;
    if !removed {
        return Err(AppError::not_found("会话不存在"));
    }
    Ok(Json(json!({"ok": true})))
}

pub async fn clear_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    state
        .agent_manager
        .clear_messages(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("会话不存在"))?;
    Ok(Json(json!({"ok": true})))
}

pub async fn get_session_context(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<AgentContextStatus>> {
    let session = state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let credential = resolve_llm_credential(&state, &session.model, &session.api_preset_id).await?;
    Ok(Json(context_status(&session, &credential)))
}

pub async fn get_run_status(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<AgentRunStatus>> {
    state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    Ok(Json(state.agent_manager.active_run_status(&session_id).await))
}

pub async fn list_approvals(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let approvals = state.agent_manager.list_approvals(&session_id).await;
    Ok(Json(json!({"approvals": approvals})))
}

pub async fn allow_approval(
    State(state): State<AppState>,
    AxumPath((session_id, approval_id)): AxumPath<(String, String)>,
) -> ApiResult<Json<Value>> {
    state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let approval = state
        .agent_manager
        .approve_approval(&session_id, &approval_id)
        .await
        .ok_or_else(|| AppError::not_found("批准请求不存在或已处理"))?;
    let session = state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    Ok(Json(json!({
        "ok": true,
        "approval": approval,
        "approval_policy": session.approval_policy,
    })))
}

pub async fn deny_approval(
    State(state): State<AppState>,
    AxumPath((session_id, approval_id)): AxumPath<(String, String)>,
) -> ApiResult<Json<Value>> {
    state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let approval = state
        .agent_manager
        .deny_approval(&session_id, &approval_id)
        .await
        .ok_or_else(|| AppError::not_found("批准请求不存在或已处理"))?;
    Ok(Json(json!({"ok": true, "approval": approval})))
}

pub async fn allow_all_approvals(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let approved = state.agent_manager.approve_all_pending(&session_id).await;
    let session = state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    Ok(Json(json!({
        "ok": true,
        "approved": approved,
        "approval_policy": session.approval_policy,
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionSummaryRequest {
    pub summary: String,
    #[serde(default)]
    pub context_files: Vec<String>,
}

pub async fn update_session_summary(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<UpdateSessionSummaryRequest>,
) -> ApiResult<Json<AgentSession>> {
    state
        .agent_manager
        .update_context_summary(&session_id, &payload.summary, &payload.context_files)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))
        .map(Json)
}

pub async fn list_checkpoints(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let session = state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    engineering_tools::list_checkpoints(
        &state,
        (!session.cwd.is_empty()).then_some(session.cwd.as_str()),
    )
    .await
    .map(Json)
}

pub async fn restore_checkpoint(
    State(state): State<AppState>,
    AxumPath((session_id, checkpoint_id)): AxumPath<(String, String)>,
) -> ApiResult<Json<Value>> {
    let session = state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    engineering_tools::restore_checkpoint(
        &state,
        (!session.cwd.is_empty()).then_some(session.cwd.as_str()),
        &checkpoint_id,
    )
    .await
    .map(Json)
}

pub async fn compact_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<CompactSessionResult>> {
    let session = state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let credential = resolve_llm_credential(&state, &session.model, &session.api_preset_id).await?;
    let environment = llm::environment::LlmHttpEnvironment::capture(
        &state.proxy_manager,
        &state.workspace_settings,
    )
    .await?;
    let http_context = environment.context_for(
        &credential.terminal_env,
        Duration::from_secs(credential.context_limits.llm_timeout_secs),
    )?;
    compact_session_history(&state, &session_id, &credential, &http_context.client)
        .await
        .map(Json)
}

#[derive(Debug, Deserialize)]
pub struct StartBackgroundCommandRequest {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub rows: Option<u16>,
    #[serde(default)]
    pub cols: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct WriteBackgroundCommandRequest {
    pub input: String,
}

pub async fn list_background_commands(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let commands = state
        .agent_manager
        .background_commands
        .list(&session_id)
        .await;
    Ok(Json(json!({"commands": commands})))
}

pub async fn start_background_command_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<StartBackgroundCommandRequest>,
) -> ApiResult<Json<background_commands::BackgroundCommandSession>> {
    let session = state
        .agent_manager
        .get_session(&session_id)
        .await
        .ok_or_else(|| AppError::not_found("agent 会话不存在"))?;
    let requested_cwd = payload
        .cwd
        .as_deref()
        .filter(|cwd| !cwd.trim().is_empty())
        .or_else(|| (!session.cwd.is_empty()).then_some(session.cwd.as_str()));
    let cwd = engineering_tools::resolve_cwd(&state, requested_cwd)?;
    state
        .agent_manager
        .background_commands
        .start(&session_id, &payload.command, &cwd, payload.rows, payload.cols)
        .await
        .map(Json)
}

pub async fn get_background_command_session(
    State(state): State<AppState>,
    AxumPath((session_id, command_id)): AxumPath<(String, String)>,
) -> ApiResult<Json<background_commands::BackgroundCommandSession>> {
    state
        .agent_manager
        .background_commands
        .get(&session_id, &command_id)
        .await
        .map(Json)
}

pub async fn write_background_command_session(
    State(state): State<AppState>,
    AxumPath((session_id, command_id)): AxumPath<(String, String)>,
    Json(payload): Json<WriteBackgroundCommandRequest>,
) -> ApiResult<Json<background_commands::BackgroundCommandSession>> {
    state
        .agent_manager
        .background_commands
        .write_stdin(&session_id, &command_id, &payload.input)
        .await
        .map(Json)
}

pub async fn terminate_background_command_session(
    State(state): State<AppState>,
    AxumPath((session_id, command_id)): AxumPath<(String, String)>,
) -> ApiResult<Json<background_commands::BackgroundCommandSession>> {
    state
        .agent_manager
        .background_commands
        .terminate(&session_id, &command_id)
        .await
        .map(Json)
}

async fn resolve_default_model(state: &AppState) -> ApiResult<String> {
    let config = state.agent_config.get().await;
    if !config.default_model.is_empty() {
        return Ok(config.default_model);
    }
    // If a preset is pinned, try to derive its model from config_overrides.
    if !config.api_preset_id.is_empty() {
        let presets = state.auth_manager.api_presets_snapshot();
        if let Some(preset) = presets.iter().find(|p| p.id == config.api_preset_id) {
            if let Some(model) = api_preset_model(preset) {
                return Ok(model.to_string());
            }
        }
    } else {
        // No preset pinned: derive from the currently applied Codex_API preset
        let user = state.workspace_settings.terminal_user();
        let auth_file = runtime_paths::resolve_user_file(&user, AUTH_FILE_RELATIVE_PATH)
            .map_err(|e| AppError::internal(format!("解析用户路径失败: {e}")))?;
        let config_file = runtime_paths::resolve_user_file(&user, CONFIG_FILE_RELATIVE_PATH)
            .map_err(|e| AppError::internal(format!("解析用户路径失败: {e}")))?;
        let api_presets = state.auth_manager.api_presets_snapshot();
        let current_auth = read_current_auth_state(&auth_file).await.ok().flatten();
        let current_config = read_current_config_provider(&config_file)
            .await
            .ok()
            .flatten();
        let current_api =
            derive_current_api_state(current_config.as_ref(), current_auth.as_ref(), &api_presets);
        if let Some(model) = current_api
            .as_ref()
            .and_then(|c| c.config_values.get("model"))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            return Ok(model);
        }
    }
    resolve_llm_credential(state, "", "").await.map(|c| c.model)
}

/// Resolve the effective preset name, provider name, and model that Agent will
/// actually use for new sessions, mirroring the resolution logic in
/// resolve_default_model / resolve_llm_credential_with_preset. Used by the
/// settings UI to show the currently applied preset and model.
async fn resolve_effective_preset_info(
    state: &AppState,
) -> (Option<String>, Option<String>, String) {
    let config = state.agent_config.get().await;
    let presets = state.auth_manager.api_presets_snapshot();

    if !config.api_preset_id.is_empty() {
        // A specific preset is pinned: derive name/model from it directly.
        if let Some(preset) = presets.iter().find(|p| p.id == config.api_preset_id) {
            let model = if !config.default_model.is_empty() {
                config.default_model.clone()
            } else {
                api_preset_model(preset)
                    .map(str::to_string)
                    .unwrap_or_else(|| DEFAULT_MODEL.to_string())
            };
            let provider = if preset.provider_name.is_empty() {
                None
            } else {
                Some(preset.provider_name.clone())
            };
            return (Some(preset.name.clone()), provider, model);
        }
        // Pinned preset no longer exists.
        let model = if !config.default_model.is_empty() {
            config.default_model.clone()
        } else {
            DEFAULT_MODEL.to_string()
        };
        return (None, None, model);
    }

    // No preset pinned: derive from the currently applied Codex_API preset.
    let user = state.workspace_settings.terminal_user();
    let current_api = resolve_current_api_state(state, &user).await;

    let model = if !config.default_model.is_empty() {
        config.default_model.clone()
    } else {
        current_api
            .as_ref()
            .and_then(|c| c.config_values.get("model"))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string())
    };
    let preset_name = current_api.as_ref().and_then(|c| c.preset_name.clone());
    let provider_name = current_api
        .as_ref()
        .and_then(|c| c.provider_name.clone())
        .or_else(|| current_api.and_then(|c| c.provider_id.clone()));

    (preset_name, provider_name, model)
}

/// Resolve the currently applied Codex_API state (config.toml + auth.json).
/// Returns None if no preset is applied or the files cannot be read.
async fn resolve_current_api_state(
    state: &AppState,
    user: &str,
) -> Option<auth_core::CurrentApiState> {
    let auth_file = runtime_paths::resolve_user_file(user, AUTH_FILE_RELATIVE_PATH).ok()?;
    let config_file = runtime_paths::resolve_user_file(user, CONFIG_FILE_RELATIVE_PATH).ok()?;
    let presets = state.auth_manager.api_presets_snapshot();
    let current_auth = read_current_auth_state(&auth_file).await.ok().flatten();
    let current_config = read_current_config_provider(&config_file)
        .await
        .ok()
        .flatten();
    derive_current_api_state(current_config.as_ref(), current_auth.as_ref(), &presets)
}

pub async fn list_tools() -> ApiResult<Json<Value>> {
    Ok(Json(json!({"tools": tool_definitions()})))
}

pub async fn available_models(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let config = state.agent_config.get().await;
    let default_model = resolve_default_model(&state)
        .await
        .unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    // When a specific API preset is selected, the model candidates come from
    // that preset only. When using the "currently applied" preset, fall back
    // to the generic model list.
    let mut all_models: Vec<String> = Vec::new();

    if !config.api_preset_id.is_empty() {
        // Derive model from the selected preset's config_overrides
        let presets = state.auth_manager.api_presets_snapshot();
        if let Some(preset) = presets.iter().find(|p| p.id == config.api_preset_id) {
            if let Some(model) = api_preset_model(preset) {
                all_models.push(model.to_string());
            }
        }
        // Also include the resolved default / config default as a fallback option
        if !default_model.is_empty() && !all_models.contains(&default_model) {
            all_models.push(default_model.clone());
        }
    } else {
        // No preset pinned: derive model from the currently applied Codex_API preset
        let user = state.workspace_settings.terminal_user();
        let auth_file = runtime_paths::resolve_user_file(&user, AUTH_FILE_RELATIVE_PATH)
            .map_err(|e| AppError::internal(format!("解析用户路径失败: {e}")))?;
        let config_file = runtime_paths::resolve_user_file(&user, CONFIG_FILE_RELATIVE_PATH)
            .map_err(|e| AppError::internal(format!("解析用户路径失败: {e}")))?;
        let api_presets = state.auth_manager.api_presets_snapshot();
        let current_auth = read_current_auth_state(&auth_file).await.ok().flatten();
        let current_config = read_current_config_provider(&config_file)
            .await
            .ok()
            .flatten();
        let current_api =
            derive_current_api_state(current_config.as_ref(), current_auth.as_ref(), &api_presets);

        if let Some(current) = &current_api {
            // Extract model from the resolved config_values (key "model")
            if let Some(model) = current
                .config_values
                .get("model")
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
            {
                all_models.push(model);
            }
        }

        // Also include config default_model if set
        if !config.default_model.is_empty() && !all_models.contains(&config.default_model) {
            all_models.insert(0, config.default_model.clone());
        }
    }

    Ok(Json(json!({
        "default": default_model,
        "models": all_models,
    })))
}

// ---------------------------------------------------------------------------
// Agent config API handlers (skill management)
// ---------------------------------------------------------------------------

pub async fn get_config(State(state): State<AppState>) -> ApiResult<Json<AgentConfig>> {
    Ok(Json(state.agent_config.get().await))
}

pub async fn save_config(
    State(state): State<AppState>,
    Json(payload): Json<AgentConfig>,
) -> ApiResult<Json<AgentConfig>> {
    // Validate extra_skill_dirs: only keep existing dirs
    let valid_dirs: Vec<String> = payload
        .extra_skill_dirs
        .iter()
        .filter(|d| {
            !d.trim().is_empty() && PathBuf::from(d).is_absolute() && PathBuf::from(d).exists()
        })
        .cloned()
        .collect();
    let terminal_agent_profiles =
        normalize_terminal_agent_profiles(&payload.terminal_agent_profiles)?;
    let config = state
        .agent_config
        .update(|c| {
            c.default_model = payload.default_model.clone();
            c.api_preset_id = payload.api_preset_id.clone();
            c.disabled_skills = payload.disabled_skills.clone();
            c.extra_skill_dirs = valid_dirs;
            c.system_prompt_override = payload.system_prompt_override.clone();
            c.terminal_agent_profiles = terminal_agent_profiles;
        })
        .await;
    Ok(Json(config))
}

pub async fn list_terminal_profiles(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let config = state.agent_config.get().await;
    Ok(Json(json!({"profiles": config.terminal_agent_profiles})))
}

pub async fn get_terminal_profile(
    State(state): State<AppState>,
    AxumPath(profile_id): AxumPath<String>,
) -> ApiResult<Json<TerminalAgentProfile>> {
    state
        .agent_config
        .get()
        .await
        .terminal_agent_profiles
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| AppError::not_found("智能体不存在"))
        .map(Json)
}

pub async fn create_terminal_profile(
    State(state): State<AppState>,
    Json(mut payload): Json<TerminalAgentProfile>,
) -> ApiResult<Json<TerminalAgentProfile>> {
    if payload.id.trim().is_empty() {
        payload.id = format!("agent_{}", generate_id());
    }
    let profile = normalize_terminal_agent_profile(&payload)?;
    let current = state.agent_config.get().await;
    if current.terminal_agent_profiles.len() >= MAX_TERMINAL_AGENT_PROFILES {
        return Err(AppError::bad_request("智能体不能超过 64 个。"));
    }
    if current
        .terminal_agent_profiles
        .iter()
        .any(|item| item.id == profile.id)
    {
        return Err(AppError::bad_request("智能体 ID 已存在。"));
    }
    let stored = profile.clone();
    state
        .agent_config
        .update(|config| config.terminal_agent_profiles.push(stored))
        .await;
    Ok(Json(profile))
}

pub async fn update_terminal_profile(
    State(state): State<AppState>,
    AxumPath(profile_id): AxumPath<String>,
    Json(mut payload): Json<TerminalAgentProfile>,
) -> ApiResult<Json<TerminalAgentProfile>> {
    payload.id = profile_id.clone();
    let profile = normalize_terminal_agent_profile(&payload)?;
    let current = state.agent_config.get().await;
    if !current
        .terminal_agent_profiles
        .iter()
        .any(|item| item.id == profile_id)
    {
        return Err(AppError::not_found("智能体不存在"));
    }
    let stored = profile.clone();
    state
        .agent_config
        .update(|config| {
            if let Some(item) = config
                .terminal_agent_profiles
                .iter_mut()
                .find(|item| item.id == profile_id)
            {
                *item = stored;
            }
        })
        .await;
    Ok(Json(profile))
}

pub async fn delete_terminal_profile(
    State(state): State<AppState>,
    AxumPath(profile_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let current = state.agent_config.get().await;
    if !current
        .terminal_agent_profiles
        .iter()
        .any(|item| item.id == profile_id)
    {
        return Err(AppError::not_found("智能体不存在"));
    }
    state
        .agent_config
        .update(|config| {
            config
                .terminal_agent_profiles
                .retain(|item| item.id != profile_id)
        })
        .await;
    Ok(Json(json!({"ok": true})))
}

pub async fn list_skills_api(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let skills = scan_all_skills(&state).await?;
    Ok(Json(json!({"skills": skills})))
}

pub async fn toggle_skill(
    State(state): State<AppState>,
    Json(payload): Json<ToggleSkillRequest>,
) -> ApiResult<Json<Value>> {
    let skill_name = payload.skill_name.trim().to_string();
    if skill_name.is_empty() {
        return Err(AppError::bad_request("skill_name 不能为空"));
    }
    let config = state
        .agent_config
        .update(|c| {
            if payload.disable {
                if !c.disabled_skills.contains(&skill_name) {
                    c.disabled_skills.push(skill_name.clone());
                }
            } else {
                c.disabled_skills.retain(|s| s != &skill_name);
            }
        })
        .await;
    let is_disabled = config.disabled_skills.contains(&skill_name);
    Ok(Json(json!({"ok": true, "skill_name": skill_name, "disabled": is_disabled})))
}

pub async fn add_skill_dir(
    State(state): State<AppState>,
    Json(payload): Json<AddSkillDirRequest>,
) -> ApiResult<Json<Value>> {
    let dir = payload.dir.trim().to_string();
    if dir.is_empty() {
        return Err(AppError::bad_request("dir 不能为空"));
    }
    let path = PathBuf::from(&dir);
    if !path.is_absolute() {
        return Err(AppError::bad_request("路径必须是绝对路径。"));
    }
    if !path.exists() {
        return Err(AppError::bad_request(format!("目录不存在: {dir}")));
    }
    if !path.is_dir() {
        return Err(AppError::bad_request(format!("路径不是目录: {dir}")));
    }
    let config = state
        .agent_config
        .update(|c| {
            if !c.extra_skill_dirs.contains(&dir) {
                c.extra_skill_dirs.push(dir.clone());
            }
        })
        .await;
    Ok(Json(json!({"ok": true, "extra_skill_dirs": config.extra_skill_dirs})))
}

pub async fn remove_skill_dir(
    State(state): State<AppState>,
    Json(payload): Json<RemoveSkillDirRequest>,
) -> ApiResult<Json<Value>> {
    let dir = payload.dir.trim().to_string();
    let config = state
        .agent_config
        .update(|c| {
            c.extra_skill_dirs.retain(|d| d != &dir);
        })
        .await;
    Ok(Json(json!({"ok": true, "extra_skill_dirs": config.extra_skill_dirs})))
}

#[derive(Debug, Deserialize)]
pub struct ToggleSkillRequest {
    pub skill_name: String,
    pub disable: bool,
}

#[derive(Debug, Deserialize)]
pub struct AddSkillDirRequest {
    pub dir: String,
}

#[derive(Debug, Deserialize)]
pub struct RemoveSkillDirRequest {
    pub dir: String,
}

/// Returns a simplified list of Codex_API presets for the agent preset dropdown.
/// Each entry has id, name, base_url, and provider_name.
pub async fn list_api_presets_for_agent(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let presets = state.auth_manager.api_presets_snapshot();
    let config = state.agent_config.get().await;
    let items: Vec<Value> = presets
        .iter()
        .map(|p| {
            let model = api_preset_model(p).unwrap_or_default();
            json!({
                "id": p.id,
                "name": p.name,
                "base_url": p.base_url,
                "provider_name": p.provider_name,
                "model": model,
            })
        })
        .collect();
    let (effective_preset_name, effective_provider_name, effective_model) =
        resolve_effective_preset_info(&state).await;

    Ok(Json(json!({
        "presets": items,
        "current_preset_id": config.api_preset_id,
        "effective": {
            "preset_name": effective_preset_name,
            "provider_name": effective_provider_name,
            "model": effective_model,
        },
    })))
}

pub async fn agent_page(State(state): State<AppState>) -> Response {
    let path = state.static_dir.join("agent.html");
    let fallback = include_str!("../static/agent.html");
    crate::html_page_response(path, fallback).await
}

#[cfg(test)]
mod terminal_profile_tests {
    use super::*;

    #[test]
    fn default_terminal_profiles_include_proxy_and_work_agents() {
        let profiles = default_terminal_agent_profiles();
        let proxy = profiles
            .iter()
            .find(|profile| profile.id == "proxy_settings_agent")
            .expect("proxy settings profile");
        assert_eq!(proxy.preset_selector, "miniMax");
        assert_eq!(proxy.preset_match, "unique_contains");
        assert_eq!(proxy.cwd, "/home/system");
        assert_eq!(proxy.project_path, "/home/system");
        assert_eq!(proxy.skill_name, "mihomo-proxy-ops");

        let work = profiles
            .iter()
            .find(|profile| profile.id == "work_agent")
            .expect("work agent profile");
        assert_eq!(work.cwd, "/home/third_party");
        assert_eq!(work.skill_name, "autopilot");
    }

    #[test]
    fn terminal_profile_validation_requires_fixed_launch_context() {
        let mut profile = default_terminal_agent_profiles().remove(0);
        assert!(normalize_terminal_agent_profile(&profile).is_ok());

        profile.cwd = "relative/path".to_string();
        assert!(normalize_terminal_agent_profile(&profile).is_err());
        profile.cwd = "/home/system".to_string();
        profile.preset_match = "first_match".to_string();
        assert!(normalize_terminal_agent_profile(&profile).is_err());
        profile.preset_match = "id".to_string();
        profile.skill_name = "two skills".to_string();
        assert!(normalize_terminal_agent_profile(&profile).is_err());
    }

    #[test]
    fn terminal_profile_agent_type_defaults_to_codex_and_validates_supported_engines() {
        let legacy: TerminalAgentProfile = serde_json::from_value(json!({
            "id": "legacy",
            "name": "Legacy",
            "description": "",
            "preset_selector": "api-1",
            "preset_match": "id",
            "cwd": "/home/codes/webClx",
            "project_path": "/home/codes/webClx",
            "skill_name": "autopilot",
            "initial_task": "",
            "terminal_name": "Legacy"
        }))
        .expect("legacy terminal profile remains readable");
        assert_eq!(legacy.agent_type, "codex");

        for agent_type in ["native", "codex", "claude"] {
            let mut profile = legacy.clone();
            profile.agent_type = agent_type.to_string();
            assert!(
                normalize_terminal_agent_profile(&profile).is_ok(),
                "{agent_type} should be supported",
            );
        }

        let mut unsupported = legacy;
        unsupported.agent_type = "opencode".to_string();
        assert!(normalize_terminal_agent_profile(&unsupported).is_err());
    }

    #[test]
    fn blank_tool_paths_fall_back_to_the_native_session_cwd() {
        let session = AgentSession {
            id: "session-cwd".to_string(),
            title: "Session cwd".to_string(),
            model: "test-model".to_string(),
            api_preset_id: "api-test".to_string(),
            profile_id: "agent_factory".to_string(),
            cwd: "/home/codes/webClx".to_string(),
            sandbox_mode: "default".to_string(),
            approval_policy: "ask_once".to_string(),
            system_prompt: None,
            context_summary: None,
            context_files: Vec::new(),
            compacted_messages: 0,
            compacted_at: None,
            last_token_usage: None,
            context_usage_source: "estimated".to_string(),
            active_run: None,
            run_interrupted_at: None,
            messages: Vec::new(),
            created_at: 1,
            updated_at: 1,
        };

        assert_eq!(session_tool_path(&session, &json!({"path": ""})), Some("/home/codes/webClx"),);
        assert_eq!(session_tool_path(&session, &json!({"path": "src"})), Some("src"),);
    }

    #[test]
    fn dollar_skill_references_create_explicit_read_skill_calls() {
        assert_eq!(
            extract_explicit_skill_invocations(
                "先用 $mihomo-proxy-ops，再用 $terminal-message 汇报；重复 $mihomo-proxy-ops。",
            ),
            vec!["mihomo-proxy-ops", "terminal-message"],
        );
        assert!(
            extract_explicit_skill_invocations("价格 $100，变量price$skill，单独一个 $").is_empty()
        );

        let tool_call = skill_invocation_tool_call("mihomo-proxy-ops", 0);
        assert_eq!(tool_call.function.name, "read_skill");
        assert_eq!(
            serde_json::from_str::<Value>(&tool_call.function.arguments).unwrap(),
            json!({"skill_name": "mihomo-proxy-ops"}),
        );
    }

    #[test]
    fn built_in_agent_exposes_practical_engineering_tools() {
        let definitions = tool_definitions();
        let names = definitions
            .iter()
            .filter_map(|definition| definition["function"]["name"].as_str())
            .collect::<std::collections::HashSet<_>>();
        for required in [
            "list_files",
            "search_files",
            "read_file",
            "apply_patch",
            "git_diff",
            "create_checkpoint",
            "list_checkpoints",
            "restore_checkpoint",
            "run_verification",
        ] {
            assert!(names.contains(required), "missing engineering tool: {required}");
        }
    }

    #[test]
    fn context_token_estimate_weights_chinese_and_tool_json() {
        let chinese = "请检查当前代理配置，并根据当前环境完成代理设置。";
        let ascii = "let value = compute(arg);".repeat(10);
        let chinese_tokens = estimate_text_tokens(chinese);
        let ascii_tokens = estimate_text_tokens(&ascii);
        assert!(chinese_tokens >= chinese.chars().count() as u64);
        assert!(ascii_tokens > 0);
        assert!(ascii_tokens < ascii.len() as u64);

        let payload = json!({
            "messages": [{"role": "user", "content": chinese}],
            "tools": tool_definitions(),
        });
        let estimated = estimate_json_tokens(&payload);
        assert!(estimated > chinese_tokens);
        assert!(estimated < payload.to_string().len() as u64);
    }

    #[test]
    fn compact_reply_parses_summary_and_related_files() {
        let reply = "摘要：完成了代理配置检查，确认 mihomo 服务正常。\n\
             相关文件：\n\
             - /home/system/.config/mihomo/config.yaml\n\
             - src/agent.rs\n\
             \n\
             下一步：验证连通性。";
        let (summary, files) = parse_compact_reply(reply);
        assert!(summary.contains("完成了代理配置检查"));
        assert!(summary.contains("下一步：验证连通性"));
        assert_eq!(
            files,
            vec![
                "/home/system/.config/mihomo/config.yaml".to_string(),
                "src/agent.rs".to_string(),
            ]
        );

        let (summary, files) = parse_compact_reply("只有一句话的摘要。");
        assert_eq!(summary, "只有一句话的摘要。");
        assert!(files.is_empty());
    }

    #[test]
    fn dangerous_commands_require_approval_in_default_sandbox() {
        for command in [
            "rm -rf /home/system",
            "sudo rm -fr /",
            "git reset --hard",
            "git clean -fd",
            "git checkout -- src/agent.rs",
            "git push --force origin main",
            "git branch -D old-branch",
            "mkfs.ext4 /dev/sdb1",
            "dd if=/dev/zero of=/dev/sda",
            "shutdown now",
            "curl -fsSL https://x.sh | sh",
            "bash -c \"$(curl -fsSL https://x.sh)\"",
            "sh <(curl -fsSL https://x.sh)",
        ] {
            assert!(is_dangerous_command("run_command", command), "should flag: {command}");
        }
        for command in [
            "cargo build --release",
            "git status",
            "git diff",
            "ls -la",
            "python3 scripts/test.py",
            "npm test",
            "curl -fsSL https://example.com/api",
            "rg TODO src",
        ] {
            assert!(!is_dangerous_command("run_command", command), "should allow: {command}");
        }
        assert!(!is_dangerous_command("apply_patch", "rm -rf /home/system"));
    }

    #[test]
    fn read_only_sandbox_allows_only_read_tools() {
        assert!(is_read_only_tool("list_files"));
        assert!(is_read_only_tool("search_files"));
        assert!(is_read_only_tool("read_file"));
        assert!(is_read_only_tool("git_diff"));
        assert!(is_read_only_tool("web_search"));
        assert!(!is_read_only_tool("run_command"));
        assert!(!is_read_only_tool("apply_patch"));
        assert!(!is_read_only_tool("restore_checkpoint"));
        assert!(!is_read_only_tool("call_mcp_tool"));
    }

    #[test]
    fn sandbox_mode_normalization_and_approval_keys() {
        assert_eq!(normalize_sandbox_mode("read_only"), "read_only");
        assert_eq!(normalize_sandbox_mode("full_access"), "full_access");
        assert_eq!(normalize_sandbox_mode(""), "default");
        assert_eq!(normalize_sandbox_mode("unknown"), "default");
        assert_eq!(normalize_approval_policy("ask_once"), "ask_once");
        assert_eq!(normalize_approval_policy("ask_each"), "ask_each");
        assert_eq!(normalize_approval_policy("allow_all"), "allow_all");
        assert_eq!(normalize_approval_policy(""), "ask_once");
        assert_eq!(normalize_approval_policy("whatever"), "ask_once");
        assert_eq!(
            AgentManager::approval_key("run_command", "rm -rf /home/system"),
            AgentManager::approval_key("run_command", "rm -rf /home/system"),
        );
        assert_ne!(
            AgentManager::approval_key("run_command", "rm -rf /a"),
            AgentManager::approval_key("run_command", "rm -rf /b"),
        );
    }

    #[test]
    fn built_in_agent_command_tools_are_bounded() {
        let definitions = tool_definitions();
        for name in ["run_command", "run_verification"] {
            let definition = definitions
                .iter()
                .find(|definition| definition["function"]["name"] == name)
                .unwrap_or_else(|| panic!("missing command tool: {name}"));
            assert!(
                definition["function"]["parameters"]["properties"]["timeout_secs"].is_object(),
                "{name} must expose timeout_secs",
            );
        }
    }

    #[test]
    fn native_agent_context_limits_follow_the_selected_preset() {
        let overrides = vec![
            auth_core::PresetConfigOverride {
                key: Some("model_context_window".to_string()),
                value: Some("262_144".to_string()),
            },
            auth_core::PresetConfigOverride {
                key: Some("model_auto_compact_token_limit".to_string()),
                value: Some("200000".to_string()),
            },
        ];

        let limits = context_limits_from_overrides(&overrides);
        assert_eq!(limits.context_window, 262_144);
        assert_eq!(limits.compact_threshold, 200_000);
        assert_eq!(limits.source, "preset");
    }

    #[test]
    fn compact_split_keeps_tool_calls_with_their_results() {
        let messages = vec![
            test_message("user", Some("old request")),
            AgentMessage {
                role: "assistant".to_string(),
                content: None,
                reasoning_content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call-1".to_string(),
                    call_type: "function".to_string(),
                    function: ToolFunction {
                        name: "run_command".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                created_at: None,
            },
            AgentMessage {
                role: "tool".to_string(),
                content: Some(json!({"stdout": "done"})),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: Some("call-1".to_string()),
                name: Some("run_command".to_string()),
                created_at: None,
            },
            test_message("assistant", Some("old result")),
            test_message("user", Some("current request")),
            test_message("assistant", Some("current result")),
        ];

        let split = compact_split_index(&messages).expect("older history is compactable");
        assert_eq!(split, 4);
        assert_eq!(messages[split].role, "user");
        assert!(
            messages[..split]
                .iter()
                .any(|message| message.role == "tool")
        );
    }

    #[test]
    fn api_messages_keep_tool_history_content_compatible_with_chat_completions() {
        let session = AgentSession {
            id: "session-chat-content".to_string(),
            title: "Chat content".to_string(),
            model: "deepseek-chat".to_string(),
            api_preset_id: "api-deepseek".to_string(),
            profile_id: "agent_factory".to_string(),
            cwd: "/home/codes/webClx".to_string(),
            sandbox_mode: "default".to_string(),
            approval_policy: "ask_once".to_string(),
            system_prompt: None,
            context_summary: None,
            context_files: Vec::new(),
            compacted_messages: 0,
            compacted_at: None,
            last_token_usage: None,
            context_usage_source: "estimated".to_string(),
            active_run: None,
            run_interrupted_at: None,
            messages: vec![
                AgentMessage {
                    role: "assistant".to_string(),
                    content: None,
                    reasoning_content: Some("retain this reasoning".to_string()),
                    tool_calls: Some(vec![ToolCall {
                        id: "call-1".to_string(),
                        call_type: "function".to_string(),
                        function: ToolFunction {
                            name: "run_command".to_string(),
                            arguments: "{}".to_string(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                    created_at: None,
                },
                AgentMessage {
                    role: "tool".to_string(),
                    content: Some(json!({"stdout": "done"})),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some("call-1".to_string()),
                    name: Some("run_command".to_string()),
                    created_at: None,
                },
                test_message("user", Some("plain text")),
                AgentMessage {
                    role: "user".to_string(),
                    content: Some(json!([
                        {"type": "text", "text": "inspect image"},
                        {"type": "image_url", "image_url": {"url": "data:image/png;base64,AA=="}},
                    ])),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    created_at: None,
                },
                AgentMessage {
                    role: "tool".to_string(),
                    content: Some(Value::Null),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some("call-null".to_string()),
                    name: Some("read_file".to_string()),
                    created_at: None,
                },
                AgentMessage {
                    role: "tool".to_string(),
                    content: Some(Value::Bool(true)),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some("call-bool".to_string()),
                    name: Some("run_verification".to_string()),
                    created_at: None,
                },
                AgentMessage {
                    role: "tool".to_string(),
                    content: Some(Value::from(42)),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some("call-number".to_string()),
                    name: Some("read_background_command".to_string()),
                    created_at: None,
                },
            ],
            created_at: 1,
            updated_at: 1,
        };

        let messages = build_api_messages(&session, &[]);
        let history = &messages[1..];
        assert_eq!(history[0]["content"], "");
        assert_eq!(history[0]["reasoning_content"], "retain this reasoning");
        assert_eq!(history[1]["content"], r#"{"stdout":"done"}"#);
        assert_eq!(history[2]["content"], "plain text");
        assert!(history[3]["content"].is_array());
        assert_eq!(history[4]["content"], "");
        assert_eq!(history[5]["content"], "true");
        assert_eq!(history[6]["content"], "42");
        assert!(
            history
                .iter()
                .all(|message| { message["content"].is_string() || message["content"].is_array() })
        );
    }

    #[test]
    fn api_messages_drop_dangling_tool_calls_without_results() {
        let mut session = AgentSession {
            id: "session-dangling".to_string(),
            title: "Dangling".to_string(),
            model: "test-model".to_string(),
            api_preset_id: "api-test".to_string(),
            profile_id: "agent_factory".to_string(),
            cwd: "/home/codes/webClx".to_string(),
            sandbox_mode: "default".to_string(),
            approval_policy: "ask_once".to_string(),
            system_prompt: None,
            context_summary: None,
            context_files: Vec::new(),
            compacted_messages: 0,
            compacted_at: None,
            last_token_usage: None,
            context_usage_source: "estimated".to_string(),
            active_run: None,
            run_interrupted_at: None,
            messages: vec![
                AgentMessage {
                    role: "assistant".to_string(),
                    content: Some(Value::String("我来执行".to_string())),
                    reasoning_content: None,
                    tool_calls: Some(vec![
                        ToolCall {
                            id: "call-kept".to_string(),
                            call_type: "function".to_string(),
                            function: ToolFunction {
                                name: "run_command".to_string(),
                                arguments: "{}".to_string(),
                            },
                        },
                        ToolCall {
                            id: "call-dangling".to_string(),
                            call_type: "function".to_string(),
                            function: ToolFunction {
                                name: "apply_patch".to_string(),
                                arguments: "{}".to_string(),
                            },
                        },
                    ]),
                    tool_call_id: None,
                    name: None,
                    created_at: None,
                },
                AgentMessage {
                    role: "tool".to_string(),
                    content: Some(json!({"stdout": "ok"})),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some("call-kept".to_string()),
                    name: Some("run_command".to_string()),
                    created_at: None,
                },
                test_message("user", Some("你好")),
            ],
            created_at: 1,
            updated_at: 1,
        };

        let messages = build_api_messages(&session, &[]);
        let assistant = messages
            .iter()
            .find(|message| message["role"] == "assistant" && message["tool_calls"].is_array())
            .expect("assistant tool call message");
        let calls = assistant["tool_calls"].as_array().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["id"], "call-kept");

        let mut sessions = std::collections::HashMap::new();
        sessions.insert(session.id.clone(), session.clone());
        assert!(AgentManager::repair_message_pairs(&mut sessions));
        session = sessions
            .remove("session-dangling")
            .expect("repaired session");
        let repaired_calls = session
            .messages
            .iter()
            .find(|message| message.role == "assistant")
            .and_then(|message| message.tool_calls.as_ref())
            .expect("tool calls");
        assert_eq!(repaired_calls.len(), 1);
        assert_eq!(repaired_calls[0].id, "call-kept");
    }

    #[test]
    fn built_in_agent_exposes_background_command_sessions() {
        let definitions = tool_definitions();
        let names = definitions
            .iter()
            .filter_map(|definition| definition["function"]["name"].as_str())
            .collect::<std::collections::HashSet<_>>();
        for required in [
            "start_background_command",
            "read_background_command",
            "write_background_command",
            "terminate_background_command",
        ] {
            assert!(names.contains(required), "missing background command tool: {required}");
        }
    }

    fn test_message(role: &str, content: Option<&str>) -> AgentMessage {
        AgentMessage {
            role: role.to_string(),
            content: content.map(|value| Value::String(value.to_string())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
            created_at: None,
        }
    }

    #[test]
    fn agent_session_persists_preset_and_loads_legacy_sessions() {
        let session = AgentSession {
            id: "session-1".to_string(),
            title: "Preset session".to_string(),
            model: "MiniMax-M3".to_string(),
            api_preset_id: "api-minimax".to_string(),
            profile_id: "agent_factory".to_string(),
            cwd: "/home/codes/webClx".to_string(),
            sandbox_mode: "default".to_string(),
            approval_policy: "ask_once".to_string(),
            system_prompt: None,
            context_summary: None,
            context_files: Vec::new(),
            compacted_messages: 0,
            compacted_at: None,
            last_token_usage: None,
            context_usage_source: "estimated".to_string(),
            active_run: None,
            run_interrupted_at: None,
            messages: Vec::new(),
            created_at: 1,
            updated_at: 1,
        };
        let encoded = serde_json::to_value(&session).expect("serialize agent session");
        assert_eq!(encoded["api_preset_id"], "api-minimax");
        assert_eq!(encoded["profile_id"], "agent_factory");
        assert_eq!(encoded["cwd"], "/home/codes/webClx");

        let legacy: AgentSession = serde_json::from_value(json!({
            "id": "legacy-session",
            "title": "Legacy",
            "model": "gpt-5.6-sol",
            "system_prompt": null,
            "messages": [],
            "created_at": 1,
            "updated_at": 1
        }))
        .expect("legacy session remains readable");
        assert!(legacy.api_preset_id.is_empty());
        assert!(legacy.profile_id.is_empty());
        assert!(legacy.cwd.is_empty());
        assert!(legacy.last_token_usage.is_none());
        assert_eq!(legacy.context_usage_source, "estimated");
    }

    #[tokio::test]
    async fn agent_manager_repairs_stale_runs_inside_tokio_runtime() {
        let app_dir =
            std::env::temp_dir().join(format!("webclx-agent-manager-startup-{}", generate_id()));
        std::fs::create_dir_all(&app_dir).expect("create agent manager fixture");
        let sessions_path = app_dir.join(AGENT_SESSIONS_FILE);
        std::fs::write(
            &sessions_path,
            serde_json::to_vec(&json!({
                "session-1": {
                    "id": "session-1",
                    "title": "Interrupted run",
                    "model": "test-model",
                    "messages": [],
                    "active_run": {"run_id": "run-1", "started_at": 1},
                    "created_at": 1,
                    "updated_at": 1
                }
            }))
            .expect("serialize stale session"),
        )
        .expect("write stale session");

        let _manager = AgentManager::new(&app_dir);
        let persisted: Value =
            serde_json::from_slice(&std::fs::read(&sessions_path).expect("read repaired sessions"))
                .expect("parse repaired sessions");
        assert!(persisted["session-1"]["active_run"].is_null());
        assert!(persisted["session-1"]["run_interrupted_at"].is_number());

        std::fs::remove_dir_all(&app_dir).expect("remove agent manager fixture");
    }

    #[tokio::test]
    async fn agent_event_stream_is_fused_after_channel_closes() {
        let (tx, rx) = mpsc::channel(1);
        drop(tx);
        let stream = receiver_to_event_stream(rx);
        futures_util::pin_mut!(stream);
        assert!(futures_util::StreamExt::next(&mut stream).await.is_none());
        assert!(futures_util::StreamExt::next(&mut stream).await.is_none());
    }

    #[test]
    fn skill_resolution_supports_extra_dirs_and_rejects_unsafe_paths() {
        let temp_root =
            std::env::temp_dir().join(format!("webclx-agent-skill-resolution-{}", generate_id()));
        let primary = temp_root.join("primary");
        let extra = temp_root.join("extra");
        let skill_dir = extra.join("project-skill");
        std::fs::create_dir_all(&primary).expect("create primary skills dir");
        std::fs::create_dir_all(skill_dir.join("scripts")).expect("create extra skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "# Project skill\n")
            .expect("write skill document");
        std::fs::write(skill_dir.join("scripts/check.sh"), "#!/bin/sh\n")
            .expect("write skill script");

        let resolved = resolve_skill_dir_from_roots(
            &primary,
            &[extra.to_string_lossy().to_string()],
            &[],
            "project-skill",
        )
        .expect("resolve project skill from extra dir");
        assert_eq!(resolved, skill_dir);
        assert_eq!(
            resolve_skill_script_path(&resolved, "scripts/check.sh")
                .expect("resolve script inside the skill scripts directory"),
            skill_dir.join("scripts/check.sh"),
        );
        assert!(
            resolve_skill_dir_from_roots(&primary, &[], &[], "../outside").is_err(),
            "skill names must not escape their configured root",
        );
        assert!(
            resolve_skill_dir_from_roots(
                &primary,
                &[extra.to_string_lossy().to_string()],
                &["project-skill".to_string()],
                "project-skill",
            )
            .is_err(),
            "disabled skills must not be directly readable",
        );
        assert!(
            resolve_skill_script_path(&resolved, "../SKILL.md").is_err(),
            "skill scripts must remain inside the scripts directory",
        );
        assert!(
            resolve_skill_script_path(&resolved, "SKILL.md").is_err(),
            "only files below scripts may be executed",
        );

        let primary_skill_dir = primary.join("project-skill");
        std::fs::create_dir_all(&primary_skill_dir).expect("create primary skill dir");
        std::fs::write(primary_skill_dir.join("SKILL.md"), "# Primary skill\n")
            .expect("write primary skill document");
        assert_eq!(
            resolve_skill_dir_from_roots(
                &primary,
                &[extra.to_string_lossy().to_string()],
                &[],
                "project-skill",
            )
            .expect("prefer the user skill directory"),
            primary_skill_dir,
        );

        std::fs::remove_dir_all(&temp_root).expect("remove skill fixture");
    }
}
