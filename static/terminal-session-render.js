// webClx terminal session selection, rename, and history UI helpers.
// Extracted from terminal.js as global declarations; no top-level DOM setup.

function activeSession() {
  return state.sessions.find((session) => session.id === state.activeSessionId) || null;
}

function renamingSession() {
  return state.sessions.find((session) => session.id === state.renamingSessionId) || null;
}

// Keep this module's rename state private from the workspace-page renderer,
// which is loaded alongside it on the home page.
let terminalPageRenameTriggerEl = null;
let terminalRenameHistoryRequestId = 0;
let terminalModelPeakToastKey = "";
// 对话史建议按钮随改名弹窗存续；renderSessionRenamePresets 重建时必须保留，
// 否则周期性会话刷新（syncSessionRenameEditor）会把它清掉。
let terminalRenameHistoryPresetValue = "";
let terminalRenameHistoryPresetLoading = false;

const terminalSessionActivity = globalThis.WebClxTerminalSessionActivity || {};
const sharedTerminalSessionInModelPeakWindow =
  terminalSessionActivity.terminalSessionInModelPeakWindow || (() => false);
const sharedTerminalModelPeakTransitionKey =
  terminalSessionActivity.terminalModelPeakTransitionKey || (() => "");
const sharedTerminalModelPeakFamily =
  terminalSessionActivity.terminalModelPeakFamily || (() => "");
const sharedTerminalGroupedSessionIds =
  terminalSessionActivity.terminalGroupedSessionIds || (() => new Set());
const sharedTerminalGroupColor =
  terminalSessionActivity.terminalGroupColor || (() => "");

function notifyTerminalModelPeakEntries(sessions) {
  const activeSessions = (Array.isArray(sessions) ? sessions : [])
    .filter((session) => sharedTerminalSessionInModelPeakWindow(session));
  const nextKey = sharedTerminalModelPeakTransitionKey(sessions);
  const changed = nextKey !== terminalModelPeakToastKey;
  terminalModelPeakToastKey = nextKey;
  if (!changed || activeSessions.length === 0 || typeof showToast !== "function") {
    return;
  }
  const labels = activeSessions.map((session) => {
    const family = sharedTerminalModelPeakFamily(session) || "高峰";
    return `${session?.name || session?.id || "终端"}（${family}）`;
  });
  showToast(`${labels.join("、")} 已进入忙时高峰时段`, "warn", 8000);
}

function syncTerminalModelPeakToasts(sessions) {
  notifyTerminalModelPeakEntries(sessions);
}

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
  if (state.renamingSessionId) {
    if (terminalRenameHistoryPresetLoading) {
      const loadingButton = document.createElement("button");
      loadingButton.className = "button secondary session-rename-preset";
      loadingButton.type = "button";
      loadingButton.disabled = true;
      loadingButton.dataset.historyPreset = "true";
      loadingButton.textContent = "对话史…";
      sessionRenamePresetsEl.appendChild(loadingButton);
    } else if (terminalRenameHistoryPresetValue) {
      const historyButton = createSessionRenameHistoryButton(
        terminalRenameHistoryPresetValue,
        terminalRenameHistoryPresetValue,
        `追加 _${terminalRenameHistoryPresetValue}`,
      );
      historyButton.dataset.historyPreset = "true";
      sessionRenamePresetsEl.appendChild(historyButton);
    }
  }
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

function createSessionRenameHistoryButton(label, value, title) {
  const button = document.createElement("button");
  button.className = "button secondary session-rename-preset";
  button.type = "button";
  button.setAttribute("data-action", "append-session-rename-preset");
  button.dataset.preset = value;
  button.textContent = label;
  button.title = title;
  return button;
}

function renderSessionRenameHistoryButton(value) {
  if (!state.renamingSessionId) {
    return;
  }

  terminalRenameHistoryPresetLoading = false;
  terminalRenameHistoryPresetValue = normalizeTerminalRenamePreset(value);
  renderSessionRenamePresets();
}

function setSessionRenameHistoryLoading(loading) {
  if (!state.renamingSessionId) {
    return;
  }

  terminalRenameHistoryPresetLoading = !!loading;
  if (loading) {
    terminalRenameHistoryPresetValue = "";
  }
  renderSessionRenamePresets();
}

async function loadSessionRenameFirstConversation() {
  const sessionId = state.renamingSessionId;
  if (!sessionId) {
    return;
  }

  const requestId = ++terminalRenameHistoryRequestId;
  setSessionRenameHistoryLoading(true);
  try {
    const payload = await requestJson(
      `/api/terminal/sessions/${encodeURIComponent(sessionId)}/input-history`,
    );
    if (requestId !== terminalRenameHistoryRequestId || state.renamingSessionId !== sessionId) {
      return;
    }
    const entries = Array.isArray(payload?.entries) ? payload.entries : [];
    renderSessionRenameHistoryButton(terminalInputHistoryNameFromEntry(entries[0]));
  } catch {
    if (requestId === terminalRenameHistoryRequestId && state.renamingSessionId === sessionId) {
      setSessionRenameHistoryLoading(false);
    }
  }
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
  terminalPageRenameTriggerEl = trigger;
  terminalRenameHistoryPresetValue = "";
  terminalRenameHistoryPresetLoading = false;
  sessionRenameInputEl.value = name;
  updateTerminalRenameDialogStatus("");
  setSessionRenameControlsDisabled(false);
  renderSessionRenamePresets();
  loadSessionRenameFirstConversation();
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
  const trigger = terminalPageRenameTriggerEl;
  terminalPageRenameTriggerEl = null;
  state.renamingSessionId = "";
  terminalRenameHistoryRequestId += 1;
  terminalRenameHistoryPresetValue = "";
  terminalRenameHistoryPresetLoading = false;
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
    // Desktop dialogs share terminal IME recovery; a delayed button focus
    // must not steal the helper focus after that recovery has completed.
    if (hasPrimaryTouchInput()) trigger?.focus();
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
  reconcileTerminalSessionOptions(idleSessionSelectEl, [
    {
      value: "",
      text: sessions.length > 0 ? "闲置终端" : "暂无闲置",
      title: "",
      workspacePath: "",
    },
    ...sessions.map((session) => ({
      value: session.id,
      text: sessionOptionLabel(session),
      title: sessionOptionTitle(session),
      workspacePath: sessionPath(session),
      iconProjectPath: session.icon_project_path || sessionPath(session),
      peak: sharedTerminalSessionInModelPeakWindow(session),
    })),
  ]);

  idleSessionSelectEl.disabled = sessions.length === 0;
  // 选中即恢复：选择被消费后回到占位。
  idleSessionSelectEl.value = "";
  idleSessionSelectEl.workspaceIconSelectController?.sync();
}

function renderHiddenSessions() {
  if (!hiddenSessionSelectEl) {
    return;
  }

  const sessions = typeof hiddenSessions === "function" ? hiddenSessions() : [];
  reconcileTerminalSessionOptions(hiddenSessionSelectEl, [
    {
      value: "",
      text: sessions.length > 0 ? `隐藏专区 (${sessions.length})` : "暂无隐藏",
      title: "",
      workspacePath: "",
    },
    ...sessions.map((session) => ({
      value: session.id,
      text: sessionOptionLabel(session),
      title: sessionOptionTitle(session),
      workspacePath: sessionPath(session),
      iconProjectPath: session.icon_project_path || sessionPath(session),
      peak: sharedTerminalSessionInModelPeakWindow(session),
    })),
  ]);

  hiddenSessionSelectEl.disabled = sessions.length === 0;
  hiddenSessionSelectEl.value = "";
  hiddenSessionSelectEl.workspaceIconSelectController?.sync();
}

function isManagedTerminalSession(session) {
  return session?.origin === "workflow" || session?.origin === "agent";
}

function terminalSelectorGroupRoleKey(role) {
  const normalized = String(role || "").trim().toLowerCase();
  if (["manager", "管理者", "管理", "计划者", "planner"].includes(normalized)) return "manager";
  return "";
}

// 下拉列表的管理者标记只认分组对话框显式指定的角色；没有人被指定时，按
// 创建时间取稳定首位兜底。不能按 group_order 兜底——激活终端会把当前终端
// 轮转到组首（“编组跟随终端”只是显示顺序），皇冠不能跟着这次轮转漂移。
function terminalSelectorGroupManagerId(members) {
  if (!Array.isArray(members) || members.length === 0) return "";
  const designated = members.find(
    (member) => terminalSelectorGroupRoleKey(member?.group_role) === "manager",
  );
  if (designated?.id) return String(designated.id);
  const stable = [...members].sort((left, right) => {
    const leftTime = Number(left?.created_at ?? left?.createdAt ?? 0) || 0;
    const rightTime = Number(right?.created_at ?? right?.createdAt ?? 0) || 0;
    if (leftTime !== rightTime) return leftTime - rightTime;
    return String(left?.id || "").localeCompare(String(right?.id || ""));
  });
  return String(stable[0]?.id || "");
}

function reconcileTerminalSessionOptions(selectEl, desiredEntries) {
  const existingByValue = new Map(
    Array.from(selectEl.options || selectEl.children || []).map((option) => [option.value, option]),
  );
  const desiredOptions = desiredEntries.map((entry) => {
    const option = existingByValue.get(entry.value) || document.createElement("option");
    option.value = entry.value;
    option.dataset.workspacePath = entry.workspacePath;
    option.dataset.iconProjectPath = entry.iconProjectPath || entry.workspacePath;
    option.classList.toggle("session-option-peak", Boolean(entry.peak));
    option.dataset.grouped = entry.grouped ? "true" : "false";
    option.dataset.groupRole = entry.groupRole || "";
    option.dataset.groupName = entry.groupName || "";
    if (entry.groupColor) option.dataset.groupColor = entry.groupColor;
    else delete option.dataset.groupColor;
    option.textContent = entry.text;
    option.title = entry.title;
    return option;
  });

  desiredOptions.forEach((option, index) => {
    const currentOption = (selectEl.options || selectEl.children || [])[index] || null;
    if (currentOption !== option) {
      if (typeof selectEl.insertBefore === "function") {
        selectEl.insertBefore(option, currentOption);
      } else {
        selectEl.appendChild(option);
      }
    }
  });

  const desiredOptionSet = new Set(desiredOptions);
  Array.from(selectEl.options || selectEl.children || []).forEach((option) => {
    if (!desiredOptionSet.has(option)) {
      if (typeof selectEl.removeChild === "function") {
        selectEl.removeChild(option);
      }
    }
  });
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

  const desiredEntries = [];
  if (sessions.length === 0 || includePlaceholder) {
    desiredEntries.push({
      value: "",
      text: sessions.length > 0 ? emptyLabel : `暂无${emptyLabel}`,
      title: "",
      workspacePath: "",
    });
  }

  const groupedIds = sharedTerminalGroupedSessionIds(sessions);
  const groupedMembers = new Map();
  for (const candidate of sessions) {
    const token = String(candidate?.group_token || "").trim();
    if (!token || !groupedIds.has(candidate.id)) continue;
    if (!groupedMembers.has(token)) groupedMembers.set(token, []);
    groupedMembers.get(token).push(candidate);
  }
  for (const members of groupedMembers.values()) {
    members.sort((left, right) => Number(left.group_order || 0) - Number(right.group_order || 0) || String(left.id).localeCompare(String(right.id)));
  }
  const groupManagerIds = new Map(
    [...groupedMembers].map(([token, members]) => [token, terminalSelectorGroupManagerId(members)]),
  );
  for (const session of sessions) {
    const token = String(session?.group_token || "").trim();
    const members = groupedMembers.get(token) || [];
    const managerId = groupManagerIds.get(token) || "";
    desiredEntries.push({
      value: session.id,
      text: sessionOptionLabel(session),
      title: sessionOptionTitle(session),
      workspacePath: sessionPath(session),
      iconProjectPath: session.icon_project_path || sessionPath(session),
      peak: sharedTerminalSessionInModelPeakWindow(session),
      grouped: groupedIds.has(session.id),
      groupColor: sharedTerminalGroupColor(session, sessions),
      groupName: String(session.group_name || session.groupName || "").trim(),
      groupRole: managerId ? (session.id === managerId ? "manager" : "executor") : "",
      groupRoleName: managerId ? (session.id === managerId ? "管理者" : "执行者") : "",
      groupRoleMembers: members.map((member) => ({
        id: String(member.id || ""),
        name: String(member.name || member.id || ""),
        role: member.id === managerId ? "manager" : "executor",
      })),
    });
  }

  reconcileTerminalSessionOptions(selectEl, desiredEntries);

  selectEl.disabled = sessions.length === 0;
  if (sessions.some((session) => session.id === activeSessionId)) {
    selectEl.value = activeSessionId;
  } else {
    selectEl.value = includePlaceholder ? "" : sessions[0]?.id || "";
  }
  selectEl.workspaceIconSelectController?.sync();
}

function renderSessions() {
  const sessions = visibleSessions();

  if (typeof syncAutoCompactToggleForActiveSession === "function") {
    syncAutoCompactToggleForActiveSession();
  }

  syncTerminalSessionSortControl();

  if (state?.showSessionContext && typeof maybeRefreshSessionsContext === "function") {
    maybeRefreshSessionsContext();
  }

  if (sessions.length === 0) {
    closeSessionRenameEditor();
    const emptyLabel = idleSessions().length > 0 ? "活动终端已闲置" : "终端";
    renderTerminalSessionSelector(sessionSelectEl, [], "", emptyLabel);
    renderTerminalSessionSelector(agentSessionSelectEl, [], "", "智能体终端");
    if (renameSessionButton) {
      renameSessionButton.disabled = true;
    }
    if (saveExperienceSessionButton) {
      saveExperienceSessionButton.disabled = true;
    }
    if (deleteSessionButton) {
      deleteSessionButton.disabled = true;
    }
    if (idleSessionButton) {
      idleSessionButton.disabled = !activeSession() || isIdleSession(activeSession().id);
    }
    renderIdleSessions();
    renderHiddenSessions();
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
  if (saveExperienceSessionButton) {
    saveExperienceSessionButton.disabled = !current || isIdleSession(current.id);
  }
  if (deleteSessionButton) {
    deleteSessionButton.disabled = !current || isIdleSession(current.id);
  }
  if (idleSessionButton) {
    idleSessionButton.disabled = !current || isIdleSession(current.id);
  }

  renderIdleSessions();
  renderHiddenSessions();
  syncTerminalModelPeakToasts(sessions);
  syncSessionRenameEditor();
  syncTerminalInputHistoryButton();
}

if (typeof module === "object" && module.exports) {
  module.exports = { terminalSelectorGroupManagerId, terminalSelectorGroupRoleKey };
}
