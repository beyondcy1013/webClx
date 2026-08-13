// webClx terminal session selection, rename, and history UI helpers.
// Extracted from terminal.js as global declarations; no top-level DOM setup.

function activeSession() {
  return state.sessions.find((session) => session.id === state.activeSessionId) || null;
}

function renamingSession() {
  return state.sessions.find((session) => session.id === state.renamingSessionId) || null;
}

let terminalRenameTriggerEl = null;

function setSessionRenameControlsDisabled(disabled) {
  if (!sessionRenameFormEl) {
    return;
  }

  sessionRenameFormEl.querySelectorAll("input, button").forEach((element) => {
    element.disabled = disabled;
  });
}

function renderSessionRenamePresets() {
  if (!sessionRenamePresetsEl) {
    return;
  }

  sessionRenamePresetsEl.textContent = "";
  state.terminalRenamePresets.forEach((preset) => {
    const button = document.createElement("button");
    button.className = "button secondary session-rename-preset";
    button.type = "button";
    button.setAttribute("data-action", "append-session-rename-preset");
    button.dataset.preset = preset;
    button.textContent = preset;
    button.title = `追加 _${preset}`;
    sessionRenamePresetsEl.appendChild(button);
  });
}

function appendSessionRenamePreset(preset) {
  if (!sessionRenameInputEl) {
    return;
  }

  preset = normalizeTerminalRenamePreset(preset);
  if (!preset) {
    return;
  }

  const session = renamingSession();
  const base = sessionRenameInputEl.value.trim() || session?.name || "";
  sessionRenameInputEl.value = base ? `${base}_${preset}` : preset;
  focusTextInputToEnd(sessionRenameInputEl);
}

function updateTerminalRenameDialogStatus(message, tone = "muted") {
  if (!terminalRenameDialogStatusEl) {
    return;
  }
  terminalRenameDialogStatusEl.textContent = message || "";
  terminalRenameDialogStatusEl.dataset.tone = tone;
  terminalRenameDialogStatusEl.hidden = !message;
}

function openTerminalRenameDialog(name, trigger) {
  if (!sessionRenameDialogEl || !sessionRenameInputEl) {
    return;
  }
  terminalRenameTriggerEl = trigger;
  sessionRenameInputEl.value = name;
  updateTerminalRenameDialogStatus("");
  setSessionRenameControlsDisabled(false);
  renderSessionRenamePresets();
  if (typeof sessionRenameDialogEl.showModal === "function") {
    if (!sessionRenameDialogEl.open) {
      sessionRenameDialogEl.showModal();
    }
  } else {
    sessionRenameDialogEl.setAttribute("open", "");
  }
  window.requestAnimationFrame(() => {
    focusTextInputToEnd(sessionRenameInputEl);
  });
}

function closeSessionRenameEditor() {
  const trigger = terminalRenameTriggerEl;
  terminalRenameTriggerEl = null;
  state.renamingSessionId = "";
  if (sessionRenameDialogEl?.open) {
    sessionRenameDialogEl.close();
  } else if (sessionRenameDialogEl) {
    sessionRenameDialogEl.removeAttribute("open");
  }
  if (sessionRenameInputEl) {
    sessionRenameInputEl.value = "";
  }
  updateTerminalRenameDialogStatus("");
  setSessionRenameControlsDisabled(false);
  window.setTimeout(() => {
    trigger?.focus();
  }, 0);
}

function syncSessionRenameEditor() {
  if (!sessionRenameDialogEl || !state.renamingSessionId) {
    return;
  }

  const session = renamingSession();
  if (!session) {
    closeSessionRenameEditor();
    return;
  }

  renderSessionRenamePresets();
}

function startSessionRename(session, trigger) {
  if (!session?.id || !sessionRenameDialogEl || !sessionRenameInputEl) {
    return;
  }

  state.renamingSessionId = session.id;
  openTerminalRenameDialog(sessionRenameDraftName(session.name), trigger);
}

function forgetSessionPreference(pathValue, sessionId) {
  if (state.activeSessionId === sessionId) {
    state.activeSessionId = "";
  }
  if (getStoredSessionId(pathValue) === sessionId) {
    storeSessionId(pathValue, "");
  }
  if (getStoredGlobalSessionId() === sessionId) {
    storeGlobalSessionId("");
  }
}

function syncHistory({ push = false } = {}) {
  if (state.initialTerminalIntentPending) {
    syncTopNavigation();
    updateNavigationButtons();
    return;
  }

  const nextUrl = buildTerminalUrl(state.activeSessionId);
  if (push) {
    state.historyIndex += 1;
    state.historyMaxIndex = state.historyIndex;
    window.history.pushState(
      {
        webclxTerminal: true,
        index: state.historyIndex,
      },
      "",
      nextUrl,
    );
  } else {
    const currentIndex =
      window.history.state?.webclxTerminal && Number.isInteger(window.history.state.index)
        ? window.history.state.index
        : state.historyIndex;
    state.historyIndex = currentIndex;
    state.historyMaxIndex = Math.max(state.historyMaxIndex, currentIndex);
    window.history.replaceState(
      {
        webclxTerminal: true,
        index: currentIndex,
      },
      "",
      nextUrl,
    );
  }
  syncTopNavigation();
  updateNavigationButtons();
}

function initializeNavigationState() {
  const currentState = window.history.state;
  if (currentState?.webclxTerminal && Number.isInteger(currentState.index)) {
    state.historyIndex = currentState.index;
    state.historyMaxIndex = Math.max(state.historyMaxIndex, currentState.index);
  } else {
    state.historyIndex = 0;
    state.historyMaxIndex = 0;
  }
  syncHistory();
}

function navigateHistory(delta) {
  if (delta < 0) {
    window.history.back();
  } else if (delta > 0) {
    window.history.forward();
  }
}

function navigateBackWithFallback() {
  const currentUrl = window.location.href;
  navigateHistory(-1);
  window.setTimeout(() => {
    if (window.location.href === currentUrl) {
      window.location.href = getDirectoryListingHref();
    }
  }, 160);
}

function clearActiveSession() {
  cancelNewSessionQuickStart();
  state.activeSessionId = "";
  state.renamingSessionId = "";
  storeSessionId(state.currentPath, "");
  if (getStoredGlobalSessionId()) {
    storeGlobalSessionId("");
  }
  syncHistory();
  renderSessions();
}

function syncTerminalSessionSortControl() {
  if (!terminalSessionSortButtonEl) {
    return;
  }

  const mode = sharedNormalizeTerminalSessionSortMode(state.sessionSortMode);
  const currentLabel = sharedTerminalSessionSortModeLabel(mode);
  const nextLabel = sharedTerminalSessionSortModeLabel(sharedNextTerminalSessionSortMode(mode));
  terminalSessionSortButtonEl.dataset.sortMode = mode;
  terminalSessionSortButtonEl.textContent = mode ? `排序：${currentLabel}` : "切换终端排序";
  terminalSessionSortButtonEl.setAttribute(
    "aria-label",
    mode ? `终端当前按${currentLabel}排序，下次按${nextLabel}` : "切换终端排序，首次按工作区",
  );
  terminalSessionSortButtonEl.title = mode
    ? `当前按${currentLabel}排序；再次调用将按${nextLabel}排序`
    : "依次按工作区、Agent 类型、状态排序";
}

function setTerminalSessionSortMode(mode) {
  const normalizedMode = sharedNormalizeTerminalSessionSortMode(mode);
  if (!normalizedMode) {
    return "";
  }

  state.sessionSortMode = normalizedMode;
  state.sessions = sharedSortTerminalSessions(state.sessions, normalizedMode);
  renderSessions();
  syncTerminalSessionSortControl();
  return normalizedMode;
}

function cycleTerminalSessionSortMode() {
  return setTerminalSessionSortMode(sharedNextTerminalSessionSortMode(state.sessionSortMode));
}

function renderIdleSessions() {
  if (!idleSessionSelectEl) {
    return;
  }

  const sessions = idleSessions();
  const previousValue = idleSessionSelectEl.value;
  idleSessionSelectEl.textContent = "";
  const placeholder = document.createElement("option");
  placeholder.value = "";
  placeholder.textContent = sessions.length > 0 ? "闲置终端" : "暂无闲置";
  placeholder.selected = true;
  idleSessionSelectEl.appendChild(placeholder);

  sessions.forEach((session) => {
    const option = document.createElement("option");
    option.value = session.id;
    option.textContent = sessionOptionLabel(session);
    option.title = sessionOptionTitle(session);
    idleSessionSelectEl.appendChild(option);
  });

  idleSessionSelectEl.disabled = sessions.length === 0;
  // 选中即恢复：选择被消费后回到占位。
  idleSessionSelectEl.value = "";
}

function isManagedTerminalSession(session) {
  return session?.origin === "workflow" || session?.origin === "agent";
}

function renderTerminalSessionSelector(
  selectEl,
  sessions,
  activeSessionId,
  emptyLabel,
  { includePlaceholder = true } = {},
) {
  if (!selectEl) {
    return;
  }

  selectEl.textContent = "";
  if (sessions.length === 0 || includePlaceholder) {
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = sessions.length > 0 ? emptyLabel : `暂无${emptyLabel}`;
    placeholder.selected = true;
    selectEl.appendChild(placeholder);
  }

  for (const session of sessions) {
    const option = document.createElement("option");
    option.value = session.id;
    option.dataset.workspacePath = sessionPath(session);
    option.textContent = sessionOptionLabel(session);
    option.title = sessionOptionTitle(session);
    selectEl.appendChild(option);
  }

  selectEl.disabled = sessions.length === 0;
  if (sessions.some((session) => session.id === activeSessionId)) {
    selectEl.value = activeSessionId;
  } else {
    selectEl.value = includePlaceholder ? "" : sessions[0]?.id || "";
  }
}

function renderSessions() {
  const sessions = visibleSessions();

  syncTerminalSessionSortControl();

  if (sessions.length === 0) {
    closeSessionRenameEditor();
    const emptyLabel = idleSessions().length > 0 ? "活动终端已闲置" : "终端";
    renderTerminalSessionSelector(sessionSelectEl, [], "", emptyLabel);
    renderTerminalSessionSelector(agentSessionSelectEl, [], "", "智能体终端");
    if (renameSessionButton) {
      renameSessionButton.disabled = true;
    }
    if (deleteSessionButton) {
      deleteSessionButton.disabled = true;
    }
    if (idleSessionButton) {
      idleSessionButton.disabled = !activeSession() || isIdleSession(activeSession().id);
    }
    renderIdleSessions();
    syncTerminalInputHistoryButton();
    return;
  }

  const current = activeSession();
  const managedSessions = sessions.filter((session) => isManagedTerminalSession(session));
  const selectedSessionId = current && !isIdleSession(current.id) ? current.id : state.activeSessionId;
  renderTerminalSessionSelector(sessionSelectEl, sessions, selectedSessionId, "终端列表", {
    includePlaceholder: false,
  });
  renderTerminalSessionSelector(agentSessionSelectEl, managedSessions, selectedSessionId, "智能体终端");
  if (renameSessionButton) {
    renameSessionButton.disabled = !current || isIdleSession(current.id);
  }
  if (deleteSessionButton) {
    deleteSessionButton.disabled = !current || isIdleSession(current.id);
  }
  if (idleSessionButton) {
    idleSessionButton.disabled = !current || isIdleSession(current.id);
  }

  renderIdleSessions();
  syncSessionRenameEditor();
  syncTerminalInputHistoryButton();
}
