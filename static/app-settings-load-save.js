function announceSettingsMutation(settings = {}) {
  try {
    window.localStorage.setItem(
      SETTINGS_EVENT_STORAGE_KEY,
      JSON.stringify({
        updated_at: Date.now(),
        terminal_quick_start_default_key: settings.terminal_quick_start_default_key || "",
      }),
    );
  } catch {
    // Storage broadcast is best-effort; saved settings remain authoritative.
  }
}

function renderCodexCommonConfig(result = {}) {
  if (codexCommonApprovalNeverInputEl) {
    codexCommonApprovalNeverInputEl.checked = Boolean(result.approval_never);
  }
  if (codexCommonSandboxFullAccessInputEl) {
    codexCommonSandboxFullAccessInputEl.checked = Boolean(result.sandbox_full_access);
  }
  if (codexCommonConfigPathEl) {
    codexCommonConfigPathEl.textContent = result.config_file || "~/.codex/config.toml";
  }
}

async function loadCodexCommonConfig() {
  if (!codexCommonApprovalNeverInputEl || !codexCommonSandboxFullAccessInputEl) {
    return;
  }
  setButtonBusy(codexCommonConfigRefreshButtonEl, true, "读取中…");
  try {
    const result = await requestJson("/api/settings/codex-common-config");
    renderCodexCommonConfig(result);
    updateStatus(
      codexCommonConfigStatusEl,
      result.exists ? `已读取 ${result.user} 的当前配置。` : "配置文件尚未创建。",
      "ok",
    );
  } catch (error) {
    updateStatus(codexCommonConfigStatusEl, error.message, "warn");
  } finally {
    setButtonBusy(codexCommonConfigRefreshButtonEl, false);
  }
}

async function saveCodexCommonConfig() {
  if (!codexCommonApprovalNeverInputEl || !codexCommonSandboxFullAccessInputEl) {
    return;
  }
  updateStatus(codexCommonConfigStatusEl, "正在保存当前配置…", "info");
  setButtonBusy(codexCommonConfigSaveButtonEl, true, "保存中…");
  if (codexCommonConfigRefreshButtonEl) {
    codexCommonConfigRefreshButtonEl.disabled = true;
  }
  try {
    const result = await requestJson("/api/settings/codex-common-config", {
      method: "PUT",
      body: JSON.stringify({
        approval_never: codexCommonApprovalNeverInputEl.checked,
        sandbox_full_access: codexCommonSandboxFullAccessInputEl.checked,
      }),
    });
    renderCodexCommonConfig(result);
    updateStatus(codexCommonConfigStatusEl, "当前 config.toml 已保存。", "ok");
  } catch (error) {
    updateStatus(codexCommonConfigStatusEl, error.message, "warn");
  } finally {
    setButtonBusy(codexCommonConfigSaveButtonEl, false);
    if (codexCommonConfigRefreshButtonEl) {
      codexCommonConfigRefreshButtonEl.disabled = false;
    }
  }
}

async function saveCodexDefaultConfigEntries() {
  const entries = readCodexDefaultConfigEntriesFromTable();
  const providerOwnedEntry = entries.find(
    (entry) => codexConfigScopeForKey(entry.key).kind === "provider",
  );
  if (providerOwnedEntry) {
    updateStatus(
      codexDefaultConfigStatusEl,
      `${providerOwnedEntry.key} 属于预设 Provider，请在对应预设中设置。`,
      "warn",
    );
    return;
  }
  updateStatus(codexDefaultConfigStatusEl, "正在保存默认值…", "info");
  setButtonBusy(codexDefaultConfigSaveButtonEl, true, "保存中…");
  if (codexDefaultConfigResetButtonEl) {
    codexDefaultConfigResetButtonEl.disabled = true;
  }

  try {
    const settings = await requestJson("/api/settings", {
      method: "PUT",
      body: JSON.stringify({
        codex_default_config_entries: entries,
      }),
    });
    syncLegacyCodexConfigStateFromEntries(
      normalizeCodexDefaultConfigEntriesFromSettings(settings),
    );
    renderCodexDefaultConfigEntries(state.codexDefaultConfigEntries);
    refreshConfigOverrideDatalists();
    announceSettingsMutation(settings);
    updateStatus(
      codexDefaultConfigStatusEl,
      `默认值已保存，共 ${state.codexDefaultConfigEntries.length} 项。`,
      "ok",
    );
  } catch (error) {
    updateStatus(codexDefaultConfigStatusEl, error.message, "warn");
  } finally {
    setButtonBusy(codexDefaultConfigSaveButtonEl, false);
    if (codexDefaultConfigResetButtonEl) {
      codexDefaultConfigResetButtonEl.disabled = false;
    }
  }
}

async function saveClaudeDefaultConfigEntries() {
  const entries = readClaudeDefaultConfigEntriesFromTable();
  updateStatus(claudeDefaultConfigStatusEl, "正在保存默认值…", "info");
  setButtonBusy(claudeDefaultConfigSaveButtonEl, true, "保存中…");
  if (claudeDefaultConfigResetButtonEl) {
    claudeDefaultConfigResetButtonEl.disabled = true;
  }

  try {
    const settings = await requestJson("/api/settings", {
      method: "PUT",
      body: JSON.stringify({
        claude_default_config_entries: entries,
      }),
    });
    state.claudeDefaultConfigEntries = normalizeClaudeDefaultConfigEntriesFromSettings(settings);
    renderClaudeDefaultConfigEntries(state.claudeDefaultConfigEntries);
    announceSettingsMutation(settings);
    updateStatus(
      claudeDefaultConfigStatusEl,
      `默认值已保存，共 ${state.claudeDefaultConfigEntries.length} 项。`,
      "ok",
    );
  } catch (error) {
    updateStatus(claudeDefaultConfigStatusEl, error.message, "warn");
  } finally {
    setButtonBusy(claudeDefaultConfigSaveButtonEl, false);
    if (claudeDefaultConfigResetButtonEl) {
      claudeDefaultConfigResetButtonEl.disabled = false;
    }
  }
}

async function loadSettings() {
  try {
    const previousTerminalUser = state.terminalUser;
    const settings = await requestJson("/api/settings");
    state.hostName = normalizeHostName(settings.host_name);
    applyDocumentTitle();
    state.workspaceDir = settings.workspace_dir;
    state.defaultWorkspaceDir = settings.default_workspace_dir;
    state.terminalUser = normalizeTerminalUser(settings.terminal_user);
    state.defaultTerminalUser = normalizeTerminalUser(settings.default_terminal_user);
    state.availableUsers = normalizeAvailableUsers(settings.available_users, state.terminalUser);
    state.defaultTerminalQuickCommands = normalizeTerminalQuickCommands(
      settings.default_terminal_quick_commands,
      cloneDefaultTerminalQuickCommands(),
    );
    state.defaultTerminalQuickStartDefaultKey = normalizeTerminalQuickStartDefaultKey(
      typeof settings.default_terminal_quick_start_default_key === "string"
        ? settings.default_terminal_quick_start_default_key
        : DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY,
      state.defaultTerminalQuickCommands,
    );
    state.defaultTerminalDefaultEnvVars = normalizeTerminalDefaultEnvVars(
      settings.default_terminal_default_env_vars || DEFAULT_TERMINAL_DEFAULT_ENV_VARS,
    );
    const rawTerminalQuickCommands = Array.isArray(settings.terminal_quick_commands)
      ? settings.terminal_quick_commands
      : state.defaultTerminalQuickCommands;
    state.terminalQuickCommands = normalizeTerminalQuickCommands(
      rawTerminalQuickCommands,
      Array.isArray(settings.terminal_quick_commands) ? null : state.defaultTerminalQuickCommands,
    );
    state.defaultTerminalSlashCommands = ensureBuiltInTerminalSlashCommands(
      settings.default_terminal_slash_commands || DEFAULT_TERMINAL_SLASH_COMMANDS,
      DEFAULT_TERMINAL_SLASH_COMMANDS,
    );
    state.terminalSlashCommands = ensureBuiltInTerminalSlashCommands(
      Array.isArray(settings.terminal_slash_commands)
        ? settings.terminal_slash_commands
        : state.defaultTerminalSlashCommands,
      Array.isArray(settings.terminal_slash_commands) ? null : state.defaultTerminalSlashCommands,
    );
    state.defaultTerminalFunctionCommands = ensureBuiltInTerminalFunctionCommands(
      settings.default_terminal_function_commands || DEFAULT_TERMINAL_FUNCTION_COMMANDS,
      DEFAULT_TERMINAL_FUNCTION_COMMANDS,
    );
    state.defaultTerminalRenamePresets = normalizeTerminalRenamePresets(
      settings.default_terminal_rename_presets || DEFAULT_TERMINAL_RENAME_PRESETS,
      DEFAULT_TERMINAL_RENAME_PRESETS,
    );
    state.terminalFunctionCommands = ensureBuiltInTerminalFunctionCommands(
      normalizeTerminalFunctionCommands(
        Array.isArray(settings.terminal_function_commands)
          ? settings.terminal_function_commands
          : state.defaultTerminalFunctionCommands,
        Array.isArray(settings.terminal_function_commands) ? null : state.defaultTerminalFunctionCommands,
      ),
      state.defaultTerminalFunctionCommands,
    );
    state.defaultTerminalCommandCollections = normalizeTerminalCommandCollections(
      settings.default_terminal_command_collections || cloneDefaultTerminalCommandCollections(),
      cloneDefaultTerminalCommandCollections(),
    );
    state.terminalCommandCollections = normalizeTerminalCommandCollections(
      Array.isArray(settings.terminal_command_collections)
        ? settings.terminal_command_collections
        : state.defaultTerminalCommandCollections,
      Array.isArray(settings.terminal_command_collections)
        ? null
        : state.defaultTerminalCommandCollections,
    );
    state.defaultTerminalToolEntries = ensureBuiltInTerminalToolEntries(
      settings.default_terminal_tool_entries || [],
    );
    state.terminalToolEntries = ensureBuiltInTerminalToolEntries(
      Array.isArray(settings.terminal_tool_entries)
        ? settings.terminal_tool_entries
        : state.defaultTerminalToolEntries,
    );
    state.terminalRenamePresets = normalizeTerminalRenamePresets(
      Array.isArray(settings.terminal_rename_presets)
        ? settings.terminal_rename_presets
        : state.defaultTerminalRenamePresets,
      Array.isArray(settings.terminal_rename_presets) ? null : state.defaultTerminalRenamePresets,
    );
    state.terminalQuickStartDefaultKey = normalizeTerminalQuickStartDefaultKey(
      typeof settings.terminal_quick_start_default_key === "string"
        ? settings.terminal_quick_start_default_key
        : state.defaultTerminalQuickStartDefaultKey,
      state.terminalQuickCommands,
    );
    state.terminalDefaultEnvVars = normalizeTerminalDefaultEnvVars(
      settings.terminal_default_env_vars || state.defaultTerminalDefaultEnvVars,
    );
    state.showDotEntries = Boolean(settings.show_dot_entries);
    if (workspaceShowHiddenInputEl) {
      workspaceShowHiddenInputEl.checked = state.showDotEntries;
    }
    state.showAllWorkspaceSessions = Boolean(settings.show_all_workspace_sessions);
    state.desktopTerminalSoftKeyboardEnabled = Boolean(
      settings.desktop_terminal_soft_keyboard_enabled,
    );
    state.terminalSoftKeyboardScale = normalizeTerminalSoftKeyboardScale(
      settings.terminal_soft_keyboard_scale,
    );
    state.terminalFloatingButtonOffsetVh = normalizeTerminalFloatingButtonOffsetVh(
      settings.terminal_floating_button_offset_vh,
    );
    state.terminalFabActionColor = normalizeTerminalFabActionColor(
      settings.terminal_fab_action_color,
    );
    state.terminalFabActionOpacity = normalizeTerminalFabActionOpacity(
      settings.terminal_fab_action_opacity,
    );
    state.terminalFabAutoExpand = settings.terminal_fab_auto_expand !== false;
    state.terminalTouchSelectionLongPressMs = normalizeTerminalTouchSelectionLongPressMs(
      settings.terminal_touch_selection_long_press_ms,
    );
    state.terminalScrollbackLines = normalizeTerminalScrollbackLines(
      settings.terminal_scrollback_lines,
    );
    state.terminalErrorMatchLineLimit = normalizeTerminalErrorMatchLineLimit(
      settings.terminal_error_match_line_limit,
    );
    state.terminalAutoContinueIntervalSeconds = normalizeTerminalAutoContinueIntervalSeconds(
      settings.terminal_auto_continue_interval_seconds,
    );
    state.terminalAutoContinueBackoffFactor = normalizeTerminalAutoContinueBackoffFactor(
      settings.terminal_auto_continue_backoff_factor,
    );
    state.terminalAutoContinueBackoffMaxMinutes = normalizeTerminalAutoContinueBackoffMaxMinutes(
      settings.terminal_auto_continue_backoff_max_minutes,
    );
    state.terminalAutoContinueRespectManualInterrupt =
      settings.terminal_auto_continue_respect_manual_interrupt !== false;
    state.terminalAutoContinueTimePatterns = normalizeTerminalAutoContinueTimePatterns(
      settings.terminal_auto_continue_time_patterns || DEFAULT_TERMINAL_AUTO_CONTINUE_TIME_PATTERNS,
    );
    state.terminalAutoContinueActiveWindow = normalizeTerminalAutoContinueActiveWindow(
      settings.terminal_auto_continue_active_window || "",
    );
    state.terminalScheduledInputAvoidWindow = normalizeTerminalScheduledInputAvoidWindow(
      settings.terminal_scheduled_input_avoid_window || DEFAULT_TERMINAL_SCHEDULED_INPUT_AVOID_WINDOW,
    );
    state.terminalErrorKeywords = normalizeTerminalErrorKeywords(
      settings.terminal_error_keywords || DEFAULT_TERMINAL_ERROR_KEYWORDS,
    );
    state.terminalErrorKeywordActions = normalizeTerminalErrorKeywordActions(
      settings.terminal_error_keyword_actions || DEFAULT_TERMINAL_ERROR_KEYWORD_ACTIONS,
    );
    state.terminalActivityAgentDisplay = normalizeTerminalActivityAgentDisplay(
      settings.terminal_activity_agent_display,
    );
    state.terminalCompletionBellEnabled = settings.terminal_completion_bell_enabled !== false;
    state.serverPortAutoIncrement =
      typeof settings.server_port_auto_increment === "boolean"
        ? settings.server_port_auto_increment
        : DEFAULT_SERVER_PORT_AUTO_INCREMENT;
    {
      const rawTimeout =
        typeof settings.compile_command_timeout_secs === "number"
          ? settings.compile_command_timeout_secs
          : settings.default_compile_command_timeout_secs ?? DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS;
      const clamped = Math.min(3600, Math.max(60, Number(rawTimeout) || DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS));
      state.compileCommandTimeoutSecs = clamped;
    }
    {
      const rawConcurrency =
        typeof settings.compile_max_concurrency === "number"
          ? settings.compile_max_concurrency
          : settings.default_compile_max_concurrency ?? DEFAULT_COMPILE_MAX_CONCURRENCY;
      state.compileMaxConcurrency = Math.min(
        32,
        Math.max(1, Math.trunc(Number(rawConcurrency) || DEFAULT_COMPILE_MAX_CONCURRENCY)),
      );
    }
    state.compileEnvironment = normalizeCompileEnvironment(
      settings.compile_environment || DEFAULT_COMPILE_ENVIRONMENT,
    );
    {
      const rawDays =
        typeof settings.session_ttl_days === "number"
          ? settings.session_ttl_days
          : settings.default_session_ttl_days ?? DEFAULT_SESSION_TTL_DAYS;
      state.sessionTtlDays = Math.min(365, Math.max(1, Number(rawDays) || DEFAULT_SESSION_TTL_DAYS));
    }
    state.favoritePaths = settings.favorite_paths;
    const savedWorkspaceHistory = normalizeWorkspaceHistoryItems(settings.workspace_history);
    const cachedWorkspaceHistory = readWorkspaceHistory();
    const shouldMigrateWorkspaceHistory =
      shouldMigrateCachedWorkspaceHistory() && cachedWorkspaceHistory.length > 0;
    const mergedWorkspaceHistory = shouldMigrateWorkspaceHistory
      ? mergeWorkspaceHistoryItems(savedWorkspaceHistory, cachedWorkspaceHistory)
      : savedWorkspaceHistory;
    state.workspaceHistory = mergedWorkspaceHistory;
    storeWorkspaceHistory(mergedWorkspaceHistory);
    if (shouldMigrateWorkspaceHistory) {
      if (workspaceHistoryItemsEqual(mergedWorkspaceHistory, savedWorkspaceHistory)) {
        markWorkspaceHistoryMigrated();
      } else {
        persistWorkspaceHistory(mergedWorkspaceHistory, {
          keepalive: true,
          silent: true,
          markMigrated: true,
        });
      }
    }
    state.claudeModelOptions = claudeManager.normalizeClaudeModelOptions(settings.claude_model_options);
    state.claudeDefaultConfigEntries = normalizeClaudeDefaultConfigEntriesFromSettings(settings);
    state.presetSyncRemoteUrlHistory = presetSyncManager.normalizePresetSyncRemoteUrlHistory(
      settings.preset_sync_remote_url_history,
    );
    presetSyncManager.renderPresetSyncRemoteUrlHistory();
    state.desktopRemoteUrl = normalizeDesktopRemoteUrl(settings.desktop_remote_url);
    state.desktopRemoteUrlHistory = normalizeDesktopRemoteUrlHistory(
      settings.desktop_remote_url_history,
    );
    applyDesktopRemoteUrl(state.desktopRemoteUrl);
    state.codexApiAutoProxyMatchProviderIds = normalizeCodexApiAutoProxyMatchProviderIds(
      settings.codex_api_auto_proxy_match_provider_ids,
    );
    syncLegacyCodexConfigStateFromEntries(normalizeCodexDefaultConfigEntriesFromSettings(settings));
    state.showFullPath = settings.show_full_path !== false;
    state.workspaceBrowserIconPath = normalizeProjectIconPath(
      settings.workspace_browser_icon_path,
      DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
    );
    state.terminalWorkspaceIconPath = normalizeProjectIconPath(
      settings.terminal_workspace_icon_path,
      DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
    );
    state.terminalUserHome = settings.terminal_user_home || "";
    state.serverVersion =
      typeof settings.server_version === "string" && settings.server_version.trim()
        ? settings.server_version.trim()
        : state.serverVersion || "";
    state.themeMode = applyThemeMode(settings.theme_mode, { persist: true });
    state.fontSizeTiers = normalizeFontSizeTiers([
      settings.font_size_tier_1,
      settings.font_size_tier_2,
      settings.font_size_tier_3,
      settings.font_size_tier_4,
    ]);
    applyTypographySettings(state.fontSizeTiers);
    settingsCurrentDirEl.textContent = settings.workspace_dir;
    settingsCurrentUserEl.textContent = state.terminalUser;
    settingsUserHomeEl.textContent = [
      settings.terminal_user_home || "",
      settings.terminal_user_shell || "",
    ]
      .filter(Boolean)
      .join(" | ") || "加载中…";
    settingsConfigFileEl.textContent = settings.config_file;
    if (updateCurrentVersionEl) {
      updateCurrentVersionEl.textContent = state.serverVersion || "未知";
    }
    workspaceDirInputEl.placeholder = settings.default_workspace_dir || "/home";
    workspaceDirInputEl.value = settings.workspace_dir;
    workspaceDirInputEl.scrollLeft = 0;
    renderTerminalUserOptions(state.availableUsers, state.terminalUser);
    renderTerminalQuickCommands(state.terminalQuickCommands, state.terminalQuickStartDefaultKey);
    syncTerminalCommandTextareasFromState();
    renderTerminalCommandCollectionsEditor();
    renderTerminalToolEntriesTable();
    renderTerminalShortcutSettings();
    if (terminalDefaultEnvInputEl) {
      terminalDefaultEnvInputEl.value = formatTerminalDefaultEnvVars(state.terminalDefaultEnvVars);
    }
    renderTerminalRenamePresetsSetting(state.terminalRenamePresets);
    clearTerminalQuickCommandEditor();
    showDotEntriesInputEl.checked = settings.show_dot_entries;
    if (workspaceShowHiddenInputEl) {
      workspaceShowHiddenInputEl.checked = Boolean(settings.show_dot_entries);
    }
    if (serverPortAutoIncrementInputEl) {
      serverPortAutoIncrementInputEl.checked = state.serverPortAutoIncrement;
    }
    if (compileCommandTimeoutInputEl) {
      compileCommandTimeoutInputEl.value = String(state.compileCommandTimeoutSecs);
    }
    if (compileMaxConcurrencyInputEl) {
      compileMaxConcurrencyInputEl.value = String(state.compileMaxConcurrency);
    }
    if (compileEnvironmentInputEl) {
      compileEnvironmentInputEl.value = formatCompileEnvironment(state.compileEnvironment);
    }
    if (sessionTtlDaysInputEl) {
      sessionTtlDaysInputEl.value = String(state.sessionTtlDays);
    }
    showAllWorkspaceSessionsInputEl.checked = state.showAllWorkspaceSessions;
    desktopTerminalSoftKeyboardInputEl.checked = state.desktopTerminalSoftKeyboardEnabled;
    if (terminalSoftKeyboardScaleInputEl) {
      terminalSoftKeyboardScaleInputEl.value = formatTerminalSoftKeyboardScale(
        state.terminalSoftKeyboardScale,
      );
    }
    if (terminalFloatingButtonOffsetInputEl) {
      terminalFloatingButtonOffsetInputEl.value = formatTerminalFloatingButtonOffsetVh(
        state.terminalFloatingButtonOffsetVh,
      );
    }
    if (terminalFabActionColorInputEl) {
      terminalFabActionColorInputEl.value = state.terminalFabActionColor;
    }
    if (terminalFabActionOpacityInputEl) {
      terminalFabActionOpacityInputEl.value = String(state.terminalFabActionOpacity);
      renderTerminalFabActionOpacityOutput(state.terminalFabActionOpacity);
    }
    if (terminalFabAutoExpandInputEl) {
      terminalFabAutoExpandInputEl.checked = state.terminalFabAutoExpand;
    }
    if (terminalTouchSelectionLongPressInputEl) {
      terminalTouchSelectionLongPressInputEl.value = formatTerminalTouchSelectionLongPressMs(
        state.terminalTouchSelectionLongPressMs,
      );
    }
    if (terminalScrollbackLinesInputEl) {
      terminalScrollbackLinesInputEl.value = String(state.terminalScrollbackLines);
    }
    if (terminalErrorLineLimitInputEl) {
      terminalErrorLineLimitInputEl.value = formatTerminalErrorMatchLineLimit(
        state.terminalErrorMatchLineLimit,
      );
    }
    if (terminalAutoContinueIntervalInputEl) {
      terminalAutoContinueIntervalInputEl.value = formatTerminalAutoContinueIntervalSeconds(
        state.terminalAutoContinueIntervalSeconds,
      );
    }
    if (terminalAutoContinueBackoffFactorInputEl) {
      terminalAutoContinueBackoffFactorInputEl.value = formatTerminalAutoContinueBackoffFactor(
        state.terminalAutoContinueBackoffFactor,
      );
    }
    if (terminalAutoContinueBackoffMaxMinutesInputEl) {
      terminalAutoContinueBackoffMaxMinutesInputEl.value =
        formatTerminalAutoContinueBackoffMaxMinutes(
          state.terminalAutoContinueBackoffMaxMinutes,
        );
    }
    if (terminalAutoContinueRespectManualInterruptInputEl) {
      terminalAutoContinueRespectManualInterruptInputEl.checked =
        state.terminalAutoContinueRespectManualInterrupt;
    }
    if (terminalAutoContinueTimePatternsInputEl) {
      terminalAutoContinueTimePatternsInputEl.value = formatTerminalAutoContinueTimePatterns(
        state.terminalAutoContinueTimePatterns,
      );
    }
    if (terminalAutoContinueActiveWindowInputEl) {
      terminalAutoContinueActiveWindowInputEl.value = state.terminalAutoContinueActiveWindow || "";
    }
    if (terminalScheduledInputAvoidWindowInputEl) {
      terminalScheduledInputAvoidWindowInputEl.value = state.terminalScheduledInputAvoidWindow || "";
    }
    renderTerminalErrorKeywordRulesTable();
    if (terminalActivityAgentDisplaySelectEl) {
      terminalActivityAgentDisplaySelectEl.value = state.terminalActivityAgentDisplay;
    }
    if (terminalCompletionBellEnabledInputEl) {
      terminalCompletionBellEnabledInputEl.checked = state.terminalCompletionBellEnabled;
    }
    syncDirectorySessionScopeLabel();
    claudeModelOptionsInputEl.value = state.claudeModelOptions.join("\n");
    renderClaudeDefaultConfigEntries(state.claudeDefaultConfigEntries);
    renderCodexApiAutoProxyMatchProviders(state.codexApiAutoProxyMatchProviderIds);
    renderCodexDefaultConfigEntries(state.codexDefaultConfigEntries);
    syncApiApplyProxyRecommendation();
    if (showFullPathInputEl) {
      showFullPathInputEl.checked = state.showFullPath;
    }
    if (workspaceBrowserIconPathInputEl) {
      workspaceBrowserIconPathInputEl.value = state.workspaceBrowserIconPath;
    }
    if (terminalWorkspaceIconPathInputEl) {
      terminalWorkspaceIconPathInputEl.value = state.terminalWorkspaceIconPath;
    }
    directorySessionListEl?.workspaceIconSelectController?.sync();
    sessionsSessionListEl?.workspaceIconSelectController?.sync();
    setThemeModeInputs(state.themeMode);
    if (fontSizeTier1InputEl) {
      fontSizeTier1InputEl.value = formatFontSizeTier(
        state.fontSizeTiers[0],
        DEFAULT_FONT_SIZE_TIER_1,
      );
    }
    if (fontSizeTier2InputEl) {
      fontSizeTier2InputEl.value = formatFontSizeTier(
        state.fontSizeTiers[1],
        DEFAULT_FONT_SIZE_TIER_2,
      );
    }
    if (fontSizeTier3InputEl) {
      fontSizeTier3InputEl.value = formatFontSizeTier(
        state.fontSizeTiers[2],
        DEFAULT_FONT_SIZE_TIER_3,
      );
    }
    if (fontSizeTier4InputEl) {
      fontSizeTier4InputEl.value = formatFontSizeTier(
        state.fontSizeTiers[3],
        DEFAULT_FONT_SIZE_TIER_4,
      );
    }
    updateFontSettingsSummary();
    refreshConfigOverrideDatalists();
    claudeManager.renderClaudeModelOptions(state.claudeModelOptions);
    if (!state.currentDirectory) {
      renderCurrentPath(settings.workspace_dir || "/");
    }
    renderWorkspaceHistory();
    markWorkspaceHistorySettingsReady();
    renderFavoriteOptions();
    if (state.currentDirectory) {
      renderEntries(state.currentDirectory);
    }
    if (previousTerminalUser !== state.terminalUser) {
      state.settingsConfigFileLoaded = false;
      state.settingsConfigFileDirty = false;
      if (state.activeSettingsTab === "config-files") {
        loadSettingsConfigFile(state.settingsConfigFileKey);
      }
    }
    updateStatus(settingsStatusEl, "设置已加载。", "ok");
  } catch (error) {
    updateStatus(settingsStatusEl, error.message, "warn");
  }
}

async function saveSettings(
  nextDir,
  nextTerminalUser,
  nextTerminalQuickCommands,
  nextTerminalQuickStartDefaultKey,
  nextTerminalDefaultEnvVars,
  nextTerminalSlashCommands,
  nextTerminalFunctionCommands,
  nextTerminalCommandCollections,
  nextTerminalToolEntries,
  nextTerminalRenamePresets,
  nextShowDotEntries,
  nextShowAllWorkspaceSessions,
  nextDesktopTerminalSoftKeyboardEnabled,
  nextTerminalSoftKeyboardScale,
  nextTerminalFloatingButtonOffsetVh,
  nextTerminalFabActionColor,
  nextTerminalFabActionOpacity,
  nextTerminalFabAutoExpand,
  nextTerminalTouchSelectionLongPressMs,
  nextTerminalScrollbackLines,
  nextTerminalErrorMatchLineLimit,
  nextTerminalAutoContinueIntervalSeconds,
  nextTerminalAutoContinueBackoffFactor,
  nextTerminalAutoContinueBackoffMaxMinutes,
  nextTerminalAutoContinueRespectManualInterrupt,
  nextTerminalAutoContinueTimePatterns,
  nextTerminalAutoContinueActiveWindow,
  nextTerminalScheduledInputAvoidWindow,
  nextTerminalErrorKeywords,
  nextTerminalActivityAgentDisplay,
  nextTerminalCompletionBellEnabled,
  nextServerPortAutoIncrement,
  nextCompileCommandTimeoutSecs,
  nextCompileMaxConcurrency,
  nextSessionTtlDays,
  nextClaudeModelOptions,
  nextCodexDefaultConfigEntries,
  nextCodexApiAutoProxyMatchProviderIds,
  nextCodexConfigKey,
  nextCodexConfigValue,
  nextCodexSecondaryConfigKey,
  nextCodexSecondaryConfigValue,
  nextShowFullPath,
  nextWorkspaceBrowserIconPath,
  nextTerminalWorkspaceIconPath,
  nextThemeMode,
  nextFontSizeTiers,
  nextDesktopRemoteUrl,
  nextDesktopRemoteUrlHistory,
) {
  updateStatus(settingsStatusEl, "正在保存设置…", "info");
  saveSettingsButton.disabled = true;
  resetSettingsButton.disabled = true;

  try {
    const workspaceChanged = nextDir.trim() !== state.workspaceDir;
    const terminalUserChanged = normalizeTerminalUser(nextTerminalUser) !== state.terminalUser;
    const settings = await requestJson("/api/settings", {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        workspace_dir: nextDir,
        terminal_user: nextTerminalUser,
        terminal_quick_commands: nextTerminalQuickCommands,
        terminal_quick_start_default_key: nextTerminalQuickStartDefaultKey,
        terminal_default_env_vars: nextTerminalDefaultEnvVars,
        terminal_slash_commands: nextTerminalSlashCommands,
        terminal_function_commands: nextTerminalFunctionCommands,
        terminal_command_collections: nextTerminalCommandCollections,
        terminal_tool_entries: nextTerminalToolEntries,
        terminal_rename_presets: nextTerminalRenamePresets,
        show_dot_entries: nextShowDotEntries,
        show_all_workspace_sessions: nextShowAllWorkspaceSessions,
        desktop_terminal_soft_keyboard_enabled: nextDesktopTerminalSoftKeyboardEnabled,
        terminal_soft_keyboard_scale: nextTerminalSoftKeyboardScale,
        terminal_floating_button_offset_vh: nextTerminalFloatingButtonOffsetVh,
        terminal_fab_action_color: nextTerminalFabActionColor,
        terminal_fab_action_opacity: nextTerminalFabActionOpacity,
        terminal_fab_auto_expand: nextTerminalFabAutoExpand,
        terminal_touch_selection_long_press_ms: nextTerminalTouchSelectionLongPressMs,
        terminal_scrollback_lines: nextTerminalScrollbackLines,
        terminal_error_match_line_limit: nextTerminalErrorMatchLineLimit,
        terminal_auto_continue_interval_seconds: nextTerminalAutoContinueIntervalSeconds,
        terminal_auto_continue_backoff_factor: nextTerminalAutoContinueBackoffFactor,
        terminal_auto_continue_backoff_max_minutes: nextTerminalAutoContinueBackoffMaxMinutes,
        terminal_auto_continue_respect_manual_interrupt:
          nextTerminalAutoContinueRespectManualInterrupt,
        terminal_auto_continue_time_patterns: nextTerminalAutoContinueTimePatterns,
        terminal_auto_continue_active_window: nextTerminalAutoContinueActiveWindow,
        terminal_scheduled_input_avoid_window: nextTerminalScheduledInputAvoidWindow,
        terminal_error_keywords: nextTerminalErrorKeywords,
        terminal_error_keyword_actions: state.terminalErrorKeywordActions,
        terminal_activity_agent_display: nextTerminalActivityAgentDisplay,
        terminal_completion_bell_enabled: nextTerminalCompletionBellEnabled,
        server_port_auto_increment: nextServerPortAutoIncrement,
        compile_command_timeout_secs: nextCompileCommandTimeoutSecs,
        compile_max_concurrency: nextCompileMaxConcurrency,
        compile_environment: parseCompileEnvironmentInput(compileEnvironmentInputEl?.value || ""),
        session_ttl_days: nextSessionTtlDays,
        workspace_history: state.workspaceHistory,
        claude_model_options: nextClaudeModelOptions,
        codex_default_config_entries: nextCodexDefaultConfigEntries,
        codex_api_auto_proxy_match_provider_ids: nextCodexApiAutoProxyMatchProviderIds,
        codex_config_key: nextCodexConfigKey,
        codex_config_value: nextCodexConfigValue,
        codex_secondary_config_key: nextCodexSecondaryConfigKey,
        codex_secondary_config_value: nextCodexSecondaryConfigValue,
        show_full_path: nextShowFullPath,
        workspace_browser_icon_path: normalizeProjectIconPath(
          nextWorkspaceBrowserIconPath,
          DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
        ),
        terminal_workspace_icon_path: normalizeProjectIconPath(
          nextTerminalWorkspaceIconPath,
          DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
        ),
        theme_mode: normalizeThemeMode(nextThemeMode),
        font_size_tier_1: nextFontSizeTiers[0],
        font_size_tier_2: nextFontSizeTiers[1],
        font_size_tier_3: nextFontSizeTiers[2],
        font_size_tier_4: nextFontSizeTiers[3],
        desktop_remote_url: nextDesktopRemoteUrl,
        desktop_remote_url_history: nextDesktopRemoteUrlHistory,
      }),
    });

    if (workspaceChanged) {
      state.currentPath = "";
      state.currentFilePath = "";
      clearEditor("默认工作目录已更新，请重新选择文件。");
      renderCurrentPath(settings.workspace_dir || "/", settings.workspace_dir || "");
      window.history.replaceState({}, "", buildWorkspaceUrl(""));
    }

    await loadSettings();
    announceSettingsMutation(settings);
    await loadDirectory();
    updateStatus(settingsStatusEl, `设置已保存：${settings.workspace_dir}`, "ok");
    if (terminalUserChanged) {
      updateStatus(
        settingsStatusEl,
        `设置已保存：${settings.workspace_dir}，新终端将使用 ${settings.terminal_user}`,
        "ok",
      );
    } else if (Array.isArray(settings.terminal_quick_commands)) {
      const envCount = Array.isArray(settings.terminal_default_env_vars)
        ? settings.terminal_default_env_vars.length
        : 0;
      const functionCount = Array.isArray(settings.terminal_function_commands)
        ? settings.terminal_function_commands.length
        : 0;
      const slashCount = Array.isArray(settings.terminal_slash_commands)
        ? settings.terminal_slash_commands.length
        : 0;
      const collectionCount = Array.isArray(settings.terminal_command_collections)
        ? settings.terminal_command_collections.length
        : 0;
      updateStatus(
        settingsStatusEl,
        `设置已保存：${settings.workspace_dir}，快捷命令 ${settings.terminal_quick_commands.length} 个，斜杠命令 ${slashCount} 个，功能命令 ${functionCount} 个，命令合集 ${collectionCount} 个，默认环境变量 ${envCount} 个`,
        "ok",
      );
    }
  } catch (error) {
    updateStatus(settingsStatusEl, error.message, "warn");
  } finally {
    saveSettingsButton.disabled = false;
    resetSettingsButton.disabled = false;
  }
}
