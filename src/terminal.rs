use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result};
use auth_core::{
    AUTH_FILE_RELATIVE_PATH, CONFIG_FILE_RELATIVE_PATH, CurrentAuthMode, StoredApiPreset,
    api_preset_summary, api_preset_summary_with_proxy_state, api_provider_base_url,
    derive_current_api_state, derive_current_mode, read_current_auth_state,
    read_current_config_provider,
};
use axum::{
    Json,
    body::Body,
    extract::{
        ConnectInfo, Multipart, Path as AxumPath, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, header},
    response::Response,
};
use futures_util::{Sink, SinkExt, StreamExt};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use settings_core::TerminalQuickCommand;
use terminal_core::*;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::{ApiResult, AppError, AppState, filesystem, runtime_paths};

mod activity;
mod agent_session;
mod codex_status;
mod docs;
mod manager;
mod session;
mod tmux;

pub use docs::{list_session_agents_docs, read_session_agents_doc, save_session_agents_doc};
use manager::clamp_input_history_entries;
use session::{TerminalOutputChunk, TerminalSession};
#[cfg(not(windows))]
use tmux::capture_tmux_initial_pane_snapshot;

#[cfg(test)]
use manager::{
    collect_session_infos_locked, ensure_unique_session_name_locked,
    refresh_auto_session_names_for_path_locked, should_notify_session_list_sync,
    sort_session_ids_by_recent_activity,
};

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 32;
const MAX_BACKLOG_BYTES: usize = 32 * 1024 * 1024;
const MAX_INITIAL_BACKLOG_BYTES: usize = 1024 * 1024;
const SESSION_EVENT_CHANNEL_CAPACITY: usize = 256;
const INITIAL_TMUX_REDRAW_SUPPRESS_MS: u64 = 160;
const TERMINAL_RECENT_OUTPUT_ACTIVE_MS: u64 = 15_000;
const MAX_TERMINAL_INPUT_HISTORY_ENTRIES: usize = 500;
const MAX_TERMINAL_INPUT_HISTORY_LINE_BYTES: usize = 16 * 1024;
const MAX_CODEX_CONVERSATION_SCAN_LINES: usize = 512;
const MAX_CODEX_CONVERSATION_TITLE_MESSAGES: usize = 8;
const MAX_CODEX_CONVERSATION_TITLE_CHARS: usize = 2048;
const PASTE_ASSET_DIR_NAME: &str = ".webclx-paste";
const SHUTDOWN_CTRL_C_DELAY_MS: u64 = 120;
const SHUTDOWN_CTRL_C_CLAUDE_TOTAL_COUNT: usize = 3;
const TERMINAL_MESSAGE_VERIFY_POLL_MS: u64 = 100;
const TERMINAL_MESSAGE_VERIFY_POLLS: usize = 20;
const TERMINAL_MESSAGE_SUBMIT_RETRY_DELAYS_MS: [u64; 3] = [1_000, 2_000, 4_000];
const TERMINAL_DELETE_CONFIRM_HEADER: &str = "x-webclx-confirm-session";
const TERMINAL_DELETE_SOURCE_HEADER: &str = "x-webclx-delete-source";
const TERMINAL_DELETE_AUDIT_HEADER_MAX_CHARS: usize = 256;
const MAX_PASTE_ASSET_BYTES: usize = 12 * 1024 * 1024;
const MAX_PASTE_ASSET_COUNT: usize = 12;
const TERMINAL_SCHEDULED_INPUT_FILE_NAME: &str = ".webclx-terminal-scheduled-inputs.json";
const TERMINAL_AUTO_CONTINUE_SCHEDULE_FILE_NAME: &str =
    ".webclx-terminal-auto-continue-schedules.json";
const TERMINAL_PENDING_BUILD_FILE_NAME: &str = ".webclx-terminal-pending-builds.json";
const TERMINAL_PENDING_BUILD_MAX_AGE_MS: u64 = 24 * 60 * 60 * 1000;
const MAX_TERMINAL_SCHEDULED_INPUT_BYTES: usize = 512 * 1024;
const COMPLETION_BELL_SAMPLE_RATE: u32 = 22_050;
const COMPLETION_BELL_DURATION_MS: u32 = 420;
const CHILD_PROCESS_ENV_KEYS_TO_CLEAR: [&str; 25] = [
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "LIBRARY_PATH",
    "HOME",
    "WEBCLX_USER_HOME",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    auth_core::WEBCLX_LOCAL_API_TOKEN_ENV,
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
];
const TERMINAL_SESSION_ENV_KEYS_TO_CLEAR: [&str; 25] = [
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "LIBRARY_PATH",
    "HOME",
    "WEBCLX_USER_HOME",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    auth_core::WEBCLX_LOCAL_API_TOKEN_ENV,
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
];
#[derive(Clone)]
pub struct TerminalManager {
    state: Arc<RwLock<TerminalState>>,
    state_file: Arc<PathBuf>,
    archive_file: Arc<PathBuf>,
    scheduled_input_file: Arc<PathBuf>,
    auto_continue_file: Arc<PathBuf>,
    pending_build_file: Arc<PathBuf>,
    shutdown_restore_file: Arc<PathBuf>,
    env_snapshot: Arc<TerminalEnvironmentSnapshot>,
    next_id: Arc<AtomicU64>,
    event_sender: broadcast::Sender<TerminalManagerEvent>,
    auto_continue_schedules: Arc<Mutex<HashMap<String, TerminalAutoContinueTask>>>,
    auto_continue_notify: Arc<tokio::sync::Notify>,
    canceled_auto_continue_signatures: Arc<Mutex<HashSet<String>>>,
    error_auto_continue_records: Arc<Mutex<HashMap<String, TerminalErrorAutoContinueRecord>>>,
    auto_continue_last_sent_at: Arc<Mutex<HashMap<String, u64>>>,
    pending_build_requests: Arc<Mutex<HashMap<String, TerminalPendingBuildRequest>>>,
    auto_continue_interval_seconds: Arc<AtomicU64>,
    auto_continue_backoff_factor: Arc<AtomicU64>,
    auto_continue_backoff_max_millis: Arc<AtomicU64>,
    auto_continue_respect_manual_interrupt: Arc<AtomicBool>,
    quota_reset_cache: crate::quota_reset_cache::QuotaResetCache,
    api_preset_snapshot: Arc<RwLock<Vec<StoredApiPreset>>>,
    scheduled_input_tasks: Arc<Mutex<HashMap<String, TerminalScheduledInputTask>>>,
    scheduled_input_notify: Arc<tokio::sync::Notify>,
    activity_probe_cache: Arc<Mutex<manager::TerminalActivityProbeCache>>,
    activity_probe_scan_lock: Arc<Mutex<()>>,
}

#[derive(Default)]
struct TerminalState {
    sessions_by_id: HashMap<String, StoredTerminalSession>,
    sessions_by_path: HashMap<PathBuf, Vec<String>>,
    live_sessions: HashMap<String, Arc<TerminalSession>>,
    output_observations: HashMap<String, TerminalOutputObservation>,
    input_histories: HashMap<String, TerminalInputHistoryCapture>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct TerminalOutputObservation {
    #[serde(default)]
    fingerprint: Option<u64>,
    #[serde(skip)]
    last_fingerprint_probe_sequence: u64,
    #[serde(skip)]
    rebaseline_after_restore: bool,
    #[serde(default)]
    last_output_at: u64,
    #[serde(default)]
    last_viewed_output_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TerminalPendingBuildRequest {
    request_id: String,
    session_id: String,
    queued_at_millis: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTerminalPendingBuildRegistry {
    #[serde(default)]
    requests: Vec<TerminalPendingBuildRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TerminalAutoContinueSchedule {
    signature: String,
    keyword: String,
    reset_at: String,
    due_at_millis: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTerminalAutoContinueScheduleRegistry {
    #[serde(default)]
    tasks: Vec<TerminalAutoContinueTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TerminalAutoContinueTask {
    session_id: String,
    #[serde(default)]
    terminal_name: String,
    schedule: TerminalAutoContinueSchedule,
    backend_due_at_millis: u64,
    created_at_millis: u64,
    error_line_limit: u32,
    #[serde(default = "default_auto_continue_interval_seconds")]
    auto_continue_interval_seconds: u32,
    #[serde(default = "default_auto_continue_respect_manual_interrupt")]
    auto_continue_respect_manual_interrupt: bool,
    #[serde(default)]
    error_keywords: Vec<String>,
    #[serde(default)]
    auto_continue_time_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalErrorAutoContinueRecord {
    key: String,
    sent_at_millis: u64,
    reset_at: String,
    // Number of consecutive auto-continue attempts for the same error key.
    // Drives exponential backoff so repeated failures don't retry at the
    // flat interval forever. Reset to 0 when the error clears.
    consecutive_attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalAutoContinueSendOutcome {
    Sent,
    /// `/compact` was sent followed by "继续" because the context window was
    /// exhausted. Distinct from `Sent` so the frontend can show a precise
    /// status message.
    CompactSent,
    Cooldown {
        last_sent_at_millis: u64,
        retry_at_millis: u64,
    },
    NotEligible,
}

fn default_auto_continue_interval_seconds() -> u32 {
    settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS
}

fn default_auto_continue_respect_manual_interrupt() -> bool {
    settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTerminalScheduledInputRegistry {
    #[serde(default)]
    tasks: Vec<TerminalScheduledInputTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TerminalScheduledInputTask {
    id: String,
    session_id: String,
    #[serde(default)]
    terminal_name: String,
    due_at_millis: u64,
    created_at_millis: u64,
    #[serde(default)]
    label: String,
    text: String,
    #[serde(default = "default_true")]
    send_enter: bool,
    #[serde(default)]
    task_type: String,
    #[serde(default)]
    working_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TerminalScheduledInputTaskInfo {
    pub(crate) id: String,
    pub(crate) task_id: String,
    pub(crate) session_id: String,
    pub(crate) terminal_name: String,
    pub(crate) due_at: u64,
    pub(crate) due_at_millis: u64,
    pub(crate) created_at_millis: u64,
    pub(crate) label: String,
    pub(crate) preview: String,
    pub(crate) text: String,
    pub(crate) send_enter: bool,
    pub(crate) task_type: String,
    pub(crate) working_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalManagerEvent {
    SessionListChanged {
        action: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        session_id: String,
    },
    Toast {
        session_id: String,
        message: String,
        tone: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename = "terminal_backlog_replay")]
#[serde(tag = "type", rename_all = "snake_case")]
struct TerminalBacklogReplayControl<'a> {
    action: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename = "terminal_connection_error")]
#[serde(tag = "type", rename_all = "snake_case")]
struct TerminalConnectionError<'a> {
    message: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTerminalRegistry {
    #[serde(default = "default_next_ordinal")]
    next_ordinal: u64,
    #[serde(default)]
    sessions: Vec<StoredTerminalSession>,
    #[serde(default)]
    input_histories: HashMap<String, Vec<TerminalInputHistoryEntry>>,
    #[serde(default)]
    output_observations: HashMap<String, TerminalOutputObservation>,
}

impl Default for StoredTerminalRegistry {
    fn default() -> Self {
        Self {
            next_ordinal: default_next_ordinal(),
            sessions: Vec::new(),
            input_histories: HashMap::new(),
            output_observations: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TerminalSessionOrigin {
    #[default]
    Normal,
    Workflow,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTerminalSession {
    id: String,
    path: PathBuf,
    #[serde(default = "default_terminal_user_name")]
    user_name: String,
    name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    codex_api_preset_name: String,
    #[serde(default)]
    codex_api_base_url: String,
    #[serde(default)]
    origin: TerminalSessionOrigin,
    #[serde(default)]
    owner_key: String,
    #[serde(default)]
    manually_renamed: bool,
    #[serde(default)]
    idle: bool,
    #[serde(default)]
    created_at: u64,
    #[serde(default)]
    last_opened_at: u64,
}

impl StoredTerminalSession {
    fn new(
        id: String,
        path: PathBuf,
        user_name: String,
        name: String,
        codex_api_preset_name: String,
        codex_api_base_url: String,
        origin: TerminalSessionOrigin,
        owner_key: String,
    ) -> Self {
        let timestamp = current_timestamp_millis();
        Self {
            id,
            path,
            user_name,
            name,
            title: String::new(),
            codex_api_preset_name,
            codex_api_base_url,
            origin,
            owner_key,
            manually_renamed: false,
            idle: false,
            created_at: timestamp,
            last_opened_at: timestamp,
        }
    }

    fn info(
        &self,
        base_dir: &Path,
        display_root: &Path,
        activity: TerminalActivitySnapshot,
        connected: bool,
    ) -> TerminalSessionInfo {
        let relative = filesystem::relative_path(base_dir, &self.path).unwrap_or_default();
        TerminalSessionInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            title: self.title(),
            user_name: self.user_name.clone(),
            codex_api_preset_name: self.codex_api_preset_name.clone(),
            codex_api_base_url: self.codex_api_base_url.clone(),
            origin: self.origin,
            owner_key: self.owner_key.clone(),
            path: relative_to_string(&relative),
            display_path: filesystem::display_path(base_dir, display_root, &self.path),
            alive: true,
            connected,
            busy: activity.busy,
            activity_state: activity.state,
            activity_label: activity.label,
            activity_agent: activity.agent,
            activity_error_keyword: activity.error_keyword,
            activity_error_signature: activity.error_signature,
            activity_error_continue_sent: activity.error_continue_sent,
            activity_error_input_queued: activity.error_input_queued,
            activity_error_auto_continue_at: activity.error_auto_continue_at,
            last_output_at: activity.last_output_at,
            idle: self.idle,
            created_at: self.created_at,
            last_opened_at: self.last_opened_at,
        }
    }

    fn title(&self) -> Option<String> {
        normalize_session_title(&self.title)
    }
}

#[derive(Debug, Clone)]
struct TerminalActivitySnapshot {
    busy: bool,
    state: String,
    label: String,
    agent: Option<String>,
    error_keyword: Option<String>,
    error_signature: Option<String>,
    error_continue_sent: bool,
    error_input_queued: bool,
    error_auto_continue_at: Option<String>,
    last_output_at: u64,
}

impl TerminalActivitySnapshot {
    fn idle(last_output_at: u64) -> Self {
        Self {
            busy: false,
            state: "idle".to_string(),
            label: "空闲".to_string(),
            agent: None,
            error_keyword: None,
            error_signature: None,
            error_continue_sent: false,
            error_input_queued: false,
            error_auto_continue_at: None,
            last_output_at,
        }
    }

    fn recent_output(last_output_at: u64) -> Self {
        Self {
            busy: true,
            state: "recent_output".to_string(),
            label: "输出中".to_string(),
            agent: None,
            error_keyword: None,
            error_signature: None,
            error_continue_sent: false,
            error_input_queued: false,
            error_auto_continue_at: None,
            last_output_at,
        }
    }

    fn completed(last_output_at: u64) -> Self {
        Self {
            busy: false,
            state: "completed".to_string(),
            label: "待查看".to_string(),
            agent: None,
            error_keyword: None,
            error_signature: None,
            error_continue_sent: false,
            error_input_queued: false,
            error_auto_continue_at: None,
            last_output_at,
        }
    }

    fn building(last_output_at: u64) -> Self {
        Self {
            busy: true,
            state: "building".to_string(),
            label: "编译中".to_string(),
            agent: None,
            error_keyword: None,
            error_signature: None,
            error_continue_sent: false,
            error_input_queued: false,
            error_auto_continue_at: None,
            last_output_at,
        }
    }

    fn working(last_output_at: u64) -> Self {
        Self {
            busy: true,
            state: "working".to_string(),
            label: "工作中".to_string(),
            agent: None,
            error_keyword: None,
            error_signature: None,
            error_continue_sent: false,
            error_input_queued: false,
            error_auto_continue_at: None,
            last_output_at,
        }
    }

    fn error(
        keyword: String,
        signature: String,
        continue_sent: bool,
        input_queued: bool,
        auto_continue_at: Option<String>,
        last_output_at: u64,
    ) -> Self {
        Self {
            busy: true,
            state: "error".to_string(),
            label: "错误".to_string(),
            agent: None,
            error_keyword: Some(keyword),
            error_signature: Some(signature),
            error_continue_sent: continue_sent,
            error_input_queued: input_queued,
            error_auto_continue_at: auto_continue_at,
            last_output_at,
        }
    }

    fn retrying(
        keyword: String,
        signature: String,
        auto_continue_at: Option<String>,
        last_output_at: u64,
    ) -> Self {
        Self {
            busy: true,
            state: "retrying".to_string(),
            label: "重试中".to_string(),
            agent: None,
            error_keyword: Some(keyword),
            error_signature: Some(signature),
            error_continue_sent: true,
            error_input_queued: false,
            error_auto_continue_at: auto_continue_at,
            last_output_at,
        }
    }

    fn agent(label: String, agent: Option<String>, last_output_at: u64) -> Self {
        Self {
            busy: true,
            state: "agent".to_string(),
            label,
            agent,
            error_keyword: None,
            error_signature: None,
            error_continue_sent: false,
            error_input_queued: false,
            error_auto_continue_at: None,
            last_output_at,
        }
    }

    fn with_agent(mut self, agent: Option<String>) -> Self {
        self.agent = agent;
        self
    }
}

pub async fn completion_bell_sound() -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "audio/wav")
        .header(header::CACHE_CONTROL, "no-store, max-age=0, must-revalidate")
        .body(Body::from(completion_bell_wav_bytes()))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn completion_bell_wav_bytes() -> Vec<u8> {
    let sample_count = (COMPLETION_BELL_SAMPLE_RATE * COMPLETION_BELL_DURATION_MS / 1000) as usize;
    let data_len = (sample_count * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);

    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&COMPLETION_BELL_SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(COMPLETION_BELL_SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());

    for sample_index in 0..sample_count {
        let t = sample_index as f32 / COMPLETION_BELL_SAMPLE_RATE as f32;
        let duration = COMPLETION_BELL_DURATION_MS as f32 / 1000.0;
        let frequency = if t < 0.18 { 880.0 } else { 1174.66 };
        let attack = (t / 0.025).min(1.0);
        let release = ((duration - t) / 0.12).clamp(0.0, 1.0);
        let envelope = attack * release;
        let phase = 2.0 * std::f32::consts::PI * frequency * t;
        let sample = (phase.sin() * envelope * 0.28 * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    bytes
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
    Visibility { visible: bool },
}

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    all: bool,
}

#[derive(Debug, Deserialize)]
pub struct CodexConversationsQuery {
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchSessionsQuery {
    #[serde(default)]
    q: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalSessionQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default = "default_true")]
    visible: bool,
}

/// Fields that may be changed on an existing scheduled input task. Any field
/// left as `None` is left untouched on the stored task.
#[derive(Debug, Clone, Default)]
pub(crate) struct ScheduledInputUpdate {
    pub(crate) due_at: Option<u64>,
    pub(crate) text: Option<String>,
    pub(crate) send_enter: Option<bool>,
    pub(crate) task_type: Option<String>,
    pub(crate) session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    path: String,
    #[serde(default)]
    origin: TerminalSessionOrigin,
    #[serde(default)]
    owner_key: String,
    #[serde(default)]
    codex_api_preset_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameSessionRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionOriginRequest {
    origin: TerminalSessionOrigin,
    #[serde(default)]
    owner_key: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalInputRequest {
    data: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalScheduledInputRequest {
    #[serde(default)]
    session_id: String,
    text: String,
    due_at: u64,
    #[serde(default)]
    label: String,
    #[serde(default = "default_true")]
    send_enter: bool,
    /// "existing" (default) to send to an existing terminal, "new" to create
    /// a fresh terminal at schedule time.
    #[serde(default)]
    terminal_mode: String,
    /// Required when terminal_mode == "new": the working directory for the
    /// new terminal.
    #[serde(default)]
    working_dir: String,
    /// Task type label shown in the unified table, e.g. "paste", "command",
    /// "continue". Defaults to "paste".
    #[serde(default)]
    task_type: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalScheduledInputUpdateRequest {
    #[serde(default)]
    due_at: Option<u64>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    send_enter: Option<bool>,
    #[serde(default)]
    task_type: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TerminalScheduledInputListResponse {
    tasks: Vec<TerminalScheduledInputTaskInfo>,
}

#[derive(Debug, Serialize)]
pub struct TerminalScheduledInputResponse {
    ok: bool,
    task: TerminalScheduledInputTaskInfo,
    tasks: Vec<TerminalScheduledInputTaskInfo>,
}

#[derive(Debug, Deserialize)]
pub struct TerminalQuickCommandRequest {
    command_line: String,
    #[serde(default)]
    session_id: String,
}

#[derive(Debug, Serialize)]
pub struct TerminalQuickCommandResponse {
    data: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalMessageRequest {
    #[serde(default)]
    target: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    terminal_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    data: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    enter: bool,
    #[serde(default)]
    submit: bool,
    #[serde(default)]
    submit_enters: u8,
    #[serde(default)]
    bracketed_paste: bool,
    #[serde(default)]
    verify_submission: bool,
    #[serde(default)]
    delivery_id: String,
    #[serde(default)]
    completed_build_request_id: String,
}

/// Request body for `/api/terminal/auto-typed-input`. Used for commands
/// that webClx itself injects into the terminal (initial session launch,
/// quick-start, `reload_claude`, resume-command injection) which must
/// reach the pane but should NOT be recorded in the "本终端对话历史" panel.
#[derive(Debug, Deserialize)]
pub struct TerminalAutoTypedInputRequest {
    command_line: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    terminal_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    /// Number of trailing Enter keys to send after the command line.
    /// The prepared string from `build_terminal_quick_command_input`
    /// already ends in one `\n`, so this is only needed when the
    /// caller wants extra submits.
    #[serde(default)]
    submit_enters: u8,
}

#[derive(Debug, Serialize)]
pub struct TerminalAutoTypedInputResponse {
    /// The fully prepared string (PATH/hash prefixes for claude etc.
    /// already applied, plus the trailing newline). Echoed so the
    /// caller's status bar can show "已启动：<command>".
    data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalInputHistoryEntry {
    text: String,
    created_at: u64,
}

#[derive(Debug, Clone, Default)]
struct TerminalInputHistoryCapture {
    buffer: String,
    entries: Vec<TerminalInputHistoryEntry>,
}

#[derive(Debug, Serialize)]
pub struct TerminalInputHistoryResponse {
    entries: Vec<TerminalInputHistoryEntry>,
}

#[derive(Debug, Serialize)]
pub struct TerminalPasteAssetInfo {
    name: String,
    path: String,
    relative_path: String,
    markdown: String,
    mime: String,
    size: usize,
}

#[derive(Debug, Serialize)]
pub struct TerminalPasteAssetsResponse {
    assets: Vec<TerminalPasteAssetInfo>,
}

#[derive(Debug, Serialize)]
pub struct TerminalSessionInfo {
    id: String,
    name: String,
    title: Option<String>,
    user_name: String,
    codex_api_preset_name: String,
    codex_api_base_url: String,
    origin: TerminalSessionOrigin,
    owner_key: String,
    path: String,
    display_path: String,
    alive: bool,
    connected: bool,
    busy: bool,
    activity_state: String,
    activity_label: String,
    activity_agent: Option<String>,
    activity_error_keyword: Option<String>,
    activity_error_signature: Option<String>,
    activity_error_continue_sent: bool,
    activity_error_input_queued: bool,
    activity_error_auto_continue_at: Option<String>,
    last_output_at: u64,
    idle: bool,
    created_at: u64,
    last_opened_at: u64,
}

impl TerminalSessionInfo {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Serialize)]
pub struct TerminalPresetExtractionResponse {
    session_id: String,
    preset_name: String,
    base_url: String,
    provider: String,
}

#[derive(Debug, Serialize)]
pub struct TerminalSessionsResponse {
    all: bool,
    path: String,
    display_path: String,
    sessions: Vec<TerminalSessionInfo>,
}

#[derive(Debug, Serialize)]
pub struct CurrentTerminalDirectoryResponse {
    path: String,
    display_path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TerminalAutoContinueTaskInfo {
    marker: String,
    task_kind: String,
    task_label: String,
    session_id: String,
    session_name: Option<String>,
    webclx_terminal_name: Option<String>,
    tmux_session_name: String,
    signature: String,
    schedule: String,
    command: String,
    script_path: Option<String>,
    script_exists: bool,
    /// Intended fire epoch (seconds), parsed from the
    /// `webclx-auto-continue-due:` metadata line written at install time.
    /// None for legacy entries that predate the metadata.
    #[serde(default)]
    due_epoch: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpiredAutoContinueTaskInfo {
    pub marker: String,
    pub session_id: String,
    pub session_name: Option<String>,
    pub webclx_terminal_name: Option<String>,
    pub tmux_session_name: String,
    pub signature: String,
    pub schedule: String,
    pub expired_at: i64,
}

#[derive(Debug, Serialize)]
pub struct TerminalAutoContinueTasksResponse {
    auto_continue_tasks: Vec<TerminalAutoContinueTaskInfo>,
    expired_tasks: Vec<ExpiredAutoContinueTaskInfo>,
    crontab_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TerminalAutoContinueTaskUpdateRequest {
    due_at: u64,
}

#[derive(Debug, Serialize)]
pub struct TerminalAutoContinueSendResponse {
    ok: bool,
    sent: bool,
    retry_after_millis: u64,
    /// True when `/compact` was sent before "继续" because the context window
    /// was exhausted. Lets the frontend show a precise status message.
    #[serde(default)]
    compact_sent: bool,
}

#[derive(Debug, Serialize)]
pub struct TerminalSessionSearchMatch {
    session_id: String,
    session_name: String,
    title: Option<String>,
    path: String,
    display_path: String,
    line_number: usize,
    line: String,
    match_count: usize,
}

#[derive(Debug, Serialize)]
pub struct TerminalSessionSearchResponse {
    query: String,
    matches: Vec<TerminalSessionSearchMatch>,
}

#[derive(Debug, Serialize)]
pub struct CodexResumeArchivesResponse {
    archives: Vec<CodexResumeArchive>,
}

pub const TERMINAL_SHUTDOWN_RESTORE_FILE_NAME: &str = ".webclx-terminal-shutdown-restores.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalResumeRestoreRecord {
    pub session_id: String,
    pub path: PathBuf,
    pub user_name: String,
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub codex_api_preset_name: String,
    #[serde(default)]
    pub codex_api_base_url: String,
    #[serde(default)]
    pub origin: TerminalSessionOrigin,
    #[serde(default)]
    pub owner_key: String,
    #[serde(default)]
    pub manually_renamed: bool,
    #[serde(default)]
    pub idle: bool,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub last_opened_at: u64,
    #[serde(default)]
    pub input_history: Vec<TerminalInputHistoryEntry>,
    pub resume_id: String,
    pub command: String,
    pub program: String,
    pub source: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalShutdownRestoreRegistry {
    #[serde(default)]
    pub records: Vec<TerminalResumeRestoreRecord>,
}

impl Default for TerminalResumeRestoreRecord {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            path: PathBuf::new(),
            user_name: String::new(),
            name: String::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 0,
            last_opened_at: 0,
            input_history: Vec::new(),
            resume_id: String::new(),
            command: String::new(),
            program: String::new(),
            source: String::new(),
            updated_at: 0,
        }
    }
}

pub fn load_terminal_shutdown_restore_registry(
    path: &Path,
) -> anyhow::Result<TerminalShutdownRestoreRegistry> {
    if !path.exists() {
        return Ok(TerminalShutdownRestoreRegistry::default());
    }

    let content = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut registry: TerminalShutdownRestoreRegistry = serde_json::from_slice(&content)
        .with_context(|| format!("cannot decode {}", path.display()))?;
    normalize_terminal_shutdown_restore_registry(&mut registry);
    sort_terminal_shutdown_restore_records(&mut registry.records);
    Ok(registry)
}

pub fn persist_terminal_shutdown_restore_registry(
    path: &Path,
    registry: &TerminalShutdownRestoreRegistry,
) -> anyhow::Result<()> {
    let content =
        serde_json::to_vec_pretty(registry).context("cannot encode terminal shutdown restores")?;
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn normalize_terminal_shutdown_restore_registry(registry: &mut TerminalShutdownRestoreRegistry) {
    registry.records.retain_mut(|record| {
        record.session_id = record.session_id.trim().to_string();
        record.name = record.name.trim().to_string();
        record.title = record.title.trim().to_string();
        record.user_name = record.user_name.trim().to_string();
        record.codex_api_preset_name = record.codex_api_preset_name.trim().to_string();
        record.codex_api_base_url = record.codex_api_base_url.trim().to_string();
        record.resume_id = record.resume_id.trim().to_string();
        record.command = record.command.trim().to_string();
        record.program = record.program.trim().to_string();
        record.source = record.source.trim().to_string();
        record.input_history = clamp_input_history_entries(record.input_history.clone());
        record.updated_at = record.updated_at.max(record.created_at);
        !record.session_id.is_empty()
            && !record.name.is_empty()
            && !record.user_name.is_empty()
            && !record.resume_id.is_empty()
            && !record.command.is_empty()
    });
}

pub fn sort_terminal_shutdown_restore_records(records: &mut [TerminalResumeRestoreRecord]) {
    records.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
}

#[derive(Debug, Serialize)]
pub struct CodexConversationInfo {
    session_id: String,
    cwd: String,
    title: String,
    file_path: String,
    size_bytes: u64,
    updated_at: u64,
    created_at: u64,
}

#[derive(Debug, Serialize)]
pub struct CodexConversationsResponse {
    codex_home: String,
    conversations: Vec<CodexConversationInfo>,
}

#[derive(Debug, Serialize)]
pub struct DeleteCodexConversationResponse {
    session_id: String,
    rollout_files_deleted: usize,
    session_index_entries_deleted: usize,
    history_entries_deleted: usize,
    database_threads_deleted: usize,
    resume_archive_deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct CurrentAgentSessionResponse {
    resume_id: Option<String>,
    command: Option<String>,
    source: String,
    codex_status: Option<codex_status::CodexCompactStatus>,
}

#[derive(Debug, Serialize)]
pub struct TerminalInterruptResumeResponse {
    ok: bool,
    outcome: String,
    resume_id: String,
    program: String,
    command: String,
    interrupted_processes: usize,
}

#[derive(Debug, Serialize)]
pub struct TerminalAgentsDocResponse {
    path: String,
    display_path: String,
    exists: bool,
    content: String,
    documents: Vec<TerminalAgentsDocItem>,
}

#[derive(Debug, Deserialize)]
pub struct TerminalAgentsDocSaveRequest {
    #[serde(default)]
    path: String,
    content: String,
    #[serde(default)]
    show_hidden: bool,
    #[serde(default)]
    recursive_dirs: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalAgentsDocPathQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    show_hidden: bool,
    #[serde(default)]
    recursive_dirs: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TerminalAgentsDocItem {
    path: String,
    display_path: String,
    label: String,
    exists: bool,
    modified: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TerminalAgentsDocListResponse {
    documents: Vec<TerminalAgentsDocItem>,
}

struct CurrentApiTerminalStartup {
    codex_api_preset_name: String,
    codex_api_base_url: String,
}

impl CurrentApiTerminalStartup {
    fn empty() -> Self {
        Self {
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
        }
    }
}

fn api_terminal_startup_for_preset(
    presets: &[StoredApiPreset],
    preset_id: &str,
) -> Option<CurrentApiTerminalStartup> {
    let preset_id = preset_id.trim();
    if preset_id.is_empty() {
        return None;
    }
    presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .map(|preset| CurrentApiTerminalStartup {
            codex_api_preset_name: preset.name.clone(),
            codex_api_base_url: preset.base_url.clone(),
        })
}

#[derive(Debug, Clone)]
pub(crate) struct TerminalEnvironmentSnapshot {
    pub workspace_root: PathBuf,
    pub display_root: PathBuf,
    pub user_profile: runtime_paths::UserProfile,
    pub terminal_default_env: Vec<(String, String)>,
    pub proxy_env: Vec<(String, String)>,
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<ListSessionsQuery>,
) -> ApiResult<Json<TerminalSessionsResponse>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let api_presets = state.auth_manager.api_presets_snapshot();
    let error_line_limit = state.workspace_settings.terminal_error_match_line_limit();
    let error_keywords = state.workspace_settings.terminal_error_keywords();
    let auto_continue_time_patterns = state
        .workspace_settings
        .terminal_auto_continue_time_patterns();
    let auto_continue_interval_seconds = state
        .workspace_settings
        .terminal_auto_continue_interval_seconds();
    let auto_continue_respect_manual_interrupt = state
        .workspace_settings
        .terminal_auto_continue_respect_manual_interrupt();
    let auto_continue_backoff_factor = state
        .workspace_settings
        .terminal_auto_continue_backoff_factor();
    let auto_continue_backoff_max_minutes = state
        .workspace_settings
        .terminal_auto_continue_backoff_max_minutes();

    if query.all {
        return run_terminal_task(move || {
            terminal_manager.update_api_preset_snapshot(api_presets.clone());
            let mut sessions = terminal_manager.list_all_sessions(
                &base_dir,
                &display_root,
                error_line_limit,
                &error_keywords,
                &auto_continue_time_patterns,
                auto_continue_interval_seconds,
                auto_continue_respect_manual_interrupt,
                auto_continue_backoff_factor,
                auto_continue_backoff_max_minutes,
            );
            annotate_terminal_session_api_preset_names(&mut sessions, &api_presets);
            Ok(Json(TerminalSessionsResponse {
                all: true,
                path: String::new(),
                display_path: "全部目录".to_string(),
                sessions,
            }))
        })
        .await;
    }

    let directory = filesystem::resolve_directory_path(&base_dir, &query.path)?;
    let relative = filesystem::relative_path(&base_dir, &directory)?;
    let terminal_manager = state.terminal_manager.clone();
    let api_presets = state.auth_manager.api_presets_snapshot();
    let error_line_limit = state.workspace_settings.terminal_error_match_line_limit();
    let error_keywords = state.workspace_settings.terminal_error_keywords();
    let auto_continue_time_patterns = state
        .workspace_settings
        .terminal_auto_continue_time_patterns();
    let auto_continue_interval_seconds = state
        .workspace_settings
        .terminal_auto_continue_interval_seconds();
    let auto_continue_respect_manual_interrupt = state
        .workspace_settings
        .terminal_auto_continue_respect_manual_interrupt();
    let auto_continue_backoff_factor = state
        .workspace_settings
        .terminal_auto_continue_backoff_factor();
    let auto_continue_backoff_max_minutes = state
        .workspace_settings
        .terminal_auto_continue_backoff_max_minutes();

    run_terminal_task(move || {
        terminal_manager.update_api_preset_snapshot(api_presets.clone());
        let mut sessions = terminal_manager.list_sessions(
            &base_dir,
            &display_root,
            &directory,
            error_line_limit,
            &error_keywords,
            &auto_continue_time_patterns,
            auto_continue_interval_seconds,
            auto_continue_respect_manual_interrupt,
            auto_continue_backoff_factor,
            auto_continue_backoff_max_minutes,
        );
        annotate_terminal_session_api_preset_names(&mut sessions, &api_presets);
        Ok(Json(TerminalSessionsResponse {
            all: false,
            path: relative_to_string(&relative),
            display_path: filesystem::display_path(&base_dir, &display_root, &directory),
            sessions,
        }))
    })
    .await
}

pub async fn search_sessions(
    State(state): State<AppState>,
    Query(query): Query<SearchSessionsQuery>,
) -> ApiResult<Json<TerminalSessionSearchResponse>> {
    let needle = query.q.trim().to_string();
    if needle.is_empty() {
        return Ok(Json(TerminalSessionSearchResponse {
            query: String::new(),
            matches: Vec::new(),
        }));
    }

    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();

    run_terminal_task(move || {
        terminal_manager
            .search_active_session_output(&base_dir, &display_root, &needle)
            .map(|matches| {
                Json(TerminalSessionSearchResponse {
                    query: needle,
                    matches,
                })
            })
            .map_err(|error| AppError::internal(format!("搜索终端输出失败: {error}")))
    })
    .await
}

pub async fn list_auto_continue_tasks(
    State(state): State<AppState>,
) -> ApiResult<Json<TerminalAutoContinueTasksResponse>> {
    let terminal_manager = state.terminal_manager.clone();
    run_terminal_task(move || {
        let session_names = terminal_manager.session_names_by_id();
        let (raw_crontab, mut auto_continue_tasks, mut crontab_error) =
            match read_terminal_auto_continue_crontab() {
                Ok(crontab) => {
                    let tasks = parse_terminal_auto_continue_tasks_from_crontab(&crontab);
                    (crontab, tasks, None)
                }
                Err(error) => (String::new(), Vec::new(), Some(error)),
            };
        let due_epochs = manager::parse_auto_continue_due_epochs(&raw_crontab);
        for task in &mut auto_continue_tasks {
            let name = session_names.get(&task.session_id).cloned();
            task.session_name = name.clone();
            task.webclx_terminal_name = name;
            task.due_epoch = due_epochs
                .get(&(task.session_id.clone(), task.signature.clone()))
                .copied();
        }
        // Prune expired one-shot entries from the live crontab and archive them.
        // Only run pruning when crontab itself is readable; otherwise surface the
        // original error and skip destructive rewrites.
        let (history_records, prune_error) = if crontab_error.is_none() {
            terminal_manager.prune_expired_auto_continue_tasks(&mut auto_continue_tasks)
        } else {
            (Vec::new(), None)
        };
        if let Some(error) = prune_error {
            crontab_error = Some(error);
        }
        let expired_tasks = history_records
            .into_iter()
            .map(|record| ExpiredAutoContinueTaskInfo {
                marker: record.marker,
                session_id: record.session_id,
                session_name: record.session_name,
                webclx_terminal_name: record.webclx_terminal_name,
                tmux_session_name: record.tmux_session_name,
                signature: record.signature,
                schedule: record.schedule,
                expired_at: record.expired_at,
            })
            .collect();
        Ok(Json(TerminalAutoContinueTasksResponse {
            auto_continue_tasks,
            expired_tasks,
            crontab_error,
        }))
    })
    .await
}

pub async fn delete_auto_continue_tasks(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let terminal_manager = state.terminal_manager.clone();
    run_terminal_task(move || {
        let removed = terminal_manager
            .clear_auto_continue_history()
            .map_err(|error| AppError::internal(format!("清空自动继续历史失败: {error}")))?;
        Ok(Json(serde_json::json!({ "removed": removed })))
    })
    .await
}

pub async fn update_auto_continue_task(
    State(state): State<AppState>,
    AxumPath(marker): AxumPath<String>,
    Json(payload): Json<TerminalAutoContinueTaskUpdateRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let terminal_manager = state.terminal_manager.clone();
    run_terminal_task(move || {
        terminal_manager
            .update_auto_continue_task_due_at(&marker, payload.due_at)
            .map_err(|error| AppError::bad_request(format!("更新自动继续任务失败: {error}")))
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete_auto_continue_task(
    State(state): State<AppState>,
    AxumPath(marker): AxumPath<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let terminal_manager = state.terminal_manager.clone();
    let removed = run_terminal_task(move || {
        terminal_manager
            .cancel_auto_continue_task(&marker)
            .map_err(|error| AppError::bad_request(format!("取消自动继续任务失败: {error}")))
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true, "removed": removed })))
}

fn annotate_terminal_session_api_preset_names(
    sessions: &mut [TerminalSessionInfo],
    presets: &[StoredApiPreset],
) {
    for session in sessions {
        if !session.codex_api_preset_name.trim().is_empty() {
            continue;
        }
        let base_url = session.codex_api_base_url.trim();
        if base_url.is_empty() {
            continue;
        }
        if let Some(preset) = presets
            .iter()
            .find(|preset| preset.base_url == base_url || api_provider_base_url(preset) == base_url)
        {
            session.codex_api_preset_name = preset.name.clone();
        }
    }
}

fn read_terminal_auto_continue_crontab() -> Result<String, String> {
    let output = Command::new("crontab")
        .arg("-l")
        .output()
        .map_err(|error| format!("读取 crontab 失败: {error}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("no crontab") {
        return Ok(String::new());
    }
    Err(format!("读取 crontab 失败: {}", stderr.trim()))
}

pub(in crate::terminal) fn parse_terminal_auto_continue_tasks_from_crontab(
    crontab: &str,
) -> Vec<TerminalAutoContinueTaskInfo> {
    crontab
        .lines()
        .filter_map(parse_terminal_auto_continue_task_line)
        .collect()
}

fn parse_terminal_auto_continue_task_line(line: &str) -> Option<TerminalAutoContinueTaskInfo> {
    const MARKER_PREFIX: &str = "webclx-auto-continue:";

    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || !trimmed.contains(MARKER_PREFIX) {
        return None;
    }

    let marker = extract_auto_continue_marker(trimmed)?;
    let mut marker_parts = marker.splitn(3, ':');
    if marker_parts.next()? != "webclx-auto-continue" {
        return None;
    }
    let session_id = marker_parts.next()?.to_string();
    let signature = marker_parts.next()?.to_string();

    let mut fields = Vec::new();
    let mut command_start = 0;
    for (index, part) in trimmed.split_whitespace().enumerate() {
        if index < 5 {
            fields.push(part);
            continue;
        }
        if let Some(position) = nth_whitespace_split_position(trimmed, 5) {
            command_start = position;
        }
        break;
    }
    if fields.len() != 5 || command_start == 0 {
        return None;
    }
    let command = trimmed[command_start..].trim().to_string();
    let script_path = extract_cron_script_path(&command);
    let script_exists = script_path
        .as_deref()
        .map(|path| Path::new(path).exists())
        .unwrap_or(false);
    let tmux_session_name = tmux_session_name(&session_id);

    Some(TerminalAutoContinueTaskInfo {
        marker,
        task_kind: "quota_reset".to_string(),
        task_label: "限额重置".to_string(),
        session_id,
        session_name: None,
        webclx_terminal_name: None,
        tmux_session_name,
        signature,
        schedule: fields.join(" "),
        command,
        script_path,
        script_exists,
        due_epoch: None,
    })
}

fn extract_auto_continue_marker(line: &str) -> Option<String> {
    let marker_start = line.rfind("webclx-auto-continue:")?;
    let marker = line[marker_start..]
        .split_whitespace()
        .next()?
        .trim_matches('#')
        .trim();
    if marker.split(':').count() == 3 {
        Some(marker.to_string())
    } else {
        None
    }
}

fn nth_whitespace_split_position(line: &str, field_count: usize) -> Option<usize> {
    let mut in_field = false;
    let mut completed_fields = 0;
    for (index, ch) in line.char_indices() {
        if ch.is_whitespace() {
            if in_field {
                completed_fields += 1;
                in_field = false;
                if completed_fields == field_count {
                    return line[index..]
                        .char_indices()
                        .find(|(_, next)| !next.is_whitespace())
                        .map(|(offset, _)| index + offset);
                }
            }
        } else {
            in_field = true;
        }
    }
    None
}

fn extract_cron_script_path(command: &str) -> Option<String> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('\'') {
        let mut value = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\'' {
                if chars.peek().is_none_or(|next| next.is_whitespace()) {
                    return Some(value);
                }
                return None;
            }
            value.push(ch);
        }
        return None;
    }
    command
        .split_whitespace()
        .next()
        .filter(|value| value.ends_with(".sh") || value.starts_with('/'))
        .map(ToString::to_string)
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateSessionRequest>,
) -> ApiResult<Json<TerminalSessionInfo>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let directory = filesystem::resolve_terminal_directory_path(&base_dir, &payload.path)?;
    let terminal_manager = state.terminal_manager.clone();
    let proxy_env = state.proxy_manager.get_terminal_proxy_env();
    let requested_preset_id = payload.codex_api_preset_id.trim();
    let startup = if requested_preset_id.is_empty() {
        current_api_terminal_startup(&state).await
    } else {
        api_terminal_startup_for_preset(
            &state.auth_manager.api_presets_snapshot(),
            requested_preset_id,
        )
        .ok_or_else(|| AppError::bad_request("指定的 Codex API 预设不存在。"))?
    };
    let terminal_user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    let terminal_default_env = state.workspace_settings.terminal_default_env_entries();
    let origin = payload.origin;
    let owner_key = normalize_terminal_session_owner_key(&payload.owner_key)
        .map_err(|error| AppError::bad_request(format!("终端归属标识无效: {error}")))?;
    let session = run_terminal_task(move || {
        terminal_manager
            .create_session_with_origin(
                &base_dir,
                &display_root,
                directory,
                terminal_user,
                terminal_default_env,
                proxy_env,
                None,
                startup.codex_api_preset_name,
                startup.codex_api_base_url,
                origin,
                owner_key,
            )
            .map_err(|error| AppError::internal(format!("创建终端会话失败: {error}")))
    })
    .await?;
    Ok(Json(session))
}

fn normalize_terminal_session_owner_key(value: &str) -> Result<String> {
    const MAX_OWNER_KEY_BYTES: usize = 160;

    let normalized = value.trim();
    if normalized.len() > MAX_OWNER_KEY_BYTES {
        anyhow::bail!("不能超过 {MAX_OWNER_KEY_BYTES} 个字节");
    }
    if normalized.chars().any(char::is_control) {
        anyhow::bail!("不能包含控制字符");
    }
    Ok(normalized.to_string())
}

pub async fn rename_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<RenameSessionRequest>,
) -> ApiResult<Json<TerminalSessionInfo>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let session = run_terminal_task(move || {
        terminal_manager
            .rename_session(&base_dir, &display_root, &session_id, payload.name)
            .map_err(|error| AppError::bad_request(format!("修改会话名称失败: {error}")))
    })
    .await?;
    Ok(Json(session))
}

pub async fn update_session_origin(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<UpdateSessionOriginRequest>,
) -> ApiResult<Json<TerminalSessionInfo>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let owner_key = normalize_terminal_session_owner_key(&payload.owner_key)
        .map_err(|error| AppError::bad_request(format!("终端归属标识无效: {error}")))?;
    let session = run_terminal_task(move || {
        terminal_manager
            .update_session_origin(&base_dir, &display_root, &session_id, payload.origin, owner_key)
            .map_err(|error| AppError::bad_request(format!("更新会话来源失败: {error}")))
    })
    .await?;
    Ok(Json(session))
}

pub async fn send_session_input(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<TerminalInputRequest>,
) -> ApiResult<Json<Value>> {
    let terminal_manager = state.terminal_manager.clone();
    run_terminal_task(move || {
        terminal_manager
            .send_session_input(&session_id, payload.data)
            .map_err(|error| AppError::bad_request(format!("发送终端输入失败: {error}")))
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Send Codex `/status` to one terminal and persist the provider reported by
/// that running Codex process as the session's current preset.
pub async fn extract_session_preset(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<TerminalPresetExtractionResponse>> {
    let terminal_manager = state.terminal_manager.clone();
    let extracted = run_terminal_task(move || {
        terminal_manager
            .extract_codex_preset_from_status(&session_id)
            .map_err(|error| AppError::bad_request(format!("命令提取预设失败: {error}")))
    })
    .await?;
    Ok(Json(extracted))
}

pub async fn send_session_continue(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let terminal_manager = state.terminal_manager.clone();
    run_terminal_task(move || {
        terminal_manager
            .send_session_continue(&session_id)
            .map_err(|error| AppError::bad_request(format!("发送终端继续失败: {error}")))
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn send_session_auto_continue(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<TerminalAutoContinueSendResponse>> {
    let terminal_manager = state.terminal_manager.clone();
    let error_line_limit = state.workspace_settings.terminal_error_match_line_limit();
    let error_keywords = state.workspace_settings.terminal_error_keywords();
    let keyword_actions = state.workspace_settings.terminal_error_keyword_actions();
    let auto_continue_time_patterns = state
        .workspace_settings
        .terminal_auto_continue_time_patterns();
    let respect_manual_interrupt = state
        .workspace_settings
        .terminal_auto_continue_respect_manual_interrupt();
    let interval_seconds = state
        .workspace_settings
        .terminal_auto_continue_interval_seconds();
    let outcome = run_terminal_task(move || {
        terminal_manager
            .send_session_auto_continue_if_error(
                &session_id,
                error_line_limit,
                &error_keywords,
                &keyword_actions,
                &auto_continue_time_patterns,
                respect_manual_interrupt,
                interval_seconds,
            )
            .map_err(|error| AppError::bad_request(format!("自动发送终端继续失败: {error}")))
    })
    .await?;
    let (sent, retry_after_millis, compact_sent) = match outcome {
        TerminalAutoContinueSendOutcome::Sent => (true, 0, false),
        TerminalAutoContinueSendOutcome::CompactSent => (true, 0, true),
        TerminalAutoContinueSendOutcome::Cooldown {
            retry_at_millis, ..
        } => (false, retry_at_millis.saturating_sub(current_timestamp_millis()), false),
        TerminalAutoContinueSendOutcome::NotEligible => (false, 0, false),
    };
    Ok(Json(TerminalAutoContinueSendResponse {
        ok: true,
        sent,
        retry_after_millis,
        compact_sent,
    }))
}

pub async fn list_scheduled_inputs(
    State(state): State<AppState>,
) -> ApiResult<Json<TerminalScheduledInputListResponse>> {
    let terminal_manager = state.terminal_manager.clone();
    let tasks = run_terminal_task(move || Ok(terminal_manager.list_scheduled_inputs())).await?;
    Ok(Json(TerminalScheduledInputListResponse { tasks }))
}

pub async fn create_scheduled_input(
    State(state): State<AppState>,
    Json(payload): Json<TerminalScheduledInputRequest>,
) -> ApiResult<Json<TerminalScheduledInputResponse>> {
    let terminal_mode = payload.terminal_mode.trim().to_lowercase();
    let is_new_terminal = terminal_mode == "new";

    let session_id = if is_new_terminal {
        // Create a new terminal session at the requested working directory,
        // then schedule the input against the freshly created session.
        let working_dir = payload.working_dir.trim().to_string();
        if working_dir.is_empty() {
            return Err(AppError::bad_request("新建终端模式下必须指定工作目录。"));
        }
        let base_dir = state.workspace_root();
        let display_root = state.workspace_display_root();
        let directory = filesystem::resolve_terminal_directory_path(&base_dir, &working_dir)?;
        let terminal_manager = state.terminal_manager.clone();
        let proxy_env = state.proxy_manager.get_terminal_proxy_env();
        let startup = current_api_terminal_startup(&state).await;
        let terminal_user = state
            .workspace_settings
            .terminal_user_profile()
            .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
        let terminal_default_env = state.workspace_settings.terminal_default_env_entries();
        let session = run_terminal_task(move || {
            terminal_manager
                .create_session(
                    &base_dir,
                    &display_root,
                    directory,
                    terminal_user,
                    terminal_default_env,
                    proxy_env,
                    None,
                    startup.codex_api_preset_name,
                    startup.codex_api_base_url,
                )
                .map_err(|error| AppError::internal(format!("创建终端会话失败: {error}")))
        })
        .await?;
        session.id
    } else {
        let sid = payload.session_id.trim().to_string();
        if sid.is_empty() {
            return Err(AppError::bad_request("缺少目标终端 session_id。"));
        }
        sid
    };

    let task_type = payload.task_type.trim().to_string();
    let terminal_manager = state.terminal_manager.clone();
    let task = run_terminal_task(move || {
        terminal_manager
            .create_scheduled_input(
                &session_id,
                payload.text,
                payload.due_at,
                payload.label,
                payload.send_enter,
                task_type,
                payload.working_dir,
            )
            .map_err(|error| AppError::bad_request(format!("创建定时发送任务失败: {error}")))
    })
    .await?;
    let tasks = state.terminal_manager.list_scheduled_inputs();
    Ok(Json(TerminalScheduledInputResponse {
        ok: true,
        task,
        tasks,
    }))
}

pub async fn delete_scheduled_input(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let terminal_manager = state.terminal_manager.clone();
    let removed = run_terminal_task(move || {
        terminal_manager
            .cancel_scheduled_input(&task_id)
            .map_err(|error| AppError::bad_request(format!("取消定时发送任务失败: {error}")))
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true, "removed": removed })))
}

pub async fn update_scheduled_input(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
    Json(payload): Json<TerminalScheduledInputUpdateRequest>,
) -> ApiResult<Json<Value>> {
    // Validate session_id (if changing) resolves to a known terminal so the
    // error is reported as bad_request instead of an internal failure.
    if let Some(ref session_id) = payload.session_id {
        if session_id.trim().is_empty() {
            return Err(AppError::bad_request("目标终端不能为空。"));
        }
    }
    // Reject an explicit non-future due_at before touching storage, mirroring
    // the validation done on create so the caller gets a clear message.
    if let Some(due_at) = payload.due_at {
        if due_at <= current_timestamp_millis() {
            return Err(AppError::bad_request("发送时间必须晚于当前时间。"));
        }
    }
    let update = ScheduledInputUpdate {
        due_at: payload.due_at,
        text: payload.text,
        send_enter: payload.send_enter,
        task_type: payload.task_type,
        session_id: payload.session_id,
    };
    let terminal_manager = state.terminal_manager.clone();
    let task = run_terminal_task(move || {
        terminal_manager
            .update_scheduled_input(&task_id, update)
            .map_err(|error| AppError::bad_request(format!("更新定时发送任务失败: {error}")))
    })
    .await?;
    let tasks = state.terminal_manager.list_scheduled_inputs();
    Ok(Json(serde_json::json!({ "ok": true, "task": task, "tasks": tasks })))
}

pub async fn prepare_quick_command(
    State(state): State<AppState>,
    Json(payload): Json<TerminalQuickCommandRequest>,
) -> ApiResult<Json<TerminalQuickCommandResponse>> {
    let command_line = payload.command_line.trim();
    if command_line.is_empty() {
        return Err(AppError::bad_request("缺少快捷命令内容。"));
    }

    let quick_commands = state.workspace_settings.terminal_quick_commands();
    let resolved_command =
        resolve_terminal_quick_command_input(command_line, &quick_commands).unwrap_or(command_line);
    let input = prepare_terminal_quick_command_for_session(
        &state,
        payload.session_id.trim(),
        resolved_command,
    )
    .await?;
    Ok(Json(TerminalQuickCommandResponse { data: input }))
}

/// Atomically resolves, builds, and sends a webClx-auto-typed command
/// into the target session, without recording it to the per-session
/// input history. See `TerminalAutoTypedInputRequest`.
pub async fn send_auto_typed_input(
    State(state): State<AppState>,
    Json(payload): Json<TerminalAutoTypedInputRequest>,
) -> ApiResult<Json<TerminalAutoTypedInputResponse>> {
    let command_line = payload.command_line.trim();
    if command_line.is_empty() {
        return Err(AppError::bad_request("缺少快捷命令内容。"));
    }

    let target = first_nonempty([
        payload.target.as_str(),
        payload.session_id.as_str(),
        payload.terminal_name.as_str(),
        payload.name.as_str(),
    ])
    .ok_or_else(|| AppError::bad_request("缺少目标终端 target/name/session_id"))?
    .to_string();

    let base_dir = state.workspace_root();
    let target_path = if payload.path.trim().is_empty() {
        None
    } else {
        Some(filesystem::resolve_directory_path(&base_dir, &payload.path)?)
    };

    let terminal_manager = state.terminal_manager.clone();
    let target_for_send = target.clone();
    let path_for_send = target_path.clone();
    let (session_id, _session_name) = run_terminal_task(move || {
        terminal_manager
            .resolve_session_target(&target_for_send, path_for_send.as_deref())
            .map_err(|error| AppError::bad_request(format!("解析自动输入目标失败: {error}")))
    })
    .await?;

    let quick_commands = state.workspace_settings.terminal_quick_commands();
    let resolved_command =
        resolve_terminal_quick_command_input(command_line, &quick_commands).unwrap_or(command_line);
    let input =
        prepare_terminal_quick_command_for_session(&state, &session_id, resolved_command).await?;

    let terminal_manager = state.terminal_manager.clone();
    let submit_enters = payload.submit_enters.min(4);
    let session_id_for_send = session_id.clone();
    let input_for_send = input.clone();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || -> Result<()> {
            terminal_manager.send_session_input_silent(&session_id_for_send, input_for_send)?;
            for index in 0..submit_enters {
                if index > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(120));
                }
                terminal_manager
                    .send_session_input_silent(&session_id_for_send, "\r".to_string())?;
            }
            Ok(())
        }),
    )
    .await;
    match result {
        Ok(Ok(Ok(_sent))) => {}
        Ok(Ok(Err(error))) => {
            return Err(AppError::bad_request(format!("发送自动输入失败: {error}")));
        }
        Ok(Err(join_error)) => {
            return Err(AppError::internal(format!("终端后台任务失败: {join_error}")));
        }
        Err(_) => {
            return Err(AppError::internal(format!(
                "终端任务超时（{} 秒），可能是 tmux 未安装或无响应。",
                TERMINAL_TASK_TIMEOUT_SECS
            )));
        }
    }

    Ok(Json(TerminalAutoTypedInputResponse { data: input }))
}

pub async fn send_session_message(
    State(state): State<AppState>,
    Json(payload): Json<TerminalMessageRequest>,
) -> ApiResult<Json<Value>> {
    let target = first_nonempty([
        payload.target.as_str(),
        payload.session_id.as_str(),
        payload.terminal_name.as_str(),
        payload.name.as_str(),
    ])
    .ok_or_else(|| AppError::bad_request("缺少目标终端 target/name/session_id"))?
    .to_string();
    let data = first_nonempty([payload.data.as_str(), payload.message.as_str()])
        .ok_or_else(|| AppError::bad_request("缺少要发送的消息 data/message"))?
        .to_string();
    let delivery_id = first_nonempty([payload.delivery_id.as_str()])
        .map(str::to_string)
        .unwrap_or_else(|| data.clone());
    let completed_build_request_id = payload.completed_build_request_id.trim().to_string();
    if !completed_build_request_id.is_empty() && !payload.verify_submission {
        return Err(AppError::bad_request("completed_build_request_id 需要 verify_submission"));
    }
    let submit_enters = if payload.submit_enters > 0 {
        payload.submit_enters.min(4)
    } else if payload.enter || payload.submit {
        1
    } else {
        0
    };
    if payload.verify_submission && submit_enters == 0 {
        return Err(AppError::bad_request("verify_submission 需要 submit/enter/submit_enters"));
    }

    let base_dir = state.workspace_root();
    let target_path = if payload.path.trim().is_empty() {
        None
    } else {
        Some(filesystem::resolve_directory_path(&base_dir, &payload.path)?)
    };
    let terminal_manager = state.terminal_manager.clone();
    let bracketed_paste = payload.bracketed_paste;
    let verify_submission = payload.verify_submission;
    let delivery_id_for_send = delivery_id.clone();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || -> Result<((String, String), usize)> {
            let (session_id, _) =
                terminal_manager.resolve_session_target(&target, target_path.as_deref())?;
            let delivery_baseline = if verify_submission {
                terminal_manager
                    .session_agent_rollout_history(&session_id)?
                    .as_deref()
                    .map(|entries| {
                        manager::terminal_message_delivery_count(entries, &delivery_id_for_send)
                    })
                    .unwrap_or(0)
            } else {
                0
            };
            let sent = terminal_manager.send_session_message(
                &session_id,
                None,
                data,
                submit_enters,
                bracketed_paste,
            )?;
            Ok((sent, delivery_baseline))
        }),
    )
    .await;
    let (sent, delivery_baseline) = match result {
        Ok(Ok(Ok(sent))) => sent,
        Ok(Ok(Err(error))) => {
            return Err(AppError::bad_request(format!("发送终端消息失败: {error}")));
        }
        Ok(Err(join_error)) => {
            return Err(AppError::internal(format!("终端后台任务失败: {join_error}")));
        }
        Err(_) => {
            return Err(AppError::internal(format!(
                "终端任务超时（{} 秒），可能是 tmux 未安装或无响应。",
                TERMINAL_TASK_TIMEOUT_SECS
            )));
        }
    };

    let mut submitted = !payload.verify_submission;
    let mut submit_attempts = usize::from(submit_enters);
    if payload.verify_submission {
        submitted = wait_for_terminal_message_delivery(
            state.terminal_manager.clone(),
            sent.0.clone(),
            delivery_id.clone(),
            delivery_baseline,
        )
        .await?;

        for delay_ms in TERMINAL_MESSAGE_SUBMIT_RETRY_DELAYS_MS {
            if submitted {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let terminal_manager = state.terminal_manager.clone();
            let session_id = sent.0.clone();
            run_terminal_task(move || {
                terminal_manager
                    .send_session_message(&session_id, None, String::new(), 1, false)
                    .map_err(|error| {
                        AppError::bad_request(format!("重试终端消息提交失败: {error}"))
                    })
            })
            .await?;
            submit_attempts += 1;
            submitted = wait_for_terminal_message_delivery(
                state.terminal_manager.clone(),
                sent.0.clone(),
                delivery_id.clone(),
                delivery_baseline,
            )
            .await?;
        }
    }

    if submitted && !completed_build_request_id.is_empty() {
        state
            .terminal_manager
            .complete_pending_build_request(&completed_build_request_id);
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "session_id": sent.0,
        "terminal_name": sent.1,
        "submitted": submitted,
        "submit_attempts": submit_attempts,
    })))
}

async fn wait_for_terminal_message_delivery(
    terminal_manager: TerminalManager,
    session_id: String,
    delivery_id: String,
    delivery_baseline: usize,
) -> ApiResult<bool> {
    for poll_index in 0..TERMINAL_MESSAGE_VERIFY_POLLS {
        let terminal_manager = terminal_manager.clone();
        let session_id = session_id.clone();
        let entries = run_terminal_task(move || {
            terminal_manager
                .session_agent_rollout_history(&session_id)
                .map_err(|error| AppError::internal(format!("确认终端消息提交失败: {error}")))
        })
        .await?;
        if manager::terminal_message_delivery_confirmed(
            entries.as_deref(),
            &delivery_id,
            delivery_baseline,
        ) {
            return Ok(true);
        }
        if poll_index + 1 < TERMINAL_MESSAGE_VERIFY_POLLS {
            tokio::time::sleep(std::time::Duration::from_millis(TERMINAL_MESSAGE_VERIFY_POLL_MS))
                .await;
        }
    }
    Ok(false)
}

pub async fn get_session_input_history(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<TerminalInputHistoryResponse>> {
    let terminal_manager = state.terminal_manager.clone();
    let entries = run_terminal_task(move || {
        terminal_manager
            .session_input_history(&session_id)
            .map_err(|error| AppError::internal(format!("获取终端输入历史失败: {error}")))
    })
    .await?;
    Ok(Json(TerminalInputHistoryResponse { entries }))
}

fn first_nonempty<const N: usize>(values: [&str; N]) -> Option<&str> {
    values
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

pub async fn upload_paste_assets(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    mut multipart: Multipart,
) -> ApiResult<Json<TerminalPasteAssetsResponse>> {
    let base_dir = state.workspace_root();
    let session_path = state
        .terminal_manager
        .session_path(&session_id)
        .ok_or_else(|| AppError::not_found("终端会话不存在。"))?;
    let session_dir = filesystem::canonical_directory_in_access_scope(&base_dir, &session_path)?;

    let asset_dir = session_dir.join(PASTE_ASSET_DIR_NAME);
    tokio::fs::create_dir_all(&asset_dir)
        .await
        .map_err(|error| AppError::internal(format!("创建粘贴图片目录失败: {error}")))?;

    let mut assets = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::bad_request(format!("读取上传内容失败: {error}")))?
    {
        if field.name() != Some("files") {
            continue;
        }
        if assets.len() >= MAX_PASTE_ASSET_COUNT {
            return Err(AppError::bad_request("一次最多粘贴 12 张图片。"));
        }

        let mime = field
            .content_type()
            .map(str::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let extension = paste_asset_extension(&mime)
            .ok_or_else(|| AppError::bad_request(format!("不支持的图片类型: {mime}")))?;
        let bytes = field
            .bytes()
            .await
            .map_err(|error| AppError::bad_request(format!("读取图片数据失败: {error}")))?;
        if bytes.is_empty() {
            return Err(AppError::bad_request("图片数据为空。"));
        }
        if bytes.len() > MAX_PASTE_ASSET_BYTES {
            return Err(AppError::bad_request("单张图片不能超过 12 MB。"));
        }

        let ordinal = assets.len() + 1;
        let file_name = unique_paste_asset_name(&asset_dir, extension, ordinal).await;
        let file_path = asset_dir.join(&file_name);
        tokio::fs::write(&file_path, &bytes)
            .await
            .map_err(|error| AppError::internal(format!("保存粘贴图片失败: {error}")))?;
        info!(
            session_id,
            path = %file_path.display(),
            mime,
            size = bytes.len(),
            "saved terminal paste asset"
        );
        let relative_path = filesystem::relative_path(&session_dir, &file_path)?;
        let relative_path_text = relative_to_string(&relative_path);
        let absolute_path_text = file_path.to_string_lossy().to_string();
        let markdown =
            format!("![{}]({})", paste_asset_alt_text(&relative_path_text), relative_path_text);
        assets.push(TerminalPasteAssetInfo {
            name: file_name,
            path: absolute_path_text,
            relative_path: relative_path_text,
            markdown,
            mime,
            size: bytes.len(),
        });
    }

    if assets.is_empty() {
        return Err(AppError::bad_request("没有可保存的图片。"));
    }

    Ok(Json(TerminalPasteAssetsResponse { assets }))
}

pub async fn idle_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<TerminalSessionInfo>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let session = run_terminal_task(move || {
        terminal_manager
            .set_session_idle(&base_dir, &display_root, &session_id, true)
            .map_err(|error| AppError::bad_request(format!("闲置会话失败: {error}")))
    })
    .await?;
    Ok(Json(session))
}

pub async fn restore_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<TerminalSessionInfo>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let session = run_terminal_task(move || {
        terminal_manager
            .set_session_idle(&base_dir, &display_root, &session_id, false)
            .map_err(|error| AppError::bad_request(format!("恢复会话失败: {error}")))
    })
    .await?;
    Ok(Json(session))
}

struct TerminalDeleteAuditContext {
    client_addr: SocketAddr,
    requester: String,
    user_agent: String,
    referer: String,
    request_source: String,
}

fn terminal_delete_audit_header(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<unspecified>")
        .chars()
        .take(TERMINAL_DELETE_AUDIT_HEADER_MAX_CHARS)
        .collect()
}

fn terminal_delete_audit_context(
    state: &AppState,
    headers: &HeaderMap,
    client_addr: SocketAddr,
) -> TerminalDeleteAuditContext {
    let requester =
        crate::login::verify_session_from_headers(headers, state).unwrap_or_else(|| {
            if client_addr.ip().is_loopback() {
                "local-loopback".to_string()
            } else {
                "unknown".to_string()
            }
        });
    TerminalDeleteAuditContext {
        client_addr,
        requester,
        user_agent: terminal_delete_audit_header(headers, header::USER_AGENT.as_str()),
        referer: terminal_delete_audit_header(headers, header::REFERER.as_str()),
        request_source: terminal_delete_audit_header(headers, TERMINAL_DELETE_SOURCE_HEADER),
    }
}

fn require_terminal_delete_confirmation(headers: &HeaderMap, session_id: &str) -> ApiResult<()> {
    let confirmed_session_id = headers
        .get(TERMINAL_DELETE_CONFIRM_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();
    if confirmed_session_id != session_id {
        return Err(AppError::bad_request(
            "结束终端请求缺少匹配的目标确认信息，请刷新页面后重试。",
        ));
    }
    Ok(())
}

pub async fn delete_session(
    State(state): State<AppState>,
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<TerminalSessionInfo>> {
    let audit = terminal_delete_audit_context(&state, &headers, client_addr);
    warn!(
        requester = %audit.requester,
        client_addr = %audit.client_addr,
        user_agent = %audit.user_agent,
        referer = %audit.referer,
        request_source = %audit.request_source,
        target_session_id = %session_id,
        "terminal session delete requested"
    );
    if let Err(error) = require_terminal_delete_confirmation(&headers, &session_id) {
        warn!(
            requester = %audit.requester,
            client_addr = %audit.client_addr,
            request_source = %audit.request_source,
            target_session_id = %session_id,
            reason = %error,
            "terminal session delete failed"
        );
        return Err(error);
    }

    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let delete_session_id = session_id.clone();
    let session = match run_terminal_task(move || {
        terminal_manager
            .delete_session(&base_dir, &display_root, &delete_session_id)
            .map_err(|error| AppError::bad_request(format!("结束会话失败: {error}")))
    })
    .await
    {
        Ok(session) => session,
        Err(error) => {
            warn!(
                requester = %audit.requester,
                client_addr = %audit.client_addr,
                request_source = %audit.request_source,
                target_session_id = %session_id,
                reason = %error,
                "terminal session delete failed"
            );
            return Err(error);
        }
    };
    warn!(
        requester = %audit.requester,
        client_addr = %audit.client_addr,
        user_agent = %audit.user_agent,
        referer = %audit.referer,
        request_source = %audit.request_source,
        target_session_id = %session_id,
        target_session_name = %session.name,
        target_session_path = %session.path,
        "terminal session deleted"
    );
    Ok(Json(session))
}

pub async fn list_resume_archives(
    State(state): State<AppState>,
) -> ApiResult<Json<CodexResumeArchivesResponse>> {
    let terminal_manager = state.terminal_manager.clone();

    run_terminal_task(move || {
        terminal_manager
            .list_resume_archives()
            .map(|archives| Json(CodexResumeArchivesResponse { archives }))
            .map_err(|error| AppError::internal(format!("读取 Codex 归档失败: {error}")))
    })
    .await
}

pub async fn list_codex_conversations(
    State(state): State<AppState>,
    Query(query): Query<CodexConversationsQuery>,
) -> ApiResult<Json<CodexConversationsResponse>> {
    let terminal_user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    let codex_home = terminal_user.home.join(".codex");
    let cwd_filter = query
        .cwd
        .as_deref()
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from);

    run_terminal_task(move || {
        let conversations = scan_codex_conversations_for_cwd(&codex_home, cwd_filter.as_deref())
            .map_err(|error| AppError::internal(format!("读取 Codex 对话列表失败: {error}")))?;

        Ok(Json(CodexConversationsResponse {
            codex_home: codex_home.display().to_string(),
            conversations,
        }))
    })
    .await
}

pub async fn delete_codex_conversation(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<DeleteCodexConversationResponse>> {
    let terminal_user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    let codex_home = terminal_user.home.join(".codex");
    let normalized_session_id = validated_codex_session_id(&session_id)
        .map_err(|error| AppError::bad_request(format!("删除 Codex 会话失败: {error}")))?;
    let terminal_manager = state.terminal_manager.clone();
    let active_terminal_manager = terminal_manager.clone();
    let active_session_id = normalized_session_id.clone();

    let resume_session_is_active = run_terminal_task(move || {
        let is_active = active_terminal_manager
            .resume_session_is_active(&active_session_id)
            .map_err(|error| AppError::internal(format!("检查活动终端失败: {error}")))?;
        Ok(is_active)
    })
    .await?;
    if resume_session_is_active {
        return Err(AppError::bad_request("请先结束活动终端，再删除 Codex 会话。"));
    }

    let deleted = run_terminal_task(move || {
        let mut deleted = delete_codex_conversation_files(&codex_home, &normalized_session_id)
            .map_err(|error| AppError::internal(format!("删除 Codex 会话失败: {error}")))?;
        let has_archive = terminal_manager
            .list_resume_archives()
            .map_err(|error| AppError::internal(format!("读取 Codex 归档失败: {error}")))?
            .iter()
            .any(|archive| {
                archive.id == deleted.session_id || archive.resume_id == deleted.session_id
            });
        if has_archive {
            terminal_manager
                .delete_resume_archive(&deleted.session_id)
                .map_err(|error| AppError::internal(format!("删除 Codex 归档失败: {error}")))?;
            deleted.resume_archive_deleted = true;
        }
        Ok(deleted)
    })
    .await?;
    if deleted.total_deleted() == 0 {
        return Err(AppError::not_found(format!("Codex 会话 `{}` 不存在。", deleted.session_id)));
    }
    Ok(Json(deleted))
}

pub async fn save_resume_archive(
    State(state): State<AppState>,
    Json(payload): Json<SaveCodexResumeArchiveRequest>,
) -> ApiResult<Json<CodexResumeArchive>> {
    let terminal_manager = state.terminal_manager.clone();

    run_terminal_task(move || {
        terminal_manager
            .save_resume_archive(payload)
            .map(Json)
            .map_err(|error| AppError::bad_request(format!("保存 Codex 归档失败: {error}")))
    })
    .await
}

#[cfg(test)]
fn scan_codex_conversations(codex_home: &Path) -> Result<Vec<CodexConversationInfo>> {
    scan_codex_conversations_for_cwd(codex_home, None)
}

fn scan_codex_conversations_for_cwd(
    codex_home: &Path,
    cwd_filter: Option<&Path>,
) -> Result<Vec<CodexConversationInfo>> {
    let sessions_dir = codex_home.join("sessions");
    let mut conversations = Vec::new();
    let metadata_index = codex_conversation_metadata_index(codex_home);
    collect_codex_conversations(&sessions_dir, &metadata_index, cwd_filter, &mut conversations)?;
    conversations.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.size_bytes.cmp(&left.size_bytes))
            .then_with(|| left.cwd.cmp(&right.cwd))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    Ok(conversations)
}

impl DeleteCodexConversationResponse {
    fn total_deleted(&self) -> usize {
        self.rollout_files_deleted
            + self.session_index_entries_deleted
            + self.history_entries_deleted
            + self.database_threads_deleted
            + usize::from(self.resume_archive_deleted)
    }
}

fn delete_codex_conversation_files(
    codex_home: &Path,
    raw_session_id: &str,
) -> Result<DeleteCodexConversationResponse> {
    let session_id = validated_codex_session_id(raw_session_id)?;

    let rollout_paths = codex_rollout_paths_for_session(&codex_home.join("sessions"), &session_id)?;
    let session_index_entries_deleted =
        rewrite_jsonl_without_session(&codex_home.join("session_index.jsonl"), "id", &session_id)?;
    let history_entries_deleted = rewrite_jsonl_without_session(
        &codex_home.join("history.jsonl"),
        "session_id",
        &session_id,
    )?;
    let database_threads_deleted =
        delete_codex_state_thread(&codex_home.join("state_5.sqlite"), &session_id)?;

    let mut rollout_files_deleted = 0;
    for path in rollout_paths {
        match fs::remove_file(&path) {
            Ok(()) => rollout_files_deleted += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("删除 Codex rollout 失败: {}", path.display()));
            }
        }
    }

    Ok(DeleteCodexConversationResponse {
        session_id,
        rollout_files_deleted,
        session_index_entries_deleted,
        history_entries_deleted,
        database_threads_deleted,
        resume_archive_deleted: false,
    })
}

fn validated_codex_session_id(raw_session_id: &str) -> Result<String> {
    let session_id = normalize_resume_id(raw_session_id)
        .map_err(|error| anyhow::anyhow!("Codex session ID 无效: {error}"))?;
    if !looks_like_codex_uuid(&session_id) {
        anyhow::bail!("Codex session ID 无效。");
    }
    Ok(session_id)
}

fn looks_like_codex_uuid(session_id: &str) -> bool {
    let bytes = session_id.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
}

fn codex_rollout_paths_for_session(directory: &Path, session_id: &str) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_codex_rollout_paths_for_session(directory, session_id, &mut paths)?;
    Ok(paths)
}

fn collect_codex_rollout_paths_for_session(
    directory: &Path,
    session_id: &str,
    paths: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("读取 Codex sessions 目录失败: {}", directory.display()));
        }
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_codex_rollout_paths_for_session(&path, session_id, paths)?;
        } else if file_type.is_file()
            && codex_session_id_from_path(&path).as_deref() == Some(session_id)
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn rewrite_jsonl_without_session(path: &Path, id_field: &str, session_id: &str) -> Result<usize> {
    for _ in 0..3 {
        let content = match fs::read(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("读取 Codex 元数据失败: {}", path.display()));
            }
        };
        let (replacement, removed) = filtered_jsonl_content(&content, id_field, session_id);
        if removed == 0 {
            return Ok(0);
        }
        if fs::read(path)? != content {
            continue;
        }
        replace_file_contents(path, &replacement)?;
        return Ok(removed);
    }
    anyhow::bail!("Codex 正在写入 {}，请稍后重试。", path.display())
}

fn filtered_jsonl_content(content: &[u8], id_field: &str, session_id: &str) -> (Vec<u8>, usize) {
    let mut replacement = Vec::with_capacity(content.len());
    let mut removed = 0;
    for line in content.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let should_remove = serde_json::from_slice::<Value>(line)
            .ok()
            .and_then(|value| {
                value
                    .get(id_field)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .is_some_and(|id| id == session_id);
        if should_remove {
            removed += 1;
        } else {
            replacement.extend_from_slice(line);
            replacement.push(b'\n');
        }
    }
    (replacement, removed)
}

fn replace_file_contents(path: &Path, content: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("Codex 元数据文件名无效")?;
    let nonce = current_timestamp_millis();
    let temp_path = path.with_file_name(format!(".{file_name}.webclx-{nonce}.tmp"));
    let backup_path = path.with_file_name(format!(".{file_name}.webclx-{nonce}.bak"));
    fs::write(&temp_path, content)
        .with_context(|| format!("写入 Codex 元数据临时文件失败: {}", temp_path.display()))?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temp_path, metadata.permissions())?;
    }
    fs::rename(path, &backup_path)
        .with_context(|| format!("备份 Codex 元数据失败: {}", path.display()))?;
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::rename(&backup_path, path);
        let _ = fs::remove_file(&temp_path);
        return Err(error).with_context(|| format!("替换 Codex 元数据失败: {}", path.display()));
    }
    fs::remove_file(&backup_path)
        .with_context(|| format!("清理 Codex 元数据备份失败: {}", backup_path.display()))?;
    Ok(())
}

fn delete_codex_state_thread(database_path: &Path, session_id: &str) -> Result<usize> {
    if !database_path.exists() {
        return Ok(0);
    }
    let mut connection = Connection::open(database_path)
        .with_context(|| format!("打开 Codex 状态数据库失败: {}", database_path.display()))?;
    connection.busy_timeout(std::time::Duration::from_secs(3))?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    delete_from_table_if_present(
        &transaction,
        "thread_dynamic_tools",
        "DELETE FROM thread_dynamic_tools WHERE thread_id = ?1",
        session_id,
    )?;
    delete_from_table_if_present(
        &transaction,
        "thread_spawn_edges",
        "DELETE FROM thread_spawn_edges WHERE parent_thread_id = ?1 OR child_thread_id = ?1",
        session_id,
    )?;
    if sqlite_table_exists(&transaction, "agent_job_items")? {
        transaction.execute(
            "UPDATE agent_job_items SET assigned_thread_id = NULL WHERE assigned_thread_id = ?1",
            params![session_id],
        )?;
    }
    let deleted = delete_from_table_if_present(
        &transaction,
        "threads",
        "DELETE FROM threads WHERE id = ?1",
        session_id,
    )?;
    transaction.commit()?;
    Ok(deleted)
}

fn delete_from_table_if_present(
    transaction: &Transaction<'_>,
    table: &str,
    statement: &str,
    session_id: &str,
) -> Result<usize> {
    if !sqlite_table_exists(transaction, table)? {
        return Ok(0);
    }
    Ok(transaction.execute(statement, params![session_id])?)
}

fn sqlite_table_exists(transaction: &Transaction<'_>, table: &str) -> Result<bool> {
    Ok(transaction
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn collect_codex_conversations(
    directory: &Path,
    metadata_index: &HashMap<String, CodexConversationMetadata>,
    cwd_filter: Option<&Path>,
    conversations: &mut Vec<CodexConversationInfo>,
) -> Result<()> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_codex_conversations(&path, metadata_index, cwd_filter, conversations)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Some(session_id) = codex_session_id_from_path(&path) else {
            continue;
        };
        let mut conversation_metadata =
            metadata_index.get(&session_id).cloned().unwrap_or_default();
        if let (Some(expected), Some(actual)) = (cwd_filter, conversation_metadata.cwd.as_deref())
            && Path::new(actual).components().ne(expected.components())
        {
            continue;
        }
        if conversation_metadata.cwd.is_none() || conversation_metadata.title.is_empty() {
            let rollout_metadata = codex_conversation_metadata(&path);
            if conversation_metadata.cwd.is_none() {
                conversation_metadata.cwd = rollout_metadata.cwd;
            }
            if conversation_metadata.title.is_empty() {
                conversation_metadata.title = rollout_metadata.title;
            }
        }
        if let Some(expected) = cwd_filter
            && conversation_metadata
                .cwd
                .as_deref()
                .is_none_or(|actual| Path::new(actual).components().ne(expected.components()))
        {
            continue;
        }
        conversations.push(CodexConversationInfo {
            session_id,
            cwd: conversation_metadata.cwd.unwrap_or_default(),
            title: conversation_metadata.title,
            file_path: path.display().to_string(),
            size_bytes: metadata.len(),
            updated_at: metadata
                .modified()
                .map(system_time_to_millis)
                .unwrap_or_default(),
            created_at: metadata
                .created()
                .map(system_time_to_millis)
                .unwrap_or_default(),
        });
    }

    Ok(())
}

#[derive(Debug, Clone, Default)]
struct CodexConversationMetadata {
    cwd: Option<String>,
    title: String,
    title_count: usize,
}

fn codex_conversation_metadata_index(
    codex_home: &Path,
) -> HashMap<String, CodexConversationMetadata> {
    let mut metadata_by_session = HashMap::new();

    if let Ok(file) = fs::File::open(codex_home.join("history.jsonl")) {
        for line in BufReader::new(file)
            .lines()
            .map_while(std::result::Result::ok)
        {
            let Ok(record) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(session_id) = record
                .get("session_id")
                .and_then(Value::as_str)
                .filter(|session_id| looks_like_codex_uuid(session_id))
            else {
                continue;
            };
            let Some(text) = record
                .get("text")
                .and_then(Value::as_str)
                .and_then(normalize_codex_conversation_user_text)
            else {
                continue;
            };
            let metadata = metadata_by_session
                .entry(session_id.to_string())
                .or_insert_with(CodexConversationMetadata::default);
            append_codex_conversation_title(&mut metadata.title, &text, &mut metadata.title_count);
        }
    }

    if let Ok(file) = fs::File::open(codex_home.join("session_index.jsonl")) {
        for line in BufReader::new(file)
            .lines()
            .map_while(std::result::Result::ok)
        {
            let Ok(record) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(session_id) = record
                .get("id")
                .and_then(Value::as_str)
                .filter(|session_id| looks_like_codex_uuid(session_id))
            else {
                continue;
            };
            let metadata = metadata_by_session
                .entry(session_id.to_string())
                .or_insert_with(CodexConversationMetadata::default);
            if let Some(cwd) = record
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|cwd| !cwd.is_empty())
            {
                metadata.cwd = Some(cwd.to_string());
            }
            if metadata.title.is_empty()
                && let Some(title) = record
                    .get("thread_name")
                    .and_then(Value::as_str)
                    .and_then(normalize_codex_conversation_user_text)
            {
                append_codex_conversation_title(
                    &mut metadata.title,
                    &title,
                    &mut metadata.title_count,
                );
            }
        }
    }

    metadata_by_session
}

fn codex_conversation_metadata(path: &Path) -> CodexConversationMetadata {
    let mut metadata = CodexConversationMetadata::default();
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return metadata,
    };
    let reader = BufReader::new(file);

    for line in reader
        .lines()
        .map_while(std::result::Result::ok)
        .take(MAX_CODEX_CONVERSATION_SCAN_LINES)
    {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if metadata.cwd.is_none() {
            metadata.cwd = codex_conversation_meta_cwd(&value);
        }

        for text in codex_conversation_user_texts(&value) {
            append_codex_conversation_title(&mut metadata.title, &text, &mut metadata.title_count);
            if metadata.title_count >= MAX_CODEX_CONVERSATION_TITLE_MESSAGES {
                break;
            }
        }

        if metadata.cwd.is_some() && metadata.title_count >= MAX_CODEX_CONVERSATION_TITLE_MESSAGES {
            break;
        }
    }

    metadata
}

fn codex_conversation_meta_cwd(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let cwd = value
        .get("payload")
        .and_then(|payload| payload.get("cwd"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if cwd.is_empty() {
        return None;
    }
    Some(cwd.to_string())
}

fn codex_conversation_user_texts(value: &Value) -> Vec<String> {
    let payload = if value.get("type").and_then(Value::as_str) == Some("response_item") {
        value.get("payload").unwrap_or(value)
    } else {
        value
    };
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return Vec::new();
    }
    if payload.get("role").and_then(Value::as_str) != Some("user") {
        return Vec::new();
    }

    let Some(content) = payload.get("content") else {
        return Vec::new();
    };
    if let Some(text) = content.as_str() {
        return normalize_codex_conversation_user_text(text)
            .into_iter()
            .collect();
    }

    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| {
            let kind = part.get("type").and_then(Value::as_str).unwrap_or_default();
            if !matches!(kind, "input_text" | "text") {
                return None;
            }
            let text = part.get("text").and_then(Value::as_str)?;
            normalize_codex_conversation_user_text(text)
        })
        .collect()
}

fn normalize_codex_conversation_user_text(text: &str) -> Option<String> {
    let normalized = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if normalized.is_empty() || should_skip_codex_conversation_user_text(&normalized) {
        return None;
    }
    Some(normalized)
}

fn should_skip_codex_conversation_user_text(text: &str) -> bool {
    text == "继续"
        || text.eq_ignore_ascii_case("continue")
        || text.starts_with("[from webClx-compile-api]")
        || text.starts_with("<turn_aborted>")
        || text.starts_with("Skill descriptions were shortened to fit the 2% skills context budget")
        || text.starts_with("service temporarily unavailable (source:")
        || text.starts_with("# AGENTS.md instructions")
        || text.starts_with("<environment_context>")
        || text.starts_with("<permissions instructions>")
        || text.starts_with("<collaboration_mode>")
        || text.starts_with("<skills_instructions>")
        || text.starts_with("<model_config>")
}

fn append_codex_conversation_title(title: &mut String, text: &str, title_count: &mut usize) {
    if *title_count >= MAX_CODEX_CONVERSATION_TITLE_MESSAGES {
        return;
    }
    let separator_chars = usize::from(!title.is_empty());
    let used_chars = title.chars().count();
    let remaining = MAX_CODEX_CONVERSATION_TITLE_CHARS.saturating_sub(used_chars + separator_chars);
    if remaining == 0 {
        return;
    }

    if !title.is_empty() {
        title.push('\n');
    }
    title.push_str(&clamp_text_chars(text, remaining));
    *title_count += 1;
}

fn clamp_text_chars(text: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            output.push('…');
            break;
        }
        output.push(ch);
    }
    output
}

pub async fn touch_resume_archive(
    State(state): State<AppState>,
    AxumPath(archive_id): AxumPath<String>,
) -> ApiResult<Json<CodexResumeArchive>> {
    let terminal_manager = state.terminal_manager.clone();

    run_terminal_task(move || {
        terminal_manager
            .touch_resume_archive(&archive_id)
            .map(Json)
            .map_err(|error| AppError::bad_request(format!("更新 Codex 归档失败: {error}")))
    })
    .await
}

pub async fn delete_resume_archive(
    State(state): State<AppState>,
    AxumPath(archive_id): AxumPath<String>,
) -> ApiResult<Json<CodexResumeArchive>> {
    let terminal_manager = state.terminal_manager.clone();

    run_terminal_task(move || {
        terminal_manager
            .delete_resume_archive(&archive_id)
            .map(Json)
            .map_err(|error| AppError::bad_request(format!("删除 Codex 归档失败: {error}")))
    })
    .await
}

pub async fn current_agent_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<CurrentAgentSessionResponse>> {
    current_agent_session_response(state, session_id, false).await
}

pub async fn current_session_directory(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<CurrentTerminalDirectoryResponse>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let current_directory = run_terminal_task(move || {
        terminal_manager
            .current_working_directory(&session_id)
            .map_err(|error| AppError::bad_request(format!("读取终端当前目录失败: {error}")))
    })
    .await?;
    let directory = filesystem::canonical_directory_in_access_scope(&base_dir, &current_directory)?;
    let relative = filesystem::relative_path(&base_dir, &directory)?;

    Ok(Json(CurrentTerminalDirectoryResponse {
        path: relative_to_string(&relative),
        display_path: filesystem::display_path(&base_dir, &display_root, &directory),
    }))
}

pub async fn current_agent_session_complete(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<CurrentAgentSessionResponse>> {
    current_agent_session_response(state, session_id, true).await
}

pub async fn force_interrupt_and_resume_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<TerminalInterruptResumeResponse>> {
    let terminal_manager = state.terminal_manager.clone();
    run_terminal_task(move || {
        terminal_manager
            .force_interrupt_and_resume(&session_id)
            .map(Json)
            .map_err(|error| AppError::bad_request(format!("强制中断并恢复失败: {error}")))
    })
    .await
}

async fn current_agent_session_response(
    state: AppState,
    session_id: String,
    complete: bool,
) -> ApiResult<Json<CurrentAgentSessionResponse>> {
    let terminal_manager = state.terminal_manager.clone();

    run_terminal_task(move || {
        let detected = if complete {
            terminal_manager.current_resume_session_complete(&session_id)
        } else {
            terminal_manager.current_resume_session(&session_id)
        }
        .map_err(|error| AppError::bad_request(format!("读取当前会话失败: {error}")))?;

        Ok(Json(CurrentAgentSessionResponse {
            resume_id: detected
                .as_ref()
                .map(|session| session.info.resume_id.clone()),
            command: detected
                .as_ref()
                .map(|session| session.info.command.clone()),
            source: detected
                .as_ref()
                .map(|session| session.source.to_string())
                .unwrap_or_else(|| "process_fd".to_string()),
            codex_status: codex_status::detect_current_codex_status(&session_id),
        }))
    })
    .await
}

pub async fn terminal_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<TerminalSessionQuery>,
) -> ApiResult<Response> {
    Ok(ws.on_upgrade(move |socket| handle_socket_connect(socket, state, query)))
}

async fn terminal_session_for_socket(
    state: &AppState,
    query: TerminalSessionQuery,
) -> ApiResult<(Arc<TerminalSession>, TerminalManager, bool)> {
    let manager = state.terminal_manager.clone();
    let connect_manager = manager.clone();
    let requested_session_id = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let output_visible = query.visible;
    let directory = match requested_session_id.as_deref() {
        Some(session_id) => match manager.session_path(session_id) {
            Some(path) => path,
            None => {
                filesystem::resolve_terminal_directory_path(&state.workspace_root(), &query.path)?
            }
        },
        None => filesystem::resolve_terminal_directory_path(&state.workspace_root(), &query.path)?,
    };
    let proxy_env = state.proxy_manager.get_terminal_proxy_env();
    let startup = current_api_terminal_startup(state).await;
    let terminal_user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    let terminal_default_env = state.workspace_settings.terminal_default_env_entries();
    let session = run_terminal_task(move || {
        connect_manager
            .get_for_connection(
                directory,
                requested_session_id.as_deref(),
                terminal_user,
                terminal_default_env,
                proxy_env,
                None,
                startup.codex_api_preset_name,
                startup.codex_api_base_url,
                output_visible,
            )
            .map_err(|error| AppError::internal(format!("创建终端会话失败: {error}")))
    })
    .await?;

    Ok((session, manager, output_visible))
}

async fn current_api_terminal_startup(state: &AppState) -> CurrentApiTerminalStartup {
    let user = state.workspace_settings.terminal_user();
    let auth_file = match runtime_paths::resolve_user_file(&user, AUTH_FILE_RELATIVE_PATH) {
        Ok(path) => path,
        Err(error) => {
            warn!("resolve terminal auth file for API startup env failed: {error}");
            return CurrentApiTerminalStartup::empty();
        }
    };
    let config_file = match runtime_paths::resolve_user_file(&user, CONFIG_FILE_RELATIVE_PATH) {
        Ok(path) => path,
        Err(error) => {
            warn!("resolve terminal config file for API startup env failed: {error}");
            return CurrentApiTerminalStartup::empty();
        }
    };
    let presets = state.auth_manager.api_presets_snapshot();
    if presets.is_empty() {
        return CurrentApiTerminalStartup::empty();
    }
    let current_auth = read_current_auth_state(&auth_file).await.ok().flatten();
    let current_config = read_current_config_provider(&config_file)
        .await
        .ok()
        .flatten();
    let current_mode = derive_current_mode(current_auth.as_ref(), current_config.as_ref());
    if !current_mode_uses_api_terminal_startup(current_mode) {
        return CurrentApiTerminalStartup::empty();
    }
    let current_api =
        derive_current_api_state(current_config.as_ref(), current_auth.as_ref(), &presets);
    let upstream_proxy = state.auth_manager.upstream_proxy_settings();
    let Some(preset) = presets.iter().find(|preset| {
        api_preset_summary_with_proxy_state(
            preset,
            current_mode,
            current_api.as_ref(),
            &upstream_proxy,
        )
        .active
            || api_preset_summary(preset, current_mode, current_api.as_ref()).active
    }) else {
        return CurrentApiTerminalStartup::empty();
    };
    CurrentApiTerminalStartup {
        codex_api_preset_name: preset.name.clone(),
        codex_api_base_url: preset.base_url.clone(),
    }
}

fn current_mode_uses_api_terminal_startup(current_mode: CurrentAuthMode) -> bool {
    current_mode == CurrentAuthMode::Api
}

#[cfg(test)]
const CLAUDE_MANAGED_TERMINAL_ENV_KEYS: [&str; 8] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
];

#[cfg(test)]
fn claude_terminal_unset_env_from_current(
    current: &auth_core::CurrentClaudeState,
    presets: &[auth_core::StoredClaudePreset],
) -> Vec<String> {
    let mut keys = Vec::new();
    for key in CLAUDE_MANAGED_TERMINAL_ENV_KEYS {
        push_terminal_unset_env(&mut keys, key);
    }
    for preset in presets {
        for override_item in &preset.config_overrides {
            if let Some(key) = override_item.key.as_deref() {
                push_terminal_unset_env(&mut keys, key);
            }
        }
    }
    for key in current.config_values.keys() {
        push_terminal_unset_env(&mut keys, key);
    }
    keys
}

#[cfg(test)]
fn push_terminal_unset_env(keys: &mut Vec<String>, key: &str) {
    let key = key.trim();
    if !is_valid_shell_env_key(key) || keys.iter().any(|existing| existing == key) {
        return;
    }
    keys.push(key.to_string());
}

#[cfg(test)]
fn is_valid_shell_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

async fn prepare_terminal_quick_command_for_session(
    state: &AppState,
    session_id: &str,
    command_line: &str,
) -> ApiResult<String> {
    let terminal_manager = state.terminal_manager.clone();
    let session_id = session_id.to_string();
    let user_name = run_terminal_task(move || {
        terminal_manager
            .session_user_name(&session_id)
            .map_err(|error| AppError::bad_request(format!("读取终端用户失败: {error}")))
    })
    .await?;
    let command_line = crate::codex_conversation_model::prepare_codex_history_model_for_user(
        &user_name,
        command_line,
    )
    .map_err(|error| AppError::bad_request(format!("更新 Codex 会话模型失败: {error}")))?;
    let command_line =
        crate::codex_launch::prepare_codex_history_command_for_user(&user_name, &command_line)
            .map_err(|error| AppError::bad_request(format!("读取当前 Codex 模型失败: {error}")))?;
    Ok(build_terminal_quick_command_input(&command_line))
}

fn build_terminal_quick_command_input(command_line: &str) -> String {
    let command_line = command_line.trim();
    format!("{command_line}\n")
}

fn resolve_terminal_quick_command_input<'a>(
    input: &'a str,
    commands: &'a [TerminalQuickCommand],
) -> Option<&'a str> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    commands.iter().find_map(|command| {
        let configured_command = command.command().trim();
        if configured_command.is_empty() {
            return None;
        }
        let matches_configured_shortcut = [command.key().trim(), command.label().trim()]
            .into_iter()
            .any(|candidate| !candidate.is_empty() && candidate == input);
        if matches_configured_shortcut || configured_command == input {
            Some(configured_command)
        } else {
            None
        }
    })
}

fn paste_asset_extension(mime: &str) -> Option<&'static str> {
    match mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

async fn unique_paste_asset_name(dir: &Path, extension: &str, ordinal: usize) -> String {
    let timestamp = current_timestamp_millis();
    for attempt in 0..1000 {
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let candidate = format!("clipboard-{timestamp}-{ordinal}{suffix}.{extension}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("clipboard-{timestamp}-{ordinal}-fallback.{extension}")
}

fn paste_asset_alt_text(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("clipboard-image")
        .to_string()
}

const TERMINAL_TASK_TIMEOUT_SECS: u64 = 10;
const TERMINAL_OUTPUT_VIEWED_MARK_INTERVAL_MS: u128 = 1000;

pub(in crate::terminal) async fn run_terminal_task<T, F>(task: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS),
        tokio::task::spawn_blocking(task),
    )
    .await;

    match result {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(error))) => Err(AppError::internal(format!("终端后台任务失败: {error}"))),
        Ok(Err(join_error)) => Err(AppError::internal(format!("终端后台任务失败: {join_error}"))),
        Err(_) => Err(AppError::internal(format!(
            "终端任务超时（{} 秒），可能是 tmux 未安装或无响应。",
            TERMINAL_TASK_TIMEOUT_SECS
        ))),
    }
}

fn sanitize_child_command(command: &mut Command) {
    for key in CHILD_PROCESS_ENV_KEYS_TO_CLEAR {
        command.env_remove(key);
    }
}

fn default_terminal_user_name() -> String {
    runtime_paths::DEFAULT_USER_NAME.to_string()
}

fn default_true() -> bool {
    true
}

async fn handle_socket_connect(socket: WebSocket, state: AppState, query: TerminalSessionQuery) {
    let requested_session_id = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("<latest>")
        .to_string();
    let session_result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS + 2),
        terminal_session_for_socket(&state, query),
    )
    .await;

    match session_result {
        Ok(Ok((session, manager, output_visible))) => {
            handle_socket(socket, session, manager, output_visible).await;
        }
        Ok(Err(error)) => {
            warn!("terminal websocket session prepare failed: {requested_session_id}: {error}");
            send_socket_connection_error(socket, error.to_string()).await;
        }
        Err(_) => {
            warn!("terminal websocket session prepare timed out: {requested_session_id}");
            send_socket_connection_error(
                socket,
                format!(
                    "终端连接超时（{} 秒），可能是 tmux 或会话恢复无响应。",
                    TERMINAL_TASK_TIMEOUT_SECS + 2
                ),
            )
            .await;
        }
    }
}

async fn send_socket_connection_error(mut socket: WebSocket, message: String) {
    let payload = serde_json::to_string(&TerminalConnectionError { message: &message })
        .unwrap_or_else(|_| {
            "{\"type\":\"terminal_connection_error\",\"message\":\"终端连接失败。\"}".to_string()
        });
    let _ = socket.send(Message::Text(payload.into())).await;
}

async fn handle_socket(
    socket: WebSocket,
    session: Arc<TerminalSession>,
    manager: TerminalManager,
    initially_visible: bool,
) {
    let viewport_id = session.register_viewport(initially_visible);
    handle_registered_socket(socket, session.clone(), manager, initially_visible, viewport_id)
        .await;
    if let Err(error) = session.unregister_viewport(viewport_id).await {
        warn!("terminal websocket viewport cleanup failed: {error}");
    }
}

async fn handle_registered_socket(
    socket: WebSocket,
    session: Arc<TerminalSession>,
    manager: TerminalManager,
    initially_visible: bool,
    viewport_id: u64,
) {
    // 先订阅 broadcast 再读取 seq，最后抓 backlog snapshot。
    // 顺序很关键：subscribe(T2) -> read_seq(S1) -> snapshot(T3)。
    //  - subscribe 之前的历史 chunk (seq <= S1) 会被 socket_loop 跳过；这些内容
    //    必然已被更晚的 snapshot(T3) 覆盖，不会丢。
    //  - subscribe 之后产生的新 chunk seq > S1，会被正常发送。
    //  - read_seq 到 snapshot 之间的 chunk 会同时被发送和出现在 snapshot 里，
    //    属于重复显示，xterm 幂等，无害。
    // 这样既不重放历史（修复切换终端"滚一遍"），也不丢任何新输出。
    let mut receiver = session.subscribe();
    let live_output_start_seq = session.current_output_seq();
    let backlog = initial_backlog_for_socket(&session);
    let mut event_receiver = manager.subscribe_events();
    let (mut sender, mut socket_receiver) = socket.split();
    let input_session = session.clone();
    let input_manager = manager.clone();
    let output_visible = Arc::new(AtomicBool::new(initially_visible));
    let input_output_visible = output_visible.clone();

    let mut input_task = tokio::spawn(async move {
        while let Some(next_input) = socket_receiver.next().await {
            match next_input {
                Ok(message) => {
                    if process_socket_message(
                        &input_session,
                        &input_manager,
                        &input_output_visible,
                        viewport_id,
                        message,
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    warn!("terminal websocket receive error: {error}");
                    break;
                }
            }
        }
    });

    let connect_event = TerminalManagerEvent::SessionListChanged {
        action: "connected".to_string(),
        session_id: session.id.clone(),
    };
    if let Ok(payload) = serde_json::to_string(&connect_event)
        && sender.send(Message::Text(payload.into())).await.is_err()
    {
        input_task.abort();
        return;
    }

    if send_backlog_replay_control(&mut sender, "start")
        .await
        .is_err()
    {
        input_task.abort();
        return;
    }

    if !backlog.is_empty() && send_binary_chunks(&mut sender, &backlog).await.is_err() {
        input_task.abort();
        return;
    }

    if send_backlog_replay_control(&mut sender, "end")
        .await
        .is_err()
    {
        input_task.abort();
        return;
    }

    let output_viewed_mark_interval_duration =
        std::time::Duration::from_millis(TERMINAL_OUTPUT_VIEWED_MARK_INTERVAL_MS as u64);
    let mut output_viewed_mark_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + output_viewed_mark_interval_duration,
        output_viewed_mark_interval_duration,
    );
    output_viewed_mark_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut pending_output_viewed_mark = false;
    let mut last_output_seq_sent = live_output_start_seq;
    'socket_loop: loop {
        tokio::select! {
            _ = &mut input_task => break,
            _ = output_viewed_mark_interval.tick(), if pending_output_viewed_mark => {
                if output_visible.load(Ordering::SeqCst) {
                    manager.mark_session_output_viewed_in_memory(&session.id);
                    pending_output_viewed_mark = false;
                }
            }
            next_output = receiver.recv() => {
                match next_output {
                    Ok(chunk) => {
                        if chunk.seq <= last_output_seq_sent {
                            continue;
                        }
                        if chunk.seq > last_output_seq_sent + 1 {
                            warn!(
                                "terminal websocket observed output gap before chunk {}, recovering from backlog after {}",
                                chunk.seq, last_output_seq_sent
                            );
                            for recovered in session.backlog_chunks_after(last_output_seq_sent) {
                                if recovered.seq <= last_output_seq_sent {
                                    continue;
                                }
                                if recovered.seq > last_output_seq_sent + 1 {
                                    warn!(
                                        "terminal websocket backlog recovery has unrecoverable gap before chunk {} after {}",
                                        recovered.seq, last_output_seq_sent
                                    );
                                }
                                if send_terminal_output_chunk(&mut sender, &recovered).await.is_err() {
                                    break 'socket_loop;
                                }
                                last_output_seq_sent = recovered.seq;
                                if output_visible.load(Ordering::SeqCst) {
                                    pending_output_viewed_mark = true;
                                }
                            }
                            continue;
                        }
                        if send_terminal_output_chunk(&mut sender, &chunk).await.is_err() {
                            break;
                        }
                        last_output_seq_sent = chunk.seq;
                        if output_visible.load(Ordering::SeqCst) {
                            pending_output_viewed_mark = true;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(
                            "terminal websocket lagged by {skipped} chunks, recovering from backlog after {}",
                            last_output_seq_sent
                        );
                        let recovered_chunks = session.backlog_chunks_after(last_output_seq_sent);
                        if recovered_chunks.is_empty() {
                            warn!(
                                "terminal websocket lagged but no backlog chunks are available after {}",
                                last_output_seq_sent
                            );
                            continue;
                        }
                        for recovered in recovered_chunks {
                            if recovered.seq <= last_output_seq_sent {
                                continue;
                            }
                            if recovered.seq > last_output_seq_sent + 1 {
                                warn!(
                                    "terminal websocket backlog recovery has unrecoverable gap before chunk {} after {}",
                                    recovered.seq, last_output_seq_sent
                                );
                            }
                            if send_terminal_output_chunk(&mut sender, &recovered).await.is_err() {
                                break 'socket_loop;
                            }
                            last_output_seq_sent = recovered.seq;
                            if output_visible.load(Ordering::SeqCst) {
                                pending_output_viewed_mark = true;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            next_event = event_receiver.recv() => {
                match next_event {
                    Ok(event) => {
                        let payload = match serde_json::to_string(&event) {
                            Ok(payload) => payload,
                            Err(error) => {
                                warn!("serialize terminal session event failed: {error}");
                                continue;
                            }
                        };

                        if sender.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("terminal websocket lagged, skipped {skipped} session events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    if output_visible.load(Ordering::SeqCst) {
        manager.mark_session_output_viewed(&session.id);
    }
    input_task.abort();
}

async fn send_terminal_output_chunk<S>(socket: &mut S, chunk: &TerminalOutputChunk) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    socket
        .send(Message::Binary(chunk.bytes.clone().into()))
        .await
        .context("send terminal output chunk failed")?;
    Ok(())
}

fn initial_backlog_for_socket(session: &TerminalSession) -> Vec<u8> {
    session
        .initial_backlog_snapshot()
        .unwrap_or_else(|| backend_snapshot_or_backlog(session))
}

#[cfg(windows)]
fn backend_snapshot_or_backlog(session: &TerminalSession) -> Vec<u8> {
    session.backlog_tail_snapshot(MAX_INITIAL_BACKLOG_BYTES)
}

#[cfg(not(windows))]
fn backend_snapshot_or_backlog(session: &TerminalSession) -> Vec<u8> {
    capture_tmux_initial_pane_snapshot(&session.id)
        .ok()
        .filter(|snapshot| !snapshot.is_empty())
        .unwrap_or_else(|| session.backlog_tail_snapshot(MAX_INITIAL_BACKLOG_BYTES))
}

async fn send_binary_chunks<S>(socket: &mut S, bytes: &[u8]) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    const CHUNK_SIZE: usize = 64 * 1024;

    for chunk in bytes.chunks(CHUNK_SIZE) {
        socket.send(Message::Binary(chunk.to_vec().into())).await?;
    }

    Ok(())
}

async fn send_backlog_replay_control<S>(socket: &mut S, action: &str) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let payload = serde_json::to_string(&TerminalBacklogReplayControl { action })?;
    socket.send(Message::Text(payload.into())).await?;
    Ok(())
}

/// 判断输入是否仅由 xterm 鼠标报告序列组成。
///
/// 对应前端 `filterTerminalMouseInput` 识别的两种鼠标报告：
/// - SGR（DECSET 1006）：`\x1b[<<btn>;<col>;<row>(M|m)`
/// - X10/普通（DECSET 9/1000）：`\x1b[M<b1><b2><b3>`（三个任意 payload 字节）
///
/// 鼠标报告是尽力而为的交互输入，单次写入失败时降级丢弃而非断开整条连接，
/// 避免拖动选区时高频鼠标报告偶发写失败导致连接震荡闪烁。
fn is_mouse_only_input(data: &str) -> bool {
    let bytes = data.as_bytes();
    let mut cursor = 0;
    let mut matched_any = false;

    while cursor < bytes.len() {
        // SGR 鼠标报告：ESC [ < digits ; digits ; digits (M|m)
        if bytes[cursor] == 0x1b
            && cursor + 2 < bytes.len()
            && bytes[cursor + 1] == b'['
            && bytes[cursor + 2] == b'<'
        {
            let mut scan = cursor + 3;
            // 三个由 ';' 分隔的非空数字段
            let mut fields = 0;
            while fields < 3 && scan < bytes.len() && bytes[scan].is_ascii_digit() {
                while scan < bytes.len() && bytes[scan].is_ascii_digit() {
                    scan += 1;
                }
                fields += 1;
                if fields < 3 {
                    if scan >= bytes.len() || bytes[scan] != b';' {
                        return false;
                    }
                    scan += 1;
                }
            }
            if fields != 3 {
                return false;
            }
            if scan >= bytes.len() || (bytes[scan] != b'M' && bytes[scan] != b'm') {
                return false;
            }
            cursor = scan + 1;
            matched_any = true;
            continue;
        }

        // X10 鼠标报告：ESC [ M <b1> <b2> <b3>
        if bytes[cursor] == 0x1b
            && cursor + 5 < bytes.len()
            && bytes[cursor + 1] == b'['
            && bytes[cursor + 2] == b'M'
        {
            cursor += 6;
            matched_any = true;
            continue;
        }

        return false;
    }

    matched_any
}

/// 写入终端输入，对纯鼠标报告的写入失败降级处理。
///
/// 真实键盘输入/粘贴/控制字符写失败仍返回 Err 以触发断连，因为那说明 PTY
/// 确实不可用；纯鼠标报告写失败只记日志并丢弃，避免选区拖动产生的高频报告
/// 偶发撞上 PTY 关闭窗口时整条连接反复断连重放。
async fn write_terminal_input(
    session: &TerminalSession,
    manager: &TerminalManager,
    data: &str,
) -> Result<()> {
    match session.write_input(data.to_string()).await {
        Ok(()) => {
            manager.record_session_input(&session.id, data);
            Ok(())
        }
        Err(error) if is_mouse_only_input(data) => {
            warn!("dropping mouse-only terminal input after write failure: {error}");
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn process_socket_message(
    session: &TerminalSession,
    manager: &TerminalManager,
    output_visible: &AtomicBool,
    viewport_id: u64,
    message: Message,
) -> Result<()> {
    match message {
        Message::Text(text) => {
            if let Ok(client_message) = serde_json::from_str::<ClientMessage>(&text) {
                match client_message {
                    ClientMessage::Input { data } => {
                        write_terminal_input(session, manager, &data).await?;
                    }
                    ClientMessage::Resize { cols, rows } => {
                        session.resize_viewport(viewport_id, cols, rows).await?
                    }
                    ClientMessage::Visibility { visible } => {
                        session
                            .set_viewport_visibility(viewport_id, visible)
                            .await?;
                        let was_visible = output_visible.swap(visible, Ordering::SeqCst);
                        if visible {
                            manager.mark_session_opened(&session.id);
                            manager.mark_session_output_viewed(&session.id);
                        } else if was_visible {
                            manager.mark_session_output_viewed(&session.id);
                        }
                    }
                }
            }
        }
        Message::Binary(bytes) => {
            if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                write_terminal_input(session, manager, &text).await?;
            }
        }
        Message::Ping(_) | Message::Pong(_) => {}
        Message::Close(_) => return Err(anyhow::anyhow!("websocket closed")),
    }

    Ok(())
}

#[cfg(test)]
mod tests;
