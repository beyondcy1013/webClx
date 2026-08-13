function bindTerminalUserHomeSuggestion() {
  if (!terminalUserSelectEl || !workspaceDirInputEl) {
    return;
  }

  terminalUserSelectEl.addEventListener("change", () => {
    const suggestion = terminalUserHomeSuggestion(
      state.availableUsers,
      terminalUserSelectEl.value,
      workspaceDirInputEl.value,
    );
    if (!suggestion) {
      return;
    }

    const confirmed = window.confirm(
      `已选择用户“${suggestion.name}”，其 HOME 目录为 ${suggestion.home}。\n` +
        `当前默认工作目录为 ${suggestion.currentWorkspaceDir || "（空）"}，是否切换到该用户的 HOME 目录？`,
    );
    if (!confirmed) {
      return;
    }

    workspaceDirInputEl.value = suggestion.home;
    workspaceDirInputEl.scrollLeft = 0;
  });
}

function bindSettingsEventHandlers() {
  bindTerminalUserHomeSuggestion();
  bindTerminalToolSettings();

  fontSettingsOpenButtonEl?.addEventListener("pointerdown", (event) => {
    event.preventDefault();
  });
  fontSettingsOpenButtonEl?.addEventListener("click", () => {
    if (!fontSettingsDialogEl || fontSettingsDialogEl.open) {
      return;
    }
    fontSettingsDialogEl.showModal();
    requestAnimationFrame(() => fontSettingsCloseButtonEl?.focus({ preventScroll: true }));
  });
  fontSettingsCloseButtonEl?.addEventListener("click", () => {
    fontSettingsDialogEl?.close();
    fontSettingsOpenButtonEl?.focus({ preventScroll: true });
  });
  fontSettingsDialogEl?.addEventListener("close", () => {
    updateFontSettingsSummary();
    fontSettingsOpenButtonEl?.focus({ preventScroll: true });
  });
  updateFontSettingsSummary();

  function refreshSessionViewsAfterPageResume() {
    if (document.visibilityState === "hidden") {
      return;
    }
    scheduleSessionViewsRefresh({ preserveCurrentList: true, silentSessionsStatus: true });
  }

  window.addEventListener("pageshow", () => {
    refreshSessionViewsAfterPageResume();
    refreshPasteScheduledTasksIfVisible();
  });
  window.addEventListener("online", refreshSessionViewsAfterPageResume);
  window.addEventListener("focus", () => {
    refreshSessionViewsAfterPageResume();
    refreshPasteScheduledTasksIfVisible();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      refreshSessionViewsAfterPageResume();
      refreshPasteScheduledTasksIfVisible();
    }
  });

  codexDefaultConfigAddButtonEl?.addEventListener("click", () => {
    addCodexDefaultConfigRow();
  });

  claudeDefaultConfigAddButtonEl?.addEventListener("click", () => {
    addClaudeDefaultConfigRow();
  });

  claudeDefaultConfigResetButtonEl?.addEventListener("click", () => {
    renderClaudeDefaultConfigEntries(cloneDefaultClaudeDefaultConfigEntries());
    updateStatus(claudeDefaultConfigStatusEl, "已恢复内置值，保存后生效。", "info");
  });

  claudeDefaultConfigSaveButtonEl?.addEventListener("click", () => {
    saveClaudeDefaultConfigEntries();
  });

  codexDefaultConfigResetButtonEl?.addEventListener("click", () => {
    renderCodexDefaultConfigEntries(cloneDefaultCodexDefaultConfigEntries());
    updateStatus(codexDefaultConfigStatusEl, "已恢复内置值，保存后生效。", "info");
  });

  codexDefaultConfigSaveButtonEl?.addEventListener("click", () => {
    saveCodexDefaultConfigEntries();
  });

  codexCommonConfigRefreshButtonEl?.addEventListener("click", () => {
    loadCodexCommonConfig();
  });

  codexCommonConfigSaveButtonEl?.addEventListener("click", () => {
    saveCodexCommonConfig();
  });

  terminalErrorKeywordAddBtnEl?.addEventListener("click", () => {
    addTerminalErrorKeywordRule();
  });

  codexDefaultConfigListEl?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button || button.dataset.action !== "delete-codex-default-config") {
      return;
    }
    button.closest(".codex-default-config-row")?.remove();
    if (!codexDefaultConfigListEl.querySelector(".codex-default-config-row")) {
      renderCodexDefaultConfigEntries(cloneDefaultCodexDefaultConfigEntries());
    }
  });

  claudeDefaultConfigListEl?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button || button.dataset.action !== "delete-claude-default-config") {
      return;
    }
    button.closest(".claude-default-config-row")?.remove();
    if (!claudeDefaultConfigListEl.querySelector(".claude-default-config-row")) {
      renderClaudeDefaultConfigEntries(cloneDefaultClaudeDefaultConfigEntries());
    }
  });

  window.addEventListener("storage", (event) => {
    if (event.key !== SESSION_EVENT_STORAGE_KEY || !event.newValue) {
      return;
    }

    const mutation = parseSessionMutationEvent(event.newValue);
    if (!shouldRefreshForSessionMutation(mutation?.action)) {
      return;
    }

    scheduleSessionViewsRefresh();
  });

  window.addEventListener("storage", (event) => {
    if (event.key !== RESUME_ARCHIVE_EVENT_STORAGE_KEY || !event.newValue) {
      return;
    }
    if (state.activeTab === "terminal-archives") {
      loadTerminalArchives();
    }
  });

  window.addEventListener("storage", (event) => {
    if (event.key !== THEME_MODE_STORAGE_KEY) {
      return;
    }
    applyThemeMode(event.newValue || DEFAULT_THEME_MODE);
  });

  saveSettingsButton.addEventListener("click", () => {
    if (!commitTerminalQuickCommandEditor({ silent: true, allowEmpty: true })) {
      return;
    }
    commitTerminalShortcutInputs();
    const nextTerminalQuickCommands = readSanitizedTerminalQuickCommandsFromInputs();
    const nextTerminalDefaultEnvVars = parseTerminalDefaultEnvInput(terminalDefaultEnvInputEl?.value || "");
    const nextTerminalSlashCommands = parseTerminalFunctionCommandsInput(
      terminalSlashCommandsInputEl?.value || "",
    );
    const nextTerminalFunctionCommands = parseTerminalFunctionCommandsInput(
      terminalFunctionCommandsInputEl?.value || "",
    );
    const nextTerminalCommandCollections = normalizeTerminalCommandCollections(
      Array.isArray(state.terminalCommandCollections) ? state.terminalCommandCollections : [],
      null,
    );
    const nextTerminalToolEntries = normalizeTerminalToolEntries(state.terminalToolEntries);
    const nextTerminalRenamePresets = readTerminalRenamePresetsFromInput();
    const nextCodexDefaultConfigEntries = readCodexDefaultConfigEntriesFromTable();
    const nextLegacyCodexConfigEntries = normalizeCodexDefaultConfigEntries(
      nextCodexDefaultConfigEntries,
    );
    const nextPrimaryCodexConfig = nextLegacyCodexConfigEntries[0] || {};
    const nextSecondaryCodexConfig = nextLegacyCodexConfigEntries[1] || {};
    saveSettings(
      workspaceDirInputEl.value,
      terminalUserSelectEl.value,
      nextTerminalQuickCommands,
      normalizeTerminalQuickStartDefaultKey(
        terminalQuickStartDefaultSelectEl?.value || "",
        nextTerminalQuickCommands,
      ),
      nextTerminalDefaultEnvVars,
      nextTerminalSlashCommands,
      nextTerminalFunctionCommands,
      nextTerminalCommandCollections,
      nextTerminalToolEntries,
      nextTerminalRenamePresets,
      showDotEntriesInputEl.checked,
      showAllWorkspaceSessionsInputEl.checked,
      desktopTerminalSoftKeyboardInputEl.checked,
      readTerminalSoftKeyboardScaleFromInput(),
      readTerminalFloatingButtonOffsetVhFromInput(),
      readTerminalFabActionColorFromInput(),
      readTerminalFabActionOpacityFromInput(),
      terminalFabAutoExpandInputEl.checked,
      readTerminalTouchSelectionLongPressMsFromInput(),
      normalizeTerminalScrollbackLines(terminalScrollbackLinesInputEl?.value),
      readTerminalErrorMatchLineLimitFromInput(),
      readTerminalAutoContinueIntervalSecondsFromInput(),
      readTerminalAutoContinueBackoffFactorFromInput(),
      readTerminalAutoContinueBackoffMaxMinutesFromInput(),
      terminalAutoContinueRespectManualInterruptInputEl?.checked !== false,
      parseTerminalAutoContinueTimePatternsInput(terminalAutoContinueTimePatternsInputEl?.value || ""),
      normalizeTerminalAutoContinueActiveWindow(terminalAutoContinueActiveWindowInputEl?.value || ""),
      normalizeTerminalScheduledInputAvoidWindow(terminalScheduledInputAvoidWindowInputEl?.value || ""),
      collectTerminalErrorKeywordsFromTable(),
      normalizeTerminalActivityAgentDisplay(terminalActivityAgentDisplaySelectEl?.value),
      terminalCompletionBellEnabledInputEl?.checked !== false,
      Boolean(serverPortAutoIncrementInputEl?.checked),
      (() => {
        const v = Number(compileCommandTimeoutInputEl?.value);
        return Number.isFinite(v) ? Math.min(3600, Math.max(60, Math.trunc(v))) : DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS;
      })(),
      (() => {
        const v = Number(compileMaxConcurrencyInputEl?.value);
        return Number.isFinite(v) ? Math.min(32, Math.max(1, Math.trunc(v))) : DEFAULT_COMPILE_MAX_CONCURRENCY;
      })(),
      (() => {
        const v = Number(sessionTtlDaysInputEl?.value);
        return Number.isFinite(v) ? Math.min(365, Math.max(1, Math.trunc(v))) : DEFAULT_SESSION_TTL_DAYS;
      })(),
      claudeManager.parseClaudeModelOptionsInput(claudeModelOptionsInputEl.value),
      nextCodexDefaultConfigEntries,
      readCodexApiAutoProxyMatchProviderIds(),
      nextPrimaryCodexConfig.key || DEFAULT_CODEX_CONFIG_KEY,
      nextPrimaryCodexConfig.value || DEFAULT_CODEX_MODEL,
      nextSecondaryCodexConfig.key || DEFAULT_CODEX_SECONDARY_CONFIG_KEY,
      nextSecondaryCodexConfig.value || DEFAULT_CODEX_SECONDARY_CONFIG_VALUE,
      showFullPathInputEl?.checked !== false,
      workspaceBrowserIconPathInputEl?.value || DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
      terminalWorkspaceIconPathInputEl?.value || DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
      readThemeModeFromInputs(),
      readFontSizeTiersFromInputs(),
      normalizeDesktopRemoteUrl(desktopUrlInputEl?.value || ""),
      normalizeDesktopRemoteUrlHistory(state.desktopRemoteUrlHistory),
    );
  });

  resetSettingsButton.addEventListener("click", () => {
    workspaceDirInputEl.value = state.defaultWorkspaceDir;
    renderTerminalUserOptions(state.availableUsers, state.defaultTerminalUser);
    renderTerminalQuickCommands(
      state.defaultTerminalQuickCommands,
      state.defaultTerminalQuickStartDefaultKey,
    );
    if (terminalDefaultEnvInputEl) {
      terminalDefaultEnvInputEl.value = formatTerminalDefaultEnvVars(state.defaultTerminalDefaultEnvVars);
    }
    if (terminalFunctionCommandsInputEl) {
      terminalFunctionCommandsInputEl.value = formatTerminalFunctionCommands(
        state.defaultTerminalFunctionCommands,
      );
    }
    if (terminalSlashCommandsInputEl) {
      terminalSlashCommandsInputEl.value = formatTerminalFunctionCommands(
        state.defaultTerminalSlashCommands,
      );
    }
    state.terminalFunctionCommands = normalizeTerminalFunctionCommands(
      state.defaultTerminalFunctionCommands,
      null,
    );
    state.terminalSlashCommands = ensureBuiltInTerminalSlashCommands(state.defaultTerminalSlashCommands);
    resetTerminalCommandCollectionsToDefaults();
    renderTerminalShortcutSettings();
    renderTerminalRenamePresetsSetting(state.defaultTerminalRenamePresets);
    clearTerminalQuickCommandEditor();
    showDotEntriesInputEl.checked = false;
    if (workspaceShowHiddenInputEl) {
      workspaceShowHiddenInputEl.checked = false;
    }
    showAllWorkspaceSessionsInputEl.checked = true;
    desktopTerminalSoftKeyboardInputEl.checked = true;
    if (terminalFabAutoExpandInputEl) {
      terminalFabAutoExpandInputEl.checked = true;
    }
    if (terminalSoftKeyboardScaleInputEl) {
      terminalSoftKeyboardScaleInputEl.value = formatTerminalSoftKeyboardScale(
        DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE,
      );
    }
    if (terminalFloatingButtonOffsetInputEl) {
      terminalFloatingButtonOffsetInputEl.value = formatTerminalFloatingButtonOffsetVh(
        DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
      );
    }
    if (terminalFabActionColorInputEl) {
      terminalFabActionColorInputEl.value = DEFAULT_TERMINAL_FAB_ACTION_COLOR;
    }
    if (terminalFabActionOpacityInputEl) {
      terminalFabActionOpacityInputEl.value = String(DEFAULT_TERMINAL_FAB_ACTION_OPACITY);
      renderTerminalFabActionOpacityOutput(DEFAULT_TERMINAL_FAB_ACTION_OPACITY);
    }
    if (terminalTouchSelectionLongPressInputEl) {
      terminalTouchSelectionLongPressInputEl.value = formatTerminalTouchSelectionLongPressMs(
        DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
      );
    }
    if (terminalScrollbackLinesInputEl) {
      terminalScrollbackLinesInputEl.value = String(DEFAULT_TERMINAL_SCROLLBACK_LINES);
    }
    if (terminalErrorLineLimitInputEl) {
      terminalErrorLineLimitInputEl.value = formatTerminalErrorMatchLineLimit(
        DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT,
      );
    }
    if (terminalAutoContinueIntervalInputEl) {
      terminalAutoContinueIntervalInputEl.value = formatTerminalAutoContinueIntervalSeconds(
        DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
      );
    }
    if (terminalAutoContinueBackoffFactorInputEl) {
      terminalAutoContinueBackoffFactorInputEl.value = formatTerminalAutoContinueBackoffFactor(
        DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR,
      );
    }
    if (terminalAutoContinueBackoffMaxMinutesInputEl) {
      terminalAutoContinueBackoffMaxMinutesInputEl.value =
        formatTerminalAutoContinueBackoffMaxMinutes(
          DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES,
        );
    }
    if (terminalAutoContinueRespectManualInterruptInputEl) {
      terminalAutoContinueRespectManualInterruptInputEl.checked =
        DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT;
    }
    if (terminalAutoContinueTimePatternsInputEl) {
      terminalAutoContinueTimePatternsInputEl.value = formatTerminalAutoContinueTimePatterns(
        DEFAULT_TERMINAL_AUTO_CONTINUE_TIME_PATTERNS,
      );
    }
    if (terminalAutoContinueActiveWindowInputEl) {
      terminalAutoContinueActiveWindowInputEl.value = "";
    }
    if (terminalScheduledInputAvoidWindowInputEl) {
      terminalScheduledInputAvoidWindowInputEl.value = DEFAULT_TERMINAL_SCHEDULED_INPUT_AVOID_WINDOW;
    }
    state.terminalErrorKeywordActions = DEFAULT_TERMINAL_ERROR_KEYWORD_ACTIONS.map((action) => ({ ...action }));
    renderTerminalErrorKeywordRulesTable();
    if (terminalActivityAgentDisplaySelectEl) {
      terminalActivityAgentDisplaySelectEl.value = DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY;
    }
    if (terminalCompletionBellEnabledInputEl) {
      terminalCompletionBellEnabledInputEl.checked = DEFAULT_TERMINAL_COMPLETION_BELL_ENABLED;
    }
    if (serverPortAutoIncrementInputEl) {
      serverPortAutoIncrementInputEl.checked = DEFAULT_SERVER_PORT_AUTO_INCREMENT;
    }
    if (compileCommandTimeoutInputEl) {
      compileCommandTimeoutInputEl.value = String(DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS);
    }
    if (compileMaxConcurrencyInputEl) {
      compileMaxConcurrencyInputEl.value = String(DEFAULT_COMPILE_MAX_CONCURRENCY);
    }
    if (compileEnvironmentInputEl) {
      compileEnvironmentInputEl.value = formatCompileEnvironment(DEFAULT_COMPILE_ENVIRONMENT);
    }
    if (sessionTtlDaysInputEl) {
      sessionTtlDaysInputEl.value = String(DEFAULT_SESSION_TTL_DAYS);
    }
    claudeModelOptionsInputEl.value = DEFAULT_CLAUDE_MODEL_OPTIONS.join("\n");
    renderClaudeDefaultConfigEntries(cloneDefaultClaudeDefaultConfigEntries());
    renderCodexApiAutoProxyMatchProviders(DEFAULT_CODEX_API_AUTO_PROXY_MATCH_PROVIDER_IDS);
    renderCodexDefaultConfigEntries(cloneDefaultCodexDefaultConfigEntries());
    if (showFullPathInputEl) {
      showFullPathInputEl.checked = DEFAULT_SHOW_FULL_PATH;
    }
    if (workspaceBrowserIconPathInputEl) {
      workspaceBrowserIconPathInputEl.value = DEFAULT_WORKSPACE_BROWSER_ICON_PATH;
    }
    if (terminalWorkspaceIconPathInputEl) {
      terminalWorkspaceIconPathInputEl.value = DEFAULT_TERMINAL_WORKSPACE_ICON_PATH;
    }
    setThemeModeInputs(DEFAULT_THEME_MODE);
    if (fontSizeTier1InputEl) {
      fontSizeTier1InputEl.value = formatFontSizeTier(DEFAULT_FONT_SIZE_TIER_1, DEFAULT_FONT_SIZE_TIER_1);
    }
    if (fontSizeTier2InputEl) {
      fontSizeTier2InputEl.value = formatFontSizeTier(DEFAULT_FONT_SIZE_TIER_2, DEFAULT_FONT_SIZE_TIER_2);
    }
    if (fontSizeTier3InputEl) {
      fontSizeTier3InputEl.value = formatFontSizeTier(DEFAULT_FONT_SIZE_TIER_3, DEFAULT_FONT_SIZE_TIER_3);
    }
    if (fontSizeTier4InputEl) {
      fontSizeTier4InputEl.value = formatFontSizeTier(DEFAULT_FONT_SIZE_TIER_4, DEFAULT_FONT_SIZE_TIER_4);
    }
    applyThemeMode(DEFAULT_THEME_MODE);
    applyTypographySettings(DEFAULT_FONT_SIZE_TIERS);
    updateFontSettingsSummary();
    saveSettings(
      state.defaultWorkspaceDir,
      state.defaultTerminalUser,
      state.defaultTerminalQuickCommands,
      state.defaultTerminalQuickStartDefaultKey,
      state.defaultTerminalDefaultEnvVars,
      state.defaultTerminalSlashCommands,
      state.defaultTerminalFunctionCommands,
      state.defaultTerminalCommandCollections,
      state.defaultTerminalToolEntries,
      state.defaultTerminalRenamePresets,
      true,
      true,
      false,
      DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE,
      DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
      DEFAULT_TERMINAL_FAB_ACTION_COLOR,
      DEFAULT_TERMINAL_FAB_ACTION_OPACITY,
      true,
      DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
      DEFAULT_TERMINAL_SCROLLBACK_LINES,
      DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT,
      DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
      DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR,
      DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES,
      DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT,
      [...DEFAULT_TERMINAL_AUTO_CONTINUE_TIME_PATTERNS],
      "",
      "",
      [...DEFAULT_TERMINAL_ERROR_KEYWORDS],
      DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY,
      DEFAULT_TERMINAL_COMPLETION_BELL_ENABLED,
      DEFAULT_SERVER_PORT_AUTO_INCREMENT,
      DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS,
      DEFAULT_COMPILE_MAX_CONCURRENCY,
      DEFAULT_SESSION_TTL_DAYS,
      [...DEFAULT_CLAUDE_MODEL_OPTIONS],
      cloneDefaultCodexDefaultConfigEntries(),
      [...DEFAULT_CODEX_API_AUTO_PROXY_MATCH_PROVIDER_IDS],
      DEFAULT_CODEX_CONFIG_KEY,
      DEFAULT_CODEX_MODEL,
      DEFAULT_CODEX_SECONDARY_CONFIG_KEY,
      DEFAULT_CODEX_SECONDARY_CONFIG_VALUE,
      DEFAULT_SHOW_FULL_PATH,
      DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
      DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
      DEFAULT_THEME_MODE,
      [...DEFAULT_FONT_SIZE_TIERS],
      DEFAULT_DESKTOP_REMOTE_URL,
      [],
    );
  });

  if (terminalCompletionBellTestButtonEl) {
    terminalCompletionBellTestButtonEl.addEventListener("click", playTerminalCompletionBellTest);
  }
}
