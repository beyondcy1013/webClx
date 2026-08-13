use std::path::Path;

use tokio::fs;

use crate::*;

pub fn build_settings_response(
    manager: &SettingsManager,
    host_name: String,
    server_listen_addr: String,
    server_version: String,
) -> SettingsResult<SettingsResponse> {
    let terminal_profile = manager
        .terminal_user_profile()
        .map_err(|error| SettingsError::internal(format!("当前用户身份无效: {error}")))?;
    Ok(SettingsResponse {
        workspace_dir: manager.display_root().display().to_string(),
        default_workspace_dir: built_in_default_workspace_dir_display(),
        terminal_user: terminal_profile.name.clone(),
        default_terminal_user: default_terminal_user(),
        terminal_user_home: terminal_profile.home.display().to_string(),
        terminal_user_shell: terminal_profile.shell.display().to_string(),
        available_users: available_user_options(&terminal_profile),
        terminal_quick_commands: manager.terminal_quick_commands(),
        terminal_quick_start_default_key: manager.terminal_quick_start_default_key(),
        terminal_default_env_vars: manager.terminal_default_env_vars(),
        terminal_slash_commands: manager.terminal_slash_commands(),
        terminal_function_commands: manager.terminal_function_commands(),
        terminal_command_collections: manager.terminal_command_collections(),
        terminal_tool_entries: manager.terminal_tool_entries(),
        terminal_rename_presets: manager.terminal_rename_presets(),
        default_terminal_quick_commands: default_terminal_quick_commands(),
        default_terminal_quick_start_default_key: default_terminal_quick_start_default_key(),
        default_terminal_default_env_vars: default_terminal_default_env_vars(),
        default_terminal_slash_commands: default_terminal_slash_commands(),
        default_terminal_function_commands: default_terminal_function_commands(),
        default_terminal_command_collections: default_terminal_command_collections(),
        default_terminal_tool_entries: default_terminal_tool_entries(),
        default_terminal_rename_presets: default_terminal_rename_presets(),
        host_name,
        server_listen_addr,
        show_dot_entries: manager.show_dot_entries(),
        show_all_workspace_sessions: manager.show_all_workspace_sessions(),
        desktop_terminal_soft_keyboard_enabled: manager.desktop_terminal_soft_keyboard_enabled(),
        terminal_soft_keyboard_scale: manager.terminal_soft_keyboard_scale(),
        terminal_floating_button_offset_vh: manager.terminal_floating_button_offset_vh(),
        terminal_fab_action_color: manager.terminal_fab_action_color(),
        terminal_fab_action_opacity: manager.terminal_fab_action_opacity(),
        terminal_fab_auto_expand: manager.terminal_fab_auto_expand(),
        terminal_touch_selection_long_press_ms: manager.terminal_touch_selection_long_press_ms(),
        terminal_scrollback_lines: manager.terminal_scrollback_lines(),
        terminal_error_match_line_limit: manager.terminal_error_match_line_limit(),
        terminal_auto_continue_on_error: manager.terminal_auto_continue_on_error(),
        terminal_auto_continue_interval_seconds: manager.terminal_auto_continue_interval_seconds(),
        terminal_auto_continue_backoff_factor: manager.terminal_auto_continue_backoff_factor(),
        terminal_auto_continue_backoff_max_minutes: manager
            .terminal_auto_continue_backoff_max_minutes(),
        terminal_auto_continue_respect_manual_interrupt: manager
            .terminal_auto_continue_respect_manual_interrupt(),
        terminal_auto_continue_time_patterns: manager.terminal_auto_continue_time_patterns(),
        terminal_auto_continue_active_window: manager.terminal_auto_continue_active_window(),
        terminal_scheduled_input_avoid_window: manager.terminal_scheduled_input_avoid_window(),
        terminal_error_keywords: manager.terminal_error_keywords(),
        terminal_error_keyword_actions: manager.terminal_error_keyword_actions(),
        terminal_activity_agent_display: manager.terminal_activity_agent_display(),
        terminal_completion_bell_enabled: manager.terminal_completion_bell_enabled(),
        server_port_auto_increment: manager.server_port_auto_increment(),
        default_server_port_auto_increment: default_server_port_auto_increment(),
        compile_command_timeout_secs: manager.compile_command_timeout_secs(),
        default_compile_command_timeout_secs: default_compile_command_timeout_secs(),
        compile_max_concurrency: manager.compile_max_concurrency(),
        default_compile_max_concurrency: default_compile_max_concurrency(),
        compile_environment: manager.compile_environment(),
        gateway_listen_non_loopback: manager.gateway_listen_non_loopback(),
        default_gateway_listen_non_loopback: default_gateway_listen_non_loopback(),
        session_ttl_days: manager.session_ttl_days(),
        default_session_ttl_days: default_session_ttl_days(),
        favorite_paths: manager.favorite_paths(),
        workspace_history: manager.workspace_history(),
        preset_sync_remote_url_history: manager.preset_sync_remote_url_history(),
        desktop_remote_url: manager.desktop_remote_url(),
        desktop_remote_url_history: manager.desktop_remote_url_history(),
        claude_model_options: manager.claude_model_options(),
        claude_default_config_entries: manager.claude_default_config_entries(),
        codex_default_config_entries: manager.codex_default_config_entries(),
        codex_api_auto_proxy_match_provider_ids: manager.codex_api_auto_proxy_match_provider_ids(),
        codex_config_key: manager.codex_config_key(),
        codex_config_value: manager.codex_config_value(),
        codex_secondary_config_key: manager.codex_secondary_config_key(),
        codex_secondary_config_value: manager.codex_secondary_config_value(),
        show_full_path: manager.show_full_path(),
        workspace_browser_icon_path: manager.workspace_browser_icon_path(),
        terminal_workspace_icon_path: manager.terminal_workspace_icon_path(),
        theme_mode: manager.theme_mode(),
        font_size_tier_1: manager.font_size_tier_1(),
        font_size_tier_2: manager.font_size_tier_2(),
        font_size_tier_3: manager.font_size_tier_3(),
        font_size_tier_4: manager.font_size_tier_4(),
        config_file: display_path_for_settings(&manager.config_path()),
        server_version,
    })
}

fn display_path_for_settings(path: &Path) -> String {
    let value = path.display().to_string();
    if cfg!(windows) {
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    value
}

pub async fn save_settings(
    manager: &SettingsManager,
    payload: SaveSettingsRequest,
) -> SettingsResult<SaveSettingsResponse> {
    let codex_legacy_fields_supplied = payload.codex_config_key.is_some()
        || payload.codex_config_value.is_some()
        || payload.codex_secondary_config_key.is_some()
        || payload.codex_secondary_config_value.is_some()
        || payload.codex_model.is_some();
    let workspace_dir = match payload.workspace_dir {
        Some(raw) if raw.trim().is_empty() => resolve_built_in_default_workspace_dir()
            .map_err(|error| SettingsError::internal(format!("内置默认工作目录不可用: {error}")))?,
        Some(raw) => validate_workspace_dir(&raw)?,
        None => ResolvedWorkspaceDir {
            canonical: manager.current_root(),
            display: manager.display_root(),
        },
    };
    let terminal_profile = match payload.terminal_user {
        Some(raw) if raw.trim().is_empty() => resolve_terminal_user(&default_terminal_user())?,
        Some(raw) => resolve_terminal_user(&raw)?,
        None => manager
            .terminal_user_profile()
            .map_err(|error| SettingsError::bad_request(format!("用户身份无效: {error}")))?,
    };
    let terminal_user = terminal_profile.name.clone();
    let terminal_quick_commands = sanitize_terminal_quick_commands(
        &payload
            .terminal_quick_commands
            .unwrap_or_else(|| manager.terminal_quick_commands()),
    );
    let terminal_quick_start_default_key = sanitize_terminal_quick_start_default_key(
        &payload
            .terminal_quick_start_default_key
            .unwrap_or_else(|| manager.terminal_quick_start_default_key()),
        &terminal_quick_commands,
    );
    let terminal_default_env_vars = sanitize_terminal_default_env_vars(
        &payload
            .terminal_default_env_vars
            .unwrap_or_else(|| manager.terminal_default_env_vars()),
    );
    let terminal_slash_commands = sanitize_terminal_function_commands(
        &payload
            .terminal_slash_commands
            .unwrap_or_else(|| manager.terminal_slash_commands()),
    );
    let terminal_function_commands = sanitize_terminal_function_commands(
        &payload
            .terminal_function_commands
            .unwrap_or_else(|| manager.terminal_function_commands()),
    );
    let terminal_command_collections = sanitize_terminal_command_collections(
        &payload
            .terminal_command_collections
            .unwrap_or_else(|| manager.terminal_command_collections()),
    );
    let terminal_tool_entries = match payload.terminal_tool_entries {
        Some(entries) => validate_terminal_tool_entries(&entries)?,
        None => manager.terminal_tool_entries(),
    };
    let terminal_rename_presets = sanitize_terminal_rename_presets(
        &payload
            .terminal_rename_presets
            .unwrap_or_else(|| manager.terminal_rename_presets()),
    );
    let show_dot_entries = payload
        .show_dot_entries
        .unwrap_or_else(|| manager.show_dot_entries());
    let show_all_workspace_sessions = payload
        .show_all_workspace_sessions
        .unwrap_or_else(|| manager.show_all_workspace_sessions());
    let desktop_terminal_soft_keyboard_enabled = payload
        .desktop_terminal_soft_keyboard_enabled
        .unwrap_or_else(|| manager.desktop_terminal_soft_keyboard_enabled());
    let terminal_soft_keyboard_scale = normalize_terminal_soft_keyboard_scale(
        payload
            .terminal_soft_keyboard_scale
            .unwrap_or_else(|| manager.terminal_soft_keyboard_scale()),
    );
    let terminal_floating_button_offset_vh = normalize_terminal_floating_button_offset_vh(
        payload
            .terminal_floating_button_offset_vh
            .unwrap_or_else(|| manager.terminal_floating_button_offset_vh()),
    );
    let terminal_fab_action_color = payload
        .terminal_fab_action_color
        .as_deref()
        .map(normalize_terminal_fab_action_color)
        .unwrap_or_else(|| manager.terminal_fab_action_color());
    let terminal_fab_action_opacity = normalize_terminal_fab_action_opacity(
        payload
            .terminal_fab_action_opacity
            .unwrap_or_else(|| manager.terminal_fab_action_opacity()),
    );
    let terminal_fab_auto_expand = payload
        .terminal_fab_auto_expand
        .unwrap_or_else(|| manager.terminal_fab_auto_expand());
    let terminal_touch_selection_long_press_ms = normalize_terminal_touch_selection_long_press_ms(
        payload
            .terminal_touch_selection_long_press_ms
            .unwrap_or_else(|| manager.terminal_touch_selection_long_press_ms()),
    );
    let terminal_scrollback_lines = normalize_terminal_scrollback_lines(
        payload
            .terminal_scrollback_lines
            .unwrap_or_else(|| manager.terminal_scrollback_lines()),
    );
    let terminal_error_match_line_limit = normalize_terminal_error_match_line_limit(
        payload
            .terminal_error_match_line_limit
            .unwrap_or_else(|| manager.terminal_error_match_line_limit()),
    );
    let terminal_auto_continue_on_error = payload
        .terminal_auto_continue_on_error
        .unwrap_or_else(|| manager.terminal_auto_continue_on_error());
    let terminal_auto_continue_interval_seconds = normalize_terminal_auto_continue_interval_seconds(
        payload
            .terminal_auto_continue_interval_seconds
            .unwrap_or_else(|| manager.terminal_auto_continue_interval_seconds()),
    );
    let terminal_auto_continue_backoff_factor = normalize_terminal_auto_continue_backoff_factor(
        payload
            .terminal_auto_continue_backoff_factor
            .unwrap_or_else(|| manager.terminal_auto_continue_backoff_factor()),
    );
    let terminal_auto_continue_backoff_max_minutes =
        normalize_terminal_auto_continue_backoff_max_minutes(
            payload
                .terminal_auto_continue_backoff_max_minutes
                .unwrap_or_else(|| manager.terminal_auto_continue_backoff_max_minutes()),
        );
    let terminal_auto_continue_respect_manual_interrupt = payload
        .terminal_auto_continue_respect_manual_interrupt
        .unwrap_or_else(|| manager.terminal_auto_continue_respect_manual_interrupt());
    let terminal_auto_continue_time_patterns = sanitize_terminal_auto_continue_time_patterns(
        &payload
            .terminal_auto_continue_time_patterns
            .unwrap_or_else(|| manager.terminal_auto_continue_time_patterns()),
    );
    let terminal_auto_continue_active_window = normalize_terminal_auto_continue_active_window(
        &payload
            .terminal_auto_continue_active_window
            .unwrap_or_else(|| manager.terminal_auto_continue_active_window()),
    );
    let terminal_scheduled_input_avoid_window = normalize_terminal_scheduled_input_avoid_window(
        &payload
            .terminal_scheduled_input_avoid_window
            .unwrap_or_else(|| manager.terminal_scheduled_input_avoid_window()),
    );
    let terminal_error_keywords = sanitize_terminal_error_keywords(
        &payload
            .terminal_error_keywords
            .unwrap_or_else(|| manager.terminal_error_keywords()),
    );
    let terminal_error_keyword_actions = sanitize_terminal_error_keyword_actions(
        &payload
            .terminal_error_keyword_actions
            .unwrap_or_else(|| manager.terminal_error_keyword_actions()),
    );
    let terminal_activity_agent_display = payload
        .terminal_activity_agent_display
        .unwrap_or_else(|| manager.terminal_activity_agent_display());
    let terminal_completion_bell_enabled = payload
        .terminal_completion_bell_enabled
        .unwrap_or_else(|| manager.terminal_completion_bell_enabled());
    let server_port_auto_increment = payload
        .server_port_auto_increment
        .unwrap_or_else(|| manager.server_port_auto_increment());
    let compile_command_timeout_secs = normalize_compile_command_timeout_secs(
        payload
            .compile_command_timeout_secs
            .unwrap_or_else(|| manager.compile_command_timeout_secs()),
    );
    let compile_max_concurrency = normalize_compile_max_concurrency(
        payload
            .compile_max_concurrency
            .unwrap_or_else(|| manager.compile_max_concurrency()),
    );
    let compile_environment = sanitize_compile_environment(
        &payload
            .compile_environment
            .unwrap_or_else(|| manager.compile_environment()),
    );
    let gateway_listen_non_loopback = payload
        .gateway_listen_non_loopback
        .unwrap_or_else(|| manager.gateway_listen_non_loopback());
    let session_ttl_days = normalize_session_ttl_days(
        payload
            .session_ttl_days
            .unwrap_or_else(|| manager.session_ttl_days()),
    );
    let favorite_paths = sanitize_favorite_paths(
        &payload
            .favorite_paths
            .unwrap_or_else(|| manager.favorite_paths()),
    )
    .map_err(|error| SettingsError::bad_request(format!("收藏路径无效: {error}")))?;
    let workspace_history = sanitize_workspace_history(
        &payload
            .workspace_history
            .unwrap_or_else(|| manager.workspace_history()),
    );
    let preset_sync_remote_url_history = sanitize_preset_sync_remote_url_history(
        &payload
            .preset_sync_remote_url_history
            .unwrap_or_else(|| manager.preset_sync_remote_url_history()),
    );
    let desktop_remote_url = sanitize_desktop_remote_url(
        &payload
            .desktop_remote_url
            .unwrap_or_else(|| manager.desktop_remote_url()),
    );
    let desktop_remote_url_history = sanitize_desktop_remote_url_history(
        &payload
            .desktop_remote_url_history
            .unwrap_or_else(|| manager.desktop_remote_url_history()),
    );
    let claude_model_options = sanitize_claude_model_options(
        &payload
            .claude_model_options
            .unwrap_or_else(|| manager.claude_model_options()),
    );
    let claude_default_config_entries = sanitize_claude_default_config_entries(
        &payload
            .claude_default_config_entries
            .unwrap_or_else(|| manager.claude_default_config_entries()),
    );
    let codex_config_key = payload
        .codex_config_key
        .map(|value| sanitize_codex_config_key(&value))
        .unwrap_or_else(|| manager.codex_config_key());
    let codex_config_value = payload
        .codex_config_value
        .or(payload.codex_model)
        .map(|value| sanitize_codex_config_value(&value))
        .unwrap_or_else(|| manager.codex_config_value());
    let codex_secondary_config_key = payload
        .codex_secondary_config_key
        .map(|value| sanitize_codex_secondary_config_key(&value))
        .unwrap_or_else(|| manager.codex_secondary_config_key());
    let codex_secondary_config_value = payload
        .codex_secondary_config_value
        .map(|value| sanitize_codex_secondary_config_value(&value))
        .unwrap_or_else(|| manager.codex_secondary_config_value());
    let codex_default_config_entries = match payload.codex_default_config_entries {
        Some(entries) => sanitize_codex_default_config_entries(&entries),
        None if codex_legacy_fields_supplied => legacy_codex_default_config_entries(
            &codex_config_key,
            &codex_config_value,
            &codex_secondary_config_key,
            &codex_secondary_config_value,
        ),
        None => manager.codex_default_config_entries(),
    };
    let (
        codex_config_key,
        codex_config_value,
        codex_secondary_config_key,
        codex_secondary_config_value,
    ) = legacy_codex_fields_from_default_entries(&codex_default_config_entries);
    let codex_api_auto_proxy_match_provider_ids = sanitize_codex_api_auto_proxy_match_provider_ids(
        &payload
            .codex_api_auto_proxy_match_provider_ids
            .unwrap_or_else(|| manager.codex_api_auto_proxy_match_provider_ids()),
    );
    let show_full_path = sanitize_show_full_path(payload.show_full_path);
    let workspace_browser_icon_path = normalize_project_icon_relative_path(
        payload
            .workspace_browser_icon_path
            .as_deref()
            .unwrap_or(&manager.workspace_browser_icon_path()),
        DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
    );
    let terminal_workspace_icon_path = normalize_project_icon_relative_path(
        payload
            .terminal_workspace_icon_path
            .as_deref()
            .unwrap_or(&manager.terminal_workspace_icon_path()),
        DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
    );
    let theme_mode = payload.theme_mode.unwrap_or_else(|| manager.theme_mode());
    let font_size_tiers = normalize_font_size_tiers([
        payload
            .font_size_tier_1
            .unwrap_or_else(|| manager.font_size_tier_1()),
        payload
            .font_size_tier_2
            .unwrap_or_else(|| manager.font_size_tier_2()),
        payload
            .font_size_tier_3
            .unwrap_or_else(|| manager.font_size_tier_3()),
        payload
            .font_size_tier_4
            .unwrap_or_else(|| manager.font_size_tier_4()),
    ]);

    let settings_file = SettingsFile {
        workspace_dir: workspace_dir.display.display().to_string(),
        terminal_user: terminal_user.clone(),
        terminal_quick_commands: terminal_quick_commands.clone(),
        terminal_quick_start_default_key: terminal_quick_start_default_key.clone(),
        terminal_default_env_vars: terminal_default_env_vars.clone(),
        terminal_slash_commands: terminal_slash_commands.clone(),
        terminal_function_commands: terminal_function_commands.clone(),
        terminal_command_collections: terminal_command_collections.clone(),
        terminal_tool_entries: terminal_tool_entries.clone(),
        terminal_rename_presets: terminal_rename_presets.clone(),
        show_dot_entries,
        show_all_workspace_sessions,
        desktop_terminal_soft_keyboard_enabled,
        terminal_soft_keyboard_scale,
        terminal_floating_button_offset_vh,
        terminal_fab_action_color: terminal_fab_action_color.clone(),
        terminal_fab_action_opacity,
        terminal_fab_auto_expand,
        terminal_touch_selection_long_press_ms,
        terminal_scrollback_lines,
        terminal_error_match_line_limit,
        terminal_auto_continue_on_error,
        terminal_auto_continue_interval_seconds,
        terminal_auto_continue_backoff_factor,
        terminal_auto_continue_backoff_max_minutes,
        terminal_auto_continue_respect_manual_interrupt,
        terminal_auto_continue_time_patterns: terminal_auto_continue_time_patterns.clone(),
        terminal_auto_continue_active_window: terminal_auto_continue_active_window.clone(),
        terminal_scheduled_input_avoid_window: terminal_scheduled_input_avoid_window.clone(),
        terminal_error_keywords: terminal_error_keywords.clone(),
        terminal_error_keyword_actions: terminal_error_keyword_actions.clone(),
        terminal_activity_agent_display,
        terminal_completion_bell_enabled,
        server_port_auto_increment,
        compile_command_timeout_secs,
        compile_max_concurrency,
        compile_environment: compile_environment.clone(),
        gateway_listen_non_loopback,
        session_ttl_days,
        favorite_paths: favorite_paths.clone(),
        workspace_history: workspace_history.clone(),
        preset_sync_remote_url_history: preset_sync_remote_url_history.clone(),
        desktop_remote_url: desktop_remote_url.clone(),
        desktop_remote_url_history: desktop_remote_url_history.clone(),
        claude_model_options: claude_model_options.clone(),
        claude_default_config_entries: claude_default_config_entries.clone(),
        codex_default_config_entries: codex_default_config_entries.clone(),
        codex_api_auto_proxy_match_provider_ids: codex_api_auto_proxy_match_provider_ids.clone(),
        codex_config_key: codex_config_key.clone(),
        codex_config_value: Some(codex_config_value.clone()),
        codex_secondary_config_key: codex_secondary_config_key.clone(),
        codex_secondary_config_value: Some(codex_secondary_config_value.clone()),
        codex_model: None,
        show_full_path,
        workspace_browser_icon_path: workspace_browser_icon_path.clone(),
        terminal_workspace_icon_path: terminal_workspace_icon_path.clone(),
        path_display_prefix: None,
        theme_mode,
        font_size_tier_1: font_size_tiers[0],
        font_size_tier_2: font_size_tiers[1],
        font_size_tier_3: font_size_tiers[2],
        font_size_tier_4: font_size_tiers[3],
    };
    let encoded = serde_json::to_vec_pretty(&settings_file)
        .map_err(|error| SettingsError::internal(format!("序列化设置失败: {error}")))?;

    fs::write(manager.config_path(), encoded)
        .await
        .map_err(|error| SettingsError::internal(format!("写入设置文件失败: {error}")))?;

    manager.update(
        workspace_dir.canonical.clone(),
        workspace_dir.display.clone(),
        terminal_user.clone(),
        terminal_quick_commands.clone(),
        terminal_quick_start_default_key.clone(),
        terminal_default_env_vars.clone(),
        terminal_slash_commands.clone(),
        terminal_function_commands.clone(),
        terminal_command_collections.clone(),
        terminal_tool_entries.clone(),
        terminal_rename_presets.clone(),
        show_dot_entries,
        show_all_workspace_sessions,
        desktop_terminal_soft_keyboard_enabled,
        terminal_soft_keyboard_scale,
        terminal_floating_button_offset_vh,
        terminal_fab_action_color.clone(),
        terminal_fab_action_opacity,
        terminal_fab_auto_expand,
        terminal_touch_selection_long_press_ms,
        terminal_scrollback_lines,
        terminal_error_match_line_limit,
        terminal_auto_continue_on_error,
        terminal_auto_continue_interval_seconds,
        terminal_auto_continue_backoff_factor,
        terminal_auto_continue_backoff_max_minutes,
        terminal_auto_continue_respect_manual_interrupt,
        terminal_auto_continue_time_patterns.clone(),
        terminal_auto_continue_active_window.clone(),
        terminal_scheduled_input_avoid_window.clone(),
        terminal_error_keywords.clone(),
        terminal_error_keyword_actions.clone(),
        terminal_activity_agent_display,
        terminal_completion_bell_enabled,
        server_port_auto_increment,
        compile_command_timeout_secs,
        compile_max_concurrency,
        compile_environment.clone(),
        gateway_listen_non_loopback,
        session_ttl_days,
        favorite_paths.clone(),
        workspace_history.clone(),
        preset_sync_remote_url_history.clone(),
        desktop_remote_url.clone(),
        desktop_remote_url_history.clone(),
        claude_model_options.clone(),
        claude_default_config_entries.clone(),
        codex_default_config_entries.clone(),
        codex_api_auto_proxy_match_provider_ids.clone(),
        codex_config_key.clone(),
        codex_config_value.clone(),
        codex_secondary_config_key.clone(),
        codex_secondary_config_value.clone(),
        show_full_path,
        workspace_browser_icon_path.clone(),
        terminal_workspace_icon_path.clone(),
        theme_mode,
        font_size_tiers,
    );

    Ok(SaveSettingsResponse {
        ok: true,
        workspace_dir: workspace_dir.display.display().to_string(),
        terminal_user,
        terminal_user_home: terminal_profile.home.display().to_string(),
        terminal_user_shell: terminal_profile.shell.display().to_string(),
        terminal_quick_commands,
        terminal_quick_start_default_key,
        terminal_default_env_vars,
        terminal_slash_commands,
        terminal_function_commands,
        terminal_command_collections,
        terminal_tool_entries,
        terminal_rename_presets,
        show_dot_entries,
        show_all_workspace_sessions,
        desktop_terminal_soft_keyboard_enabled,
        terminal_soft_keyboard_scale,
        terminal_floating_button_offset_vh,
        terminal_fab_action_color,
        terminal_fab_action_opacity,
        terminal_fab_auto_expand,
        terminal_touch_selection_long_press_ms,
        terminal_scrollback_lines,
        terminal_error_match_line_limit,
        terminal_auto_continue_on_error,
        terminal_auto_continue_interval_seconds,
        terminal_auto_continue_backoff_factor,
        terminal_auto_continue_backoff_max_minutes,
        terminal_auto_continue_respect_manual_interrupt,
        terminal_auto_continue_time_patterns,
        terminal_auto_continue_active_window,
        terminal_scheduled_input_avoid_window,
        terminal_error_keywords,
        terminal_error_keyword_actions,
        terminal_activity_agent_display,
        terminal_completion_bell_enabled,
        server_port_auto_increment,
        compile_command_timeout_secs,
        compile_max_concurrency,
        compile_environment,
        gateway_listen_non_loopback,
        session_ttl_days,
        favorite_paths,
        workspace_history,
        preset_sync_remote_url_history,
        desktop_remote_url,
        desktop_remote_url_history,
        claude_model_options,
        claude_default_config_entries,
        codex_default_config_entries,
        codex_api_auto_proxy_match_provider_ids,
        codex_config_key,
        codex_config_value,
        codex_secondary_config_key,
        codex_secondary_config_value,
        show_full_path,
        workspace_browser_icon_path,
        terminal_workspace_icon_path,
        theme_mode,
        font_size_tier_1: font_size_tiers[0],
        font_size_tier_2: font_size_tiers[1],
        font_size_tier_3: font_size_tiers[2],
        font_size_tier_4: font_size_tiers[3],
    })
}

fn fields_for_tab(tab: &str) -> &'static [&'static str] {
    match tab {
        "system" => &[
            "workspace_dir",
            "terminal_user",
            "server_port_auto_increment",
            "compile_max_concurrency",
            "session_ttl_days",
        ],
        "terminal" => &[
            "terminal_error_keywords",
            "terminal_error_keyword_actions",
            "terminal_rename_presets",
            "terminal_activity_agent_display",
            "terminal_completion_bell_enabled",
            "server_port_auto_increment",
            "terminal_scrollback_lines",
            "terminal_error_match_line_limit",
            "terminal_auto_continue_on_error",
            "terminal_auto_continue_interval_seconds",
            "terminal_auto_continue_backoff_factor",
            "terminal_auto_continue_backoff_max_minutes",
            "terminal_auto_continue_respect_manual_interrupt",
            "terminal_auto_continue_time_patterns",
        ],
        "appearance" => &[
            "show_full_path",
            "workspace_browser_icon_path",
            "terminal_workspace_icon_path",
            "show_dot_entries",
            "show_all_workspace_sessions",
            "terminal_floating_button_offset_vh",
            "terminal_fab_action_color",
            "terminal_fab_action_opacity",
            "theme_mode",
            "font_size_tier_1",
            "font_size_tier_2",
            "font_size_tier_3",
            "font_size_tier_4",
        ],
        "compile" => &["compile_command_timeout_secs", "compile_environment"],
        "auto-continue-tasks" => &[
            "terminal_auto_continue_active_window",
            "terminal_scheduled_input_avoid_window",
        ],
        "soft-keyboard" | "shortcuts" => &[
            "desktop_terminal_soft_keyboard_enabled",
            "terminal_soft_keyboard_scale",
            "terminal_fab_auto_expand",
            "terminal_touch_selection_long_press_ms",
            "terminal_slash_commands",
            "terminal_function_commands",
            "terminal_command_collections",
            "terminal_quick_commands",
            "terminal_quick_start_default_key",
            "terminal_default_env_vars",
        ],
        "tools" => &["terminal_tool_entries"],
        "theme" => &["theme_mode"],
        "model" => &[
            "claude_model_options",
            "claude_default_config_entries",
            "codex_default_config_entries",
            "codex_api_auto_proxy_match_provider_ids",
            "codex_config_key",
            "codex_config_value",
            "codex_secondary_config_key",
            "codex_secondary_config_value",
        ],
        // Compatibility with links and clients from the previous settings layout.
        "workspace" => &[
            "terminal_error_keywords",
            "terminal_error_keyword_actions",
            "terminal_rename_presets",
            "terminal_activity_agent_display",
            "terminal_completion_bell_enabled",
            "server_port_auto_increment",
            "terminal_scrollback_lines",
            "terminal_error_match_line_limit",
            "terminal_auto_continue_on_error",
            "terminal_auto_continue_interval_seconds",
            "terminal_auto_continue_backoff_factor",
            "terminal_auto_continue_backoff_max_minutes",
            "terminal_auto_continue_respect_manual_interrupt",
            "terminal_auto_continue_time_patterns",
            "terminal_auto_continue_active_window",
            "terminal_scheduled_input_avoid_window",
        ],
        "display" => &[
            "show_full_path",
            "workspace_browser_icon_path",
            "terminal_workspace_icon_path",
            "show_dot_entries",
            "show_all_workspace_sessions",
            "terminal_floating_button_offset_vh",
            "terminal_fab_action_color",
            "terminal_fab_action_opacity",
        ],
        "font" => &[
            "font_size_tier_1",
            "font_size_tier_2",
            "font_size_tier_3",
            "font_size_tier_4",
        ],
        _ => &[],
    }
}

const ALL_MERGE_FIELDS: &[&str] = &[
    "terminal_quick_commands",
    "terminal_quick_start_default_key",
    "terminal_default_env_vars",
    "terminal_slash_commands",
    "terminal_function_commands",
    "terminal_command_collections",
    "terminal_tool_entries",
    "terminal_rename_presets",
    "show_dot_entries",
    "show_all_workspace_sessions",
    "desktop_terminal_soft_keyboard_enabled",
    "terminal_soft_keyboard_scale",
    "terminal_floating_button_offset_vh",
    "terminal_fab_action_color",
    "terminal_fab_action_opacity",
    "terminal_fab_auto_expand",
    "terminal_touch_selection_long_press_ms",
    "terminal_scrollback_lines",
    "terminal_error_match_line_limit",
    "terminal_auto_continue_on_error",
    "terminal_auto_continue_interval_seconds",
    "terminal_auto_continue_backoff_factor",
    "terminal_auto_continue_backoff_max_minutes",
    "terminal_auto_continue_respect_manual_interrupt",
    "terminal_auto_continue_time_patterns",
    "terminal_auto_continue_active_window",
    "terminal_scheduled_input_avoid_window",
    "terminal_error_keywords",
    "terminal_error_keyword_actions",
    "terminal_activity_agent_display",
    "terminal_completion_bell_enabled",
    "server_port_auto_increment",
    "compile_command_timeout_secs",
    "compile_max_concurrency",
    "compile_environment",
    "gateway_listen_non_loopback",
    "session_ttl_days",
    "favorite_paths",
    "workspace_history",
    "preset_sync_remote_url_history",
    "desktop_remote_url",
    "desktop_remote_url_history",
    "claude_model_options",
    "claude_default_config_entries",
    "codex_default_config_entries",
    "codex_api_auto_proxy_match_provider_ids",
    "codex_config_key",
    "codex_config_value",
    "codex_secondary_config_key",
    "codex_secondary_config_value",
    "show_full_path",
    "workspace_browser_icon_path",
    "terminal_workspace_icon_path",
    "theme_mode",
    "font_size_tier_1",
    "font_size_tier_2",
    "font_size_tier_3",
    "font_size_tier_4",
];

fn is_array_field(field: &str) -> bool {
    matches!(
        field,
        "terminal_quick_commands"
            | "terminal_default_env_vars"
            | "terminal_slash_commands"
            | "terminal_function_commands"
            | "terminal_command_collections"
            | "terminal_tool_entries"
            | "terminal_rename_presets"
            | "terminal_auto_continue_time_patterns"
            | "terminal_error_keywords"
            | "terminal_error_keyword_actions"
            | "compile_environment"
            | "preset_sync_remote_url_history"
            | "desktop_remote_url_history"
            | "claude_model_options"
            | "claude_default_config_entries"
            | "codex_default_config_entries"
            | "codex_api_auto_proxy_match_provider_ids"
            | "favorite_paths"
            | "workspace_history"
    )
}

fn is_instance_specific_field(field: &str) -> bool {
    matches!(field, "workspace_dir" | "terminal_user")
}

fn merge_arrays<T: Clone + std::hash::Hash + Eq>(local: Vec<T>, remote: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for item in remote.into_iter().chain(local) {
        if seen.insert(item.clone()) {
            merged.push(item);
        }
    }
    merged
}

fn merge_error_keyword_actions(
    local: Vec<TerminalErrorKeywordAction>,
    remote: Vec<TerminalErrorKeywordAction>,
) -> Vec<TerminalErrorKeywordAction> {
    let mut map: std::collections::HashMap<String, TerminalErrorKeywordAction> =
        std::collections::HashMap::new();
    for item in remote {
        map.insert(item.keyword.to_lowercase(), item);
    }
    for item in local {
        map.insert(item.keyword.to_lowercase(), item);
    }
    map.into_values().collect()
}

fn merge_quick_commands(
    local: Vec<TerminalQuickCommand>,
    remote: Vec<TerminalQuickCommand>,
) -> Vec<TerminalQuickCommand> {
    let mut map: std::collections::HashMap<String, TerminalQuickCommand> =
        std::collections::HashMap::new();
    for item in remote {
        map.insert(item.key.clone(), item);
    }
    for item in local {
        map.insert(item.key.clone(), item);
    }
    map.into_values().collect()
}

fn merge_function_commands(
    local: Vec<TerminalFunctionCommand>,
    remote: Vec<TerminalFunctionCommand>,
) -> Vec<TerminalFunctionCommand> {
    let mut map: std::collections::HashMap<String, TerminalFunctionCommand> =
        std::collections::HashMap::new();
    for item in remote {
        map.insert(item.key.clone(), item);
    }
    for item in local {
        map.insert(item.key.clone(), item);
    }
    map.into_values().collect()
}

fn merge_command_collections(
    local: Vec<TerminalCommandCollection>,
    remote: Vec<TerminalCommandCollection>,
) -> Vec<TerminalCommandCollection> {
    let mut map: std::collections::HashMap<String, TerminalCommandCollection> =
        std::collections::HashMap::new();
    for item in remote {
        map.insert(item.key.clone(), item);
    }
    for item in local {
        map.insert(item.key.clone(), item);
    }
    map.into_values().collect()
}

fn merge_terminal_tool_entries(
    local: Vec<TerminalToolEntry>,
    remote: Vec<TerminalToolEntry>,
) -> Vec<TerminalToolEntry> {
    let local_fallback = sanitize_terminal_tool_entries(&local);
    let mut map: std::collections::HashMap<String, TerminalToolEntry> =
        std::collections::HashMap::new();
    for item in remote {
        map.insert(item.id.clone(), item);
    }
    for item in local {
        map.insert(item.id.clone(), item);
    }
    let mut merged: Vec<_> = map.into_values().collect();
    merged.sort_by(|left, right| {
        left.root_key
            .cmp(&right.root_key)
            .then(left.sort_order.cmp(&right.sort_order))
            .then(left.label.cmp(&right.label))
    });
    validate_terminal_tool_entries(&merged).unwrap_or(local_fallback)
}

fn merge_default_env_vars(
    local: Vec<TerminalDefaultEnvVar>,
    remote: Vec<TerminalDefaultEnvVar>,
) -> Vec<TerminalDefaultEnvVar> {
    let mut map: std::collections::HashMap<String, TerminalDefaultEnvVar> =
        std::collections::HashMap::new();
    for item in remote {
        map.insert(item.key.clone(), item);
    }
    for item in local {
        map.insert(item.key.clone(), item);
    }
    map.into_values().collect()
}

fn merge_compile_env_vars(
    local: Vec<CompileEnvVar>,
    remote: Vec<CompileEnvVar>,
) -> Vec<CompileEnvVar> {
    let mut map = std::collections::HashMap::new();
    for item in remote {
        map.insert(item.key.clone(), item);
    }
    for item in local {
        map.insert(item.key.clone(), item);
    }
    map.into_values().collect()
}

fn merge_codex_default_config_entries(
    local: Vec<CodexDefaultConfigEntry>,
    remote: Vec<CodexDefaultConfigEntry>,
) -> Vec<CodexDefaultConfigEntry> {
    let mut map = std::collections::HashMap::new();
    for item in remote {
        map.insert(item.key.clone(), item);
    }
    for item in local {
        map.insert(item.key.clone(), item);
    }
    map.into_values().collect()
}

fn merge_favorite_paths(local: Vec<FavoritePath>, remote: Vec<FavoritePath>) -> Vec<FavoritePath> {
    let mut map: std::collections::HashMap<String, FavoritePath> = std::collections::HashMap::new();
    for item in remote {
        map.insert(item.path.clone(), item);
    }
    for item in local {
        map.insert(item.path.clone(), item);
    }
    map.into_values().collect()
}

fn merge_workspace_history(
    local: Vec<WorkspaceHistoryItem>,
    remote: Vec<WorkspaceHistoryItem>,
) -> Vec<WorkspaceHistoryItem> {
    let mut map: std::collections::HashMap<String, WorkspaceHistoryItem> =
        std::collections::HashMap::new();
    for item in remote {
        map.insert(item.path.clone(), item);
    }
    for item in local {
        map.insert(item.path.clone(), item);
    }
    map.into_values().collect()
}

fn merged_field_response(
    manager: &SettingsManager,
    field: &str,
    remote: SettingsResponse,
) -> SettingsResult<MergeFieldResponse> {
    if is_instance_specific_field(field) {
        return Err(SettingsError::bad_request(format!("字段 {} 为实例特定字段，不能合并", field)));
    }

    let merged_value = match field {
        "terminal_quick_commands" => serde_json::to_value(merge_quick_commands(
            manager.terminal_quick_commands(),
            remote.terminal_quick_commands,
        ))
        .unwrap(),
        "terminal_default_env_vars" => serde_json::to_value(merge_default_env_vars(
            manager.terminal_default_env_vars(),
            remote.terminal_default_env_vars,
        ))
        .unwrap(),
        "terminal_slash_commands" => serde_json::to_value(merge_function_commands(
            manager.terminal_slash_commands(),
            remote.terminal_slash_commands,
        ))
        .unwrap(),
        "terminal_function_commands" => serde_json::to_value(merge_function_commands(
            manager.terminal_function_commands(),
            remote.terminal_function_commands,
        ))
        .unwrap(),
        "terminal_command_collections" => serde_json::to_value(merge_command_collections(
            manager.terminal_command_collections(),
            remote.terminal_command_collections,
        ))
        .unwrap(),
        "terminal_tool_entries" => serde_json::to_value(merge_terminal_tool_entries(
            manager.terminal_tool_entries(),
            remote.terminal_tool_entries,
        ))
        .unwrap(),
        "terminal_rename_presets" => serde_json::to_value(merge_arrays(
            manager.terminal_rename_presets(),
            remote.terminal_rename_presets,
        ))
        .unwrap(),
        "terminal_error_keywords" => serde_json::to_value(merge_arrays(
            manager.terminal_error_keywords(),
            remote.terminal_error_keywords,
        ))
        .unwrap(),
        "terminal_error_keyword_actions" => serde_json::to_value(merge_error_keyword_actions(
            manager.terminal_error_keyword_actions(),
            remote.terminal_error_keyword_actions,
        ))
        .unwrap(),
        "compile_environment" => serde_json::to_value(merge_compile_env_vars(
            manager.compile_environment(),
            remote.compile_environment,
        ))
        .unwrap(),
        "preset_sync_remote_url_history" => serde_json::to_value(merge_arrays(
            manager.preset_sync_remote_url_history(),
            remote.preset_sync_remote_url_history,
        ))
        .unwrap(),
        "desktop_remote_url_history" => serde_json::to_value(merge_arrays(
            manager.desktop_remote_url_history(),
            remote.desktop_remote_url_history,
        ))
        .unwrap(),
        "claude_model_options" => serde_json::to_value(merge_arrays(
            manager.claude_model_options(),
            remote.claude_model_options,
        ))
        .unwrap(),
        "claude_default_config_entries" => {
            serde_json::to_value(merge_codex_default_config_entries(
                manager.claude_default_config_entries(),
                remote.claude_default_config_entries,
            ))
            .unwrap()
        }
        "codex_default_config_entries" => serde_json::to_value(merge_codex_default_config_entries(
            manager.codex_default_config_entries(),
            remote.codex_default_config_entries,
        ))
        .unwrap(),
        "codex_api_auto_proxy_match_provider_ids" => serde_json::to_value(merge_arrays(
            manager.codex_api_auto_proxy_match_provider_ids(),
            remote.codex_api_auto_proxy_match_provider_ids,
        ))
        .unwrap(),
        "favorite_paths" => serde_json::to_value(merge_favorite_paths(
            manager.favorite_paths(),
            remote.favorite_paths,
        ))
        .unwrap(),
        "workspace_history" => serde_json::to_value(merge_workspace_history(
            manager.workspace_history(),
            remote.workspace_history,
        ))
        .unwrap(),
        "show_dot_entries" => serde_json::json!(remote.show_dot_entries),
        "show_all_workspace_sessions" => serde_json::json!(remote.show_all_workspace_sessions),
        "desktop_terminal_soft_keyboard_enabled" => {
            serde_json::json!(remote.desktop_terminal_soft_keyboard_enabled)
        }
        "terminal_soft_keyboard_scale" => serde_json::json!(remote.terminal_soft_keyboard_scale),
        "terminal_floating_button_offset_vh" => {
            serde_json::json!(remote.terminal_floating_button_offset_vh)
        }
        "terminal_fab_action_color" => serde_json::json!(remote.terminal_fab_action_color),
        "terminal_fab_action_opacity" => serde_json::json!(remote.terminal_fab_action_opacity),
        "terminal_fab_auto_expand" => {
            serde_json::json!(remote.terminal_fab_auto_expand)
        }
        "terminal_touch_selection_long_press_ms" => {
            serde_json::json!(remote.terminal_touch_selection_long_press_ms)
        }
        "terminal_scrollback_lines" => serde_json::json!(remote.terminal_scrollback_lines),
        "terminal_error_match_line_limit" => {
            serde_json::json!(remote.terminal_error_match_line_limit)
        }
        "terminal_auto_continue_on_error" => {
            serde_json::json!(remote.terminal_auto_continue_on_error)
        }
        "terminal_auto_continue_interval_seconds" => {
            serde_json::json!(remote.terminal_auto_continue_interval_seconds)
        }
        "terminal_auto_continue_backoff_factor" => {
            serde_json::json!(remote.terminal_auto_continue_backoff_factor)
        }
        "terminal_auto_continue_backoff_max_minutes" => {
            serde_json::json!(remote.terminal_auto_continue_backoff_max_minutes)
        }
        "terminal_auto_continue_respect_manual_interrupt" => {
            serde_json::json!(remote.terminal_auto_continue_respect_manual_interrupt)
        }
        "terminal_auto_continue_time_patterns" => {
            serde_json::json!(remote.terminal_auto_continue_time_patterns)
        }
        "terminal_auto_continue_active_window" => {
            serde_json::json!(remote.terminal_auto_continue_active_window)
        }
        "terminal_scheduled_input_avoid_window" => {
            serde_json::json!(remote.terminal_scheduled_input_avoid_window)
        }
        "terminal_activity_agent_display" => {
            serde_json::json!(remote.terminal_activity_agent_display)
        }
        "terminal_completion_bell_enabled" => {
            serde_json::json!(remote.terminal_completion_bell_enabled)
        }
        "server_port_auto_increment" => serde_json::json!(remote.server_port_auto_increment),
        "compile_command_timeout_secs" => serde_json::json!(remote.compile_command_timeout_secs),
        "compile_max_concurrency" => serde_json::json!(remote.compile_max_concurrency),
        "gateway_listen_non_loopback" => serde_json::json!(remote.gateway_listen_non_loopback),
        "session_ttl_days" => serde_json::json!(remote.session_ttl_days),
        "desktop_remote_url" => serde_json::json!(remote.desktop_remote_url),
        "codex_config_key" => serde_json::json!(remote.codex_config_key),
        "codex_config_value" => serde_json::json!(remote.codex_config_value),
        "codex_secondary_config_key" => serde_json::json!(remote.codex_secondary_config_key),
        "codex_secondary_config_value" => serde_json::json!(remote.codex_secondary_config_value),
        "show_full_path" => serde_json::json!(remote.show_full_path),
        "workspace_browser_icon_path" => serde_json::json!(remote.workspace_browser_icon_path),
        "terminal_workspace_icon_path" => serde_json::json!(remote.terminal_workspace_icon_path),
        "theme_mode" => serde_json::json!(remote.theme_mode),
        "terminal_quick_start_default_key" => {
            serde_json::json!(remote.terminal_quick_start_default_key)
        }
        "font_size_tier_1" => serde_json::json!(remote.font_size_tier_1),
        "font_size_tier_2" => serde_json::json!(remote.font_size_tier_2),
        "font_size_tier_3" => serde_json::json!(remote.font_size_tier_3),
        "font_size_tier_4" => serde_json::json!(remote.font_size_tier_4),
        _ => {
            return Err(SettingsError::bad_request(format!("未知字段: {}", field)));
        }
    };

    Ok(MergeFieldResponse {
        field: field.to_string(),
        merged_value,
        merge_type: if is_array_field(field) {
            MergeType::UnionArray
        } else {
            MergeType::ScalarReplace
        },
    })
}

async fn persist_merged_fields(
    manager: &SettingsManager,
    responses: &[MergeFieldResponse],
) -> SettingsResult<()> {
    let current = build_settings_response(manager, String::new(), String::new(), String::new())?;
    let mut payload = serde_json::to_value(current)
        .map_err(|error| SettingsError::internal(format!("序列化当前设置失败: {error}")))?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| SettingsError::internal("当前设置不是 JSON 对象"))?;
    for response in responses {
        object.insert(response.field.clone(), response.merged_value.clone());
    }
    let request: SaveSettingsRequest = serde_json::from_value(payload)
        .map_err(|error| SettingsError::internal(format!("构造合并设置失败: {error}")))?;
    save_settings(manager, request).await?;
    Ok(())
}

pub async fn merge_field(
    manager: &SettingsManager,
    _remote_url: &str,
    field: &str,
    remote: SettingsResponse,
) -> SettingsResult<MergeFieldResponse> {
    let response = merged_field_response(manager, field, remote)?;
    persist_merged_fields(manager, std::slice::from_ref(&response)).await?;
    Ok(response)
}

pub async fn merge_tab(
    manager: &SettingsManager,
    _remote_url: &str,
    tab: &str,
    remote: SettingsResponse,
) -> SettingsResult<MergeTabResponse> {
    let fields = fields_for_tab(tab);
    if fields.is_empty() {
        return Err(SettingsError::bad_request(format!("未知Tab: {}", tab)));
    }

    let mut skipped = Vec::new();
    let mut responses = Vec::new();
    for field in fields {
        if is_instance_specific_field(field) {
            skipped.push(field.to_string());
            continue;
        }
        responses.push(merged_field_response(manager, field, remote.clone())?);
    }
    persist_merged_fields(manager, &responses).await?;

    Ok(MergeTabResponse {
        tab: tab.to_string(),
        applied: true,
        field_count: fields.len() - skipped.len(),
        skipped_fields: skipped,
    })
}

pub async fn merge_all(
    manager: &SettingsManager,
    _remote_url: &str,
    remote: SettingsResponse,
) -> SettingsResult<MergeAllResponse> {
    let mut responses = Vec::with_capacity(ALL_MERGE_FIELDS.len());
    for field in ALL_MERGE_FIELDS {
        responses.push(merged_field_response(manager, field, remote.clone())?);
    }
    persist_merged_fields(manager, &responses).await?;

    Ok(MergeAllResponse {
        applied: true,
        field_count: responses.len(),
        skipped_fields: vec!["workspace_dir".to_string(), "terminal_user".to_string()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn terminal_tool_test_entry(
        id: &str,
        parent_id: Option<&str>,
        kind: &str,
    ) -> TerminalToolEntry {
        TerminalToolEntry {
            id: id.to_string(),
            root_key: TERMINAL_TOOL_ROOT_TOOLS.to_string(),
            parent_id: parent_id.map(str::to_string),
            kind: kind.to_string(),
            label: id.to_string(),
            sort_order: 10,
            actions: if kind == TERMINAL_TOOL_ENTRY_KIND_ACTION {
                vec![TerminalToolAction {
                    kind: TERMINAL_TOOL_ACTION_CREATE_TERMINAL.to_string(),
                    value: String::new(),
                    seconds: 0.0,
                    ..Default::default()
                }]
            } else {
                Vec::new()
            },
        }
    }

    fn temp_app_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("webclx-settings-{label}-{}-{nonce}", std::process::id()))
    }

    #[tokio::test]
    async fn merge_terminal_tab_persists_remote_values() {
        let local_dir = temp_app_dir("merge-local");
        let remote_dir = temp_app_dir("merge-remote");
        std::fs::create_dir_all(&local_dir).expect("create local app dir");
        std::fs::create_dir_all(&remote_dir).expect("create remote app dir");
        std::fs::write(
            remote_dir.join(SETTINGS_FILE_NAME),
            r#"{
  "workspace_dir": "/home/codes",
  "terminal_completion_bell_enabled": false,
  "terminal_auto_continue_interval_seconds": 321
}"#,
        )
        .expect("write remote settings");

        let local = SettingsManager::load(&local_dir).expect("load local settings");
        let remote = SettingsManager::load(&remote_dir).expect("load remote settings");
        let remote_response =
            build_settings_response(&remote, String::new(), String::new(), String::new())
                .expect("build remote response");

        let response = merge_tab(&local, "", "terminal", remote_response)
            .await
            .expect("merge terminal tab");

        assert!(response.applied);
        assert!(!local.terminal_completion_bell_enabled());
        assert_eq!(local.terminal_auto_continue_interval_seconds(), 321);
        let persisted = std::fs::read_to_string(local_dir.join(SETTINGS_FILE_NAME))
            .expect("read local settings");
        assert!(persisted.contains(r#""terminal_completion_bell_enabled": false"#));
        assert!(persisted.contains(r#""terminal_auto_continue_interval_seconds": 321"#));

        std::fs::remove_dir_all(local_dir).ok();
        std::fs::remove_dir_all(remote_dir).ok();
    }

    #[test]
    fn merge_terminal_tool_entries_keeps_local_tree_when_remote_breaks_hierarchy() {
        let local = vec![
            terminal_tool_test_entry("local-folder", None, TERMINAL_TOOL_ENTRY_KIND_FOLDER),
            terminal_tool_test_entry(
                "local-action",
                Some("local-folder"),
                TERMINAL_TOOL_ENTRY_KIND_ACTION,
            ),
        ];
        let remote = vec![terminal_tool_test_entry(
            "orphan-action",
            Some("missing-folder"),
            TERMINAL_TOOL_ENTRY_KIND_ACTION,
        )];

        let merged = merge_terminal_tool_entries(local.clone(), remote);

        assert_eq!(merged, local);
    }
}
