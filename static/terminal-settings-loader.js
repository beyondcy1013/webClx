// webClx terminal settings loading and runtime application.
// Extracted from terminal.js as global declarations; no top-level DOM setup.

async function loadTerminalSettings() {
  try {
    const settings = await requestJson("/api/settings");
    state.hostName = normalizeHostName(settings.host_name);
    applyDocumentTitle();
    state.workspaceDir =
      typeof settings.workspace_dir === "string" ? settings.workspace_dir : "";
    state.showAllWorkspaceSessions = Boolean(settings.show_all_workspace_sessions);
    state.terminalWorkspaceIconPath = normalizeProjectIconPath(
      settings.terminal_workspace_icon_path,
      DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
    );
    sessionSelectEl?.workspaceIconSelectController?.sync();
    agentSessionSelectEl?.workspaceIconSelectController?.sync();
    const rawTerminalQuickCommands = Array.isArray(settings.terminal_quick_commands)
      ? settings.terminal_quick_commands
      : DEFAULT_TERMINAL_QUICK_COMMANDS;
    state.terminalQuickCommands = normalizeTerminalQuickCommands(
      rawTerminalQuickCommands,
      Array.isArray(settings.terminal_quick_commands) ? null : DEFAULT_TERMINAL_QUICK_COMMANDS,
      { includeCommandLine: true },
    );
    state.terminalQuickStartDefaultKey = normalizeTerminalQuickStartDefaultKey(
      typeof settings.terminal_quick_start_default_key === "string"
        ? settings.terminal_quick_start_default_key
        : DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY,
      state.terminalQuickCommands,
    );
    state.terminalActivityAgentDisplay = normalizeTerminalActivityAgentDisplay(
      settings.terminal_activity_agent_display,
    );
    state.terminalAutoContinueIntervalSeconds = normalizeTerminalAutoContinueIntervalSeconds(
      settings.terminal_auto_continue_interval_seconds,
    );
    const serverAutoContinueOnError = settings.terminal_auto_continue_on_error === true;
    const legacyAutoContinueOnError = readStoredTerminalAutoContinueOnError();
    state.autoContinueOnError = serverAutoContinueOnError || legacyAutoContinueOnError;
    storeTerminalAutoContinueOnError(state.autoContinueOnError);
    if (sessionAutoContinueToggleEl) {
      sessionAutoContinueToggleEl.checked = state.autoContinueOnError;
    }
    if (state.autoContinueOnError && !serverAutoContinueOnError) {
      persistTerminalAutoContinueOnError(true).catch((error) => {
        updateStatus(error.message || "迁移自动继续设置失败。", "warn");
      });
    }
    state.terminalCompletionBellEnabled = settings.terminal_completion_bell_enabled !== false;
    state.terminalSlashCommands = ensureBuiltInTerminalSlashCommands(
      normalizeTerminalFunctionCommands(
        Array.isArray(settings.terminal_slash_commands)
          ? settings.terminal_slash_commands
          : DEFAULT_TERMINAL_SLASH_COMMANDS,
        Array.isArray(settings.terminal_slash_commands) ? null : DEFAULT_TERMINAL_SLASH_COMMANDS,
      ),
    );
    state.terminalFunctionCommands = ensureBuiltInTerminalFunctionCommands(
      normalizeTerminalFunctionCommands(
        Array.isArray(settings.terminal_function_commands)
          ? settings.terminal_function_commands
          : DEFAULT_TERMINAL_FUNCTION_COMMANDS,
        Array.isArray(settings.terminal_function_commands) ? null : DEFAULT_TERMINAL_FUNCTION_COMMANDS,
      ),
    );
    state.terminalCommandCollections = normalizeTerminalCommandCollections(
      Array.isArray(settings.terminal_command_collections)
        ? settings.terminal_command_collections
        : cloneDefaultTerminalCommandCollections(),
      Array.isArray(settings.terminal_command_collections)
        ? null
        : cloneDefaultTerminalCommandCollections(),
    );
    state.terminalToolEntries = ensureBuiltInTerminalToolEntries(
      Array.isArray(settings.terminal_tool_entries)
        ? settings.terminal_tool_entries
        : settings.default_terminal_tool_entries,
    );
    state.terminalRenamePresets = normalizeTerminalRenamePresets(
      Array.isArray(settings.terminal_rename_presets)
        ? settings.terminal_rename_presets
        : DEFAULT_TERMINAL_RENAME_PRESETS,
      Array.isArray(settings.terminal_rename_presets) ? null : DEFAULT_TERMINAL_RENAME_PRESETS,
    );
    renderTerminalQuickCommandButtons();
    renderTerminalSlashCommandMenu();
    renderTerminalFunctionCommandOptions();
    renderTerminalCommandCollectionsButton();
    renderTerminalToolRootButtons();
    renderSessionRenamePresets();
    applyDesktopTerminalSoftKeyboardSetting(settings.desktop_terminal_soft_keyboard_enabled);
    applyTerminalSoftKeyboardScale(settings.terminal_soft_keyboard_scale);
    applyTerminalFloatingButtonOffset(settings.terminal_floating_button_offset_vh);
    applyTerminalFabAppearance(settings.terminal_fab_action_color,
      settings.terminal_fab_action_opacity);
    applyTerminalFabAutoExpand(settings.terminal_fab_auto_expand !== false);
    applyTerminalTouchSelectionLongPress(settings.terminal_touch_selection_long_press_ms);
    applyTerminalScrollbackLines(settings.terminal_scrollback_lines);
    applyThemeMode(settings.theme_mode, { persist: true });
    const tiers = applyTypographySettings([
      settings.font_size_tier_1,
      settings.font_size_tier_2,
      settings.font_size_tier_3,
      settings.font_size_tier_4,
    ]);
    const terminalFontFamily = readCssCustomProperty(
      "--font-family-mono",
      '"IBM Plex Mono", "SFMono-Regular", Consolas, monospace'
    );
    forEachTerminalSessionContext((context) => {
      context.term.options.fontFamily = terminalFontFamily;
      context.term.options.fontSize = fontTierPx(tiers[3]);
      context.term.options.theme = terminalThemeForCursorState(false);
    });
    setTerminalCursorHiddenForCorrection(terminalCursorCorrectionActive, { force: true });
    syncTerminalHostHeight();
    syncTerminalStickyOffsets();
    syncTerminalNavScroll({ forceEnd: true });
    syncScrollTopButtonOffset();
    updateScrollTopButton();
    updateTerminalScrollBottomButton();
    updatePageScrollRail();
    await ensureFontsReady();
    fitTerminal({ force: true });
    scheduleTerminalSizeSettle();
  } catch {
    renderTerminalQuickCommandButtons();
    renderTerminalSlashCommandMenu();
    renderTerminalFunctionCommandOptions();
    renderTerminalCommandCollectionsButton();
    renderTerminalToolRootButtons();
    renderSessionRenamePresets();
    applyDesktopTerminalSoftKeyboardSetting(state.desktopTerminalSoftKeyboardEnabled);
    applyTerminalSoftKeyboardScale(state.terminalSoftKeyboardScale);
    applyTerminalFloatingButtonOffset(state.terminalFloatingButtonOffsetVh);
    applyTerminalFabAppearance(state.terminalFabActionColor, state.terminalFabActionOpacity);
    applyTerminalFabAutoExpand(state.terminalFabAutoExpand);
    applyTerminalTouchSelectionLongPress(state.terminalTouchSelectionLongPressMs);
    applyTerminalScrollbackLines(state.terminalScrollbackLines);
    // Keep working with the optimistic default if settings are temporarily unavailable.
  }
}
