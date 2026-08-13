// 会话目录控制、重命名编辑与搜索辅助模块。
// 由 app.js 拆出，在 app.js 之前以 <script defer> 加载，
// 通过共享全局作用域向 app.js 提供下列函数，无需修改调用方。
// 依赖的全局（state.*、DOM 元素引用等）均为 app.js 顶层声明，加载顺序保证可用。

function renderSessionsSessionPicker() {
  if (!hasSessionsSessionControls) {
    syncSessionsTerminalLink();
    return;
  }

  // Rebuilding a native <select> while its popup is open makes the browser
  // collapse it, so the user's just-opened menu snaps shut. Defer the rebuild
  // until the dropdown closes (see the focus/blur listeners on
  // sessionsSessionListEl). Mirrors the directory-session-list protection.
  if (state.sessionsSessionUiBlocked) {
    state.pendingSessionsSessionUiSync = true;
    return;
  }

  state.pendingSessionsSessionUiSync = false;
  sessionsSessionListEl.innerHTML = "";

  if (state.sessions.length === 0) {
    const empty = document.createElement("option");
    empty.value = "";
    empty.textContent = "当前没有活动终端";
    empty.selected = true;
    sessionsSessionListEl.appendChild(empty);
    sessionsSessionListEl.disabled = true;
    syncSessionsTerminalLink();
    return;
  }

  const selectedSession = activeGlobalSession();
  if (!selectedSession) {
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "选择活动终端";
    placeholder.selected = true;
    sessionsSessionListEl.appendChild(placeholder);
  }

  sessionsSessionListEl.disabled = false;
  state.sessions.forEach((session) => {
    const option = document.createElement("option");
    option.value = session.id;
    option.dataset.workspacePath = session.path || "";
    option.textContent = `${sessionActivityAgentPrefix(session)}${sessionActivityPrefix(session)}${session.name}${sessionActivityAgentSuffix(session)}`;
    option.title = `${sessionActivityLabel(session)} - ${sessionLocationLabel(session)}${session.activity_error_keyword ? ` - ${session.activity_error_keyword}` : ""}`;
    if (session.id === state.preferredSessionId) {
      option.selected = true;
    }
    sessionsSessionListEl.appendChild(option);
  });

  syncSessionsTerminalLink();
}

function syncDirectorySessionControls() {
  if (!hasDirectorySessionControls) {
    syncWorkspaceTerminalLink();
    return;
  }

  // Rebuilding a native <select> while its popup is opening can make some
  // browsers briefly reopen it, which looks like a double popup.
  if (state.directorySessionUiBlocked) {
    state.pendingDirectorySessionUiSync = true;
    return;
  }

  directorySessionListEl.innerHTML = "";

  if (state.directorySessionUiMode !== "ready") {
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = state.directorySessionPlaceholderMessage || directorySessionLoadingMessage();
    placeholder.selected = true;
    directorySessionListEl.appendChild(placeholder);
    directorySessionListEl.disabled = true;
    state.pendingDirectorySessionUiSync = false;
    syncWorkspaceTerminalLink();
    return;
  }

  if (state.directorySessions.length === 0) {
    const empty = document.createElement("option");
    empty.value = "";
    empty.textContent = directorySessionEmptyMessage();
    empty.selected = true;
    directorySessionListEl.appendChild(empty);
    directorySessionListEl.disabled = true;
    state.pendingDirectorySessionUiSync = false;
    syncWorkspaceTerminalLink();
    return;
  }

  let selectedSession = activeDirectorySession();
  if (selectedSession && !sessionMatchesPath(selectedSession)) {
    selectedSession = null;
  }
  state.directorySessionId = selectedSession?.id || "";

  if (!selectedSession) {
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = directorySessionPromptMessage();
    placeholder.selected = true;
    directorySessionListEl.appendChild(placeholder);
  }

  directorySessionListEl.disabled = false;
  state.directorySessions.forEach((session) => {
    const option = document.createElement("option");
    option.value = session.id;
    option.dataset.workspacePath = session.path || "";
    option.textContent = directorySessionOptionLabel(session);
    option.title = session.title || session.name;
    if (session.id === state.directorySessionId) {
      option.selected = true;
    }
    directorySessionListEl.appendChild(option);
  });

  state.pendingDirectorySessionUiSync = false;
  syncWorkspaceTerminalLink();
}

function setDirectorySessionPlaceholder(message) {
  state.directorySessionUiMode = "placeholder";
  state.directorySessionPlaceholderMessage = message;
  syncDirectorySessionControls();
}

function renderDirectorySessions() {
  state.directorySessionUiMode = "ready";
  state.directorySessionPlaceholderMessage = "";
  syncDirectorySessionControls();
}

async function loadDirectorySessions({ preferredSessionId = "", preserveCurrentList = false } = {}) {
  if (!hasDirectorySessionControls) {
    state.directorySessions = [];
    state.directorySessionId = "";
    syncWorkspaceTerminalLink();
    return;
  }

  const requestToken = ++state.directorySessionRequestToken;
  const keepCurrentList =
    preserveCurrentList && state.directorySessionUiMode === "ready" && state.directorySessions.length > 0;
  if (!keepCurrentList) {
    setDirectorySessionPlaceholder(directorySessionLoadingMessage());
  }

  try {
    const response = await requestJson(
      state.showAllWorkspaceSessions
        ? "/api/terminal/sessions?all=true"
        : `/api/terminal/sessions?path=${encodeURIComponent(state.currentPath)}`,
    );
    if (requestToken !== state.directorySessionRequestToken) {
      return;
    }

    state.directorySessions = sortSessionsByRecentActivity(activeTerminalSessions(response.sessions));
    const storedSessionId = getStoredSessionId(state.currentPath);
    const currentPathSessions = state.directorySessions.filter((session) => sessionMatchesPath(session));
    const currentPathSelection = state.directorySessions.find(
      (session) => session.id === state.directorySessionId && sessionMatchesPath(session),
    );
    const nextSession =
      currentPathSessions.find((session) => session.id === preferredSessionId) ||
      currentPathSessions.find((session) => session.id === storedSessionId) ||
      currentPathSelection ||
      currentPathSessions[0] ||
      null;

    if (storedSessionId && !currentPathSessions.some((session) => session.id === storedSessionId)) {
      storeSessionId(state.currentPath, "");
    }

    state.directorySessionId = nextSession?.id || "";
    renderDirectorySessions();
  } catch (error) {
    if (requestToken !== state.directorySessionRequestToken) {
      return;
    }

    if (keepCurrentList) {
      return;
    }

    state.directorySessions = [];
    state.directorySessionId = "";
    setDirectorySessionPlaceholder(`读取目录终端会话失败：${error.message}`);
  }
}

function openSession(session) {
  if (!session?.id) {
    return;
  }

  rememberPreferredSession(session.path, session.id);
  window.location.assign(buildTerminalUrl(session.path, session.id));
}

function resolveCurrentActiveSession() {
  if (!Array.isArray(state.sessions) || state.sessions.length === 0) {
    return null;
  }

  const candidateIds = [
    state.preferredSessionId,
    getStoredGlobalSessionId(),
    state.directorySessionId,
  ].filter(Boolean);

  for (const sessionId of candidateIds) {
    const matched = state.sessions.find((session) => session.id === sessionId);
    if (matched) {
      return matched;
    }
  }

  return state.sessions[0] || null;
}

function editingSession() {
  return state.sessions.find((session) => session.id === state.renamingSessionId) || null;
}

let terminalRenameTriggerEl = null;
let terminalRenameTriggerKey = "";

function setSessionRenameControlsDisabled(disabled) {
  if (!sessionRenameFormEl) {
    return;
  }

  sessionRenameFormEl.querySelectorAll("input, button").forEach((element) => {
    element.disabled = disabled;
  });
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
  terminalRenameTriggerKey = trigger?.dataset?.terminalRenameKey || "";
  sessionRenameInputEl.value = name;
  updateTerminalRenameDialogStatus("");
  setSessionRenameControlsDisabled(false);
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

function closeTerminalRenameDialog() {
  const trigger = terminalRenameTriggerEl;
  const triggerKey = terminalRenameTriggerKey;
  terminalRenameTriggerEl = null;
  terminalRenameTriggerKey = "";
  state.renamingSessionId = "";
  workspaceHistoryRenamingItem = null;
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
  window.requestAnimationFrame(() => {
    const replacement = triggerKey
      ? Array.from(document.querySelectorAll("[data-terminal-rename-key]")).find(
          (element) => element.dataset.terminalRenameKey === triggerKey,
        )
      : null;
    const focusTarget = trigger?.isConnected ? trigger : replacement;
    focusTarget?.focus();
  });
}

function closeSessionRenameEditor() {
  if (workspaceHistoryRenamingItem) {
    return;
  }
  closeTerminalRenameDialog();
}

function syncSessionRenameEditor() {
  if (!sessionRenameDialogEl || workspaceHistoryRenamingItem || !state.renamingSessionId) {
    return;
  }

  const session = editingSession();
  if (!session) {
    closeSessionRenameEditor();
    return;
  }

}

function startSessionRename(session, trigger) {
  if (!session?.id || !sessionRenameDialogEl || !sessionRenameInputEl) {
    return;
  }

  state.renamingSessionId = session.id;
  workspaceHistoryRenamingItem = null;
  openTerminalRenameDialog(sessionRenameDraftName(session.name), trigger);
}

function sessionSearchResultMap() {
  const map = new Map();
  state.sessionSearchResults.forEach((result) => {
    if (result?.session_id) {
      map.set(result.session_id, result);
    }
  });
  return map;
}

function sessionFromSearchResult(result) {
  return {
    id: result.session_id || "",
    name: result.session_name || result.sessionName || result.session_id || "",
    title: result.title || "",
    path: result.path || "",
    display_path: result.display_path || result.displayPath || "",
    last_opened_at: 0,
    created_at: 0,
  };
}

function visibleSessionsForSearch(resultMap) {
  if (!state.sessionSearchQuery) {
    return state.sessions;
  }
  return state.sessionSearchResults
    .map((result) => {
      return state.sessions.find((session) => session.id === result.session_id) || sessionFromSearchResult(result);
    })
    .filter((session) => session.id && resultMap.has(session.id));
}

function sessionSearchMatchLabel(result) {
  if (!result) {
    return "";
  }

  const lineNumber = Number(result.line_number || result.lineNumber || 0);
  const matchCount = Number(result.match_count || result.matchCount || 0);
  const linePrefix = lineNumber > 0 ? `第 ${lineNumber} 行` : "命中";
  const countLabel = matchCount > 1 ? ` · ${matchCount} 处` : "";
  const line = String(result.line || "").trim();
  return line ? `${linePrefix}${countLabel} · ${line}` : `${linePrefix}${countLabel}`;
}

function setSessionSearchControlsBusy(isBusy) {
  if (sessionsSearchInputEl) {
    sessionsSearchInputEl.disabled = isBusy;
  }
  if (sessionsSearchSubmitButton) {
    sessionsSearchSubmitButton.disabled = isBusy;
  }
  if (sessionsSearchClearButton) {
    sessionsSearchClearButton.disabled = isBusy && !state.sessionSearchQuery;
  }
}
