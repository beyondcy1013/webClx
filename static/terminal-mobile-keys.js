// webClx 终端移动键盘 / 命令派发子系统：从 terminal.js 抽出，保持全局函数声明。
// 依赖运行时全局：state、term、MOBILE_KEY_SEQUENCES 等 MOBILE_* 常量、
// focusTerminalAfterSoftKeyboardInput/sendTerminalInput 等同模块函数。
// 必须在 terminal.js 之前 <script defer> 加载。

const PROJECT_WEB_CONFIG_FILE = ".webclx.json";

function mobileKeyInputChunks(button) {
  const sequenceKey = button.dataset.sequence;
  if (sequenceKey) {
    const sequence = MOBILE_KEY_SEQUENCES[sequenceKey] || "";
    return sequence ? [sequence] : [];
  }

  const text = button.dataset.text || "";
  if (!text) {
    return [];
  }

  return button.dataset.enter === "true" ? [`${text}${MOBILE_KEY_SEQUENCES.enter}`] : [text];
}

function waitForMobileKeyDelay(delayMs) {
  return new Promise((resolve) => {
    window.setTimeout(resolve, delayMs);
  });
}

function mobileKeyDelayMs(rawValue, fallback = 0) {
  const parsed = Number(rawValue);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return fallback;
  }
  return parsed;
}

function terminalCommandEnterDelayMs(command) {
  if (String(command?.command || "").trim().startsWith("/")) {
    return MOBILE_SLASH_COMMAND_ENTER_DELAY_MS;
  }
  if (command?.action === "send_text") {
    return MOBILE_TEXT_COMMAND_ENTER_DELAY_MS;
  }
  return 0;
}

function sendSlashCommand(command, { enterDelayMs = 0, sessionId = "" } = {}) {
  const commandLine = String(command || "").trim();
  if (!commandLine) {
    return false;
  }
  const targetSessionId = String(sessionId || "").trim();
  const requestedEnterDelayMs = mobileKeyDelayMs(enterDelayMs, 0);
  const effectiveEnterDelayMs =
    requestedEnterDelayMs > 0
      ? requestedEnterDelayMs
      : commandLine.startsWith("/")
        ? MOBILE_SLASH_COMMAND_ENTER_DELAY_MS
        : 0;

  mobileKeySendQueue = mobileKeySendQueue
    .catch(() => {})
    .then(async () => {
      const sendInput = (data) => {
        if (!targetSessionId) {
          sendTerminalInput(data);
          return true;
        }
        return sendTerminalInputToSession(data, targetSessionId);
      };
      if (!sendInput(commandLine)) {
        throw new Error(`终端 ${targetSessionId} 当前不可输入。`);
      }
      if (effectiveEnterDelayMs > 0) {
        await waitForMobileKeyDelay(effectiveEnterDelayMs);
      }
      if (!sendInput(MOBILE_KEY_SEQUENCES.enter)) {
        throw new Error(`终端 ${targetSessionId} 在提交斜杠命令前已断开。`);
      }
      await waitForMobileKeyDelay(MOBILE_SLASH_COMMAND_CONFIRM_DELAY_MS);
      if (!sendInput(MOBILE_KEY_SEQUENCES.enter)) {
        throw new Error(`终端 ${targetSessionId} 在确认斜杠命令前已断开。`);
      }
    });
  focusTerminalAfterSoftKeyboardInput();
  return true;
}

async function runTerminalSlashCommandByKey(key, options = {}) {
  const commandKey = String(key || "").trim();
  const command = state.terminalSlashCommands.find((item) => item.key === commandKey) || null;
  if (!command || command.action !== "send_slash_command") {
    return false;
  }
  if (!runTerminalFunctionCommand(command, options)) {
    return false;
  }
  await mobileKeySendQueue;
  return true;
}

function sendTextCommand(command, { enterDelayMs = MOBILE_TEXT_COMMAND_ENTER_DELAY_MS } = {}) {
  const commandLine = String(command || "").trim();
  if (!commandLine) {
    return false;
  }

  mobileKeySendQueue = mobileKeySendQueue
    .catch(() => {})
    .then(async () => {
      sendTerminalInput(commandLine);
      if (enterDelayMs > 0) {
        await waitForMobileKeyDelay(enterDelayMs);
      }
      sendTerminalInput(MOBILE_KEY_SEQUENCES.enter);
    });
  focusTerminalAfterSoftKeyboardInput();
  return true;
}

function sendContinueCommand(options = {}) {
  return sendTextCommand("继续", options);
}

function sendTerminalEscapeCtrlCInput(sequenceKey) {
  const sequence = MOBILE_KEY_SEQUENCES[sequenceKey] || "";
  if (!sequence) {
    return;
  }

  mobileKeySendQueue = mobileKeySendQueue
    .catch(() => {})
    .then(() => {
      sendTerminalInput(sequence);
    });
  focusTerminalAfterSoftKeyboardInput();
}

function insertTextCommand(command) {
  const commandLine = normalizeTerminalFunctionCommandLine(command, 1000);
  if (!commandLine) {
    return false;
  }

  sendTerminalInput(commandLine);
  focusTerminalAfterSoftKeyboardInput();
  return true;
}

function queueMobileKeyInput(button, chunks) {
  if (!chunks.length) {
    return;
  }

  mobileKeySendQueue = mobileKeySendQueue
    .catch(() => {})
    .then(async () => {
      for (let index = 0; index < chunks.length; index += 1) {
        const chunk = chunks[index];
        if (!chunk) {
          continue;
        }

        sendTerminalInput(chunk);
      }
    });
}

function canRepeatMobileKey(button) {
  return MOBILE_KEY_REPEATABLE_SEQUENCES.has(button?.dataset.sequence || "");
}

function sendMobileKeyChunks(button) {
  const chunks = mobileKeyInputChunks(button);
  if (!chunks.length) {
    return false;
  }

  queueMobileKeyInput(button, chunks);
  return true;
}

function stopMobileKeyRepeat(press = mobileKeyPress) {
  if (!press) {
    return;
  }

  if (press.repeatDelayTimer !== null) {
    window.clearTimeout(press.repeatDelayTimer);
    press.repeatDelayTimer = null;
  }

  if (press.repeatIntervalTimer !== null) {
    window.clearInterval(press.repeatIntervalTimer);
    press.repeatIntervalTimer = null;
  }
}

function stopMobileKeyLongPress(press = mobileKeyPress) {
  if (!press) {
    return;
  }

  if (press.longPressTimer !== null) {
    window.clearTimeout(press.longPressTimer);
    press.longPressTimer = null;
  }
}

function startMobileKeyRepeat(press) {
  if (!press || !canRepeatMobileKey(press.button) || press.repeatDelayTimer !== null) {
    return;
  }

  press.repeatDelayTimer = window.setTimeout(() => {
    press.repeatDelayTimer = null;
    if (!mobileKeyPress || mobileKeyPress.pointerId !== press.pointerId) {
      return;
    }

    press.hasRepeated = sendMobileKeyChunks(press.button);
    if (!press.hasRepeated) {
      return;
    }

    focusTerminalAfterSoftKeyboardInput();
    press.repeatIntervalTimer = window.setInterval(() => {
      if (!mobileKeyPress || mobileKeyPress.pointerId !== press.pointerId) {
        stopMobileKeyRepeat(press);
        return;
      }

      sendMobileKeyChunks(press.button);
    }, MOBILE_KEY_REPEAT_INTERVAL_MS);
  }, MOBILE_KEY_REPEAT_INITIAL_DELAY_MS);
}

function startMobileKeyLongPress(press) {
  if (!press || press.button.dataset.action !== "escape_ctrl_c" || press.longPressTimer !== null) {
    return;
  }

  press.longPressTimer = window.setTimeout(() => {
    press.longPressTimer = null;
    if (!mobileKeyPress || mobileKeyPress.pointerId !== press.pointerId) {
      return;
    }
    press.longPressTriggered = true;
    sendTerminalEscapeCtrlCInput("escape");
    restoreSystemImeAfterSoftKeyboardControl({ target: press.button });
  }, MOBILE_ESCAPE_LONG_PRESS_MS);
}

function triggerMobileKey(button) {
  if (maybeHandleNewSessionQuickStartButton(button)) {
    return;
  }

  if (button.dataset.action === "escape_ctrl_c") {
    sendTerminalEscapeCtrlCInput("ctrl_c");
    return;
  }

  if (button.dataset.action === "toggle_ime") {
    blurTerminalHelperTextarea();
    syncTerminalKeyboardCheckboxes();
    return;
  }

  if (button.dataset.action === "show_system_keyboard" || button.dataset.action === "disable_system_keyboard") {
    runTerminalKeyboardCommand(button.dataset.action);
    return;
  }

  if (button.dataset.action === "slash_command") {
    const command = button.dataset.command || "";
    const enterDelayMs = mobileKeyDelayMs(
      button.dataset.enterDelayMs,
      command.trim().startsWith("/") ? MOBILE_SLASH_COMMAND_ENTER_DELAY_MS : 0,
    );
    sendSlashCommand(command, { enterDelayMs });
    return;
  }

  if (button.dataset.action === "terminal_quick_command") {
    const command = newSessionQuickStartOption(button.dataset.quickKey);
    sendTerminalQuickCommand(command).then((sent) => {
      if (!sent) {
        return;
      }
      updateStatus(`已启动 ${command.label}。`, "ok");
      focusTerminalAfterSoftKeyboardInput();
    });
    return;
  }

  if (button.dataset.action === "open_terminal_tools") {
    toggleTerminalToolsMenu();
    return;
  }

  if (button.dataset.action === "open_schedule_paste") {
    openScheduledTerminalPasteDialog();
    return;
  }

  if (button.dataset.action === "open_command_collections") {
    openTerminalCommandCollectionsMenu();
    return;
  }

  if (button.dataset.action === "open_quota_dialog") {
    openTerminalQuotaDialog();
    return;
  }

  if (button.dataset.action === "extract_resume") {
    injectLatestResumeCommand();
    return;
  }

  if (button.dataset.action === "deploy_project") {
    triggerProjectDeploy();
    return;
  }

  if (button.dataset.action === "copy_all_text") {
    copyTerminalAllText();
    return;
  }

  if (button.dataset.action === "sort_directory_sessions_by_path") {
    runTerminalFunctionCommand({ action: button.dataset.action });
    return;
  }

  const chunks = mobileKeyInputChunks(button);
  if (!chunks.length) {
    return;
  }

  queueMobileKeyInput(button, chunks);
  focusTerminalAfterSoftKeyboardInput();
}

function runTerminalKeyboardCommand(action, { source = null } = {}) {
  const imeAction = terminalImePolicy.terminalImeFunctionAction({ action }, Date.now());
  if (imeAction.kind === "disable") {
    terminalSystemImeSuppressedUntil = imeAction.suppressedUntil;
    setTerminalSystemImeEnabled(false);
    updateStatus("已禁用系统键盘，1 分钟内不会自动弹出。", "ok");
    return true;
  }
  if (imeAction.kind === "show") {
    if (source !== terminalSystemKeyboardCheckboxEl) {
      blurTerminalHelperTextarea();
      syncTerminalKeyboardCheckboxes();
      return false;
    }
    terminalSystemImeSuppressedUntil = 0;
    focusTerminalForDirectInput();
    updateStatus("已恢复系统键盘。", "ok");
    if (typeof syncTerminalKeyboardCheckboxes === "function") {
      syncTerminalKeyboardCheckboxes();
    }
    return true;
  }
  if (typeof syncTerminalKeyboardCheckboxes === "function") {
    syncTerminalKeyboardCheckboxes();
  }
  return false;
}

function runTerminalFunctionCommand(command, options = {}) {
  if (!command) {
    return false;
  }

  cancelNewSessionQuickStart();

  if (runTerminalKeyboardCommand(command.action)) {
    return true;
  }

  if (command.action === "toggle_soft_keyboard") {
    toggleTerminalSoftKeyboard();
    return true;
  }

  if (command.action === "extract_resume") {
    injectLatestResumeCommand();
    return true;
  }

  if (command.action === "extract_current_session") {
    extractCurrentAgentSessionId();
    return true;
  }

  if (command.action === "resume_current_agent_session" || command.action === "resume_current_codex_session") {
    resumeCurrentAgentSession();
    return true;
  }

  if (command.action === "copy_resume_id") {
    copyLatestResumeId();
    return true;
  }

  if (command.action === "copy_current_resume_id") {
    copyCurrentAgentResumeId();
    return true;
  }

  if (command.action === "copy_terminal_name") {
    copyCurrentTerminalName();
    return true;
  }

  if (command.action === "copy_id_and_ask") {
    copyCurrentSessionIdAndAsk();
    return true;
  }

  if (command.action === "open_project_url") {
    openProjectUrl();
    return true;
  }

  if (command.action === "copy_terminal_view_in_new_window") {
    openTerminalVisibleTextCopyWindow();
    return true;
  }

  if (command.action === "reload_claude") {
    sendTerminalAutoTypedInput("claude").then((sent) => {
      if (!sent) {
        return;
      }
      updateStatus("已重读 Claude 当前配置并启动。", "ok");
      focusTerminalAfterSoftKeyboardInput();
    });
    return true;
  }

  if (command.action === "open_artifact_downloads") {
    window.open("/downloads", "_blank", "noopener");
    updateStatus("已打开编译产物下载页。", "ok");
    return true;
  }

  if (command.action === "deploy_project") {
    triggerProjectDeploy();
    return true;
  }

  if (command.action === "open_schedule_paste") {
    openScheduledTerminalPasteDialog();
    return true;
  }

  if (command.action === "open_quota_dialog") {
    openTerminalQuotaDialog();
    return true;
  }

  if (command.action === "disable_touch_selection") {
    terminalTouchSelectionDisabled = true;
    clearTerminalTouchSelectionCandidate();
    endTerminalTouchSelection();
    if (typeof term?.clearSelection === "function") {
      try { term.clearSelection(); } catch (e) {}
    }
    updateStatus("已禁止触摸选词。", "ok");
    if (typeof syncTerminalKeyboardCheckboxes === "function") {
      syncTerminalKeyboardCheckboxes();
    }
    return true;
  }

  if (command.action === "enable_touch_selection") {
    terminalTouchSelectionDisabled = false;
    updateStatus("已允许触摸选词。", "ok");
    if (typeof syncTerminalKeyboardCheckboxes === "function") {
      syncTerminalKeyboardCheckboxes();
    }
    return true;
  }

  if (command.action === "toggle_terminal_width") {
    toggleTerminalWideMode();
    return true;
  }

  if (command.action === "save_and_poweroff") {
    saveAndPoweroff();
    return true;
  }

  if (command.action === "save_and_restart") {
    saveAndRestartService();
    return true;
  }

  if (command.action === "sort_directory_sessions_by_path") {
    const mode = cycleTerminalSessionSortMode();
    const currentLabel = sharedTerminalSessionSortModeLabel(mode);
    const nextLabel = sharedTerminalSessionSortModeLabel(sharedNextTerminalSessionSortMode(mode));
    updateStatus(`已按${currentLabel}排序；再次调用将按${nextLabel}排序。`, "ok");
    return true;
  }

  if (command.action === "sort_directory_sessions_by_status") {
    setTerminalSessionSortMode("status");
    updateStatus("已按状态排序；再次调用切换排序可回到工作区。", "ok");
    return true;
  }

  if (command.action === "send_sequence") {
    if (command.command === "ctrl_v") {
      pasteFromClipboard();
      return true;
    }

    const seq = MOBILE_KEY_SEQUENCES[command.command] || command.command;
    if (seq) {
      sendTerminalInput(seq);
    }
    focusTerminalAfterSoftKeyboardInput();
    return true;
  }

  if (command.key === "continue") {
    sendContinueCommand({ enterDelayMs: options.enterDelayMs });
    return true;
  }

  if (command.action === "send_slash_command" || command.command.startsWith("/")) {
    sendSlashCommand(command.command, {
      enterDelayMs: options.enterDelayMs ?? terminalCommandEnterDelayMs(command),
      sessionId: options.sessionId,
    });
    return true;
  }

  if (command.action === "send_text") {
    sendTextCommand(command.command, { enterDelayMs: options.enterDelayMs ?? terminalCommandEnterDelayMs(command) });
    return true;
  }

  if (command.action === "insert_text") {
    insertTextCommand(command.command);
    return true;
  }

  if (command.command) {
    sendTextCommand(command.command, { enterDelayMs: options.enterDelayMs ?? terminalCommandEnterDelayMs(command) });
    return true;
  }

  return false;
}

function handleTerminalFunctionCommandMenuAction(event) {
  const button = event.target instanceof Element
    ? event.target.closest("button[data-action]")
    : null;
  if (!(button instanceof HTMLButtonElement) || !terminalFunctionCommandMenuEl?.contains(button)) {
    return;
  }
  event.preventDefault();
  if (button.dataset.action === "upload_terminal_image") {
    closeTerminalFunctionCommandMenu();
    openTerminalImageUploadPicker();
    return;
  }
  const command = state.terminalFunctionCommands.find((item) => item.key === button.dataset.key) || null;
  const enterDelayMs = mobileKeyDelayMs(
    button.dataset.enterDelayMs,
    terminalCommandEnterDelayMs(command),
  );
  closeTerminalFunctionCommandMenu();
  if (command) {
    runTerminalFunctionCommand(command, { enterDelayMs });
  } else if (button.dataset.action) {
    runTerminalFunctionCommand({ action: button.dataset.action, command: button.dataset.command || "" });
  }
}

let terminalFunctionCommandMenuActionsBound = false;

function ensureTerminalFunctionCommandMenuActionsBound() {
  if (!terminalFunctionCommandMenuEl || terminalFunctionCommandMenuActionsBound) {
    return;
  }
  terminalFunctionCommandMenuEl.addEventListener("click", handleTerminalFunctionCommandMenuAction);
  terminalFunctionCommandMenuActionsBound = true;
}

function positionTerminalFunctionCommandMenu() {
  if (!terminalFunctionCommandMenuEl || !terminalFunctionCommandButtonEl || terminalFunctionCommandMenuEl.hidden) {
    return;
  }
  const triggerRect = terminalFunctionCommandButtonEl.getBoundingClientRect();
  const availableHeight = Math.max(0, triggerRect.top - 14);
  terminalFunctionCommandMenuEl.style.maxHeight = `${Math.floor(availableHeight)}px`;
  terminalFunctionCommandMenuEl.style.overflowY = "auto";
  const menuRect = terminalFunctionCommandMenuEl.getBoundingClientRect();
  const viewportWidth = document.documentElement.clientWidth || window.innerWidth;
  const maxLeft = Math.max(8, viewportWidth - menuRect.width - 8);
  const left = Math.min(Math.max(triggerRect.left, 8), maxLeft);
  const top = Math.max(8, triggerRect.top - menuRect.height - 6);
  terminalFunctionCommandMenuEl.style.left = `${Math.round(left)}px`;
  terminalFunctionCommandMenuEl.style.top = `${Math.round(top)}px`;
}

function setTerminalFunctionCommandMenuExpanded(expanded) {
  if (!terminalFunctionCommandMenuEl || !terminalFunctionCommandButtonEl) {
    return;
  }
  terminalFunctionCommandMenuEl.hidden = !expanded;
  terminalFunctionCommandButtonEl.setAttribute("aria-expanded", expanded ? "true" : "false");
  if (expanded) {
    positionTerminalFunctionCommandMenu();
    window.requestAnimationFrame(positionTerminalFunctionCommandMenu);
  } else {
    terminalFunctionCommandMenuEl.style.removeProperty("left");
    terminalFunctionCommandMenuEl.style.removeProperty("top");
    terminalFunctionCommandMenuEl.style.removeProperty("max-height");
    terminalFunctionCommandMenuEl.style.removeProperty("overflow-y");
  }
}

function closeTerminalFunctionCommandMenu() {
  setTerminalFunctionCommandMenuExpanded(false);
}

function toggleTerminalFunctionCommandMenu() {
  if (!terminalFunctionCommandMenuEl) {
    return;
  }
  ensureTerminalFunctionCommandMenuActionsBound();
  setTerminalFunctionCommandMenuExpanded(terminalFunctionCommandMenuEl.hidden);
}

// Guard so programmatic state sync does not re-trigger the user action handler.
let syncingTerminalKeyboardCheckboxes = false;

// System-keyboard checkbox: checked = enabled (system IME), unchecked = suppressed for ~1 min.
function handleTerminalSystemKeyboardCheckboxChange() {
  if (syncingTerminalKeyboardCheckboxes || !terminalSystemKeyboardCheckboxEl) {
    return;
  }
  softKeyboardMenuImeFocus = null;
  softKeyboardMenuImeFocusUntil = 0;
  const enabled = terminalSystemKeyboardCheckboxEl.checked;
  runTerminalKeyboardCommand(
    enabled ? "show_system_keyboard" : "disable_system_keyboard",
    { source: terminalSystemKeyboardCheckboxEl },
  );
  syncTerminalKeyboardCheckboxes();
}

// Touch-copy checkbox: checked = touch selection allowed (copy enabled), unchecked = disabled.
function handleTerminalTouchCopyCheckboxChange() {
  if (syncingTerminalKeyboardCheckboxes || !terminalTouchCopyCheckboxEl) {
    return;
  }
  const enabled = terminalTouchCopyCheckboxEl.checked;
  runTerminalFunctionCommand({
    action: enabled ? "enable_touch_selection" : "disable_touch_selection",
  });
  syncTerminalKeyboardCheckboxes();
}

// Reflect the live state into the checkboxes without firing change handlers.
function syncTerminalKeyboardCheckboxes() {
  if (syncingTerminalKeyboardCheckboxes) {
    return;
  }
  syncingTerminalKeyboardCheckboxes = true;
  try {
    if (terminalSystemKeyboardCheckboxEl) {
      terminalSystemKeyboardCheckboxEl.checked = Boolean(terminalSystemImeEnabled);
    }
    if (terminalTouchCopyCheckboxEl) {
      terminalTouchCopyCheckboxEl.checked = !terminalTouchSelectionDisabled;
    }
  } finally {
    syncingTerminalKeyboardCheckboxes = false;
  }
}



function runTerminalProjectCommandAction(action) {
  if (action === "switch_server") {
    openTerminalServerSwitchDialog();
  } else if (action === "deploy_project") {
    triggerProjectDeploy();
  } else if (action === "open_project_url") {
    openProjectUrl();
  } else if (action === "open_artifact_downloads") {
    window.open("/downloads", "_blank", "noopener");
    updateStatus("已打开编译产物下载页。", "ok");
  } else if (action === "codes_backup") {
    insertTextCommand("!codes_backup ");
  } else if (action === "open_agents_doc") {
    openTerminalAgentsDocEditor();
  } else if (action === "open_specified_task") {
    openTerminalSpecifiedTaskDialog();
  } else if (action === "permanent_switch_preset") {
    openTerminalPermanentPresetSwitchDialog();
  } else if (action === "designate_preset_fork") {
    openTerminalDesignatePresetForkDialog();
  } else if (action === "designate_preset_resume") {
    openTerminalDesignatePresetResumeDialog();
  } else if (action === "switch_preset_in_terminal") {
    openTerminalInPlacePresetSwitchDialog(null, { sessionAction: "resume" });
  } else if (action === "switch_preset_in_terminal_new_session") {
    openTerminalInPlacePresetSwitchDialog(null, { sessionAction: "new" });
  } else if (action === "designate_preset_terminal") {
    openTerminalDesignatePresetDialog();
  }
}

function handleTerminalProjectCommandSelectChange() {
  if (!terminalProjectCommandSelectEl) {
    return;
  }
  const action = terminalProjectCommandSelectEl.value;
  terminalProjectCommandSelectEl.value = "";
  runTerminalProjectCommandAction(action);
}

function positionTerminalProjectCommandMenu() {
  if (!terminalProjectCommandMenuEl || !terminalProjectCommandButtonEl || terminalProjectCommandMenuEl.hidden) {
    return;
  }
  const triggerRect = terminalProjectCommandButtonEl.getBoundingClientRect();
  const menuRect = terminalProjectCommandMenuEl.getBoundingClientRect();
  const viewportWidth = document.documentElement.clientWidth || window.innerWidth;
  const viewportHeight = window.innerHeight;
  const left = Math.min(Math.max(triggerRect.left, 8), Math.max(8, viewportWidth - menuRect.width - 8));
  const above = triggerRect.top - menuRect.height - 6;
  const below = triggerRect.bottom + 6;
  const top = above >= 8 ? above : Math.min(Math.max(below, 8), Math.max(8, viewportHeight - menuRect.height - 8));
  terminalProjectCommandMenuEl.style.left = `${Math.round(left)}px`;
  terminalProjectCommandMenuEl.style.top = `${Math.round(top)}px`;
}

function setTerminalProjectCommandMenuExpanded(expanded, { restoreFocus = false } = {}) {
  if (!terminalProjectCommandMenuEl || !terminalProjectCommandButtonEl) {
    return;
  }
  terminalProjectCommandMenuEl.hidden = !expanded;
  terminalProjectCommandButtonEl.setAttribute("aria-expanded", expanded ? "true" : "false");
  if (expanded) {
    positionTerminalProjectCommandMenu();
    window.requestAnimationFrame(positionTerminalProjectCommandMenu);
    return;
  }
  terminalProjectCommandMenuEl.style.removeProperty("left");
  terminalProjectCommandMenuEl.style.removeProperty("top");
  if (restoreFocus) {
    terminalProjectCommandButtonEl.focus({ preventScroll: true });
  }
}

function closeTerminalProjectCommandMenu(options = {}) {
  setTerminalProjectCommandMenuExpanded(false, options);
}

function toggleTerminalProjectCommandMenu() {
  setTerminalProjectCommandMenuExpanded(Boolean(terminalProjectCommandMenuEl?.hidden));
}

function handleTerminalProjectCommandMenuClick(event) {
  const button = event.target instanceof Element
    ? event.target.closest("button[data-project-action]")
    : null;
  if (!(button instanceof HTMLButtonElement)) {
    return;
  }
  event.preventDefault();
  closeTerminalProjectCommandMenu();
  runTerminalProjectCommandAction(button.dataset.projectAction || "");
}

function positionTerminalSlashCommandMenu() {
  if (!terminalSlashCommandMenuEl || !terminalSlashCommandButtonEl || terminalSlashCommandMenuEl.hidden) {
    return;
  }
  const triggerRect = terminalSlashCommandButtonEl.getBoundingClientRect();
  const menuRect = terminalSlashCommandMenuEl.getBoundingClientRect();
  const viewportWidth = document.documentElement.clientWidth || window.innerWidth;
  const viewportHeight = window.innerHeight;
  const left = Math.min(Math.max(triggerRect.left, 8), Math.max(8, viewportWidth - menuRect.width - 8));
  const above = triggerRect.top - menuRect.height - 6;
  const below = triggerRect.bottom + 6;
  const top = above >= 8 ? above : Math.min(Math.max(below, 8), Math.max(8, viewportHeight - menuRect.height - 8));
  terminalSlashCommandMenuEl.style.left = `${Math.round(left)}px`;
  terminalSlashCommandMenuEl.style.top = `${Math.round(top)}px`;
}

function scrollTerminalSlashCommandMenuToBottom() {
  if (!terminalSlashCommandMenuEl || terminalSlashCommandMenuEl.hidden) {
    return;
  }
  terminalSlashCommandMenuEl.scrollTop = terminalSlashCommandMenuEl.scrollHeight;
}

function setTerminalSlashCommandMenuExpanded(expanded, { restoreFocus = false } = {}) {
  if (!terminalSlashCommandMenuEl || !terminalSlashCommandButtonEl) {
    return;
  }
  terminalSlashCommandMenuEl.hidden = !expanded;
  terminalSlashCommandButtonEl.setAttribute("aria-expanded", expanded ? "true" : "false");
  if (expanded) {
    positionTerminalSlashCommandMenu();
    scrollTerminalSlashCommandMenuToBottom();
    window.requestAnimationFrame(() => {
      positionTerminalSlashCommandMenu();
      scrollTerminalSlashCommandMenuToBottom();
    });
    return;
  }
  terminalSlashCommandMenuEl.style.removeProperty("left");
  terminalSlashCommandMenuEl.style.removeProperty("top");
  if (restoreFocus) {
    terminalSlashCommandButtonEl.focus({ preventScroll: true });
  }
}

function closeTerminalSlashCommandMenu(options = {}) {
  setTerminalSlashCommandMenuExpanded(false, options);
}

function toggleTerminalSlashCommandMenu() {
  setTerminalSlashCommandMenuExpanded(Boolean(terminalSlashCommandMenuEl?.hidden));
}

function handleTerminalSlashCommandMenuClick(event) {
  const button = event.target instanceof Element
    ? event.target.closest("button[data-key]")
    : null;
  if (!(button instanceof HTMLButtonElement)) {
    return;
  }

  const key = button.dataset.key || "";
  const command = state.terminalSlashCommands.find((item) => item.key === key) || null;
  const enterDelayMs = mobileKeyDelayMs(
    button.dataset.enterDelayMs,
    terminalCommandEnterDelayMs(command),
  );
  closeTerminalSlashCommandMenu();

  if (!command?.action && !command?.command) {
    return;
  }

  runTerminalFunctionCommand(command, { enterDelayMs });
}

function positionTerminalNumberMenu() {
  if (!terminalNumberMenuEl || !terminalNumberButtonEl || terminalNumberMenuEl.hidden) {
    return;
  }
  const triggerRect = terminalNumberButtonEl.getBoundingClientRect();
  const menuRect = terminalNumberMenuEl.getBoundingClientRect();
  const viewportWidth = document.documentElement.clientWidth || window.innerWidth;
  const viewportHeight = window.innerHeight;
  const left = Math.min(Math.max(triggerRect.left, 8), Math.max(8, viewportWidth - menuRect.width - 8));
  const above = triggerRect.top - menuRect.height - 6;
  const below = triggerRect.bottom + 6;
  const top = above >= 8 ? above : Math.min(Math.max(below, 8), Math.max(8, viewportHeight - menuRect.height - 8));
  terminalNumberMenuEl.style.left = `${Math.round(left)}px`;
  terminalNumberMenuEl.style.top = `${Math.round(top)}px`;
}

function setTerminalNumberMenuExpanded(expanded, { restoreFocus = false } = {}) {
  if (!terminalNumberMenuEl || !terminalNumberButtonEl) {
    return;
  }
  terminalNumberMenuEl.hidden = !expanded;
  terminalNumberButtonEl.setAttribute("aria-expanded", expanded ? "true" : "false");
  if (expanded) {
    positionTerminalNumberMenu();
    window.requestAnimationFrame(positionTerminalNumberMenu);
    return;
  }
  terminalNumberMenuEl.style.removeProperty("left");
  terminalNumberMenuEl.style.removeProperty("top");
  if (restoreFocus) {
    terminalNumberButtonEl.focus({ preventScroll: true });
  }
}

function closeTerminalNumberMenu(options = {}) {
  setTerminalNumberMenuExpanded(false, options);
}

function toggleTerminalNumberMenu() {
  setTerminalNumberMenuExpanded(Boolean(terminalNumberMenuEl?.hidden));
}

function handleTerminalNumberMenuClick(event) {
  const button = event.target instanceof Element
    ? event.target.closest("button[data-digit]")
    : null;
  if (!(button instanceof HTMLButtonElement)) {
    return;
  }

  const digit = button.dataset.digit || "";
  closeTerminalNumberMenu();

  if (!digit) {
    return;
  }

  if (maybeHandleNewSessionQuickStartInput(digit)) {
    return;
  }

  mobileKeySendQueue = mobileKeySendQueue
    .catch(() => {})
    .then(() => {
      sendTerminalInput(digit);
    });
  focusTerminalAfterSoftKeyboardInput();
}

function clearMobileKeyPress() {
  if (!mobileKeyPress) {
    return;
  }

  stopMobileKeyRepeat(mobileKeyPress);
  stopMobileKeyLongPress(mobileKeyPress);

  if (
    typeof mobileKeyPress.button.hasPointerCapture === "function" &&
    mobileKeyPress.button.hasPointerCapture(mobileKeyPress.pointerId)
  ) {
    mobileKeyPress.button.releasePointerCapture(mobileKeyPress.pointerId);
  }
  mobileKeyPress.button.classList.remove("pressed");
  mobileKeyPress = null;
}

function mobileKeyButtonFromEventTarget(target) {
  if (!(target instanceof Element)) {
    return null;
  }

  const button = target.closest(MOBILE_KEY_BUTTON_SELECTOR);
  return button instanceof HTMLButtonElement ? button : null;
}

function mobileKeyButtonFromPoint(clientX, clientY) {
  return mobileKeyButtonFromEventTarget(document.elementFromPoint(clientX, clientY));
}

function prepareMobileKeyControl(button) {
  if (!(button instanceof HTMLButtonElement)) {
    return;
  }

  button.tabIndex = -1;
}

function softKeyboardFocusPreservingControl(target) {
  if (!(target instanceof Element)) {
    return null;
  }
  const label = target.closest("label");
  const labeledCheckbox = label?.querySelector('input[type="checkbox"]') || null;
  const control = labeledCheckbox || target.closest("button, input[type=checkbox]");
  if (!(control instanceof HTMLElement)) {
    return null;
  }
  const surfaces = [
    mobileKeysEl,
    terminalNumberMenuEl,
    terminalSlashCommandMenuEl,
    terminalFunctionCommandMenuEl,
    terminalProjectCommandMenuEl,
    terminalToolsMenuEl,
    terminalToolMenuEl,
    terminalCommandCollectionsMenuEl,
  ];
  if (!surfaces.some((surface) => surface?.contains(control))) {
    return null;
  }
  return control;
}

function explicitSystemImeControl(control) {
  return control === terminalSystemKeyboardCheckboxEl;
}

const softKeyboardImeFocusSnapshots = new WeakMap();
let softKeyboardImeGestureSequence = 0;
let softKeyboardMenuImeFocus = null;
let softKeyboardMenuImeFocusUntil = 0;

function suppressSystemImeForSoftKeyboardControl(event) {
  const control = softKeyboardFocusPreservingControl(event.target);
  if (!control || explicitSystemImeControl(control)) {
    return;
  }
  const isMainSoftKey = Boolean(mobileKeysEl?.contains(control));
  const now = Date.now();
  const helperWasFocused =
    !isMainSoftKey && now < softKeyboardMenuImeFocusUntil && softKeyboardMenuImeFocus !== null
      ? softKeyboardMenuImeFocus
      : terminalHelperTextareaFocused();
  softKeyboardImeGestureSequence += 1;
  if (isMainSoftKey) {
    softKeyboardMenuImeFocus = helperWasFocused;
    softKeyboardMenuImeFocusUntil = now + 1000;
  }
  softKeyboardImeFocusSnapshots.set(control, {
    focused: helperWasFocused,
    sequence: softKeyboardImeGestureSequence,
  });
  event.preventDefault();
  focusTerminalAfterSoftKeyboardInput();
}

function restoreSystemImeAfterSoftKeyboardControl(event) {
  const control = softKeyboardFocusPreservingControl(event.target);
  if (!control || explicitSystemImeControl(control)) {
    return;
  }
  const snapshot = softKeyboardImeFocusSnapshots.get(control);
  if (!snapshot) {
    return;
  }
  const restore = () => {
    if (
      softKeyboardImeFocusSnapshots.get(control) !== snapshot ||
      snapshot.sequence !== softKeyboardImeGestureSequence
    ) {
      return;
    }
    if (!snapshot.focused) {
      blurTerminalHelperTextarea();
    } else {
      syncTerminalImePolicy();
    }
  };
  window.setTimeout(restore, 0);
  window.requestAnimationFrame(restore);
  [80, 180, 320].forEach((delayMs, index, delays) => {
    window.setTimeout(() => {
      restore();
      if (
        index === delays.length - 1 &&
        softKeyboardImeFocusSnapshots.get(control) === snapshot
      ) {
        softKeyboardImeFocusSnapshots.delete(control);
      }
    }, delayMs);
  });
}

function suppressMobileKeyNativeEvent(event) {
  const button = mobileKeyButtonFromEventTarget(event.target);
  if (!button) {
    return null;
  }

  event.preventDefault();
  event.stopPropagation();
  return button;
}

function handleMobileKeyPointerDown(event) {
  suppressSystemImeForSoftKeyboardControl(event);
  focusTerminalAfterSoftKeyboardInput();
  const button = suppressMobileKeyNativeEvent(event);
  if (!button) {
    return;
  }

  if (event.pointerType === "mouse" && event.button !== 0) {
    return;
  }

  clearMobileKeyPress();
  mobileKeyPress = {
    pointerId: event.pointerId,
    button,
    startX: event.clientX,
    startY: event.clientY,
    hasRepeated: false,
    longPressTimer: null,
    longPressTriggered: false,
    repeatDelayTimer: null,
    repeatIntervalTimer: null,
  };
  if (typeof button.setPointerCapture === "function") {
    button.setPointerCapture(event.pointerId);
  }
  button.classList.add("pressed");
  startMobileKeyRepeat(mobileKeyPress);
  startMobileKeyLongPress(mobileKeyPress);
}

function handleMobileKeyPointerMove(event) {
  if (!mobileKeyPress || event.pointerId !== mobileKeyPress.pointerId) {
    return;
  }

  const offsetX = event.clientX - mobileKeyPress.startX;
  const offsetY = event.clientY - mobileKeyPress.startY;
  if (Math.hypot(offsetX, offsetY) >= MOBILE_KEY_DRAG_THRESHOLD) {
    clearMobileKeyPress();
  }
}

function handleMobileKeyPointerEnd(event) {
  if (!mobileKeyPress || event.pointerId !== mobileKeyPress.pointerId) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();

  const button = mobileKeyPress.button;
  const hasRepeated = mobileKeyPress.hasRepeated;
  const longPressTriggered =
    button.dataset.action === "escape_ctrl_c" && mobileKeyPress.longPressTriggered;
  const shouldTrigger =
    event.type === "pointerup" &&
    !hasRepeated &&
    (event.pointerType !== "mouse" || mobileKeyButtonFromPoint(event.clientX, event.clientY) === button);
  clearMobileKeyPress();

  if (!shouldTrigger || longPressTriggered) {
    return;
  }

  if (event.pointerType !== "mouse" && button.dataset.action === "open_terminal_tools") {
    window.setTimeout(() => {
      triggerMobileKey(button);
      restoreSystemImeAfterSoftKeyboardControl({ target: button });
    }, 0);
    return;
  }

  triggerMobileKey(button);
  restoreSystemImeAfterSoftKeyboardControl({ target: button });
}

function handleMobileKeyKeyboardEvent(event) {
  focusTerminalAfterSoftKeyboardInput();
  suppressMobileKeyNativeEvent(event);
}

function handleMobileKeyClick(event) {
  const button = suppressMobileKeyNativeEvent(event);
  if (!button || window.PointerEvent) {
    return;
  }

  focusTerminalAfterSoftKeyboardInput();
  triggerMobileKey(button);
  restoreSystemImeAfterSoftKeyboardControl({ target: button });
}

function handleMobileKeyTouchStart(event) {
  // Passive listener: we no longer call preventDefault here so the browser
  // can start a native horizontal pan-x scroll on the key row immediately.
  // Focus prevention is handled in handleMobileKeyFocusIn instead.
  // The touchstart is still observed to suppress the iOS callout/selection.
  focusTerminalAfterSoftKeyboardInput();
  const button = mobileKeyButtonFromEventTarget(event.target);
  if (!button) {
    return;
  }
  if (window.getSelection) {
    // Clear any stray selection so iOS does not show a callout on long press.
    const selection = window.getSelection();
    if (selection && selection.toString()) {
      selection.removeAllRanges();
    }
  }
}

function handleMobileKeyFocusIn(event) {
  // If focus lands on an editable element or a plain button inside the mobile
  // keys area while the soft keyboard is the active input method, blur it
  // immediately so the system keyboard does not appear. Buttons are included
  // because iOS Safari can leak focus from a touched button to the xterm
  // helper textarea. <select> elements are left alone: iOS opens a native
  // picker rather than the system keyboard when a select receives focus.
  if (!terminalSoftKeyboardVisible()) {
    return;
  }

  const target = event.target;
  if (target === terminalSystemKeyboardCheckboxEl) {
    return;
  }
  if (target === terminalToolsButtonEl && terminalToolsRestoringTriggerFocus) {
    return;
  }
  if (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLButtonElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  ) {
    target.blur();
    const control = softKeyboardFocusPreservingControl(target);
    const snapshot = control ? softKeyboardImeFocusSnapshots.get(control) : null;
    if (!snapshot?.focused) {
      blurTerminalHelperTextarea();
    } else {
      syncTerminalImePolicy();
    }
  }
}



/// Trigger a project deploy through /api/build/deploy.
///
/// Resolves the current terminal's project directory and source terminal
/// identity, then POSTs a minimal deploy payload. The backend auto-detects
/// the correct deploy script (scripts/rebuild-and-deploy.sh, scripts/deploy.sh,
/// etc.) based on the AGENTS.md convention.
async function resolveCurrentDeploySourceTerminal() {
  const terminalId = String(state.activeSessionId || "").trim();
  if (!terminalId) {
    throw new Error("当前没有活动终端，无法发起部署。");
  }

  const response = await requestJson("/api/terminal/sessions?all=true");
  const session = (Array.isArray(response?.sessions) ? response.sessions : []).find(
    (item) => item.id === terminalId,
  );
  const terminalName = String(session?.name || "").trim();
  if (!terminalName) {
    throw new Error("无法从最新会话列表确认当前终端名称，请刷新页面后重试。");
  }

  return {
    id: terminalId,
    name: terminalName,
    tmuxSessionName: `webclx_${terminalId}`,
  };
}

async function triggerProjectDeploy() {
  const relativePath = normalizeRelativePath(state.currentPath || "");
  const workspaceRoot = String(state.workspaceDir || "").replace(/\/+$/, "");
  if (!workspaceRoot) {
    updateStatus("终端设置尚未加载，无法定位项目目录，请稍候再试。", "warn");
    return;
  }
  const projectDir = relativePath ? `${workspaceRoot}/${relativePath}` : workspaceRoot;
  const projectName = relativePath
    ? relativePath.split("/").pop()
    : workspaceRoot.split("/").filter(Boolean).pop() || "";

  updateStatus(`正在确认当前终端并排队部署 ${projectName || "当前项目"}…`, "info");

  try {
    const sourceTerminal = await resolveCurrentDeploySourceTerminal();
    const payload = {
      source_terminal_name: sourceTerminal.name,
      source_terminal_id: sourceTerminal.id,
      source_tmux_session: sourceTerminal.tmuxSessionName,
      project: projectName,
      project_dir: projectDir,
      project_path: relativePath,
      note: "软键盘部署按钮触发",
      // install_command intentionally omitted — backend auto-detects deploy script
    };
    const response = await requestJson("/api/build/deploy", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (response?.queued) {
      const script = Array.isArray(response.install_command)
        ? response.install_command.join(" ")
        : "";
      updateStatus(
        `已由终端 ${sourceTerminal.name} 排队部署 ${response.project || projectName}。部署脚本：${script}`,
        "ok",
      );
    } else {
      updateStatus("部署请求已提交，但状态未知。", "warn");
    }
  } catch (error) {
    updateStatus(error?.message || "部署请求失败。", "warn");
  }
}

function resolveProjectWebUrl(config, locationLike = window.location) {
  const web = config?.web;
  if (!web || typeof web !== "object") {
    return "";
  }

  const configuredUrl = typeof web.url === "string" ? web.url.trim() : "";
  if (configuredUrl) {
    try {
      const resolved = new URL(configuredUrl, locationLike.origin);
      return resolved.protocol === "http:" || resolved.protocol === "https:" ? resolved.href : "";
    } catch {
      return "";
    }
  }

  const port = Number(web.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    return "";
  }
  const scheme = String(web.scheme || locationLike.protocol || "http:")
    .trim()
    .toLowerCase()
    .replace(/:$/, "");
  if (scheme !== "http" && scheme !== "https") {
    return "";
  }
  const path = typeof web.path === "string" && web.path.trim()
    ? `/${web.path.trim().replace(/^\/+/, "")}`
    : "/";
  try {
    return new URL(path, `${scheme}://${locationLike.hostname}:${port}`).href;
  } catch {
    return "";
  }
}

async function openProjectUrl() {
  const relativePath = normalizeRelativePath(state.currentPath || "");
  const configPath = relativePath
    ? `${relativePath}/${PROJECT_WEB_CONFIG_FILE}`
    : PROJECT_WEB_CONFIG_FILE;
  const popup = window.open("", "_blank");
  if (popup) {
    popup.opener = null;
  }

  try {
    const response = await requestJson(`/api/file?path=${encodeURIComponent(configPath)}`);
    let config;
    try {
      config = JSON.parse(response?.content || "");
    } catch {
      throw new Error(`${configPath} 不是有效的 JSON。`);
    }
    const projectUrl = resolveProjectWebUrl(config);
    if (!projectUrl) {
      throw new Error(`${configPath} 需要配置 web.url，或配置有效的 web.port。`);
    }
    if (popup && !popup.closed) {
      popup.location.replace(projectUrl);
    } else {
      window.open(projectUrl, "_blank", "noopener");
    }
    updateStatus(`已打开项目 URL：${projectUrl}`, "ok");
  } catch (error) {
    if (popup && !popup.closed) {
      popup.close();
    }
    const message = String(error?.message || "");
    if (message.includes("不存在") || message.includes("not found")) {
      updateStatus(
        `未找到 ${configPath}。请在项目根目录配置 web.url 或 web.port。`,
        "warn",
      );
    } else {
      updateStatus(message || "无法打开项目 URL。", "warn");
    }
  }
}
