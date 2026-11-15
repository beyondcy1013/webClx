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
    for (const pendingId of Array.from(state.pendingCreatedSessionIds)) {
      if (!fetchedSessionIds.has(pendingId) && !isIdleSession(pendingId)) {
        const pendingSession = sessionsBeforeLoad.find((session) => session.id === pendingId);
        if (pendingSession && !fetchedSessions.some((s) => s.id === pendingId)) {
          fetchedSessions.push(pendingSession);
        }
      }
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
    // 自动压缩默认开启，无需在创建时逐个加入集合。
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

function resolveTerminalGroupMemberInitialRole(item, currentSession, allSessions, currentToken) {
  const persistedRole = terminalGroupRoleValue(item?.group_role);
  if (persistedRole) return persistedRole;

  const nameRole = terminalGroupRoleValue(terminalCollaborationRenameRole(item?.name));
  if (nameRole) return nameRole;

  let fallbackRole = "executor";
  if (item?.id === currentSession?.id) {
    if (!currentToken || currentToken !== String(item?.group_token || "")) {
      fallbackRole = "manager";
    } else {
      const groupMembers = (allSessions || []).filter((s) => String(s?.group_token || "") === currentToken);
      const hasOtherManager = groupMembers.some((s) => s.id !== currentSession.id && (
        ["super_admin", "manager"].includes(terminalGroupRoleValue(s.group_role)
          || terminalGroupRoleValue(terminalCollaborationRenameRole(s.name)))
      ));
      fallbackRole = hasOtherManager ? "executor" : "manager";
    }
  }
  return fallbackRole;
}

let activeRoleMenuTrigger = null;
let activeRoleMenuSelect = null;
let roleMenuEl = null;

function ensureTerminalGroupRoleMenu() {
  if (roleMenuEl && roleMenuEl.isConnected) return roleMenuEl;
  let menu = document.getElementById("terminal-group-role-popup-menu");
  if (!menu) {
    menu = document.createElement("div");
    menu.id = "terminal-group-role-popup-menu";
    menu.className = "terminal-group-role-menu";
    menu.setAttribute("role", "listbox");
    menu.setAttribute("aria-label", "选择群组角色");
    menu.hidden = true;
  }
  const dialog = (typeof terminalGroupDialogEl !== "undefined" && terminalGroupDialogEl)
    || document.getElementById("terminal-group-dialog")
    || document.body;
  if (menu.parentElement !== dialog) {
    dialog.appendChild(menu);
  }
  roleMenuEl = menu;
  return roleMenuEl;
}

function positionTerminalGroupRoleMenu(trigger, menu) {
  if (!trigger || !menu || menu.hidden) return;
  const triggerRect = trigger.getBoundingClientRect();
  const viewportWidth = window.innerWidth || document.documentElement.clientWidth;
  const viewportHeight = window.innerHeight || document.documentElement.clientHeight;
  const margin = 8;
  const gap = 4;

  const width = Math.max(triggerRect.width, 96);
  const spaceBelow = viewportHeight - triggerRect.bottom - gap - margin;
  const spaceAbove = triggerRect.top - gap - margin;
  const openAbove = spaceBelow < 110 && spaceAbove > spaceBelow;

  menu.style.position = "fixed";
  menu.style.width = `${Math.round(width)}px`;
  menu.style.left = `${Math.round(Math.min(
    Math.max(margin, triggerRect.left),
    viewportWidth - width - margin,
  ))}px`;
  if (openAbove) {
    const menuHeight = menu.offsetHeight || 110;
    menu.style.top = `${Math.round(Math.max(margin, triggerRect.top - gap - menuHeight))}px`;
  } else {
    menu.style.top = `${Math.round(triggerRect.bottom + gap)}px`;
  }
}

function closeTerminalGroupRoleMenu({ restoreFocus = false } = {}) {
  const trigger = activeRoleMenuTrigger;
  activeRoleMenuTrigger = null;
  activeRoleMenuSelect = null;
  if (roleMenuEl) {
    roleMenuEl.hidden = true;
    roleMenuEl.replaceChildren();
  }
  if (trigger) {
    trigger.setAttribute("aria-expanded", "false");
    trigger.classList.remove("is-open");
    if (restoreFocus && trigger.isConnected) {
      try {
        trigger.focus({ preventScroll: true });
      } catch {
        trigger.focus();
      }
    }
  }
}

function openTerminalGroupRoleMenu(trigger, select) {
  if (!trigger || !select || select.disabled || trigger.disabled) {
    return;
  }
  const menu = ensureTerminalGroupRoleMenu();
  activeRoleMenuTrigger = trigger;
  activeRoleMenuSelect = select;
  trigger.setAttribute("aria-expanded", "true");
  trigger.classList.add("is-open");

  menu.replaceChildren();
  const options = [
    ["super_admin", "👑 超级管理员"],
    ["manager", "普通管理员"],
    ["executor", "执行者"],
    ["reviewer", "复核者"],
  ];
  for (const [val, label] of options) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "terminal-group-role-option";
    item.setAttribute("role", "option");
    item.dataset.value = val;
    item.textContent = label;
    const isSelected = select.value === val;
    item.setAttribute("aria-selected", isSelected ? "true" : "false");
    if (isSelected) {
      item.classList.add("is-selected");
    }
    item.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      const changed = select.value !== val;
      select.value = val;
      syncTerminalGroupRoleTrigger(select);
      closeTerminalGroupRoleMenu({ restoreFocus: true });
      if (changed) {
        select.dispatchEvent(new Event("change", { bubbles: true }));
      }
    });
    menu.appendChild(item);
  }

  menu.hidden = false;
  positionTerminalGroupRoleMenu(trigger, menu);
  if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(() => {
      if (activeRoleMenuTrigger === trigger) {
        positionTerminalGroupRoleMenu(trigger, menu);
      }
    });
  }
}

function toggleTerminalGroupRoleMenu(trigger, select) {
  if (activeRoleMenuTrigger === trigger && roleMenuEl && !roleMenuEl.hidden) {
    closeTerminalGroupRoleMenu();
  } else {
    closeTerminalGroupRoleMenu();
    openTerminalGroupRoleMenu(trigger, select);
  }
}

function syncTerminalGroupRoleTrigger(select) {
  if (!select) return;
  const picker = select.closest(".terminal-group-role-picker");
  if (!picker) return;
  const trigger = picker.querySelector(".terminal-group-role-trigger");
  if (!trigger) return;
  const labelEl = trigger.querySelector(".terminal-group-role-trigger-label");
  const roleLabel = (select.value === "super_admin" ? "👑 " : "") + (terminalGroupRoleLabel(select.value) || "执行者");
  if (labelEl) {
    labelEl.textContent = roleLabel;
  }
  trigger.disabled = Boolean(select.disabled);
  trigger.classList.toggle("is-manager", select.value === "manager");
  trigger.title = `当前角色：${roleLabel}`;
}

function resolveTerminalSessionAgentType(session) {
  const direct = String(session?.activity_agent || session?.activityAgent || "").trim();
  if (direct) {
    return direct;
  }
  if (session?.codex_api_preset_name) {
    return "Codex";
  }
  const name = String(session?.name || "").toLowerCase();
  if (name.includes("agy")) return "AGY";
  if (name.includes("claude")) return "Claude";
  if (name.includes("codebuddy")) return "CodeBuddy";
  if (name.includes("devin")) return "Devin";
  if (name.includes("qoder")) return "Qoder";
  if (name.includes("cdx") || name.includes("codex")) return "Codex";
  if (session?.origin === "agent") return "Agent";
  return "Shell";
}

function renderTerminalGroupSessionList(session) {
  closeTerminalGroupRoleMenu();
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
    option.title = String(item.name || item.id);
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.value = item.id;
    checkbox.checked = item.id === session.id || (!!currentToken && String(item.group_token || "") === currentToken);
    if (item.id === session.id) {
      checkbox.disabled = true;
    }
    option.appendChild(checkbox);
    const roleSelect = document.createElement("select");
    roleSelect.className = "terminal-group-role-select terminal-group-role-native-select";
    roleSelect.setAttribute("aria-label", `设置 ${item.name || item.id} 的群组角色`);
    roleSelect.dataset.roleFor = item.id;
    roleSelect.tabIndex = -1;
    roleSelect.setAttribute("aria-hidden", "true");
    [["super_admin", "👑 超级管理员"], ["manager", "普通管理员"], ["executor", "执行者"], ["reviewer", "复核者"]].forEach(
      ([value, label]) => {
        const roleOption = document.createElement("option");
        roleOption.value = value;
        roleOption.textContent = label;
        roleSelect.append(roleOption);
      },
    );
    roleSelect.value = resolveTerminalGroupMemberInitialRole(item, session, sessions, currentToken);
    roleSelect.disabled = !checkbox.checked;

    const rolePicker = document.createElement("div");
    rolePicker.className = "terminal-group-role-picker";

    const roleTrigger = document.createElement("button");
    roleTrigger.type = "button";
    roleTrigger.className = "terminal-group-role-trigger";
    roleTrigger.setAttribute("aria-haspopup", "listbox");
    roleTrigger.setAttribute("aria-expanded", "false");
    roleTrigger.setAttribute("aria-label", `设置 ${item.name || item.id} 的群组角色`);
    roleTrigger.disabled = roleSelect.disabled;

    const roleTriggerLabel = document.createElement("span");
    roleTriggerLabel.className = "terminal-group-role-trigger-label";
    roleTriggerLabel.textContent = (roleSelect.value === "super_admin" ? "👑 " : "") + (terminalGroupRoleLabel(roleSelect.value) || "执行者");
    roleTrigger.appendChild(roleTriggerLabel);

    const roleTriggerArrow = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    roleTriggerArrow.setAttribute("class", "terminal-group-role-trigger-arrow");
    roleTriggerArrow.setAttribute("viewBox", "0 0 10 6");
    roleTriggerArrow.setAttribute("aria-hidden", "true");
    roleTriggerArrow.setAttribute("fill", "currentColor");
    const roleTriggerArrowPath = document.createElementNS("http://www.w3.org/2000/svg", "path");
    roleTriggerArrowPath.setAttribute("d", "M0 0.5L5 5.5L10 0.5Z");
    roleTriggerArrow.appendChild(roleTriggerArrowPath);
    roleTrigger.appendChild(roleTriggerArrow);

    roleTrigger.addEventListener("pointerdown", (event) => {
      event.stopPropagation();
    });
    roleTrigger.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      toggleTerminalGroupRoleMenu(roleTrigger, roleSelect);
    });

    rolePicker.appendChild(roleSelect);
    rolePicker.appendChild(roleTrigger);

    roleSelect.addEventListener("change", () => {
      syncTerminalGroupRoleTrigger(roleSelect);
      enforceTerminalGroupManagerInvariant(roleSelect);
    });
    checkbox.addEventListener("change", () => {
      roleSelect.disabled = !checkbox.checked;
      syncTerminalGroupRoleTrigger(roleSelect);
      enforceTerminalGroupManagerInvariant(null);
    });
    option.appendChild(rolePicker);
    const agentSpan = document.createElement("span");
    agentSpan.className = "terminal-group-session-agent";
    const agentLabel = resolveTerminalSessionAgentType(item);
    agentSpan.textContent = agentLabel;
    agentSpan.dataset.agent = agentLabel;
    agentSpan.title = `智能体类型：${agentLabel}`;
    option.appendChild(agentSpan);
    const nameSpan = document.createElement("span");
    nameSpan.className = "terminal-group-session-name";
    nameSpan.textContent = String(item.name || item.id);
    nameSpan.title = String(item.name || item.id);
    option.appendChild(nameSpan);
    if (item.id === session.id) {
      const currentBadge = document.createElement("span");
      currentBadge.className = "terminal-group-current-badge";
      currentBadge.textContent = "当前";
      currentBadge.title = "当前所在终端";
      option.appendChild(currentBadge);
    }
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
    const itemGroupToken = String(item.group_token || "").trim();
    if (itemGroupToken && item.id !== session.id) {
      if (currentToken && itemGroupToken === currentToken) {
        const marker = document.createElement("span");
        marker.className = "terminal-group-session-marker";
        marker.textContent = "已同组";
        option.appendChild(marker);
      } else {
        const marker = document.createElement("button");
        marker.type = "button";
        marker.className = "terminal-group-session-marker is-other-group";
        const targetGroupName = currentTerminalGroupName(item) || "现有群组";
        marker.textContent = `加入: ${targetGroupName}`;
        marker.title = `点击将当前终端加入「${targetGroupName}」群组`;
        const groupColor = currentTerminalGroupColor(item);
        if (groupColor) {
          marker.style.setProperty("--badge-group-color", groupColor);
        }
        marker.addEventListener("click", (event) => {
          event.preventDefault();
          event.stopPropagation();
          applyTerminalGroupJoinSelection(itemGroupToken, session);
        });
        option.appendChild(marker);
      }
    }
    terminalGroupSessionListEl.appendChild(option);
  }
  enforceTerminalGroupManagerInvariant(null);
  applyTerminalGroupSessionListOrder();
  syncTerminalGroupOrderButtons(session);
}

// 超级管理员拥有组内最高决策权；普通管理员可并存。每级最多一名，
// 没有任何管理角色时才补普通管理员。只处理已勾选成员，不修改其他分组。
function normalizeTerminalGroupLeadership(members, currentId, exclusiveId = "") {
  for (const role of ["super_admin", "manager"]) {
    const candidates = members.filter((member) => member.role === role);
    const keep = candidates.find((member) => member.id === exclusiveId) || candidates[0];
    for (const member of candidates) {
      if (member !== keep) member.role = role === "super_admin" ? "manager" : "executor";
    }
  }
  if (!members.some((member) => ["super_admin", "manager"].includes(member.role))) {
    const designated = members.find((member) => terminalCollaborationRenameRole(member.name) === "普通管理员");
    const fallback = designated || members.find((member) => member.id === currentId) || members[0];
    if (fallback) fallback.role = "manager";
  }
}

function enforceTerminalGroupManagerInvariant(exclusiveSelect) {
  if (!terminalGroupSessionListEl) return;
  const rows = Array.from(terminalGroupSessionListEl.querySelectorAll(".terminal-group-session-option"));
  const members = rows.filter((row) => row.querySelector('input[type="checkbox"]')?.checked === true)
    .map((row) => {
      const select = row.querySelector("select[data-role-for]");
      return { id: select.dataset.roleFor, name: row.dataset.sessionName, role: select.value, select };
    });
  normalizeTerminalGroupLeadership(members, terminalGroupDialogEl?.dataset.sessionId || "", exclusiveSelect?.dataset.roleFor);
  for (const member of members) member.select.value = member.role;
  for (const row of rows) {
    const checked = row.querySelector('input[type="checkbox"]')?.checked === true;
    const select = row.querySelector("select[data-role-for]");
    row.classList.toggle("is-manager", Boolean(checked && ["super_admin", "manager"].includes(select?.value)));
    if (select) syncTerminalGroupRoleTrigger(select);
  }
}

function currentTerminalGroupColor(session) {
  const activity = globalThis.WebClxTerminalSessionActivity;
  if (activity?.terminalGroupColor && Array.isArray(state?.sessions)) {
    const resolved = activity.terminalGroupColor(session, state.sessions);
    if (resolved && resolved.startsWith("#")) return resolved;
  }
  const normalizeColor = (activity?.normalizeTerminalGroupColor || ((value) => String(value || "")))(
    session?.group_color || session?.groupColor,
  );
  return normalizeColor;
}

function syncTerminalGroupColorControls(session) {
  if (!terminalGroupColorEl || !terminalGroupColorValueEl || !session) return;
  const color = currentTerminalGroupColor(session);
  const fallbackColor = (globalThis.WebClxTerminalSessionActivity?.defaultUnusedTerminalGroupColor || (() => "#4f8cff"))(
    session,
    state?.sessions,
  );
  terminalGroupColorEl.value = color || fallbackColor;
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
  closeTerminalGroupRoleMenu();
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

function getAvailableTerminalGroups(currentSessionId = "") {
  const groupsByToken = new Map();
  const all = Array.isArray(state?.sessions) ? state.sessions : [];
  for (const item of all) {
    const token = String(item.group_token || item.groupToken || "").trim();
    if (!token) continue;
    if (!groupsByToken.has(token)) {
      groupsByToken.set(token, {
        token,
        name: "",
        color: "",
        fixedOrder: false,
        members: [],
      });
    }
    const group = groupsByToken.get(token);
    if (!group.name) {
      group.name = String(item.group_name || item.groupName || "").trim();
    }
    if (!group.color) {
      const color = String(item.group_color || item.groupColor || "").trim();
      if (/^#[0-9a-fA-F]{6}$/.test(color)) group.color = color.toLowerCase();
    }
    if (item.group_fixed_order) group.fixedOrder = true;
    group.members.push(item);
  }
  return Array.from(groupsByToken.values())
    .filter((group) => group.members.length >= 2)
    .map((group) => {
    const label = group.name || group.members.map((m) => m.name || m.id).slice(0, 3).join("、") || group.token;
    return {
      ...group,
      label,
      isCurrentGroup: group.members.some((m) => m.id === currentSessionId),
    };
  });
}

function renderTerminalGroupJoinControls(session) {
  const joinContainer = document.getElementById("terminal-group-join-container");
  const joinSelect = document.getElementById("terminal-group-join-select");
  const joinButton = document.getElementById("terminal-group-join-button");
  const joinRenameButton = document.getElementById("terminal-group-rename-button");
  const joinHint = document.getElementById("terminal-group-join-hint");
  if (!joinContainer || !joinSelect) return;

  const currentToken = String(session?.group_token || "").trim();
  const availableGroups = getAvailableTerminalGroups(session?.id);
  const joinableGroups = availableGroups.filter((g) => g.members.some((m) => m.id !== session?.id));

  if (joinableGroups.length === 0) {
    joinContainer.hidden = true;
    joinSelect.innerHTML = '<option value="">— 选择现有群组加入 —</option>';
    if (joinButton) joinButton.disabled = true;
    if (joinRenameButton) joinRenameButton.disabled = true;
    if (joinHint) {
      joinHint.textContent = "";
      joinHint.hidden = true;
    }
    return;
  }

  joinContainer.hidden = false;
  joinSelect.innerHTML = "";

  const defaultOption = document.createElement("option");
  defaultOption.value = "";
  defaultOption.textContent = currentToken
    ? "— 保持当前编组或自定义选择 —"
    : "— 选择现有群组加入（可选）—";
  joinSelect.appendChild(defaultOption);

  for (const group of joinableGroups) {
    const option = document.createElement("option");
    option.value = group.token;
    const memberNames = group.members.map((m) => m.name || m.id).join("、");
    const count = group.members.length;
    const groupNameText = group.name ? `「${group.name}」` : "未命名群组";
    option.textContent = `${groupNameText} (${count}人: ${memberNames})`;
    if (currentToken && group.token === currentToken) {
      option.selected = true;
    }
    joinSelect.appendChild(option);
  }

  if (joinButton) {
    joinButton.disabled = !joinSelect.value;
  }
  if (joinRenameButton) {
    joinRenameButton.disabled = !joinSelect.value;
  }

  if (joinHint) {
    if (currentToken) {
      const myGroup = availableGroups.find((g) => g.token === currentToken);
      joinHint.textContent = myGroup?.name
        ? `当前终端属于「${myGroup.name}」群组。可选择切换加入其他现有群组。`
        : "当前终端已在编组中。可选择切换加入其他现有群组。";
    } else {
      joinHint.textContent = "当前终端尚未加入群组。选择上方已有群组可一键填入现有成员，保存后即可加入该群组。";
    }
    joinHint.hidden = false;
  }

  if (!joinSelect.dataset.bound) {
    joinSelect.dataset.bound = "true";
    joinSelect.addEventListener("change", () => {
      if (joinButton) joinButton.disabled = !joinSelect.value;
      if (joinRenameButton) joinRenameButton.disabled = !joinSelect.value;
      const currentActive = terminalGroupDialogEl?.dataset.sessionId
        ? (state?.sessions || []).find((s) => s.id === terminalGroupDialogEl.dataset.sessionId)
        : activeSession();
      if (currentActive) {
        applyTerminalGroupJoinSelection(joinSelect.value, currentActive);
      }
    });
  }
}

function applyTerminalGroupJoinSelection(token, session) {
  if (!terminalGroupSessionListEl) return;
  const joinSelect = document.getElementById("terminal-group-join-select");
  const joinButton = document.getElementById("terminal-group-join-button");
  const joinRenameButton = document.getElementById("terminal-group-rename-button");
  const joinHint = document.getElementById("terminal-group-join-hint");
  if (joinSelect && joinSelect.value !== token) {
    joinSelect.value = token;
  }
  if (joinButton) {
    joinButton.disabled = !token;
  }
  if (joinRenameButton) {
    joinRenameButton.disabled = !token;
  }

  if (!token) {
    const checkboxes = terminalGroupSessionListEl.querySelectorAll('input[type="checkbox"]');
    checkboxes.forEach((cb) => {
      const isCurrent = cb.value === session.id;
      cb.checked = isCurrent;
      const roleSelect = cb.closest(".terminal-group-session-option")?.querySelector("select[data-role-for]");
      if (roleSelect) {
        roleSelect.disabled = !isCurrent;
        syncTerminalGroupRoleTrigger(roleSelect);
      }
    });
    enforceTerminalGroupManagerInvariant(null);
    if (joinHint) {
      joinHint.textContent = "已恢复为自定义勾选状态。可在列表中手动勾选终端。";
    }
    return;
  }

  const availableGroups = getAvailableTerminalGroups(session.id);
  const targetGroup = availableGroups.find((g) => g.token === token);
  if (!targetGroup) return;

  const targetMemberIds = new Set(targetGroup.members.map((m) => m.id));
  targetMemberIds.add(session.id);

  if (terminalGroupNameEl) {
    terminalGroupNameEl.value = targetGroup.name || "";
  }
  if (terminalGroupColorEl) {
    const color = targetGroup.color || currentTerminalGroupColor(targetGroup.members[0]) || "#4f8cff";
    terminalGroupColorEl.value = color;
    if (terminalGroupColorValueEl) terminalGroupColorValueEl.textContent = color;
  }
  if (terminalGroupFixedOrderEl) {
    terminalGroupFixedOrderEl.checked = targetGroup.fixedOrder === true;
  }

  const rows = terminalGroupSessionListEl.querySelectorAll(".terminal-group-session-option");
  rows.forEach((row) => {
    const cb = row.querySelector('input[type="checkbox"]');
    const roleSelect = row.querySelector("select[data-role-for]");
    if (!cb || !roleSelect) return;

    const isMember = targetMemberIds.has(cb.value);
    cb.checked = isMember;
    roleSelect.disabled = !isMember;

    if (isMember) {
      const existingMember = targetGroup.members.find((m) => m.id === cb.value);
      if (existingMember) {
        roleSelect.value = resolveTerminalGroupMemberInitialRole(
          existingMember,
          session,
          state.sessions,
          targetGroup.token,
        );
      } else if (cb.value === session.id) {
        if (!roleSelect.value || roleSelect.value === "manager" || roleSelect.value === "super_admin") {
          roleSelect.value = "executor";
        }
      }
    }
    syncTerminalGroupRoleTrigger(roleSelect);
  });

  enforceTerminalGroupManagerInvariant(null);
  syncTerminalGroupOrderButtons(session);

  const tokenDisplay = document.getElementById("terminal-group-token-display");
  if (tokenDisplay) {
    const groupNameText = targetGroup.name ? `「${targetGroup.name}」` : "所选群组";
    tokenDisplay.textContent = `已载入现有群组${groupNameText}成员。当前终端将作为新成员加入该群组。`;
    tokenDisplay.hidden = false;
  }
  updateTerminalGroupDialogStatus(
    `已选定加入群组「${targetGroup.name || targetGroup.token}」（共 ${targetGroup.members.length + 1} 人）。核对角色后点击「保存编组」生效。`,
    "ok",
  );

  if (joinHint) {
    const groupNameText = targetGroup.name ? `「${targetGroup.name}」` : "所选";
    joinHint.textContent = `已载入${groupNameText}群组全部成员（共 ${targetGroup.members.length} 人）。点击下方「保存编组」即可完成加入。`;
    joinHint.hidden = false;
  }
}

function applyJoinExistingGroup(token) {
  const sessionId = terminalGroupDialogEl?.dataset.sessionId;
  const currentActive = sessionId
    ? (state?.sessions || []).find((s) => s.id === sessionId)
    : activeSession();
  if (currentActive) {
    applyTerminalGroupJoinSelection(token, currentActive);
  }
}

function handleTerminalGroupRenamePrompt(token) {
  const sessionId = terminalGroupDialogEl?.dataset.sessionId;
  const currentActive = sessionId
    ? (state?.sessions || []).find((s) => s.id === sessionId)
    : activeSession();
  const selectedToken = token || document.getElementById("terminal-group-join-select")?.value;
  if (!selectedToken) return;

  const availableGroups = getAvailableTerminalGroups(currentActive?.id);
  const targetGroup = availableGroups.find((g) => g.token === selectedToken);
  if (!targetGroup) return;

  if (currentActive) {
    applyTerminalGroupJoinSelection(selectedToken, currentActive);
  }

  const currentGroupName = targetGroup.name || (terminalGroupNameEl ? terminalGroupNameEl.value.trim() : "");
  const input = typeof window !== "undefined" && typeof window.prompt === "function"
    ? window.prompt("为此群组命名或更名（留空表示清除名称）：", currentGroupName)
    : null;

  if (input === null) {
    if (terminalGroupNameEl) {
      terminalGroupNameEl.focus();
      terminalGroupNameEl.select();
    }
    return;
  }

  const trimmed = input.trim();
  if (terminalGroupNameEl) {
    terminalGroupNameEl.value = trimmed;
    terminalGroupNameEl.focus();
    terminalGroupNameEl.select();
  }

  targetGroup.name = trimmed;
  if (state?.sessions) {
    state.sessions.forEach((s) => {
      if (s.group_token === selectedToken) {
        s.group_name = trimmed;
      }
    });
  }

  const joinSelect = document.getElementById("terminal-group-join-select");
  if (joinSelect) {
    const option = Array.from(joinSelect.options).find((opt) => opt.value === selectedToken);
    if (option) {
      const memberNames = targetGroup.members.map((m) => m.name || m.id).join("、");
      const count = targetGroup.members.length;
      const groupNameText = trimmed ? `「${trimmed}」` : "未命名群组";
      option.textContent = `${groupNameText} (${count}人: ${memberNames})`;
    }
  }

  const displayName = trimmed ? `「${trimmed}」` : "未命名";
  const joinHint = document.getElementById("terminal-group-join-hint");
  if (joinHint) {
    joinHint.textContent = `已将群组命名为${displayName}。核对下方成员与角色后，点击「保存编组」生效。`;
    joinHint.hidden = false;
  }
  updateTerminalGroupDialogStatus(
    `已将群组命名为${displayName}。点击「保存编组」完成保存。`,
    "ok",
  );
}

if (typeof globalThis !== "undefined") {
  globalThis.applyJoinExistingGroup = applyJoinExistingGroup;
  globalThis.handleTerminalGroupRenamePrompt = handleTerminalGroupRenamePrompt;
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
  renderTerminalGroupJoinControls(session);
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
  closeTerminalGroupRoleMenu();
  if (terminalGroupDialogEl?.open) {
    terminalGroupDialogEl.close();
  } else if (terminalGroupDialogEl) {
    terminalGroupDialogEl.removeAttribute("open");
  }
  if (terminalGroupSessionListEl) {
    terminalGroupSessionListEl.textContent = "";
  }
  const joinContainer = document.getElementById("terminal-group-join-container");
  if (joinContainer) joinContainer.hidden = true;
  const joinSelect = document.getElementById("terminal-group-join-select");
  if (joinSelect) joinSelect.value = "";
  const joinButton = document.getElementById("terminal-group-join-button");
  if (joinButton) joinButton.disabled = true;
  const joinHint = document.getElementById("terminal-group-join-hint");
  if (joinHint) {
    joinHint.textContent = "";
    joinHint.hidden = true;
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
  if (/(超级管理员|super_admin)/.test(normalized)) return "超级管理员";
  if (/(普通管理员|管理者|管理|计划者|manager|planner)/.test(normalized)) return "普通管理员";
  if (/(执行者|执行|executor|worker)/.test(normalized)) return "执行者";
  if (/(复核者|复核|reviewer)/.test(normalized)) return "复核者";
  return "";
}

function terminalGroupRoleLabel(role) {
  const normalized = String(role || "").trim();
  if (["super_admin", "超级管理员"].includes(normalized)) return "超级管理员";
  if (["manager", "普通管理员", "管理者"].includes(normalized)) return "普通管理员";
  if (normalized === "reviewer" || normalized === "复核者") return "复核者";
  if (normalized === "executor" || normalized === "执行者") return "执行者";
  return "";
}

// 后端把角色规范化成中文（超级管理员/普通管理员/执行者/复核者）存储，下拉框选项值用英文键；
// 回显和比较前先统一映射，避免持久化角色显示成空白。
function terminalGroupRoleValue(role) {
  const normalized = String(role || "").trim().toLowerCase();
  if (["super_admin", "超级管理员"].includes(normalized)) return "super_admin";
  if (["manager", "普通管理员", "管理者", "管理", "计划者", "planner"].includes(normalized)) return "manager";
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

function terminalSessionRoleKey(role) {
  const norm = String(role || "").trim().toLowerCase();
  if (norm.includes("super_admin") || norm.includes("超级管理员")) return "super_admin";
  if (norm.includes("manager") || norm.includes("普通管理员") || norm.includes("管理员")) return "manager";
  if (norm.includes("reviewer") || norm.includes("复核者")) return "reviewer";
  return "executor";
}

function terminalSessionsHaveCollaborationContact(sessionA, sessionB, allMembers = []) {
  if (!sessionA || !sessionB || sessionA.id === sessionB.id) return false;
  const roleA = terminalSessionRoleKey(sessionA.role || sessionA.group_role);
  const roleB = terminalSessionRoleKey(sessionB.role || sessionB.group_role);

  const hasSuperAdmin = allMembers.some((m) => terminalSessionRoleKey(m.role || m.group_role) === "super_admin");
  const hasManager = allMembers.some((m) => terminalSessionRoleKey(m.role || m.group_role) === "manager");

  // 如果组内无管理员（对等模式），所有成员互通
  if (!hasSuperAdmin && !hasManager) {
    return true;
  }

  // 如果只有超级管理员，没有普通管理员，则超级管理员与所有执行者互通；但两个执行者之间不互通
  if (hasSuperAdmin && !hasManager) {
    if (roleA === "super_admin" || roleB === "super_admin") return true;
    return false;
  }

  // 标准层级：超级管理员(super_admin) <-> 普通管理员(manager) <-> 执行者(executor/reviewer)
  // 超级管理员原则上只与管理员联系，不越级与执行者联系
  if (roleA === "super_admin" || roleB === "super_admin") {
    return roleA === "manager" || roleB === "manager";
  }

  // 普通管理员与执行者/复核者联系
  if (roleA === "manager" || roleB === "manager") {
    return true;
  }

  // 执行者之间无直接联系（执行者1跟2没联系，不相互通知）
  return false;
}

function resolveTerminalGroupUpdateRecipients({
  groupedMembers,
  previousMembers,
  previousGroupName,
  groupName,
}) {
  const isNewGroup = previousMembers.length < 2 && groupedMembers.length >= 2;
  // 初次建群时通知全组
  if (isNewGroup) {
    return groupedMembers;
  }

  const oldGroupName = String(previousGroupName || "").trim();
  const newGroupName = String(groupName || "").trim();
  const groupNameChanged = Boolean(oldGroupName !== newGroupName && (oldGroupName || newGroupName));
  // 群组本身改名，通知组内全部成员
  if (groupNameChanged) {
    return groupedMembers;
  }

  const before = new Map(previousMembers.map((m) => [m.id, m]));
  const added = groupedMembers.filter((m) => !before.has(m.id));
  const changed = groupedMembers.filter((m) => {
    const old = before.get(m.id);
    if (!old) return false;
    return old.role !== m.role;
  });
  const renamed = groupedMembers.filter((m) => {
    const old = before.get(m.id);
    if (!old) return false;
    return (old.name || "") !== (m.name || "");
  });

  const recipientIds = new Set();

  // 1. 新加入成员逻辑：
  // "新加入执行者，只需要通知管理者，不需要通知其它执行者和超级管理员"
  for (const newMember of added) {
    const roleKey = terminalSessionRoleKey(newMember.role);
    if (roleKey === "executor" || roleKey === "reviewer") {
      // 仅通知管理者，排除其它执行者和超级管理员
      for (const m of groupedMembers) {
        if (terminalSessionRoleKey(m.role) === "manager") {
          recipientIds.add(m.id);
        } else if (terminalSessionRoleKey(m.role) === "super_admin" && !groupedMembers.some((g) => terminalSessionRoleKey(g.role) === "manager")) {
          recipientIds.add(m.id);
        }
      }
    } else {
      // 若新加入的是管理员或超级管理员，按联系关系通知有联系的成员
      for (const m of groupedMembers) {
        if (m.id === newMember.id || terminalSessionsHaveCollaborationContact(newMember, m, groupedMembers)) {
          recipientIds.add(m.id);
        }
      }
    }
  }

  // 2. 更名或角色变更逻辑：
  // "终端更名只通知跟它有联系的终端，比如执行者1跟2没联系，则不需要相互通知"
  for (const item of [...renamed, ...changed]) {
    for (const m of groupedMembers) {
      if (m.id === item.id || terminalSessionsHaveCollaborationContact(item, m, groupedMembers)) {
        recipientIds.add(m.id);
      }
    }
  }

  return groupedMembers.filter((m) => recipientIds.has(m.id));
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
  const allGroupMembers = state.sessions.filter((item) =>
    currentToken && String(item.group_token || "") === currentToken
  );
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
    // 终端更名只通知跟它有联系的终端，比如执行者1跟2没联系，则不需要相互通知
    const contactTargets = allGroupMembers.filter((item) =>
      item.id !== session.id && terminalSessionsHaveCollaborationContact(renamed, item, allGroupMembers)
    );
    if (contactTargets.length) {
      Promise.all(contactTargets.map((member) => terminalGroupNotifyDelivery(member.id, {
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
        nameSpan.textContent = String(renamed.name || renamed.id);
        nameSpan.title = String(renamed.name || renamed.id);
      }
      row.dataset.sessionName = String(renamed.name || renamed.id);
      row.title = String(renamed.name || renamed.id);
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
  previousGroupName = "",
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
  const oldGroupName = String(previousGroupName || "").trim();
  const newGroupName = String(groupName || "").trim();
  const groupNameChanged = Boolean(oldGroupName !== newGroupName && (oldGroupName || newGroupName));

  if (!added.length && !removed.length && !changed.length && !groupNameChanged) return "";

  // 纯群组改名通知（无增减员及角色变动）
  if (!added.length && !removed.length && !changed.length && groupNameChanged) {
    const nameDesc = oldGroupName && newGroupName
      ? `「${oldGroupName}」→「${newGroupName}」`
      : (newGroupName ? `为「${newGroupName}」` : `已清除（原名「${oldGroupName}」）`);
    return `[系统消息][群组更新:${newGroupName || oldGroupName || "terminal-group"}] 群组名称已更新${nameDesc}。此消息已发送给你和当前合作方，用于避免误解；无需立即回复。`;
  }

  const lines = [`[系统消息][群组更新:${newGroupName || "terminal-group"}] 成员、终端名或角色已更新。`];
  if (groupNameChanged) {
    const nameDesc = oldGroupName && newGroupName
      ? `「${oldGroupName}」→「${newGroupName}」`
      : (newGroupName ? `「${newGroupName}」` : `已清除`);
    lines.push(`群组名称：${nameDesc}。`);
  }
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
      data: `[协作] 你已改名为「${session.name}」，当前角色是${role}。开始工作前先用 $webclx-terminal-collaboration 技能读取工作须知，并用 webclx-terminal-message 技能的定时消息流程与搭档确认。协作消息一律通过 /api/terminal/scheduled-inputs 设置未来短延时发送（实时消息也延迟数秒），不得同步等待投递返回；排程成功不等于接收，以对方定时 ACK 或原生记录确认，不重复发送正文。`,
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
  previousGroupName,
  groupName,
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
        // 群组更新广播严格只发送给有联系的目标成员（新执行者仅通知管理者，不通知其它执行者和超级管理）
        const targetMembers = resolveTerminalGroupUpdateRecipients({
          groupedMembers,
          previousMembers,
          previousGroupName,
          groupName,
        });
        const updateOutcomes = await Promise.all(targetMembers.map(
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
  const currentSession = state.sessions.find((item) => item.id === sessionId);
  const currentToken = String(currentSession?.group_token || "").trim();

  // 精准识别基准群组 token，避免将独立终端或迁入终端误作为基准导致把其他群组当作旧成员广播
  const selectedJoinToken = document.getElementById("terminal-group-join-select")?.value?.trim();
  let baseToken = "";
  if (selectedJoinToken) {
    baseToken = selectedJoinToken;
  } else {
    const tokenCounts = {};
    for (const id of memberIds) {
      const s = state.sessions.find((item) => item.id === id);
      const tok = String(s?.group_token || "").trim();
      if (tok) {
        tokenCounts[tok] = (tokenCounts[tok] || 0) + 1;
      }
    }
    const sortedTokens = Object.keys(tokenCounts).sort((a, b) => tokenCounts[b] - tokenCounts[a]);
    if (currentToken && memberIds.includes(sessionId) && sortedTokens.includes(currentToken)) {
      baseToken = currentToken;
    } else if (sortedTokens.length > 0) {
      baseToken = sortedTokens[0];
    }
  }

  let previousMembers = [];
  if (baseToken) {
    previousMembers = state.sessions
      .filter((session) => session.group_token && session.group_token === baseToken)
      .map((member) => ({
        id: member.id,
        name: member.name || member.id,
        role: terminalGroupRoleValue(member.group_role)
          || terminalGroupRoleValue(terminalCollaborationRenameRole(member.name))
          || (member.id === sessionId ? "manager" : "executor"),
      }));
  }

  const previousNames = new Map(previousMembers.map((member) => [member.id, member.name]));
  const groupedMembers = memberIds
    .map((id) => state.sessions.find((item) => item.id === id))
    .filter(Boolean)
    .map((member) => ({
      id: member.id,
      name: member.name || member.id,
      role: roleAssignments.find((assignment) => assignment.session_id === member.id)?.role || "",
    }));
  normalizeTerminalGroupLeadership(groupedMembers, sessionId);
  const manager = groupedMembers.find((member) => member.role === "super_admin")
    || groupedMembers.find((member) => member.role === "manager");
  const executors = groupedMembers.filter((member) => member !== manager);
  const baseSession = baseToken ? state.sessions.find((s) => s.group_token === baseToken && s.group_name) : null;
  const previousGroupName = String(baseSession?.group_name || currentSession?.group_name || "").trim();
  const nextGroupName = terminalGroupNameEl ? terminalGroupNameEl.value.trim() : previousGroupName;
  const groupName = nextGroupName || previousGroupName || "";
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
      previousGroupName,
    );

    // 仅在初次组建群组时发送协作建队提示，已有群组加入新成员不再全组重发建队提示（避免骚扰已有执行者）
    const isNewGroupCreation = previousMembers.length < 2 && groupedMembers.length >= 2;
    const shouldSendBrief = isNewGroupCreation && Boolean(manager && executors.length > 0);

    let brief = "";
    if (shouldSendBrief) {
      const skillName = "webclx-terminal-collaboration";
      const task = [
        "这是群组建队提示，不是新任务授权。",
        "先读取角色/搭档/群组信息和 $webclx-terminal-collaboration 工作须知；",
        "除非超级管理员或普通管理员随后下发明确任务，否则保持只读待命，不改动文件、不部署、不新建工作流。",
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
        previousGroupName,
        groupName,
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
  document.addEventListener("pointerdown", (event) => {
    if (!roleMenuEl || roleMenuEl.hidden) return;
    if (roleMenuEl.contains(event.target)) return;
    if (activeRoleMenuTrigger && activeRoleMenuTrigger.contains(event.target)) return;
    closeTerminalGroupRoleMenu();
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && roleMenuEl && !roleMenuEl.hidden) {
      event.preventDefault();
      event.stopPropagation();
      closeTerminalGroupRoleMenu({ restoreFocus: true });
    }
  });

  const sessionListEl = document.getElementById("terminal-group-session-list");
  if (sessionListEl) {
    sessionListEl.addEventListener("scroll", () => {
      if (roleMenuEl && !roleMenuEl.hidden) {
        closeTerminalGroupRoleMenu();
      }
    }, { passive: true });
  }

  if (typeof window !== "undefined") {
    window.addEventListener("resize", () => {
      if (roleMenuEl && !roleMenuEl.hidden) {
        closeTerminalGroupRoleMenu();
      }
    });
  }

  const terminalGroupManagerFormEl = document.getElementById("terminal-group-manager-form");
  if (terminalGroupManagerFormEl) {
    terminalGroupManagerFormEl.addEventListener("submit", submitTerminalGroupManagerRename);
  }
  const terminalGroupManagerCancelButtonEl = document.getElementById("terminal-group-manager-cancel");
  if (terminalGroupManagerCancelButtonEl) {
    terminalGroupManagerCancelButtonEl.addEventListener("click", closeTerminalGroupManagerDialog);
  }
}

if (typeof module === "object" && module.exports) {
  module.exports = {
    terminalCollaborationRenameRole,
    terminalGroupRoleLabel,
    terminalGroupRoleValue,
    normalizeTerminalGroupLeadership,
    resolveTerminalGroupMemberInitialRole,
    getAvailableTerminalGroups,
    renderTerminalGroupJoinControls,
    applyTerminalGroupJoinSelection,
    applyJoinExistingGroup,
    handleTerminalGroupRenamePrompt,
    terminalGroupChangeBrief,
    terminalSessionRoleKey,
    terminalSessionsHaveCollaborationContact,
    resolveTerminalGroupUpdateRecipients,
  };
}
