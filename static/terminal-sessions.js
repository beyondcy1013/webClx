// webClx 终端会话生命周期子系统：从 terminal.js 抽出，保持全局函数声明。
// 包含 idleCurrentSession/restoreIdleSession/loadSessions/createSession/
// renameSession/deleteSession/refreshTerminalViewportLayout。
// 注意：本文件只含函数声明，不含顶层执行代码（resizeObserver 依赖 terminalHost，
// 保留在 terminal.js 中）。必须在 terminal.js 之前 <script defer> 加载。

function mergeTerminalSessionRecentOpenTimes(previousSessions, fetchedSessions) {
  const previousOpenedAtById = new Map(
    (previousSessions || [])
      .filter((session) => session?.id)
      .map((session) => [
        session.id,
        sessionTimestamp(session, "last_opened_at", "lastOpenedAt"),
      ]),
  );

  return (fetchedSessions || []).map((session) => {
    const fetchedOpenedAt = sessionTimestamp(session, "last_opened_at", "lastOpenedAt");
    const previousOpenedAt = previousOpenedAtById.get(session?.id) || 0;
    if (previousOpenedAt <= fetchedOpenedAt) {
      return session;
    }
    return { ...session, last_opened_at: previousOpenedAt };
  });
}

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

async function restoreHiddenSession(sessionId) {
  const session = state.sessions.find((item) => item.id === sessionId);
  if (!session) {
    if (typeof renderHiddenSessions === "function") {
      renderHiddenSessions();
    }
    return;
  }

  updateSessionStatus(`正在从隐藏专区恢复终端：${session.name}…`, "info");
  try {
    const updated = await requestJson(`/api/terminal/sessions/${encodeURIComponent(sessionId)}/restore`, {
      method: "PUT",
    });
    updateSessionIdleState(updated.id || sessionId, false);
    if (typeof updateSessionHiddenState === "function") {
      updateSessionHiddenState(updated.id || sessionId, false);
    }
    announceSessionMutation("restored", updated);
    closeSessionRenameEditor();
    selectSession(session.id, { connect: true, pushHistory: true });
    if (hiddenSessionSelectEl) {
      hiddenSessionSelectEl.value = "";
    }
    updateSessionStatus(`已恢复隐藏终端：${updated.name || session.name}。`, "ok");
  } catch (error) {
    updateSessionStatus(error.message || "恢复隐藏终端失败。", "warn");
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
  const sessionsBeforeLoad = state.sessions.slice();

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
    if (preserveCurrentList && fetchedSessions.length === 0 && sessionsBeforeLoad.length > 0) {
      renderSessions();
      updateSessionStatus("终端列表刷新未取得数据，已保留现有终端。", "warn");
      return;
    }
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

    const mergedFetchedSessions = mergeTerminalSessionRecentOpenTimes(
      sessionsBeforeLoad,
      fetchedSessions,
    );
    state.sessions = sortSessionsByRecentActivity(mergedFetchedSessions);
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
        const pathMatch =
          visibleSessions().find((session) => sessionPath(session) === state.currentPath) ||
          state.sessions.find((session) => sessionPath(session) === state.currentPath);
        if (pathMatch) {
          targetSession = pathMatch;
        }
      }
    }

    if (!targetSession) {
      targetSession =
        (state.currentPath &&
          visibleSessions().find((session) => sessionPath(session) === state.currentPath)) ||
        visibleSessions()[0] ||
        (state.currentPath &&
          state.sessions.find((session) => sessionPath(session) === state.currentPath)) ||
        state.sessions[0];
    }

    if (targetSession && isIdleSession(targetSession.id)) {
      targetSession =
        (state.currentPath &&
          visibleSessions().find((session) => sessionPath(session) === state.currentPath)) ||
        visibleSessions()[0] ||
        null;
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
    const session = await WebClxNewTerminal.createSession(requestJson, {
      path,
      origin,
      ownerKey,
      codexApiPresetId,
    });
    enableAutoCompactForSession(session.id);
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
      focusTerminalAfterNewTerminalLaunch(session.id, createSessionButton);
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
    scheduleTerminalCollaborationRenameNotice(session);
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
    scheduleTerminalCollaborationRenameNotice(renamed);
    updateSessionStatus(`终端已改名为 ${renamed.name}。`, "ok");
  } catch (error) {
    updateSessionStatus(`终端改名失败：${error.message}`, "warn");
  }
}

function updateTerminalGroupDialogStatus(message, tone = "muted") {
  if (!terminalGroupDialogStatusEl) {
    return;
  }
  terminalGroupDialogStatusEl.dataset.tone = tone;
  terminalGroupDialogStatusEl.textContent = message;
  terminalGroupDialogStatusEl.hidden = !message;
}

function selectedTerminalGroupMembers() {
  if (!terminalGroupSessionListEl) return [];
  return Array.from(
    terminalGroupSessionListEl.querySelectorAll('input[type="checkbox"]:checked'),
    (input) => input.value,
  );
}

function selectedTerminalGroupRoles() {
  if (!terminalGroupSessionListEl) return [];
  return Array.from(
    terminalGroupSessionListEl.querySelectorAll("select[data-role-for]"),
    (select) => ({
      session_id: select.dataset.roleFor,
      role: terminalGroupRoleValue(select.value),
      checked: select.closest(".terminal-group-session-option")
        ?.querySelector('input[type="checkbox"]')?.checked === true,
    }),
  ).filter((assignment) => assignment.session_id && assignment.checked);
}

function renderTerminalGroupSessionList(session) {
  if (!terminalGroupSessionListEl) {
    return;
  }
  const currentToken = String(session.group_token || "");
  const sortedByName = terminalGroupSessionListEl.dataset.sortMode === "name-asc" ||
    terminalGroupSessionListEl.dataset.sortMode === "name-desc";
  const manualOrder = Array.from(terminalGroupSessionListEl.querySelectorAll("input[type=checkbox]"))
    .map((input) => input.value)
    .filter(Boolean);
  const sessions = !terminalGroupSessionListEl.dataset.sortMode && manualOrder.length
    ? manualOrder
        .map((id) => state.sessions.find((item) => item.id === id))
        .filter(Boolean)
        .concat(state.sessions.filter((item) => !manualOrder.includes(item.id)))
    : state.sessions;
  if (!sortedByName) {
    terminalGroupSessionListEl.dataset.sortMode = "";
  }
  terminalGroupSessionListEl.textContent = "";
  for (let index = 0; index < sessions.length; index += 1) {
    const item = sessions[index];
    const option = document.createElement("label");
    option.className = "terminal-group-session-option";
    option.dataset.sessionName = String(item.name || item.id);
    option.dataset.sessionOrder = String(index);
    option.dataset.originalOrder = String(index);
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.value = item.id;
    checkbox.checked = item.id === session.id || (!!currentToken && String(item.group_token || "") === currentToken);
    if (item.id === session.id) {
      checkbox.disabled = true;
    }
    option.appendChild(checkbox);
    const roleSelect = document.createElement("select");
    roleSelect.className = "terminal-group-role-select";
    roleSelect.setAttribute("aria-label", `设置 ${item.name || item.id} 的群组角色`);
    roleSelect.dataset.roleFor = item.id;
    [["manager", "管理者"], ["executor", "执行者"], ["reviewer", "复核者"]].forEach(
      ([value, label]) => {
        const roleOption = document.createElement("option");
        roleOption.value = value;
        roleOption.textContent = label;
        roleSelect.append(roleOption);
      },
    );
    const persistedRole = terminalGroupRoleValue(item.group_role);
    roleSelect.value = persistedRole || (item.id === session.id ? "manager" : "executor");
    roleSelect.disabled = !checkbox.checked;
    roleSelect.addEventListener("change", () => {
      enforceTerminalGroupManagerInvariant(roleSelect);
    });
    checkbox.addEventListener("change", () => {
      roleSelect.disabled = !checkbox.checked;
      enforceTerminalGroupManagerInvariant(null);
    });
    option.appendChild(roleSelect);
    const nameSpan = document.createElement("span");
    nameSpan.className = "terminal-group-session-name";
    nameSpan.textContent = item.id === session.id ? `${item.name}（当前终端）` : String(item.name || item.id);
    option.appendChild(nameSpan);
    const renameButton = document.createElement("button");
    renameButton.type = "button";
    renameButton.className = "button secondary terminal-group-session-rename";
    renameButton.textContent = "改名";
    renameButton.setAttribute("aria-label", `修改 ${item.name || item.id} 的终端名`);
    renameButton.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      openTerminalGroupMemberRenameDialog(item.id);
    });
    option.appendChild(renameButton);
    if (!!currentToken && item.id !== session.id && String(item.group_token || "") === currentToken) {
      const marker = document.createElement("span");
      marker.className = "terminal-group-session-marker";
      marker.textContent = "已同组";
      option.appendChild(marker);
    }
    terminalGroupSessionListEl.appendChild(option);
  }
  enforceTerminalGroupManagerInvariant(null);
  applyTerminalGroupSessionListOrder();
  syncTerminalGroupOrderButtons(session);
}

// 勾选成员里始终保留一名管理者：把某行设为管理者时其余已勾选的管理者自动
// 改为执行者；管理者被切换走或取消勾选时，优先提升当前终端，否则提升第一个
// 已勾选成员。未勾选的行属于其他分组，角色值不会被改动。
function enforceTerminalGroupManagerInvariant(exclusiveSelect) {
  if (!terminalGroupSessionListEl) return;
  const rows = Array.from(terminalGroupSessionListEl.querySelectorAll(".terminal-group-session-option"));
  const checkedRows = rows.filter(
    (row) => row.querySelector('input[type="checkbox"]')?.checked === true,
  );
  const checkedSelects = checkedRows
    .map((row) => row.querySelector("select[data-role-for]"))
    .filter(Boolean);
  if (exclusiveSelect && exclusiveSelect.value === "manager") {
    for (const select of checkedSelects) {
      if (select !== exclusiveSelect && select.value === "manager") select.value = "executor";
    }
  }
  if (!checkedSelects.some((select) => select.value === "manager")) {
    const currentId = terminalGroupDialogEl?.dataset.sessionId || "";
    const promote = checkedSelects.find((select) => select.dataset.roleFor === currentId)
      || checkedSelects[0];
    if (promote) promote.value = "manager";
  }
  for (const row of rows) {
    const checked = row.querySelector('input[type="checkbox"]')?.checked === true;
    const select = row.querySelector("select[data-role-for]");
    row.classList.toggle("is-manager", Boolean(checked && select?.value === "manager"));
  }
}

function currentTerminalGroupColor(session) {
  const normalizeColor = (globalThis.WebClxTerminalSessionActivity?.normalizeTerminalGroupColor || ((value) => String(value || "")))(
    session?.group_color || session?.groupColor,
  );
  return normalizeColor;
}

function syncTerminalGroupColorControls(session) {
  if (!terminalGroupColorEl || !terminalGroupColorValueEl || !session) return;
  const color = currentTerminalGroupColor(session);
  terminalGroupColorEl.value = color || "#4f8cff";
  terminalGroupColorValueEl.textContent = terminalGroupColorEl.value;
}

function currentTerminalGroupName(session) {
  return String(session?.group_name || session?.groupName || "").trim();
}

function syncTerminalGroupControls(session) {
  if (terminalGroupNameEl) {
    terminalGroupNameEl.value = currentTerminalGroupName(session);
  }
  syncTerminalGroupColorControls(session);
}

function selectedTerminalGroupMoveTarget(session) {
  if (!session) return "";
  const checkedIds = selectedTerminalGroupMembers().filter((id) => id !== session.id);
  return checkedIds.length === 1 ? checkedIds[0] : "";
}

function syncTerminalGroupOrderButtons(session) {
  if (!terminalGroupSessionListEl) return;
  const targetId = selectedTerminalGroupMoveTarget(session);
  const checkboxes = Array.from(terminalGroupSessionListEl.querySelectorAll("input[type=checkbox]"));
  const targetIndex = checkboxes.findIndex((input) => input.value === targetId);
  terminalGroupMoveUpButton.disabled = targetIndex <= 0;
  terminalGroupMoveDownButton.disabled = targetIndex < 0 || targetIndex >= checkboxes.length - 1;
}

function syncTerminalGroupSortTabs() {
  if (!terminalGroupSessionListEl || !terminalGroupSortTabs.length) return;
  const mode = terminalGroupSessionListEl.dataset.sortMode || "";
  for (const tab of terminalGroupSortTabs) {
    const active = (tab.dataset.sortMode || "") === mode;
    tab.classList.toggle("is-active", active);
    tab.setAttribute("aria-selected", active ? "true" : "false");
  }
}

function applyTerminalGroupSessionListOrder() {
  const options = Array.from(terminalGroupSessionListEl.querySelectorAll(".terminal-group-session-option"));
  const sortMode = terminalGroupSessionListEl.dataset.sortMode || "";
  if (sortMode === "name-asc" || sortMode === "name-desc") {
    const direction = sortMode === "name-asc" ? 1 : -1;
    options.sort((left, right) => direction * String(left.dataset.sessionName || "").localeCompare(
      String(right.dataset.sessionName || ""),
      "zh-Hans-CN",
      { numeric: true },
    ));
  } else {
    options.sort((left, right) =>
      Number(left.dataset.originalOrder || 0) - Number(right.dataset.originalOrder || 0),
    );
  }
  for (let index = 0; index < options.length; index += 1) {
    options[index].dataset.sessionOrder = String(index);
  }
  for (const option of options) {
    terminalGroupSessionListEl.appendChild(option);
  }
  syncTerminalGroupSortTabs();
}

function setTerminalGroupSessionListSortMode(mode) {
  if (!terminalGroupSessionListEl) return;
  terminalGroupSessionListEl.dataset.sortMode = mode === "name-asc" || mode === "name-desc" ? mode : "";
  applyTerminalGroupSessionListOrder();
  const session = activeSession();
  if (session) {
    syncTerminalGroupOrderButtons(session);
  }
}

function moveTerminalGroupSession(direction) {
  const session = activeSession();
  const targetId = session ? selectedTerminalGroupMoveTarget(session) : "";
  if (!session || !targetId || !terminalGroupSessionListEl) return;
  const options = Array.from(terminalGroupSessionListEl.querySelectorAll(".terminal-group-session-option"));
  const currentIndex = options.findIndex((option) => option.querySelector("input")?.value === targetId);
  const nextIndex = currentIndex + direction;
  if (currentIndex < 0 || nextIndex < 0 || nextIndex >= options.length) return;
  const target = options[currentIndex];
  const anchor = direction < 0 ? options[nextIndex] : options[nextIndex].nextSibling;
  terminalGroupSessionListEl.insertBefore(target, anchor);
  Array.from(terminalGroupSessionListEl.querySelectorAll(".terminal-group-session-option")).forEach(
    (option, index) => {
      option.dataset.sessionOrder = String(index);
      option.dataset.originalOrder = String(index);
    },
  );
  syncTerminalGroupOrderButtons(session);
}

function openTerminalGroupDialog() {
  const session = activeSession();
  if (!session || !terminalGroupDialogEl || !terminalGroupSessionListEl) {
    return;
  }
  if (typeof terminalActionKeyboardPolicyActive === "boolean") {
    terminalActionKeyboardPolicyActive = false;
    terminalActionEditableFocusBlocked = false;
  }
  terminalGroupDialogEl.dataset.sessionId = session.id;
  terminalGroupDialogEl.dataset.sourceRenameButton = "";
  renderTerminalGroupSessionList(session);
  if (terminalGroupFixedOrderEl) {
    terminalGroupFixedOrderEl.checked = session.group_fixed_order === true;
  }
  syncTerminalGroupControls(session);
  const tokenDisplay = document.getElementById("terminal-group-token-display");
  if (tokenDisplay) {
    const currentToken = String(session.group_token || "");
    tokenDisplay.textContent = currentToken
      ? "勾选要一起跟随的终端；选中一个搭档后可上移或下移，保存分组顺序。"
      : "在终端下拉列表中勾选终端，激活时整组移动。";
    tokenDisplay.hidden = false;
  }
  updateTerminalGroupDialogStatus("");
  if (typeof terminalGroupDialogEl.showModal === "function") {
    if (!terminalGroupDialogEl.open) {
      terminalGroupDialogEl.showModal();
    }
  } else {
    terminalGroupDialogEl.setAttribute("open", "");
  }
}

function closeTerminalGroupDialog() {
  if (terminalGroupDialogEl?.open) {
    terminalGroupDialogEl.close();
  } else if (terminalGroupDialogEl) {
    terminalGroupDialogEl.removeAttribute("open");
  }
  if (terminalGroupSessionListEl) {
    terminalGroupSessionListEl.textContent = "";
  }
  const tokenDisplay = document.getElementById("terminal-group-token-display");
  if (tokenDisplay) {
    tokenDisplay.textContent = "";
    tokenDisplay.hidden = true;
  }
  if (terminalGroupNameEl) terminalGroupNameEl.value = "";
  if (terminalGroupColorEl) terminalGroupColorEl.value = "#4f8cff";
  if (terminalGroupColorValueEl) terminalGroupColorValueEl.textContent = terminalGroupColorEl?.value || "";
  updateTerminalGroupDialogStatus("");
  const sourceTrigger = terminalGroupDialogEl?.dataset.sourceRenameButton;
  if (sourceTrigger) {
    document.getElementById(sourceTrigger)?.focus?.({ preventScroll: true });
    delete terminalGroupDialogEl.dataset.sourceRenameButton;
  }
}

function terminalCollaborationRenameRole(name) {
  const normalized = String(name || "").toLocaleLowerCase("zh-Hans-CN");
  if (/(管理者|管理|计划者|manager|planner)/.test(normalized)) return "管理者";
  if (/(执行者|执行|executor|worker)/.test(normalized)) return "执行者";
  if (/(复核者|复核|reviewer)/.test(normalized)) return "复核者";
  return "";
}

function terminalGroupRoleLabel(role) {
  const normalized = String(role || "").trim();
  if (normalized === "manager" || normalized === "管理者") return "管理者";
  if (normalized === "reviewer" || normalized === "复核者") return "复核者";
  if (normalized === "executor" || normalized === "执行者") return "执行者";
  return "";
}

// 后端把角色规范化成中文（管理者/执行者/复核者）存储，下拉框选项值用英文键；
// 回显和比较前先统一映射，避免持久化角色显示成空白。
function terminalGroupRoleValue(role) {
  const normalized = String(role || "").trim().toLowerCase();
  if (["manager", "管理者", "管理", "计划者", "planner"].includes(normalized)) return "manager";
  if (["executor", "执行者", "执行", "worker"].includes(normalized)) return "executor";
  if (["reviewer", "复核者", "复核"].includes(normalized)) return "reviewer";
  return "";
}

function terminalGroupNotifyDelivery(targetId, body, timeoutMs = 5000) {
  const timeout = new Promise((resolve) => {
    window.setTimeout(() => resolve({
      submitted: false,
      timed_out: true,
      error: "投递确认超时（5 秒），消息可能仍在后台提交。",
    }), timeoutMs);
  });
  const request = requestJson("/api/terminal/sessions/message", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  return Promise.race([request, timeout]).catch((error) => ({
    submitted: false,
    error: error.message || String(error),
  })).then((response) => ({ target: targetId, ...(response || {}) }));
}

function openTerminalGroupMemberRenameDialog(memberId = "") {
  if (!terminalGroupDialogEl?.open || !terminalGroupManagerDialog) return;
  const sessionId = memberId || terminalGroupDialogEl.dataset.sessionId;
  const session = state.sessions.find((item) => item.id === sessionId);
  if (!session) return;
  terminalGroupManagerDialog.dataset.sessionId = session.id;
  terminalGroupManagerTarget.textContent = `成员：${session.name || session.id}`;
  terminalGroupManagerInput.value = session.name || "";
  updateTerminalGroupManagerStatus("", "");
  if (typeof terminalGroupManagerDialog.showModal === "function") {
    if (!terminalGroupManagerDialog.open) terminalGroupManagerDialog.showModal();
  } else {
    terminalGroupManagerDialog.setAttribute("open", "");
  }
}

function updateTerminalGroupManagerStatus(message, tone = "muted") {
  if (!terminalGroupManagerStatus) return;
  terminalGroupManagerStatus.dataset.tone = tone;
  terminalGroupManagerStatus.textContent = message;
  terminalGroupManagerStatus.hidden = !message;
}

function normalizeTerminalManagerName(value) {
  return String(value || "").trim().replace(/_+$/u, "");
}

async function submitTerminalGroupManagerRename(event) {
  event.preventDefault();
  const sessionId = terminalGroupManagerDialog?.dataset.sessionId;
  const session = state.sessions.find((item) => item.id === sessionId);
  const nextName = normalizeTerminalManagerName(terminalGroupManagerInput?.value);
  if (!session || !nextName) {
    updateTerminalGroupManagerStatus("请输入终端名称。", "warn");
    return;
  }
  if (nextName === session.name) {
    closeTerminalGroupManagerDialog();
    return;
  }
  const currentToken = String(session.group_token || "");
  const others = state.sessions.filter((item) => item.id !== session.id
    && currentToken
    && String(item.group_token || "") === currentToken);
  const previousName = session.name;
  updateTerminalGroupManagerStatus("正在保存终端名…", "info");
  try {
    const renamed = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path: sessionPath(session), name: nextName }),
    });
    state.sessions = state.sessions.map((item) => (item.id === renamed.id ? renamed : item));
    renderSessions();
    const groupName = currentTerminalGroupName(renamed) || currentToken || "terminal-group";
    if (others.length) {
      Promise.all(others.map((member) => terminalGroupNotifyDelivery(member.id, {
        target: member.id,
        data: `[系统消息][群组:${groupName}] 成员终端已改名：「${previousName}」→「${renamed.name}」。`,
        submit: true,
        submit_enters: 1,
        bracketed_paste: true,
        verify_submission: true,
        delivery_id: `terminal-manager-rename:${session.id}:${Date.now()}:${member.id}`,
      }))).catch((error) => console.warn("成员改名通知发送失败:", error));
    }
    updateSessionStatus(`成员终端已改名为 ${renamed.name}。`, "ok");
    closeTerminalGroupManagerDialog();
    // 编组对话框保持打开，只同步被改名成员那一行，避免丢失未保存的角色调整。
    const groupSessionId = terminalGroupDialogEl?.dataset.sessionId;
    const row = terminalGroupSessionListEl?.querySelector(
      `select[data-role-for="${CSS.escape(renamed.id)}"]`,
    )?.closest(".terminal-group-session-option");
    if (row) {
      const nameSpan = row.querySelector(".terminal-group-session-name");
      const currentId = groupSessionId;
      if (nameSpan) {
        nameSpan.textContent = renamed.id === currentId
          ? `${renamed.name}（当前终端）`
          : String(renamed.name || renamed.id);
      }
      row.dataset.sessionName = String(renamed.name || renamed.id);
    }
    if (groupSessionId) {
      const groupSession = state.sessions.find((item) => item.id === groupSessionId);
      if (groupSession) syncTerminalGroupControls(groupSession);
    }
  } catch (error) {
    updateTerminalGroupManagerStatus(`改名失败：${error.message}`, "warn");
  }
}

function closeTerminalGroupManagerDialog() {
  if (terminalGroupManagerDialog?.open) terminalGroupManagerDialog.close();
  else if (terminalGroupManagerDialog) terminalGroupManagerDialog.removeAttribute("open");
  if (terminalGroupManagerInput) terminalGroupManagerInput.value = "";
  if (terminalGroupManagerDialog) delete terminalGroupManagerDialog.dataset.sessionId;
  updateTerminalGroupManagerStatus("");
}

function terminalGroupChangeBrief(
  previousMembers,
  nextMembers,
  previousNames,
  nextNames,
  groupName,
) {
  const before = new Map(previousMembers.map((member) => [member.id, member]));
  const after = new Map(nextMembers.map((member) => [member.id, member]));
  const added = nextMembers.filter((member) => !before.has(member.id));
  const removed = previousMembers.filter((member) => !after.has(member.id));
  const changed = nextMembers.filter((member) => {
    const old = before.get(member.id);
    if (!old) return false;
    return old.role !== member.role
      || (previousNames.get(member.id) || "") !== (nextNames.get(member.id) || "");
  });
  if (!added.length && !removed.length && !changed.length) return "";
  const lines = [`[系统消息][群组更新:${groupName || "terminal-group"}] 成员、终端名或角色已更新。`];
  if (added.length) {
    lines.push(`新成员：${added.map((member) => `「${member.name}」(${terminalGroupRoleLabel(member.role) || "未指定"})`).join("、")}`);
  }
  if (removed.length) {
    lines.push(`已移除：${removed.map((member) => `「${member.name}」`).join("、")}`);
  }
  if (changed.length) {
    lines.push(`已变更：${changed.map((member) => {
      const old = before.get(member.id);
      const oldName = previousNames.get(member.id) || old?.name || member.id;
      const namePart = oldName === member.name ? "" : ` 名称「${oldName}」→「${member.name}」`;
      const rolePart = old.role !== member.role
        ? ` 角色「${terminalGroupRoleLabel(old.role) || "未指定"}」→「${terminalGroupRoleLabel(member.role) || "未指定"}」`
        : "";
      return `「${member.name}」${namePart}${rolePart}`;
    }).join("；")}`);
  }
  lines.push("此消息已发送给你和当前合作方，用于避免误解；无需立即回复。");
  return lines.filter(Boolean).join(" ");
}

async function scheduleTerminalCollaborationRenameNotice(session) {
  // 角色名称不代表分组身份；普通终端改名不发送协作通知。
  if (!String(session?.group_token || "").trim()) return;
  const role = terminalCollaborationRenameRole(session?.name);
  if (!role) return;
  await requestJson("/api/terminal/sessions/message", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      target: session.id,
      data: `[协作] 你已改名为「${session.name}」，当前角色是${role}。开始工作前先阅读协作工作须知，并用终端定时消息流程与搭档确认。协作消息一律通过 /api/terminal/scheduled-inputs 设置未来短延时发送（实时消息也延迟数秒），不得同步等待投递返回；排程成功不等于接收，以对方定时 ACK 或原生记录确认，不重复发送正文。`,
      submit: true,
      submit_enters: 1,
      bracketed_paste: true,
      verify_submission: true,
      delivery_id: `terminal-role-rename:${session.id}:${session.name}`,
    }),
  }).catch(() => {});
}

function dispatchTerminalGroupNotifications({
  sessionId,
  groupedMembers,
  previousMembers,
  executors,
  updateNotice,
  brief,
  savedText,
}) {
  const notificationOutcomes = [];
  window.__webclxGroupNotificationOutcomes = notificationOutcomes;
  const promise = (async () => {
    try {
      if (updateNotice) {
        const current = groupedMembers.find((member) => member.id === sessionId);
        const partners = groupedMembers.filter((member) => member.id !== sessionId);
        const removedPartners = previousMembers.filter((member) => member.id !== sessionId
          && !groupedMembers.some((next) => next.id === member.id));
        const updateOutcomes = await Promise.all([current, ...partners, ...removedPartners].filter(Boolean).map(
          (member) => terminalGroupNotifyDelivery(member.id, {
            target: member.id,
            data: updateNotice,
            submit: true,
            submit_enters: 1,
            bracketed_paste: true,
            verify_submission: true,
            delivery_id: `terminal-group-change:${sessionId}:${Date.now()}:${member.id}`,
          }),
        ));
        notificationOutcomes.push(...updateOutcomes);
        window.__webclxGroupNotificationOutcomes = [...notificationOutcomes];
      }
      if (brief && executors.length > 0) {
        const briefOutcomes = await Promise.all(executors.map((executor, index) => terminalGroupNotifyDelivery(
          executor.id,
          {
            target: executor.id,
            data: brief,
            submit: true,
            submit_enters: 1,
            bracketed_paste: true,
            verify_submission: true,
            delivery_id: `terminal-group-brief:${sessionId}:${Date.now()}:${index}`,
          },
        )));
        notificationOutcomes.push(...briefOutcomes);
        window.__webclxGroupNotificationOutcomes = [...notificationOutcomes];
      }
    } catch (error) {
      console.warn("群组通知发送异常:", error);
    }
    const notified = notificationOutcomes.filter((item) => item.submitted).length;
    const pending = notificationOutcomes.filter((item) => item.timed_out).length;
    const failed = notificationOutcomes.filter((item) => !item.submitted && !item.timed_out).length;
    const notifyText = notificationOutcomes.length
      ? `通知确认 ${notified}/${notificationOutcomes.length}`
      : "";
    const notifyDetail = pending || failed
      ? `；${[pending ? `${pending} 个确认超时` : "", failed ? `${failed} 个发送失败` : ""].filter(Boolean).join("，")}`
      : "";
    if (notificationOutcomes.length > 0) {
      updateSessionStatus(`${savedText}${notifyText ? `，${notifyText}` : ""}${notifyDetail}。`, failed ? "warn" : "ok");
    }
    return notificationOutcomes;
  })();
  window.__webclxGroupNotificationPromise = promise;
  return promise;
}

let terminalGroupSaving = false;
async function saveTerminalGroupToken() {
  if (terminalGroupSaving) return;
  const sessionId = terminalGroupDialogEl?.dataset.sessionId;
  if (!sessionId || !terminalGroupSessionListEl) return;
  const memberIds = selectedTerminalGroupMembers();
  const roleAssignments = selectedTerminalGroupRoles();
  const previousMembers = state.sessions
    .filter((session) => session.group_token
      && session.group_token === (state.sessions.find((item) => item.id === sessionId)?.group_token || ""))
    .map((member) => ({
      id: member.id,
      name: member.name || member.id,
      role: terminalGroupRoleValue(member.group_role)
        || (member.id === sessionId ? "manager" : "executor"),
    }));
  const previousNames = new Map(previousMembers.map((member) => [member.id, member.name]));
  const groupedMembers = memberIds
    .map((id) => state.sessions.find((item) => item.id === id))
    .filter(Boolean)
    .map((member) => ({
      id: member.id,
      name: member.name || member.id,
      role: roleAssignments.find((assignment) => assignment.session_id === member.id)?.role || "",
    }));
  // 每组只保留一名管理者：重复指定时以列表顺序第一个为准，其余降级为执行者；
  // 无人被指定时默认当前终端为管理者。
  let hasManager = false;
  for (const member of groupedMembers) {
    if (member.role === "manager") {
      if (hasManager) member.role = "executor";
      else hasManager = true;
    }
  }
  if (!hasManager && groupedMembers.length > 0) {
    const fallback = groupedMembers.find((member) => member.id === sessionId) || groupedMembers[0];
    fallback.role = "manager";
  }
  const manager = groupedMembers.find((member) => member.role === "manager");
  const executors = groupedMembers.filter((member) => member !== manager);
  const groupName = terminalGroupNameEl?.value.trim() || "";
  terminalGroupSaving = true;
  const submit = terminalGroupFormEl.querySelector('[type="submit"]');
  submit.disabled = true;
  updateTerminalGroupDialogStatus("正在保存分组…");
  try {
    await requestJson(`/api/terminal/sessions/${encodeURIComponent(sessionId)}/group`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        member_ids: memberIds,
        group_roles: groupedMembers.map((member) => ({
          session_id: member.id,
          role: member.role,
        })),
        fixed_order: terminalGroupFixedOrderEl?.checked === true,
        group_color: terminalGroupColorEl?.value || "",
        group_name: terminalGroupNameEl ? terminalGroupNameEl.value.trim() : undefined,
      }),
    });
    await loadSessions({ preserveCurrentList: true });
    const nextNames = new Map(groupedMembers.map((member) => [member.id, member.name]));
    const updateNotice = terminalGroupChangeBrief(
      previousMembers,
      groupedMembers,
      previousNames,
      nextNames,
      groupName,
    );
    let brief = "";
    if (manager && executors.length > 0) {
      const skillName = "webclx-terminal-collaboration";
      const task = [
        "这是群组建队提示，不是新任务授权。",
        "先读取角色/搭档/群组信息和 $webclx-terminal-collaboration 工作须知；",
        "除非管理者随后下发明确任务，否则保持只读待命，不改动文件、不部署、不新建工作流。",
      ].join(" ");
      brief = terminalCollaborationBrief({
        manager,
        executors,
        groupName,
        task,
        skillName,
      }) || "";
    }
    const savedText = memberIds.length > 1
      ? (currentTerminalGroupName(
        state.sessions.find((item) => item.id === sessionId) || {},
      ) ? "群组终端、角色、顺序和名称已保存。" : "群组终端、角色和顺序已保存。")
      : "已解除终端分组。";

    // 确保提交消息发送动作后异步执行，不阻塞窗口返回
    if (updateNotice || brief) {
      window.__webclxGroupNotificationOutcomes = [];
      dispatchTerminalGroupNotifications({
        sessionId,
        groupedMembers,
        previousMembers,
        executors,
        updateNotice,
        brief,
        savedText,
      });
      updateSessionStatus(`${savedText}，正在发送群组通知…`, "ok");
    } else {
      window.__webclxGroupNotificationOutcomes = [];
      updateSessionStatus(`${savedText}。`, "ok");
    }
    closeTerminalGroupDialog();
  } catch (error) {
    let message = `终端分组保存失败：${error.message}`;
    updateTerminalGroupDialogStatus(message, "warn");
  } finally {
    terminalGroupSaving = false;
    submit.disabled = false;
  }
}

async function saveExperienceAndCloseSession(session) {
  return deleteSession(session, { withExperienceSave: true });
}

async function deleteSession(session) {
  if (!session) {
    return;
  }

  // Only the target session is busy; other terminals and new work stay usable.
  const pending = deleteSession.pendingSessionIds ||= new Set();
  if (pending.has(session.id)) return;

  const options = arguments[1] || {};
  let withExperienceSave = Boolean(options.withExperienceSave);
  const confirm = Boolean(options.confirm);

  let confirmation = null;
  if (confirm) {
    confirmation = await confirmTerminalExperienceSaveAndHide(session);
    if (confirmation.action === "cancel" || (!confirmation.shouldHide && !confirmation.shouldClose)) {
      return;
    }
    withExperienceSave = confirmation.shouldHide;
  }

  function updateSessionHiddenState(sessionId, hidden) {
    state.sessions = state.sessions.map((item) =>
      item.id === sessionId ? { ...item, hidden: Boolean(hidden), idle: false } : item,
    );
  }

  function leaveHiddenSession() {
    // Re-read the selection after hide completes: the user may already have
    // switched elsewhere while that request was in flight.
    if (state.activeSessionId === session.id) {
      const remaining = visibleSessions().filter((item) => item.id !== session.id);
      const next = remaining.find((item) => sessionPath(item) === sessionPath(session)) || remaining[0];
      if (next) {
        selectSession(next.id, { connect: true, pushHistory: true });
      } else {
        closeSocket({ suppressEvents: true });
        clearActiveSession();
        updateStatus("当前没有活动终端连接。", "muted");
      }
    } else {
      renderSessions();
    }
    disposeTerminalSessionContext(session.id);
    forgetSessionPreference(state.currentPath, session.id);
  }

  if (pending.has(session.id)) return;
  pending.add(session.id);

  if (withExperienceSave) {
    // 选“落盘并结束”：后台落盘，转入隐藏专区，完成后自行关闭
    updateSessionStatus(`正在转入隐藏并提交落盘：${session.name}…`, "info");

    try {
      if (typeof scheduleTerminalExperienceSave === "function") {
        scheduleTerminalExperienceSave(session).catch((err) => {
          console.warn("经验落盘指令排程未确认:", err);
        });
      }

      const hidden = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}/hide`, {
        method: "PUT",
      });

      state.pendingCreatedSessionIds.delete(session.id);
      updateSessionHiddenState(hidden.id || session.id, true);
      announceSessionMutation("idle", hidden);
      leaveHiddenSession();
      updateSessionStatus(`已提交经验落盘任务，${hidden.name || session.name} 已转入隐藏专区，完成后将自行关闭。`, "ok");
      if (confirmation) {
        reportTerminalExperienceSaveOutcome(confirmation, updateSessionStatus);
      }
    } catch (error) {
      updateSessionStatus(error.message, "warn");
    } finally {
      pending.delete(session.id);
    }
  } else {
    // 选“结束”：先转入隐藏专区并切走当前连接，再关闭终端。
    updateSessionStatus(`正在转入隐藏并关闭 ${session.name}…`, "info");

    try {
      const hidden = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}/hide`, {
        method: "PUT",
      });
      state.pendingCreatedSessionIds.delete(session.id);
      updateSessionHiddenState(hidden.id || session.id, true);
      announceSessionMutation("idle", hidden);
      leaveHiddenSession();

      // Finish in the background: no global button lock or completion navigation.
      updateSessionStatus(`正在后台关闭终端 ${session.name}…`, "info");
      const closed = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}/close`, {
        method: "POST",
      });

      state.pendingCreatedSessionIds.delete(session.id);
      state.sessions = state.sessions.filter((item) => item.id !== session.id);
      announceSessionMutation("deleted", closed);
      renderSessions();
      updateSessionStatus(`已关闭终端 ${session.name}。`, "ok");
    } catch (error) {
      updateSessionStatus(`关闭终端 ${session.name} 失败：${error.message || "请到隐藏专区重试。"}`, "warn");
    } finally {
      pending.delete(session.id);
    }
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

// terminal.js 在本脚本之后才求值，这里不能引用它的 const，只能查 DOM；
// 无 DOM 的测试沙箱（如 terminal-deleted-session-reconnect）直接跳过绑定。
if (typeof document !== "undefined") {
  const terminalGroupManagerFormEl = document.getElementById("terminal-group-manager-form");
  if (terminalGroupManagerFormEl) {
    terminalGroupManagerFormEl.addEventListener("submit", submitTerminalGroupManagerRename);
  }
  const terminalGroupManagerCancelButtonEl = document.getElementById("terminal-group-manager-cancel");
  if (terminalGroupManagerCancelButtonEl) {
    terminalGroupManagerCancelButtonEl.addEventListener("click", closeTerminalGroupManagerDialog);
  }
}
