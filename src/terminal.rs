use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
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
    extract::{ConnectInfo, Multipart, Path as AxumPath, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use settings_core::TerminalQuickCommand;
use terminal_core::*;
use tokio::sync::{broadcast, watch};
use tracing::{info, warn};

use crate::{ApiResult, AppError, AppState, filesystem, runtime_paths};

mod activity;
mod agent_session;
mod codex_status;
mod conversations;
#[cfg(test)]
use conversations::{
    CodexConversationMetadata, codex_conversation_metadata, scan_codex_conversations,
    scan_codex_conversations_for_cwd,
};
use conversations::{
    delete_codex_conversation_files, scan_codex_conversations_for_cwd_since,
    validated_codex_session_id,
};
mod docs;
mod input_pacing;
mod manager;
mod session;
mod tmux;
mod websocket;
use websocket::handle_socket_connect;
#[cfg(test)]
use websocket::{initial_replay_for_socket, is_mouse_only_input};

pub use docs::{list_session_agents_docs, read_session_agents_doc, save_session_agents_doc};
use manager::clamp_input_history_entries;
use session::TerminalSession;

#[cfg(test)]
use manager::{
    collect_session_infos_locked, ensure_unique_session_name_locked,
    refresh_auto_session_names_for_path_locked, should_notify_session_list_sync,
    sort_session_ids_by_recent_activity,
};

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 32;
const MAX_BACKLOG_BYTES: usize = 32 * 1024 * 1024;
/// 首屏回放（含 attach 深度快照种子）的最大字节数。超过时保留尾部，
/// 保证回放流仍从某一字节边界连续延伸到当前输出。
const MAX_INITIAL_REPLAY_BYTES: usize = 8 * 1024 * 1024;
const SESSION_EVENT_CHANNEL_CAPACITY: usize = 256;

/// attach 时写入 backlog 前缀种子的 tmux 抓取行数，启动时取自设置
/// `terminal_scrollback_lines`。它决定重连/刷新页面后可回看的终端历史深度；
/// tmux 侧 history-limit 为 100000，取值范围与设置归一化一致。
static TERMINAL_INITIAL_SNAPSHOT_LINE_LIMIT: AtomicU32 = AtomicU32::new(5_000);

pub(crate) fn set_terminal_initial_snapshot_line_limit(value: u32) {
    let clamped = value.clamp(100, 100_000);
    TERMINAL_INITIAL_SNAPSHOT_LINE_LIMIT.store(clamped, Ordering::SeqCst);
}
const INITIAL_TMUX_REDRAW_SUPPRESS_MS: u64 = 160;
const TERMINAL_RECENT_OUTPUT_ACTIVE_MS: u64 = 15_000;
const MAX_TERMINAL_INPUT_HISTORY_ENTRIES: usize = 500;
const MAX_TERMINAL_INPUT_HISTORY_LINE_BYTES: usize = 16 * 1024;
const PASTE_ASSET_DIR_NAME: &str = ".webclx-paste";
const SHUTDOWN_CTRL_C_DELAY_MS: u64 = 120;
const SHUTDOWN_CTRL_C_CLAUDE_TOTAL_COUNT: usize = 3;
const TERMINAL_MESSAGE_VERIFY_POLL_MS: u64 = 100;
const TERMINAL_MESSAGE_VERIFY_POLLS: usize = 60;
const TERMINAL_MESSAGE_SUBMIT_RETRY_DELAYS_MS: [u64; 3] = [1_000, 2_000, 4_000];
const TERMINAL_MESSAGE_DEDUP_TTL_SECS: u64 = 600;
const TERMINAL_MESSAGE_SEND_TIMEOUT_SECS: u64 = 10;
const TERMINAL_AUTO_TYPED_ENTER_DELAY_MS: u64 = 120;
const TERMINAL_DELETE_CONFIRM_HEADER: &str = "x-webclx-confirm-session";
const TERMINAL_DELETE_SOURCE_HEADER: &str = "x-webclx-delete-source";
const TERMINAL_DELETE_AUDIT_HEADER_MAX_CHARS: usize = 256;
const MAX_PASTE_ASSET_BYTES: usize = 12 * 1024 * 1024;
const MAX_PASTE_ASSET_COUNT: usize = 12;
const TERMINAL_SCHEDULED_INPUT_FILE_NAME: &str = ".webclx-terminal-scheduled-inputs.json";
const TERMINAL_AUTO_CONTINUE_SCHEDULE_FILE_NAME: &str =
    ".webclx-terminal-auto-continue-schedules.json";
const TERMINAL_PENDING_BUILD_FILE_NAME: &str = ".webclx-terminal-pending-builds.json";
const TERMINAL_MESSAGE_DELIVERY_LEDGER_FILE_NAME: &str = "terminal-message-deliveries.json";
const TERMINAL_PENDING_BUILD_MAX_AGE_MS: u64 = 24 * 60 * 60 * 1000;
const MAX_TERMINAL_SCHEDULED_INPUT_BYTES: usize = 512 * 1024;
const MIN_TERMINAL_SCHEDULED_REPEAT_INTERVAL_MILLIS: u64 = 1_000;
const MAX_TERMINAL_SCHEDULED_REPEAT_INTERVAL_MILLIS: u64 = 9_007_199_254_740_991;
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
#[derive(Clone)]
pub struct TerminalManager {
    state: Arc<RwLock<TerminalState>>,
    state_file: Arc<PathBuf>,
    registry_write_lock: Arc<Mutex<()>>,
    /// Serializes workspace-restore registry read/modify/write transactions.
    workspace_restore_write_lock: Arc<Mutex<()>>,
    /// (worker running, dirty); bounds input persistence to one background worker.
    input_persist_work: Arc<Mutex<(bool, bool)>>,
    archive_file: Arc<PathBuf>,
    scheduled_input_file: Arc<PathBuf>,
    auto_continue_file: Arc<PathBuf>,
    pending_build_file: Arc<PathBuf>,
    shutdown_restore_file: Arc<PathBuf>,
    workspace_restore_file: Arc<PathBuf>,
    env_snapshot: Arc<TerminalEnvironmentSnapshot>,
    next_id: Arc<AtomicU64>,
    event_sender: broadcast::Sender<TerminalManagerEvent>,
    auto_continue_schedules: Arc<Mutex<HashMap<String, TerminalAutoContinueTask>>>,
    auto_continue_notify: Arc<tokio::sync::Notify>,
    /// 每会话运行时状态（错误恢复记录、compact 标记、冷却时间戳、取消签名、
    /// 启动/发送串行锁）合并为一个对象，随会话生命周期整体存取与清理。
    per_session_runtime: Arc<Mutex<HashMap<String, SessionRuntimeState>>>,
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
    /// 每会话活动探测快照复用缓存：live PTY 会话 `last_output_at` 未变时
    /// 直接复用上次 capture-pane 快照，跳过昂贵的 tmux 子进程；匹配结果
    /// 仍每轮基于缓存快照重算，设置变更即时生效。
    probe_snapshot_cache: Arc<Mutex<manager::activity_probe::ProbeSnapshotCache>>,
    /// Service restart keeps tmux-backed sessions available for reconnection.
    service_restart_pending: Arc<AtomicBool>,
    /// 持久化的终端消息投递账本：dedup/冲突状态跨重启保留，投递状态可查询。
    terminal_message_ledger: Arc<Mutex<manager::message_ledger::TerminalMessageLedger>>,
}

/// 单个终端会话的内存运行时状态：原先分散在多张 per-session map 中的
/// 错误恢复记录、compact 一次性标记、冷却时间戳、取消签名与生命周期锁
/// 合并为一个对象，统一随会话存取与清理；会话 id 由外层 map 键承担，
/// 各字段不再自带 `"{session}:"` 前缀。
#[derive(Default)]
struct SessionRuntimeState {
    /// 当前错误自动继续记录（每会话至多一条）。
    error_auto_continue: Option<TerminalErrorAutoContinueRecord>,
    /// `/compact` 恢复的一次性标记：发送 `/compact` 时终端的错误签名。
    /// `/compact` 不能因为旧错误仍留在 pane 尾部就重试。
    auto_compact_signature: Option<String>,
    /// 上次 continue/compact 发送完成时间（毫秒），驱动冷却与退避。
    auto_continue_last_sent_at: Option<u64>,
    /// 已取消的自动继续签名（裸签名；会话 id 由 map 键承担）。
    canceled_signatures: HashSet<String>,
    /// 串行化同一会话的 start/delete/restore/cleanup。
    start_lock: Arc<Mutex<()>>,
    /// 串行化同一会话的 continue/compact 发送（冷却检查+字节写入+记账）。
    /// 旧实现由 last_sent map 锁顺带承担全局互斥；合并后的 map 锁不能
    /// 压过发送——写路径会取 `state` 读锁，与 `state → per_session_runtime`
    /// 的持锁顺序构成 ABBA——因此互斥下沉为每会话一把锁。
    send_lock: Arc<Mutex<()>>,
}

impl SessionRuntimeState {
    /// 除锁以外的业务字段是否已全部清空。
    fn runtime_fields_cleared(&self) -> bool {
        self.error_auto_continue.is_none()
            && self.auto_compact_signature.is_none()
            && self.auto_continue_last_sent_at.is_none()
            && self.canceled_signatures.is_empty()
    }

    /// 条目可安全移除：业务字段为空，且两把锁都只剩 map 这一份引用
    /// （没有等待中的 start/delete 或进行中的发送持有克隆）。
    /// 只能在持有 `per_session_runtime` map 锁时调用。
    fn prunable(&self) -> bool {
        self.runtime_fields_cleared()
            && Arc::strong_count(&self.start_lock) == 1
            && Arc::strong_count(&self.send_lock) == 1
    }
}

#[derive(Default)]
pub(crate) struct TerminalState {
    sessions_by_id: HashMap<String, StoredTerminalSession>,
    previous_terminal_names: HashMap<String, String>,
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
    /// `/compact` was sent because the context window was exhausted. Distinct
    /// from `Sent` so the frontend can show a precise status message.
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
    /// Empty means queued for backward compatibility. Nonempty tasks never resend.
    #[serde(default)]
    status: String,
    #[serde(default)]
    last_error: String,
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
    #[serde(default)]
    repeat_interval_millis: u64,
    #[serde(default)]
    parent_task_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TerminalScheduledInputTaskInfo {
    pub(crate) status: String,
    pub(crate) last_error: String,
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
    pub(crate) repeat_interval_millis: u64,
    pub(crate) parent_task_id: String,
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
    /// true 表示本次回放是按客户端上报的 seq 水位增量续传缺失 chunk，
    /// 客户端保留现有终端缓冲区；false 为全量快照回放。
    delta: bool,
    /// 会话输出流实例标识：会话对象重建（服务重启、恢复重建）后 seq 重新
    /// 从 0 计数，客户端必须匹配实例 id 才能安全使用 seq 水位续传。
    instance: u64,
}

/// 通知新协议客户端：本连接的每条二进制输出帧带 8 字节大端 seq 前缀。
/// 旧客户端不会请求 seq_framed，也就永远收不到该控制消息。
#[derive(Debug, Serialize)]
#[serde(rename = "terminal_output_framing")]
#[serde(tag = "type", rename_all = "snake_case")]
struct TerminalOutputFramingControl {
    version: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename = "terminal_viewport_size")]
#[serde(tag = "type", rename_all = "snake_case")]
struct TerminalViewportSizeControl {
    cols: u16,
    rows: u16,
}

struct TerminalInitialReplay {
    bytes: Vec<u8>,
    last_output_seq: u64,
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
    previous_names: Vec<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    codex_api_preset_name: String,
    #[serde(default)]
    codex_api_base_url: String,
    #[serde(default)]
    group_token: String,
    #[serde(default)]
    group_color: String,
    #[serde(default)]
    group_name: String,
    #[serde(default)]
    group_role: String,
    #[serde(default)]
    group_order: u64,
    #[serde(default)]
    group_fixed_order: bool,
    #[serde(default)]
    origin: TerminalSessionOrigin,
    #[serde(default)]
    owner_key: String,
    #[serde(default)]
    manually_renamed: bool,
    #[serde(default)]
    idle: bool,
    #[serde(default)]
    pub hidden: bool,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name,
            codex_api_base_url,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            origin,
            owner_key,
            manually_renamed: false,
            idle: false,
            hidden: false,
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
            previous_names: self.previous_names.clone(),
            title: self.title(),
            user_name: self.user_name.clone(),
            codex_api_preset_name: self.codex_api_preset_name.clone(),
            codex_api_base_url: self.codex_api_base_url.clone(),
            origin: self.origin,
            owner_key: self.owner_key.clone(),
            path: relative_to_string(&relative),
            icon_project_path: relative_to_string(&filesystem::workspace_icon_project_path(
                base_dir, &self.path,
            )),
            display_path: filesystem::display_path(base_dir, display_root, &self.path),
            alive: true,
            connected,
            busy: activity.busy,
            activity_state: activity.state,
            activity_label: activity.label,
            activity_agent: activity.agent,
            model_family: activity.model_family,
            activity_error_keyword: activity.error_keyword,
            activity_error_signature: activity.error_signature,
            activity_error_continue_sent: activity.error_continue_sent,
            activity_error_input_queued: activity.error_input_queued,
            activity_error_auto_continue_at: activity.error_auto_continue_at,
            last_output_at: activity.last_output_at,
            idle: self.idle,
            hidden: self.hidden,
            created_at: self.created_at,
            last_opened_at: self.last_opened_at,
            group_token: self.group_token.clone(),
            group_color: self.group_color.clone(),
            group_name: self.group_name.clone(),
            group_role: self.group_role.clone(),
            group_order: self.group_order,
            group_fixed_order: self.group_fixed_order,
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
    #[allow(dead_code)]
    model_family: String,
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
            model_family: String::new(),
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
            model_family: String::new(),
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
            model_family: String::new(),
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
            model_family: String::new(),
            error_keyword: None,
            error_signature: None,
            error_continue_sent: false,
            error_input_queued: false,
            error_auto_continue_at: None,
            last_output_at,
        }
    }

    fn compacting(last_output_at: u64) -> Self {
        Self {
            busy: true,
            state: "compacting".to_string(),
            label: "压缩中".to_string(),
            agent: None,
            model_family: String::new(),
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
            model_family: String::new(),
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
            model_family: String::new(),
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
            model_family: String::new(),
            error_keyword: Some(keyword),
            error_signature: Some(signature),
            error_continue_sent: true,
            error_input_queued: false,
            error_auto_continue_at: auto_continue_at,
            last_output_at,
        }
    }

    fn agent(agent: Option<String>, last_output_at: u64) -> Self {
        Self {
            busy: false,
            state: "agent".to_string(),
            label: "空闲".to_string(),
            agent,
            model_family: String::new(),
            error_keyword: None,
            error_signature: None,
            error_continue_sent: false,
            error_input_queued: false,
            error_auto_continue_at: None,
            last_output_at,
        }
    }

    fn orphaned_agent(agent: Option<String>, last_output_at: u64) -> Self {
        Self {
            busy: true,
            state: "orphaned_agent".to_string(),
            label: "Agent 异常".to_string(),
            agent,
            model_family: String::new(),
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

    fn with_model_family(mut self, model_family: String) -> Self {
        self.model_family = model_family;
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
    #[serde(default)]
    history: bool,
}

#[derive(Debug, Deserialize)]
pub struct CodexConversationsQuery {
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    since_ms: Option<u64>,
    #[serde(default)]
    recent_days: Option<u64>,
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
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_lenient_bool_default_true"
    )]
    visible: bool,
    /// 客户端支持 seq 帧协议：二进制输出帧带 8 字节大端 seq 前缀。
    #[serde(default, deserialize_with = "deserialize_lenient_bool")]
    seq_framed: bool,
    /// 客户端已应用到该 output seq，请求增量续传而非全量快照回放。
    #[serde(default)]
    resume_seq: Option<u64>,
    /// 客户端记录的会话输出流实例 id，用于识别会话重建后的 seq 重置。
    #[serde(default)]
    resume_instance: Option<u64>,
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
    pub(crate) label: Option<String>,
    pub(crate) repeat_interval_millis: Option<u64>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct TerminalGroupRoleAssignment {
    session_id: String,
    #[serde(default)]
    role: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionGroupRequest {
    member_ids: Vec<String>,
    #[serde(default)]
    group_roles: Vec<TerminalGroupRoleAssignment>,
    #[serde(default)]
    fixed_order: Option<bool>,
    #[serde(default)]
    group_color: Option<String>,
    #[serde(default)]
    group_name: Option<String>,
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
pub struct TerminalEnterRequest {
    #[serde(default)]
    submit_enters: u8,
    #[serde(default)]
    verify_submission: bool,
    #[serde(default)]
    delivery_id: String,
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
    #[serde(default)]
    repeat_interval_millis: u64,
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
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    repeat_interval_millis: Option<u64>,
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
    #[serde(default)]
    group: String,
    #[serde(default)]
    group_token: String,
    #[serde(default)]
    sender_session_id: String,
}

#[derive(Debug)]
pub(crate) struct TerminalMessageDeliveryTarget {
    session_id: String,
    terminal_name: String,
    renamed_target: bool,
    current_terminal_name: String,
}

impl TerminalMessageDeliveryTarget {
    fn deduplicated(session_id: String) -> Self {
        Self {
            session_id,
            terminal_name: String::new(),
            renamed_target: false,
            current_terminal_name: String::new(),
        }
    }

    fn delivered(
        session_id: String,
        requested_terminal_name: String,
        current_terminal_name: String,
    ) -> Self {
        let renamed_target = requested_terminal_name != current_terminal_name;
        Self {
            session_id,
            terminal_name: requested_terminal_name,
            renamed_target,
            current_terminal_name,
        }
    }
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
    /// The handler always submits the prepared command with one semantic
    /// Enter, so this is only needed when the caller wants extra submits.
    #[serde(default)]
    submit_enters: u8,
}

#[derive(Debug, PartialEq, Eq)]
struct SpecifiedPresetRunCommand {
    kind: SpecifiedPresetRunKind,
    preset_id: String,
}

#[derive(Debug, PartialEq, Eq)]
enum SpecifiedPresetRunKind {
    Api,
    OAuth,
    Claude,
}

/// Extract the exact preset selected by a webClx-generated temporary launch.
///
/// The command is inspected before model preparation rewrites the inner Codex
/// command. Only shell-safe lexical tokens are accepted; shell syntax is not
/// evaluated here.
fn specified_preset_run_command(command_line: &str) -> Option<SpecifiedPresetRunCommand> {
    let tokens = shell_words::split(command_line.trim()).ok()?;
    if tokens.len() < 5 || !is_program_token(&tokens[0], "webclx") || tokens[1] != "run" {
        return None;
    }
    let kind = match tokens[2].as_str() {
        "api" => SpecifiedPresetRunKind::Api,
        "oauth" => SpecifiedPresetRunKind::OAuth,
        "claude" => SpecifiedPresetRunKind::Claude,
        _ => return None,
    };
    let preset_id = tokens[3].trim();
    let is_safe_preset_id = !preset_id.is_empty()
        && preset_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    (is_safe_preset_id && tokens[4] == "--").then(|| SpecifiedPresetRunCommand {
        kind,
        preset_id: preset_id.to_string(),
    })
}

fn is_program_token(candidate: &str, expected: &str) -> bool {
    std::path::Path::new(candidate)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
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
    /// 是否正处于 bracketed paste 块内。粘贴块内的换行不切分历史条目，
    /// 整块在提交 Enter 时作为一条对话历史。
    paste_active: bool,
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
    #[serde(default)]
    previous_names: Vec<String>,
    title: Option<String>,
    user_name: String,
    codex_api_preset_name: String,
    codex_api_base_url: String,
    origin: TerminalSessionOrigin,
    owner_key: String,
    path: String,
    icon_project_path: String,
    display_path: String,
    alive: bool,
    connected: bool,
    busy: bool,
    activity_state: String,
    activity_label: String,
    activity_agent: Option<String>,
    #[serde(default)]
    model_family: String,
    activity_error_keyword: Option<String>,
    activity_error_signature: Option<String>,
    activity_error_continue_sent: bool,
    activity_error_input_queued: bool,
    activity_error_auto_continue_at: Option<String>,
    last_output_at: u64,
    idle: bool,
    #[serde(default)]
    pub hidden: bool,
    created_at: u64,
    last_opened_at: u64,
    group_token: String,
    #[serde(default)]
    group_color: String,
    #[serde(default)]
    group_name: String,
    #[serde(default)]
    group_role: String,
    #[serde(default)]
    group_order: u64,
    #[serde(default)]
    group_fixed_order: bool,
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
pub const TERMINAL_WORKSPACE_RESTORE_FILE_NAME: &str = ".webclx-terminal-workspace-restores.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalWorkspaceRestoreRecord {
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
    pub group_token: String,
    #[serde(default)]
    pub group_color: String,
    #[serde(default)]
    pub group_name: String,
    #[serde(default)]
    pub group_role: String,
    #[serde(default)]
    pub group_order: u64,
    #[serde(default)]
    pub group_fixed_order: bool,
    #[serde(default)]
    pub input_history: Vec<TerminalInputHistoryEntry>,
    #[serde(default)]
    pub resume_id: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub program: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub restore_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalWorkspaceRestoreRegistry {
    #[serde(default)]
    pub records: Vec<TerminalWorkspaceRestoreRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalWorkspaceRestoreInfo {
    pub session_id: String,
    pub name: String,
    pub user_name: String,
    pub path: String,
    pub display_path: String,
    pub group_name: String,
    pub group_role: String,
    pub group_token: String,
    pub group_color: String,
    pub group_order: u64,
    pub group_fixed_order: bool,
    pub program: String,
    pub resume_id: String,
    pub command: String,
    pub updated_at: u64,
    pub created_at: u64,
    pub restore_count: u32,
    pub recoverable: bool,
}

#[derive(Debug, Serialize)]
pub struct TerminalWorkspaceRestoresResponse {
    pub records: Vec<TerminalWorkspaceRestoreInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalWorkspaceRestoreFailure {
    pub session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalWorkspaceRestoreSkip {
    pub session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalWorkspaceRestoreResult {
    pub succeeded: Vec<String>,
    pub skipped: Vec<TerminalWorkspaceRestoreSkip>,
    pub failed: Vec<TerminalWorkspaceRestoreFailure>,
}

impl TerminalWorkspaceRestoreResult {
    pub fn restored_count(&self) -> usize {
        self.succeeded.len()
    }
}

#[derive(Debug, Deserialize)]
pub struct RestoreTerminalWorkspacesRequest {
    #[serde(default)]
    pub session_ids: Vec<String>,
}

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
    pub group_token: String,
    #[serde(default)]
    pub group_color: String,
    #[serde(default)]
    pub group_name: String,
    #[serde(default)]
    pub group_role: String,
    #[serde(default)]
    pub group_order: u64,
    #[serde(default)]
    pub group_fixed_order: bool,
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
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
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

#[derive(Debug, Clone, Serialize)]
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

impl DeleteCodexConversationResponse {
    fn total_deleted(&self) -> usize {
        self.rollout_files_deleted
            + self.session_index_entries_deleted
            + self.history_entries_deleted
            + self.database_threads_deleted
            + usize::from(self.resume_archive_deleted)
    }
}

#[derive(Debug, Serialize)]
pub struct CurrentAgentSessionResponse {
    resume_id: Option<String>,
    command: Option<String>,
    source: String,
    #[serde(default)]
    model_family: String,
    codex_status: Option<codex_status::CodexCompactStatus>,
    /// pane 当前后代进程中真实挂着的 agent 名称（Codex/Claude/CodeBuddy 等）。
    /// 进程 FD 检测可能在 rollout 文件未打开时拿不到 resume_id，该字段让
    /// 前端在"agent 还在终端里运行"时仍可走 /status 探测兜底。
    #[serde(default)]
    agents: Vec<String>,
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
    pub proxy_env: Vec<(String, String)>,
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<ListSessionsQuery>,
) -> ApiResult<Json<TerminalSessionsResponse>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();

    if query.history {
        if query.all {
            let sessions = terminal_manager.list_all_sessions_lightweight(&base_dir, &display_root);
            return Ok(Json(TerminalSessionsResponse {
                all: true,
                path: String::new(),
                display_path: "全部目录".to_string(),
                sessions,
            }));
        }

        let directory = filesystem::resolve_directory_path(&base_dir, &query.path)?;
        let relative = filesystem::relative_path(&base_dir, &directory)?;
        let sessions =
            terminal_manager.list_sessions_lightweight(&base_dir, &display_root, &directory);
        return Ok(Json(TerminalSessionsResponse {
            all: false,
            path: relative_to_string(&relative),
            display_path: filesystem::display_path(&base_dir, &display_root, &directory),
            sessions,
        }));
    }

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
    let mut command = Command::new("crontab");
    command.arg("-l");
    let output = crate::process_exec::bounded_output(
        command,
        std::time::Duration::from_secs(10),
        1024 * 1024,
    )
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
    let terminal_user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
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
                Vec::new(),
                None,
                String::new(),
                String::new(),
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

pub async fn update_session_group(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<UpdateSessionGroupRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let manager = state.terminal_manager.clone();
    run_terminal_task(move || {
        manager
            .update_session_group(
                &session_id,
                &payload.member_ids,
                &payload.group_roles,
                payload.fixed_order.unwrap_or(false),
                payload.group_color,
                payload.group_name,
            )
            .map_err(|error| AppError::bad_request(format!("更新终端分组失败: {error}")))
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
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

pub async fn send_session_enter(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<TerminalEnterRequest>,
) -> ApiResult<Json<Value>> {
    let submit_enters = payload.submit_enters.max(1).min(4);
    let delivery_id = payload.delivery_id.trim().to_string();
    if payload.verify_submission && delivery_id.is_empty() {
        return Err(AppError::bad_request("verify_submission 需要 delivery_id"));
    }

    let terminal_manager = state.terminal_manager.clone();
    let delivery_baseline = if payload.verify_submission {
        let session_id = session_id.clone();
        let delivery_id = delivery_id.clone();
        run_terminal_task(move || {
            terminal_manager
                .session_agent_rollout_history(&session_id)
                .map(|entries| {
                    entries
                        .as_deref()
                        .map(|entries| {
                            manager::terminal_message_delivery_count(entries, &delivery_id)
                        })
                        .unwrap_or(0)
                })
                .map_err(|error| AppError::internal(format!("读取终端消息提交基线失败: {error}")))
        })
        .await?
    } else {
        0
    };

    let terminal_manager = state.terminal_manager.clone();
    let session_id_for_send = session_id.clone();
    let verify_submission = payload.verify_submission;
    let paste_settle_delay = manager::terminal_message_paste_settle_delay(&delivery_id, true);
    run_terminal_task(move || {
        if verify_submission {
            terminal_manager
                .ensure_session_agent_active(&session_id_for_send)
                .map_err(|error| AppError::bad_request(format!("目标 Agent 已退出: {error}")))?;
        }
        std::thread::sleep(paste_settle_delay);
        for index in 0..submit_enters {
            terminal_manager
                .send_session_enter_key(&session_id_for_send)
                .map_err(|error| AppError::bad_request(format!("发送终端 Enter 失败: {error}")))?;
            if index + 1 < submit_enters {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
        Ok(())
    })
    .await?;

    let mut submitted = !payload.verify_submission;
    let mut submit_attempts = usize::from(submit_enters);
    if payload.verify_submission {
        // delivery_id 非空已由入参校验保证；账本中无该记录时标记为空操作。
        state
            .terminal_manager
            .record_terminal_message_submit_attempt(
                &session_id,
                &delivery_id,
                submit_attempts as u32,
            );
        submitted = wait_for_terminal_message_delivery(
            state.terminal_manager.clone(),
            session_id.clone(),
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
            let session_id_for_send = session_id.clone();
            let retry_result = run_terminal_task(move || {
                terminal_manager
                    .ensure_session_agent_active(&session_id_for_send)
                    .map_err(|error| {
                        AppError::bad_request(format!("目标 Agent 已退出，停止重试提交: {error}"))
                    })?;
                terminal_manager
                    .retry_session_message_enter(&session_id_for_send)
                    .map_err(|error| {
                        AppError::bad_request(format!("重试终端消息提交失败: {error}"))
                    })
            })
            .await;
            if let Err(error) = retry_result {
                state.terminal_manager.mark_terminal_message_failed(
                    &session_id,
                    &delivery_id,
                    &error.to_string(),
                );
                return Err(error);
            }
            if matches!(retry_result, Ok(false)) {
                submitted = true;
                break;
            }
            submit_attempts += 1;
            state
                .terminal_manager
                .record_terminal_message_submit_attempt(
                    &session_id,
                    &delivery_id,
                    submit_attempts as u32,
                );
            submitted = wait_for_terminal_message_delivery(
                state.terminal_manager.clone(),
                session_id.clone(),
                delivery_id.clone(),
                delivery_baseline,
            )
            .await?;
        }
        if submitted {
            state
                .terminal_manager
                .mark_terminal_message_confirmed(&session_id, &delivery_id);
        } else {
            state
                .terminal_manager
                .mark_terminal_message_unconfirmed(&session_id, &delivery_id);
        }
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "session_id": session_id,
        "submitted": submitted,
        "submit_attempts": submit_attempts,
    })))
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
    let glm_quota_preflight_required = {
        let terminal_manager = terminal_manager.clone();
        let session_id = session_id.clone();
        let error_keywords = error_keywords.clone();
        let auto_continue_time_patterns = auto_continue_time_patterns.clone();
        run_terminal_task(move || {
            Ok(terminal_manager.glm_quota_auto_continue_requires_preflight(
                &session_id,
                error_line_limit,
                &error_keywords,
                &auto_continue_time_patterns,
                respect_manual_interrupt,
            ))
        })
        .await?
    };
    if glm_quota_preflight_required {
        let api_presets = state.auth_manager.api_presets_snapshot();
        terminal_manager.update_api_preset_snapshot(api_presets.clone());
        let now_millis = current_timestamp_millis();
        let cached_fresh = state
            .quota_reset_cache
            .get_latest_zhipu_reset()
            .as_deref()
            .and_then(|reset| manager::terminal_auto_continue_due_millis(reset, now_millis))
            .is_some();
        let preflight_status = if cached_fresh {
            crate::quota::ZhipuQuotaPreflightStatus::Exhausted { reset_millis: 0 }
        } else {
            let quota_config = state.quota_manager.snapshot().await;
            crate::quota::refresh_zhipu_quota_reset_cache_with_quota_config(
                &api_presets,
                &state.quota_reset_cache,
                Some(&quota_config),
            )
            .await
        };
        if !matches!(preflight_status, crate::quota::ZhipuQuotaPreflightStatus::Available) {
            return Ok(Json(TerminalAutoContinueSendResponse {
                ok: true,
                sent: false,
                retry_after_millis: 0,
                compact_sent: false,
            }));
        }
    }
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
    manager::scheduled_input::validate_scheduled_input_repeat_interval(
        payload.repeat_interval_millis,
    )
    .map_err(|error| AppError::bad_request(format!("创建定时发送任务失败: {error}")))?;
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
        let terminal_user = state
            .workspace_settings
            .terminal_user_profile()
            .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
        let session = run_terminal_task(move || {
            terminal_manager
                .create_session(
                    &base_dir,
                    &display_root,
                    directory,
                    terminal_user,
                    Vec::new(),
                    None,
                    String::new(),
                    String::new(),
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
                payload.repeat_interval_millis,
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

pub async fn scheduled_input_status(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> ApiResult<Json<TerminalScheduledInputTaskInfo>> {
    let manager = state.terminal_manager.clone();
    let info = run_terminal_task(move || {
        manager
            .scheduled_input_status(&task_id)
            .map_err(|error| AppError::bad_request(error.to_string()))
    })
    .await?;
    Ok(Json(info))
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
        label: payload.label,
        repeat_interval_millis: payload.repeat_interval_millis,
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
    let command_body = input.trim_end_matches(['\r', '\n']).to_string();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || -> Result<()> {
            terminal_manager.send_session_input_silent(&session_id_for_send, command_body)?;
            for _ in 0..=submit_enters {
                std::thread::sleep(std::time::Duration::from_millis(
                    TERMINAL_AUTO_TYPED_ENTER_DELAY_MS,
                ));
                terminal_manager.send_session_enter_key(&session_id_for_send)?;
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
                "终端任务超时（{} 秒），请检查会话锁等待、磁盘负载及 tmux 响应。",
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
    let message_body = first_nonempty([payload.data.as_str(), payload.message.as_str()])
        .ok_or_else(|| AppError::bad_request("缺少要发送的消息 data/message"))?
        .to_string();
    let data = message_body.clone();
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
    let bracketed_paste = payload.bracketed_paste;
    let verify_submission = payload.verify_submission;

    let group_target = first_nonempty([payload.group_token.as_str(), payload.group.as_str()]);
    if let Some(group_target) = group_target {
        let manager = state.terminal_manager.clone();
        let sender_id = payload.sender_session_id.trim().to_string();
        let group_target = group_target.to_string();
        let response_group = group_target.clone();
        let response_sender = sender_id.clone();
        let data_for_group = data.clone();
        let delivery_id_for_group = delivery_id.clone();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS),
            tokio::task::spawn_blocking(move || -> Result<(Vec<Value>, bool)> {
                let mut ids = manager.resolve_group_session_ids(&group_target)?;
                let sender_in_group = !sender_id.is_empty() && ids.iter().any(|id| id == &sender_id);
                if sender_in_group { ids.retain(|id| id != &sender_id); }
                let mut deliveries = Vec::with_capacity(ids.len());
                for (index, id) in ids.into_iter().enumerate() {
                    let per_delivery = format!("{delivery_id_for_group}:{index}");
                    if verify_submission
                        && !manager.claim_terminal_message_delivery(
                            &id,
                            &per_delivery,
                            &data_for_group,
                        )?
                    {
                        continue;
                    }
                    let sent = manager.send_session_message(&id, None, data_for_group.clone(), submit_enters, bracketed_paste, verify_submission);
                    match sent {
                        Ok((session_id, terminal_name, renamed_target, current_terminal_name)) => {
                            if verify_submission {
                                manager.mark_terminal_message_injected(&id, &per_delivery);
                                manager.record_terminal_message_submit_attempt(
                                    &id,
                                    &per_delivery,
                                    u32::from(submit_enters),
                                );
                            }
                            deliveries.push(serde_json::json!({
                                "session_id": session_id, "terminal_name": terminal_name,
                                "renamed_target": renamed_target, "current_terminal_name": current_terminal_name,
                                "submitted": !verify_submission
                            }));
                        }
                        Err(error) => {
                            if verify_submission { manager.release_terminal_message_delivery(&id, &per_delivery); }
                            return Err(error);
                        }
                    }
                }
                Ok((deliveries, sender_in_group))
            }),
        ).await;
        let deliveries = match result {
            Ok(Ok(Ok(value))) => value,
            Ok(Ok(Err(error))) => {
                return Err(AppError::bad_request(format!("发送终端分组消息失败: {error}")));
            }
            Ok(Err(error)) => return Err(AppError::internal(format!("终端后台任务失败: {error}"))),
            Err(_) => {
                return Err(AppError::internal(format!(
                    "终端分组消息任务超时（{} 秒）",
                    TERMINAL_TASK_TIMEOUT_SECS
                )));
            }
        };
        let (deliveries, excluded_sender) = deliveries;
        return Ok(Json(serde_json::json!({
            "ok": true, "group": response_group, "sender_session_id": response_sender,
            "excluded_sender": excluded_sender, "delivered": deliveries.len(), "deliveries": deliveries,
            "submitted": !verify_submission || deliveries.iter().all(|item| item.get("submitted").and_then(Value::as_bool).unwrap_or(false))
        })));
    }

    let base_dir = state.workspace_root();
    let target_path = if payload.path.trim().is_empty() {
        None
    } else {
        Some(filesystem::resolve_directory_path(&base_dir, &payload.path)?)
    };
    let terminal_manager = state.terminal_manager.clone();
    let delivery_id_for_send = delivery_id.clone();
    let delivery_id_in_closure = delivery_id.clone();
    let message_body_for_send = message_body.clone();
    let mut terminal_message_send_timed_out = false;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_MESSAGE_SEND_TIMEOUT_SECS),
        tokio::task::spawn_blocking(
            move || -> Result<(TerminalMessageDeliveryTarget, usize, bool)> {
                let (session_id, _) =
                    terminal_manager.resolve_session_target(&target, target_path.as_deref())?;
                let rollout_history = if verify_submission {
                    terminal_manager.session_agent_rollout_history(&session_id)?
                } else {
                    None
                };
                let rollout_verifiable = rollout_history.is_some();
                let delivery_baseline = rollout_history
                    .as_deref()
                    .map(|entries| {
                        manager::terminal_message_delivery_count(entries, &delivery_id_in_closure)
                    })
                    .unwrap_or(0);
                if verify_submission
                    && !terminal_manager.claim_terminal_message_delivery(
                        &session_id,
                        &delivery_id_in_closure,
                        &data,
                    )?
                {
                    return Ok((
                        TerminalMessageDeliveryTarget::deduplicated(session_id),
                        delivery_baseline,
                        rollout_verifiable,
                    ));
                }
                let sent = terminal_manager.send_session_message(
                    &session_id,
                    None,
                    message_body_for_send,
                    submit_enters,
                    bracketed_paste,
                    verify_submission,
                );
                if sent.is_err() && verify_submission {
                    terminal_manager
                        .release_terminal_message_delivery(&session_id, &delivery_id_in_closure);
                }
                let (session_id, requested_terminal_name, _, current_terminal_name) = sent?;
                if verify_submission {
                    terminal_manager
                        .mark_terminal_message_injected(&session_id, &delivery_id_in_closure);
                    terminal_manager.record_terminal_message_submit_attempt(
                        &session_id,
                        &delivery_id_in_closure,
                        u32::from(submit_enters),
                    );
                }
                Ok((
                    TerminalMessageDeliveryTarget::delivered(
                        session_id,
                        requested_terminal_name,
                        current_terminal_name,
                    ),
                    delivery_baseline,
                    rollout_verifiable,
                ))
            },
        ),
    )
    .await;
    let (sent, delivery_baseline, rollout_verifiable) = match result {
        Ok(Ok(Ok(sent))) => sent,
        Ok(Ok(Err(error))) => {
            return Err(AppError::bad_request(format!("发送终端消息失败: {error}")));
        }
        Ok(Err(join_error)) => {
            return Err(AppError::internal(format!("终端后台任务失败: {join_error}")));
        }
        Err(_) => {
            // The background thread is not cancelled by an HTTP timeout. The
            // body may already have reached the terminal, so retain the claim
            // and report an explicit in-flight delivery for callers to poll.
            terminal_message_send_timed_out = true;
            (
                TerminalMessageDeliveryTarget::delivered(
                    delivery_id_for_send.clone(),
                    String::new(),
                    String::new(),
                ),
                0,
                false,
            )
        }
    };

    if terminal_message_send_timed_out {
        return Ok(Json(serde_json::json!({
            "ok": true,
            "session_id": sent.session_id,
            "terminal_name": sent.terminal_name,
            "renamed_target": sent.renamed_target,
            "current_terminal_name": sent.current_terminal_name,
            "submitted": false,
            "status": "in_flight",
            "deduplicated": false,
            "submit_attempts": 0,
        })));
    }

    if sent.terminal_name.is_empty() && payload.verify_submission {
        let status = state
            .terminal_manager
            .terminal_message_delivery_status(&sent.session_id, &delivery_id, &message_body)
            .map_err(|error| AppError::bad_request(format!("delivery_id conflict: {error}")))?;
        let submitted = matches!(status, "confirmed" | "submitted_unverifiable");
        if submitted && !completed_build_request_id.is_empty() {
            state
                .terminal_manager
                .complete_pending_build_request(&completed_build_request_id);
        }
        return Ok(Json(serde_json::json!({
            "ok": true,
            "session_id": sent.session_id,
            "terminal_name": "",
            "renamed_target": false,
            "current_terminal_name": "",
            "submitted": submitted,
            "deduplicated": true,
            "status": status,
            "submit_attempts": 0,
        })));
    }

    let mut submitted = !payload.verify_submission;
    let mut submit_attempts = usize::from(submit_enters);
    if payload.verify_submission {
        if rollout_verifiable {
            submitted = wait_for_terminal_message_delivery(
                state.terminal_manager.clone(),
                sent.session_id.clone(),
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
                let session_id = sent.session_id.clone();
                let retry_result = run_terminal_task(move || {
                    terminal_manager
                        .ensure_session_agent_active(&session_id)
                        .map_err(|error| {
                            AppError::bad_request(format!(
                                "目标 Agent 已退出，停止重试提交: {error}"
                            ))
                        })?;
                    terminal_manager
                        .retry_session_message_enter(&session_id)
                        .map_err(|error| {
                            AppError::bad_request(format!("重试终端消息提交失败: {error}"))
                        })
                })
                .await;
                if let Err(error) = retry_result {
                    state.terminal_manager.mark_terminal_message_failed(
                        &sent.session_id,
                        &delivery_id,
                        &error.to_string(),
                    );
                    return Err(error);
                }
                if matches!(retry_result, Ok(false)) {
                    submitted = true;
                    break;
                }
                submit_attempts += 1;
                state
                    .terminal_manager
                    .record_terminal_message_submit_attempt(
                        &sent.session_id,
                        &delivery_id,
                        submit_attempts as u32,
                    );
                submitted = wait_for_terminal_message_delivery(
                    state.terminal_manager.clone(),
                    sent.session_id.clone(),
                    delivery_id.clone(),
                    delivery_baseline,
                )
                .await?;
            }
            if submitted {
                state
                    .terminal_manager
                    .mark_terminal_message_confirmed(&sent.session_id, &delivery_id);
            } else {
                state
                    .terminal_manager
                    .mark_terminal_message_unconfirmed(&sent.session_id, &delivery_id);
            }
        } else {
            // 该 agent 不产生 rollout 历史，没有可用于投递确认的证据面；正文与
            // Enter 已在发送阶段成功写入终端即视为送达。继续 rollout 轮询与 Enter
            // 重试只会让同一段正文被重复灌入终端，因此直接上报已提交，并把账本
            // 记录为 unconfirmed，使后续相同 delivery_id 的调用被去重并读到同一状态。
            submitted = true;
            state
                .terminal_manager
                .mark_terminal_message_unconfirmed(&sent.session_id, &delivery_id);
        }
    }

    if submitted && !completed_build_request_id.is_empty() {
        state
            .terminal_manager
            .complete_pending_build_request(&completed_build_request_id);
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "session_id": sent.session_id,
        "terminal_name": sent.terminal_name,
        "renamed_target": sent.renamed_target,
        "current_terminal_name": sent.current_terminal_name,
        "submitted": submitted,
        "status": if submitted && payload.verify_submission && !rollout_verifiable {
            "submitted_unverifiable"
        } else if submitted {
            "confirmed"
        } else {
            "pending"
        },
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
        let delivery_id = delivery_id.clone();
        let confirmed = run_terminal_task(move || {
            if terminal_manager
                .session_message_input_queued(&session_id)
                .map_err(|error| AppError::internal(format!("读取终端消息队列失败: {error}")))?
            {
                return Ok(true);
            }
            let entries = terminal_manager
                .session_agent_rollout_history(&session_id)
                .map_err(|error| AppError::internal(format!("确认终端消息提交失败: {error}")))?;
            Ok(manager::terminal_message_delivery_confirmed(
                entries.as_deref(),
                &delivery_id,
                delivery_baseline,
            ))
        })
        .await?;
        if confirmed {
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

#[derive(Debug, Default, Deserialize)]
pub struct TerminalMessageDeliveryQuery {
    #[serde(default)]
    pub delivery_id: String,
    #[serde(default)]
    pub data: String,
}

pub async fn get_session_message_delivery(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<TerminalMessageDeliveryQuery>,
) -> ApiResult<Json<Value>> {
    let terminal_manager = state.terminal_manager.clone();
    let delivery_id = query.delivery_id.trim().to_string();
    let data = query.data;
    let session_id_for_query = session_id.clone();
    let status = run_terminal_task(move || {
        terminal_manager
            .terminal_message_delivery_status(&session_id_for_query, &delivery_id, &data)
            .map_err(|error| AppError::bad_request(format!("delivery_id conflict: {error}")))
    })
    .await?;

    let ledger = state
        .terminal_manager
        .terminal_message_delivery_record(&session_id, query.delivery_id.trim());

    Ok(Json(serde_json::json!({
        "ok": true,
        "session_id": session_id,
        "delivery_id": query.delivery_id.trim(),
        "status": status,
        "submitted": matches!(status, "confirmed" | "submitted_unverifiable"),
        "ledger": ledger.map(|record| serde_json::json!({
            "state": record.state,
            "submit_attempts": record.submit_attempts,
            "last_error": record.last_error,
            "claimed_at": record.claimed_at,
            "updated_at": record.updated_at,
        })),
    })))
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
            return Err(AppError::bad_request("一次最多粘贴 12 个文件。"));
        }

        let mime = field
            .content_type()
            .map(str::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let uploaded_name = field
            .file_name()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        let bytes = field
            .bytes()
            .await
            .map_err(|error| AppError::bad_request(format!("读取文件数据: {error}")))?;
        if bytes.is_empty() {
            return Err(AppError::bad_request("文件数据为空。"));
        }
        if bytes.len() > MAX_PASTE_ASSET_BYTES {
            return Err(AppError::bad_request("单个文件不能超过 12 MB。"));
        }

        let ordinal = assets.len() + 1;
        let stored_name =
            unique_paste_asset_name(&asset_dir, uploaded_name.as_deref(), mime.as_str(), ordinal)
                .await;
        let file_path = asset_dir.join(&stored_name);
        tokio::fs::write(&file_path, &bytes)
            .await
            .map_err(|error| AppError::internal(format!("保存粘贴文件失败: {error}")))?;
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
        let markdown = relative_path_text.clone();
        assets.push(TerminalPasteAssetInfo {
            name: stored_name,
            path: absolute_path_text,
            relative_path: relative_path_text,
            markdown,
            mime,
            size: bytes.len(),
        });
    }

    if assets.is_empty() {
        return Err(AppError::bad_request("没有可保存的文件。"));
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

pub async fn hide_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<TerminalSessionInfo>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let session = run_terminal_task(move || {
        terminal_manager
            .set_session_hidden(&base_dir, &display_root, &session_id, true)
            .map_err(|error| AppError::bad_request(format!("隐藏会话失败: {error}")))
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

const TERMINAL_CLOSE_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(10);
const TERMINAL_CLOSE_RATE_LIMIT_MAX: usize = 5;

static TERMINAL_CLOSE_TRACKER: std::sync::LazyLock<Mutex<HashMap<IpAddr, Vec<Instant>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn check_terminal_close_rate_limit(ip: IpAddr) -> ApiResult<()> {
    let now = Instant::now();
    let mut tracker = match TERMINAL_CLOSE_TRACKER.lock() {
        Ok(t) => t,
        Err(_) => return Ok(()),
    };
    let timestamps = tracker.entry(ip).or_default();
    timestamps.retain(|&ts| now.duration_since(ts) < TERMINAL_CLOSE_RATE_LIMIT_WINDOW);
    if timestamps.len() >= TERMINAL_CLOSE_RATE_LIMIT_MAX {
        return Err(AppError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "终端关闭或删除操作过于频繁，请稍候重试".to_string(),
        });
    }
    timestamps.push(now);
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
    check_terminal_close_rate_limit(client_addr.ip())?;
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

pub async fn close_session(
    State(state): State<AppState>,
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let audit = terminal_delete_audit_context(&state, &headers, client_addr);
    warn!(
        requester = %audit.requester,
        client_addr = %audit.client_addr,
        user_agent = %audit.user_agent,
        referer = %audit.referer,
        request_source = %audit.request_source,
        target_session_id = %session_id,
        "terminal session close requested"
    );
    check_terminal_close_rate_limit(client_addr.ip())?;

    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let terminal_manager = state.terminal_manager.clone();
    let delete_session_id = session_id.clone();
    let session = match run_terminal_task(move || {
        terminal_manager
            .delete_session(&base_dir, &display_root, &delete_session_id)
            .map_err(|error| AppError::bad_request(format!("关闭会话失败: {error}")))
    })
    .await
    {
        Ok(session) => session,
        Err(error) => {
            warn!(
                requester = %audit.requester,
                client_addr = %audit.client_addr,
                target_session_id = %session_id,
                reason = %error,
                "terminal session close failed"
            );
            return Err(error);
        }
    };
    info!(
        session_id = %session.id(),
        session_name = %session.name,
        "terminal session closed successfully"
    );
    Ok(Json(serde_json::json!({
        "ok": true,
        "session_id": session_id,
        "name": session.name,
        "message": "终端会话已关闭"
    })))
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

#[derive(Clone, PartialEq, Eq, Hash)]
struct CodexScanKey {
    codex_home: PathBuf,
    cwd_filter: Option<PathBuf>,
    since_ms: Option<u64>,
}

type CodexScanShared = Arc<Result<Vec<CodexConversationInfo>, String>>;

#[derive(Clone)]
struct CodexScanSlot {
    id: u64,
    created_at: Instant,
    result_tx: watch::Sender<Option<CodexScanShared>>,
    result_rx: watch::Receiver<Option<CodexScanShared>>,
}

fn codex_scan_slots() -> &'static Mutex<HashMap<CodexScanKey, CodexScanSlot>> {
    static SLOTS: std::sync::OnceLock<Mutex<HashMap<CodexScanKey, CodexScanSlot>>> =
        std::sync::OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers the caller for an in-flight Codex conversation scan. When an
/// identical scan is already running (same `codex_home` + `cwd_filter`), the
/// caller becomes a follower that waits for the shared result instead of
/// starting a duplicate directory scan. Stale slots (leader exceeded the
/// terminal task timeout window without completing) are taken over so a lost
/// leader cannot block later requests forever.
fn register_codex_scan_slot(key: &CodexScanKey) -> (CodexScanSlot, bool) {
    let takeover_after = std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS + 2);
    static NEXT_SLOT_ID: AtomicU64 = AtomicU64::new(1);
    let mut slots = crate::lock_or_recover!(codex_scan_slots().lock());
    if let Some(existing) = slots.get(key)
        && existing.created_at.elapsed() < takeover_after
    {
        return (existing.clone(), false);
    }
    let (result_tx, result_rx) = watch::channel(None);
    let slot = CodexScanSlot {
        id: NEXT_SLOT_ID.fetch_add(1, Ordering::Relaxed),
        created_at: Instant::now(),
        result_tx,
        result_rx,
    };
    slots.insert(key.clone(), slot.clone());
    (slot, true)
}

/// Publishes the shared scan outcome and drops this leader's slot from the
/// registry so the next request starts a fresh scan. The slot is only removed
/// when it is still the registered leader for `key`.
fn complete_codex_scan_slot(key: &CodexScanKey, slot: &CodexScanSlot, shared: CodexScanShared) {
    {
        let mut slots = crate::lock_or_recover!(codex_scan_slots().lock());
        if slots.get(key).map(|existing| existing.id) == Some(slot.id) {
            slots.remove(key);
        }
    }
    let _ = slot.result_tx.send(Some(shared));
}

async fn wait_codex_scan_result(
    mut result_rx: watch::Receiver<Option<CodexScanShared>>,
) -> ApiResult<Vec<CodexConversationInfo>> {
    let wait_result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS + 2),
        async move {
            loop {
                if let Some(shared) = result_rx.borrow_and_update().clone() {
                    return Arc::unwrap_or_clone(shared).map_err(AppError::internal);
                }
                if result_rx.changed().await.is_err() {
                    return Err(AppError::internal("Codex 对话扫描任务已终止，请重试。"));
                }
            }
        },
    )
    .await;

    match wait_result {
        Ok(result) => result,
        Err(_) => Err(AppError::internal(format!(
            "Codex 对话扫描等待超时（{} 秒），请点击“刷新对话”重试。",
            TERMINAL_TASK_TIMEOUT_SECS + 2
        ))),
    }
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
    let since_ms = query.since_ms.or_else(|| {
        query.recent_days.map(|days| {
            current_timestamp_millis()
                .saturating_sub(days.clamp(1, 36_500).saturating_mul(86_400_000))
        })
    });

    let scan_key = CodexScanKey {
        codex_home: codex_home.clone(),
        cwd_filter: cwd_filter.clone(),
        since_ms,
    };
    let (slot, is_leader) = register_codex_scan_slot(&scan_key);
    let conversations = if is_leader {
        let scan_codex_home = codex_home.clone();
        let scan_result = run_terminal_task(move || {
            scan_codex_conversations_for_cwd_since(
                &scan_codex_home,
                cwd_filter.as_deref(),
                since_ms,
            )
            .map_err(|error| AppError::internal(format!("读取 Codex 对话列表失败: {error}")))
        })
        .await;
        let shared: CodexScanShared = match &scan_result {
            Ok(conversations) => Arc::new(Ok(conversations.clone())),
            Err(app_error) => Arc::new(Err(app_error.to_string())),
        };
        complete_codex_scan_slot(&scan_key, &slot, shared);
        scan_result
    } else {
        wait_codex_scan_result(slot.result_rx).await
    };

    conversations.map(|conversations| {
        Json(CodexConversationsResponse {
            codex_home: codex_home.display().to_string(),
            conversations,
        })
    })
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

pub async fn list_workspace_restores(
    State(state): State<AppState>,
) -> ApiResult<Json<TerminalWorkspaceRestoresResponse>> {
    let terminal_manager = state.terminal_manager.clone();
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let records = run_terminal_task(move || {
        terminal_manager
            .list_workspace_restores(&base_dir, &display_root)
            .map_err(|error| AppError::internal(format!("读取终端工作现场失败：{error}")))
    })
    .await?;
    Ok(Json(TerminalWorkspaceRestoresResponse { records }))
}

pub async fn refresh_workspace_restores(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let terminal_manager = state.terminal_manager.clone();
    let updated = run_terminal_task(move || {
        terminal_manager
            .persist_all_workspace_restores()
            .map_err(|error| AppError::internal(format!("刷新终端工作现场失败：{error}")))
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true, "updated": updated })))
}

pub async fn delete_workspace_restore(
    State(state): State<AppState>,
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let audit = terminal_delete_audit_context(&state, &headers, client_addr);
    let session_id = session_id.trim().to_string();
    warn!(
        requester = %audit.requester,
        client_addr = %audit.client_addr,
        user_agent = %audit.user_agent,
        referer = %audit.referer,
        request_source = %audit.request_source,
        target_session_id = %session_id,
        "terminal workspace restore delete requested"
    );
    if let Err(error) = require_terminal_delete_confirmation(&headers, &session_id) {
        warn!(
            requester = %audit.requester,
            client_addr = %audit.client_addr,
            request_source = %audit.request_source,
            target_session_id = %session_id,
            reason = %error,
            "terminal workspace restore delete failed"
        );
        return Err(error);
    }

    let terminal_manager = state.terminal_manager.clone();
    let delete_session_id = session_id.clone();
    let removed = run_terminal_task(move || {
        terminal_manager
            .delete_workspace_restore_record(&delete_session_id)
            .map_err(|error| AppError::bad_request(format!("删除工作现场记录失败: {error}")))
    })
    .await?;
    warn!(
        requester = %audit.requester,
        client_addr = %audit.client_addr,
        request_source = %audit.request_source,
        target_session_id = %session_id,
        removed,
        "terminal workspace restore delete completed"
    );
    Ok(Json(serde_json::json!({ "ok": true, "removed": removed })))
}

pub async fn restore_terminal_workspaces(
    State(state): State<AppState>,
    Json(payload): Json<RestoreTerminalWorkspacesRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let terminal_manager = state.terminal_manager.clone();
    let result = run_terminal_task(move || {
        terminal_manager
            .restore_terminal_workspaces(&payload.session_ids)
            .map_err(|error| AppError::internal(format!("恢复终端工作现场失败：{error}")))
    })
    .await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "restored": result.restored_count(),
        "succeeded": result.succeeded,
        "skipped": result.skipped,
        "failed": result.failed,
    })))
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

        let lightweight_session = terminal_manager
            .list_all_sessions_lightweight(&state.workspace_root(), &state.workspace_display_root())
            .into_iter()
            .find(|session| session.id == session_id);
        let model_family = lightweight_session
            .map(|session| session.model_family)
            .unwrap_or_default();
        let agents = activity::TerminalAgentDetector::new()
            .detect(&session_id)
            .agents;
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
            model_family,
            codex_status: codex_status::detect_current_codex_status(&session_id),
            agents,
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
    query: &TerminalSessionQuery,
) -> ApiResult<(Arc<TerminalSession>, TerminalManager, bool)> {
    let started_at = Instant::now();
    let manager = state.terminal_manager.clone();
    let connect_manager = manager.clone();
    let requested_session_id = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let requested_session_id_for_log = requested_session_id
        .clone()
        .unwrap_or_else(|| "<latest>".to_string());
    let output_visible = query.visible;
    let session_lookup_started_at = Instant::now();
    let lookup_manager = manager.clone();
    let lookup_id = requested_session_id.clone();
    let registered_session_path = run_terminal_task(move || {
        Ok(lookup_id
            .as_deref()
            .and_then(|id| lookup_manager.session_path(id)))
    })
    .await?;
    let session_lookup_completed_at = Instant::now();
    let directory = match requested_session_id.as_deref() {
        Some(_) => match registered_session_path {
            Some(path) => path,
            None => {
                filesystem::resolve_terminal_directory_path(&state.workspace_root(), &query.path)?
            }
        },
        None => filesystem::resolve_terminal_directory_path(&state.workspace_root(), &query.path)?,
    };
    let path_resolved_at = Instant::now();
    let startup_loaded_at = Instant::now();
    let terminal_user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    let session_result = run_terminal_task(move || {
        connect_manager
            .get_for_connection(
                directory,
                requested_session_id.as_deref(),
                terminal_user,
                Vec::new(),
                None,
                String::new(),
                String::new(),
                output_visible,
            )
            .map_err(|error| AppError::internal(format!("创建终端会话失败: {error}")))
    })
    .await;

    let completed_at = Instant::now();
    let elapsed = completed_at.duration_since(started_at);
    if elapsed >= std::time::Duration::from_millis(250) || session_result.is_err() {
        warn!(
            requested_session_id = requested_session_id_for_log,
            session_lookup_ms = session_lookup_completed_at
                .duration_since(session_lookup_started_at)
                .as_millis(),
            path_validate_ms = path_resolved_at
                .duration_since(session_lookup_completed_at)
                .as_millis(),
            path_ms = path_resolved_at.duration_since(started_at).as_millis(),
            startup_ms = startup_loaded_at
                .duration_since(path_resolved_at)
                .as_millis(),
            session_ms = completed_at.duration_since(startup_loaded_at).as_millis(),
            total_ms = elapsed.as_millis(),
            outcome = if session_result.is_ok() {
                "ok"
            } else {
                "error"
            },
            "slow terminal websocket session preparation"
        );
    }
    let session = session_result?;

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

fn paste_asset_extension(file_name: Option<&str>, mime: &str) -> String {
    let extension = file_name
        .and_then(|name| Path::new(name).extension())
        .and_then(|extension| extension.to_str())
        .map(str::trim)
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 24
                && extension.chars().all(|char| char.is_ascii_alphanumeric())
        })
        .map(str::to_ascii_lowercase);
    extension.unwrap_or_else(|| {
        match mime
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "image/png" => "png",
            "image/jpeg" | "image/jpg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "text/plain" => "txt",
            "text/markdown" => "md",
            "application/pdf" => "pdf",
            "application/json" => "json",
            "application/zip" => "zip",
            _ => "bin",
        }
        .to_string()
    })
}

async fn unique_paste_asset_name(
    dir: &Path,
    uploaded_name: Option<&str>,
    mime: &str,
    ordinal: usize,
) -> String {
    let timestamp = current_timestamp_millis();
    let extension = paste_asset_extension(uploaded_name, mime);
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

const TERMINAL_TASK_TIMEOUT_SECS: u64 = 10;
const TERMINAL_TASK_QUEUE_WARN_MILLIS: u128 = 1000;
const TERMINAL_TASK_EXECUTE_WARN_MILLIS: u128 = 2000;
const TERMINAL_OUTPUT_VIEWED_MARK_INTERVAL_MS: u128 = 1000;

pub(in crate::terminal) async fn run_terminal_task<T, F>(task: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    let queued_at = Instant::now();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            let queue_wait_millis = queued_at.elapsed().as_millis();
            let started_at = Instant::now();
            let output = task();
            let execution_millis = started_at.elapsed().as_millis();
            if queue_wait_millis >= TERMINAL_TASK_QUEUE_WARN_MILLIS
                || execution_millis >= TERMINAL_TASK_EXECUTE_WARN_MILLIS
            {
                warn!(
                    queue_ms = queue_wait_millis,
                    execution_ms = execution_millis,
                    "slow terminal background task"
                );
            }
            output
        }),
    )
    .await;

    match result {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(error))) => Err(AppError::internal(format!("终端后台任务失败: {error}"))),
        Ok(Err(join_error)) => Err(AppError::internal(format!("终端后台任务失败: {join_error}"))),
        Err(_) => {
            warn!(
                timeout_secs = TERMINAL_TASK_TIMEOUT_SECS,
                elapsed_ms = queued_at.elapsed().as_millis(),
                "terminal task timed out; blocking work continues in background"
            );
            Err(AppError::internal(format!(
                "终端任务超时（{} 秒），请检查会话锁等待、磁盘负载及 tmux 响应。",
                TERMINAL_TASK_TIMEOUT_SECS
            )))
        }
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

pub(crate) fn deserialize_lenient_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct LenientBoolVisitor;

    impl<'de> serde::de::Visitor<'de> for LenientBoolVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean or boolean-like value ('true', 'false', '1', '0')")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v)
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v != 0)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v != 0)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match v.trim() {
                "1" | "true" | "TRUE" | "True" | "yes" | "on" => Ok(true),
                "0" | "false" | "FALSE" | "False" | "no" | "off" | "" => Ok(false),
                _ => Err(E::custom(format!("invalid boolean: {v}"))),
            }
        }
    }

    deserializer.deserialize_any(LenientBoolVisitor)
}

pub(crate) fn deserialize_lenient_bool_default_true<'de, D>(
    deserializer: D,
) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct LenientBoolVisitor;

    impl<'de> serde::de::Visitor<'de> for LenientBoolVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean or boolean-like value ('true', 'false', '1', '0')")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v)
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v != 0)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v != 0)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match v.trim() {
                "1" | "true" | "TRUE" | "True" | "yes" | "on" => Ok(true),
                "0" | "false" | "FALSE" | "False" | "no" | "off" => Ok(false),
                "" => Ok(true),
                _ => Err(E::custom(format!("invalid boolean: {v}"))),
            }
        }
    }

    deserializer.deserialize_any(LenientBoolVisitor)
}

#[cfg(test)]
mod tests;
