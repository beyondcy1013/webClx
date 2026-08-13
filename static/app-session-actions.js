// Terminal session list loading, search, create, rename, and delete actions for the home page.
// Loaded before app.js; functions run after app.js globals are initialized.
async function hydrateHomeSessionInputHistory(requestToken, sessions) {
  await Promise.all(sessions.map(async (session) => {
    try {
      const payload = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}/input-history`);
      if (requestToken === state.sessionRequestToken) {
        session.input_history_text = workspaceHistoryInputHistoryText(payload.entries || []);
      }
    } catch {
      if (requestToken === state.sessionRequestToken) {
        session.input_history_text = "";
      }
    }
  }));

  if (requestToken === state.sessionRequestToken) {
    renderSessions();
  }
}

async function loadSessions({ preserveCurrentList = false, silentStatus = false } = {}) {
  const requestToken = ++state.sessionRequestToken;
  if (!silentStatus) {
    updateSessionsStatus("正在读取终端会话列表…", "info", { sticky: true });
  }
  refreshSessionsButton.disabled = true;
  createSessionButton.disabled = true;
  const hadSessionsBeforeLoad = state.sessions.length > 0;
  const keepCurrentList = preserveCurrentList && hadSessionsBeforeLoad;
  if (hasSessionsSessionControls && !keepCurrentList) {
    sessionsSessionListEl.innerHTML = "";
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "正在读取终端会话列表…";
    placeholder.selected = true;
    sessionsSessionListEl.appendChild(placeholder);
    sessionsSessionListEl.disabled = true;
  }
  syncSessionsTerminalLink();

  try {
    const response = await requestJson("/api/terminal/sessions?all=true");
    if (requestToken !== state.sessionRequestToken) {
      return;
    }

    state.sessions = sortSessionsByRecentActivity(activeTerminalSessions(response.sessions));
    const storedSessionId = getStoredGlobalSessionId();
    applyCurrentWorkspaceSessionSelection(state.sessions);
    if (storedSessionId && !state.sessions.some((session) => session.id === storedSessionId)) {
      storeGlobalSessionId("");
    }

    renderSessions();
    void hydrateHomeSessionInputHistory(requestToken, state.sessions);
    if (state.activeTab === "workspace") {
      syncTabUrl();
    }
    if (!silentStatus) {
      updateSessionsStatus(
        state.sessions.length === 0 ? "还没有终端会话。" : "终端会话列表已更新。",
        state.sessions.length === 0 ? "muted" : "ok",
      );
    }
  } catch (error) {
    if (requestToken !== state.sessionRequestToken) {
      return;
    }

    if (!hadSessionsBeforeLoad) {
      state.sessions = [];
      state.preferredSessionId = "";
      renderSessions();
      if (!silentStatus) {
        updateSessionsStatus(error.message, "warn");
      }
    } else {
      renderSessions();
      if (!silentStatus) {
        updateSessionsStatus(`读取失败，已保留当前终端列表：${error.message}`, "warn");
      }
    }
  } finally {
    if (requestToken === state.sessionRequestToken) {
      refreshSessionsButton.disabled = false;
      createSessionButton.disabled = false;
    }
  }
}

async function searchSessionsOutput(query) {
  const normalizedQuery = String(query || "").trim();
  state.sessionSearchQuery = normalizedQuery;
  if (sessionsSearchInputEl && sessionsSearchInputEl.value !== normalizedQuery) {
    sessionsSearchInputEl.value = normalizedQuery;
  }

  if (!normalizedQuery) {
    state.sessionSearchResults = [];
    renderSessions();
    updateSessionsStatus(
      state.sessions.length === 0 ? "还没有终端会话。" : "终端会话列表已更新。",
      state.sessions.length === 0 ? "muted" : "ok",
    );
    return;
  }

  const requestToken = ++state.sessionSearchRequestToken;
  setSessionSearchControlsBusy(true);
  updateSessionsStatus(`正在搜索终端输出：${normalizedQuery}`, "info", { sticky: true });

  try {
    const response = await requestJson(
      `/api/terminal/sessions/search?q=${encodeURIComponent(normalizedQuery)}`,
    );
    if (requestToken !== state.sessionSearchRequestToken) {
      return;
    }

    state.sessionSearchResults = Array.isArray(response.matches) ? response.matches : [];
    renderSessions();
    updateSessionsStatus(
      state.sessionSearchResults.length === 0
        ? `没有终端输出包含“${normalizedQuery}”。`
        : `找到 ${state.sessionSearchResults.length} 个匹配终端。`,
      state.sessionSearchResults.length === 0 ? "muted" : "ok",
    );
  } catch (error) {
    if (requestToken !== state.sessionSearchRequestToken) {
      return;
    }

    state.sessionSearchResults = [];
    renderSessions();
    updateSessionsStatus(`搜索终端输出失败：${error.message}`, "warn");
  } finally {
    if (requestToken === state.sessionSearchRequestToken) {
      setSessionSearchControlsBusy(false);
    }
  }
}

function clearSessionsOutputSearch() {
  state.sessionSearchRequestToken += 1;
  state.sessionSearchQuery = "";
  state.sessionSearchResults = [];
  if (sessionsSearchInputEl) {
    sessionsSearchInputEl.value = "";
  }
  setSessionSearchControlsBusy(false);
  renderSessions();
  updateSessionsStatus(
    state.sessions.length === 0 ? "还没有终端会话。" : "已清除终端输出搜索。",
    state.sessions.length === 0 ? "muted" : "ok",
  );
}

async function createSession() {
  updateSessionsStatus("正在创建新终端…", "info", { sticky: true });
  refreshSessionsButton.disabled = true;
  createSessionButton.disabled = true;

  try {
    const session = await requestJson("/api/terminal/sessions", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        path: state.currentPath,
      }),
    });
    announceSessionMutation("created", session);
    rememberPreferredSession(session.path || state.currentPath, session.id);
    updateSessionsStatus(`已创建 ${session.name}，正在打开终端启动默认快捷命令…`, "ok");
    window.location.assign(
      buildTerminalUrl(session.path || state.currentPath, session.id, {
        quickStart: true,
      }),
    );
  } catch (error) {
    updateSessionsStatus(error.message, "warn");
  } finally {
    refreshSessionsButton.disabled = false;
    createSessionButton.disabled = false;
  }
}

async function renameSession() {
  const session = editingSession();
  if (!session || !sessionRenameInputEl) {
    closeSessionRenameEditor();
    return;
  }

  const nextName = sessionRenameSavedName(sessionRenameInputEl.value);
  if (!nextName) {
    updateTerminalRenameDialogStatus("请输入新的终端名称。", "warn");
    sessionRenameInputEl.focus();
    return;
  }
  if (nextName === session.name) {
    closeSessionRenameEditor();
    updateSessionsStatus("终端名称未变化。", "muted");
    return;
  }

  closeTerminalRenameDialog();
  updateSessionsStatus(`正在改名 ${session.name}…`, "info");

  try {
    const renamed = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}`, {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        path: session.path || state.currentPath,
        name: nextName,
      }),
    });

    announceSessionMutation("renamed", renamed);
    state.sessions = sortSessionsByRecentActivity(
      state.sessions.map((item) => (item.id === renamed.id ? renamed : item)),
    );
    if ((renamed.path || "") === state.currentPath) {
      state.directorySessions = sortSessionsByRecentActivity(
        state.directorySessions.map((item) => (item.id === renamed.id ? renamed : item)),
      );
      renderDirectorySessions();
    }
    renderSessions();
    if (state.activeTab === "workspace-history") {
      renderWorkspaceHistory();
    }
    updateSessionsStatus(`终端已改名为 ${renamed.name}。`, "ok");
  } catch (error) {
    updateSessionsStatus(`终端改名失败：${error.message}`, "warn");
  }
}

async function deleteSession(session) {
  if (
    !window.confirm(`结束终端"${session.name}"后，里面正在运行的命令也会被终止。确定继续吗？`)
  ) {
    return;
  }

  updateSessionsStatus(`正在结束 ${session.name}…`, "info", { sticky: true });
  refreshSessionsButton.disabled = true;
  createSessionButton.disabled = true;

  const affectsCurrentPath = (session.path || "") === state.currentPath;

  try {
    const deleted = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}`, {
      method: "DELETE",
      headers: {
        "X-WebClx-Confirm-Session": session.id,
        "X-WebClx-Delete-Source": "home-sessions",
      },
    });

    announceSessionMutation("deleted", deleted);
    forgetPreferredSession(session.path || "", session.id);
    await Promise.all([
      loadSessions(),
      affectsCurrentPath ? loadDirectorySessions() : Promise.resolve(),
    ]);
    updateSessionsStatus(`已结束 ${deleted.name}。`, "ok");
  } catch (error) {
    updateSessionsStatus(error.message, "warn");
  } finally {
    refreshSessionsButton.disabled = false;
    createSessionButton.disabled = false;
  }
}
