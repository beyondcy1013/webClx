function rememberPreferredSession(sessionPath, sessionId) {
  state.preferredSessionId = sessionId;
  state.returnTerminalSessionId = sessionId;
  storeSessionId(sessionPath || "", sessionId);
  storeGlobalSessionId(sessionId);
  recordWorkspaceHistory(sessionPath);
  syncSessionsTerminalLink();
}

function forgetPreferredSession(sessionPath, sessionId) {
  if (state.preferredSessionId === sessionId) {
    state.preferredSessionId = "";
  }
  if (state.directorySessionId === sessionId) {
    state.directorySessionId = "";
  }
  if (state.returnTerminalSessionId === sessionId) {
    state.returnTerminalSessionId = "";
  }
  if (getStoredSessionId(sessionPath || "") === sessionId) {
    storeSessionId(sessionPath || "", "");
  }
  if (getStoredGlobalSessionId() === sessionId) {
    storeGlobalSessionId("");
  }
  syncSessionsTerminalLink();
}

function activeDirectorySession() {
  return state.directorySessions.find((session) => session.id === state.directorySessionId) || null;
}

function activeGlobalSession() {
  return state.sessions.find((session) => session.id === state.preferredSessionId) || null;
}

function sessionMatchesPath(session, path = state.currentPath) {
  return normalizeRelativePath(session?.path || "") === normalizeRelativePath(path || "");
}

function preferredSessionForCurrentWorkspace(sessions = state.sessions) {
  const matchingSessions = (Array.isArray(sessions) ? sessions : []).filter((session) =>
    sessionMatchesPath(session),
  );
  if (matchingSessions.length === 0) {
    return null;
  }

  const candidateIds = [
    state.returnTerminalSessionId,
    getStoredSessionId(state.currentPath),
    state.directorySessionId,
    state.preferredSessionId,
    getStoredGlobalSessionId(),
  ].filter(Boolean);
  for (const sessionId of candidateIds) {
    const matched = matchingSessions.find((session) => session.id === sessionId);
    if (matched) {
      return matched;
    }
  }

  return matchingSessions[0] || null;
}

function applyCurrentWorkspaceSessionSelection(sessions = state.sessions) {
  const selectedSession = preferredSessionForCurrentWorkspace(sessions);
  state.preferredSessionId = selectedSession?.id || "";
  state.returnTerminalSessionId = selectedSession?.id || "";
  return selectedSession;
}

function syncDirectorySessionScopeLabel() {
  if (!directorySessionListEl) {
    return;
  }
}

function directorySessionLoadingMessage() {
  return state.showAllWorkspaceSessions ? "正在读取全部目录终端会话…" : "正在读取当前目录终端会话…";
}

function directorySessionEmptyMessage() {
  return state.showAllWorkspaceSessions ? "当前工作区暂无终端会话" : "当前目录暂无终端会话";
}

function directorySessionPromptMessage() {
  return "跳转终端";
}

function directorySessionOptionLabel(session) {
  return `${sessionActivityAgentPrefix(session)}${sessionActivityPrefix(session)}${session.name}${sessionActivityAgentSuffix(session)}`;
}

function sessionActivityState(session) {
  return sharedSessionActivityState(session);
}

function sessionActivityText(session) {
  return sharedSessionActivityText(session);
}

function sessionActivityLabel(session) {
  return sharedSessionActivityLabel(session);
}

function sessionActivityAgentLabel(session) {
  return sharedSessionActivityAgentLabel(session);
}

function sessionActivityAgentPrefix(session) {
  return sharedSessionActivityAgentPrefix(session, state.terminalActivityAgentDisplay);
}

function sessionActivityAgentSuffix(session) {
  return sharedSessionActivityAgentSuffix(session, state.terminalActivityAgentDisplay);
}

function sessionActivityPrefix(session) {
  return sharedSessionActivityPrefix(session);
}

function syncWorkspaceTerminalLink() {
  terminalLink.href = buildFreshTerminalUrl(state.currentPath);
}

function syncSessionsTerminalLink() {
  if (!sessionTerminalLink && !topNavTerminalLink) {
    return;
  }

  const selectedSession =
    activeGlobalSession() ||
    state.sessions.find((session) => session.id === state.returnTerminalSessionId) ||
    null;
  const targetUrl = selectedSession
    ? buildTerminalUrl(selectedSession.path, selectedSession.id)
    : state.returnTerminalSessionId
      ? buildTerminalUrl(state.currentPath, state.returnTerminalSessionId)
      : "";
  if (sessionTerminalLink) {
    sessionTerminalLink.href = targetUrl || buildFreshTerminalUrl(state.currentPath);
  }
  if (topNavTerminalLink) {
    topNavTerminalLink.href = targetUrl || buildTerminalUrl(state.currentPath);
  }
}

function renderSessions() {
  sessionsListEl.textContent = "";
  renderSessionsSessionPicker();
  const searchResultMap = sessionSearchResultMap();
  const visibleSessions = visibleSessionsForSearch(searchResultMap);

  if (state.sessions.length === 0 && !state.sessionSearchQuery) {
    closeSessionRenameEditor();
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "session-empty";
    cell.textContent = "还没有终端会话。";
    row.appendChild(cell);
    sessionsListEl.appendChild(row);
    return;
  }

  if (visibleSessions.length === 0) {
    closeSessionRenameEditor();
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "session-empty";
    cell.textContent = state.sessionSearchQuery
      ? `没有终端输出包含“${state.sessionSearchQuery}”。`
      : "还没有终端会话。";
    row.appendChild(cell);
    sessionsListEl.appendChild(row);
    return;
  }

  visibleSessions.forEach((session) => {
    const searchResult = searchResultMap.get(session.id);
    const row = document.createElement("tr");
    if (searchResult) {
      row.classList.add("session-search-match-row");
    }

    const openLink = createActionLink(
      "进入",
      buildTerminalUrl(session.path, session.id),
      "mini-button accent",
    );
    openLink.addEventListener("click", () => {
      rememberPreferredSession(session.path, session.id);
    });
    let renameButton;
    renameButton = createActionButton("改名", () => {
      startSessionRename(session, renameButton);
    });
    renameButton.dataset.terminalRenameKey = `session:${session.id}`;
    const deleteButton = createActionButton("结束", () => {
      deleteSession(session);
    });

    const openCell = document.createElement("td");
    openCell.className = "session-action-cell";
    openCell.appendChild(openLink);

    const renameCell = document.createElement("td");
    renameCell.className = "session-action-cell";
    renameCell.appendChild(renameButton);

    const deleteCell = document.createElement("td");
    deleteCell.className = "session-action-cell";
    deleteCell.appendChild(deleteButton);

    const nameCell = document.createElement("td");
    nameCell.className = "session-name-cell";
    const activityLabel = sessionActivityLabel(session);
    const activityBadge = document.createElement("span");
    activityBadge.className = [
      "session-activity-badge",
      activityLabel === "错误" ? "session-activity-error" : "",
      activityLabel === "重试中" ? "session-activity-retrying" : "",
      activityLabel === "待查看" ? "session-activity-completed" : "",
    ]
      .filter(Boolean)
      .join(" ");
    activityBadge.textContent = `[${activityLabel}]`;
    activityBadge.title = session.activity_error_keyword
      ? `错误关键字：${session.activity_error_keyword}`
      : activityLabel;
    const nameText = document.createElement("span");
    nameText.textContent = session.name;
    const agentLabel = sessionActivityAgentLabel(session);
    if (state.terminalActivityAgentDisplay === "prefix" && agentLabel) {
      const agentBadge = document.createElement("span");
      agentBadge.className = "session-activity-badge";
      agentBadge.textContent = `[${agentLabel}]`;
      agentBadge.title = `运行程序：${agentLabel}`;
      nameCell.append(agentBadge, " ");
    }
    nameCell.append(activityBadge, " ", nameText);
    if (state.terminalActivityAgentDisplay === "suffix" && agentLabel) {
      const agentBadge = document.createElement("span");
      agentBadge.className = "session-activity-badge";
      agentBadge.textContent = `[${agentLabel}]`;
      agentBadge.title = `运行程序：${agentLabel}`;
      nameCell.append(" ", agentBadge);
    }

    const dirCell = document.createElement("td");
    dirCell.className = "mono-text session-path-cell";
    dirCell.textContent = sessionDirectoryLabel(session);
    dirCell.title = sessionLocationLabel(session);

    const titleCell = document.createElement("td");
    titleCell.className = "session-title-cell";
    const conversationTitle = session.input_history_text || session.title || "";
    titleCell.textContent = conversationTitle;
    titleCell.tabIndex = 0;
    attachWorkspaceHistoryTooltip(titleCell, {
      title: conversationTitle,
      dir: "",
    });
    titleCell.addEventListener("click", () => {
      titleCell.focus();
    });

    const matchCell = document.createElement("td");
    matchCell.className = "session-search-match-cell";
    matchCell.textContent = sessionSearchMatchLabel(searchResult);
    matchCell.title = matchCell.textContent;

    row.append(openCell, renameCell, deleteCell, nameCell, dirCell, titleCell, matchCell);
    sessionsListEl.appendChild(row);
  });

  syncSessionRenameEditor();
}
