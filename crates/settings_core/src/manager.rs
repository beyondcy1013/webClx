use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result};
use runtime_paths_core as runtime_paths;
use tracing::warn;

use crate::*;

impl SettingsManager {
    pub fn load(app_dir: &Path) -> Result<Self> {
        let config_path = if cfg!(windows) {
            app_dir.join("config").join(SETTINGS_FILE_NAME)
        } else {
            app_dir.join(SETTINGS_FILE_NAME)
        };
        let loaded = match load_saved_settings(&config_path) {
            Ok(Some(settings)) => settings,
            Ok(None) => {
                let workspace_dir = resolve_built_in_default_workspace_dir()
                    .context("默认工作目录不存在或不可访问")?;
                LoadedSettings {
                    workspace_dir: workspace_dir.canonical,
                    display_workspace_dir: workspace_dir.display,
                    terminal_user: default_terminal_user(),
                    terminal_quick_commands: default_terminal_quick_commands(),
                    terminal_quick_start_default_key: default_terminal_quick_start_default_key(),
                    terminal_default_env_vars: default_terminal_default_env_vars(),
                    terminal_slash_commands: default_terminal_slash_commands(),
                    terminal_function_commands: default_terminal_function_commands(),
                    terminal_command_collections: default_terminal_command_collections(),
                    terminal_tool_entries: default_terminal_tool_entries(),
                    terminal_rename_presets: default_terminal_rename_presets(),
                    show_dot_entries: default_show_dot_entries(),
                    show_all_workspace_sessions: default_show_all_workspace_sessions(),
                    desktop_terminal_soft_keyboard_enabled:
                        default_desktop_terminal_soft_keyboard_enabled(),
                    terminal_soft_keyboard_scale: default_terminal_soft_keyboard_scale(),
                    terminal_floating_button_offset_vh: default_terminal_floating_button_offset_vh(
                    ),
                    terminal_fab_action_color: default_terminal_fab_action_color(),
                    terminal_fab_action_opacity: default_terminal_fab_action_opacity(),
                    terminal_fab_auto_expand: default_terminal_fab_auto_expand(),
                    terminal_touch_selection_long_press_ms:
                        default_terminal_touch_selection_long_press_ms(),
                    terminal_scrollback_lines: default_terminal_scrollback_lines(),
                    terminal_error_match_line_limit: default_terminal_error_match_line_limit(),
                    terminal_auto_continue_on_error: default_terminal_auto_continue_on_error(),
                    terminal_auto_continue_interval_seconds:
                        default_terminal_auto_continue_interval_seconds(),
                    terminal_auto_continue_backoff_factor:
                        default_terminal_auto_continue_backoff_factor(),
                    terminal_auto_continue_backoff_max_minutes:
                        default_terminal_auto_continue_backoff_max_minutes(),
                    terminal_auto_continue_respect_manual_interrupt:
                        default_terminal_auto_continue_respect_manual_interrupt(),
                    terminal_auto_continue_time_patterns:
                        default_terminal_auto_continue_time_patterns(),
                    terminal_auto_continue_active_window:
                        default_terminal_auto_continue_active_window(),
                    terminal_scheduled_input_avoid_window:
                        default_terminal_scheduled_input_avoid_window(),
                    terminal_error_keywords: default_terminal_error_keywords(),
                    terminal_error_keyword_actions: default_terminal_error_keyword_actions(),
                    terminal_activity_agent_display: default_terminal_activity_agent_display(),
                    terminal_completion_bell_enabled: default_terminal_completion_bell_enabled(),
                    server_port_auto_increment: default_server_port_auto_increment(),
                    compile_command_timeout_secs: default_compile_command_timeout_secs(),
                    compile_max_concurrency: default_compile_max_concurrency(),
                    compile_environment: default_compile_environment(),
                    gateway_listen_non_loopback: default_gateway_listen_non_loopback(),
                    session_ttl_days: default_session_ttl_days(),
                    favorite_paths: Vec::new(),
                    workspace_history: Vec::new(),
                    preset_sync_remote_url_history: Vec::new(),
                    desktop_remote_url: default_desktop_remote_url(),
                    desktop_remote_url_history: Vec::new(),
                    claude_model_options: default_claude_model_options(),
                    claude_default_config_entries: default_claude_default_config_entries(),
                    codex_default_config_entries: default_codex_default_config_entries(),
                    codex_api_auto_proxy_match_provider_ids:
                        default_codex_api_auto_proxy_match_provider_ids(),
                    codex_config_key: default_codex_config_key(),
                    codex_config_value: default_codex_config_value(),
                    codex_secondary_config_key: default_codex_secondary_config_key(),
                    codex_secondary_config_value: default_codex_secondary_config_value(),
                    show_full_path: default_show_full_path(),
                    workspace_browser_icon_path: default_workspace_browser_icon_path(),
                    terminal_workspace_icon_path: default_terminal_workspace_icon_path(),
                    theme_mode: default_theme_mode(),
                    font_size_tier_1: default_font_size_tier_1(),
                    font_size_tier_2: default_font_size_tier_2(),
                    font_size_tier_3: default_font_size_tier_3(),
                    font_size_tier_4: default_font_size_tier_4(),
                }
            }
            Err(error) => {
                warn!("load settings failed, fallback to default: {error}");
                let workspace_dir = resolve_built_in_default_workspace_dir()
                    .context("默认工作目录不存在或不可访问")?;
                LoadedSettings {
                    workspace_dir: workspace_dir.canonical,
                    display_workspace_dir: workspace_dir.display,
                    terminal_user: default_terminal_user(),
                    terminal_quick_commands: default_terminal_quick_commands(),
                    terminal_quick_start_default_key: default_terminal_quick_start_default_key(),
                    terminal_default_env_vars: default_terminal_default_env_vars(),
                    terminal_slash_commands: default_terminal_slash_commands(),
                    terminal_function_commands: default_terminal_function_commands(),
                    terminal_command_collections: default_terminal_command_collections(),
                    terminal_tool_entries: default_terminal_tool_entries(),
                    terminal_rename_presets: default_terminal_rename_presets(),
                    show_dot_entries: default_show_dot_entries(),
                    show_all_workspace_sessions: default_show_all_workspace_sessions(),
                    desktop_terminal_soft_keyboard_enabled:
                        default_desktop_terminal_soft_keyboard_enabled(),
                    terminal_soft_keyboard_scale: default_terminal_soft_keyboard_scale(),
                    terminal_floating_button_offset_vh: default_terminal_floating_button_offset_vh(
                    ),
                    terminal_fab_action_color: default_terminal_fab_action_color(),
                    terminal_fab_action_opacity: default_terminal_fab_action_opacity(),
                    terminal_fab_auto_expand: default_terminal_fab_auto_expand(),
                    terminal_touch_selection_long_press_ms:
                        default_terminal_touch_selection_long_press_ms(),
                    terminal_scrollback_lines: default_terminal_scrollback_lines(),
                    terminal_error_match_line_limit: default_terminal_error_match_line_limit(),
                    terminal_auto_continue_on_error: default_terminal_auto_continue_on_error(),
                    terminal_auto_continue_interval_seconds:
                        default_terminal_auto_continue_interval_seconds(),
                    terminal_auto_continue_backoff_factor:
                        default_terminal_auto_continue_backoff_factor(),
                    terminal_auto_continue_backoff_max_minutes:
                        default_terminal_auto_continue_backoff_max_minutes(),
                    terminal_auto_continue_respect_manual_interrupt:
                        default_terminal_auto_continue_respect_manual_interrupt(),
                    terminal_auto_continue_time_patterns:
                        default_terminal_auto_continue_time_patterns(),
                    terminal_auto_continue_active_window:
                        default_terminal_auto_continue_active_window(),
                    terminal_scheduled_input_avoid_window:
                        default_terminal_scheduled_input_avoid_window(),
                    terminal_error_keywords: default_terminal_error_keywords(),
                    terminal_error_keyword_actions: default_terminal_error_keyword_actions(),
                    terminal_activity_agent_display: default_terminal_activity_agent_display(),
                    terminal_completion_bell_enabled: default_terminal_completion_bell_enabled(),
                    server_port_auto_increment: default_server_port_auto_increment(),
                    compile_command_timeout_secs: default_compile_command_timeout_secs(),
                    compile_max_concurrency: default_compile_max_concurrency(),
                    compile_environment: default_compile_environment(),
                    gateway_listen_non_loopback: default_gateway_listen_non_loopback(),
                    session_ttl_days: default_session_ttl_days(),
                    favorite_paths: Vec::new(),
                    workspace_history: Vec::new(),
                    preset_sync_remote_url_history: Vec::new(),
                    desktop_remote_url: default_desktop_remote_url(),
                    desktop_remote_url_history: Vec::new(),
                    claude_model_options: default_claude_model_options(),
                    claude_default_config_entries: default_claude_default_config_entries(),
                    codex_default_config_entries: default_codex_default_config_entries(),
                    codex_api_auto_proxy_match_provider_ids:
                        default_codex_api_auto_proxy_match_provider_ids(),
                    codex_config_key: default_codex_config_key(),
                    codex_config_value: default_codex_config_value(),
                    codex_secondary_config_key: default_codex_secondary_config_key(),
                    codex_secondary_config_value: default_codex_secondary_config_value(),
                    show_full_path: default_show_full_path(),
                    workspace_browser_icon_path: default_workspace_browser_icon_path(),
                    terminal_workspace_icon_path: default_terminal_workspace_icon_path(),
                    theme_mode: default_theme_mode(),
                    font_size_tier_1: default_font_size_tier_1(),
                    font_size_tier_2: default_font_size_tier_2(),
                    font_size_tier_3: default_font_size_tier_3(),
                    font_size_tier_4: default_font_size_tier_4(),
                }
            }
        };

        persist_settings_file(
            &config_path,
            &loaded.display_workspace_dir,
            &loaded.terminal_user,
            &loaded.terminal_quick_commands,
            &loaded.terminal_quick_start_default_key,
            &loaded.terminal_default_env_vars,
            &loaded.terminal_slash_commands,
            &loaded.terminal_function_commands,
            &loaded.terminal_command_collections,
            &loaded.terminal_tool_entries,
            &loaded.terminal_rename_presets,
            loaded.show_dot_entries,
            loaded.show_all_workspace_sessions,
            loaded.desktop_terminal_soft_keyboard_enabled,
            loaded.terminal_soft_keyboard_scale,
            loaded.terminal_floating_button_offset_vh,
            &loaded.terminal_fab_action_color,
            loaded.terminal_fab_action_opacity,
            loaded.terminal_fab_auto_expand,
            loaded.terminal_touch_selection_long_press_ms,
            loaded.terminal_scrollback_lines,
            loaded.terminal_error_match_line_limit,
            loaded.terminal_auto_continue_on_error,
            loaded.terminal_auto_continue_interval_seconds,
            loaded.terminal_auto_continue_backoff_factor,
            loaded.terminal_auto_continue_backoff_max_minutes,
            loaded.terminal_auto_continue_respect_manual_interrupt,
            &loaded.terminal_auto_continue_time_patterns,
            &loaded.terminal_auto_continue_active_window,
            &loaded.terminal_scheduled_input_avoid_window,
            &loaded.terminal_error_keywords,
            &loaded.terminal_error_keyword_actions,
            loaded.terminal_activity_agent_display,
            loaded.terminal_completion_bell_enabled,
            loaded.server_port_auto_increment,
            loaded.compile_command_timeout_secs,
            loaded.compile_max_concurrency,
            &loaded.compile_environment,
            loaded.gateway_listen_non_loopback,
            loaded.session_ttl_days,
            &loaded.favorite_paths,
            &loaded.workspace_history,
            &loaded.preset_sync_remote_url_history,
            &loaded.desktop_remote_url,
            &loaded.desktop_remote_url_history,
            &loaded.claude_model_options,
            &loaded.claude_default_config_entries,
            &loaded.codex_default_config_entries,
            &loaded.codex_api_auto_proxy_match_provider_ids,
            &loaded.codex_config_key,
            &loaded.codex_config_value,
            &loaded.codex_secondary_config_key,
            &loaded.codex_secondary_config_value,
            loaded.show_full_path,
            &loaded.workspace_browser_icon_path,
            &loaded.terminal_workspace_icon_path,
            loaded.theme_mode,
            [
                loaded.font_size_tier_1,
                loaded.font_size_tier_2,
                loaded.font_size_tier_3,
                loaded.font_size_tier_4,
            ],
        )?;

        Ok(Self {
            current_root: Arc::new(RwLock::new(loaded.workspace_dir)),
            display_root: Arc::new(RwLock::new(loaded.display_workspace_dir)),
            terminal_user: Arc::new(RwLock::new(loaded.terminal_user)),
            terminal_quick_commands: Arc::new(RwLock::new(loaded.terminal_quick_commands)),
            terminal_quick_start_default_key: Arc::new(RwLock::new(
                loaded.terminal_quick_start_default_key,
            )),
            terminal_default_env_vars: Arc::new(RwLock::new(loaded.terminal_default_env_vars)),
            terminal_slash_commands: Arc::new(RwLock::new(loaded.terminal_slash_commands)),
            terminal_function_commands: Arc::new(RwLock::new(loaded.terminal_function_commands)),
            terminal_command_collections: Arc::new(RwLock::new(
                loaded.terminal_command_collections,
            )),
            terminal_tool_entries: Arc::new(RwLock::new(loaded.terminal_tool_entries)),
            terminal_rename_presets: Arc::new(RwLock::new(loaded.terminal_rename_presets)),
            show_dot_entries: Arc::new(RwLock::new(loaded.show_dot_entries)),
            show_all_workspace_sessions: Arc::new(RwLock::new(loaded.show_all_workspace_sessions)),
            desktop_terminal_soft_keyboard_enabled: Arc::new(RwLock::new(
                loaded.desktop_terminal_soft_keyboard_enabled,
            )),
            terminal_soft_keyboard_scale: Arc::new(RwLock::new(
                loaded.terminal_soft_keyboard_scale,
            )),
            terminal_floating_button_offset_vh: Arc::new(RwLock::new(
                loaded.terminal_floating_button_offset_vh,
            )),
            terminal_fab_action_color: Arc::new(RwLock::new(loaded.terminal_fab_action_color)),
            terminal_fab_action_opacity: Arc::new(RwLock::new(loaded.terminal_fab_action_opacity)),
            terminal_fab_auto_expand: Arc::new(RwLock::new(loaded.terminal_fab_auto_expand)),
            terminal_touch_selection_long_press_ms: Arc::new(RwLock::new(
                loaded.terminal_touch_selection_long_press_ms,
            )),
            terminal_scrollback_lines: Arc::new(RwLock::new(loaded.terminal_scrollback_lines)),
            terminal_error_match_line_limit: Arc::new(RwLock::new(
                loaded.terminal_error_match_line_limit,
            )),
            terminal_auto_continue_on_error: Arc::new(RwLock::new(
                loaded.terminal_auto_continue_on_error,
            )),
            terminal_auto_continue_interval_seconds: Arc::new(RwLock::new(
                loaded.terminal_auto_continue_interval_seconds,
            )),
            terminal_auto_continue_backoff_factor: Arc::new(RwLock::new(
                loaded.terminal_auto_continue_backoff_factor,
            )),
            terminal_auto_continue_backoff_max_minutes: Arc::new(RwLock::new(
                loaded.terminal_auto_continue_backoff_max_minutes,
            )),
            terminal_auto_continue_respect_manual_interrupt: Arc::new(RwLock::new(
                loaded.terminal_auto_continue_respect_manual_interrupt,
            )),
            terminal_auto_continue_time_patterns: Arc::new(RwLock::new(
                loaded.terminal_auto_continue_time_patterns,
            )),
            terminal_auto_continue_active_window: Arc::new(RwLock::new(
                loaded.terminal_auto_continue_active_window,
            )),
            terminal_scheduled_input_avoid_window: Arc::new(RwLock::new(
                loaded.terminal_scheduled_input_avoid_window,
            )),
            terminal_error_keywords: Arc::new(RwLock::new(loaded.terminal_error_keywords)),
            terminal_error_keyword_actions: Arc::new(RwLock::new(
                loaded.terminal_error_keyword_actions,
            )),
            terminal_activity_agent_display: Arc::new(RwLock::new(
                loaded.terminal_activity_agent_display,
            )),
            terminal_completion_bell_enabled: Arc::new(RwLock::new(
                loaded.terminal_completion_bell_enabled,
            )),
            server_port_auto_increment: Arc::new(RwLock::new(loaded.server_port_auto_increment)),
            compile_command_timeout_secs: Arc::new(RwLock::new(
                loaded.compile_command_timeout_secs,
            )),
            compile_max_concurrency: Arc::new(RwLock::new(loaded.compile_max_concurrency)),
            compile_environment: Arc::new(RwLock::new(loaded.compile_environment)),
            gateway_listen_non_loopback: Arc::new(RwLock::new(loaded.gateway_listen_non_loopback)),
            session_ttl_days: Arc::new(RwLock::new(loaded.session_ttl_days)),
            favorite_paths: Arc::new(RwLock::new(loaded.favorite_paths)),
            workspace_history: Arc::new(RwLock::new(loaded.workspace_history)),
            preset_sync_remote_url_history: Arc::new(RwLock::new(
                loaded.preset_sync_remote_url_history,
            )),
            desktop_remote_url: Arc::new(RwLock::new(loaded.desktop_remote_url)),
            desktop_remote_url_history: Arc::new(RwLock::new(loaded.desktop_remote_url_history)),
            claude_model_options: Arc::new(RwLock::new(loaded.claude_model_options)),
            claude_default_config_entries: Arc::new(RwLock::new(
                loaded.claude_default_config_entries,
            )),
            codex_default_config_entries: Arc::new(RwLock::new(
                loaded.codex_default_config_entries,
            )),
            codex_api_auto_proxy_match_provider_ids: Arc::new(RwLock::new(
                loaded.codex_api_auto_proxy_match_provider_ids,
            )),
            codex_config_key: Arc::new(RwLock::new(loaded.codex_config_key)),
            codex_config_value: Arc::new(RwLock::new(loaded.codex_config_value)),
            codex_secondary_config_key: Arc::new(RwLock::new(loaded.codex_secondary_config_key)),
            codex_secondary_config_value: Arc::new(RwLock::new(
                loaded.codex_secondary_config_value,
            )),
            show_full_path: Arc::new(RwLock::new(loaded.show_full_path)),
            workspace_browser_icon_path: Arc::new(RwLock::new(loaded.workspace_browser_icon_path)),
            terminal_workspace_icon_path: Arc::new(RwLock::new(
                loaded.terminal_workspace_icon_path,
            )),
            theme_mode: Arc::new(RwLock::new(loaded.theme_mode)),
            font_size_tier_1: Arc::new(RwLock::new(loaded.font_size_tier_1)),
            font_size_tier_2: Arc::new(RwLock::new(loaded.font_size_tier_2)),
            font_size_tier_3: Arc::new(RwLock::new(loaded.font_size_tier_3)),
            font_size_tier_4: Arc::new(RwLock::new(loaded.font_size_tier_4)),
            config_path: Arc::new(config_path),
        })
    }

    pub fn current_root(&self) -> PathBuf {
        self.current_root
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn display_root(&self) -> PathBuf {
        self.display_root
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_user(&self) -> String {
        self.terminal_user
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_user_profile(&self) -> Result<runtime_paths::UserProfile> {
        runtime_paths::resolve_user_profile(&self.terminal_user())
    }

    pub fn terminal_quick_commands(&self) -> Vec<TerminalQuickCommand> {
        self.terminal_quick_commands
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_quick_start_default_key(&self) -> String {
        self.terminal_quick_start_default_key
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_default_env_vars(&self) -> Vec<TerminalDefaultEnvVar> {
        self.terminal_default_env_vars
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_function_commands(&self) -> Vec<TerminalFunctionCommand> {
        self.terminal_function_commands
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_command_collections(&self) -> Vec<TerminalCommandCollection> {
        self.terminal_command_collections
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_tool_entries(&self) -> Vec<TerminalToolEntry> {
        self.terminal_tool_entries
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_slash_commands(&self) -> Vec<TerminalFunctionCommand> {
        self.terminal_slash_commands
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_rename_presets(&self) -> Vec<String> {
        self.terminal_rename_presets
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_default_env_entries(&self) -> Vec<(String, String)> {
        self.terminal_default_env_vars()
            .into_iter()
            .map(|entry| (entry.key, entry.value))
            .collect()
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_path.as_ref().clone()
    }

    pub fn show_dot_entries(&self) -> bool {
        *self
            .show_dot_entries
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn favorite_paths(&self) -> Vec<FavoritePath> {
        self.favorite_paths
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn workspace_history(&self) -> Vec<WorkspaceHistoryItem> {
        self.workspace_history
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn preset_sync_remote_url_history(&self) -> Vec<String> {
        self.preset_sync_remote_url_history
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn desktop_remote_url(&self) -> String {
        self.desktop_remote_url
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn desktop_remote_url_history(&self) -> Vec<String> {
        self.desktop_remote_url_history
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn show_all_workspace_sessions(&self) -> bool {
        *self
            .show_all_workspace_sessions
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn desktop_terminal_soft_keyboard_enabled(&self) -> bool {
        *self
            .desktop_terminal_soft_keyboard_enabled
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_floating_button_offset_vh(&self) -> f32 {
        *self
            .terminal_floating_button_offset_vh
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_fab_action_color(&self) -> String {
        self.terminal_fab_action_color
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_fab_action_opacity(&self) -> f32 {
        *self
            .terminal_fab_action_opacity
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_fab_auto_expand(&self) -> bool {
        *self
            .terminal_fab_auto_expand
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_soft_keyboard_scale(&self) -> f32 {
        *self
            .terminal_soft_keyboard_scale
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_touch_selection_long_press_ms(&self) -> u32 {
        *self
            .terminal_touch_selection_long_press_ms
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_scrollback_lines(&self) -> u32 {
        *self
            .terminal_scrollback_lines
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_error_match_line_limit(&self) -> u32 {
        *self
            .terminal_error_match_line_limit
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_auto_continue_interval_seconds(&self) -> u32 {
        *self
            .terminal_auto_continue_interval_seconds
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_auto_continue_backoff_factor(&self) -> f64 {
        *self
            .terminal_auto_continue_backoff_factor
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_auto_continue_backoff_max_minutes(&self) -> u32 {
        *self
            .terminal_auto_continue_backoff_max_minutes
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_auto_continue_on_error(&self) -> bool {
        *self
            .terminal_auto_continue_on_error
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_auto_continue_respect_manual_interrupt(&self) -> bool {
        *self
            .terminal_auto_continue_respect_manual_interrupt
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_auto_continue_time_patterns(&self) -> Vec<String> {
        self.terminal_auto_continue_time_patterns
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_auto_continue_active_window(&self) -> String {
        self.terminal_auto_continue_active_window
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_scheduled_input_avoid_window(&self) -> String {
        self.terminal_scheduled_input_avoid_window
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_error_keywords(&self) -> Vec<String> {
        self.terminal_error_keywords
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_error_keyword_actions(&self) -> Vec<TerminalErrorKeywordAction> {
        self.terminal_error_keyword_actions
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_activity_agent_display(&self) -> TerminalActivityAgentDisplay {
        *self
            .terminal_activity_agent_display
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn terminal_completion_bell_enabled(&self) -> bool {
        *self
            .terminal_completion_bell_enabled
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn server_port_auto_increment(&self) -> bool {
        *self
            .server_port_auto_increment
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn compile_command_timeout_secs(&self) -> u64 {
        *self
            .compile_command_timeout_secs
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn compile_max_concurrency(&self) -> u32 {
        *self
            .compile_max_concurrency
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn compile_environment(&self) -> Vec<CompileEnvVar> {
        self.compile_environment
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn gateway_listen_non_loopback(&self) -> bool {
        *self
            .gateway_listen_non_loopback
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn session_ttl_days(&self) -> u32 {
        *self
            .session_ttl_days
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn claude_model_options(&self) -> Vec<String> {
        self.claude_model_options
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn codex_default_config_entries(&self) -> Vec<CodexDefaultConfigEntry> {
        self.codex_default_config_entries
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn claude_default_config_entries(&self) -> Vec<CodexDefaultConfigEntry> {
        self.claude_default_config_entries
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn codex_api_auto_proxy_match_provider_ids(&self) -> Vec<String> {
        self.codex_api_auto_proxy_match_provider_ids
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn codex_config_key(&self) -> String {
        self.codex_config_key
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn codex_config_value(&self) -> String {
        self.codex_config_value
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn codex_secondary_config_key(&self) -> String {
        self.codex_secondary_config_key
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn codex_secondary_config_value(&self) -> String {
        self.codex_secondary_config_value
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn show_full_path(&self) -> bool {
        *self
            .show_full_path
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn workspace_browser_icon_path(&self) -> String {
        self.workspace_browser_icon_path
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn terminal_workspace_icon_path(&self) -> String {
        self.terminal_workspace_icon_path
            .read()
            .expect("workspace settings poisoned")
            .clone()
    }

    pub fn theme_mode(&self) -> ThemeMode {
        *self.theme_mode.read().expect("workspace settings poisoned")
    }

    pub fn font_size_tier_1(&self) -> f32 {
        *self
            .font_size_tier_1
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn font_size_tier_2(&self) -> f32 {
        *self
            .font_size_tier_2
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn font_size_tier_3(&self) -> f32 {
        *self
            .font_size_tier_3
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn font_size_tier_4(&self) -> f32 {
        *self
            .font_size_tier_4
            .read()
            .expect("workspace settings poisoned")
    }

    pub fn record_preset_sync_remote_url(&self, remote_url: &str) -> Result<Vec<String>> {
        let mut next_history = vec![remote_url.to_string()];
        next_history.extend(self.preset_sync_remote_url_history());
        let next_history = sanitize_preset_sync_remote_url_history(&next_history);

        persist_settings_file(
            &self.config_path(),
            &self.display_root(),
            &self.terminal_user(),
            &self.terminal_quick_commands(),
            &self.terminal_quick_start_default_key(),
            &self.terminal_default_env_vars(),
            &self.terminal_slash_commands(),
            &self.terminal_function_commands(),
            &self.terminal_command_collections(),
            &self.terminal_tool_entries(),
            &self.terminal_rename_presets(),
            self.show_dot_entries(),
            self.show_all_workspace_sessions(),
            self.desktop_terminal_soft_keyboard_enabled(),
            self.terminal_soft_keyboard_scale(),
            self.terminal_floating_button_offset_vh(),
            &self.terminal_fab_action_color(),
            self.terminal_fab_action_opacity(),
            self.terminal_fab_auto_expand(),
            self.terminal_touch_selection_long_press_ms(),
            self.terminal_scrollback_lines(),
            self.terminal_error_match_line_limit(),
            self.terminal_auto_continue_on_error(),
            self.terminal_auto_continue_interval_seconds(),
            self.terminal_auto_continue_backoff_factor(),
            self.terminal_auto_continue_backoff_max_minutes(),
            self.terminal_auto_continue_respect_manual_interrupt(),
            &self.terminal_auto_continue_time_patterns(),
            &self.terminal_auto_continue_active_window(),
            &self.terminal_scheduled_input_avoid_window(),
            &self.terminal_error_keywords(),
            &self.terminal_error_keyword_actions(),
            self.terminal_activity_agent_display(),
            self.terminal_completion_bell_enabled(),
            self.server_port_auto_increment(),
            self.compile_command_timeout_secs(),
            self.compile_max_concurrency(),
            &self.compile_environment(),
            self.gateway_listen_non_loopback(),
            self.session_ttl_days(),
            &self.favorite_paths(),
            &self.workspace_history(),
            &next_history,
            &self.desktop_remote_url(),
            &self.desktop_remote_url_history(),
            &self.claude_model_options(),
            &self.claude_default_config_entries(),
            &self.codex_default_config_entries(),
            &self.codex_api_auto_proxy_match_provider_ids(),
            &self.codex_config_key(),
            &self.codex_config_value(),
            &self.codex_secondary_config_key(),
            &self.codex_secondary_config_value(),
            self.show_full_path(),
            &self.workspace_browser_icon_path(),
            &self.terminal_workspace_icon_path(),
            self.theme_mode(),
            [
                self.font_size_tier_1(),
                self.font_size_tier_2(),
                self.font_size_tier_3(),
                self.font_size_tier_4(),
            ],
        )?;

        *self
            .preset_sync_remote_url_history
            .write()
            .expect("workspace settings poisoned") = next_history.clone();
        Ok(next_history)
    }

    pub fn update(
        &self,
        path: PathBuf,
        display_path: PathBuf,
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
        font_size_tiers: [f32; 4],
    ) {
        *self
            .current_root
            .write()
            .expect("workspace settings poisoned") = path;
        *self
            .display_root
            .write()
            .expect("workspace settings poisoned") = display_path;
        *self
            .terminal_user
            .write()
            .expect("workspace settings poisoned") = terminal_user;
        *self
            .terminal_quick_commands
            .write()
            .expect("workspace settings poisoned") = terminal_quick_commands;
        *self
            .terminal_quick_start_default_key
            .write()
            .expect("workspace settings poisoned") = terminal_quick_start_default_key;
        *self
            .terminal_default_env_vars
            .write()
            .expect("workspace settings poisoned") = terminal_default_env_vars;
        *self
            .terminal_slash_commands
            .write()
            .expect("workspace settings poisoned") = terminal_slash_commands;
        *self
            .terminal_function_commands
            .write()
            .expect("workspace settings poisoned") = terminal_function_commands;
        *self
            .terminal_command_collections
            .write()
            .expect("workspace settings poisoned") = terminal_command_collections;
        *self
            .terminal_tool_entries
            .write()
            .expect("workspace settings poisoned") = terminal_tool_entries;
        *self
            .terminal_rename_presets
            .write()
            .expect("workspace settings poisoned") = terminal_rename_presets;
        *self
            .show_dot_entries
            .write()
            .expect("workspace settings poisoned") = show_dot_entries;
        *self
            .show_all_workspace_sessions
            .write()
            .expect("workspace settings poisoned") = show_all_workspace_sessions;
        *self
            .desktop_terminal_soft_keyboard_enabled
            .write()
            .expect("workspace settings poisoned") = desktop_terminal_soft_keyboard_enabled;
        *self
            .terminal_soft_keyboard_scale
            .write()
            .expect("workspace settings poisoned") = terminal_soft_keyboard_scale;
        *self
            .terminal_floating_button_offset_vh
            .write()
            .expect("workspace settings poisoned") = terminal_floating_button_offset_vh;
        *self
            .terminal_fab_action_color
            .write()
            .expect("workspace settings poisoned") = terminal_fab_action_color;
        *self
            .terminal_fab_action_opacity
            .write()
            .expect("workspace settings poisoned") = terminal_fab_action_opacity;
        *self
            .terminal_fab_auto_expand
            .write()
            .expect("workspace settings poisoned") = terminal_fab_auto_expand;
        *self
            .terminal_touch_selection_long_press_ms
            .write()
            .expect("workspace settings poisoned") = terminal_touch_selection_long_press_ms;
        *self
            .terminal_scrollback_lines
            .write()
            .expect("workspace settings poisoned") = terminal_scrollback_lines;
        *self
            .terminal_error_match_line_limit
            .write()
            .expect("workspace settings poisoned") = terminal_error_match_line_limit;
        *self
            .terminal_auto_continue_on_error
            .write()
            .expect("workspace settings poisoned") = terminal_auto_continue_on_error;
        *self
            .terminal_auto_continue_interval_seconds
            .write()
            .expect("workspace settings poisoned") = terminal_auto_continue_interval_seconds;
        *self
            .terminal_auto_continue_backoff_factor
            .write()
            .expect("workspace settings poisoned") = terminal_auto_continue_backoff_factor;
        *self
            .terminal_auto_continue_backoff_max_minutes
            .write()
            .expect("workspace settings poisoned") = terminal_auto_continue_backoff_max_minutes;
        *self
            .terminal_auto_continue_respect_manual_interrupt
            .write()
            .expect("workspace settings poisoned") =
            terminal_auto_continue_respect_manual_interrupt;
        *self
            .terminal_auto_continue_time_patterns
            .write()
            .expect("workspace settings poisoned") = terminal_auto_continue_time_patterns;
        *self
            .terminal_auto_continue_active_window
            .write()
            .expect("workspace settings poisoned") = terminal_auto_continue_active_window;
        *self
            .terminal_scheduled_input_avoid_window
            .write()
            .expect("workspace settings poisoned") = terminal_scheduled_input_avoid_window;
        *self
            .terminal_error_keywords
            .write()
            .expect("workspace settings poisoned") = terminal_error_keywords;
        *self
            .terminal_error_keyword_actions
            .write()
            .expect("workspace settings poisoned") = terminal_error_keyword_actions;
        *self
            .terminal_activity_agent_display
            .write()
            .expect("workspace settings poisoned") = terminal_activity_agent_display;
        *self
            .terminal_completion_bell_enabled
            .write()
            .expect("workspace settings poisoned") = terminal_completion_bell_enabled;
        *self
            .server_port_auto_increment
            .write()
            .expect("workspace settings poisoned") = server_port_auto_increment;
        *self
            .compile_command_timeout_secs
            .write()
            .expect("workspace settings poisoned") = compile_command_timeout_secs;
        *self
            .compile_max_concurrency
            .write()
            .expect("workspace settings poisoned") = compile_max_concurrency;
        *self
            .compile_environment
            .write()
            .expect("workspace settings poisoned") = compile_environment;
        *self
            .gateway_listen_non_loopback
            .write()
            .expect("workspace settings poisoned") = gateway_listen_non_loopback;
        *self
            .session_ttl_days
            .write()
            .expect("workspace settings poisoned") = session_ttl_days;
        *self
            .favorite_paths
            .write()
            .expect("workspace settings poisoned") = favorite_paths;
        *self
            .workspace_history
            .write()
            .expect("workspace settings poisoned") = workspace_history;
        *self
            .preset_sync_remote_url_history
            .write()
            .expect("workspace settings poisoned") = preset_sync_remote_url_history;
        *self
            .desktop_remote_url
            .write()
            .expect("workspace settings poisoned") = desktop_remote_url;
        *self
            .desktop_remote_url_history
            .write()
            .expect("workspace settings poisoned") = desktop_remote_url_history;
        *self
            .claude_model_options
            .write()
            .expect("workspace settings poisoned") = claude_model_options;
        *self
            .claude_default_config_entries
            .write()
            .expect("workspace settings poisoned") = claude_default_config_entries;
        *self
            .codex_default_config_entries
            .write()
            .expect("workspace settings poisoned") = codex_default_config_entries;
        *self
            .codex_api_auto_proxy_match_provider_ids
            .write()
            .expect("workspace settings poisoned") = codex_api_auto_proxy_match_provider_ids;
        *self
            .codex_config_key
            .write()
            .expect("workspace settings poisoned") = codex_config_key;
        *self
            .codex_config_value
            .write()
            .expect("workspace settings poisoned") = codex_config_value;
        *self
            .codex_secondary_config_key
            .write()
            .expect("workspace settings poisoned") = codex_secondary_config_key;
        *self
            .codex_secondary_config_value
            .write()
            .expect("workspace settings poisoned") = codex_secondary_config_value;
        *self
            .show_full_path
            .write()
            .expect("workspace settings poisoned") = show_full_path;
        *self
            .workspace_browser_icon_path
            .write()
            .expect("workspace settings poisoned") = workspace_browser_icon_path;
        *self
            .terminal_workspace_icon_path
            .write()
            .expect("workspace settings poisoned") = terminal_workspace_icon_path;
        *self
            .theme_mode
            .write()
            .expect("workspace settings poisoned") = theme_mode;
        *self
            .font_size_tier_1
            .write()
            .expect("workspace settings poisoned") = font_size_tiers[0];
        *self
            .font_size_tier_2
            .write()
            .expect("workspace settings poisoned") = font_size_tiers[1];
        *self
            .font_size_tier_3
            .write()
            .expect("workspace settings poisoned") = font_size_tiers[2];
        *self
            .font_size_tier_4
            .write()
            .expect("workspace settings poisoned") = font_size_tiers[3];
    }
}
