use std::path::Path;

use anyhow::{Context, Result};
use tracing::warn;

use crate::*;

pub(crate) fn load_saved_settings(config_path: &Path) -> Result<Option<LoadedSettings>> {
    if !config_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("cannot read {}", config_path.display()))?;
    let parsed: SettingsFile =
        serde_json::from_str(&content).context("cannot parse settings file")?;
    let workspace_dir = match resolve_workspace_dir(&parsed.workspace_dir) {
        Ok(resolved) => resolved,
        Err(error) => {
            warn!(
                "saved workspace dir `{}` is invalid, fallback to default workspace: {error}",
                parsed.workspace_dir
            );
            resolve_built_in_default_workspace_dir()
                .context("saved workspace dir is invalid and no built-in default is available")?
        }
    };
    let terminal_user = match validate_terminal_user(&parsed.terminal_user) {
        Ok(profile) => profile.name,
        Err(error) => {
            warn!(
                "saved terminal user `{}` is invalid, fallback to default user: {error}",
                parsed.terminal_user
            );
            validate_terminal_user(&default_terminal_user())
                .context("saved terminal user is invalid and default user is unavailable")?
                .name
        }
    };
    let terminal_quick_commands = sanitize_terminal_quick_commands(&parsed.terminal_quick_commands);
    let terminal_quick_start_default_key = sanitize_terminal_quick_start_default_key(
        &parsed.terminal_quick_start_default_key,
        &terminal_quick_commands,
    );
    let terminal_default_env_vars =
        sanitize_terminal_default_env_vars(&parsed.terminal_default_env_vars);
    let terminal_slash_commands =
        sanitize_terminal_function_commands(&parsed.terminal_slash_commands);
    let terminal_function_commands = sanitize_terminal_function_commands(
        &parsed
            .terminal_function_commands
            .into_iter()
            .filter(|command| {
                command.action != "send_slash_command" && !command.command.starts_with('/')
            })
            .collect::<Vec<_>>(),
    );
    let terminal_function_commands = if terminal_function_commands.is_empty() {
        sanitize_terminal_function_commands(&default_terminal_function_commands())
    } else {
        terminal_function_commands
    };
    let terminal_command_collections =
        sanitize_terminal_command_collections(&parsed.terminal_command_collections);
    let terminal_command_collections = if terminal_command_collections.is_empty() {
        sanitize_terminal_command_collections(&default_terminal_command_collections())
    } else {
        terminal_command_collections
    };
    let terminal_tool_entries = merge_builtin_terminal_tool_entries(
        &sanitize_terminal_tool_entries(&parsed.terminal_tool_entries),
    );
    let terminal_rename_presets = sanitize_terminal_rename_presets(&parsed.terminal_rename_presets);
    let favorite_paths = sanitize_favorite_paths(&parsed.favorite_paths)
        .context("saved favorite paths are invalid")?;
    let workspace_history = sanitize_workspace_history(&parsed.workspace_history);
    let preset_sync_remote_url_history =
        sanitize_preset_sync_remote_url_history(&parsed.preset_sync_remote_url_history);
    let desktop_remote_url = sanitize_desktop_remote_url(&parsed.desktop_remote_url);
    let desktop_remote_url_history =
        sanitize_desktop_remote_url_history(&parsed.desktop_remote_url_history);
    let claude_model_options = sanitize_claude_model_options(&parsed.claude_model_options);
    let claude_default_config_entries = if parsed.claude_default_config_entries.is_empty() {
        default_claude_default_config_entries()
    } else {
        sanitize_claude_default_config_entries(&parsed.claude_default_config_entries)
    };
    let codex_config_key = sanitize_codex_config_key(&parsed.codex_config_key);
    let codex_config_value = sanitize_codex_config_value(
        parsed
            .codex_config_value
            .as_deref()
            .or(parsed.codex_model.as_deref())
            .unwrap_or(DEFAULT_CODEX_MODEL),
    );
    let codex_secondary_config_key =
        sanitize_codex_secondary_config_key(&parsed.codex_secondary_config_key);
    let codex_secondary_config_value = sanitize_codex_secondary_config_value(
        parsed
            .codex_secondary_config_value
            .as_deref()
            .unwrap_or(DEFAULT_CODEX_SECONDARY_CONFIG_VALUE),
    );
    let legacy_config_entries = legacy_codex_default_config_entries(
        &codex_config_key,
        &codex_config_value,
        &codex_secondary_config_key,
        &codex_secondary_config_value,
    );
    let codex_default_config_entries = if parsed.codex_default_config_entries.is_empty() {
        legacy_config_entries
    } else {
        sanitize_codex_default_config_entries(&parsed.codex_default_config_entries)
    };
    let (
        codex_config_key,
        codex_config_value,
        codex_secondary_config_key,
        codex_secondary_config_value,
    ) = legacy_codex_fields_from_default_entries(&codex_default_config_entries);
    let font_size_tiers = normalize_font_size_tiers([
        parsed.font_size_tier_1,
        parsed.font_size_tier_2,
        parsed.font_size_tier_3,
        parsed.font_size_tier_4,
    ]);
    let terminal_soft_keyboard_scale =
        normalize_terminal_soft_keyboard_scale(parsed.terminal_soft_keyboard_scale);
    let terminal_floating_button_offset_vh =
        normalize_terminal_floating_button_offset_vh(parsed.terminal_floating_button_offset_vh);
    let terminal_fab_action_color =
        normalize_terminal_fab_action_color(&parsed.terminal_fab_action_color);
    let terminal_fab_action_opacity =
        normalize_terminal_fab_action_opacity(parsed.terminal_fab_action_opacity);
    let terminal_fab_auto_expand = parsed.terminal_fab_auto_expand;
    let terminal_touch_selection_long_press_ms = normalize_terminal_touch_selection_long_press_ms(
        parsed.terminal_touch_selection_long_press_ms,
    );
    let terminal_scrollback_lines =
        normalize_terminal_scrollback_lines(parsed.terminal_scrollback_lines);
    let terminal_error_match_line_limit =
        normalize_terminal_error_match_line_limit(parsed.terminal_error_match_line_limit);
    let terminal_auto_continue_on_error = parsed.terminal_auto_continue_on_error;
    let terminal_auto_continue_interval_seconds = normalize_terminal_auto_continue_interval_seconds(
        parsed.terminal_auto_continue_interval_seconds,
    );
    let terminal_auto_continue_backoff_factor = normalize_terminal_auto_continue_backoff_factor(
        parsed.terminal_auto_continue_backoff_factor,
    );
    let terminal_auto_continue_backoff_max_minutes =
        normalize_terminal_auto_continue_backoff_max_minutes(
            parsed.terminal_auto_continue_backoff_max_minutes,
        );
    let terminal_auto_continue_respect_manual_interrupt =
        parsed.terminal_auto_continue_respect_manual_interrupt;
    let terminal_auto_continue_time_patterns =
        sanitize_terminal_auto_continue_time_patterns(&parsed.terminal_auto_continue_time_patterns);
    let terminal_auto_continue_active_window = normalize_terminal_auto_continue_active_window(
        &parsed.terminal_auto_continue_active_window,
    );
    let terminal_scheduled_input_avoid_window = normalize_terminal_scheduled_input_avoid_window(
        &parsed.terminal_scheduled_input_avoid_window,
    );
    let terminal_error_keywords =
        merge_builtin_terminal_error_keywords(&parsed.terminal_error_keywords);
    let terminal_error_keyword_actions =
        merge_builtin_terminal_error_keyword_actions(&parsed.terminal_error_keyword_actions);
    let terminal_activity_agent_display = parsed.terminal_activity_agent_display;
    let terminal_completion_bell_enabled = parsed.terminal_completion_bell_enabled;
    let server_port_auto_increment = parsed.server_port_auto_increment;
    let compile_command_timeout_secs = parsed.compile_command_timeout_secs;
    let compile_max_concurrency = normalize_compile_max_concurrency(parsed.compile_max_concurrency);
    let compile_environment = sanitize_compile_environment(&parsed.compile_environment);
    let gateway_listen_non_loopback = parsed.gateway_listen_non_loopback;
    let session_ttl_days = parsed.session_ttl_days;
    Ok(Some(LoadedSettings {
        workspace_dir: workspace_dir.canonical,
        display_workspace_dir: workspace_dir.display,
        terminal_user,
        terminal_quick_commands,
        terminal_quick_start_default_key,
        terminal_default_env_vars,
        terminal_slash_commands,
        terminal_function_commands,
        terminal_command_collections,
        terminal_tool_entries,
        terminal_rename_presets,
        show_dot_entries: parsed.show_dot_entries,
        show_all_workspace_sessions: parsed.show_all_workspace_sessions,
        desktop_terminal_soft_keyboard_enabled: parsed.desktop_terminal_soft_keyboard_enabled,
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
        codex_api_auto_proxy_match_provider_ids: sanitize_codex_api_auto_proxy_match_provider_ids(
            &parsed.codex_api_auto_proxy_match_provider_ids,
        ),
        codex_config_key,
        codex_config_value,
        codex_secondary_config_key,
        codex_secondary_config_value,
        show_full_path: sanitize_show_full_path(
            migrate_path_display_prefix_to_show_full_path(&parsed.path_display_prefix)
                .or(Some(parsed.show_full_path)),
        ),
        workspace_browser_icon_path: normalize_project_icon_relative_path(
            &parsed.workspace_browser_icon_path,
            DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
        ),
        terminal_workspace_icon_path: normalize_project_icon_relative_path(
            &parsed.terminal_workspace_icon_path,
            DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
        ),
        theme_mode: parsed.theme_mode,
        font_size_tier_1: font_size_tiers[0],
        font_size_tier_2: font_size_tiers[1],
        font_size_tier_3: font_size_tiers[2],
        font_size_tier_4: font_size_tiers[3],
    }))
}

pub(crate) fn persist_settings_file(
    config_path: &Path,
    workspace_dir: &Path,
    terminal_user: &str,
    terminal_quick_commands: &[TerminalQuickCommand],
    terminal_quick_start_default_key: &str,
    terminal_default_env_vars: &[TerminalDefaultEnvVar],
    terminal_slash_commands: &[TerminalFunctionCommand],
    terminal_function_commands: &[TerminalFunctionCommand],
    terminal_command_collections: &[TerminalCommandCollection],
    terminal_tool_entries: &[TerminalToolEntry],
    terminal_rename_presets: &[String],
    show_dot_entries: bool,
    show_all_workspace_sessions: bool,
    desktop_terminal_soft_keyboard_enabled: bool,
    terminal_soft_keyboard_scale: f32,
    terminal_floating_button_offset_vh: f32,
    terminal_fab_action_color: &str,
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
    terminal_auto_continue_time_patterns: &[String],
    terminal_auto_continue_active_window: &str,
    terminal_scheduled_input_avoid_window: &str,
    terminal_error_keywords: &[String],
    terminal_error_keyword_actions: &[TerminalErrorKeywordAction],
    terminal_activity_agent_display: TerminalActivityAgentDisplay,
    terminal_completion_bell_enabled: bool,
    server_port_auto_increment: bool,
    compile_command_timeout_secs: u64,
    compile_max_concurrency: u32,
    compile_environment: &[CompileEnvVar],
    gateway_listen_non_loopback: bool,
    session_ttl_days: u32,
    favorite_paths: &[FavoritePath],
    workspace_history: &[WorkspaceHistoryItem],
    preset_sync_remote_url_history: &[String],
    desktop_remote_url: &str,
    desktop_remote_url_history: &[String],
    claude_model_options: &[String],
    claude_default_config_entries: &[CodexDefaultConfigEntry],
    codex_default_config_entries: &[CodexDefaultConfigEntry],
    codex_api_auto_proxy_match_provider_ids: &[String],
    codex_config_key: &str,
    codex_config_value: &str,
    codex_secondary_config_key: &str,
    codex_secondary_config_value: &str,
    show_full_path: bool,
    workspace_browser_icon_path: &str,
    terminal_workspace_icon_path: &str,
    theme_mode: ThemeMode,
    font_size_tiers: [f32; 4],
) -> Result<()> {
    let sanitized_quick_commands = sanitize_terminal_quick_commands(terminal_quick_commands);
    let sanitized_quick_start_default_key = sanitize_terminal_quick_start_default_key(
        terminal_quick_start_default_key,
        &sanitized_quick_commands,
    );
    let sanitized_terminal_default_env_vars =
        sanitize_terminal_default_env_vars(terminal_default_env_vars);
    let sanitized_terminal_slash_commands =
        sanitize_terminal_function_commands(terminal_slash_commands);
    let sanitized_terminal_function_commands =
        sanitize_terminal_function_commands(terminal_function_commands);
    let sanitized_terminal_command_collections =
        sanitize_terminal_command_collections(terminal_command_collections);
    let sanitized_terminal_tool_entries = sanitize_terminal_tool_entries(terminal_tool_entries);
    let sanitized_terminal_rename_presets =
        sanitize_terminal_rename_presets(terminal_rename_presets);
    let sanitized_terminal_error_keywords =
        sanitize_terminal_error_keywords(terminal_error_keywords);
    let sanitized_terminal_error_keyword_actions =
        sanitize_terminal_error_keyword_actions(terminal_error_keyword_actions);
    let sanitized_claude_default_config_entries =
        sanitize_claude_default_config_entries(claude_default_config_entries);
    let legacy_entries = legacy_codex_default_config_entries(
        codex_config_key,
        codex_config_value,
        codex_secondary_config_key,
        codex_secondary_config_value,
    );
    let sanitized_codex_default_config_entries = if codex_default_config_entries.is_empty() {
        legacy_entries
    } else {
        sanitize_codex_default_config_entries(codex_default_config_entries)
    };
    let (
        codex_config_key,
        codex_config_value,
        codex_secondary_config_key,
        codex_secondary_config_value,
    ) = legacy_codex_fields_from_default_entries(&sanitized_codex_default_config_entries);
    let content = serde_json::to_vec_pretty(&SettingsFile {
        workspace_dir: workspace_dir.display().to_string(),
        terminal_user: sanitize_terminal_user_name(terminal_user),
        terminal_quick_commands: sanitized_quick_commands,
        terminal_quick_start_default_key: sanitized_quick_start_default_key,
        terminal_default_env_vars: sanitized_terminal_default_env_vars,
        terminal_slash_commands: sanitized_terminal_slash_commands,
        terminal_function_commands: sanitized_terminal_function_commands,
        terminal_command_collections: sanitized_terminal_command_collections,
        terminal_tool_entries: sanitized_terminal_tool_entries,
        terminal_rename_presets: sanitized_terminal_rename_presets,
        show_dot_entries,
        show_all_workspace_sessions,
        desktop_terminal_soft_keyboard_enabled,
        terminal_soft_keyboard_scale: normalize_terminal_soft_keyboard_scale(
            terminal_soft_keyboard_scale,
        ),
        terminal_floating_button_offset_vh: normalize_terminal_floating_button_offset_vh(
            terminal_floating_button_offset_vh,
        ),
        terminal_fab_action_color: normalize_terminal_fab_action_color(terminal_fab_action_color),
        terminal_fab_action_opacity: normalize_terminal_fab_action_opacity(
            terminal_fab_action_opacity,
        ),
        terminal_fab_auto_expand,
        terminal_touch_selection_long_press_ms: normalize_terminal_touch_selection_long_press_ms(
            terminal_touch_selection_long_press_ms,
        ),
        terminal_scrollback_lines: normalize_terminal_scrollback_lines(terminal_scrollback_lines),
        terminal_error_match_line_limit: normalize_terminal_error_match_line_limit(
            terminal_error_match_line_limit,
        ),
        terminal_auto_continue_on_error,
        terminal_auto_continue_interval_seconds: normalize_terminal_auto_continue_interval_seconds(
            terminal_auto_continue_interval_seconds,
        ),
        terminal_auto_continue_backoff_factor: normalize_terminal_auto_continue_backoff_factor(
            terminal_auto_continue_backoff_factor,
        ),
        terminal_auto_continue_backoff_max_minutes:
            normalize_terminal_auto_continue_backoff_max_minutes(
                terminal_auto_continue_backoff_max_minutes,
            ),
        terminal_auto_continue_respect_manual_interrupt,
        terminal_auto_continue_time_patterns: sanitize_terminal_auto_continue_time_patterns(
            terminal_auto_continue_time_patterns,
        ),
        terminal_auto_continue_active_window: normalize_terminal_auto_continue_active_window(
            terminal_auto_continue_active_window,
        ),
        terminal_scheduled_input_avoid_window: normalize_terminal_scheduled_input_avoid_window(
            terminal_scheduled_input_avoid_window,
        ),
        terminal_error_keywords: sanitized_terminal_error_keywords,
        terminal_error_keyword_actions: sanitized_terminal_error_keyword_actions,
        terminal_activity_agent_display,
        terminal_completion_bell_enabled,
        server_port_auto_increment,
        compile_command_timeout_secs: normalize_compile_command_timeout_secs(
            compile_command_timeout_secs,
        ),
        compile_max_concurrency: normalize_compile_max_concurrency(compile_max_concurrency),
        compile_environment: sanitize_compile_environment(compile_environment),
        gateway_listen_non_loopback,
        session_ttl_days,
        favorite_paths: favorite_paths.to_vec(),
        workspace_history: sanitize_workspace_history(workspace_history),
        preset_sync_remote_url_history: sanitize_preset_sync_remote_url_history(
            preset_sync_remote_url_history,
        ),
        desktop_remote_url: sanitize_desktop_remote_url(desktop_remote_url),
        desktop_remote_url_history: sanitize_desktop_remote_url_history(desktop_remote_url_history),
        claude_model_options: claude_model_options.to_vec(),
        claude_default_config_entries: sanitized_claude_default_config_entries,
        codex_default_config_entries: sanitized_codex_default_config_entries,
        codex_api_auto_proxy_match_provider_ids: sanitize_codex_api_auto_proxy_match_provider_ids(
            codex_api_auto_proxy_match_provider_ids,
        ),
        codex_config_key,
        codex_config_value: Some(codex_config_value),
        codex_secondary_config_key,
        codex_secondary_config_value: Some(codex_secondary_config_value),
        codex_model: None,
        show_full_path,
        workspace_browser_icon_path: normalize_project_icon_relative_path(
            workspace_browser_icon_path,
            DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
        ),
        terminal_workspace_icon_path: normalize_project_icon_relative_path(
            terminal_workspace_icon_path,
            DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
        ),
        path_display_prefix: None,
        theme_mode,
        font_size_tier_1: font_size_tiers[0],
        font_size_tier_2: font_size_tiers[1],
        font_size_tier_3: font_size_tiers[2],
        font_size_tier_4: font_size_tiers[3],
    })
    .context("cannot encode settings file")?;
    std::fs::write(config_path, content)
        .with_context(|| format!("cannot write {}", config_path.display()))?;
    set_owner_only_permissions(config_path)
        .with_context(|| format!("cannot update {} permissions", config_path.display()))?;
    Ok(())
}

#[cfg(unix)]
fn set_owner_only_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("cannot chmod {}", path.display()))
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
