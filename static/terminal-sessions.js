// webClx 终端会话生命周期子系统：从 terminal.js 抽出，保持全局函数声明。
// 包含 idleCurrentSession/restoreIdleSession/loadSessions/createSession/
// renameSession/deleteSession/refreshTerminalViewportLayout。
// 注意：本文件只含函数声明，不含顶层执行代码（resizeObserver 依赖 terminalHost，
// 保留在 terminal.js 中）。必须在 terminal.js 之前 <script defer> 加载。

async function idleCurrentSession() {
  const current = activeSession();
  if (!current || isIdleSession(current.id)) {
    return;
  }

  updateSessionStatus(`正在移到闲置：${current.name}…`, "info");
  try {
    const updated = await requestJson(`/api/terminal/sessions/${encodeURIComponent(current.id)}/idle`, {
      method: "PUT",
    });
    updateSessionIdleState(updated.id || current.id, true);
    announceSessionMutation("idle", updated);
    updateSessionStatus(`已移到闲置：${updated.name || current.name}。`, "ok");
  } catch (error) {
    updateSessionStatus(error.message || "闲置终端失败。", "warn");
    renderSessions();
    return;
  }

  const nextSession = visibleSessions().find((session) => session.id !== current.id) || null;
  if (nextSession) {
    selectSession(nextSession.id, { connect: true, pushHistory: true });
    disposeTerminalSessionContext(current.id);
    return;
  }

  closeSocket({ suppressEvents: true });
  clearActiveSession();
  disposeTerminalSessionContext(current.id);
  updateStatus("活动终端都已移到闲置。", "muted");
  renderSessions();
}

async function restoreIdleSession(sessionId) {
  const session = state.sessions.find((item) => item.id === sessionId);
  if (!session) {
    renderIdleSessions();
    return;
  }

  updateSessionStatus(`正在恢复闲置终端：${session.name}…`, "info");
  try {
    const updated = await requestJson(`/api/terminal/sessions/${encodeURIComponent(sessionId)}/restore`, {
      method: "PUT",
    });
    updateSessionIdleState(updated.id || sessionId, false);
    announceSessionMutation("restored", updated);
    closeSessionRenameEditor();
    selectSession(session.id, { connect: true, pushHistory: true });
    if (idleSessionSelectEl) {
      idleSessionSelectEl.value = "";
    }
    updateSessionStatus(`已恢复闲置终端：${updated.name || session.name}。`, "ok");
  } catch (error) {
    updateSessionStatus(error.message || "恢复闲置终端失败。", "warn");
    renderSessions();
  }
}

async function loadSessions({
  preferredSessionId = "",
  pushHistoryOnSelect = false,
  preserveCurrentList = false,
  forcePreferredSession = false,
} = {}) {
  if (shouldDeferSessionListRender()) {
    mergePendingSessionRefresh({
      preferredSessionId,
      pushHistoryOnSelect,
      preserveCurrentList,
      forcePreferredSession,
    });
    return;
  }

  if (state.loadingSessions) {
    mergePendingSessionRefresh({
      preferredSessionId,
      pushHistoryOnSelect,
      preserveCurrentList,
      forcePreferredSession,
    });
    return;
  }

  state.loadingSessions = true;
  const hadSessionsBeforeLoad = state.sessions.length > 0;

  try {
    const currentPathBeforeLoad = normalizeTerminalPath(state.currentPath);
    const response = await requestJson(
      state.showAllWorkspaceSessions
        ? "/api/terminal/sessions?all=true"
        : `/api/terminal/sessions?path=${encodeURIComponent(state.currentPath)}`,
    );

    // A list request already in flight when this page creates a terminal can
    // return without that new id. Only locally created ids awaiting their first
    // list confirmation may be carried over; a normal missing id is authoritative
    // evidence that another browser deleted it.
    const fetchedSessions = (response.sessions || []).slice();
    const fetchedSessionIds = new Set(fetchedSessions.map((session) => session.id));
    state.pendingCreatedSessionIds.forEach((sessionId) => {
      if (fetchedSessionIds.has(sessionId)) {
        state.pendingCreatedSessionIds.delete(sessionId);
      }
    });
    const activeId = state.activeSessionId;
    const activeAwaitingConfirmation =
      activeId &&
      state.pendingCreatedSessionIds.has(activeId) &&
      !isIdleSession(activeId) &&
      !fetchedSessionIds.has(activeId);
    if (activeAwaitingConfirmation) {
      const previousActive = state.sessions.find((session) => session.id === activeId);
      if (previousActive) {
        fetchedSessions.push(previousActive);
      }
      state.pendingCreatedSessionIds.delete(activeId);
    }

    state.sessions = sortSessionsByRecentActivity(fetchedSessions);
    if (typeof migrateLegacyWorkflowSessionOrigins === "function") {
      await migrateLegacyWorkflowSessionOrigins();
      state.sessions = sortSessionsByRecentActivity(state.sessions);
    }
    pruneTerminalSessionContexts(state.sessions);
    maybePlayTerminalCompletionSound(state.sessions);
    await migrateLegacyIdleSessionIds();
    if (!state.showAllWorkspaceSessions) {
      state.currentPath = normalizeTerminalPath(response.path || "");
      syncCurrentPathDisplay(response.display_path || "/");
    } else {
      state.currentPath = currentPathBeforeLoad;
      syncCurrentPathDisplay();
    }

    if (shouldDeferSessionListRender()) {
      mergePendingSessionRefresh({
        preferredSessionId: preferredSessionId || state.activeSessionId,
        pushHistoryOnSelect,
        preserveCurrentList: true,
        forcePreferredSession,
      });
      return;
    }

    if (state.sessions.length === 0) {
      cancelNewSessionQuickStart();
      renderSessions();
      clearActiveSession();
      closeSocket({ suppressEvents: true });
      updateStatus("当前没有活动终端连接。", "muted");
      updateSessionStatus("", "muted");
      return;
    }

    const locationSessionId = currentLocationSessionId();
    const stableCurrentSessionId =
      !pushHistoryOnSelect &&
      locationSessionId &&
      locationSessionId === state.activeSessionId
        ? locationSessionId
        : "";
    const currentActiveSessionId =
      state.activeSessionId &&
      state.sessions.some(
        (session) => session.id === state.activeSessionId && !isIdleSession(session.id),
      )
        ? state.activeSessionId
        : "";

    // List refreshes synchronize metadata; they must not own the user's active
    // selection. A queued refresh can still carry an older push-history target
    // after the user has already switched again.
    const explicitTargetSessionIds = [
      currentActiveSessionId,
      stableCurrentSessionId,
      ...(forcePreferredSession || pushHistoryOnSelect
        ? [preferredSessionId, locationSessionId]
        : [locationSessionId, preferredSessionId]),
    ].filter(Boolean);
    const fallbackSessionIds = [
      state.activeSessionId,
      getStoredSessionId(state.currentPath),
      state.showAllWorkspaceSessions ? getStoredGlobalSessionId() : "",
    ].filter(Boolean);

    let targetSession = null;
    const explicitTargetSessionId = explicitTargetSessionIds.find((sessionId) => {
      targetSession = state.sessions.find((session) => session.id === sessionId) || null;
      return Boolean(targetSession);
    }) || "";

    if (!targetSession) {
      [...explicitTargetSessionIds, ...fallbackSessionIds].find((sessionId) => {
        targetSession = state.sessions.find((session) => session.id === sessionId) || null;
        return Boolean(targetSession);
      });
    }

    // When the URL specifies a path, prefer sessions in that directory
    // over a global/session preference pointing elsewhere.
    const pathPreferenceAllowed = !explicitTargetSessionId;
    if (pathPreferenceAllowed && state.currentPath) {
      if (!targetSession || sessionPath(targetSession) !== state.currentPath) {
        const pathMatch = state.sessions.find(
          (session) => sessionPath(session) === state.currentPath,
        );
        if (pathMatch) {
          targetSession = pathMatch;
        }
      }
    }

    if (!targetSession) {
      targetSession = visibleSessions()[0] || state.sessions[0];
    }

    if (targetSession && isIdleSession(targetSession.id)) {
      targetSession = visibleSessions()[0] || null;
    }

    if (!targetSession) {
      cancelNewSessionQuickStart();
      closeSocket({ suppressEvents: true });
      clearActiveSession();
      updateStatus("活动终端都在闲置列表中。", "muted");
      renderSessions();
      return;
    }

    updateSessionStatus("", "ok");
    renderSessions();
    selectSession(targetSession.id, {
      connect:
        !isTerminalConnected() || targetSession.id !== activeTerminalContext?.sessionId,
      pushHistory: pushHistoryOnSelect && targetSession.id !== state.activeSessionId,
    });
    syncAutoContinueHandledErrors();
    maybeAutoContinueErroredSessions();
  } catch (error) {
    if (!hadSessionsBeforeLoad) {
      state.sessions = [];
      renderSessions();
    } else {
      renderSessions();
    }
    updateSessionStatus(error.message, "warn");
  } finally {
    state.loadingSessions = false;
    if (state.pendingSessionRefresh) {
      if (shouldDeferSessionListRender()) {
        return;
      }
      const nextRefresh = state.pendingSessionRefresh;
      state.pendingSessionRefresh = null;
      window.requestAnimationFrame(() => {
        loadSessions(nextRefresh);
      });
    }
  }
}

async function createSession({
  autoSelect = true,
  suppressLoadingStatus = false,
  pushHistoryOnSelect = autoSelect,
  enableQuickStart = false,
  allowDuringInitialIntent = false,
  throwOnError = false,
  path = state.currentPath,
  origin = "normal",
  ownerKey = "",
  codexApiPresetId = "",
} = {}) {
  if (state.creatingSession) {
    if (throwOnError) {
      throw new Error("已有新建终端请求正在执行。");
    }
    return null;
  }
  if (state.initialTerminalIntentPending && !allowDuringInitialIntent) {
    updateSessionStatus("终端正在初始化，请稍候再新建。", "info");
    syncCreateSessionButton();
    if (throwOnError) {
      throw new Error("终端正在初始化，请稍候再新建。");
    }
    return null;
  }

  state.creatingSession = true;
  syncCreateSessionButton();
  if (allowDuringInitialIntent && state.initialTerminalIntentPending && !initialLocation.sessionId) {
    closeSocket({ suppressEvents: true });
    state.activeSessionId = "";
    renderSessions();
  }
  if (!suppressLoadingStatus) {
    updateSessionStatus("正在创建新终端…", "info");
  }

  try {
    if (enableQuickStart) {
      await loadTerminalSettings();
    }
    const session = await requestJson("/api/terminal/sessions", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        path,
        origin,
        owner_key: ownerKey,
        codex_api_preset_id: codexApiPresetId,
      }),
    });
    state.pendingCreatedSessionIds.add(session.id);
    announceSessionMutation("created", session);
    insertOrReplaceSession(session);
    if (enableQuickStart && autoSelect) {
      armNewSessionQuickStart(session.id);
    } else {
      cancelNewSessionQuickStart();
    }
    if (autoSelect) {
      prepareFreshTerminalDisplay(session);
      selectSession(session.id, {
        connect: true,
        pushHistory: pushHistoryOnSelect,
      });
      syncAutoContinueHandledErrors();
      maybeAutoContinueErroredSessions();
      window.requestAnimationFrame(() => {
        loadSessions({
          preferredSessionId: session.id,
          preserveCurrentList: true,
          forcePreferredSession: true,
        });
      });
    } else {
      renderSessions();
      window.requestAnimationFrame(() => {
        loadSessions({
          preferredSessionId: state.activeSessionId,
          preserveCurrentList: true,
        });
      });
    }
    if (!enableQuickStart || !autoSelect) {
      updateSessionStatus(`已创建 ${session.name}。`, "ok");
    }
    return session;
  } catch (error) {
    cancelNewSessionQuickStart();
    updateSessionStatus(error.message, "warn");
    if (throwOnError) {
      throw error;
    }
    return null;
  } finally {
    state.creatingSession = false;
    syncCreateSessionButton();
  }
}

async function renameSession() {
  const session = renamingSession();
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
    updateSessionStatus("终端名称未变化。", "muted");
    return;
  }

  closeSessionRenameEditor();
  updateSessionStatus(`正在改名 ${session.name}…`, "info");
  try {
    const renamed = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}`, {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        path: state.currentPath,
        name: nextName,
      }),
    });

    announceSessionMutation("renamed", renamed);
    state.sessions = sortSessionsByRecentActivity(
      state.sessions.map((item) => (item.id === renamed.id ? renamed : item)),
    );
    renderSessions();
    updateSessionStatus(`终端已改名为 ${renamed.name}。`, "ok");
  } catch (error) {
    updateSessionStatus(`终端改名失败：${error.message}`, "warn");
  }
}

async function deleteSession(session) {
  if (
    !window.confirm(`结束终端“${session.name}”后，里面正在运行的命令也会被终止。确定继续吗？`)
  ) {
    return;
  }

  const isActiveSession = state.activeSessionId === session.id;
  const nextSessionId =
    visibleSessions()
      .filter((item) => item.id !== session.id)
      .map((item) => item.id)
      [0] || "";

  createSessionButton.disabled = true;
  updateSessionStatus(`正在结束 ${session.name}…`, "info");

  try {
    const deleted = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}`, {
      method: "DELETE",
      headers: {
        "X-WebClx-Confirm-Session": session.id,
        "X-WebClx-Delete-Source": "terminal-page",
      },
    });

    state.pendingCreatedSessionIds.delete(session.id);
    announceSessionMutation("deleted", deleted);
    if (isActiveSession) {
      closeSocket({ suppressEvents: true });
      clearActiveSession();
      updateStatus("当前没有活动终端连接。", "muted");
    }
    disposeTerminalSessionContext(session.id);
    forgetSessionPreference(state.currentPath, session.id);
    await loadSessions({
      preferredSessionId: nextSessionId,
    });
    if (state.sessions.length === 0) {
      updateSessionStatus(`已结束 ${deleted.name}，当前目录已无终端会话，正在返回上一页…`, "ok");
      navigateBackWithFallback();
      return;
    }
    updateSessionStatus(`已结束 ${deleted.name}。`, "ok");
  } catch (error) {
    updateSessionStatus(error.message, "warn");
  } finally {
    createSessionButton.disabled = false;
  }
}

function refreshTerminalViewportLayout({ fit = true, requireConnected = false } = {}) {
  syncTerminalHostHeight();
  if (fit && (!requireConnected || isTerminalConnected())) {
    fitTerminal();
  }
  syncTerminalStickyOffsets();
  syncTerminalNavScroll();
  syncScrollTopButtonOffset();
  updateScrollTopButton();
  updateTerminalScrollBottomButton();
  updatePageScrollRail();
}
