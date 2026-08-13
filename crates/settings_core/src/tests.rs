use super::{
    CONTEXT_WINDOW_EXHAUSTED_ERROR_KEYWORD, CodexDefaultConfigEntry, CompileEnvVar,
    DEFAULT_CODEX_CONFIG_KEY, DEFAULT_CODEX_SECONDARY_CONFIG_KEY,
    DEFAULT_CODEX_SECONDARY_CONFIG_VALUE, DEFAULT_COMPILE_MAX_CONCURRENCY,
    DEFAULT_FONT_SIZE_TIER_1, DEFAULT_FONT_SIZE_TIER_2, DEFAULT_TERMINAL_FAB_ACTION_COLOR,
    DEFAULT_TERMINAL_FAB_ACTION_OPACITY, DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
    DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY, DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE,
    DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS, FavoritePath, FavoritePathKind,
    OPENAI_CYBERSECURITY_BLOCK_PHRASE_KEYWORD, OPENAI_CYBERSECURITY_BLOCK_TITLE_KEYWORD,
    SettingsFile, TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE,
    TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE, TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY,
    TerminalActivityAgentDisplay, TerminalDefaultEnvVar, TerminalErrorKeywordAction,
    TerminalFunctionCommand, TerminalQuickCommand, TerminalToolAction, TerminalToolEntry,
    ThemeMode, WorkspaceHistoryItem, default_desktop_terminal_soft_keyboard_enabled,
    default_show_all_workspace_sessions, default_show_dot_entries,
    default_terminal_error_keyword_actions, default_terminal_fab_action_color,
    default_terminal_fab_action_opacity, default_terminal_floating_button_offset_vh,
    default_terminal_tool_entries, default_terminal_touch_selection_long_press_ms,
    default_terminal_workspace_icon_path, default_theme_mode, default_workspace_browser_icon_path,
    default_workspace_dir_candidates, is_within_workspace_limit, load_saved_settings,
    merge_builtin_terminal_error_keyword_actions, normalize_compile_max_concurrency,
    normalize_favorite_path, normalize_font_size_tiers, normalize_project_icon_relative_path,
    normalize_terminal_auto_continue_interval_seconds, normalize_terminal_fab_action_color,
    normalize_terminal_fab_action_opacity, normalize_terminal_floating_button_offset_vh,
    normalize_terminal_soft_keyboard_scale, normalize_terminal_touch_selection_long_press_ms,
    platform_workspace_root_limit, resolve_workspace_dir, sanitize_claude_default_config_entries,
    sanitize_claude_model_options, sanitize_codex_api_auto_proxy_match_provider_ids,
    sanitize_codex_config_key, sanitize_codex_config_value, sanitize_codex_default_config_entries,
    sanitize_codex_secondary_config_key, sanitize_codex_secondary_config_value,
    sanitize_compile_environment, sanitize_favorite_paths, sanitize_terminal_default_env_vars,
    sanitize_terminal_error_keyword_actions, sanitize_terminal_function_commands,
    sanitize_terminal_quick_commands, sanitize_terminal_quick_start_default_key,
    sanitize_terminal_rename_presets, sanitize_workspace_history, validate_terminal_tool_entries,
};
use std::path::{Path, PathBuf};

#[test]
fn compile_max_concurrency_defaults_to_five_and_is_clamped() {
    assert_eq!(DEFAULT_COMPILE_MAX_CONCURRENCY, 5);
    assert_eq!(normalize_compile_max_concurrency(0), 1);
    assert_eq!(normalize_compile_max_concurrency(5), 5);
    assert_eq!(normalize_compile_max_concurrency(100), 32);
}

#[test]
fn accept_home_scope_workspace_dirs() {
    let resolved = resolve_workspace_dir("/home").expect("/home should be accepted");
    assert!(is_within_workspace_limit(&resolved.canonical));
    assert_eq!(resolved.display, Path::new("/home"));
}

#[test]
fn accept_existing_workspace_dirs_outside_legacy_home_scope() {
    let candidate = std::env::temp_dir();
    let resolved = resolve_workspace_dir(&candidate.display().to_string())
        .expect("existing absolute directories should be accepted");

    assert_eq!(resolved.canonical, candidate.canonicalize().unwrap());
    assert_eq!(resolved.display, candidate);
}

#[test]
fn reject_relative_workspace_dirs() {
    let error = resolve_workspace_dir("tmp").expect_err("relative paths should be rejected");
    assert!(error.to_string().contains("绝对路径"), "unexpected error: {error}");
}

#[test]
fn workspace_limit_matches_home_prefix() {
    assert!(is_within_workspace_limit(Path::new("/home")));
    assert!(is_within_workspace_limit(Path::new("/home/beyondcy")));
    assert!(!is_within_workspace_limit(Path::new("/root")));
    assert!(!is_within_workspace_limit(Path::new("/tmp/test")));
}

#[test]
fn platform_workspace_root_limit_uses_windows_user_home_when_available() {
    let root = platform_workspace_root_limit(Some(PathBuf::from(r"C:\Users\alice")));

    if cfg!(windows) {
        assert_eq!(root, PathBuf::from(r"C:\Users\alice"));
    } else {
        assert_eq!(root, PathBuf::from("/home"));
    }
}

#[test]
fn built_in_default_workspace_candidates_prefer_project_then_user_then_home() {
    let candidates = default_workspace_dir_candidates();

    assert_eq!(candidates.first(), Some(&PathBuf::from("/home/codes")));
    assert!(candidates.contains(&PathBuf::from("/home")));
    assert_eq!(
        candidates
            .iter()
            .filter(|candidate| *candidate == &PathBuf::from("/home"))
            .count(),
        1
    );
}

#[test]
fn default_show_full_path_is_true() {
    assert!(super::default_show_full_path());
}

#[test]
fn normalize_favorite_path_keeps_home_scope() {
    assert_eq!(normalize_favorite_path("/home/workspaces-src/../docs").unwrap(), "/home/docs");
}

#[test]
fn reject_favorite_path_outside_home() {
    let error = normalize_favorite_path("/tmp/test").expect_err("/tmp should be rejected");
    assert!(error.to_string().contains("`/home`"), "unexpected error: {error}");
}

#[test]
fn sanitize_favorite_paths_deduplicates_by_path() {
    let favorites = sanitize_favorite_paths(&[
        FavoritePath {
            path: "/home/workspaces-src/src".into(),
            kind: FavoritePathKind::Dir,
        },
        FavoritePath {
            path: "/home/workspaces-src/src/.".into(),
            kind: FavoritePathKind::Dir,
        },
    ])
    .unwrap();

    assert_eq!(favorites.len(), 1);
    assert_eq!(favorites[0].path, "/home/workspaces-src/src");
}

#[test]
fn sanitize_workspace_history_normalizes_sorts_and_deduplicates() {
    let history = sanitize_workspace_history(&[
        WorkspaceHistoryItem {
            path: "/home/workspaces-src/src".into(),
            last_opened_at: 10,
        },
        WorkspaceHistoryItem {
            path: "/home/workspaces-src/src/.".into(),
            last_opened_at: 30,
        },
        WorkspaceHistoryItem {
            path: "/home/workspaces-src/docs".into(),
            last_opened_at: 20,
        },
        WorkspaceHistoryItem {
            path: "/tmp/outside".into(),
            last_opened_at: 40,
        },
    ]);

    assert_eq!(history.len(), 3);
    assert_eq!(history[0].path, "/tmp/outside");
    assert_eq!(history[0].last_opened_at, 40);
    assert_eq!(history[1].path, "/home/workspaces-src/src");
    assert_eq!(history[1].last_opened_at, 30);
    assert_eq!(history[2].path, "/home/workspaces-src/docs");
}

#[test]
fn sanitize_claude_model_options_trims_and_deduplicates() {
    let options = sanitize_claude_model_options(&[
        " claude-sonnet-4-6 ".to_string(),
        "".to_string(),
        "GLM-5.1".to_string(),
        "claude-sonnet-4-6".to_string(),
    ]);

    assert_eq!(options, vec!["claude-sonnet-4-6", "GLM-5.1"]);
}

#[test]
fn terminal_quick_commands_default_to_codex_and_claude() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.terminal_quick_commands.len(), 2);
    assert_eq!(parsed.terminal_quick_commands[0].key, "1");
    assert_eq!(parsed.terminal_quick_commands[0].command, "codex");
    assert_eq!(
        parsed.terminal_quick_start_default_key,
        DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY
    );
}

#[test]
fn terminal_function_commands_default_to_keyboard_and_slash_commands() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.terminal_slash_commands.len(), 13);
    assert!(
        !parsed
            .terminal_slash_commands
            .iter()
            .any(|command| { command.key == "webui" || command.action == "open_project_url" })
    );
    assert!(parsed.terminal_slash_commands.iter().any(|command| {
        command.key == "copy_id_and_ask"
            && command.label == "复制id并提问"
            && command.action == "copy_id_and_ask"
            && command.command.is_empty()
    }));
    let first_slash = parsed
        .terminal_slash_commands
        .iter()
        .position(|command| command.command.starts_with('/'))
        .expect("slash commands should exist");
    assert!(
        parsed.terminal_slash_commands[first_slash..]
            .iter()
            .all(|command| command.command.starts_with('/'))
    );
    assert_eq!(
        parsed
            .terminal_slash_commands
            .iter()
            .map(|command| command.key.as_str())
            .collect::<Vec<_>>(),
        vec![
            "resume_current_session",
            "continue",
            "enter",
            "extract_resume",
            "copy_resume_id",
            "extract_current_session",
            "current_resume_id",
            "copy_id_and_ask",
            "copy_terminal_name",
            "resume",
            "status",
            "fork",
            "compact",
        ]
    );
    assert!(
        !parsed
            .terminal_slash_commands
            .iter()
            .any(|command| command.key == "quota" || command.action == "open_quota_dialog")
    );
    assert_eq!(parsed.terminal_function_commands.len(), 10);
    assert_eq!(parsed.terminal_function_commands[0].action, "show_system_keyboard");
    assert_eq!(parsed.terminal_function_commands[1].action, "disable_system_keyboard");
    assert_eq!(parsed.terminal_function_commands[2].action, "copy_terminal_name");
    assert_eq!(parsed.terminal_function_commands[3].action, "copy_terminal_view_in_new_window");
    assert_eq!(parsed.terminal_function_commands[4].key, "reload_claude");
    assert_eq!(parsed.terminal_function_commands[5].action, "toggle_terminal_width");
    assert_eq!(parsed.terminal_function_commands[6].action, "save_and_poweroff");
    assert_eq!(parsed.terminal_function_commands[7].action, "save_and_restart");
    assert_eq!(parsed.terminal_function_commands[8].key, "qoderclicn");
    assert_eq!(parsed.terminal_function_commands[9].key, "agy");
    assert!(
        parsed
            .terminal_function_commands
            .iter()
            .all(|command| command.key != "current_resume_id" && command.key != "enter")
    );
}

#[test]
fn terminal_rename_presets_default_to_common_suffixes() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.terminal_rename_presets, vec!["完结", "复用对话"]);
}

#[test]
fn terminal_activity_agent_display_defaults_hidden_and_accepts_suffix() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");
    assert_eq!(parsed.terminal_activity_agent_display, TerminalActivityAgentDisplay::Hidden);

    let parsed: SettingsFile = serde_json::from_str(
        r#"{"workspace_dir":"/home/codes","terminal_activity_agent_display":"suffix"}"#,
    )
    .expect("parse settings");
    assert_eq!(parsed.terminal_activity_agent_display, TerminalActivityAgentDisplay::Suffix);
}

#[test]
fn terminal_auto_continue_defaults_to_minute_cooldown_and_respects_manual_interrupt() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.terminal_auto_continue_interval_seconds, 60);
    assert!(parsed.terminal_auto_continue_respect_manual_interrupt);

    let parsed: SettingsFile = serde_json::from_str(
        r#"{"workspace_dir":"/home/codes","terminal_auto_continue_interval_seconds":5,"terminal_auto_continue_respect_manual_interrupt":false}"#,
    )
    .expect("parse settings");

    assert_eq!(parsed.terminal_auto_continue_interval_seconds, 5);
    assert!(!parsed.terminal_auto_continue_respect_manual_interrupt);
    assert_eq!(normalize_terminal_auto_continue_interval_seconds(0), 60);
    assert_eq!(normalize_terminal_auto_continue_interval_seconds(1), 1);
    assert_eq!(normalize_terminal_auto_continue_interval_seconds(100_000), 86_400);
}

#[test]
fn terminal_error_keywords_include_quota_429_rejection_variants() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert!(
        parsed
            .terminal_error_keywords
            .iter()
            .any(|keyword| keyword == "API Error: Request rejected (429)")
    );
    assert!(
        parsed
            .terminal_error_keywords
            .iter()
            .any(|keyword| keyword.contains("已达到 5 小时的使用上限"))
    );
}

#[test]
fn terminal_error_keywords_include_generic_stream_disconnect_prefix() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert!(
        parsed
            .terminal_error_keywords
            .iter()
            .any(|keyword| keyword == "stream disconnected before completion:")
    );
}

#[test]
fn terminal_error_keywords_exclude_nonfatal_mcp_startup_failure_summary() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert!(
        parsed
            .terminal_error_keywords
            .iter()
            .all(|keyword| keyword != "MCP startup incomplete")
    );
}

#[test]
fn terminal_error_keywords_include_context_window_exhausted_variant() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert!(
        parsed
            .terminal_error_keywords
            .iter()
            .any(|keyword| keyword == CONTEXT_WINDOW_EXHAUSTED_ERROR_KEYWORD)
    );
}

#[test]
fn terminal_error_keywords_include_openai_cybersecurity_block_signatures() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert!(
        parsed
            .terminal_error_keywords
            .iter()
            .any(|keyword| keyword == OPENAI_CYBERSECURITY_BLOCK_TITLE_KEYWORD)
    );
    assert!(
        parsed
            .terminal_error_keywords
            .iter()
            .any(|keyword| keyword == OPENAI_CYBERSECURITY_BLOCK_PHRASE_KEYWORD)
    );
}

#[test]
fn sanitize_terminal_rename_presets_trims_deduplicates_and_drops_invalid() {
    let presets = sanitize_terminal_rename_presets(&[
        " 完结 ".to_string(),
        "".to_string(),
        "复用对话".to_string(),
        "完结".to_string(),
        "bad\u{0007}name".to_string(),
    ]);

    assert_eq!(presets, vec!["完结", "复用对话"]);
}

#[test]
fn sanitize_terminal_function_commands_trims_deduplicates_and_drops_invalid() {
    let commands = sanitize_terminal_function_commands(&[
        TerminalFunctionCommand {
            key: " keyboard ".into(),
            label: " 弹出 ".into(),
            action: " show_system_keyboard ".into(),
            command: String::new(),
            shortcut: " Ctrl+Shift+K ".into(),
        },
        TerminalFunctionCommand {
            key: "keyboard".into(),
            label: "Duplicate".into(),
            action: "disable_system_keyboard".into(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "bad key".into(),
            label: "Bad".into(),
            action: "show_system_keyboard".into(),
            command: String::new(),
            shortcut: String::new(),
        },
        TerminalFunctionCommand {
            key: "resume".into(),
            label: "Resume".into(),
            action: "send_slash_command".into(),
            command: "/resume".into(),
            shortcut: "bad\u{0007}shortcut".into(),
        },
    ]);

    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].key, "keyboard");
    assert_eq!(commands[0].label, "弹出");
    assert_eq!(commands[0].action, "show_system_keyboard");
    assert_eq!(commands[0].shortcut, "Ctrl+Shift+K");
    assert_eq!(commands[1].command, "/resume");
    assert!(commands[1].shortcut.is_empty());

    let insert_only = sanitize_terminal_function_commands(&[TerminalFunctionCommand {
        key: "codes_backup".into(),
        label: String::new(),
        action: "insert_text".into(),
        command: " !codes_backup ".into(),
        shortcut: String::new(),
    }]);
    assert_eq!(insert_only.len(), 1);
    assert_eq!(insert_only[0].label, "!codes_backup");
    assert_eq!(insert_only[0].command, "!codes_backup ");
}

#[test]
fn sanitize_terminal_quick_commands_trims_deduplicates_and_drops_invalid() {
    let commands = sanitize_terminal_quick_commands(&[
        TerminalQuickCommand {
            key: " 1 ".into(),
            label: " Codex ".into(),
            command: " codex --search ".into(),
            program: String::new(),
            args: String::new(),
        },
        TerminalQuickCommand {
            key: "1".into(),
            label: "Duplicate".into(),
            command: "claude".into(),
            program: "claude".into(),
            args: String::new(),
        },
        TerminalQuickCommand {
            key: "two words".into(),
            label: "Invalid".into(),
            command: "claude".into(),
            program: "claude".into(),
            args: String::new(),
        },
        TerminalQuickCommand {
            key: "2".into(),
            label: "Bad program".into(),
            command: "bad\u{0007}command".into(),
            program: "bad command".into(),
            args: String::new(),
        },
    ]);

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].key, "1");
    assert_eq!(commands[0].label, "Codex");
    assert_eq!(commands[0].command, "codex --search");
    assert!(commands[0].program.is_empty());
    assert!(commands[0].args.is_empty());
}

#[test]
fn terminal_quick_start_default_key_must_match_command() {
    let commands = sanitize_terminal_quick_commands(&[TerminalQuickCommand {
        key: "x".into(),
        label: "Tool".into(),
        command: "tool --flag".into(),
        program: "tool".into(),
        args: String::new(),
    }]);

    assert_eq!(sanitize_terminal_quick_start_default_key("x", &commands), "x");
    assert_eq!(sanitize_terminal_quick_start_default_key("1", &commands), "");
    assert_eq!(sanitize_terminal_quick_start_default_key("", &commands), "");
}

#[test]
fn sanitize_terminal_quick_commands_accepts_legacy_program_args() {
    let commands = sanitize_terminal_quick_commands(&[TerminalQuickCommand {
        key: "1".into(),
        label: String::new(),
        command: String::new(),
        program: " codex ".into(),
        args: " --search ".into(),
    }]);

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].label, "codex --search");
    assert_eq!(commands[0].command, "codex --search");
}

#[test]
fn sanitize_terminal_default_env_vars_drops_reserved_and_invalid_keys() {
    let vars = sanitize_terminal_default_env_vars(&[
        TerminalDefaultEnvVar {
            key: " OPENAI_BASE_URL ".into(),
            value: " https://api.example.com ".into(),
        },
        TerminalDefaultEnvVar {
            key: "OPENAI_BASE_URL".into(),
            value: "duplicate".into(),
        },
        TerminalDefaultEnvVar {
            key: "HOME".into(),
            value: "/tmp".into(),
        },
        TerminalDefaultEnvVar {
            key: "bad-key".into(),
            value: "x".into(),
        },
    ]);

    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].key, "OPENAI_BASE_URL");
    assert_eq!(vars[0].value, "https://api.example.com");
}

#[test]
fn sanitize_codex_config_key_uses_default_when_blank_or_invalid() {
    assert_eq!(sanitize_codex_config_key("   "), DEFAULT_CODEX_CONFIG_KEY);
    assert_eq!(sanitize_codex_config_key("model"), "model");
    assert_eq!(sanitize_codex_config_key("model-name"), "model-name");
    assert_eq!(sanitize_codex_config_key("features.goals"), "features.goals");
    assert_eq!(sanitize_codex_config_key("model name"), DEFAULT_CODEX_CONFIG_KEY);
}

#[test]
fn sanitize_codex_config_value_allows_blank_default() {
    assert_eq!(sanitize_codex_config_value("   "), "");
    assert_eq!(sanitize_codex_config_value(" glm-4.7 "), "glm-4.7");
}

#[test]
fn sanitize_codex_secondary_config_key_uses_default_when_blank_or_invalid() {
    assert_eq!(sanitize_codex_secondary_config_key("   "), DEFAULT_CODEX_SECONDARY_CONFIG_KEY);
    assert_eq!(
        sanitize_codex_secondary_config_key("model_reasoning_effort"),
        "model_reasoning_effort"
    );
    assert_eq!(
        sanitize_codex_secondary_config_key("reasoning level"),
        DEFAULT_CODEX_SECONDARY_CONFIG_KEY
    );
}

#[test]
fn sanitize_codex_secondary_config_value_uses_default_when_blank() {
    assert_eq!(DEFAULT_CODEX_SECONDARY_CONFIG_VALUE, "high");
    assert_eq!(
        sanitize_codex_secondary_config_value("   "),
        DEFAULT_CODEX_SECONDARY_CONFIG_VALUE
    );
    assert_eq!(sanitize_codex_secondary_config_value(" high "), "high");
}

#[test]
fn sanitize_codex_default_config_entries_keeps_added_rows() {
    let entries = sanitize_codex_default_config_entries(&[
        CodexDefaultConfigEntry {
            key: " model ".to_string(),
            value: " gpt-5.4 ".to_string(),
        },
        CodexDefaultConfigEntry {
            key: "features.goals".to_string(),
            value: "true".to_string(),
        },
    ]);

    assert_eq!(
        entries,
        vec![
            CodexDefaultConfigEntry {
                key: "model".to_string(),
                value: "gpt-5.4".to_string(),
            },
            CodexDefaultConfigEntry {
                key: "features.goals".to_string(),
                value: "true".to_string(),
            },
        ]
    );
}

#[test]
fn sanitize_codex_default_config_entries_excludes_provider_owned_keys() {
    let entries = sanitize_codex_default_config_entries(&[
        CodexDefaultConfigEntry {
            key: "model_provider".to_string(),
            value: "webclx_api".to_string(),
        },
        CodexDefaultConfigEntry {
            key: "model_providers.webclx_api".to_string(),
            value: "ignored".to_string(),
        },
        CodexDefaultConfigEntry {
            key: "features.goals".to_string(),
            value: "true".to_string(),
        },
    ]);

    assert_eq!(
        entries,
        vec![CodexDefaultConfigEntry {
            key: "features.goals".to_string(),
            value: "true".to_string(),
        }]
    );
}

#[test]
fn sanitize_claude_default_config_entries_keeps_model_rows_and_deduplicates_keys() {
    let entries = sanitize_claude_default_config_entries(&[
        CodexDefaultConfigEntry {
            key: " ANTHROPIC_DEFAULT_SONNET_MODEL ".to_string(),
            value: " claude-sonnet-4-6 ".to_string(),
        },
        CodexDefaultConfigEntry {
            key: "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
            value: "override".to_string(),
        },
        CodexDefaultConfigEntry {
            key: "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
            value: "1".to_string(),
        },
    ]);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key, "ANTHROPIC_DEFAULT_SONNET_MODEL");
    assert_eq!(entries[0].value, "override");
    assert_eq!(entries[1].key, "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC");
    assert_eq!(entries[1].value, "1");
}

#[tokio::test]
async fn partial_claude_defaults_save_preserves_codex_default_entries() {
    let config_dir = std::env::temp_dir().join(format!(
        "webclx-settings-claude-defaults-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos()
    ));
    std::fs::create_dir_all(&config_dir).expect("temp dir should be created");
    let manager = super::SettingsManager::load(&config_dir).expect("settings manager should load");

    let codex_request = serde_json::from_value(serde_json::json!({
        "codex_default_config_entries": [
            {"key": "model", "value": "gpt-test"},
            {"key": "model_reasoning_effort", "value": "high"},
            {"key": "features.goals", "value": "true"}
        ]
    }))
    .expect("Codex request should deserialize");
    super::save_settings(&manager, codex_request)
        .await
        .expect("Codex defaults should save");

    let claude_request = serde_json::from_value(serde_json::json!({
        "claude_default_config_entries": [
            {"key": "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "value": "1"}
        ]
    }))
    .expect("Claude request should deserialize");
    let response = super::save_settings(&manager, claude_request)
        .await
        .expect("Claude defaults should save independently");

    assert_eq!(response.codex_default_config_entries.len(), 3);
    assert_eq!(response.codex_default_config_entries[2].key, "features.goals");
    assert_eq!(response.claude_default_config_entries.len(), 1);
    assert_eq!(
        manager.claude_default_config_entries()[0].key,
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"
    );

    std::fs::remove_dir_all(config_dir).ok();
}

#[test]
fn sanitize_codex_api_auto_proxy_match_provider_ids_filters_known_values() {
    let values = vec![
        " zhipu ".to_string(),
        "unknown".to_string(),
        "DeepSeek".to_string(),
        "zhipu".to_string(),
        "minimax".to_string(),
    ];

    assert_eq!(
        sanitize_codex_api_auto_proxy_match_provider_ids(&values),
        vec![
            "zhipu".to_string(),
            "deepseek".to_string(),
            "minimax".to_string(),
        ]
    );
}

#[test]
fn normalize_font_size_tiers_clamps_and_preserves_order() {
    let normalized = normalize_font_size_tiers([0.83, 0.6, 2.0, 0.739]);

    assert_eq!(normalized, [0.83, 0.83, 1.0, 1.0]);
}

#[test]
fn normalize_font_size_tiers_uses_defaults_for_invalid_values() {
    let normalized = normalize_font_size_tiers([f32::NAN, f32::INFINITY, 0.1, 0.73]);

    assert_eq!(
        normalized,
        [
            DEFAULT_FONT_SIZE_TIER_1,
            DEFAULT_FONT_SIZE_TIER_2,
            0.68,
            0.73
        ]
    );
}

#[test]
fn dot_entries_default_to_hidden() {
    assert!(!default_show_dot_entries());
}

#[test]
fn workspace_session_dropdown_defaults_to_all_directories() {
    assert!(default_show_all_workspace_sessions());
}

#[test]
fn desktop_terminal_soft_keyboard_defaults_to_enabled() {
    assert!(default_desktop_terminal_soft_keyboard_enabled());
}

#[test]
fn terminal_floating_button_offset_defaults_to_lower_stack_anchor() {
    assert_eq!(
        default_terminal_floating_button_offset_vh(),
        DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH
    );

    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(
        parsed.terminal_floating_button_offset_vh,
        DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH
    );
}

#[test]
fn terminal_fab_action_appearance_defaults_to_orange_at_half_opacity() {
    assert_eq!(default_terminal_fab_action_color(), DEFAULT_TERMINAL_FAB_ACTION_COLOR);
    assert_eq!(default_terminal_fab_action_opacity(), DEFAULT_TERMINAL_FAB_ACTION_OPACITY);

    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.terminal_fab_action_color, "#f59e0b");
    assert_eq!(parsed.terminal_fab_action_opacity, 0.5);
}

#[test]
fn workspace_icon_paths_default_to_separate_project_relative_locations() {
    assert_eq!(default_workspace_browser_icon_path(), "icon.ico");
    assert_eq!(default_terminal_workspace_icon_path(), "static/favicon.svg");

    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.workspace_browser_icon_path, "icon.ico");
    assert_eq!(parsed.terminal_workspace_icon_path, "static/favicon.svg");
}

#[test]
fn project_icon_relative_path_normalization_blocks_project_escape() {
    assert_eq!(
        normalize_project_icon_relative_path(" assets\\project icon.png ", "icon.ico"),
        "assets/project icon.png"
    );
    assert_eq!(normalize_project_icon_relative_path("/etc/passwd", "icon.ico"), "icon.ico");
    assert_eq!(
        normalize_project_icon_relative_path("../outside/icon.ico", "icon.ico"),
        "icon.ico"
    );
    assert_eq!(normalize_project_icon_relative_path("", "icon.ico"), "icon.ico");
}

#[test]
fn workspace_icon_paths_load_respond_and_persist_independently() {
    let config_dir = std::env::temp_dir()
        .join(format!("webclx-settings-workspace-icons-{}", std::process::id()));
    std::fs::remove_dir_all(&config_dir).ok();
    std::fs::create_dir_all(&config_dir).expect("temp dir should be created");
    let config_path = config_dir.join("webclx-settings.json");
    std::fs::write(
        &config_path,
        r#"{
            "workspace_dir": "/home/codes",
            "workspace_browser_icon_path": "assets/project.ico",
            "terminal_workspace_icon_path": "web/favicon.png"
        }"#,
    )
    .expect("write test settings");

    let manager = super::SettingsManager::load(&config_dir).expect("settings manager should load");
    assert_eq!(manager.workspace_browser_icon_path(), "assets/project.ico");
    assert_eq!(manager.terminal_workspace_icon_path(), "web/favicon.png");

    let response = super::build_settings_response(
        &manager,
        "test-host".to_string(),
        "127.0.0.1:3000".to_string(),
        "test-version".to_string(),
    )
    .expect("settings response should build");
    assert_eq!(response.workspace_browser_icon_path, "assets/project.ico");
    assert_eq!(response.terminal_workspace_icon_path, "web/favicon.png");

    let persisted: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&config_path).expect("persisted settings should be readable"),
    )
    .expect("persisted settings should be valid JSON");
    assert_eq!(persisted["workspace_browser_icon_path"], "assets/project.ico");
    assert_eq!(persisted["terminal_workspace_icon_path"], "web/favicon.png");

    std::fs::remove_dir_all(config_dir).ok();
}

#[test]
fn terminal_soft_keyboard_scale_defaults_to_slightly_larger_than_before() {
    assert_eq!(DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE, 1.08);

    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.terminal_soft_keyboard_scale, DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE);
}

#[test]
fn terminal_touch_selection_long_press_defaults_to_two_seconds() {
    assert_eq!(
        default_terminal_touch_selection_long_press_ms(),
        DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS
    );
    assert_eq!(DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS, 2000);

    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(
        parsed.terminal_touch_selection_long_press_ms,
        DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS
    );
}

#[test]
fn normalize_terminal_soft_keyboard_scale_clamps_and_rounds() {
    assert_eq!(normalize_terminal_soft_keyboard_scale(0.5), 0.9);
    assert_eq!(normalize_terminal_soft_keyboard_scale(1.123), 1.12);
    assert_eq!(normalize_terminal_soft_keyboard_scale(1.8), 1.3);
    assert_eq!(
        normalize_terminal_soft_keyboard_scale(f32::NAN),
        DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE
    );
}

#[test]
fn normalize_terminal_floating_button_offset_vh_clamps_and_rounds() {
    assert_eq!(normalize_terminal_floating_button_offset_vh(4.0), 12.0);
    assert_eq!(normalize_terminal_floating_button_offset_vh(33.333), 33.3);
    assert_eq!(normalize_terminal_floating_button_offset_vh(80.0), 60.0);
    assert_eq!(
        normalize_terminal_floating_button_offset_vh(f32::NAN),
        DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH
    );
}

#[test]
fn terminal_fab_action_appearance_normalizes_color_and_opacity() {
    assert_eq!(normalize_terminal_fab_action_color(" #F97316 "), "#f97316");
    assert_eq!(normalize_terminal_fab_action_color("orange"), "#f59e0b");
    assert_eq!(normalize_terminal_fab_action_color("#abcd"), "#f59e0b");
    assert_eq!(normalize_terminal_fab_action_opacity(0.04), 0.1);
    assert_eq!(normalize_terminal_fab_action_opacity(0.527), 0.53);
    assert_eq!(normalize_terminal_fab_action_opacity(2.0), 1.0);
    assert_eq!(
        normalize_terminal_fab_action_opacity(f32::NAN),
        DEFAULT_TERMINAL_FAB_ACTION_OPACITY
    );
}

#[test]
fn normalize_terminal_touch_selection_long_press_clamps_to_safe_range() {
    assert_eq!(normalize_terminal_touch_selection_long_press_ms(500), 2000);
    assert_eq!(normalize_terminal_touch_selection_long_press_ms(2500), 2500);
    assert_eq!(normalize_terminal_touch_selection_long_press_ms(20000), 10000);
}

#[test]
fn terminal_scrollback_lines_default_and_clamp_to_safe_range() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.terminal_scrollback_lines, super::DEFAULT_TERMINAL_SCROLLBACK_LINES);
    assert_eq!(super::normalize_terminal_scrollback_lines(1), 100);
    assert_eq!(super::normalize_terminal_scrollback_lines(25_000), 25_000);
    assert_eq!(super::normalize_terminal_scrollback_lines(200_000), 100_000);
}

#[test]
fn terminal_scrollback_lines_load_response_and_persistence_are_normalized() {
    let config_dir = std::env::temp_dir()
        .join(format!("webclx-settings-terminal-scrollback-{}", std::process::id()));
    std::fs::remove_dir_all(&config_dir).ok();
    std::fs::create_dir_all(&config_dir).expect("temp dir should be created");
    let config_path = config_dir.join("webclx-settings.json");
    std::fs::write(
        &config_path,
        r#"{"workspace_dir":"/home/codes","terminal_scrollback_lines":42}"#,
    )
    .expect("write test settings");

    let manager = super::SettingsManager::load(&config_dir).expect("settings manager should load");
    assert_eq!(manager.terminal_scrollback_lines(), 100);

    let response = super::build_settings_response(
        &manager,
        "test-host".to_string(),
        "127.0.0.1:3000".to_string(),
        "test-version".to_string(),
    )
    .expect("settings response should build");
    assert_eq!(response.terminal_scrollback_lines, 100);

    let persisted: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&config_path).expect("persisted settings should be readable"),
    )
    .expect("persisted settings should be valid JSON");
    assert_eq!(persisted["terminal_scrollback_lines"], 100);

    std::fs::remove_dir_all(config_dir).ok();
}

#[test]
fn theme_defaults_to_system() {
    assert_eq!(default_theme_mode(), ThemeMode::System);
}

#[test]
fn settings_file_defaults_theme_mode_when_missing() {
    let parsed: SettingsFile =
        serde_json::from_str(r#"{"workspace_dir":"/home/codes"}"#).expect("parse settings");

    assert_eq!(parsed.theme_mode, ThemeMode::System);
    assert!(parsed.desktop_terminal_soft_keyboard_enabled);
    assert_eq!(
        parsed.terminal_floating_button_offset_vh,
        DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH
    );
    assert_eq!(
        parsed.terminal_touch_selection_long_press_ms,
        DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS
    );
    assert!(parsed.workspace_history.is_empty());
}

#[test]
fn load_saved_settings_preserves_values_when_workspace_is_invalid() {
    let config_path = std::env::temp_dir()
        .join(format!("webclx-settings-invalid-workspace-{}.json", std::process::id()));
    std::fs::write(
        &config_path,
        r#"{
            "workspace_dir": "/tmp/outside-webclx-scope",
            "show_dot_entries": false,
            "theme_mode": "dark",
            "codex_config_value": "glm-4.7",
            "show_full_path": false
        }"#,
    )
    .expect("write test settings");

    let loaded = load_saved_settings(&config_path)
        .expect("load settings")
        .expect("settings should exist");

    let _ = std::fs::remove_file(&config_path);

    assert!(is_within_workspace_limit(&loaded.workspace_dir));
    assert!(!loaded.show_dot_entries);
    assert_eq!(loaded.theme_mode, ThemeMode::Dark);
    assert_eq!(loaded.codex_config_value, "glm-4.7");
    assert!(!loaded.show_full_path);
}

#[cfg(unix)]
#[test]
fn settings_manager_secures_runtime_settings_file() {
    use std::os::unix::fs::PermissionsExt;

    let config_dir =
        std::env::temp_dir().join(format!("webclx-settings-permissions-{}", std::process::id()));
    std::fs::create_dir_all(&config_dir).expect("temp dir should be created");

    super::SettingsManager::load(&config_dir).expect("settings manager should load");

    let config_path = config_dir.join("webclx-settings.json");
    let mode = std::fs::metadata(&config_path)
        .expect("settings file should exist")
        .permissions()
        .mode()
        & 0o777;
    std::fs::remove_dir_all(&config_dir).ok();

    assert_eq!(mode, 0o600);
}

#[test]
fn load_saved_settings_migrates_legacy_path_display_prefix_to_show_full_path() {
    let config_path = std::env::temp_dir()
        .join(format!("webclx-settings-legacy-prefix-{}.json", std::process::id()));
    // Legacy config with a non-default path_display_prefix should migrate to
    // show_full_path = false (user wanted abbreviated paths).
    std::fs::write(
        &config_path,
        r#"{
            "workspace_dir": "/home/codes",
            "path_display_prefix": "/custom/prefix"
        }"#,
    )
    .expect("write test settings");

    let loaded = load_saved_settings(&config_path)
        .expect("load settings")
        .expect("settings should exist");

    let _ = std::fs::remove_file(&config_path);

    assert!(!loaded.show_full_path);
}

#[test]
fn load_saved_settings_migrates_default_path_display_prefix_to_show_full_path_true() {
    let config_path = std::env::temp_dir()
        .join(format!("webclx-settings-default-prefix-{}.json", std::process::id()));
    // Legacy config where path_display_prefix equals the home dir should
    // migrate to show_full_path = true (full path already shown).
    let home = super::runtime_paths::resolve_current_user_home()
        .map(|h| h.display().to_string())
        .unwrap_or("/home".to_string());
    std::fs::write(
        &config_path,
        &format!(r#"{{"workspace_dir": "/home/codes", "path_display_prefix": "{}"}}"#, home),
    )
    .expect("write test settings");

    let loaded = load_saved_settings(&config_path)
        .expect("load settings")
        .expect("settings should exist");

    let _ = std::fs::remove_file(&config_path);

    assert!(loaded.show_full_path);
}

#[test]
fn load_saved_settings_merges_new_builtin_terminal_error_keywords() {
    let config_path = std::env::temp_dir()
        .join(format!("webclx-settings-terminal-error-keywords-{}.json", std::process::id()));
    std::fs::write(
        &config_path,
        r#"{
            "workspace_dir": "/home/codes",
            "terminal_error_keywords": [
                "custom upstream outage",
                "stream disconnected before completion: error",
                "Concurrency limit exceeded for user, please retry later"
            ]
        }"#,
    )
    .expect("write test settings");

    let loaded = load_saved_settings(&config_path)
        .expect("load settings")
        .expect("settings should exist");

    let _ = std::fs::remove_file(&config_path);

    assert!(
        loaded
            .terminal_error_keywords
            .iter()
            .any(|keyword| keyword == "custom upstream outage")
    );
    assert!(
        loaded.terminal_error_keywords.iter().any(
            |keyword| keyword == "Selected model is at capacity. Please try a different model."
        )
    );
    assert!(
        loaded
            .terminal_error_keywords
            .iter()
            .any(|keyword| keyword == "stream disconnected before completion:")
    );
    assert!(
        loaded
            .terminal_error_keywords
            .iter()
            .all(|keyword| keyword != "MCP startup incomplete")
    );
    assert!(
        loaded
            .terminal_error_keyword_actions
            .iter()
            .all(|action| action.keyword != "MCP startup incomplete")
    );
}

#[test]
fn sanitize_compile_environment_dedupes_and_filters_invalid_keys() {
    let vars = vec![
        CompileEnvVar {
            key: "PATH".to_string(),
            value: "/home/root/.cargo/bin".to_string(),
        },
        CompileEnvVar {
            key: "path".to_string(),
            value: "/dup".to_string(),
        },
        CompileEnvVar {
            key: "1BAD".to_string(),
            value: "x".to_string(),
        },
        CompileEnvVar {
            key: "CARGO_HOME".to_string(),
            value: "/home/root/.cargo".to_string(),
        },
        CompileEnvVar {
            key: "  RUSTUP_HOME  ".to_string(),
            value: "  /home/root/.rustup  ".to_string(),
        },
    ];
    // "PATH" and "path" are distinct keys (case-sensitive dedup, like env vars).
    // "1BAD" is rejected (invalid first char). Trailing/leading spaces are trimmed.
    let result = sanitize_compile_environment(&vars);
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].key, "PATH");
    assert_eq!(result[0].value, "/home/root/.cargo/bin");
    assert_eq!(result[1].key, "path");
    assert_eq!(result[1].value, "/dup");
    assert_eq!(result[2].key, "CARGO_HOME");
    assert_eq!(result[3].key, "RUSTUP_HOME");
    assert_eq!(result[3].value, "/home/root/.rustup");
}

#[test]
fn sanitize_compile_environment_defaults_empty() {
    assert!(sanitize_compile_environment(&[]).is_empty());
}

fn terminal_tool_entry(
    id: &str,
    parent_id: Option<&str>,
    kind: &str,
    actions: Vec<TerminalToolAction>,
) -> TerminalToolEntry {
    TerminalToolEntry {
        id: id.to_string(),
        root_key: "tools".to_string(),
        parent_id: parent_id.map(str::to_string),
        kind: kind.to_string(),
        label: id.to_string(),
        sort_order: 10,
        actions,
    }
}

#[test]
fn validate_terminal_tool_entries_accepts_nested_typed_actions() {
    let entries = vec![
        terminal_tool_entry("folder", None, "folder", vec![]),
        terminal_tool_entry(
            "workflow",
            Some("folder"),
            "action",
            vec![
                TerminalToolAction {
                    kind: "switch_api_preset".to_string(),
                    value: "  preset-id  ".to_string(),
                    seconds: 0.0,
                    ..Default::default()
                },
                TerminalToolAction {
                    kind: "wait".to_string(),
                    value: String::new(),
                    seconds: 1.5,
                    ..Default::default()
                },
                TerminalToolAction {
                    kind: "create_terminal".to_string(),
                    value: "ignored".to_string(),
                    seconds: 0.0,
                    ..Default::default()
                },
                TerminalToolAction {
                    kind: "codex_terminal".to_string(),
                    value: "检查项目并汇报".to_string(),
                    seconds: 0.0,
                    ..Default::default()
                },
            ],
        ),
    ];

    let validated = validate_terminal_tool_entries(&entries).expect("valid tool tree");

    assert_eq!(validated.len(), 2);
    assert_eq!(validated[1].parent_id.as_deref(), Some("folder"));
    assert_eq!(validated[1].actions[0].value, "preset-id");
    assert!(validated[1].actions[2].value.is_empty());
    assert_eq!(validated[1].actions[3].value, "检查项目并汇报");
}

#[test]
fn default_terminal_tool_entries_include_fork_session() {
    let defaults = default_terminal_tool_entries();

    assert!(defaults.iter().any(|entry| {
        entry.id == "fork_session"
            && entry.label == "fork"
            && entry.actions.len() == 1
            && entry.actions[0].kind == "fork_session"
            && entry.actions[0].value.is_empty()
    }));
}

#[test]
fn load_saved_settings_merges_builtin_fork_tool_entry() {
    let config_path = std::env::temp_dir()
        .join(format!("webclx-settings-terminal-tool-entries-{}.json", std::process::id()));
    // 用户已保存两个自定义条目，但没有内置的 fork 条目。
    std::fs::write(
        &config_path,
        r#"{
            "workspace_dir": "/home/codes",
            "terminal_tool_entries": [
                {
                    "id": "tool_alpha",
                    "root_key": "tools",
                    "kind": "action",
                    "label": "alpha",
                    "sort_order": 10,
                    "actions": [
                        {"kind": "create_terminal", "value": "", "seconds": 0.0}
                    ]
                }
            ]
        }"#,
    )
    .expect("write test settings");

    let loaded = load_saved_settings(&config_path)
        .expect("load settings")
        .expect("settings should exist");
    let _ = std::fs::remove_file(&config_path);

    // 用户自定义条目保留。
    assert!(
        loaded
            .terminal_tool_entries
            .iter()
            .any(|entry| entry.id == "tool_alpha")
    );
    // 内置默认 fork 条目被合并回来。
    assert!(loaded.terminal_tool_entries.iter().any(|entry| {
        entry.id == "fork_session"
            && entry
                .actions
                .iter()
                .any(|action| action.kind == "fork_session")
    }));
}

#[test]
fn validate_terminal_tool_entries_accepts_fork_session_without_a_value() {
    let entries = vec![terminal_tool_entry(
        "fork_session",
        None,
        "action",
        vec![TerminalToolAction {
            kind: "fork_session".to_string(),
            value: "ignored".to_string(),
            seconds: 0.0,
            ..Default::default()
        }],
    )];

    let validated = validate_terminal_tool_entries(&entries).expect("fork tool action");

    assert_eq!(validated[0].actions[0].kind, "fork_session");
    assert!(validated[0].actions[0].value.is_empty());
}

#[test]
fn validate_terminal_tool_entries_rejects_cycles_and_non_folder_parents() {
    let cycle = vec![
        terminal_tool_entry("one", Some("two"), "folder", vec![]),
        terminal_tool_entry("two", Some("one"), "folder", vec![]),
    ];
    let cycle_error = validate_terminal_tool_entries(&cycle).expect_err("cycle must fail");
    assert!(cycle_error.to_string().contains("循环"));

    let invalid_parent = vec![
        terminal_tool_entry(
            "action-parent",
            None,
            "action",
            vec![TerminalToolAction {
                kind: "create_terminal".to_string(),
                value: String::new(),
                seconds: 0.0,
                ..Default::default()
            }],
        ),
        terminal_tool_entry("child", Some("action-parent"), "folder", vec![]),
    ];
    let parent_error =
        validate_terminal_tool_entries(&invalid_parent).expect_err("non-folder parent must fail");
    assert!(parent_error.to_string().contains("目录"));
}

#[test]
fn default_error_keyword_actions_include_context_window_compact() {
    let defaults = default_terminal_error_keyword_actions();
    assert!(defaults.iter().any(|action| {
        action.keyword == CONTEXT_WINDOW_EXHAUSTED_ERROR_KEYWORD
            && action.action == TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE
    }));
    assert!(
        defaults
            .iter()
            .all(|action| action.keyword != "MCP startup incomplete")
    );
    assert!(defaults.iter().any(|action| {
        action.keyword == "404 Not Found"
            && action.action == TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY
    }));
    assert!(defaults.iter().any(|action| {
        action.keyword == "last status: 404"
            && action.action == TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY
    }));
}

#[test]
fn builtin_404_actions_upgrade_legacy_continue_rules_to_mark_only() {
    let merged = merge_builtin_terminal_error_keyword_actions(&[TerminalErrorKeywordAction {
        keyword: "404 Not Found".to_string(),
        action: TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE.to_string(),
    }]);
    assert!(merged.iter().any(|action| {
        action.keyword == "404 Not Found"
            && action.action == TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY
    }));
}

#[test]
fn sanitize_error_keyword_actions_dedupes_and_normalizes_unknown_action() {
    let actions = vec![
        TerminalErrorKeywordAction {
            keyword: "ran out of room in the model's context window".to_string(),
            action: TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE.to_string(),
        },
        // Same keyword with different whitespace/case — should dedupe.
        TerminalErrorKeywordAction {
            keyword: "  Ran  Out  of  Room  in  the  Model's  Context  Window  ".to_string(),
            action: "compact_then_continue".to_string(),
        },
        TerminalErrorKeywordAction {
            keyword: "some transient error".to_string(),
            action: "bogus_action".to_string(),
        },
    ];
    let sanitized = sanitize_terminal_error_keyword_actions(&actions);
    assert_eq!(sanitized.len(), 2);
    // Duplicate keyword (case/whitespace-insensitive) should be deduped.
    assert_eq!(sanitized[0].keyword, "ran out of room in the model's context window");
    assert_eq!(sanitized[0].action, TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE);
    // Unknown action falls back to "continue".
    assert_eq!(sanitized[1].action, TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE);
}

#[test]
fn validate_terminal_tool_entries_accepts_codex_launch() {
    let entries = vec![terminal_tool_entry(
        "proxy_settings_workflow",
        None,
        "action",
        vec![TerminalToolAction {
            kind: "codex_launch".to_string(),
            value: "$mihomo-proxy-ops 请检查当前代理配置，并根据当前环境完成代理设置。".to_string(),
            seconds: 0.0,
            preset_selector: "miniMax".to_string(),
            preset_match: "unique_contains".to_string(),
            cwd: "/home/system".to_string(),
            project_path: "/home/system".to_string(),
            terminal_name: "代理设置".to_string(),
            session_action: "new".to_string(),
            ..Default::default()
        }],
    )];
    // fix: label should be "代理设置"
    let mut entries = entries;
    entries[0].label = "代理设置".to_string();
    entries[0].sort_order = 20;

    let validated = validate_terminal_tool_entries(&entries).expect("codex_launch valid");
    assert_eq!(validated[0].actions[0].kind, "codex_launch");
    assert_eq!(validated[0].actions[0].preset_selector, "miniMax");
    assert_eq!(validated[0].actions[0].preset_match, "unique_contains");
    assert_eq!(validated[0].actions[0].cwd, "/home/system");
    assert_eq!(validated[0].actions[0].project_path, "/home/system");
    assert_eq!(validated[0].actions[0].terminal_name, "代理设置");
    assert_eq!(validated[0].actions[0].session_action, "new");
}

#[test]
fn validate_terminal_tool_entries_rejects_invalid_codex_launch_enums() {
    // invalid preset_match
    let bad = terminal_tool_entry(
        "bad1",
        None,
        "action",
        vec![TerminalToolAction {
            kind: "codex_launch".to_string(),
            value: "task".to_string(),
            seconds: 0.0,
            preset_selector: "miniMax".to_string(),
            preset_match: "bogus".to_string(),
            cwd: "/home/system".to_string(),
            project_path: "/home/system".to_string(),
            terminal_name: "代理设置".to_string(),
            session_action: "new".to_string(),
            ..Default::default()
        }],
    );
    assert!(validate_terminal_tool_entries(&[bad]).is_err());

    // invalid session_action
    let bad2 = terminal_tool_entry(
        "bad2",
        None,
        "action",
        vec![TerminalToolAction {
            kind: "codex_launch".to_string(),
            value: "task".to_string(),
            seconds: 0.0,
            preset_selector: "miniMax".to_string(),
            preset_match: "unique_contains".to_string(),
            cwd: "/home/system".to_string(),
            project_path: "/home/system".to_string(),
            terminal_name: "代理设置".to_string(),
            session_action: "bogus".to_string(),
            ..Default::default()
        }],
    );
    assert!(validate_terminal_tool_entries(&[bad2]).is_err());

    // empty preset_selector
    let bad3 = terminal_tool_entry(
        "bad3",
        None,
        "action",
        vec![TerminalToolAction {
            kind: "codex_launch".to_string(),
            value: "task".to_string(),
            seconds: 0.0,
            preset_selector: "  ".to_string(),
            preset_match: "unique_contains".to_string(),
            cwd: "/home/system".to_string(),
            project_path: "/home/system".to_string(),
            terminal_name: "代理设置".to_string(),
            session_action: "new".to_string(),
            ..Default::default()
        }],
    );
    assert!(validate_terminal_tool_entries(&[bad3]).is_err());

    // relative cwd
    let bad4 = terminal_tool_entry(
        "bad4",
        None,
        "action",
        vec![TerminalToolAction {
            kind: "codex_launch".to_string(),
            value: "task".to_string(),
            seconds: 0.0,
            preset_selector: "miniMax".to_string(),
            preset_match: "unique_contains".to_string(),
            cwd: "home/system".to_string(),
            project_path: "/home/system".to_string(),
            terminal_name: "代理设置".to_string(),
            session_action: "new".to_string(),
            ..Default::default()
        }],
    );
    assert!(validate_terminal_tool_entries(&[bad4]).is_err());
}

#[test]
fn default_terminal_tool_entries_include_proxy_settings_workflow() {
    let defaults = default_terminal_tool_entries();
    let proxy = defaults
        .iter()
        .find(|entry| entry.id == "proxy_settings_workflow");
    assert!(proxy.is_some(), "proxy_settings_workflow should be built-in");
    let proxy = proxy.unwrap();
    assert_eq!(proxy.label, "代理设置");
    assert_eq!(proxy.actions.len(), 1);
    let action = &proxy.actions[0];
    assert_eq!(action.kind, "codex_launch");
    assert_eq!(action.value, "$mihomo-proxy-ops");
    assert_eq!(action.preset_selector, "miniMax");
    assert_eq!(action.preset_match, "unique_contains");
    assert_eq!(action.cwd, "/home/system");
    assert_eq!(action.project_path, "/home/system");
    assert_eq!(action.terminal_name, "代理设置");
    assert_eq!(action.session_action, "new");
}

#[test]
fn legacy_actions_deserialize_with_empty_codex_launch_fields() {
    let json = r#"{
        "kind": "send_command",
        "value": "codex",
        "seconds": 0.0
    }"#;
    let action: TerminalToolAction =
        serde_json::from_str(json).expect("legacy action deserializes");
    assert_eq!(action.kind, "send_command");
    assert_eq!(action.value, "codex");
    assert_eq!(action.preset_selector, "");
    assert_eq!(action.preset_match, "");
    assert_eq!(action.session_action, "");
}
