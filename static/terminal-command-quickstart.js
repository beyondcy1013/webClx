// webClx terminal command menus and new-session quick-start helpers.
// Extracted from terminal.js as global declarations; no top-level DOM setup.

const NEW_SESSION_QUICK_START_KEY_CONFIRM_DELAY_MS = 120;

function newSessionQuickStartOption(key) {
  const normalizedKey = normalizeTerminalQuickText(key, 8);
  if (!normalizedKey) {
    return null;
  }
  return state.terminalQuickCommands.find((command) => command.key === normalizedKey) || null;
}

function newSessionQuickStartDefaultOption() {
  return newSessionQuickStartOption(state.terminalQuickStartDefaultKey);
}

async function preparedTerminalQuickCommandInput(commandLine) {
  const normalized = normalizeTerminalQuickText(commandLine, 1000);
  if (!normalized) {
    return "";
  }
  try {
    const response = await requestJson("/api/terminal/quick-command", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        command_line: normalized,
        session_id: state.activeSessionId || "",
      }),
    });
    return typeof response?.data === "string" && response.data.trim() ? response.data : `${normalized}\n`;
  } catch (error) {
    console.warn("prepare terminal quick command failed", error);
    return `${normalized}\n`;
  }
}

function terminalQuickCommandLine(command) {
  return (
    normalizeTerminalQuickText(command?.commandLine, 1000)
    || normalizeTerminalQuickText(command?.command, 1000)
  );
}

async function sendTerminalQuickCommand(command) {
  const commandLine = terminalQuickCommandLine(command);
  if (!commandLine) {
    return false;
  }
  const input = await preparedTerminalQuickCommandInput(commandLine);
  if (!input) {
    return false;
  }
  sendTerminalInput(input, { flush: true });
  return true;
}

/// Atomically resolve + build + send a command that webClx itself is
/// injecting (initial session launch, quick-start, `reload_claude`,
/// resume-command extraction). The line reaches the terminal pane but
/// is NOT recorded in the "本终端对话历史" panel — see the server-side
/// `send_auto_typed_input` handler.
async function sendTerminalAutoTypedInput(command, options = {}) {
  const rawCommand = typeof command === "string"
    ? command.replace(/\u0000/g, "").replace(/\r\n?/g, "\n").trim()
    : "";
  const commandLine = terminalQuickCommandLine(command) || rawCommand;
  if (!commandLine) {
    return false;
  }
  if (commandLine.length > 262144) {
    throw new Error("自动输入命令过长，最多允许 262144 个字符。");
  }
  const submitEnters = Math.min(
    4,
    Math.max(0, Number(options?.submitEnters) || 0),
  );
  const targetSessionId = options?.sessionId || state.activeSessionId || "";
  try {
    const response = await requestJson("/api/terminal/auto-typed-input", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        command_line: commandLine,
        session_id: targetSessionId,
        submit_enters: submitEnters,
      }),
    });
    return typeof response?.data === "string" && response.data.length > 0;
  } catch (error) {
    console.warn("send auto-typed terminal input failed", error);
    if (options?.throwOnError) {
      throw error;
    }
    return false;
  }
}

function formatNewSessionQuickStartPrompt() {
  if (!state.terminalQuickCommands.length) {
    return "";
  }

  const choices = state.terminalQuickCommands
    .map((command) => `按 ${command.key} 启动 ${command.label}`)
    .join("，");
  const defaultOption = newSessionQuickStartDefaultOption();
  if (!defaultOption) {
    return `新终端已创建：${choices}。`;
  }
  const seconds = Math.round(NEW_SESSION_QUICK_START_TIMEOUT_MS / 1000);
  return `新终端已创建：${choices}；${seconds} 秒后默认启动 ${defaultOption.label}。`;
}

function renderTerminalQuickCommandButtons() {
  if (!terminalQuickCommandButtonsEl) {
    return;
  }

  terminalQuickCommandButtonsEl.textContent = "";
  state.terminalQuickCommands.forEach((command) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "terminal-tools-action";
    button.dataset.action = "terminal_quick_command";
    button.dataset.quickKey = command.key;
    button.title = terminalQuickCommandLine(command);
    button.textContent = command.label || command.key;
    button.setAttribute("role", "menuitem");
    terminalQuickCommandButtonsEl.appendChild(button);
  });
}

function renderTerminalFunctionCommandOptions() {
  if (!terminalFunctionCommandButtonsEl) {
    return;
  }

  terminalFunctionCommandButtonsEl.textContent = "";

  const checkboxActions = (typeof TERMINAL_KEYBOARD_CHECKBOX_ACTIONS !== "undefined"
    && TERMINAL_KEYBOARD_CHECKBOX_ACTIONS) || null;
  state.terminalFunctionCommands.forEach((command) => {
    // Toggle-style commands are surfaced as menu checkboxes, not buttons.
    if (checkboxActions?.has(command.action)) {
      return;
    }
    const button = document.createElement("button");
    button.type = "button";
    button.className = "terminal-tools-action";
    button.dataset.key = command.key;
    button.dataset.action = command.action || "";
    button.dataset.command = command.command || "";
    button.textContent = command.label;
    button.title = command.shortcut ? `${command.label} (${command.shortcut})` : command.label;
    prepareMobileKeyControl(button);
    const enterDelayMs = terminalCommandEnterDelayMs(command);
    if (enterDelayMs > 0) {
      button.dataset.enterDelayMs = String(enterDelayMs);
    }
    terminalFunctionCommandButtonsEl.appendChild(button);
  });

  if (typeof syncTerminalKeyboardCheckboxes === "function") {
    syncTerminalKeyboardCheckboxes();
  }
}

function renderTerminalSlashCommandMenu() {
  if (!terminalSlashCommandMenuEl || !terminalSlashCommandButtonEl) {
    return;
  }

  terminalSlashCommandMenuEl.textContent = "";
  state.terminalSlashCommands.forEach((command) => {
    const button = document.createElement("button");
    button.type = "button";
    button.dataset.key = command.key;
    button.dataset.action = command.action || "";
    button.dataset.command = command.command || "";
    button.textContent = command.label;
    button.title = command.shortcut ? `${command.label} (${command.shortcut})` : command.label;
    button.setAttribute("role", "menuitem");
    const enterDelayMs = terminalCommandEnterDelayMs(command);
    if (enterDelayMs > 0) {
      button.dataset.enterDelayMs = String(enterDelayMs);
    }
    terminalSlashCommandMenuEl.appendChild(button);
  });
  terminalSlashCommandButtonEl.disabled = state.terminalSlashCommands.length === 0;
}

function renderTerminalCommandCollectionsButton() {
  if (!terminalCommandCollectionsBtnEl) {
    return;
  }
  const totalCommands = (state.terminalCommandCollections || []).reduce(
    (sum, collection) => sum + (Array.isArray(collection.commands) ? collection.commands.length : 0),
    0,
  );
  if (totalCommands > 0) {
    terminalCommandCollectionsBtnEl.disabled = false;
    terminalCommandCollectionsBtnEl.title = `维护命令（${totalCommands} 条）`;
    terminalCommandCollectionsBtnEl.setAttribute("aria-disabled", "false");
  } else {
    terminalCommandCollectionsBtnEl.disabled = true;
    terminalCommandCollectionsBtnEl.title = "未配置维护命令（可在设置 → 软键盘命令 中添加）";
    terminalCommandCollectionsBtnEl.setAttribute("aria-disabled", "true");
  }
}

function renderTerminalCommandCollectionsBody() {
  if (!terminalCommandCollectionsBodyEl) {
    return;
  }
  const collections = state.terminalCommandCollections || [];
  if (collections.length === 0) {
    terminalCommandCollectionsBodyEl.replaceChildren();
    const empty = document.createElement("p");
    empty.className = "meta-text";
    empty.textContent = "未配置命令合集。可在设置 → 软键盘命令 中添加。";
    terminalCommandCollectionsBodyEl.appendChild(empty);
    return;
  }

  terminalCommandCollectionsBodyEl.replaceChildren();
  collections.forEach((collection) => {
    const section = document.createElement("section");
    section.className = "terminal-command-collection-group";
    section.dataset.collectionKey = collection.key;

    const heading = document.createElement("h3");
    heading.className = "terminal-command-collection-title";
    heading.textContent = collection.label || collection.key;
    section.appendChild(heading);

    if (!Array.isArray(collection.commands) || collection.commands.length === 0) {
      const empty = document.createElement("p");
      empty.className = "meta-text";
      empty.textContent = "（空合集）";
      section.appendChild(empty);
      terminalCommandCollectionsBodyEl.appendChild(section);
      return;
    }

    const grid = document.createElement("div");
    grid.className = "terminal-command-collection-grid";
    collection.commands.forEach((item, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "terminal-command-collection-item";
      button.dataset.collectionKey = collection.key;
      button.dataset.commandIndex = String(index);
      button.title = item.command || item.label;
      const labelEl = document.createElement("span");
      labelEl.className = "terminal-command-collection-item-label";
      labelEl.textContent = item.label || item.command;
      const cmdEl = document.createElement("span");
      cmdEl.className = "terminal-command-collection-item-command mono-text";
      cmdEl.textContent = item.command || "";
      button.appendChild(labelEl);
      button.appendChild(cmdEl);
      grid.appendChild(button);
    });
    section.appendChild(grid);
    terminalCommandCollectionsBodyEl.appendChild(section);
  });
}

function positionTerminalCommandCollectionsMenu() {
  if (!terminalCommandCollectionsMenuEl || !terminalCommandCollectionsBtnEl || terminalCommandCollectionsMenuEl.hidden) {
    return;
  }
  const triggerRect = terminalCommandCollectionsBtnEl.getBoundingClientRect();
  const menuRect = terminalCommandCollectionsMenuEl.getBoundingClientRect();
  const viewportWidth = document.documentElement.clientWidth || window.innerWidth;
  const viewportHeight = window.innerHeight;
  const maxLeft = Math.max(8, viewportWidth - menuRect.width - 8);
  const left = Math.min(Math.max(triggerRect.left, 8), maxLeft);
  const above = triggerRect.top - menuRect.height - 6;
  const below = triggerRect.bottom + 6;
  const maxTop = Math.max(8, viewportHeight - menuRect.height - 8);
  const top = above >= 8 ? above : Math.min(Math.max(below, 8), maxTop);
  terminalCommandCollectionsMenuEl.style.left = `${Math.round(left)}px`;
  terminalCommandCollectionsMenuEl.style.top = `${Math.round(top)}px`;
}

function setTerminalCommandCollectionsMenuExpanded(expanded) {
  if (!terminalCommandCollectionsMenuEl || !terminalCommandCollectionsBtnEl) {
    return;
  }
  terminalCommandCollectionsMenuEl.hidden = !expanded;
  terminalCommandCollectionsBtnEl.setAttribute("aria-expanded", expanded ? "true" : "false");
  if (expanded) {
    positionTerminalCommandCollectionsMenu();
    window.requestAnimationFrame(positionTerminalCommandCollectionsMenu);
  } else {
    terminalCommandCollectionsMenuEl.style.removeProperty("left");
    terminalCommandCollectionsMenuEl.style.removeProperty("top");
  }
}

function openTerminalCommandCollectionsMenu() {
  if (!terminalCommandCollectionsMenuEl) {
    return;
  }
  renderTerminalCommandCollectionsBody();
  setTerminalCommandCollectionsMenuExpanded(true);
}

function closeTerminalCommandCollectionsMenu() {
  setTerminalCommandCollectionsMenuExpanded(false);
  focusTerminalSoon();
}

function toggleTerminalCommandCollectionsMenu() {
  if (!terminalCommandCollectionsMenuEl) {
    return;
  }
  if (terminalCommandCollectionsMenuEl.hidden) {
    openTerminalCommandCollectionsMenu();
  } else {
    closeTerminalCommandCollectionsMenu();
  }
}

function handleTerminalCommandCollectionsBodyClick(event) {
  const button = event.target.closest("button.terminal-command-collection-item");
  if (!button) {
    return;
  }
  const collectionKey = button.dataset.collectionKey;
  const index = Number(button.dataset.commandIndex);
  const collection = (state.terminalCommandCollections || []).find(
    (item) => item.key === collectionKey,
  );
  const item = collection?.commands?.[index];
  if (!item) {
    return;
  }
  closeTerminalCommandCollectionsMenu();
  // Reuse the existing function-command entry point: a collection item is just
  // { action, command } without a key/label/shortcut, which is all the
  // dispatcher needs to send_text or insert_text.
  runTerminalFunctionCommand({ action: item.action || "send_text", command: item.command });
}

function clearNewSessionQuickStartTimer(quickStart = pendingNewSessionQuickStart) {
  if (!quickStart || quickStart.timer === null) {
    return;
  }

  window.clearTimeout(quickStart.timer);
  quickStart.timer = null;
}

function clearNewSessionQuickStartInputTimer(quickStart = pendingNewSessionQuickStart) {
  if (!quickStart || quickStart.inputTimer === null) {
    return;
  }

  window.clearTimeout(quickStart.inputTimer);
  quickStart.inputTimer = null;
}

function cancelNewSessionQuickStart() {
  clearNewSessionQuickStartTimer();
  clearNewSessionQuickStartInputTimer();
  pendingNewSessionQuickStart = null;
}

function scheduleNewSessionQuickStart(quickStart = pendingNewSessionQuickStart) {
  if (
    !quickStart
    || pendingNewSessionQuickStart !== quickStart
    || quickStart.timer !== null
    || !newSessionQuickStartDefaultOption()
  ) {
    return;
  }

  const remainingMs = Math.max(0, quickStart.deadlineAt - Date.now());
  quickStart.timer = window.setTimeout(() => {
    if (pendingNewSessionQuickStart !== quickStart) {
      return;
    }
    quickStart.timer = null;
    if (Date.now() < quickStart.deadlineAt) {
      scheduleNewSessionQuickStart(quickStart);
      return;
    }

    const defaultOption = newSessionQuickStartDefaultOption();
    if (defaultOption) {
      runNewSessionQuickStart(defaultOption.key, { automatic: true });
    }
  }, remainingMs);
}

function flushNewSessionQuickStartInput(quickStart = pendingNewSessionQuickStart) {
  if (
    !quickStart
    || pendingNewSessionQuickStart !== quickStart
    || !quickStart.manualInput
    || !quickStart.inputBuffer
  ) {
    return false;
  }

  const input = quickStart.inputBuffer;
  quickStart.inputBuffer = "";
  quickStart.manualInput = false;
  cancelNewSessionQuickStart();
  sendTerminalInput(input, { flush: true });
  return true;
}

function runNewSessionQuickStart(choiceKey, { automatic = false } = {}) {
  const quickStart = pendingNewSessionQuickStart;
  const option = newSessionQuickStartOption(choiceKey);
  if (!quickStart || quickStart.sessionId !== state.activeSessionId || !option) {
    return false;
  }

  clearNewSessionQuickStartInputTimer(quickStart);
  quickStart.inputBuffer = "";
  quickStart.manualInput = false;

  if (automatic) {
    const targetSessionId = quickStart.sessionId;
    cancelNewSessionQuickStart();
    sendTerminalAutoTypedInput(option, { sessionId: targetSessionId }).then((sent) => {
      if (!sent) {
        return;
      }
      updateStatus(
        `${Math.round(NEW_SESSION_QUICK_START_TIMEOUT_MS / 1000)} 秒未选择，已自动启动 ${option.label}。`,
        "ok",
      );
      focusTerminalSoon();
    });
    return true;
  }

  clearNewSessionQuickStartTimer(quickStart);

  if (!isTerminalConnected()) {
    quickStart.choiceKey = String(choiceKey);
    updateStatus(`已选择 ${option.label}，终端连接后自动启动。`, "info");
    return true;
  }
  if (!terminalInitialReplaySettled()) {
    quickStart.choiceKey = String(choiceKey);
    updateStatus(`已选择 ${option.label}，终端同步完成后自动启动。`, "info");
    return true;
  }

  cancelNewSessionQuickStart();
  sendTerminalAutoTypedInput(option).then((sent) => {
    if (!sent) {
      return;
    }
    updateStatus(`已启动 ${option.label}。`, "ok");
    focusTerminalSoon();
  });
  return true;
}

function activateNewSessionQuickStart() {
  const quickStart = pendingNewSessionQuickStart;
  if (!quickStart || quickStart.sessionId !== state.activeSessionId || !isTerminalConnected()) {
    return;
  }

  if (quickStart.manualInput) {
    flushNewSessionQuickStartInput(quickStart);
    return;
  }
  if (quickStart.choiceKey) {
    runNewSessionQuickStart(quickStart.choiceKey);
    return;
  }

  const prompt = formatNewSessionQuickStartPrompt();
  if (prompt) {
    updateStatus(prompt, "info");
  }

}

function armNewSessionQuickStart(sessionId) {
  cancelNewSessionQuickStart();
  if (!sessionId || !state.terminalQuickCommands.length) {
    return;
  }

  pendingNewSessionQuickStart = {
    sessionId,
    timer: null,
    deadlineAt: Date.now() + NEW_SESSION_QUICK_START_TIMEOUT_MS,
    choiceKey: "",
    inputTimer: null,
    inputBuffer: "",
    manualInput: false,
  };

  scheduleNewSessionQuickStart();
  activateNewSessionQuickStart();
}

function maybeHandleNewSessionQuickStartButton(button) {
  const quickStart = pendingNewSessionQuickStart;
  if (!quickStart || quickStart.sessionId !== state.activeSessionId || !button) {
    return false;
  }

  const choiceKey = String(button.dataset.quickKey || button.dataset.text || "").trim();
  if (newSessionQuickStartOption(choiceKey)) {
    return runNewSessionQuickStart(choiceKey);
  }

  if (button.dataset.action === "toggle_ime") {
    return false;
  }

  const willSendInput =
    button.dataset.action === "terminal_quick_command" ||
    button.dataset.action === "function_command" ||
    button.dataset.action === "slash_command" ||
    button.dataset.action === "extract_resume" ||
    mobileKeyInputChunks(button).length > 0;

  if (willSendInput) {
    cancelNewSessionQuickStart();
  }

  return false;
}

function maybeHandleNewSessionQuickStartInput(data) {
  const quickStart = pendingNewSessionQuickStart;
  if (!quickStart || quickStart.sessionId !== state.activeSessionId || !data) {
    return false;
  }

  clearNewSessionQuickStartTimer(quickStart);

  if (quickStart.manualInput) {
    quickStart.inputBuffer += String(data);
    if (terminalInitialReplaySettled()) {
      flushNewSessionQuickStartInput(quickStart);
    }
    return true;
  }

  if (quickStart.inputBuffer) {
    clearNewSessionQuickStartInputTimer(quickStart);
    quickStart.manualInput = true;
    quickStart.inputBuffer += String(data);
    if (terminalInitialReplaySettled()) {
      flushNewSessionQuickStartInput(quickStart);
    }
    return true;
  }

  if (newSessionQuickStartOption(data)) {
    quickStart.inputBuffer = String(data);
    quickStart.inputTimer = window.setTimeout(() => {
      if (pendingNewSessionQuickStart !== quickStart) {
        return;
      }
      quickStart.inputTimer = null;
      const choiceKey = quickStart.inputBuffer;
      quickStart.inputBuffer = "";
      runNewSessionQuickStart(choiceKey);
    }, NEW_SESSION_QUICK_START_KEY_CONFIRM_DELAY_MS);
    return true;
  }

  if (!terminalInitialReplaySettled()) {
    quickStart.manualInput = true;
    quickStart.inputBuffer = String(data);
    return true;
  }

  cancelNewSessionQuickStart();
  return false;
}
