// webClx 终端设置表单格式化/读写子系统（第一部分）：从 app.js 抽出，保持全局函数声明。
// 包含 normalizeTerminal*/format*/read*FromInputs 主题/字体/错误匹配/自动继续等
// 设置项格式化与表单读写，以及 quickCommand 编辑器/渲染函数。
// 只含函数声明，无顶层执行代码。顶层 applyThemeMode(state.themeMode) 调用保留在 app.js。
// 必须在 app.js 之前 <script defer> 加载。

function normalizeTerminalErrorMatchLineLimit(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT;
  }
  return Math.round(Math.min(1000, Math.max(1, parsed)));
}

function formatTerminalErrorMatchLineLimit(value) {
  return String(normalizeTerminalErrorMatchLineLimit(value));
}

function readTerminalErrorMatchLineLimitFromInput() {
  return normalizeTerminalErrorMatchLineLimit(
    terminalErrorLineLimitInputEl?.value || DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT,
  );
}

function formatTerminalAutoContinueIntervalSeconds(value) {
  return String(normalizeTerminalAutoContinueIntervalSeconds(value));
}

function readTerminalAutoContinueIntervalSecondsFromInput() {
  return normalizeTerminalAutoContinueIntervalSeconds(
    terminalAutoContinueIntervalInputEl?.value || DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
  );
}

function formatTerminalAutoContinueBackoffFactor(value) {
  return String(normalizeTerminalAutoContinueBackoffFactor(value));
}

function readTerminalAutoContinueBackoffFactorFromInput() {
  return normalizeTerminalAutoContinueBackoffFactor(
    terminalAutoContinueBackoffFactorInputEl?.value || DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR,
  );
}

function formatTerminalAutoContinueBackoffMaxMinutes(value) {
  return String(normalizeTerminalAutoContinueBackoffMaxMinutes(value));
}

function readTerminalAutoContinueBackoffMaxMinutesFromInput() {
  return normalizeTerminalAutoContinueBackoffMaxMinutes(
    terminalAutoContinueBackoffMaxMinutesInputEl?.value ||
      DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES,
  );
}

function formatTerminalSoftKeyboardScale(value) {
  return String(normalizeTerminalSoftKeyboardScale(value));
}

function readTerminalSoftKeyboardScaleFromInput() {
  return normalizeTerminalSoftKeyboardScale(
    terminalSoftKeyboardScaleInputEl?.value || DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE,
  );
}

function formatTerminalFloatingButtonOffsetVh(value) {
  return String(normalizeTerminalFloatingButtonOffsetVh(value));
}

function readTerminalFloatingButtonOffsetVhFromInput() {
  return normalizeTerminalFloatingButtonOffsetVh(
    terminalFloatingButtonOffsetInputEl?.value || DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
  );
}

function readTerminalFabActionColorFromInput() {
  return normalizeTerminalFabActionColor(
    terminalFabActionColorInputEl?.value || DEFAULT_TERMINAL_FAB_ACTION_COLOR,
  );
}

function readTerminalFabActionOpacityFromInput() {
  return normalizeTerminalFabActionOpacity(
    terminalFabActionOpacityInputEl?.value || DEFAULT_TERMINAL_FAB_ACTION_OPACITY,
  );
}

function renderTerminalFabActionOpacityOutput(value) {
  if (terminalFabActionOpacityOutputEl) {
    terminalFabActionOpacityOutputEl.value = `${Math.round(
      normalizeTerminalFabActionOpacity(value) * 100,
    )}%`;
  }
}

function formatTerminalTouchSelectionLongPressMs(value) {
  return String(normalizeTerminalTouchSelectionLongPressMs(value));
}

function readTerminalTouchSelectionLongPressMsFromInput() {
  return normalizeTerminalTouchSelectionLongPressMs(
    terminalTouchSelectionLongPressInputEl?.value || DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
  );
}

function formatFontSizeTier(value, fallback) {
  return String(normalizeFontSizeTier(value, fallback));
}

function readFontSizeTiersFromInputs() {
  return normalizeFontSizeTiers([
    fontSizeTier1InputEl?.value || DEFAULT_FONT_SIZE_TIER_1,
    fontSizeTier2InputEl?.value || DEFAULT_FONT_SIZE_TIER_2,
    fontSizeTier3InputEl?.value || DEFAULT_FONT_SIZE_TIER_3,
    fontSizeTier4InputEl?.value || DEFAULT_FONT_SIZE_TIER_4,
  ]);
}

function normalizeTerminalAutoContinueTimePatterns(patterns) {
  const seen = new Set();
  const normalized = [];
  (Array.isArray(patterns) ? patterns : []).forEach((pattern) => {
    const value = String(pattern || "").split(/\s+/).filter(Boolean).join(" ").slice(0, 200);
    const key = value.toLowerCase();
    if (!value || !value.includes("{time}") || seen.has(key)) {
      return;
    }
    seen.add(key);
    normalized.push(value);
  });
  return normalized.length ? normalized : [...DEFAULT_TERMINAL_AUTO_CONTINUE_TIME_PATTERNS];
}

function parseTerminalAutoContinueTimePatternsInput(value) {
  return normalizeTerminalAutoContinueTimePatterns(String(value || "").split(/\r?\n/));
}

function formatTerminalAutoContinueTimePatterns(patterns) {
  return normalizeTerminalAutoContinueTimePatterns(patterns).join("\n");
}

// Normalize an unattended active-window string into `HH:MM-HH:MM` (24h).
// Empty / malformed values become "" (feature disabled).
function normalizeTerminalAutoContinueActiveWindow(value) {
  const raw = String(value || "").trim();
  if (!raw) return "";
  const dash = raw.indexOf("-");
  if (dash <= 0) return "";
  const start = raw.slice(0, dash).trim();
  const end = raw.slice(dash + 1).trim();
  if (!isValidHhMm(start) || !isValidHhMm(end)) return "";
  return `${start}-${end}`;
}

// Normalize a scheduled-input avoid-window string into `HH:MM-HH:MM` (24h).
// Same rules as the active window: empty / malformed → "" (disabled).
function normalizeTerminalScheduledInputAvoidWindow(value) {
  return normalizeTerminalAutoContinueActiveWindow(value);
}

function isValidHhMm(value) {
  const match = /^(\d{2}):(\d{2})$/.exec(value);
  if (!match) return false;
  const h = Number(match[1]);
  const m = Number(match[2]);
  return h >= 0 && h < 24 && m >= 0 && m < 60;
}

function normalizeTerminalErrorKeywords(keywords) {
  const seen = new Set();
  const normalized = [];
  (Array.isArray(keywords) ? keywords : []).forEach((keyword) => {
    const value = String(keyword || "").split(/\s+/).filter(Boolean).join(" ").slice(0, 200);
    const key = value.toLowerCase();
    if (!value || seen.has(key)) {
      return;
    }
    seen.add(key);
    normalized.push(value);
  });
  return normalized;
}

function parseTerminalErrorKeywordsInput(value) {
  return normalizeTerminalErrorKeywords(String(value || "").split(/\r?\n/));
}

function formatTerminalErrorKeywords(keywords) {
  return normalizeTerminalErrorKeywords(keywords).join("\n");
}

function normalizeTerminalErrorKeywordActions(actions) {
  const seen = new Set();
  const normalized = [];
  (Array.isArray(actions) ? actions : []).forEach((entry) => {
    const keyword = String(entry?.keyword || entry || "")
      .split(/\s+/)
      .filter(Boolean)
      .join(" ")
      .slice(0, 200);
    const key = keyword.toLowerCase();
    if (!keyword || seen.has(key)) {
      return;
    }
    seen.add(key);
    const action = normalizeTerminalErrorKeywordAction(entry?.action);
    normalized.push({ keyword, action });
  });
  return normalized;
}

function normalizeTerminalErrorKeywordAction(action) {
  const value = String(action || "").trim();
  if (
    value === TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE ||
    value === TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY
  ) {
    return value;
  }
  return TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE;
}

function persistThemeMode(themeMode) {
  try {
    window.localStorage.setItem(THEME_MODE_STORAGE_KEY, normalizeThemeMode(themeMode));
  } catch {
    // Ignore storage errors and keep the runtime theme active.
  }
}

function setThemeModeInputs(themeMode = state.themeMode) {
  const normalized = normalizeThemeMode(themeMode);
  if (themeModeSelectEl) {
    themeModeSelectEl.value = normalized;
  }
}

function readThemeModeFromInputs() {
  return normalizeThemeMode(themeModeSelectEl?.value || state.themeMode);
}

function applyThemeMode(themeMode = state.themeMode, { persist = false } = {}) {
  const normalized = normalizeThemeMode(themeMode);
  const effective = resolveThemeMode(normalized);
  state.themeMode = normalized;
  document.documentElement.dataset.theme = effective;
  document.documentElement.style.colorScheme = effective;
  setThemeModeInputs(normalized);
  if (persist) {
    persistThemeMode(normalized);
  }
  return normalized;
}

function applyTypographySettings(fontSizeTiers = state.fontSizeTiers) {
  const tiers = normalizeFontSizeTiers(fontSizeTiers);
  state.fontSizeTiers = tiers;
  const rootStyle = document.documentElement.style;
  rootStyle.setProperty("--font-size-tier-1", `${tiers[0]}rem`);
  rootStyle.setProperty("--font-size-tier-2", `${tiers[1]}rem`);
  rootStyle.setProperty("--font-size-tier-3", `${tiers[2]}rem`);
  rootStyle.setProperty("--font-size-tier-4", `${tiers[3]}rem`);
}

function updateFontSettingsSummary() {
  if (!fontSettingsSummaryEl) {
    return;
  }
  fontSettingsSummaryEl.textContent = readFontSizeTiersFromInputs()
    .map((value) => formatFontSizeTier(value, value))
    .join(" / ") + " rem";
}

function readTerminalQuickCommandsFromInputs() {
  return state.terminalQuickCommands.map((command) => ({ ...command }));
}

function readSanitizedTerminalQuickCommandsFromInputs() {
  return normalizeTerminalQuickCommands(readTerminalQuickCommandsFromInputs(), null);
}

function renderTerminalRenamePresetsSetting(presets) {
  if (terminalRenamePresetsInputEl) {
    terminalRenamePresetsInputEl.value = formatTerminalRenamePresets(presets);
  }
}

function readTerminalRenamePresetsFromInput() {
  return parseTerminalRenamePresetsInput(terminalRenamePresetsInputEl?.value || "");
}

function normalizeAvailableUsers(users, selectedUser = DEFAULT_TERMINAL_USER) {
  const selected = normalizeTerminalUser(selectedUser);
  const normalized = [];
  const seen = new Set();
  (Array.isArray(users) ? users : []).forEach((user) => {
    const name = normalizeTerminalUser(user?.name);
    if (!name || seen.has(name)) {
      return;
    }
    seen.add(name);
    normalized.push({
      name,
      uid: Number.isFinite(Number(user?.uid)) ? Number(user.uid) : null,
      gid: Number.isFinite(Number(user?.gid)) ? Number(user.gid) : null,
      home: typeof user?.home === "string" ? user.home.trim() : "",
      shell: typeof user?.shell === "string" ? user.shell.trim() : "",
    });
  });

  if (!seen.has(selected)) {
    normalized.push({
      name: selected,
      uid: null,
      gid: null,
      home: "",
      shell: "",
    });
  }
  return normalized;
}

function terminalUserHomeSuggestion(users, selectedUser, currentWorkspaceDir) {
  const selected = normalizeTerminalUser(selectedUser);
  const profile = normalizeAvailableUsers(users, selected).find((user) => user.name === selected);
  const home = profile?.home || "";
  const current = String(currentWorkspaceDir || "").trim();
  const comparablePath = (value) => {
    const trimmed = String(value || "").trim();
    return trimmed.replace(/[\\/]+$/, "") || trimmed.charAt(0);
  };

  if (!home || comparablePath(home) === comparablePath(current)) {
    return null;
  }

  return { name: selected, home, currentWorkspaceDir: current };
}

function renderTerminalUserOptions(users, selectedUser = state.terminalUser) {
  if (!terminalUserSelectEl) {
    return;
  }

  const selected = normalizeTerminalUser(selectedUser);
  const normalized = normalizeAvailableUsers(users, selected);
  terminalUserSelectEl.textContent = "";
  normalized.forEach((user) => {
    const option = document.createElement("option");
    option.value = user.name;
    option.textContent = user.name;
    option.title = [user.home, user.shell].filter(Boolean).join(" | ");
    terminalUserSelectEl.appendChild(option);
  });
  terminalUserSelectEl.value = normalized.some((user) => user.name === selected)
    ? selected
    : DEFAULT_TERMINAL_USER;
}

function syncTerminalQuickStartDefaultOptions(preferredValue = terminalQuickStartDefaultSelectEl?.value) {
  if (!terminalQuickStartDefaultSelectEl) {
    return;
  }

  const previousValue = preferredValue || state.terminalQuickStartDefaultKey || "";
  const commands = readSanitizedTerminalQuickCommandsFromInputs();
  terminalQuickStartDefaultSelectEl.textContent = "";

  const disabledOption = document.createElement("option");
  disabledOption.value = "";
  disabledOption.textContent = "不自动启动";
  terminalQuickStartDefaultSelectEl.appendChild(disabledOption);

  commands.forEach((command) => {
    const option = document.createElement("option");
    option.value = command.key;
    option.textContent = `${command.key} - ${terminalQuickCommandDisplay(command)}`;
    terminalQuickStartDefaultSelectEl.appendChild(option);
  });

  terminalQuickStartDefaultSelectEl.value = commands.some((command) => command.key === previousValue)
    ? previousValue
    : "";
}

function terminalQuickCommandEditorValue() {
  return {
    key: terminalQuickCommandKeyInputEl?.value || "",
    label: terminalQuickCommandLabelInputEl?.value || "",
    command: terminalQuickCommandCommandInputEl?.value || "",
  };
}

function terminalQuickCommandEditorHasContent() {
  return Object.values(terminalQuickCommandEditorValue()).some((value) =>
    String(value || "").trim(),
  );
}

function markTerminalQuickCommandEditingRow() {
  terminalQuickCommandsListEl
    ?.querySelectorAll("[data-terminal-quick-row]")
    .forEach((row) => {
      const index = Number(row.dataset.index);
      row.classList.toggle("is-editing", index === state.terminalQuickEditingIndex);
    });
}

function setTerminalQuickCommandEditor(command = null, index = -1) {
  const normalizedIndex = Number.isInteger(index) && index >= 0 ? index : -1;
  state.terminalQuickEditingIndex =
    normalizedIndex < state.terminalQuickCommands.length ? normalizedIndex : -1;

  if (terminalQuickCommandEditIndexEl) {
    terminalQuickCommandEditIndexEl.value =
      state.terminalQuickEditingIndex >= 0 ? String(state.terminalQuickEditingIndex) : "";
  }
  if (terminalQuickCommandKeyInputEl) {
    terminalQuickCommandKeyInputEl.value = command?.key || "";
  }
  if (terminalQuickCommandLabelInputEl) {
    terminalQuickCommandLabelInputEl.value = command?.label || "";
  }
  if (terminalQuickCommandCommandInputEl) {
    terminalQuickCommandCommandInputEl.value = command
      ? command.command || terminalQuickCommandDisplay(command)
      : "";
  }
  if (terminalQuickCommandEditingLabelEl) {
    terminalQuickCommandEditingLabelEl.textContent =
      state.terminalQuickEditingIndex >= 0
        ? `正在编辑：${terminalQuickCommandDisplay(command)}`
        : "新增快捷命令";
  }

  markTerminalQuickCommandEditingRow();
}

function clearTerminalQuickCommandEditor() {
  state.terminalQuickEditingIndex = -1;
  if (terminalQuickCommandEditIndexEl) {
    terminalQuickCommandEditIndexEl.value = "";
  }
  if (terminalQuickCommandKeyInputEl) {
    terminalQuickCommandKeyInputEl.value = "";
  }
  if (terminalQuickCommandLabelInputEl) {
    terminalQuickCommandLabelInputEl.value = "";
  }
  if (terminalQuickCommandCommandInputEl) {
    terminalQuickCommandCommandInputEl.value = "";
  }
  if (terminalQuickCommandEditingLabelEl) {
    terminalQuickCommandEditingLabelEl.textContent = "未选择";
  }
  markTerminalQuickCommandEditingRow();
}

function commitTerminalQuickCommandEditor({ silent = false, allowEmpty = true } = {}) {
  const editingIndex = state.terminalQuickEditingIndex;
  if (allowEmpty && editingIndex < 0 && !terminalQuickCommandEditorHasContent()) {
    return true;
  }

  const nextCommand = normalizeTerminalQuickCommand(terminalQuickCommandEditorValue());
  if (!nextCommand) {
    updateStatus(settingsStatusEl, "快捷命令需要填写有效的按钮和命令；按钮不能包含空格。", "warn");
    focusTextInputToEnd(terminalQuickCommandCommandInputEl);
    return false;
  }

  const commands = readTerminalQuickCommandsFromInputs();
  const existingIndex = commands.findIndex((command) => command.key === nextCommand.key);
  if (existingIndex >= 0 && existingIndex !== editingIndex) {
    updateStatus(settingsStatusEl, `按钮 ${nextCommand.key} 已存在，请换一个按钮值。`, "warn");
    focusTextInputToEnd(terminalQuickCommandKeyInputEl);
    return false;
  }

  if (editingIndex >= 0 && editingIndex < commands.length) {
    const previousKey = commands[editingIndex].key;
    commands[editingIndex] = nextCommand;
    const defaultKey =
      terminalQuickStartDefaultSelectEl?.value === previousKey
        ? nextCommand.key
        : terminalQuickStartDefaultSelectEl?.value || "";
    renderTerminalQuickCommands(commands, defaultKey);
    setTerminalQuickCommandEditor(nextCommand, editingIndex);
  } else {
    if (commands.length >= MAX_TERMINAL_QUICK_COMMANDS) {
      updateStatus(settingsStatusEl, `最多只能添加 ${MAX_TERMINAL_QUICK_COMMANDS} 个快捷命令。`, "warn");
      return false;
    }
    commands.push(nextCommand);
    renderTerminalQuickCommands(commands, terminalQuickStartDefaultSelectEl?.value || "");
    setTerminalQuickCommandEditor(nextCommand, commands.length - 1);
  }

  if (!silent) {
    updateStatus(settingsStatusEl, "快捷命令已更新到列表；点击“保存设置”后才会持久化。", "muted");
  }
  return true;
}

function editTerminalQuickCommand(index) {
  const command = state.terminalQuickCommands[index];
  if (!command) {
    clearTerminalQuickCommandEditor();
    return;
  }
  setTerminalQuickCommandEditor(command, index);
  focusTextInputToEnd(terminalQuickCommandCommandInputEl);
}

function renderTerminalQuickCommands(commands, defaultKey) {
  if (!terminalQuickCommandsListEl) {
    return;
  }

  const normalized = normalizeTerminalQuickCommands(commands, null);
  const normalizedDefaultKey = normalizeTerminalQuickStartDefaultKey(defaultKey, normalized);
  terminalQuickCommandsListEl.textContent = "";

  if (!normalized.length) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 6;
    cell.className = "settings-value muted-text";
    cell.textContent = "没有快捷命令。";
    row.appendChild(cell);
    terminalQuickCommandsListEl.appendChild(row);
  }

  normalized.forEach((command, index) => {
    const row = document.createElement("tr");
    row.dataset.terminalQuickRow = "1";
    row.dataset.index = String(index);

    const defaultCell = document.createElement("td");
    const defaultLabel = document.createElement("label");
    defaultLabel.className = "terminal-quick-default-choice";
    defaultLabel.dataset.action = "set-terminal-quick-default";
    defaultLabel.dataset.index = String(index);
    defaultLabel.dataset.quickKey = command.key;
    defaultLabel.title = `3 秒未选择时默认启动 ${terminalQuickCommandDisplay(command)}`;
    const defaultInput = document.createElement("input");
    defaultInput.type = "radio";
    defaultInput.name = "terminal-quick-start-default-row";
    defaultInput.value = command.key;
    defaultInput.checked = command.key === normalizedDefaultKey;
    defaultInput.setAttribute(
      "aria-label",
      `设为默认启动：${command.key} ${terminalQuickCommandDisplay(command)}`,
    );
    defaultLabel.appendChild(defaultInput);
    defaultCell.appendChild(defaultLabel);
    row.appendChild(defaultCell);

    const editCell = document.createElement("td");
    const editButton = document.createElement("button");
    editButton.className = "button secondary terminal-quick-command-edit";
    editButton.type = "button";
    editButton.dataset.action = "edit-terminal-quick-command";
    editButton.dataset.index = String(index);
    editButton.textContent = "编辑";
    editCell.appendChild(editButton);
    row.appendChild(editCell);

    [
      ["key", command.key],
      ["label", command.label],
      ["command", command.command],
    ].forEach(([field, value]) => {
      const cell = document.createElement("td");
      const text = document.createElement("span");
      text.className = "terminal-quick-command-value mono-text";
      text.dataset.terminalQuickField = field;
      text.textContent = value;
      text.title = value;
      cell.appendChild(text);
      row.appendChild(cell);
    });

    const deleteCell = document.createElement("td");
    const deleteButton = document.createElement("button");
    deleteButton.className = "button secondary danger terminal-quick-command-delete";
    deleteButton.type = "button";
    deleteButton.dataset.action = "delete-terminal-quick-command";
    deleteButton.dataset.index = String(index);
    deleteButton.textContent = "删除";
    deleteCell.appendChild(deleteButton);
    row.appendChild(deleteCell);

    terminalQuickCommandsListEl.appendChild(row);
  });

  state.terminalQuickCommands = normalized;
  state.terminalQuickStartDefaultKey = normalizedDefaultKey;
  if (terminalQuickStartDefaultSelectEl) {
    terminalQuickStartDefaultSelectEl.value = state.terminalQuickStartDefaultKey;
    syncTerminalQuickStartDefaultOptions(state.terminalQuickStartDefaultKey);
    terminalQuickStartDefaultSelectEl.value = state.terminalQuickStartDefaultKey;
  }
  if (state.terminalQuickEditingIndex >= normalized.length) {
    clearTerminalQuickCommandEditor();
  } else {
    markTerminalQuickCommandEditingRow();
  }
}

function nextTerminalQuickCommandKey(commands) {
  const used = new Set(commands.map((command) => command.key));
  for (let index = 1; index <= 9; index += 1) {
    const key = String(index);
    if (!used.has(key)) {
      return key;
    }
  }
  for (let code = 97; code <= 122; code += 1) {
    const key = String.fromCharCode(code);
    if (!used.has(key)) {
      return key;
    }
  }
  return String(commands.length + 1);
}

function addTerminalQuickCommandRow() {
  const commands = readTerminalQuickCommandsFromInputs();
  if (commands.length >= MAX_TERMINAL_QUICK_COMMANDS) {
    updateStatus(settingsStatusEl, `最多只能添加 ${MAX_TERMINAL_QUICK_COMMANDS} 个快捷命令。`, "warn");
    return;
  }

  const key = nextTerminalQuickCommandKey(normalizeTerminalQuickCommands(commands, null));
  setTerminalQuickCommandEditor(
    {
      key,
      label: `命令${key}`,
      command: "codex",
    },
    -1,
  );
  focusTextInputToEnd(terminalQuickCommandCommandInputEl);
}
