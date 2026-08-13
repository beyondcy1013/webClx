use std::{
    collections::HashSet,
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result};
pub(crate) use runtime_paths_core as runtime_paths;
mod api;
mod manager;
mod storage;

pub use api::{build_settings_response, merge_all, merge_field, merge_tab, save_settings};
use serde::{Deserialize, Serialize};
pub(crate) use storage::{load_saved_settings, persist_settings_file};

pub const DEFAULT_WORKSPACE_DIR: &str = "/home/codes";
pub const DEFAULT_TERMINAL_USER: &str = runtime_paths::DEFAULT_USER_NAME;
pub const DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY: &str = "1";
pub const DEFAULT_CODEX_CONFIG_KEY: &str = "model";
pub const DEFAULT_CODEX_MODEL: &str = "";
pub const DEFAULT_CODEX_SECONDARY_CONFIG_KEY: &str = "model_reasoning_effort";
pub const DEFAULT_CODEX_SECONDARY_CONFIG_VALUE: &str = "high";
pub const DEFAULT_SHOW_FULL_PATH: bool = true;
pub const DEFAULT_WORKSPACE_BROWSER_ICON_PATH: &str = "icon.ico";
pub const DEFAULT_TERMINAL_WORKSPACE_ICON_PATH: &str = "static/favicon.svg";
pub const DEFAULT_FONT_SIZE_TIER_1: f32 = 0.64;
pub const DEFAULT_FONT_SIZE_TIER_2: f32 = 0.68;
pub const DEFAULT_FONT_SIZE_TIER_3: f32 = 0.72;
pub const DEFAULT_FONT_SIZE_TIER_4: f32 = 0.74;
pub const DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH: f32 = 16.0;
pub const DEFAULT_TERMINAL_FAB_ACTION_COLOR: &str = "#f59e0b";
pub const DEFAULT_TERMINAL_FAB_ACTION_OPACITY: f32 = 0.5;
pub const DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE: f32 = 1.08;
pub const DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS: u32 = 2000;
pub const DEFAULT_TERMINAL_SCROLLBACK_LINES: u32 = 5_000;
pub const DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT: u32 = 12;
pub const DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS: u32 = 60;
pub const DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR: f64 = 1.5;
pub const DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES: u32 = 20;
pub const DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT: bool = true;
pub const DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY: TerminalActivityAgentDisplay =
    TerminalActivityAgentDisplay::Hidden;
pub const DEFAULT_TERMINAL_COMPLETION_BELL_ENABLED: bool = true;
pub const DEFAULT_SERVER_PORT_AUTO_INCREMENT: bool = true;
pub const TERMINAL_TOOL_ROOT_TOOLS: &str = "tools";
pub const TERMINAL_TOOL_ENTRY_KIND_FOLDER: &str = "folder";
pub const TERMINAL_TOOL_ENTRY_KIND_ACTION: &str = "action";
pub const TERMINAL_TOOL_ACTION_CREATE_TERMINAL: &str = "create_terminal";
pub const TERMINAL_TOOL_ACTION_FORK_SESSION: &str = "fork_session";
pub const TERMINAL_TOOL_ACTION_RENAME_TERMINAL: &str = "rename_terminal";
pub const TERMINAL_TOOL_ACTION_SWITCH_API_PRESET: &str = "switch_api_preset";
pub const TERMINAL_TOOL_ACTION_SWITCH_API_PRESET_REVERT: &str = "switch_api_preset_revert";
pub const TERMINAL_TOOL_ACTION_CODEX_EXEC: &str = "codex_exec";
pub const TERMINAL_TOOL_ACTION_CODEX_TERMINAL: &str = "codex_terminal";
pub const TERMINAL_TOOL_ACTION_WAIT: &str = "wait";
pub const TERMINAL_TOOL_ACTION_SEND_COMMAND: &str = "send_command";
pub const TERMINAL_TOOL_ACTION_CODEX_LAUNCH: &str = "codex_launch";
pub const TERMINAL_TOOL_ACTION_FUNCTION_COMMAND: &str = "function_command";
pub const TERMINAL_TOOL_ACTION_RUN_WORKFLOW: &str = "run_workflow";
pub const PRESET_MATCH_ID: &str = "id";
pub const PRESET_MATCH_EXACT_NAME: &str = "exact_name";
pub const PRESET_MATCH_UNIQUE_CONTAINS: &str = "unique_contains";
pub const SESSION_ACTION_NEW: &str = "new";
const MAX_TERMINAL_TOOL_ENTRIES: usize = 200;
const MAX_TERMINAL_TOOL_ACTIONS: usize = 20;
const MAX_TERMINAL_TOOL_ID_LEN: usize = 64;
const MAX_TERMINAL_TOOL_LABEL_LEN: usize = 64;
const MAX_TERMINAL_TOOL_ACTION_VALUE_LEN: usize = 4096;
const MAX_TERMINAL_TOOL_WAIT_SECONDS: f64 = 600.0;
/// 单个编译/安装命令的全局超时（秒）。编译 worker 用 `timeout` 强制终止
/// 超过该时长的命令，避免卡死占用 single-flight flock 拖垮整个队列。
pub const DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS: u64 = 600;
/// 允许的最小/最大编译命令超时（秒），normalizer 把越界值夹紧到此范围。
pub const MIN_COMPILE_COMMAND_TIMEOUT_SECS: u64 = 60;
pub const MAX_COMPILE_COMMAND_TIMEOUT_SECS: u64 = 3600;
pub const DEFAULT_COMPILE_MAX_CONCURRENCY: u32 = 5;
pub const MIN_COMPILE_MAX_CONCURRENCY: u32 = 1;
pub const MAX_COMPILE_MAX_CONCURRENCY: u32 = 32;
/// 对外网关开关：是否允许非 loopback 客户端访问 /api/upstream/* 与
/// /api/codex-proxy/* 代理路由。默认 false，仅本机终端可访问。
/// 详见 docs/codex/tasks/api-preset-routing-boundaries.md。
pub const DEFAULT_GATEWAY_LISTEN_NON_LOOPBACK: bool = false;
/// 登录会话保持天数，默认 30 天，范围 1–365。
pub const DEFAULT_SESSION_TTL_DAYS: u32 = 30;
pub const MIN_SESSION_TTL_DAYS: u32 = 1;
pub const MAX_SESSION_TTL_DAYS: u32 = 365;
const SELECTED_MODEL_CAPACITY_ERROR_KEYWORD: &str =
    "Selected model is at capacity. Please try a different model.";
const NONFATAL_MCP_STARTUP_SUMMARY: &str = "MCP startup incomplete";
/// Context window exhaustion: Codex prints this when the conversation no longer
/// fits. Sending "继续" alone would re-trigger it, so the auto-continue flow
/// must fire `/compact` first and only then send "继续".
pub const CONTEXT_WINDOW_EXHAUSTED_ERROR_KEYWORD: &str =
    "ran out of room in the model's context window";
/// OpenAI cybersecurity safety block: the assistant refuses to render a reply
/// and prints this guardrail page instead of an error. Treat it like any other
/// retryable block and auto-send "继续" so the turn is not left idle.
pub const OPENAI_CYBERSECURITY_BLOCK_TITLE_KEYWORD: &str = "This content can't be shown";
/// Same guardrail, matched without relying on the apostrophe (rendered
/// differently across fonts/terminals) for robustness.
pub const OPENAI_CYBERSECURITY_BLOCK_PHRASE_KEYWORD: &str =
    "extra caution with cybersecurity requests";
const WORKSPACE_ROOT_LIMIT: &str = "/home";
const SETTINGS_FILE_NAME: &str = "webclx-settings.json";
const MAX_WORKSPACE_HISTORY_ITEMS: usize = 50;
const MAX_TERMINAL_QUICK_COMMANDS: usize = 20;
const MAX_TERMINAL_QUICK_COMMAND_KEY_LEN: usize = 8;
const MAX_TERMINAL_QUICK_COMMAND_LABEL_LEN: usize = 24;
const MAX_TERMINAL_QUICK_COMMAND_PROGRAM_LEN: usize = 160;
const MAX_TERMINAL_QUICK_COMMAND_ARGS_LEN: usize = 500;
const MAX_TERMINAL_QUICK_COMMAND_COMMAND_LEN: usize = 1000;
const MAX_TERMINAL_FUNCTION_COMMANDS: usize = 20;
const MAX_TERMINAL_FUNCTION_COMMAND_KEY_LEN: usize = 64;
const MAX_TERMINAL_FUNCTION_COMMAND_LABEL_LEN: usize = 24;
const MAX_TERMINAL_FUNCTION_COMMAND_ACTION_LEN: usize = 64;
const MAX_TERMINAL_FUNCTION_COMMAND_COMMAND_LEN: usize = 1000;
const MAX_TERMINAL_FUNCTION_COMMAND_SHORTCUT_LEN: usize = 80;
const MAX_TERMINAL_COMMAND_COLLECTIONS: usize = 12;
const MAX_TERMINAL_COMMAND_COLLECTION_KEY_LEN: usize = 64;
const MAX_TERMINAL_COMMAND_COLLECTION_LABEL_LEN: usize = 24;
const MAX_TERMINAL_COMMAND_COLLECTION_ITEMS: usize = 40;
const MAX_TERMINAL_COMMAND_COLLECTION_ITEM_LABEL_LEN: usize = 32;
const MAX_TERMINAL_COMMAND_COLLECTION_ITEM_COMMAND_LEN: usize = 1000;
const MAX_TERMINAL_RENAME_PRESETS: usize = 20;
const MAX_TERMINAL_RENAME_PRESET_LEN: usize = 24;
const MAX_TERMINAL_DEFAULT_ENV_VARS: usize = 64;
const MAX_TERMINAL_DEFAULT_ENV_KEY_LEN: usize = 128;
const MAX_TERMINAL_DEFAULT_ENV_VALUE_LEN: usize = 4096;
const MAX_COMPILE_ENV_VARS: usize = 64;
const MAX_COMPILE_ENV_KEY_LEN: usize = 128;
const MAX_COMPILE_ENV_VALUE_LEN: usize = 4096;
const MAX_TERMINAL_ERROR_KEYWORDS: usize = 50;
const MAX_TERMINAL_ERROR_KEYWORD_LEN: usize = 200;
const MAX_TERMINAL_AUTO_CONTINUE_TIME_PATTERNS: usize = 20;
const MAX_TERMINAL_AUTO_CONTINUE_TIME_PATTERN_LEN: usize = 200;
const MAX_CODEX_DEFAULT_CONFIG_ENTRIES: usize = 20;
const CLAUDE_DEFAULT_HAIKU_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_HAIKU_MODEL";
const CLAUDE_DEFAULT_SONNET_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_SONNET_MODEL";
const CLAUDE_DEFAULT_OPUS_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_OPUS_MODEL";
const MAX_PRESET_SYNC_REMOTE_URL_HISTORY: usize = 20;
const MAX_DESKTOP_REMOTE_URL_HISTORY: usize = 20;
const DEFAULT_DESKTOP_REMOTE_URL: &str = "https://192.168.3.2:14083/";
const RESERVED_TERMINAL_DEFAULT_ENV_KEYS: [&str; 11] = [
    "HOME",
    "SHELL",
    "USER",
    "LOGNAME",
    "PWD",
    "OLDPWD",
    "SHLVL",
    "_",
    "TMUX",
    "TMUX_PANE",
    "TERM",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsErrorKind {
    BadRequest,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError {
    kind: SettingsErrorKind,
    message: String,
}

impl SettingsError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            kind: SettingsErrorKind::BadRequest,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: SettingsErrorKind::Internal,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> SettingsErrorKind {
        self.kind
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for SettingsError {}

pub type SettingsResult<T> = std::result::Result<T, SettingsError>;

#[derive(Clone)]
pub struct SettingsManager {
    current_root: Arc<RwLock<PathBuf>>,
    display_root: Arc<RwLock<PathBuf>>,
    terminal_user: Arc<RwLock<String>>,
    terminal_quick_commands: Arc<RwLock<Vec<TerminalQuickCommand>>>,
    terminal_quick_start_default_key: Arc<RwLock<String>>,
    terminal_default_env_vars: Arc<RwLock<Vec<TerminalDefaultEnvVar>>>,
    terminal_slash_commands: Arc<RwLock<Vec<TerminalFunctionCommand>>>,
    terminal_function_commands: Arc<RwLock<Vec<TerminalFunctionCommand>>>,
    terminal_command_collections: Arc<RwLock<Vec<TerminalCommandCollection>>>,
    terminal_tool_entries: Arc<RwLock<Vec<TerminalToolEntry>>>,
    terminal_rename_presets: Arc<RwLock<Vec<String>>>,
    show_dot_entries: Arc<RwLock<bool>>,
    show_all_workspace_sessions: Arc<RwLock<bool>>,
    desktop_terminal_soft_keyboard_enabled: Arc<RwLock<bool>>,
    terminal_soft_keyboard_scale: Arc<RwLock<f32>>,
    terminal_floating_button_offset_vh: Arc<RwLock<f32>>,
    terminal_fab_action_color: Arc<RwLock<String>>,
    terminal_fab_action_opacity: Arc<RwLock<f32>>,
    terminal_fab_auto_expand: Arc<RwLock<bool>>,
    terminal_touch_selection_long_press_ms: Arc<RwLock<u32>>,
    terminal_scrollback_lines: Arc<RwLock<u32>>,
    terminal_error_match_line_limit: Arc<RwLock<u32>>,
    terminal_auto_continue_on_error: Arc<RwLock<bool>>,
    terminal_auto_continue_interval_seconds: Arc<RwLock<u32>>,
    terminal_auto_continue_backoff_factor: Arc<RwLock<f64>>,
    terminal_auto_continue_backoff_max_minutes: Arc<RwLock<u32>>,
    terminal_auto_continue_respect_manual_interrupt: Arc<RwLock<bool>>,
    terminal_auto_continue_time_patterns: Arc<RwLock<Vec<String>>>,
    terminal_auto_continue_active_window: Arc<RwLock<String>>,
    terminal_scheduled_input_avoid_window: Arc<RwLock<String>>,
    terminal_error_keywords: Arc<RwLock<Vec<String>>>,
    terminal_error_keyword_actions: Arc<RwLock<Vec<TerminalErrorKeywordAction>>>,
    terminal_activity_agent_display: Arc<RwLock<TerminalActivityAgentDisplay>>,
    terminal_completion_bell_enabled: Arc<RwLock<bool>>,
    server_port_auto_increment: Arc<RwLock<bool>>,
    compile_command_timeout_secs: Arc<RwLock<u64>>,
    compile_max_concurrency: Arc<RwLock<u32>>,
    compile_environment: Arc<RwLock<Vec<CompileEnvVar>>>,
    gateway_listen_non_loopback: Arc<RwLock<bool>>,
    session_ttl_days: Arc<RwLock<u32>>,
    favorite_paths: Arc<RwLock<Vec<FavoritePath>>>,
    workspace_history: Arc<RwLock<Vec<WorkspaceHistoryItem>>>,
    preset_sync_remote_url_history: Arc<RwLock<Vec<String>>>,
    desktop_remote_url: Arc<RwLock<String>>,
    desktop_remote_url_history: Arc<RwLock<Vec<String>>>,
    claude_model_options: Arc<RwLock<Vec<String>>>,
    claude_default_config_entries: Arc<RwLock<Vec<CodexDefaultConfigEntry>>>,
    codex_default_config_entries: Arc<RwLock<Vec<CodexDefaultConfigEntry>>>,
    codex_api_auto_proxy_match_provider_ids: Arc<RwLock<Vec<String>>>,
    codex_config_key: Arc<RwLock<String>>,
    codex_config_value: Arc<RwLock<String>>,
    codex_secondary_config_key: Arc<RwLock<String>>,
    codex_secondary_config_value: Arc<RwLock<String>>,
    show_full_path: Arc<RwLock<bool>>,
    workspace_browser_icon_path: Arc<RwLock<String>>,
    terminal_workspace_icon_path: Arc<RwLock<String>>,
    theme_mode: Arc<RwLock<ThemeMode>>,
    font_size_tier_1: Arc<RwLock<f32>>,
    font_size_tier_2: Arc<RwLock<f32>>,
    font_size_tier_3: Arc<RwLock<f32>>,
    font_size_tier_4: Arc<RwLock<f32>>,
    config_path: Arc<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FavoritePathKind {
    Dir,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexDefaultConfigEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FavoritePath {
    path: String,
    kind: FavoritePathKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceHistoryItem {
    path: String,
    last_opened_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalActivityAgentDisplay {
    Hidden,
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemUserOption {
    name: String,
    uid: u32,
    gid: u32,
    home: String,
    shell: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalQuickCommand {
    key: String,
    label: String,
    #[serde(default)]
    command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    program: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    args: String,
}

impl TerminalQuickCommand {
    pub fn new(
        key: impl Into<String>,
        label: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            command: command.into(),
            program: String::new(),
            args: String::new(),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn command(&self) -> &str {
        &self.command
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileEnvVar {
    key: String,
    #[serde(default)]
    value: String,
}

/// Action taken when an error keyword is detected. "continue" sends "继续",
/// "compact_then_continue" sends "/compact" then "继续", "mark_only" shows the
/// error without auto-sending any command.
pub const TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE: &str = "continue";
pub const TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE: &str = "compact_then_continue";
pub const TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY: &str = "mark_only";

/// A single error-detection rule: the keyword to match and the action to take
/// when it is found in the terminal tail. Keywords without an explicit rule
/// default to "continue".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalErrorKeywordAction {
    pub keyword: String,
    #[serde(default = "default_terminal_error_keyword_action")]
    pub action: String,
}

pub fn default_terminal_error_keyword_action() -> String {
    TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalDefaultEnvVar {
    key: String,
    #[serde(default)]
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalFunctionCommand {
    key: String,
    label: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    shortcut: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandCollectionItem {
    label: String,
    #[serde(default = "default_collection_item_action")]
    action: String,
    #[serde(default)]
    command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalCommandCollection {
    key: String,
    label: String,
    #[serde(default)]
    commands: Vec<CommandCollectionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TerminalToolAction {
    kind: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    seconds: f64,
    #[serde(default)]
    preset_selector: String,
    #[serde(default)]
    preset_match: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    project_path: String,
    #[serde(default)]
    terminal_name: String,
    #[serde(default)]
    session_action: String,
    #[serde(default)]
    command_key: String,
    #[serde(default)]
    target_entry_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalToolEntry {
    id: String,
    root_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_id: Option<String>,
    kind: String,
    label: String,
    #[serde(default)]
    sort_order: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    actions: Vec<TerminalToolAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsResponse {
    workspace_dir: String,
    default_workspace_dir: String,
    terminal_user: String,
    default_terminal_user: String,
    terminal_user_home: String,
    terminal_user_shell: String,
    available_users: Vec<SystemUserOption>,
    terminal_quick_commands: Vec<TerminalQuickCommand>,
    terminal_quick_start_default_key: String,
    terminal_default_env_vars: Vec<TerminalDefaultEnvVar>,
    terminal_slash_commands: Vec<TerminalFunctionCommand>,
    terminal_function_commands: Vec<TerminalFunctionCommand>,
    terminal_command_collections: Vec<TerminalCommandCollection>,
    terminal_tool_entries: Vec<TerminalToolEntry>,
    terminal_rename_presets: Vec<String>,
    default_terminal_quick_commands: Vec<TerminalQuickCommand>,
    default_terminal_quick_start_default_key: String,
    default_terminal_default_env_vars: Vec<TerminalDefaultEnvVar>,
    default_terminal_slash_commands: Vec<TerminalFunctionCommand>,
    default_terminal_function_commands: Vec<TerminalFunctionCommand>,
    default_terminal_command_collections: Vec<TerminalCommandCollection>,
    default_terminal_tool_entries: Vec<TerminalToolEntry>,
    default_terminal_rename_presets: Vec<String>,
    host_name: String,
    server_listen_addr: String,
    show_dot_entries: bool,
    show_all_workspace_sessions: bool,
    desktop_terminal_soft_keyboard_enabled: bool,
    terminal_soft_keyboard_scale: f32,
    terminal_floating_button_offset_vh: f32,
    terminal_fab_action_color: String,
    terminal_fab_action_opacity: f32,
    terminal_fab_auto_expand: bool,
    terminal_touch_selection_long_press_ms: u32,
    terminal_scrollback_lines: u32,
    terminal_error_match_line_limit: u32,
    terminal_auto_continue_on_error: bool,
    terminal_auto_continue_interval_seconds: u32,
    terminal_auto_continue_backoff_factor: f64,
    terminal_auto_continue_backoff_max_minutes: u32,
    terminal_auto_continue_respect_manual_interrupt: bool,
    terminal_auto_continue_time_patterns: Vec<String>,
    terminal_auto_continue_active_window: String,
    terminal_scheduled_input_avoid_window: String,
    terminal_error_keywords: Vec<String>,
    terminal_error_keyword_actions: Vec<TerminalErrorKeywordAction>,
    terminal_activity_agent_display: TerminalActivityAgentDisplay,
    terminal_completion_bell_enabled: bool,
    server_port_auto_increment: bool,
    default_server_port_auto_increment: bool,
    compile_command_timeout_secs: u64,
    default_compile_command_timeout_secs: u64,
    compile_max_concurrency: u32,
    default_compile_max_concurrency: u32,
    compile_environment: Vec<CompileEnvVar>,
    gateway_listen_non_loopback: bool,
    default_gateway_listen_non_loopback: bool,
    session_ttl_days: u32,
    default_session_ttl_days: u32,
    favorite_paths: Vec<FavoritePath>,
    workspace_history: Vec<WorkspaceHistoryItem>,
    preset_sync_remote_url_history: Vec<String>,
    desktop_remote_url: String,
    desktop_remote_url_history: Vec<String>,
    claude_model_options: Vec<String>,
    claude_default_config_entries: Vec<CodexDefaultConfigEntry>,
    codex_default_config_entries: Vec<CodexDefaultConfigEntry>,
    codex_api_auto_proxy_match_provider_ids: Vec<String>,
    codex_config_key: String,
    codex_config_value: String,
    codex_secondary_config_key: String,
    codex_secondary_config_value: String,
    show_full_path: bool,
    workspace_browser_icon_path: String,
    terminal_workspace_icon_path: String,
    theme_mode: ThemeMode,
    font_size_tier_1: f32,
    font_size_tier_2: f32,
    font_size_tier_3: f32,
    font_size_tier_4: f32,
    config_file: String,
    server_version: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveSettingsRequest {
    #[serde(default)]
    workspace_dir: Option<String>,
    #[serde(default)]
    terminal_user: Option<String>,
    #[serde(default)]
    terminal_quick_commands: Option<Vec<TerminalQuickCommand>>,
    #[serde(default)]
    terminal_quick_start_default_key: Option<String>,
    #[serde(default)]
    terminal_default_env_vars: Option<Vec<TerminalDefaultEnvVar>>,
    #[serde(default)]
    terminal_slash_commands: Option<Vec<TerminalFunctionCommand>>,
    #[serde(default)]
    terminal_function_commands: Option<Vec<TerminalFunctionCommand>>,
    #[serde(default)]
    terminal_command_collections: Option<Vec<TerminalCommandCollection>>,
    #[serde(default)]
    terminal_tool_entries: Option<Vec<TerminalToolEntry>>,
    #[serde(default)]
    terminal_rename_presets: Option<Vec<String>>,
    #[serde(default)]
    show_dot_entries: Option<bool>,
    #[serde(default)]
    show_all_workspace_sessions: Option<bool>,
    #[serde(default)]
    desktop_terminal_soft_keyboard_enabled: Option<bool>,
    #[serde(default)]
    terminal_soft_keyboard_scale: Option<f32>,
    #[serde(default)]
    terminal_floating_button_offset_vh: Option<f32>,
    #[serde(default)]
    terminal_fab_action_color: Option<String>,
    #[serde(default)]
    terminal_fab_action_opacity: Option<f32>,
    #[serde(default)]
    terminal_fab_auto_expand: Option<bool>,
    #[serde(default)]
    terminal_touch_selection_long_press_ms: Option<u32>,
    #[serde(default)]
    terminal_scrollback_lines: Option<u32>,
    #[serde(default)]
    terminal_error_match_line_limit: Option<u32>,
    #[serde(default)]
    terminal_auto_continue_on_error: Option<bool>,
    #[serde(default)]
    terminal_auto_continue_interval_seconds: Option<u32>,
    #[serde(default)]
    terminal_auto_continue_backoff_factor: Option<f64>,
    #[serde(default)]
    terminal_auto_continue_backoff_max_minutes: Option<u32>,
    #[serde(default)]
    terminal_auto_continue_respect_manual_interrupt: Option<bool>,
    #[serde(default)]
    terminal_auto_continue_time_patterns: Option<Vec<String>>,
    #[serde(default)]
    terminal_auto_continue_active_window: Option<String>,
    #[serde(default)]
    terminal_scheduled_input_avoid_window: Option<String>,
    #[serde(default)]
    terminal_error_keywords: Option<Vec<String>>,
    #[serde(default)]
    terminal_error_keyword_actions: Option<Vec<TerminalErrorKeywordAction>>,
    #[serde(default)]
    terminal_activity_agent_display: Option<TerminalActivityAgentDisplay>,
    #[serde(default)]
    terminal_completion_bell_enabled: Option<bool>,
    #[serde(default)]
    server_port_auto_increment: Option<bool>,
    #[serde(default)]
    compile_command_timeout_secs: Option<u64>,
    #[serde(default)]
    compile_max_concurrency: Option<u32>,
    #[serde(default)]
    compile_environment: Option<Vec<CompileEnvVar>>,
    #[serde(default)]
    gateway_listen_non_loopback: Option<bool>,
    #[serde(default)]
    session_ttl_days: Option<u32>,
    #[serde(default)]
    favorite_paths: Option<Vec<FavoritePath>>,
    #[serde(default)]
    workspace_history: Option<Vec<WorkspaceHistoryItem>>,
    #[serde(default)]
    preset_sync_remote_url_history: Option<Vec<String>>,
    #[serde(default)]
    desktop_remote_url: Option<String>,
    #[serde(default)]
    desktop_remote_url_history: Option<Vec<String>>,
    #[serde(default)]
    claude_model_options: Option<Vec<String>>,
    #[serde(default)]
    claude_default_config_entries: Option<Vec<CodexDefaultConfigEntry>>,
    #[serde(default)]
    codex_default_config_entries: Option<Vec<CodexDefaultConfigEntry>>,
    #[serde(default)]
    codex_api_auto_proxy_match_provider_ids: Option<Vec<String>>,
    #[serde(default)]
    codex_config_key: Option<String>,
    #[serde(default)]
    codex_config_value: Option<String>,
    #[serde(default)]
    codex_secondary_config_key: Option<String>,
    #[serde(default)]
    codex_secondary_config_value: Option<String>,
    #[serde(default)]
    codex_model: Option<String>,
    #[serde(default)]
    show_full_path: Option<bool>,
    #[serde(default)]
    workspace_browser_icon_path: Option<String>,
    #[serde(default)]
    terminal_workspace_icon_path: Option<String>,
    #[serde(default)]
    theme_mode: Option<ThemeMode>,
    #[serde(default)]
    font_size_tier_1: Option<f32>,
    #[serde(default)]
    font_size_tier_2: Option<f32>,
    #[serde(default)]
    font_size_tier_3: Option<f32>,
    #[serde(default)]
    font_size_tier_4: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct SaveSettingsResponse {
    ok: bool,
    workspace_dir: String,
    terminal_user: String,
    terminal_user_home: String,
    terminal_user_shell: String,
    terminal_quick_commands: Vec<TerminalQuickCommand>,
    terminal_quick_start_default_key: String,
    terminal_default_env_vars: Vec<TerminalDefaultEnvVar>,
    terminal_slash_commands: Vec<TerminalFunctionCommand>,
    terminal_function_commands: Vec<TerminalFunctionCommand>,
    terminal_command_collections: Vec<TerminalCommandCollection>,
    terminal_tool_entries: Vec<TerminalToolEntry>,
    terminal_rename_presets: Vec<String>,
    show_dot_entries: bool,
    show_all_workspace_sessions: bool,
    desktop_terminal_soft_keyboard_enabled: bool,
    terminal_soft_keyboard_scale: f32,
    terminal_floating_button_offset_vh: f32,
    terminal_fab_action_color: String,
    terminal_fab_action_opacity: f32,
    terminal_fab_auto_expand: bool,
    terminal_touch_selection_long_press_ms: u32,
    terminal_scrollback_lines: u32,
    terminal_error_match_line_limit: u32,
    terminal_auto_continue_on_error: bool,
    terminal_auto_continue_interval_seconds: u32,
    terminal_auto_continue_backoff_factor: f64,
    terminal_auto_continue_backoff_max_minutes: u32,
    terminal_auto_continue_respect_manual_interrupt: bool,
    terminal_auto_continue_time_patterns: Vec<String>,
    terminal_auto_continue_active_window: String,
    terminal_scheduled_input_avoid_window: String,
    terminal_error_keywords: Vec<String>,
    terminal_error_keyword_actions: Vec<TerminalErrorKeywordAction>,
    terminal_activity_agent_display: TerminalActivityAgentDisplay,
    terminal_completion_bell_enabled: bool,
    server_port_auto_increment: bool,
    compile_command_timeout_secs: u64,
    compile_max_concurrency: u32,
    compile_environment: Vec<CompileEnvVar>,
    gateway_listen_non_loopback: bool,
    session_ttl_days: u32,
    favorite_paths: Vec<FavoritePath>,
    workspace_history: Vec<WorkspaceHistoryItem>,
    preset_sync_remote_url_history: Vec<String>,
    desktop_remote_url: String,
    desktop_remote_url_history: Vec<String>,
    claude_model_options: Vec<String>,
    claude_default_config_entries: Vec<CodexDefaultConfigEntry>,
    codex_default_config_entries: Vec<CodexDefaultConfigEntry>,
    codex_api_auto_proxy_match_provider_ids: Vec<String>,
    codex_config_key: String,
    codex_config_value: String,
    codex_secondary_config_key: String,
    codex_secondary_config_value: String,
    show_full_path: bool,
    workspace_browser_icon_path: String,
    terminal_workspace_icon_path: String,
    theme_mode: ThemeMode,
    font_size_tier_1: f32,
    font_size_tier_2: f32,
    font_size_tier_3: f32,
    font_size_tier_4: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeFieldRequest {
    pub remote_url: String,
    pub field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeFieldResponse {
    pub field: String,
    pub merged_value: serde_json::Value,
    pub merge_type: MergeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeTabRequest {
    pub remote_url: String,
    pub tab: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeTabResponse {
    pub tab: String,
    pub applied: bool,
    pub field_count: usize,
    pub skipped_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeAllRequest {
    pub remote_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeAllResponse {
    pub applied: bool,
    pub field_count: usize,
    pub skipped_fields: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeType {
    UnionArray,
    ScalarReplace,
    Skipped,
}

#[derive(Debug, Serialize, Deserialize)]
struct SettingsFile {
    workspace_dir: String,
    #[serde(default = "default_terminal_user")]
    terminal_user: String,
    #[serde(default = "default_terminal_quick_commands")]
    terminal_quick_commands: Vec<TerminalQuickCommand>,
    #[serde(default = "default_terminal_quick_start_default_key")]
    terminal_quick_start_default_key: String,
    #[serde(default = "default_terminal_default_env_vars")]
    terminal_default_env_vars: Vec<TerminalDefaultEnvVar>,
    #[serde(default = "default_terminal_slash_commands")]
    terminal_slash_commands: Vec<TerminalFunctionCommand>,
    #[serde(default = "default_terminal_function_commands")]
    terminal_function_commands: Vec<TerminalFunctionCommand>,
    #[serde(default = "default_terminal_command_collections")]
    terminal_command_collections: Vec<TerminalCommandCollection>,
    #[serde(default = "default_terminal_tool_entries")]
    terminal_tool_entries: Vec<TerminalToolEntry>,
    #[serde(default = "default_terminal_rename_presets")]
    terminal_rename_presets: Vec<String>,
    #[serde(default = "default_show_dot_entries")]
    show_dot_entries: bool,
    #[serde(default = "default_show_all_workspace_sessions")]
    show_all_workspace_sessions: bool,
    #[serde(default = "default_desktop_terminal_soft_keyboard_enabled")]
    desktop_terminal_soft_keyboard_enabled: bool,
    #[serde(default = "default_terminal_soft_keyboard_scale")]
    terminal_soft_keyboard_scale: f32,
    #[serde(default = "default_terminal_floating_button_offset_vh")]
    terminal_floating_button_offset_vh: f32,
    #[serde(default = "default_terminal_fab_action_color")]
    terminal_fab_action_color: String,
    #[serde(default = "default_terminal_fab_action_opacity")]
    terminal_fab_action_opacity: f32,
    #[serde(default = "default_terminal_fab_auto_expand")]
    terminal_fab_auto_expand: bool,
    #[serde(default = "default_terminal_touch_selection_long_press_ms")]
    terminal_touch_selection_long_press_ms: u32,
    #[serde(default = "default_terminal_scrollback_lines")]
    terminal_scrollback_lines: u32,
    #[serde(default = "default_terminal_error_match_line_limit")]
    terminal_error_match_line_limit: u32,
    #[serde(default = "default_terminal_auto_continue_on_error")]
    terminal_auto_continue_on_error: bool,
    #[serde(default = "default_terminal_auto_continue_interval_seconds")]
    terminal_auto_continue_interval_seconds: u32,
    #[serde(default = "default_terminal_auto_continue_backoff_factor")]
    terminal_auto_continue_backoff_factor: f64,
    #[serde(default = "default_terminal_auto_continue_backoff_max_minutes")]
    terminal_auto_continue_backoff_max_minutes: u32,
    #[serde(default = "default_terminal_auto_continue_respect_manual_interrupt")]
    terminal_auto_continue_respect_manual_interrupt: bool,
    #[serde(default = "default_terminal_auto_continue_time_patterns")]
    terminal_auto_continue_time_patterns: Vec<String>,
    #[serde(default = "default_terminal_auto_continue_active_window")]
    terminal_auto_continue_active_window: String,
    #[serde(default = "default_terminal_scheduled_input_avoid_window")]
    terminal_scheduled_input_avoid_window: String,
    #[serde(default = "default_terminal_error_keywords")]
    terminal_error_keywords: Vec<String>,
    #[serde(default = "default_terminal_error_keyword_actions")]
    terminal_error_keyword_actions: Vec<TerminalErrorKeywordAction>,
    #[serde(default = "default_terminal_activity_agent_display")]
    terminal_activity_agent_display: TerminalActivityAgentDisplay,
    #[serde(default = "default_terminal_completion_bell_enabled")]
    terminal_completion_bell_enabled: bool,
    #[serde(default = "default_server_port_auto_increment")]
    server_port_auto_increment: bool,
    #[serde(default = "default_compile_command_timeout_secs")]
    compile_command_timeout_secs: u64,
    #[serde(default = "default_compile_max_concurrency")]
    compile_max_concurrency: u32,
    #[serde(default = "default_compile_environment")]
    compile_environment: Vec<CompileEnvVar>,
    #[serde(default = "default_gateway_listen_non_loopback")]
    gateway_listen_non_loopback: bool,
    #[serde(default = "default_session_ttl_days")]
    session_ttl_days: u32,
    #[serde(default)]
    favorite_paths: Vec<FavoritePath>,
    #[serde(default)]
    workspace_history: Vec<WorkspaceHistoryItem>,
    #[serde(default)]
    preset_sync_remote_url_history: Vec<String>,
    #[serde(default = "default_desktop_remote_url")]
    desktop_remote_url: String,
    #[serde(default)]
    desktop_remote_url_history: Vec<String>,
    #[serde(default = "default_claude_model_options")]
    claude_model_options: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    claude_default_config_entries: Vec<CodexDefaultConfigEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    codex_default_config_entries: Vec<CodexDefaultConfigEntry>,
    #[serde(default = "default_codex_api_auto_proxy_match_provider_ids")]
    codex_api_auto_proxy_match_provider_ids: Vec<String>,
    #[serde(default = "default_codex_config_key")]
    codex_config_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_config_value: Option<String>,
    #[serde(default = "default_codex_secondary_config_key")]
    codex_secondary_config_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_secondary_config_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_model: Option<String>,
    #[serde(default = "default_show_full_path")]
    show_full_path: bool,
    #[serde(default = "default_workspace_browser_icon_path")]
    workspace_browser_icon_path: String,
    #[serde(default = "default_terminal_workspace_icon_path")]
    terminal_workspace_icon_path: String,
    /// Legacy field migrated to show_full_path on load.
    #[serde(default, skip_serializing)]
    path_display_prefix: Option<String>,
    #[serde(default = "default_theme_mode")]
    theme_mode: ThemeMode,
    #[serde(default = "default_font_size_tier_1")]
    font_size_tier_1: f32,
    #[serde(default = "default_font_size_tier_2")]
    font_size_tier_2: f32,
    #[serde(default = "default_font_size_tier_3")]
    font_size_tier_3: f32,
    #[serde(default = "default_font_size_tier_4")]
    font_size_tier_4: f32,
}

#[derive(Debug)]
struct LoadedSettings {
    workspace_dir: PathBuf,
    display_workspace_dir: PathBuf,
    terminal_user: String,
    terminal_quick_commands: Vec<TerminalQuickCommand>,
    terminal_quick_start_default_key: String,
    terminal_default_env_vars: Vec<TerminalDefaultEnvVar>,
    terminal_slash_commands: Vec<TerminalFunctionCommand>,
    terminal_function_commands: Vec<TerminalFunctionCommand>,
    terminal_command_collections: Vec<TerminalCommandCollection>,
    terminal_tool_entries: Vec<TerminalToolEntry>,
    terminal_rename_presets: Vec<String>,
    show_dot_entries: bool,
    show_all_workspace_sessions: bool,
    desktop_terminal_soft_keyboard_enabled: bool,
    terminal_soft_keyboard_scale: f32,
    terminal_floating_button_offset_vh: f32,
    terminal_fab_action_color: String,
    terminal_fab_action_opacity: f32,
    terminal_fab_auto_expand: bool,
    terminal_touch_selection_long_press_ms: u32,
    terminal_scrollback_lines: u32,
    terminal_error_match_line_limit: u32,
    terminal_auto_continue_on_error: bool,
    terminal_auto_continue_interval_seconds: u32,
    terminal_auto_continue_backoff_factor: f64,
    terminal_auto_continue_backoff_max_minutes: u32,
    terminal_auto_continue_respect_manual_interrupt: bool,
    terminal_auto_continue_time_patterns: Vec<String>,
    terminal_auto_continue_active_window: String,
    terminal_scheduled_input_avoid_window: String,
    terminal_error_keywords: Vec<String>,
    terminal_error_keyword_actions: Vec<TerminalErrorKeywordAction>,
    terminal_activity_agent_display: TerminalActivityAgentDisplay,
    terminal_completion_bell_enabled: bool,
    server_port_auto_increment: bool,
    compile_command_timeout_secs: u64,
    compile_max_concurrency: u32,
    compile_environment: Vec<CompileEnvVar>,
    gateway_listen_non_loopback: bool,
    session_ttl_days: u32,
    favorite_paths: Vec<FavoritePath>,
    workspace_history: Vec<WorkspaceHistoryItem>,
    preset_sync_remote_url_history: Vec<String>,
    desktop_remote_url: String,
    desktop_remote_url_history: Vec<String>,
    claude_model_options: Vec<String>,
    claude_default_config_entries: Vec<CodexDefaultConfigEntry>,
    codex_default_config_entries: Vec<CodexDefaultConfigEntry>,
    codex_api_auto_proxy_match_provider_ids: Vec<String>,
    codex_config_key: String,
    codex_config_value: String,
    codex_secondary_config_key: String,
    codex_secondary_config_value: String,
    show_full_path: bool,
    workspace_browser_icon_path: String,
    terminal_workspace_icon_path: String,
    theme_mode: ThemeMode,
    font_size_tier_1: f32,
    font_size_tier_2: f32,
    font_size_tier_3: f32,
    font_size_tier_4: f32,
}

fn default_show_dot_entries() -> bool {
    false
}

fn default_show_all_workspace_sessions() -> bool {
    true
}

fn default_desktop_terminal_soft_keyboard_enabled() -> bool {
    true
}

fn default_terminal_soft_keyboard_scale() -> f32 {
    DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE
}

fn default_terminal_floating_button_offset_vh() -> f32 {
    DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH
}

fn default_terminal_fab_action_color() -> String {
    DEFAULT_TERMINAL_FAB_ACTION_COLOR.to_string()
}

fn default_terminal_fab_action_opacity() -> f32 {
    DEFAULT_TERMINAL_FAB_ACTION_OPACITY
}

fn default_terminal_fab_auto_expand() -> bool {
    true
}

fn default_terminal_touch_selection_long_press_ms() -> u32 {
    DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS
}

fn default_terminal_scrollback_lines() -> u32 {
    DEFAULT_TERMINAL_SCROLLBACK_LINES
}

fn default_terminal_error_match_line_limit() -> u32 {
    DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT
}

fn default_terminal_auto_continue_on_error() -> bool {
    false
}

fn default_terminal_auto_continue_interval_seconds() -> u32 {
    DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS
}

fn default_terminal_auto_continue_backoff_factor() -> f64 {
    DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR
}

fn default_terminal_auto_continue_backoff_max_minutes() -> u32 {
    DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES
}

fn default_terminal_auto_continue_respect_manual_interrupt() -> bool {
    DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT
}

fn default_terminal_auto_continue_time_patterns() -> Vec<String> {
    vec![
        "限额将在 {time} 重置".to_string(),
        "reset at {time}".to_string(),
        "resets at {time}".to_string(),
        "will reset at {time}".to_string(),
    ]
}

fn default_terminal_auto_continue_active_window() -> String {
    String::new()
}

fn default_terminal_scheduled_input_avoid_window() -> String {
    "14:00-18:00".to_string()
}

fn default_terminal_error_keywords() -> Vec<String> {
    vec![
        "stream disconnected before completion:".to_string(),
        "Concurrency limit exceeded for user, please retry later".to_string(),
        SELECTED_MODEL_CAPACITY_ERROR_KEYWORD.to_string(),
        CONTEXT_WINDOW_EXHAUSTED_ERROR_KEYWORD.to_string(),
        "API Error: Request rejected (429)".to_string(),
        "已达到 5 小时的使用上限".to_string(),
        "sending request for url".to_string(),
        "(https://ai.router.team/responses)".to_string(),
        "exceeded retry limit".to_string(),
        "last status: 429".to_string(),
        "last status: 503".to_string(),
        "last status: 404".to_string(),
        "unexpected status 502 Bad Gateway: Upstream service temporarily unavailable, url:"
            .to_string(),
        "429 Too Many Requests".to_string(),
        "503 Service Unavailable".to_string(),
        "404 Not Found".to_string(),
        OPENAI_CYBERSECURITY_BLOCK_TITLE_KEYWORD.to_string(),
        OPENAI_CYBERSECURITY_BLOCK_PHRASE_KEYWORD.to_string(),
    ]
}

/// Built-in action rules. Only keywords needing a non-default action are
/// listed; everything else implicitly defaults to "continue".
fn default_terminal_error_keyword_actions() -> Vec<TerminalErrorKeywordAction> {
    vec![
        TerminalErrorKeywordAction {
            keyword: CONTEXT_WINDOW_EXHAUSTED_ERROR_KEYWORD.to_string(),
            action: TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE.to_string(),
        },
        TerminalErrorKeywordAction {
            keyword: "last status: 404".to_string(),
            action: TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY.to_string(),
        },
        TerminalErrorKeywordAction {
            keyword: "404 Not Found".to_string(),
            action: TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY.to_string(),
        },
    ]
}

fn sanitize_terminal_error_keyword_actions(
    actions: &[TerminalErrorKeywordAction],
) -> Vec<TerminalErrorKeywordAction> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();
    for action in actions {
        let keyword = action
            .keyword
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if keyword.is_empty()
            || keyword.chars().any(char::is_control)
            || keyword.eq_ignore_ascii_case(NONFATAL_MCP_STARTUP_SUMMARY)
        {
            continue;
        }
        let keyword = truncate_chars(&keyword, MAX_TERMINAL_ERROR_KEYWORD_LEN);
        let key = keyword.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let normalized_action = match action.action.as_str() {
            TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE
            | TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE
            | TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY => action.action.clone(),
            _ => default_terminal_error_keyword_action(),
        };
        sanitized.push(TerminalErrorKeywordAction {
            keyword,
            action: normalized_action,
        });
    }
    sanitized
}

fn merge_builtin_terminal_error_keyword_actions(
    actions: &[TerminalErrorKeywordAction],
) -> Vec<TerminalErrorKeywordAction> {
    let mut merged = sanitize_terminal_error_keyword_actions(actions);
    for builtin in default_terminal_error_keyword_actions() {
        if let Some(existing) = merged
            .iter_mut()
            .find(|action| action.keyword.eq_ignore_ascii_case(&builtin.keyword))
        {
            if builtin.action == TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY {
                existing.action = builtin.action;
            }
        } else {
            merged.push(builtin);
        }
    }
    merged
}

fn default_terminal_activity_agent_display() -> TerminalActivityAgentDisplay {
    DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY
}

fn default_terminal_completion_bell_enabled() -> bool {
    DEFAULT_TERMINAL_COMPLETION_BELL_ENABLED
}

fn default_server_port_auto_increment() -> bool {
    DEFAULT_SERVER_PORT_AUTO_INCREMENT
}

fn default_compile_command_timeout_secs() -> u64 {
    DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS
}

fn default_compile_max_concurrency() -> u32 {
    DEFAULT_COMPILE_MAX_CONCURRENCY
}

fn default_session_ttl_days() -> u32 {
    DEFAULT_SESSION_TTL_DAYS
}

fn normalize_session_ttl_days(value: u32) -> u32 {
    value.clamp(MIN_SESSION_TTL_DAYS, MAX_SESSION_TTL_DAYS)
}

/// 夹紧编译命令超时到 `[MIN, MAX]`；非数字或越界值收敛到合法范围。
pub fn normalize_compile_command_timeout_secs(value: u64) -> u64 {
    value.clamp(MIN_COMPILE_COMMAND_TIMEOUT_SECS, MAX_COMPILE_COMMAND_TIMEOUT_SECS)
}

pub fn normalize_compile_max_concurrency(value: u32) -> u32 {
    value.clamp(MIN_COMPILE_MAX_CONCURRENCY, MAX_COMPILE_MAX_CONCURRENCY)
}

fn default_gateway_listen_non_loopback() -> bool {
    DEFAULT_GATEWAY_LISTEN_NON_LOOPBACK
}

fn default_terminal_user() -> String {
    if cfg!(windows) {
        return runtime_paths::resolve_current_user_profile()
            .map(|profile| profile.name)
            .unwrap_or_else(|| DEFAULT_TERMINAL_USER.to_string());
    }

    DEFAULT_TERMINAL_USER.to_string()
}

fn default_terminal_quick_commands() -> Vec<TerminalQuickCommand> {
    vec![
        TerminalQuickCommand {
            key: "1".to_string(),
            label: "codex".to_string(),
            command: "codex".to_string(),
            program: String::new(),
            args: String::new(),
        },
        TerminalQuickCommand {
            key: "2".to_string(),
            label: "claude".to_string(),
            command: "claude".to_string(),
            program: String::new(),
            args: String::new(),
        },
    ]
}

fn default_terminal_quick_start_default_key() -> String {
    DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY.to_string()
}

fn default_terminal_default_env_vars() -> Vec<TerminalDefaultEnvVar> {
    Vec::new()
}

fn default_compile_environment() -> Vec<CompileEnvVar> {
    Vec::new()
}

fn default_terminal_slash_commands() -> Vec<TerminalFunctionCommand> {
    vec![
        TerminalFunctionCommand {
            key: "resume_current_session".to_string(),
            label: "恢复会话".to_string(),
            action: "resume_current_agent_session".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "continue".to_string(),
            label: "继续".to_string(),
            action: "send_text".to_string(),
            command: "继续".to_string(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "enter".to_string(),
            label: "Enter".to_string(),
            action: "send_sequence".to_string(),
            command: "enter".to_string(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "extract_resume".to_string(),
            label: "屏幕提取id并恢复".to_string(),
            action: "extract_resume".to_string(),
            command: String::new(),
            shortcut: "Ctrl+Alt+R".to_string(),
        },
        TerminalFunctionCommand {
            key: "copy_resume_id".to_string(),
            label: "屏幕提取id".to_string(),
            action: "copy_resume_id".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "extract_current_session".to_string(),
            label: "高级复制".to_string(),
            action: "extract_current_session".to_string(),
            command: String::new(),
            shortcut: "Ctrl+Alt+S".to_string(),
        },
        TerminalFunctionCommand {
            key: "current_resume_id".to_string(),
            label: "session ID".to_string(),
            action: "copy_current_resume_id".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "copy_id_and_ask".to_string(),
            label: "复制id并提问".to_string(),
            action: "copy_id_and_ask".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "copy_terminal_name".to_string(),
            label: "复制终端名".to_string(),
            action: "copy_terminal_name".to_string(),
            command: String::new(),
            shortcut: "Ctrl+Alt+T".to_string(),
        },
        TerminalFunctionCommand {
            key: "resume".to_string(),
            label: "/resume".to_string(),
            action: "send_slash_command".to_string(),
            command: "/resume".to_string(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "status".to_string(),
            label: "/status".to_string(),
            action: "send_slash_command".to_string(),
            command: "/status".to_string(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "fork".to_string(),
            label: "/fork".to_string(),
            action: "send_slash_command".to_string(),
            command: "/fork".to_string(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "compact".to_string(),
            label: "/compact".to_string(),
            action: "send_slash_command".to_string(),
            command: "/compact".to_string(),
            shortcut: String::new(),
        },
    ]
}

fn default_terminal_function_commands() -> Vec<TerminalFunctionCommand> {
    vec![
        TerminalFunctionCommand {
            key: "system_keyboard".to_string(),
            label: "弹出系统键盘".to_string(),
            action: "show_system_keyboard".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "disable_keyboard".to_string(),
            label: "禁用系统键盘".to_string(),
            action: "disable_system_keyboard".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "copy_terminal_name".to_string(),
            label: "终端名".to_string(),
            action: "copy_terminal_name".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "copy_window".to_string(),
            label: "新窗口复制".to_string(),
            action: "copy_terminal_view_in_new_window".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "reload_claude".to_string(),
            label: "重读 Claude".to_string(),
            action: "reload_claude".to_string(),
            command: "claude".to_string(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "toggle_width".to_string(),
            label: "宽屏".to_string(),
            action: "toggle_terminal_width".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "save_and_poweroff".to_string(),
            label: "保存会话并关机".to_string(),
            action: "save_and_poweroff".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "save_and_restart".to_string(),
            label: "保存会话并重启服务".to_string(),
            action: "save_and_restart".to_string(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "qoderclicn".to_string(),
            label: "qoderclicn".to_string(),
            action: "send_text".to_string(),
            command: "qodercli --permission-mode bypass_permissions".to_string(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "agy".to_string(),
            label: "agy".to_string(),
            action: "send_text".to_string(),
            command: "agy".to_string(),
            shortcut: String::new(),
        },
    ]
}

fn default_collection_item_action() -> String {
    "send_text".to_string()
}

fn collection_item(label: &str, command: &str) -> CommandCollectionItem {
    CommandCollectionItem {
        label: label.to_string(),
        action: default_collection_item_action(),
        command: command.to_string(),
    }
}

fn default_terminal_command_collections() -> Vec<TerminalCommandCollection> {
    vec![
        TerminalCommandCollection {
            key: "rescue_install".to_string(),
            label: "Rescue 安装".to_string(),
            commands: vec![
                collection_item(
                    "Claude 原生安装",
                    "curl -fsSL https://claude.ai/install.sh | bash",
                ),
                collection_item(
                    "Claude 官方源安装",
                    "npm install -g @anthropic-ai/claude-code --registry=https://registry.npmjs.org",
                ),
                collection_item("Zhipu", "npx @z_ai/coding-helper"),
                collection_item(
                    "清华一键换源",
                    "bash <(curl -sSL https://mirrors.tuna.tsinghua.edu.cn/static/tunasync-scripts/update.sh)",
                ),
                collection_item(
                    "NodeSource 安装 Node22",
                    "curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash - && sudo apt install nodejs -y",
                ),
                collection_item("安装 Git", "sudo apt install git -y"),
                collection_item(
                    "Codex 一键安装",
                    "curl -o install-codex.sh https://1.api5.ai/install-codex.sh && bash install-codex.sh",
                ),
                collection_item("Codex npm 安装", "sudo npm install -g @openai/codex"),
                collection_item(
                    "Codex 二进制安装",
                    "wget https://github.com/openai/codex/releases/latest/download/codex-x86_64-unknown-linux-musl.tar.gz && tar -xzf codex-x86_64-unknown-linux-musl.tar.gz && sudo mv codex-x86_64-unknown-linux-musl /usr/local/bin/codex && sudo chmod +x /usr/local/bin/codex",
                ),
                collection_item(
                    "OpenCode 一键安装",
                    "curl -fsSL https://opencode.ai/install | bash",
                ),
                collection_item("OpenCode npm 安装", "npm install -g opencode-ai@latest"),
            ],
        },
        TerminalCommandCollection {
            key: "rescue_nodejs".to_string(),
            label: "Node.js 安装".to_string(),
            commands: vec![
                collection_item("dnf 更新", "sudo dnf update -y"),
                collection_item("dnf 安装 nodejs", "sudo dnf install -y nodejs npm"),
                collection_item("node 版本", "node -v"),
                collection_item("npm 版本", "npm -v"),
                collection_item(
                    "安装 NVM",
                    "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash",
                ),
                collection_item("source bashrc", "source ~/.bashrc"),
                collection_item("nvm 安装 22", "nvm install 22"),
                collection_item("nvm 使用 22", "nvm use 22"),
                collection_item("nvm 默认 22", "nvm alias default 22"),
                collection_item(
                    "下载 Node 二进制包",
                    "curl -LO https://nodejs.org/dist/v20.11.0/node-v20.11.0-linux-x64.tar.xz && tar -xvf node-v20.11.0-linux-x64.tar.xz",
                ),
                collection_item(
                    "移动 Node 到系统目录",
                    "sudo mkdir -p /usr/local/lib/nodejs && sudo mv node-v20.11.0-linux-x64 /usr/local/lib/nodejs/",
                ),
                collection_item("安装编译依赖", "sudo dnf install -y gcc gcc-c++ make python3"),
                collection_item(
                    "下载并编译 Node 源码",
                    "curl -LO https://nodejs.org/dist/v20.11.0/node-v20.11.0.tar.gz && tar -xzf node-v20.11.0.tar.gz && cd node-v20.11.0 && ./configure && make -j\"$(nproc)\"",
                ),
                collection_item("安装编译结果", "sudo make install"),
            ],
        },
        TerminalCommandCollection {
            key: "rescue_third_party".to_string(),
            label: "第三方工具".to_string(),
            commands: vec![
                collection_item("安装 OpenClaude", "npm install -g @gitlawb/openclaude"),
                collection_item("启动 OpenClaude", "openclaude"),
                collection_item("OpenClaude /provider", "/provider"),
            ],
        },
    ]
}

fn default_terminal_rename_presets() -> Vec<String> {
    vec!["完结".to_string(), "复用对话".to_string()]
}

fn default_terminal_tool_entries() -> Vec<TerminalToolEntry> {
    vec![
        TerminalToolEntry {
            id: "fork_session".to_string(),
            root_key: TERMINAL_TOOL_ROOT_TOOLS.to_string(),
            parent_id: None,
            kind: TERMINAL_TOOL_ENTRY_KIND_ACTION.to_string(),
            label: "fork".to_string(),
            sort_order: 30,
            actions: vec![TerminalToolAction {
                kind: TERMINAL_TOOL_ACTION_FORK_SESSION.to_string(),
                value: String::new(),
                seconds: 0.0,
                preset_selector: String::new(),
                preset_match: String::new(),
                cwd: String::new(),
                project_path: String::new(),
                terminal_name: String::new(),
                session_action: String::new(),
                command_key: String::new(),
                target_entry_id: String::new(),
            }],
        },
        TerminalToolEntry {
            id: "proxy_settings_workflow".to_string(),
            root_key: TERMINAL_TOOL_ROOT_TOOLS.to_string(),
            parent_id: None,
            kind: TERMINAL_TOOL_ENTRY_KIND_ACTION.to_string(),
            label: "代理设置".to_string(),
            sort_order: 20,
            actions: vec![TerminalToolAction {
                kind: TERMINAL_TOOL_ACTION_CODEX_LAUNCH.to_string(),
                value: "$mihomo-proxy-ops".to_string(),
                seconds: 0.0,
                preset_selector: "miniMax".to_string(),
                preset_match: PRESET_MATCH_UNIQUE_CONTAINS.to_string(),
                cwd: "/home/system".to_string(),
                project_path: "/home/system".to_string(),
                terminal_name: "代理设置".to_string(),
                session_action: SESSION_ACTION_NEW.to_string(),
                command_key: String::new(),
                target_entry_id: String::new(),
            }],
        },
    ]
}

fn validate_terminal_tool_id(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_TERMINAL_TOOL_ID_LEN
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn normalize_terminal_tool_action(
    action: &TerminalToolAction,
    entry_label: &str,
) -> SettingsResult<TerminalToolAction> {
    let kind = action.kind.trim().to_string();
    let mut value = action.value.trim().to_string();
    let mut seconds = 0.0;
    match kind.as_str() {
        TERMINAL_TOOL_ACTION_CREATE_TERMINAL
        | TERMINAL_TOOL_ACTION_FORK_SESSION
        | TERMINAL_TOOL_ACTION_SWITCH_API_PRESET_REVERT => value.clear(),
        TERMINAL_TOOL_ACTION_RENAME_TERMINAL | TERMINAL_TOOL_ACTION_SWITCH_API_PRESET => {
            if value.is_empty() || value.chars().count() > MAX_TERMINAL_TOOL_LABEL_LEN * 2 {
                return Err(SettingsError::bad_request(format!(
                    "利器条目“{entry_label}”的动作参数为空或过长"
                )));
            }
        }
        TERMINAL_TOOL_ACTION_WAIT => {
            if !action.seconds.is_finite()
                || action.seconds < 0.1
                || action.seconds > MAX_TERMINAL_TOOL_WAIT_SECONDS
            {
                return Err(SettingsError::bad_request(format!(
                    "利器条目“{entry_label}”的等待时间必须在 0.1 到 {MAX_TERMINAL_TOOL_WAIT_SECONDS} 秒之间"
                )));
            }
            value.clear();
            seconds = (action.seconds * 10.0).round() / 10.0;
        }
        TERMINAL_TOOL_ACTION_SEND_COMMAND
        | TERMINAL_TOOL_ACTION_CODEX_EXEC
        | TERMINAL_TOOL_ACTION_CODEX_TERMINAL => {
            if value.is_empty() || value.chars().count() > MAX_TERMINAL_TOOL_ACTION_VALUE_LEN {
                return Err(SettingsError::bad_request(format!(
                    "利器条目“{entry_label}”的命令或任务为空或过长"
                )));
            }
        }
        TERMINAL_TOOL_ACTION_FUNCTION_COMMAND => {
            let command_key = action.command_key.trim().to_string();
            if command_key.is_empty()
                || command_key
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control())
                || command_key.chars().count() > MAX_TERMINAL_TOOL_ID_LEN
            {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的功能命令键为空或无效"
                )));
            }
            value = command_key.clone();
            return Ok(TerminalToolAction {
                kind,
                value,
                seconds: 0.0,
                command_key,
                ..Default::default()
            });
        }
        TERMINAL_TOOL_ACTION_RUN_WORKFLOW => {
            let target_entry_id = action.target_entry_id.trim().to_string();
            if !validate_terminal_tool_id(&target_entry_id) {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的嵌套工作流目标 ID 无效"
                )));
            }
            value = target_entry_id.clone();
            return Ok(TerminalToolAction {
                kind,
                value,
                seconds: 0.0,
                target_entry_id,
                ..Default::default()
            });
        }
        TERMINAL_TOOL_ACTION_CODEX_LAUNCH => {
            if value.is_empty() || value.chars().count() > MAX_TERMINAL_TOOL_ACTION_VALUE_LEN {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的初始任务为空或过长"
                )));
            }
            let preset_selector = action.preset_selector.trim().to_string();
            let preset_match = action.preset_match.trim().to_string();
            let cwd = action.cwd.trim().to_string();
            let project_path = action.project_path.trim().to_string();
            let terminal_name = action.terminal_name.trim().to_string();
            let session_action = action.session_action.trim().to_string();

            if preset_selector.is_empty()
                || preset_selector.chars().count() > MAX_TERMINAL_TOOL_LABEL_LEN * 2
            {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的预设选择器为空或过长"
                )));
            }
            if preset_match != PRESET_MATCH_ID
                && preset_match != PRESET_MATCH_EXACT_NAME
                && preset_match != PRESET_MATCH_UNIQUE_CONTAINS
            {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的预设匹配方式无效：{preset_match}"
                )));
            }
            if !cwd.starts_with('/') || cwd.chars().any(char::is_control) {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的工作目录必须为绝对路径"
                )));
            }
            if !project_path.starts_with('/') || project_path.chars().any(char::is_control) {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的项目路径必须为绝对路径"
                )));
            }
            if terminal_name.is_empty() || terminal_name.chars().any(char::is_control) {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的终端名称为空或包含控制字符"
                )));
            }
            if session_action != SESSION_ACTION_NEW {
                return Err(SettingsError::bad_request(format!(
                    "工作流条目“{entry_label}”的会话动作无效：{session_action}"
                )));
            }

            return Ok(TerminalToolAction {
                kind,
                value,
                seconds: 0.0,
                preset_selector,
                preset_match,
                cwd,
                project_path,
                terminal_name,
                session_action,
                command_key: String::new(),
                target_entry_id: String::new(),
            });
        }
        _ => {
            return Err(SettingsError::bad_request(format!(
                "利器条目“{entry_label}”包含不支持的动作：{kind}"
            )));
        }
    }
    Ok(TerminalToolAction {
        kind,
        value,
        seconds,
        preset_selector: String::new(),
        preset_match: String::new(),
        cwd: String::new(),
        project_path: String::new(),
        terminal_name: String::new(),
        session_action: String::new(),
        command_key: String::new(),
        target_entry_id: String::new(),
    })
}

fn validate_terminal_tool_entries(
    entries: &[TerminalToolEntry],
) -> SettingsResult<Vec<TerminalToolEntry>> {
    if entries.len() > MAX_TERMINAL_TOOL_ENTRIES {
        return Err(SettingsError::bad_request(format!(
            "利器条目不能超过 {MAX_TERMINAL_TOOL_ENTRIES} 个"
        )));
    }

    let mut seen_ids = HashSet::new();
    let mut normalized = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = entry.id.trim().to_string();
        if !validate_terminal_tool_id(&id) || !seen_ids.insert(id.clone()) {
            return Err(SettingsError::bad_request(format!(
                "利器条目 ID 无效或重复：{}",
                entry.id
            )));
        }
        let root_key = entry.root_key.trim().to_string();
        if root_key != TERMINAL_TOOL_ROOT_TOOLS {
            return Err(SettingsError::bad_request(format!(
                "利器条目“{}”使用了不兼容的根键：{root_key}",
                entry.label
            )));
        }
        let label = entry.label.trim().to_string();
        if label.is_empty()
            || label.chars().count() > MAX_TERMINAL_TOOL_LABEL_LEN
            || label.chars().any(char::is_control)
        {
            return Err(SettingsError::bad_request("利器条目名称为空、过长或包含控制字符"));
        }
        if entry.sort_order > 10_000 {
            return Err(SettingsError::bad_request(format!(
                "利器条目“{label}”的排序值不能超过 10000"
            )));
        }
        let parent_id = entry
            .parent_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if parent_id.as_deref() == Some(id.as_str()) {
            return Err(SettingsError::bad_request(format!(
                "利器条目“{label}”不能以自身为上级目录"
            )));
        }
        let kind = entry.kind.trim().to_string();
        let actions = match kind.as_str() {
            TERMINAL_TOOL_ENTRY_KIND_FOLDER => {
                if !entry.actions.is_empty() {
                    return Err(SettingsError::bad_request(format!(
                        "利器目录“{label}”不能包含执行动作"
                    )));
                }
                Vec::new()
            }
            TERMINAL_TOOL_ENTRY_KIND_ACTION => {
                if entry.actions.is_empty() || entry.actions.len() > MAX_TERMINAL_TOOL_ACTIONS {
                    return Err(SettingsError::bad_request(format!(
                        "利器功能“{label}”必须包含 1 到 {MAX_TERMINAL_TOOL_ACTIONS} 个动作"
                    )));
                }
                entry
                    .actions
                    .iter()
                    .map(|action| normalize_terminal_tool_action(action, &label))
                    .collect::<SettingsResult<Vec<_>>>()?
            }
            _ => {
                return Err(SettingsError::bad_request(format!(
                    "利器条目“{label}”的类型无效：{kind}"
                )));
            }
        };
        normalized.push(TerminalToolEntry {
            id,
            root_key,
            parent_id,
            kind,
            label,
            sort_order: entry.sort_order,
            actions,
        });
    }

    for entry in &normalized {
        let Some(parent_id) = entry.parent_id.as_deref() else {
            continue;
        };
        let Some(parent) = normalized
            .iter()
            .find(|candidate| candidate.id == parent_id)
        else {
            return Err(SettingsError::bad_request(format!(
                "利器条目“{}”的上级目录不存在",
                entry.label
            )));
        };
        if parent.kind != TERMINAL_TOOL_ENTRY_KIND_FOLDER || parent.root_key != entry.root_key {
            return Err(SettingsError::bad_request(format!(
                "利器条目“{}”的上级必须是同一根键下的目录",
                entry.label
            )));
        }
    }

    for entry in &normalized {
        let mut visited = HashSet::new();
        let mut cursor = Some(entry.id.as_str());
        while let Some(id) = cursor {
            if !visited.insert(id) {
                return Err(SettingsError::bad_request(format!(
                    "利器目录层级存在循环：{}",
                    entry.label
                )));
            }
            cursor = normalized
                .iter()
                .find(|candidate| candidate.id == id)
                .and_then(|candidate| candidate.parent_id.as_deref());
        }
    }

    Ok(normalized)
}

fn sanitize_terminal_tool_entries(entries: &[TerminalToolEntry]) -> Vec<TerminalToolEntry> {
    validate_terminal_tool_entries(entries).unwrap_or_default()
}

/// 合并已保存的利器条目与内置默认条目（按 id 去重，已保存条目优先）。
/// 与 `merge_builtin_terminal_error_keywords` 同构：保证像 fork 这样的内置
/// 默认条目即使被旧的已保存设置覆盖也会回到界面，同时尊重用户对同名条目的自定义。
fn merge_builtin_terminal_tool_entries(entries: &[TerminalToolEntry]) -> Vec<TerminalToolEntry> {
    let mut merged: Vec<TerminalToolEntry> = entries.to_vec();
    let mut seen: HashSet<String> = merged.iter().map(|entry| entry.id.to_lowercase()).collect();
    for builtin in default_terminal_tool_entries() {
        if seen.insert(builtin.id.to_lowercase()) {
            merged.push(builtin);
        }
    }
    // 重新校验：用户条目之间可能的改动不应破坏树结构，否则回退到仅已保存集合。
    validate_terminal_tool_entries(&merged)
        .unwrap_or_else(|_| sanitize_terminal_tool_entries(entries))
}

fn sanitize_terminal_user_name(raw: &str) -> String {
    runtime_paths::normalize_user_name(raw).unwrap_or_else(|_| default_terminal_user())
}

fn validate_terminal_user(raw: &str) -> Result<runtime_paths::UserProfile> {
    let user_name = if raw.trim().is_empty() {
        default_terminal_user()
    } else {
        runtime_paths::normalize_user_name(raw)?
    };
    runtime_paths::resolve_user_profile(&user_name)
}

fn resolve_terminal_user(raw: &str) -> SettingsResult<runtime_paths::UserProfile> {
    validate_terminal_user(raw)
        .map_err(|error| SettingsError::bad_request(format!("用户身份无效: {error}")))
}

fn available_user_options(selected_profile: &runtime_paths::UserProfile) -> Vec<SystemUserOption> {
    let mut profiles = runtime_paths::list_login_user_profiles();
    if !profiles
        .iter()
        .any(|profile| profile.name == selected_profile.name)
    {
        profiles.push(selected_profile.clone());
    }
    profiles.into_iter().map(system_user_option).collect()
}

fn system_user_option(profile: runtime_paths::UserProfile) -> SystemUserOption {
    SystemUserOption {
        name: profile.name,
        uid: profile.uid,
        gid: profile.gid,
        home: profile.home.display().to_string(),
        shell: profile.shell.display().to_string(),
    }
}

fn sanitize_terminal_quick_commands(
    commands: &[TerminalQuickCommand],
) -> Vec<TerminalQuickCommand> {
    let mut seen_keys = HashSet::new();
    let mut sanitized = Vec::new();

    for command in commands {
        if sanitized.len() >= MAX_TERMINAL_QUICK_COMMANDS {
            break;
        }

        let Some(key) = sanitize_terminal_quick_command_key(&command.key) else {
            continue;
        };
        if !seen_keys.insert(key.clone()) {
            continue;
        }

        let Some(command_line) = sanitize_terminal_quick_command_line(&command.command)
            .or_else(|| legacy_terminal_quick_command_line(&command.program, &command.args))
        else {
            continue;
        };
        let label = sanitize_terminal_quick_command_label(&command.label)
            .unwrap_or_else(|| truncate_chars(&command_line, MAX_TERMINAL_QUICK_COMMAND_LABEL_LEN));

        sanitized.push(TerminalQuickCommand {
            key,
            label,
            command: command_line,
            program: String::new(),
            args: String::new(),
        });
    }

    sanitized
}

fn sanitize_terminal_quick_start_default_key(
    raw: &str,
    commands: &[TerminalQuickCommand],
) -> String {
    let Some(key) = sanitize_terminal_quick_command_key(raw) else {
        return String::new();
    };

    if commands.iter().any(|command| command.key == key) {
        key
    } else {
        String::new()
    }
}

fn sanitize_terminal_function_commands(
    commands: &[TerminalFunctionCommand],
) -> Vec<TerminalFunctionCommand> {
    let mut seen_keys = HashSet::new();
    let mut sanitized = Vec::new();

    for command in commands {
        if sanitized.len() >= MAX_TERMINAL_FUNCTION_COMMANDS {
            break;
        }

        let Some(key) = sanitize_terminal_function_command_key(&command.key) else {
            continue;
        };
        if !seen_keys.insert(key.clone()) {
            continue;
        }

        let action = sanitize_terminal_function_command_action(&command.action);
        let command_line = sanitize_terminal_function_command_line(&command.command);
        let shortcut = sanitize_terminal_function_command_shortcut(&command.shortcut);
        if action.is_none() && command_line.is_none() {
            continue;
        }

        let label = sanitize_terminal_function_command_label(&command.label).unwrap_or_else(|| {
            command_line
                .clone()
                .or_else(|| action.clone())
                .map(|value| truncate_chars(value.trim(), MAX_TERMINAL_FUNCTION_COMMAND_LABEL_LEN))
                .unwrap_or_else(|| key.clone())
        });

        sanitized.push(TerminalFunctionCommand {
            key,
            label,
            action: action.unwrap_or_default(),
            command: command_line.unwrap_or_default(),
            shortcut: shortcut.unwrap_or_default(),
        });
    }

    sanitized
}

fn sanitize_terminal_command_collections(
    collections: &[TerminalCommandCollection],
) -> Vec<TerminalCommandCollection> {
    let mut seen_keys = HashSet::new();
    let mut sanitized = Vec::new();

    for collection in collections {
        if sanitized.len() >= MAX_TERMINAL_COMMAND_COLLECTIONS {
            break;
        }

        let Some(key) =
            sanitize_terminal_command_collection_key(&collection.key, &collection.label)
        else {
            continue;
        };
        if !seen_keys.insert(key.clone()) {
            continue;
        }

        let label = sanitize_terminal_command_collection_label(&collection.label)
            .unwrap_or_else(|| key.clone());

        let mut seen_item_labels = HashSet::new();
        let mut items = Vec::new();
        for item in &collection.commands {
            if items.len() >= MAX_TERMINAL_COMMAND_COLLECTION_ITEMS {
                break;
            }
            let action = sanitize_terminal_function_command_action(&item.action);
            let command_line = sanitize_terminal_command_collection_item_command(&item.command);
            if action.is_none() && command_line.is_none() {
                continue;
            }
            let item_label = sanitize_terminal_command_collection_item_label(&item.label)
                .unwrap_or_else(|| {
                    command_line
                        .clone()
                        .map(|value| {
                            truncate_chars(
                                value.trim(),
                                MAX_TERMINAL_COMMAND_COLLECTION_ITEM_LABEL_LEN,
                            )
                        })
                        .unwrap_or_default()
                });
            if item_label.is_empty() {
                continue;
            }
            if !seen_item_labels.insert(item_label.clone()) {
                continue;
            }
            items.push(CommandCollectionItem {
                label: item_label,
                action: action.unwrap_or_else(default_collection_item_action),
                command: command_line.unwrap_or_default(),
            });
        }

        sanitized.push(TerminalCommandCollection {
            key,
            label,
            commands: items,
        });
    }

    sanitized
}

fn sanitize_terminal_command_collection_key(raw: &str, label: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        // Synthesize a key from the label slug so a user-created collection without a key still works.
        let slug: String = label
            .trim()
            .chars()
            .map(|c| {
                if c.is_control() || c.is_whitespace() {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        if slug.is_empty() {
            return None;
        }
        return Some(truncate_chars(&slug, MAX_TERMINAL_COMMAND_COLLECTION_KEY_LEN));
    }
    if trimmed.chars().count() > MAX_TERMINAL_COMMAND_COLLECTION_KEY_LEN
        || trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn sanitize_terminal_command_collection_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }
    Some(truncate_chars(trimmed, MAX_TERMINAL_COMMAND_COLLECTION_LABEL_LEN))
}

fn sanitize_terminal_command_collection_item_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }
    Some(truncate_chars(trimmed, MAX_TERMINAL_COMMAND_COLLECTION_ITEM_LABEL_LEN))
}

fn sanitize_terminal_command_collection_item_command(raw: &str) -> Option<String> {
    let trimmed = raw.trim_start();
    if trimmed.trim().is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }
    Some(truncate_chars(trimmed, MAX_TERMINAL_COMMAND_COLLECTION_ITEM_COMMAND_LEN))
}

fn sanitize_terminal_rename_presets(presets: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    for preset in presets {
        if sanitized.len() >= MAX_TERMINAL_RENAME_PRESETS {
            break;
        }

        let trimmed = preset.trim();
        if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
            continue;
        }

        let value = truncate_chars(trimmed, MAX_TERMINAL_RENAME_PRESET_LEN);
        if seen.insert(value.clone()) {
            sanitized.push(value);
        }
    }

    sanitized
}

fn sanitize_terminal_function_command_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TERMINAL_FUNCTION_COMMAND_KEY_LEN
        || trimmed
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return None;
    }

    Some(trimmed.to_string())
}

fn sanitize_terminal_function_command_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }

    Some(truncate_chars(trimmed, MAX_TERMINAL_FUNCTION_COMMAND_LABEL_LEN))
}

fn sanitize_terminal_function_command_action(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TERMINAL_FUNCTION_COMMAND_ACTION_LEN
        || !trimmed
            .chars()
            .all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return None;
    }

    Some(trimmed.to_string())
}

fn sanitize_terminal_function_command_line(raw: &str) -> Option<String> {
    let trimmed = raw.trim_start();
    if trimmed.trim().is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }

    Some(truncate_chars(trimmed, MAX_TERMINAL_FUNCTION_COMMAND_COMMAND_LEN))
}

fn sanitize_terminal_function_command_shortcut(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TERMINAL_FUNCTION_COMMAND_SHORTCUT_LEN
        || trimmed.chars().any(char::is_control)
    {
        return None;
    }

    Some(truncate_chars(trimmed, MAX_TERMINAL_FUNCTION_COMMAND_SHORTCUT_LEN))
}

fn sanitize_terminal_quick_command_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TERMINAL_QUICK_COMMAND_KEY_LEN
        || trimmed
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return None;
    }

    Some(trimmed.to_string())
}

fn sanitize_terminal_quick_command_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }

    Some(truncate_chars(trimmed, MAX_TERMINAL_QUICK_COMMAND_LABEL_LEN))
}

fn sanitize_terminal_quick_command_program(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TERMINAL_QUICK_COMMAND_PROGRAM_LEN
        || trimmed
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return None;
    }

    Some(trimmed.to_string())
}

fn sanitize_terminal_quick_command_args(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let without_controls = trimmed
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    truncate_chars(&without_controls, MAX_TERMINAL_QUICK_COMMAND_ARGS_LEN)
}

fn sanitize_terminal_quick_command_line(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }

    Some(truncate_chars(trimmed, MAX_TERMINAL_QUICK_COMMAND_COMMAND_LEN))
}

fn legacy_terminal_quick_command_line(program: &str, args: &str) -> Option<String> {
    let program = sanitize_terminal_quick_command_program(program)?;
    let args = sanitize_terminal_quick_command_args(args);
    Some(
        [program, args]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn sanitize_terminal_default_env_vars(
    vars: &[TerminalDefaultEnvVar],
) -> Vec<TerminalDefaultEnvVar> {
    let mut seen_keys = HashSet::new();
    let mut sanitized = Vec::new();

    for entry in vars {
        if sanitized.len() >= MAX_TERMINAL_DEFAULT_ENV_VARS {
            break;
        }

        let Some(key) = sanitize_terminal_default_env_key(&entry.key) else {
            continue;
        };
        if !seen_keys.insert(key.clone()) {
            continue;
        }

        sanitized.push(TerminalDefaultEnvVar {
            key,
            value: sanitize_terminal_default_env_value(&entry.value),
        });
    }

    sanitized
}

fn sanitize_terminal_default_env_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    if trimmed.chars().count() > MAX_TERMINAL_DEFAULT_ENV_KEY_LEN
        || !(first == '_' || first.is_ascii_alphabetic())
        || !chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
        || RESERVED_TERMINAL_DEFAULT_ENV_KEYS.contains(&trimmed)
    {
        return None;
    }

    Some(trimmed.to_string())
}

fn sanitize_terminal_default_env_value(raw: &str) -> String {
    let without_controls = raw
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    truncate_chars(&without_controls, MAX_TERMINAL_DEFAULT_ENV_VALUE_LEN)
}

fn sanitize_compile_environment(vars: &[CompileEnvVar]) -> Vec<CompileEnvVar> {
    let mut seen_keys = HashSet::new();
    let mut sanitized = Vec::new();
    for entry in vars {
        if sanitized.len() >= MAX_COMPILE_ENV_VARS {
            break;
        }
        let Some(key) = sanitize_compile_env_key(&entry.key) else {
            continue;
        };
        if !seen_keys.insert(key.clone()) {
            continue;
        }
        sanitized.push(CompileEnvVar {
            key,
            value: sanitize_compile_env_value(&entry.value),
        });
    }
    sanitized
}

fn sanitize_compile_env_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    if trimmed.chars().count() > MAX_COMPILE_ENV_KEY_LEN
        || !(first == '_' || first.is_ascii_alphabetic())
        || !chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn sanitize_compile_env_value(raw: &str) -> String {
    let without_controls = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>();
    truncate_chars(&without_controls, MAX_COMPILE_ENV_VALUE_LEN)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn default_claude_model_options() -> Vec<String> {
    vec![
        "claude-sonnet-4-6".to_string(),
        "claude-opus-4-6".to_string(),
        "claude-opus-4-6-thinking".to_string(),
        "GLM-5.1".to_string(),
        "GLM-4.7".to_string(),
    ]
}

fn default_claude_default_config_entries() -> Vec<CodexDefaultConfigEntry> {
    vec![
        CodexDefaultConfigEntry {
            key: CLAUDE_DEFAULT_HAIKU_MODEL_KEY.to_string(),
            value: "claude-haiku-4-5-20251001".to_string(),
        },
        CodexDefaultConfigEntry {
            key: CLAUDE_DEFAULT_SONNET_MODEL_KEY.to_string(),
            value: "claude-sonnet-4-6".to_string(),
        },
        CodexDefaultConfigEntry {
            key: CLAUDE_DEFAULT_OPUS_MODEL_KEY.to_string(),
            value: "claude-opus-4-6".to_string(),
        },
    ]
}

fn default_codex_config_key() -> String {
    DEFAULT_CODEX_CONFIG_KEY.to_string()
}

fn default_codex_model() -> String {
    DEFAULT_CODEX_MODEL.to_string()
}

fn default_codex_config_value() -> String {
    default_codex_model()
}

fn default_codex_default_config_entries() -> Vec<CodexDefaultConfigEntry> {
    vec![
        CodexDefaultConfigEntry {
            key: default_codex_config_key(),
            value: default_codex_config_value(),
        },
        CodexDefaultConfigEntry {
            key: default_codex_secondary_config_key(),
            value: default_codex_secondary_config_value(),
        },
    ]
}

fn default_codex_api_auto_proxy_match_provider_ids() -> Vec<String> {
    vec![
        "zhipu".to_string(),
        "deepseek".to_string(),
        "minimax".to_string(),
    ]
}

fn sanitize_codex_api_auto_proxy_match_provider_ids(values: &[String]) -> Vec<String> {
    let allowed = ["zhipu", "deepseek", "minimax"];
    let mut result = Vec::new();
    for value in values {
        let normalized = value.trim().to_ascii_lowercase();
        if allowed.contains(&normalized.as_str()) && !result.contains(&normalized) {
            result.push(normalized);
        }
    }
    result
}

fn default_codex_secondary_config_key() -> String {
    DEFAULT_CODEX_SECONDARY_CONFIG_KEY.to_string()
}

fn default_codex_secondary_config_value() -> String {
    DEFAULT_CODEX_SECONDARY_CONFIG_VALUE.to_string()
}

fn default_show_full_path() -> bool {
    DEFAULT_SHOW_FULL_PATH
}

fn default_workspace_browser_icon_path() -> String {
    DEFAULT_WORKSPACE_BROWSER_ICON_PATH.to_string()
}

fn default_terminal_workspace_icon_path() -> String {
    DEFAULT_TERMINAL_WORKSPACE_ICON_PATH.to_string()
}

fn normalize_project_icon_relative_path(raw: &str, fallback: &str) -> String {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() || Path::new(&normalized).is_absolute() {
        return fallback.to_string();
    }

    let mut clean = PathBuf::new();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return fallback.to_string();
            }
        }
    }

    let value = clean.to_string_lossy().replace('\\', "/");
    if value.is_empty() || value.len() > 240 {
        fallback.to_string()
    } else {
        value
    }
}

fn default_theme_mode() -> ThemeMode {
    ThemeMode::System
}

fn default_font_size_tier_1() -> f32 {
    DEFAULT_FONT_SIZE_TIER_1
}

fn default_font_size_tier_2() -> f32 {
    DEFAULT_FONT_SIZE_TIER_2
}

fn default_font_size_tier_3() -> f32 {
    DEFAULT_FONT_SIZE_TIER_3
}

fn default_font_size_tier_4() -> f32 {
    DEFAULT_FONT_SIZE_TIER_4
}

fn sanitize_show_full_path(raw: Option<bool>) -> bool {
    raw.unwrap_or(default_show_full_path())
}

/// Migrate legacy path_display_prefix to show_full_path: if a non-empty prefix
/// was configured (user wanted abbreviated paths), show_full_path = false.
fn migrate_path_display_prefix_to_show_full_path(legacy: &Option<String>) -> Option<bool> {
    legacy.as_ref().map(|raw| {
        let trimmed = raw.trim();
        // Empty or default prefix means full path display.
        trimmed.is_empty()
            || trimmed == WORKSPACE_ROOT_LIMIT
            || runtime_paths::resolve_current_user_home()
                .map(|home| home.display().to_string() == trimmed)
                .unwrap_or(false)
    })
}

fn sanitize_font_size_tier(value: f32, fallback: f32) -> f32 {
    if !value.is_finite() {
        return fallback;
    }

    value.clamp(0.5, 1.0)
}

fn normalize_terminal_floating_button_offset_vh(value: f32) -> f32 {
    if !value.is_finite() {
        return default_terminal_floating_button_offset_vh();
    }

    ((value.clamp(12.0, 60.0) * 10.0).round()) / 10.0
}

fn normalize_terminal_fab_action_color(value: &str) -> String {
    let trimmed = value.trim();
    let valid = trimmed.len() == 7
        && trimmed.starts_with('#')
        && trimmed[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if valid {
        trimmed.to_ascii_lowercase()
    } else {
        default_terminal_fab_action_color()
    }
}

fn normalize_terminal_fab_action_opacity(value: f32) -> f32 {
    if !value.is_finite() {
        return default_terminal_fab_action_opacity();
    }

    ((value.clamp(0.1, 1.0) * 100.0).round()) / 100.0
}

fn normalize_terminal_soft_keyboard_scale(value: f32) -> f32 {
    if !value.is_finite() {
        return default_terminal_soft_keyboard_scale();
    }

    ((value.clamp(0.9, 1.3) * 100.0).round()) / 100.0
}

fn normalize_terminal_touch_selection_long_press_ms(value: u32) -> u32 {
    value.clamp(2000, 10000)
}

fn normalize_terminal_scrollback_lines(value: u32) -> u32 {
    value.clamp(100, 100_000)
}

fn normalize_terminal_error_match_line_limit(value: u32) -> u32 {
    value.clamp(1, 1000)
}

fn normalize_terminal_auto_continue_interval_seconds(value: u32) -> u32 {
    if value == 0 {
        return default_terminal_auto_continue_interval_seconds();
    }
    value.min(86400)
}

fn normalize_terminal_auto_continue_backoff_factor(value: f64) -> f64 {
    if value.is_nan() || value < 1.0 {
        return default_terminal_auto_continue_backoff_factor();
    }
    value.min(10.0)
}

fn normalize_terminal_auto_continue_backoff_max_minutes(value: u32) -> u32 {
    if value == 0 {
        return default_terminal_auto_continue_backoff_max_minutes();
    }
    value.min(1440)
}

fn sanitize_terminal_auto_continue_time_patterns(patterns: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    for pattern in patterns {
        if sanitized.len() >= MAX_TERMINAL_AUTO_CONTINUE_TIME_PATTERNS {
            break;
        }

        let normalized = pattern.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty()
            || !normalized.contains("{time}")
            || normalized.chars().any(char::is_control)
        {
            continue;
        }

        let value = truncate_chars(&normalized, MAX_TERMINAL_AUTO_CONTINUE_TIME_PATTERN_LEN);
        let key = value.to_lowercase();
        if seen.insert(key) {
            sanitized.push(value);
        }
    }

    if sanitized.is_empty() {
        return default_terminal_auto_continue_time_patterns();
    }
    sanitized
}

/// Normalize the active-window string into `HH:MM-HH:MM` form (24h).
/// Empty is allowed (feature disabled). Malformed values become empty.
pub fn normalize_terminal_auto_continue_active_window(value: &str) -> String {
    let raw = value.trim();
    if raw.is_empty() {
        return String::new();
    }
    let Some((start, end)) = raw.split_once('-') else {
        return String::new();
    };
    let (start, end) = (start.trim(), end.trim());
    if !is_valid_hhmm(start) || !is_valid_hhmm(end) {
        return String::new();
    }
    format!("{start}-{end}")
}

pub fn normalize_terminal_scheduled_input_avoid_window(value: &str) -> String {
    normalize_terminal_auto_continue_active_window(value)
}

fn is_valid_hhmm(value: &str) -> bool {
    let Some((h, m)) = value.split_once(':') else {
        return false;
    };
    h.len() == 2
        && m.len() == 2
        && h.chars().all(|c| c.is_ascii_digit())
        && m.chars().all(|c| c.is_ascii_digit())
        && h.parse::<u32>().is_ok_and(|v| v < 24)
        && m.parse::<u32>().is_ok_and(|v| v < 60)
}

fn sanitize_terminal_error_keywords(keywords: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    for keyword in keywords {
        if sanitized.len() >= MAX_TERMINAL_ERROR_KEYWORDS {
            break;
        }

        let normalized = keyword.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty()
            || normalized.chars().any(char::is_control)
            || normalized.eq_ignore_ascii_case(NONFATAL_MCP_STARTUP_SUMMARY)
        {
            continue;
        }

        let value = truncate_chars(&normalized, MAX_TERMINAL_ERROR_KEYWORD_LEN);
        let key = value.to_lowercase();
        if seen.insert(key) {
            sanitized.push(value);
        }
    }

    sanitized
}

fn merge_builtin_terminal_error_keywords(keywords: &[String]) -> Vec<String> {
    let mut merged = sanitize_terminal_error_keywords(keywords);
    let mut seen = merged
        .iter()
        .map(|keyword| keyword.to_lowercase())
        .collect::<HashSet<_>>();

    for keyword in default_terminal_error_keywords() {
        if merged.len() >= MAX_TERMINAL_ERROR_KEYWORDS {
            break;
        }

        let key = keyword.to_lowercase();
        if seen.insert(key) {
            merged.push(keyword);
        }
    }

    merged
}

fn round_font_size_tier(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn normalize_font_size_tiers(values: [f32; 4]) -> [f32; 4] {
    let defaults = [
        default_font_size_tier_1(),
        default_font_size_tier_2(),
        default_font_size_tier_3(),
        default_font_size_tier_4(),
    ];
    let mut normalized = [
        sanitize_font_size_tier(values[0], defaults[0]),
        sanitize_font_size_tier(values[1], defaults[1]),
        sanitize_font_size_tier(values[2], defaults[2]),
        sanitize_font_size_tier(values[3], defaults[3]),
    ];

    for index in 1..normalized.len() {
        if normalized[index] < normalized[index - 1] {
            normalized[index] = normalized[index - 1];
        }
    }

    normalized.map(round_font_size_tier)
}

fn sanitize_favorite_paths(paths: &[FavoritePath]) -> Result<Vec<FavoritePath>> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    for favorite in paths {
        let normalized_path = normalize_favorite_path(&favorite.path)?;
        if seen.insert(normalized_path.clone()) {
            sanitized.push(FavoritePath {
                path: normalized_path,
                kind: favorite.kind.clone(),
            });
        }
    }

    Ok(sanitized)
}

fn sanitize_workspace_history(items: &[WorkspaceHistoryItem]) -> Vec<WorkspaceHistoryItem> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    let mut sorted = items.to_vec();
    sorted.sort_by_key(|right| std::cmp::Reverse(right.last_opened_at));

    for item in sorted {
        let Ok(normalized_path) = normalize_absolute_path(&item.path) else {
            continue;
        };
        let path = normalized_path.display().to_string();
        if seen.insert(path.clone()) {
            sanitized.push(WorkspaceHistoryItem {
                path,
                last_opened_at: item.last_opened_at,
            });
        }
        if sanitized.len() >= MAX_WORKSPACE_HISTORY_ITEMS {
            break;
        }
    }

    sanitized
}

fn sanitize_preset_sync_remote_url_history(urls: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    for raw_url in urls {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("http://{trimmed}")
        };
        let Ok(mut url) = url::Url::parse(&candidate) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            continue;
        }
        url.set_query(None);
        url.set_fragment(None);
        url.set_path("");
        let normalized = url.to_string().trim_end_matches('/').to_string();
        if seen.insert(normalized.clone()) {
            sanitized.push(normalized);
        }
        if sanitized.len() >= MAX_PRESET_SYNC_REMOTE_URL_HISTORY {
            break;
        }
    }

    sanitized
}

fn sanitize_desktop_remote_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_DESKTOP_REMOTE_URL.to_string();
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let Ok(url) = url::Url::parse(&candidate) else {
        return DEFAULT_DESKTOP_REMOTE_URL.to_string();
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return DEFAULT_DESKTOP_REMOTE_URL.to_string();
    }
    candidate
}

fn default_desktop_remote_url() -> String {
    DEFAULT_DESKTOP_REMOTE_URL.to_string()
}

fn sanitize_desktop_remote_url_history(urls: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    for raw_url in urls {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = sanitize_desktop_remote_url(trimmed);
        if seen.insert(normalized.clone()) {
            sanitized.push(normalized);
        }
        if sanitized.len() >= MAX_DESKTOP_REMOTE_URL_HISTORY {
            break;
        }
    }

    sanitized
}

fn sanitize_claude_model_options(options: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut sanitized = Vec::new();

    for option in options {
        let trimmed = option.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            sanitized.push(trimmed.to_string());
        }
    }

    sanitized
}

fn sanitize_codex_config_key(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default_codex_config_key();
    }
    if !is_valid_codex_config_key(trimmed) {
        return default_codex_config_key();
    }
    trimmed.to_string()
}

fn sanitize_codex_config_value(raw: &str) -> String {
    raw.trim().to_string()
}

fn is_valid_codex_config_key(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    })
}

fn is_codex_provider_owned_key(raw: &str) -> bool {
    let normalized = raw.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "model_provider" | "provider" | "wire_api" | "model_providers"
    ) || normalized.starts_with("model_providers.")
}

fn sanitize_codex_default_config_entries(
    entries: &[CodexDefaultConfigEntry],
) -> Vec<CodexDefaultConfigEntry> {
    let mut sanitized = Vec::new();
    for entry in entries {
        let key = entry.key.trim();
        let value = entry.value.trim();
        if key.is_empty() && value.is_empty() {
            continue;
        }
        if !is_valid_codex_config_key(key) || is_codex_provider_owned_key(key) {
            continue;
        }
        sanitized.push(CodexDefaultConfigEntry {
            key: key.to_string(),
            value: value.to_string(),
        });
        if sanitized.len() >= MAX_CODEX_DEFAULT_CONFIG_ENTRIES {
            break;
        }
    }
    if sanitized.is_empty() {
        return default_codex_default_config_entries();
    }
    sanitized
}

fn sanitize_claude_default_config_entries(
    entries: &[CodexDefaultConfigEntry],
) -> Vec<CodexDefaultConfigEntry> {
    let mut sanitized: Vec<CodexDefaultConfigEntry> = Vec::new();
    for entry in entries {
        let key = entry.key.trim();
        let value = entry.value.trim();
        if key.is_empty() && value.is_empty() {
            continue;
        }
        let valid_key = key
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            && key
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !valid_key || value.is_empty() {
            continue;
        }
        let next = CodexDefaultConfigEntry {
            key: key.to_string(),
            value: value.to_string(),
        };
        if let Some(existing) = sanitized
            .iter_mut()
            .find(|item| item.key.eq_ignore_ascii_case(key))
        {
            *existing = next;
        } else {
            sanitized.push(next);
        }
        if sanitized.len() >= MAX_CODEX_DEFAULT_CONFIG_ENTRIES {
            break;
        }
    }
    sanitized
}

fn legacy_codex_default_config_entries(
    config_key: &str,
    config_value: &str,
    secondary_config_key: &str,
    secondary_config_value: &str,
) -> Vec<CodexDefaultConfigEntry> {
    sanitize_codex_default_config_entries(&[
        CodexDefaultConfigEntry {
            key: sanitize_codex_config_key(config_key),
            value: sanitize_codex_config_value(config_value),
        },
        CodexDefaultConfigEntry {
            key: sanitize_codex_secondary_config_key(secondary_config_key),
            value: sanitize_codex_secondary_config_value(secondary_config_value),
        },
    ])
}

fn legacy_codex_fields_from_default_entries(
    entries: &[CodexDefaultConfigEntry],
) -> (String, String, String, String) {
    let fallback = default_codex_default_config_entries();
    let first = entries.first().or_else(|| fallback.first());
    let second = entries.get(1).or_else(|| fallback.get(1));
    (
        first
            .map(|entry| entry.key.clone())
            .unwrap_or_else(default_codex_config_key),
        first
            .map(|entry| entry.value.clone())
            .unwrap_or_else(default_codex_config_value),
        second
            .map(|entry| entry.key.clone())
            .unwrap_or_else(default_codex_secondary_config_key),
        second
            .map(|entry| entry.value.clone())
            .unwrap_or_else(default_codex_secondary_config_value),
    )
}

fn sanitize_codex_secondary_config_key(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default_codex_secondary_config_key();
    }
    if !is_valid_codex_config_key(trimmed) {
        return default_codex_secondary_config_key();
    }
    trimmed.to_string()
}

fn sanitize_codex_secondary_config_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default_codex_secondary_config_value();
    }
    trimmed.to_string()
}

fn normalize_favorite_path(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("收藏路径不能为空。");
    }

    let mut is_absolute = false;
    let mut normalized = PathBuf::new();

    for component in Path::new(trimmed).components() {
        match component {
            Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str());
            }
            Component::RootDir => {
                is_absolute = true;
                if normalized.as_os_str().is_empty() {
                    normalized = PathBuf::from(std::path::MAIN_SEPARATOR.to_string());
                } else {
                    normalized.push(std::path::MAIN_SEPARATOR.to_string());
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    if !is_absolute {
        anyhow::bail!("收藏路径必须使用绝对路径。");
    }

    if !is_within_workspace_limit(&normalized) {
        let limit = workspace_root_limit();
        anyhow::bail!("收藏路径必须位于 `{}` 下。", limit.display());
    }

    Ok(normalized.display().to_string())
}

#[derive(Debug)]
struct ResolvedWorkspaceDir {
    canonical: PathBuf,
    display: PathBuf,
}

fn validate_workspace_dir(raw: &str) -> SettingsResult<ResolvedWorkspaceDir> {
    resolve_workspace_dir(raw)
        .map_err(|error| SettingsError::bad_request(format!("工作目录无效: {error}")))
}

fn resolve_built_in_default_workspace_dir() -> Result<ResolvedWorkspaceDir> {
    let candidates = default_workspace_dir_candidates();
    let mut errors = Vec::new();

    for candidate in candidates {
        let raw = candidate.to_string_lossy().into_owned();
        match resolve_workspace_dir(&raw) {
            Ok(resolved) => return Ok(resolved),
            Err(error) => errors.push(format!("`{}`: {error}", candidate.display())),
        }
    }

    if errors.is_empty() {
        anyhow::bail!("没有可用候选目录。");
    }

    anyhow::bail!("{}", errors.join("; "));
}

fn built_in_default_workspace_dir_display() -> String {
    resolve_built_in_default_workspace_dir()
        .map(|resolved| resolved.display.display().to_string())
        .unwrap_or_else(|_| PathBuf::from(WORKSPACE_ROOT_LIMIT).display().to_string())
}
fn default_workspace_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let current_user_home = runtime_paths::resolve_current_user_home();
    if cfg!(windows) {
        push_unique_workspace_candidate(&mut candidates, current_user_home.clone());
    } else {
        push_unique_workspace_candidate(
            &mut candidates,
            Some(PathBuf::from(DEFAULT_WORKSPACE_DIR)),
        );
        push_unique_workspace_candidate(&mut candidates, current_user_home.clone());
    }
    push_unique_workspace_candidate(
        &mut candidates,
        Some(platform_workspace_root_limit(current_user_home)),
    );
    candidates
}

fn push_unique_workspace_candidate(paths: &mut Vec<PathBuf>, candidate: Option<PathBuf>) {
    if let Some(path) = candidate.filter(|path| path.is_absolute())
        && !paths.iter().any(|existing| existing == &path)
    {
        paths.push(path);
    }
}

fn resolve_workspace_dir(raw: &str) -> Result<ResolvedWorkspaceDir> {
    let display = normalize_absolute_path(raw)?;
    let candidate = display.clone();
    if !candidate.is_absolute() {
        anyhow::bail!("工作目录必须是绝对路径。");
    }

    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("无法访问目录 `{}`", candidate.display()))?;
    let metadata = std::fs::metadata(&canonical)
        .with_context(|| format!("无法读取目录信息 `{}`", canonical.display()))?;
    if !metadata.is_dir() {
        anyhow::bail!("目标不是目录。");
    }

    Ok(ResolvedWorkspaceDir { canonical, display })
}

fn normalize_absolute_path(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("工作目录不能为空。");
    }

    let mut is_absolute = false;
    let mut normalized = PathBuf::new();

    for component in Path::new(trimmed).components() {
        match component {
            Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str());
            }
            Component::RootDir => {
                is_absolute = true;
                if normalized.as_os_str().is_empty() {
                    normalized = PathBuf::from(std::path::MAIN_SEPARATOR.to_string());
                } else {
                    normalized.push(std::path::MAIN_SEPARATOR.to_string());
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    if !is_absolute {
        anyhow::bail!("工作目录必须是绝对路径。");
    }

    Ok(normalized)
}

fn is_within_workspace_limit(path: &Path) -> bool {
    if cfg!(windows) {
        return path.is_absolute();
    }

    let limit = workspace_root_limit();
    path == limit || path.starts_with(&limit)
}

fn workspace_root_limit() -> PathBuf {
    platform_workspace_root_limit(runtime_paths::resolve_current_user_home())
}

fn platform_workspace_root_limit(current_user_home: Option<PathBuf>) -> PathBuf {
    // Allow overriding the workspace root limit via env var so servers without
    // /home (e.g. containers with only /root) can still pass path validation.
    if let Ok(env_limit) = std::env::var("WEBCLX_WORKSPACE_ROOT_LIMIT") {
        let trimmed = env_limit.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_absolute() {
                return path;
            }
            tracing::warn!("WEBCLX_WORKSPACE_ROOT_LIMIT={trimmed} is not absolute; ignoring");
        }
    }

    if cfg!(windows) {
        current_user_home.unwrap_or_else(|| PathBuf::from(r"C:\"))
    } else {
        PathBuf::from(WORKSPACE_ROOT_LIMIT)
    }
}

#[cfg(test)]
mod tests;
