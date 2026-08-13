// webClx 工作区历史子系统：从 app.js 抽出，保持全局函数声明。
// 依赖 app.js 运行时已初始化的全局：state、workspaceHistory*El、requestJson、updateStatus。
// 必须在 app.js 之前用 <script defer> 加载，函数声明为全局，app.js 内调用方无需改动。

function normalizeWorkspaceHistoryItems(items) {
  if (!Array.isArray(items)) {
    return [];
  }

  const byPath = new Map();
  items.forEach((item) => {
    if (!item || typeof item !== "object") {
      return;
    }

    const rawPath = typeof item.path === "string" ? item.path.trim() : "";
    if (!rawPath) {
      return;
    }

    const path = normalizeAbsolutePath(rawPath);
    const rawTimestamp = Number(item.last_opened_at ?? item.lastOpenedAt ?? 0);
    const lastOpenedAt = Number.isFinite(rawTimestamp) && rawTimestamp > 0 ? rawTimestamp : 0;
    const current = byPath.get(path);
    if (current && current.last_opened_at >= lastOpenedAt) {
      return;
    }

    byPath.set(path, {
      path,
      last_opened_at: lastOpenedAt,
    });
  });

  const normalized = [...byPath.values()];
  normalized.sort((left, right) => right.last_opened_at - left.last_opened_at);
  return normalized.slice(0, MAX_WORKSPACE_HISTORY_ITEMS);
}

function readWorkspaceHistory() {
  try {
    const raw = window.localStorage.getItem(WORKSPACE_HISTORY_STORAGE_KEY);
    if (!raw) {
      return [];
    }

    return normalizeWorkspaceHistoryItems(JSON.parse(raw));
  } catch {
    return [];
  }
}

function storeWorkspaceHistory(items) {
  try {
    window.localStorage.setItem(WORKSPACE_HISTORY_STORAGE_KEY, JSON.stringify(items));
  } catch {
    // Keep working even if localStorage is unavailable.
  }
}

function shouldMigrateCachedWorkspaceHistory() {
  try {
    return window.localStorage.getItem(WORKSPACE_HISTORY_MIGRATED_STORAGE_KEY) !== "1";
  } catch {
    return false;
  }
}

function markWorkspaceHistoryMigrated() {
  try {
    window.localStorage.setItem(WORKSPACE_HISTORY_MIGRATED_STORAGE_KEY, "1");
  } catch {
    // Keep working even if localStorage is unavailable.
  }
}

function mergeWorkspaceHistoryItems(...groups) {
  const merged = [];
  groups.forEach((group) => {
    if (Array.isArray(group)) {
      merged.push(...group);
    }
  });
  return normalizeWorkspaceHistoryItems(merged);
}

function workspaceHistoryItemsEqual(left, right) {
  const normalizedLeft = normalizeWorkspaceHistoryItems(left);
  const normalizedRight = normalizeWorkspaceHistoryItems(right);
  if (normalizedLeft.length !== normalizedRight.length) {
    return false;
  }

  return normalizedLeft.every((item, index) => {
    const other = normalizedRight[index];
    return item.path === other.path && item.last_opened_at === other.last_opened_at;
  });
}

async function persistWorkspaceHistory(
  items,
  { keepalive = false, silent = false, markMigrated = false } = {},
) {
  const nextHistory = normalizeWorkspaceHistoryItems(items);
  state.workspaceHistory = nextHistory;
  storeWorkspaceHistory(nextHistory);
  renderWorkspaceHistory();

  const requestToken = ++state.workspaceHistoryPersistToken;
  try {
    const settings = await requestJson("/api/settings", {
      method: "PUT",
      keepalive,
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        workspace_history: nextHistory,
      }),
    });

    if (requestToken !== state.workspaceHistoryPersistToken) {
      return;
    }

    const savedHistory = Array.isArray(settings.workspace_history)
      ? normalizeWorkspaceHistoryItems(settings.workspace_history)
      : nextHistory;
    state.workspaceHistory = savedHistory;
    storeWorkspaceHistory(savedHistory);
    if (markMigrated) {
      markWorkspaceHistoryMigrated();
    }
    renderWorkspaceHistory();
  } catch (error) {
    if (!silent && requestToken === state.workspaceHistoryPersistToken) {
      updateTableCardStatus(workspaceHistoryStatusEl, `历史工作区保存失败：${error.message}`, "warn");
    }
  }
}

function workspaceHistoryFolderName(absolutePath) {
  const parts = splitPathParts(absolutePath);
  return parts[parts.length - 1] || "/";
}

function workspaceAccessRoot() {
  const parts = splitPathParts(state.workspaceDir);
  if (parts.length === 0) {
    return "/";
  }
  parts.pop();
  return `/${parts.join("/")}` || "/";
}

function isAbsolutePathWithinScope(targetPath, scopeRoot) {
  const targetParts = splitPathParts(normalizeAbsolutePath(targetPath));
  const scopeParts = splitPathParts(normalizeAbsolutePath(scopeRoot));
  if (scopeParts.length > targetParts.length) {
    return false;
  }
  return scopeParts.every((part, index) => targetParts[index] === part);
}

function resolveWorkspaceHistoryPath(path) {
  const rawPath = typeof path === "string" ? path.trim() : "";
  if (!rawPath) {
    return normalizeAbsolutePath(state.workspaceDir || "/");
  }
  if (rawPath.startsWith("/")) {
    return normalizeAbsolutePath(rawPath);
  }
  return resolveAbsolutePath(state.workspaceDir || "/", rawPath);
}

function recordWorkspaceHistory(path) {
  const absolutePath = resolveWorkspaceHistoryPath(path);
  const nextEntry = {
    path: absolutePath,
    last_opened_at: Date.now(),
  };
  const nextHistory = [
    nextEntry,
    ...state.workspaceHistory.filter((item) => item.path !== absolutePath),
  ].slice(0, MAX_WORKSPACE_HISTORY_ITEMS);

  state.workspaceHistory = nextHistory;
  persistWorkspaceHistory(nextHistory, { keepalive: true, silent: true });
}

function formatWorkspaceHistoryTime(timestampMs) {
  if (!timestampMs) {
    return "—";
  }

  return formatTimeOnly(new Date(timestampMs));
}

function workspaceHistorySelectedPath() {
  const selected = state.workspaceHistorySelectedPath || workspaceHistoryPathSelectEl?.value || "";
  return selected || workspaceHistoryGroups()[0]?.path || "";
}

function conversationWorkspacePathFromCwd(cwd) {
  const raw = String(cwd || "").trim();
  return raw ? normalizeAbsolutePath(raw) : "";
}

function workspaceHistoryGroups() {
  const byPath = new Map();
  const ensureGroup = (path, lastActivity = 0) => {
    const normalized = normalizeAbsolutePath(path || "");
    if (!normalized) {
      return null;
    }
    const existing = byPath.get(normalized);
    if (existing) {
      existing.lastActivity = Math.max(existing.lastActivity, Number(lastActivity) || 0);
      return existing;
    }
    const group = { path: normalized, lastActivity: Number(lastActivity) || 0 };
    byPath.set(normalized, group);
    return group;
  };

  state.workspaceHistory.forEach((item) => ensureGroup(item.path, item.last_opened_at));
  activeTerminalSessions(state.sessions).forEach((session) => ensureGroup(sessionWorkspacePath(session), session.last_opened_at || session.created_at));
  state.terminalArchives.forEach((archive) => ensureGroup(archiveWorkspacePath(archive), archive.last_used_at || archive.updated_at || archive.created_at));
  state.codexConversations.forEach((conversation) => ensureGroup(conversationWorkspacePathFromCwd(conversation.cwd), conversation.updated_at || conversation.created_at));
  const currentPath = currentWorkspaceDirectoryPath();
  if (currentPath) {
    ensureGroup(currentPath);
  }

  return [...byPath.values()].sort((left, right) => {
    return right.lastActivity - left.lastActivity || left.path.localeCompare(right.path);
  });
}

function workspaceHistoryRowsForPath(path) {
  const selectedPath = normalizeAbsolutePath(path || "");
  if (!selectedPath) {
    return [];
  }

  const rows = [];
  const conversationsBySessionId = new Map();
  const activeTerminalNameBySessionId = new Map();
  const archiveTerminalNameBySessionId = new Map();
  const activeSessions = activeTerminalSessions(state.sessions).filter((session) => {
    return sessionWorkspacePath(session) === selectedPath;
  });
  const agentProgramFromCommand = (command) => {
    const program = String(command || "").trim().split(/\s+/, 1)[0]?.toLowerCase() || "";
    return program === "codex" || program === "claude" ? program : "";
  };

  activeSessions.forEach((session) => {
    if (session.agent_session_id) {
      activeTerminalNameBySessionId.set(session.agent_session_id, session.name || session.id || "");
    }
  });

  state.terminalArchives.forEach((archive) => {
    if (archiveWorkspacePath(archive) !== selectedPath) {
      return;
    }
    const resumeId = archiveResumeId(archive);
    const terminalName = String(archive.terminal_name || archive.terminalName || "").trim();
    if (resumeId && terminalName) {
      archiveTerminalNameBySessionId.set(resumeId, terminalName);
    }
  });

  state.codexConversations.forEach((conversation) => {
    if (conversationWorkspacePathFromCwd(conversation.cwd) !== selectedPath) {
      return;
    }
    const sessionId = conversation.session_id || conversation.resume_id || "";
    if (sessionId) {
      conversationsBySessionId.set(sessionId, conversation);
    }
  });

  activeSessions.forEach((session) => {
    const conversation = conversationsBySessionId.get(session.agent_session_id || "");
    rows.push({
      type: "terminal",
      terminal: session,
      terminalLabel: session.name || session.id,
      activeTerminalName: session.name || session.id || "",
      agentProgram: conversation ? "codex" : agentProgramFromCommand(session.agent_session_command),
      sessionId: session.agent_session_id || "",
      size: conversation?.size_bytes ?? conversation?.size ?? null,
      cwd: selectedPath,
      title: session.input_history_text || session.title || "活动终端",
      updatedAt: session.last_opened_at || session.created_at || 0,
    });
  });

  state.terminalArchives.forEach((archive) => {
    if (archiveWorkspacePath(archive) !== selectedPath) {
      return;
    }
    const resumeId = archiveResumeId(archive);
    if (activeTerminalNameBySessionId.has(resumeId)) {
      return;
    }
    const conversation = conversationsBySessionId.get(resumeId);
    const conversationTitle = conversationHistoryTitle(conversation);
    rows.push({
      type: "archive",
      archive,
      terminalLabel: "归档",
      activeTerminalName: activeTerminalNameBySessionId.get(resumeId) || archiveTerminalNameBySessionId.get(resumeId) || "",
      agentProgram: agentProgramFromCommand(archiveCommand(archive)),
      sessionId: resumeId,
      size: conversation?.size_bytes ?? conversation?.size ?? null,
      cwd: selectedPath,
      title: archiveHistoryNote(archive, resumeId) || conversationTitle || "无对话摘要",
      updatedAt: archive.last_used_at || archive.updated_at || archive.created_at || 0,
    });
  });

  state.codexConversations.forEach((conversation) => {
    if (conversationWorkspacePathFromCwd(conversation.cwd) !== selectedPath) {
      return;
    }
    const sessionId = conversation.session_id || conversation.resume_id || "";
    const alreadyCovered = rows.some((row) => row.sessionId && row.sessionId === sessionId);
    if (alreadyCovered) {
      return;
    }
    rows.push({
      type: "conversation",
      conversation,
      terminalLabel: "历史",
      activeTerminalName: activeTerminalNameBySessionId.get(sessionId) || archiveTerminalNameBySessionId.get(sessionId) || "",
      agentProgram: "codex",
      sessionId,
      size: conversation.size_bytes ?? conversation.size ?? null,
      cwd: selectedPath,
      title: conversationHistoryTitle(conversation) || "无对话摘要",
      updatedAt: conversation.updated_at || conversation.created_at || 0,
    });
  });

  return rows.sort((left, right) => {
    return Number(right.updatedAt || 0) - Number(left.updatedAt || 0) || String(left.title).localeCompare(String(right.title));
  });
}

// Builds a single lowercase haystack string from every searchable field of a
// workspace-history row so callers can do one substring match.
function workspaceHistoryRowSearchText(row) {
  const parts = [
    row?.terminalLabel,
    row?.activeTerminalName,
    row?.sessionId,
    row?.size === null || row?.size === undefined ? "" : String(row.size),
    row?.title,
    row?.cwd,
    formatSize(row?.size ?? null),
    formatDateLikeMonthDayTime(row?.updatedAt),
  ];
  return parts.map((part) => String(part ?? "")).join("\n").toLowerCase();
}

function filterWorkspaceHistoryRows(rows, query) {
  const normalizedQuery = String(query || "").trim().toLowerCase();
  if (!normalizedQuery) {
    return { rows, filtered: false };
  }
  const terms = normalizedQuery.split(/\s+/).filter(Boolean);
  const matched = (Array.isArray(rows) ? rows : []).filter((row) => {
    const haystack = workspaceHistoryRowSearchText(row);
    return terms.every((term) => haystack.includes(term));
  });
  return { rows: matched, filtered: true };
}

function setWorkspaceHistorySearchControlsBusy(isBusy) {
  if (workspaceHistorySearchInputEl) {
    workspaceHistorySearchInputEl.disabled = isBusy;
  }
  if (workspaceHistorySearchSubmitButton) {
    workspaceHistorySearchSubmitButton.disabled = isBusy;
  }
  if (workspaceHistorySearchClearButton) {
    workspaceHistorySearchClearButton.disabled = isBusy && !state.workspaceHistorySearchQuery;
  }
}

function scheduleWorkspaceHistorySearch(query, immediate = false) {
  state.workspaceHistorySearchQuery = String(query || "");
  if (state.workspaceHistorySearchDebounceId) {
    window.clearTimeout(state.workspaceHistorySearchDebounceId);
    state.workspaceHistorySearchDebounceId = 0;
  }
  const run = () => {
    state.workspaceHistorySearchToken += 1;
    setWorkspaceHistorySearchControlsBusy(false);
    renderWorkspaceHistory();
  };
  if (immediate || !state.workspaceHistorySearchQuery) {
    run();
  } else {
    state.workspaceHistorySearchDebounceId = window.setTimeout(run, 180);
  }
}

function clearWorkspaceHistorySearch() {
  if (state.workspaceHistorySearchDebounceId) {
    window.clearTimeout(state.workspaceHistorySearchDebounceId);
    state.workspaceHistorySearchDebounceId = 0;
  }
  state.workspaceHistorySearchToken += 1;
  state.workspaceHistorySearchQuery = "";
  if (workspaceHistorySearchInputEl) {
    workspaceHistorySearchInputEl.value = "";
  }
  setWorkspaceHistorySearchControlsBusy(false);
  renderWorkspaceHistory();
}

function currentWorkspaceDirectoryPath() {
  const path = String(state.currentWorkspaceDirectoryPath || "").trim();
  return path ? normalizeAbsolutePath(path) : "";
}

async function refreshCurrentWorkspaceDirectoryFromTerminal() {
  const sessionId = String(state.returnTerminalSessionId || "").trim();
  if (!sessionId) {
    return false;
  }
  const selectedPathBeforeRequest = state.workspaceHistorySelectedPath || "";

  try {
    const directory = await requestJson(
      `/api/terminal/sessions/${encodeURIComponent(sessionId)}/current-directory`,
    );
    const rawCurrentPath = String(directory?.display_path || "").trim();
    if (!rawCurrentPath) {
      return false;
    }
    const currentPath = normalizeAbsolutePath(rawCurrentPath);
    state.currentWorkspaceDirectoryPath = currentPath;

    if (
      state.activeTab === "workspace-history" &&
      (state.workspaceHistorySelectedPath || "") === selectedPathBeforeRequest
    ) {
      state.workspaceHistorySelectedPath = currentPath;
      renderWorkspaceHistory();
      prioritizeWorkspaceHistoryCurrentDirectory();
      if (state.workspaceHistorySettingsReady) {
        await refreshWorkspaceHistoryConversations();
        if (
          state.activeTab === "workspace-history" &&
          state.workspaceHistorySelectedPath === currentPath
        ) {
          prioritizeWorkspaceHistoryCurrentDirectory();
        }
      }
    }
    return true;
  } catch {
    return false;
  }
}

function prioritizeWorkspaceHistoryCurrentDirectory() {
  if (!workspaceHistoryPathSelectEl) {
    return false;
  }
  const rawCurrentPath = String(state.currentWorkspaceDirectoryPath || "").trim();
  const currentPath = rawCurrentPath ? normalizeAbsolutePath(rawCurrentPath) : "";
  if (!currentPath) {
    return false;
  }
  const options = Array.from(workspaceHistoryPathSelectEl.options || []);
  const currentOption = options.find((option) => option.value === currentPath);
  if (!currentOption) {
    return false;
  }

  if (options[0] !== currentOption) {
    workspaceHistoryPathSelectEl.insertBefore(currentOption, options[0] || null);
  }
  Array.from(workspaceHistoryPathSelectEl.options || []).forEach((option) => {
    option.selected = option === currentOption;
  });
  workspaceHistoryPathSelectEl.value = currentPath;
  state.workspaceHistorySelectedPath = currentPath;
  return true;
}

function syncWorkspaceHistoryPathSelect(groups) {
  if (!workspaceHistoryPathSelectEl) {
    return;
  }

  workspaceHistoryPathSelectEl.textContent = "";
  if (groups.length === 0) {
    const empty = document.createElement("option");
    empty.value = "";
    empty.textContent = "暂无历史工作区";
    empty.selected = true;
    workspaceHistoryPathSelectEl.appendChild(empty);
    workspaceHistoryPathSelectEl.disabled = true;
    state.workspaceHistorySelectedPath = "";
    return;
  }

  const currentPath = currentWorkspaceDirectoryPath();
  const selectedPath = groups.some((group) => group.path === state.workspaceHistorySelectedPath)
    ? state.workspaceHistorySelectedPath
    : groups.some((group) => group.path === currentPath)
      ? currentPath
      : groups[0].path;
  state.workspaceHistorySelectedPath = selectedPath;
  workspaceHistoryPathSelectEl.disabled = false;

  groups.forEach((group) => {
    const option = document.createElement("option");
    option.value = group.path;
    option.textContent = group.path;
    option.title = group.path;
    option.selected = group.path === selectedPath;
    workspaceHistoryPathSelectEl.appendChild(option);
  });
}

function setWorkspaceHistoryActionDisabled(disabled) {
  [workspaceHistoryTerminalButton, workspaceHistoryDeleteButton].forEach((button) => {
    if (button) {
      button.disabled = disabled;
    }
  });
}

function workspaceHistoryLoadMessage() {
  const completed = Number(state.workspaceHistoryLoadCompleted) || 0;
  const total = Number(state.workspaceHistoryLoadTotal) || 0;
  if (state.workspaceHistoryLoadState === "loading") {
    return `正在读取历史记录（${completed}/${total} 个数据源）…`;
  }
  if (state.workspaceHistoryLoadState === "enriching") {
    return `已读取 ${state.codexConversations.length} 条对话，正在补充活动终端详情（${completed}/${total}）…`;
  }
  if (state.workspaceHistoryLoadState === "error") {
    return `历史记录加载失败：${state.workspaceHistoryLoadError || "未知错误"}`;
  }
  return "";
}

function setWorkspaceHistoryLoadState(loadState, { completed = 0, total = 0, error = "" } = {}) {
  state.workspaceHistoryLoadState = loadState;
  state.workspaceHistoryLoadCompleted = completed;
  state.workspaceHistoryLoadTotal = total;
  state.workspaceHistoryLoadError = error;
  const message = workspaceHistoryLoadMessage();
  if (message) {
    updateTableCardStatus(workspaceHistoryStatusEl, message, loadState === "error" ? "warn" : "info");
  }
}

function workspaceHistoryLoadPath() {
  const selectedPath = workspaceHistorySelectedPath();
  if (selectedPath) {
    return selectedPath;
  }
  return currentWorkspaceDirectoryPath();
}

function isWorkspaceHistoryMissingDirectoryError(error) {
  const message = String(error?.message || error || "");
  return message.includes("路径不存在") || message.includes("目录不存在");
}

function workspaceHistoryCoreRequestUrls() {
  if (state.workspaceHistorySearchAllWorkspaces) {
    return {
      path: "",
      sessions: "/api/terminal/sessions?all=true",
      conversations: "/api/terminal/codex-conversations",
    };
  }

  const path = workspaceHistoryLoadPath();
  const relativePath = relativePathBetweenAbsolute(state.workspaceDir || "/", path);
  return {
    path,
    sessions: `/api/terminal/sessions?path=${encodeURIComponent(relativePath)}`,
    conversations: `/api/terminal/codex-conversations?cwd=${encodeURIComponent(path)}`,
  };
}

async function refreshWorkspaceHistoryConversations() {
  const requestToken = ++state.codexConversationRequestToken;
  const requestUrls = workspaceHistoryCoreRequestUrls();
  if (workspaceHistoryRefreshButton) {
    workspaceHistoryRefreshButton.disabled = true;
  }
  const coreRequestTotal = 3;
  let completedCoreRequests = 0;
  setWorkspaceHistoryLoadState("loading", { total: coreRequestTotal });
  renderWorkspaceHistory();

  const loadCoreData = async (url, missingDirectoryFallback = null) => {
    try {
      return await requestJson(url);
    } catch (error) {
      if (missingDirectoryFallback && isWorkspaceHistoryMissingDirectoryError(error)) {
        return missingDirectoryFallback;
      }
      throw error;
    } finally {
      if (
        requestToken === state.codexConversationRequestToken &&
        state.workspaceHistoryLoadState === "loading"
      ) {
        completedCoreRequests += 1;
        setWorkspaceHistoryLoadState("loading", {
          completed: completedCoreRequests,
          total: coreRequestTotal,
        });
      }
    }
  };

  try {
    const [sessionsResponse, archivesResponse, conversationsResponse] = await Promise.all([
      loadCoreData(requestUrls.sessions, requestUrls.path ? { sessions: [] } : null),
      loadCoreData("/api/terminal/resume-archives"),
      loadCoreData(requestUrls.conversations),
    ]);
    if (requestToken !== state.codexConversationRequestToken) {
      return;
    }
    const sessions = activeTerminalSessions(sessionsResponse.sessions);
    const archives = Array.isArray(archivesResponse.archives) ? archivesResponse.archives : [];
    const conversations = Array.isArray(conversationsResponse.conversations)
      ? conversationsResponse.conversations
      : [];
    const pathMatchesScope = (path) => !requestUrls.path || path === requestUrls.path;
    state.sessions = sortSessionsByRecentActivity(
      sessions.filter((session) => pathMatchesScope(sessionWorkspacePath(session))),
    );
    state.terminalArchives = sortTerminalArchives(
      archives.filter((archive) => pathMatchesScope(archiveWorkspacePath(archive))),
    );
    state.codexConversations = conversations.filter((conversation) => {
      return pathMatchesScope(conversationWorkspacePathFromCwd(conversation.cwd));
    });
    const terminalDetailCount = activeTerminalSessions(state.sessions).slice(0, 24).length * 2;
    if (terminalDetailCount > 0) {
      let completedTerminalDetails = 0;
      const reportTerminalDetailLoaded = () => {
        if (
          requestToken !== state.codexConversationRequestToken ||
          state.workspaceHistoryLoadState !== "enriching"
        ) {
          return;
        }
        completedTerminalDetails += 1;
        setWorkspaceHistoryLoadState("enriching", {
          completed: completedTerminalDetails,
          total: terminalDetailCount,
        });
      };
      setWorkspaceHistoryLoadState("enriching", { total: terminalDetailCount });
      renderWorkspaceHistory();
      await Promise.all([
        hydrateWorkspaceHistoryTerminalSessionIds(requestToken, reportTerminalDetailLoaded),
        hydrateWorkspaceHistoryTerminalInputHistory(requestToken, reportTerminalDetailLoaded),
      ]);
    }
    if (requestToken !== state.codexConversationRequestToken) {
      return;
    }
    setWorkspaceHistoryLoadState("loaded");
    renderWorkspaceHistory();
  } catch (error) {
    if (requestToken === state.codexConversationRequestToken) {
      setWorkspaceHistoryLoadState("error", {
        error: error.message || "读取对话列表失败。",
      });
      renderWorkspaceHistory();
    }
  } finally {
    if (requestToken === state.codexConversationRequestToken && workspaceHistoryRefreshButton) {
      workspaceHistoryRefreshButton.disabled = false;
    }
  }
}

function workspaceHistoryInputHistoryText(entries) {
  return (Array.isArray(entries) ? entries : [])
    .map((entry) => String(entry?.text || "").trim())
    .filter((text) => text && !isWorkspaceHistoryAutomationText(text))
    .join("\n");
}

function isWorkspaceHistoryAutomationText(text) {
  return (
    text.startsWith("[from webClx-compile-api]") ||
    text.startsWith("<turn_aborted>") ||
    text.startsWith("Skill descriptions were shortened to fit the 2% skills context budget") ||
    text.startsWith("service temporarily unavailable (source:")
  );
}

function conversationHistoryTitle(conversation) {
  return String(conversation?.title || conversation?.input_history_text || conversation?.summary || "").trim();
}

function archiveHistoryNote(archive, resumeId) {
  const note = String(archive?.note || "").trim();
  if (!note || isDefaultArchiveHistoryNote(note, resumeId)) {
    return "";
  }
  return note;
}

function isDefaultArchiveHistoryNote(note, resumeId) {
  const normalizedResumeId = String(resumeId || "").trim();
  if (!normalizedResumeId) {
    return false;
  }
  const backendDefaultNote =
    normalizedResumeId.length > 8
      ? `Codex ${normalizedResumeId.slice(0, 8)}...`
      : `Codex ${normalizedResumeId}`;
  return note === backendDefaultNote || note === `Codex ${shortResumeId(normalizedResumeId)}`;
}

let workspaceHistoryTerminalArchivePersistQueue = Promise.resolve();
let workspaceHistoryRenamingItem = null;

function queueWorkspaceHistoryTerminalArchiveWrite(operation) {
  const persistence = workspaceHistoryTerminalArchivePersistQueue.then(operation);
  workspaceHistoryTerminalArchivePersistQueue = persistence.catch(() => undefined);
  return persistence;
}

function persistWorkspaceHistoryTerminalArchive(session, detected) {
  return queueWorkspaceHistoryTerminalArchiveWrite(() =>
    persistWorkspaceHistoryTerminalArchiveNow(session, detected),
  );
}

async function persistWorkspaceHistoryTerminalArchiveNow(session, detected) {
  const resumeId = String(detected?.resume_id || "").trim();
  const terminalName = String(session?.name || session?.id || "").trim();
  const sessionPath = sessionWorkspacePath(session);
  if (!resumeId || !terminalName || !sessionPath) {
    return;
  }

  const existing = state.terminalArchives.find((archive) => archiveResumeId(archive) === resumeId);
  if (
    existing &&
    String(existing.terminal_name || existing.terminalName || "").trim() === terminalName &&
    archiveWorkspacePath(existing) === sessionPath
  ) {
    return;
  }

  const cwd = workspaceHistoryArchiveCwd(sessionPath, existing);
  const saved = await requestJson("/api/terminal/resume-archives", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      resume_id: resumeId,
      cwd,
      terminal_name: terminalName,
      command: detected.command || existing?.command || resumeCommandFromId(resumeId),
      note: existing?.note,
      source: detected.source || existing?.source || "active_terminal",
    }),
  });

  state.terminalArchives = sortTerminalArchives([
    ...state.terminalArchives.filter((archive) => archiveResumeId(archive) !== resumeId),
    saved,
  ]);
}

async function hydrateWorkspaceHistoryTerminalSessionIds(requestToken, onSettled = null) {
  const terminalSessions = activeTerminalSessions(state.sessions).slice(0, 24);
  await Promise.all(terminalSessions.map(async (session) => {
    try {
      const response = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}/agent-session`);
      if (requestToken !== state.codexConversationRequestToken) {
        return;
      }
      session.agent_session_id = response.resume_id || "";
      session.agent_session_command = response.command || "";
      if (session.agent_session_id) {
        try {
          await persistWorkspaceHistoryTerminalArchive(session, response);
        } catch (error) {
          session.agent_session_archive_error = error.message || "记录终端名称失败";
          updateStatus(
            workspaceHistoryStatusEl,
            `历史终端名称记录失败：${session.agent_session_archive_error}`,
            "warn",
          );
        }
      }
    } catch {
      session.agent_session_id = "";
      session.agent_session_command = "";
    } finally {
      onSettled?.();
    }
  }));
}

async function hydrateWorkspaceHistoryTerminalInputHistory(requestToken, onSettled = null) {
  const terminalSessions = activeTerminalSessions(state.sessions).slice(0, 24);
  await Promise.all(terminalSessions.map(async (session) => {
    try {
      const payload = await requestJson(`/api/terminal/sessions/${encodeURIComponent(session.id)}/input-history`);
      if (requestToken !== state.codexConversationRequestToken) {
        return;
      }
      session.input_history_text = workspaceHistoryInputHistoryText(payload.entries || []);
    } catch {
      session.input_history_text = "";
    } finally {
      onSettled?.();
    }
  }));
}

function openWorkspaceHistoryTerminal(path) {
  const absolutePath = resolveWorkspaceHistoryPath(path);
  const scopeRoot = workspaceAccessRoot();
  if (!absolutePath || !isAbsolutePathWithinScope(absolutePath, scopeRoot)) {
    updateTableCardStatus(workspaceHistoryStatusEl, "这个历史目录不在当前工作区允许范围内，无法新建终端。", "warn");
    return;
  }

  recordWorkspaceHistory(absolutePath);
  openFreshTerminalSession(absolutePath);
}

function workspaceHistoryForkCommand(sessionId) {
  const normalizedSessionId = String(sessionId || "").trim();
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(normalizedSessionId)) {
    return "";
  }
  return `codex fork ${normalizedSessionId}`;
}

function workspaceHistoryForkTerminalName(item) {
  const terminalName = String(item?.activeTerminalName || "").trim();
  return terminalName ? `${terminalName}_fork` : "";
}

function workspaceHistoryForkSupported(item) {
  return item?.agentProgram === "codex";
}

function workspaceHistoryPresetForkCommand(sessionId, presetId) {
  const forkCommand = workspaceHistoryForkCommand(sessionId);
  const normalizedPresetId = String(presetId || "").trim();
  if (!forkCommand || !normalizedPresetId) {
    return "";
  }
  return forkCommand;
}

function workspaceHistoryPresetModel(preset) {
  return specifiedPresetModel(preset, "codex");
}

let workspaceHistoryPresetForkTarget = null;
let workspaceHistoryPresetForkTrigger = null;
let workspaceHistoryPresetForkRequestToken = 0;

function workspaceHistoryPresetForkDialogElements() {
  return {
    dialog: document.getElementById("workspace-history-preset-dialog"),
    form: document.getElementById("workspace-history-preset-form"),
    list: document.getElementById("workspace-history-preset-list"),
    status: document.getElementById("workspace-history-preset-status"),
    submit: document.getElementById("workspace-history-preset-submit"),
    cancel: document.getElementById("workspace-history-preset-cancel"),
  };
}

function updateWorkspaceHistoryPresetStatus(message, tone = "muted") {
  const { status } = workspaceHistoryPresetForkDialogElements();
  if (!status) {
    return;
  }
  status.textContent = message;
  status.dataset.tone = tone;
  status.hidden = !message;
}

function renderWorkspaceHistoryPresetOptions(presets, { loading = false, error = "" } = {}) {
  const { list, submit } = workspaceHistoryPresetForkDialogElements();
  if (!list || !submit) {
    return;
  }

  const availablePresets = Array.isArray(presets) ? presets.filter((preset) => preset?.id) : [];
  const selectedPresetId = list.querySelector('input[name="workspace-history-preset"]:checked')?.value || "";
  list.replaceChildren();
  submit.disabled = loading || availablePresets.length === 0;

  if (loading && availablePresets.length === 0) {
    const loadingRow = document.createElement("div");
    loadingRow.className = "workspace-history-preset-empty";
    loadingRow.textContent = "正在读取预设…";
    list.appendChild(loadingRow);
    updateWorkspaceHistoryPresetStatus("", "info");
    return;
  }

  if (availablePresets.length === 0) {
    const emptyRow = document.createElement("div");
    emptyRow.className = "workspace-history-preset-empty";
    emptyRow.textContent = error || "还没有可用的 Codex API 预设。";
    list.appendChild(emptyRow);
    updateWorkspaceHistoryPresetStatus(error, error ? "warn" : "muted");
    return;
  }

  const defaultPreset = availablePresets.find((preset) => preset.id === selectedPresetId)
    || availablePresets.find((preset) => preset.active)
    || availablePresets[0];
  availablePresets.forEach((preset) => {
    const option = document.createElement("label");
    option.className = "workspace-history-preset-option";

    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = "workspace-history-preset";
    radio.value = preset.id;
    radio.checked = preset.id === defaultPreset.id;

    const content = document.createElement("span");
    content.className = "workspace-history-preset-option-content";
    const title = document.createElement("span");
    title.className = "workspace-history-preset-option-title";
    const name = document.createElement("strong");
    name.textContent = preset.name || preset.id;
    title.appendChild(name);
    if (preset.active) {
      const current = document.createElement("span");
      current.className = "workspace-history-preset-current";
      current.textContent = "当前";
      title.appendChild(current);
    }

    const meta = document.createElement("span");
    meta.className = "workspace-history-preset-option-meta mono-text";
    const model = workspaceHistoryPresetModel(preset) || "未设置模型";
    const baseUrl = String(preset.base_url || "").trim() || "未设置 Base URL";
    meta.textContent = `${model} · ${baseUrl}`;
    content.append(title, meta);
    option.append(radio, content);
    list.appendChild(option);
  });

  updateWorkspaceHistoryPresetStatus(
    error || `共 ${availablePresets.length} 个预设`,
    error ? "warn" : "muted",
  );
}

async function openWorkspaceHistoryPresetForkDialog(item, fallbackPath, trigger = null) {
  if (!workspaceHistoryForkSupported(item) || !workspaceHistoryForkCommand(item?.sessionId)) {
    return;
  }
  const { dialog } = workspaceHistoryPresetForkDialogElements();
  if (!dialog) {
    return;
  }

  workspaceHistoryPresetForkTarget = { item, fallbackPath };
  workspaceHistoryPresetForkTrigger = trigger;
  const requestToken = ++workspaceHistoryPresetForkRequestToken;
  renderWorkspaceHistoryPresetOptions(state.apiPresets, {
    loading: !state.apiPresetsLoaded,
  });
  if (!dialog.open) {
    dialog.showModal();
  }

  try {
    const response = await requestJson("/api/auth/api-presets");
    if (requestToken !== workspaceHistoryPresetForkRequestToken) {
      return;
    }
    state.apiPresets = Array.isArray(response?.presets) ? response.presets : [];
    state.apiPresetsLoaded = true;
    renderWorkspaceHistoryPresetOptions(state.apiPresets);
    document.querySelector('input[name="workspace-history-preset"]:checked')?.focus();
  } catch (error) {
    if (requestToken !== workspaceHistoryPresetForkRequestToken) {
      return;
    }
    renderWorkspaceHistoryPresetOptions(state.apiPresets, {
      error: `读取预设失败：${error.message}`,
    });
  }
}

function closeWorkspaceHistoryPresetForkDialog({ restoreFocus = true } = {}) {
  const { dialog } = workspaceHistoryPresetForkDialogElements();
  if (dialog?.open) {
    dialog.close();
  }
  const trigger = workspaceHistoryPresetForkTrigger;
  workspaceHistoryPresetForkRequestToken += 1;
  workspaceHistoryPresetForkTarget = null;
  workspaceHistoryPresetForkTrigger = null;
  if (restoreFocus) {
    trigger?.focus?.();
  }
}

async function launchWorkspaceHistoryPresetFork() {
  const { dialog, list, submit } = workspaceHistoryPresetForkDialogElements();
  const selectedPresetId = list?.querySelector('input[name="workspace-history-preset"]:checked')?.value || "";
  const target = workspaceHistoryPresetForkTarget;
  if (!target || !workspaceHistoryPresetForkCommand(target?.item?.sessionId, selectedPresetId)) {
    updateWorkspaceHistoryPresetStatus("请选择一个可用预设。", "warn");
    return;
  }

  const workingPath = resolveWorkspaceHistoryPath(target.item?.cwd || target.fallbackPath);
  submit.disabled = true;
  updateWorkspaceHistoryPresetStatus("正在准备临时预设…", "info");
  try {
    await executeSpecifiedPreset({
      action: "launch",
      temporary: true,
      agent: "codex",
      presetId: selectedPresetId,
      cwd: workingPath,
      sessionAction: "fork",
      sessionId: target.item.sessionId,
      sourceTerminalName: target.item.activeTerminalName,
      quickStart: false,
    });
    closeWorkspaceHistoryPresetForkDialog({ restoreFocus: false });
  } catch (error) {
    updateWorkspaceHistoryPresetStatus(`准备临时预设失败：${error.message}`, "warn");
    if (dialog?.open) {
      submit.disabled = false;
    }
  }
}

function bindWorkspaceHistoryPresetForkDialog() {
  const { dialog, form, cancel } = workspaceHistoryPresetForkDialogElements();
  if (!dialog || !form || !cancel || dialog.dataset.bound === "true") {
    return;
  }
  dialog.dataset.bound = "true";
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeWorkspaceHistoryPresetForkDialog();
  });
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) {
      closeWorkspaceHistoryPresetForkDialog();
    }
  });
  cancel.addEventListener("click", () => closeWorkspaceHistoryPresetForkDialog());
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    await launchWorkspaceHistoryPresetFork();
  });
}

function createWorkspaceHistoryForkLink(item, fallbackPath) {
  if (!workspaceHistoryForkSupported(item)) {
    return null;
  }
  const forkCommand = workspaceHistoryForkCommand(item?.sessionId);
  if (!forkCommand) {
    return null;
  }
  const workingPath = resolveWorkspaceHistoryPath(item?.cwd || fallbackPath);
  const forkLink = createActionLink(
    "fork",
    buildTerminalUrl(workingPath, "", { fresh: true, runCommand: forkCommand }),
    "mini-button",
  );
  forkLink.addEventListener("click", (event) => {
    openFreshTerminalRunLink(event, workingPath, forkCommand, {
      terminalName: workspaceHistoryForkTerminalName(item),
    });
  });
  return forkLink;
}

function createWorkspaceHistoryPresetForkButton(item, fallbackPath) {
  if (!workspaceHistoryForkSupported(item) || !workspaceHistoryForkCommand(item?.sessionId)) {
    return null;
  }
  const button = document.createElement("button");
  button.type = "button";
  button.className = "mini-button workspace-history-preset-fork-button";
  button.textContent = "模型";
  button.title = "指定大模型";
  button.setAttribute("aria-label", "指定大模型");
  button.addEventListener("click", () => {
    openWorkspaceHistoryPresetForkDialog(item, fallbackPath, button);
  });
  return button;
}

function workspaceHistoryArchiveForItem(item) {
  if (item?.archive) {
    return item.archive;
  }
  const resumeId = String(item?.sessionId || "").trim();
  return state.terminalArchives.find((archive) => archiveResumeId(archive) === resumeId) || null;
}

function workspaceHistoryArchiveCwd(path, existing = null) {
  const rawPath = String(path || "").trim();
  if (rawPath) {
    return archiveWorkingPath({ cwd: rawPath });
  }
  return archiveWorkingPath(existing);
}

async function saveWorkspaceHistoryTerminalArchiveName(item, terminalName) {
  const resumeId = String(item?.sessionId || "").trim();
  if (!resumeId) {
    throw new Error("没有可保存终端名称的 Codex session ID。");
  }

  const existing = workspaceHistoryArchiveForItem(item);
  const cwd = workspaceHistoryArchiveCwd(item?.cwd, existing);
  const saved = await requestJson("/api/terminal/resume-archives", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      resume_id: resumeId,
      command: existing?.command || resumeCommandFromId(resumeId),
      cwd,
      terminal_name: terminalName,
      note: existing?.note,
      source: existing?.source || "workspace_history_rename",
    }),
  });
  state.terminalArchives = sortTerminalArchives([
    ...state.terminalArchives.filter((archive) => archiveResumeId(archive) !== resumeId),
    saved,
  ]);
  return saved;
}

async function persistWorkspaceHistoryArchiveName(item, terminalName) {
  if (!item?.sessionId || item?.terminal?.id) {
    return null;
  }
  return queueWorkspaceHistoryTerminalArchiveWrite(() =>
    saveWorkspaceHistoryTerminalArchiveName(item, terminalName),
  );
}

function startWorkspaceHistoryTerminalRename(item, trigger) {
  if (item?.terminal?.id) {
    startSessionRename(item.terminal, trigger);
    return;
  }
  if (!item?.sessionId || !sessionRenameDialogEl || !sessionRenameInputEl) {
    return;
  }

  const currentName = String(item.activeTerminalName || item.terminal?.name || "").trim();
  workspaceHistoryRenamingItem = item;
  state.renamingSessionId = "";
  openTerminalRenameDialog(sessionRenameDraftName(currentName), trigger);
}

async function renameWorkspaceHistoryTerminal() {
  const item = workspaceHistoryRenamingItem;
  if (!item || !sessionRenameInputEl) {
    closeTerminalRenameDialog();
    return;
  }

  const currentName = String(item.activeTerminalName || item.terminal?.name || "").trim();
  const nextName = sessionRenameSavedName(sessionRenameInputEl.value);
  if (!nextName) {
    updateTerminalRenameDialogStatus("请输入新的终端名称。", "warn");
    sessionRenameInputEl.focus();
    return;
  }
  if (nextName === currentName) {
    closeTerminalRenameDialog();
    updateTableCardStatus(workspaceHistoryStatusEl, "终端名称未变化。", "muted");
    return;
  }

  closeTerminalRenameDialog();
  updateTableCardStatus(workspaceHistoryStatusEl, `正在改名 ${currentName}…`, "info");
  try {
    await persistWorkspaceHistoryArchiveName(item, nextName);
    try {
      await refreshWorkspaceHistoryConversations();
    } catch {
      renderWorkspaceHistory();
    }
    updateTableCardStatus(workspaceHistoryStatusEl, `终端已改名为 ${nextName}。`, "ok");
  } catch (error) {
    updateTableCardStatus(
      workspaceHistoryStatusEl,
      `修改终端名称失败：${error.message}`,
      "warn",
    );
  }
}

function removeWorkspaceHistoryConversationLocally(sessionId) {
  const normalizedSessionId = String(sessionId || "").trim();
  if (!normalizedSessionId) {
    return;
  }

  state.codexConversations = state.codexConversations.filter((conversation) => {
    const conversationSessionId = String(
      conversation?.session_id || conversation?.resume_id || "",
    ).trim();
    return conversationSessionId !== normalizedSessionId;
  });
  state.terminalArchives = state.terminalArchives.filter(
    (archive) => archiveResumeId(archive) !== normalizedSessionId,
  );
  if (String(workspaceHistoryRenamingItem?.sessionId || "").trim() === normalizedSessionId) {
    closeTerminalRenameDialog();
  }
  renderWorkspaceHistory();
}

async function deleteWorkspaceHistoryConversation(item, button) {
  if (!item?.sessionId || item.type === "terminal") {
    return;
  }
  const confirmed = window.confirm(
    `确定永久删除这个 Codex 会话吗？\n\n${item.title || item.sessionId}\n\n该操作会删除 Codex 会话记录，无法恢复。`,
  );
  if (!confirmed) {
    return;
  }

  button.disabled = true;
  updateTableCardStatus(workspaceHistoryStatusEl, "正在删除 Codex 会话…", "info");
  try {
    await requestJson(
      `/api/terminal/codex-conversations/${encodeURIComponent(item.sessionId)}`,
      { method: "DELETE" },
    );
    removeWorkspaceHistoryConversationLocally(item.sessionId);
    updateTableCardStatus(workspaceHistoryStatusEl, "Codex 会话已删除。", "ok");
    showToast("Codex 会话已删除。", "ok", 2800);
  } catch (error) {
    button.disabled = false;
    updateTableCardStatus(workspaceHistoryStatusEl, `删除 Codex 会话失败：${error.message}`, "warn");
    showToast(`删除 Codex 会话失败：${error.message}`, "warn", 6000);
  }
}

function createWorkspaceHistoryMoreButton(item) {
  let button;
  const actions = [];
  if (item?.terminal?.id || item?.sessionId) {
    actions.push({
      label: "改名",
      handler: () => startWorkspaceHistoryTerminalRename(item, button),
    });
  }
  if (item?.sessionId) {
    actions.push({
      label: "删除",
      danger: true,
      disabled: item.type === "terminal",
      title: item.type === "terminal" ? "请先结束活动终端，再删除 Codex 会话" : "",
      handler: () => deleteWorkspaceHistoryConversation(item, button),
    });
  }
  button = createPresetActionMenu(actions, {
    label: `${item?.activeTerminalName || item?.title || "当前会话"} 的更多操作`,
  });
  button.classList.add("workspace-history-more-action");
  button.textContent = "更多";
  button.dataset.terminalRenameKey = item?.terminal?.id
    ? `session:${item.terminal.id}`
    : `history:${item?.sessionId || ""}`;
  return button;
}

function copyWorkspaceHistorySessionId(sessionId, button) {
  const value = String(sessionId || "").trim();
  if (!value) {
    updateTableCardStatus(workspaceHistoryStatusEl, "没有可复制的 Session ID。", "warn");
    return;
  }

  const markCopied = () => {
    updateTableCardStatus(workspaceHistoryStatusEl, "已复制 Session ID。", "ok");
    showToast("已复制 Session ID。", "ok", 2000);
    const previousText = button?.textContent;
    if (button) {
      button.textContent = "已复制";
      window.setTimeout(() => {
        button.textContent = previousText || "复制 ID";
      }, 1200);
    }
  };
  const markFailed = () => {
    updateTableCardStatus(workspaceHistoryStatusEl, "复制 Session ID 失败，请检查剪贴板权限。", "warn");
    showToast("复制 Session ID 失败，请检查剪贴板权限。", "warn", 4000);
  };
  const copyWithFallback = () => {
    if (copyTextWithHiddenTextarea(value)) {
      markCopied();
    } else {
      markFailed();
    }
  };

  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(value).then(markCopied, copyWithFallback);
  } else {
    copyWithFallback();
  }
}

function renderWorkspaceHistory() {
  if (!workspaceHistoryListEl || !workspaceHistoryStatusEl) {
    return;
  }

  workspaceHistoryListEl.textContent = "";
  const groups = workspaceHistoryGroups();
  syncWorkspaceHistoryPathSelect(groups);

  if (state.workspaceHistoryLoadState === "loading" || state.workspaceHistoryLoadState === "error") {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "session-empty";
    cell.textContent = state.workspaceHistoryLoadState === "error"
      ? "历史记录加载失败，请点击“刷新对话”重试。"
      : workspaceHistoryLoadMessage();
    row.appendChild(cell);
    workspaceHistoryListEl.appendChild(row);
    updateStatus(
      workspaceHistoryStatusEl,
      workspaceHistoryLoadMessage(),
      state.workspaceHistoryLoadState === "error" ? "warn" : "info",
    );
    setWorkspaceHistoryActionDisabled(groups.length === 0);
    return;
  }

  if (groups.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "session-empty";
    cell.textContent = state.workspaceHistoryLoadState === "loaded"
      ? "历史记录已加载，但还没有历史工作区记录或 Codex 对话。"
      : "还没有历史工作区记录或 Codex 对话。";
    row.appendChild(cell);
    workspaceHistoryListEl.appendChild(row);
    updateStatus(
      workspaceHistoryStatusEl,
      state.workspaceHistoryLoadState === "loaded" ? "加载完成：0 个工作目录，0 条对话。" : "尚未加载历史记录。",
      "muted",
    );
    setWorkspaceHistoryActionDisabled(true);
    return;
  }

  const selectedPath = workspaceHistorySelectedPath();
  const scopeRoot = workspaceAccessRoot();
  const accessible = Boolean(state.workspaceDir) && isAbsolutePathWithinScope(selectedPath, scopeRoot);
  setWorkspaceHistoryActionDisabled(!accessible);
  const searchAllWorkspaces = state.workspaceHistorySearchAllWorkspaces;
  const sourceGroups = searchAllWorkspaces ? groups : groups.filter((group) => group.path === selectedPath);
  const allRows = sourceGroups
    .flatMap((group) => workspaceHistoryRowsForPath(group.path))
    .sort((left, right) => Number(right.updatedAt || 0) - Number(left.updatedAt || 0) || String(left.title).localeCompare(String(right.title)));
  const { rows, filtered } = filterWorkspaceHistoryRows(allRows, state.workspaceHistorySearchQuery);
  const query = state.workspaceHistorySearchQuery.trim();
  // Hide conversations older than 30 days unless the user is searching (search ignores the date cap).
  const cutoffMs = state.workspaceHistoryRecentOnly && !query ? Date.now() - 30 * 86400000 : 0;
  const visibleRows = cutoffMs ? rows.filter((row) => Number(row.updatedAt || 0) >= cutoffMs) : rows;

  if (allRows.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "session-empty";
    cell.textContent = "这个工作目录下还没有可显示的对话。";
    row.appendChild(cell);
    workspaceHistoryListEl.appendChild(row);
  } else if (filtered && rows.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "session-empty";
    cell.textContent = `没有匹配"${state.workspaceHistorySearchQuery}"的对话。`;
    row.appendChild(cell);
    workspaceHistoryListEl.appendChild(row);
  } else if (visibleRows.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "session-empty";
    cell.textContent = `最近30天没有对话，取消勾选"仅30天"可查看全部。`;
    row.appendChild(cell);
    workspaceHistoryListEl.appendChild(row);
  }

  visibleRows.forEach((item) => {
    const row = document.createElement("tr");
    if (query) {
      row.classList.add("session-search-match-row", "workspace-history-search-match-row");
    }
    const terminalCell = document.createElement("td");
    terminalCell.className = "workspace-history-terminal-cell";
    terminalCell.textContent = item.terminalLabel || "—";
    terminalCell.title = item.terminal?.id || item.type;

    const activeTerminalNameCell = document.createElement("td");
    activeTerminalNameCell.className = "workspace-history-active-terminal-cell";
    activeTerminalNameCell.textContent = item.activeTerminalName || "—";
    activeTerminalNameCell.title = item.activeTerminalName || "没有匹配的活动终端";

    const sessionCell = document.createElement("td");
    sessionCell.className = "mono-text workspace-history-session-cell";
    sessionCell.textContent = item.sessionId ? shortResumeId(item.sessionId) : "—";
    sessionCell.title = item.sessionId || "没有检测到 Codex session ID";

    const sizeCell = document.createElement("td");
    sizeCell.className = "workspace-history-size-cell";
    sizeCell.textContent = item.size === null || item.size === undefined ? "—" : formatSize(item.size);

    const titleCell = document.createElement("td");
    titleCell.className = "workspace-history-title-cell";
    titleCell.textContent = item.title || "—";
    // Full detail is surfaced in a floating tooltip; keep the cell single-line.
    if (searchAllWorkspaces && item.cwd) {
      const dirSub = document.createElement("span");
      dirSub.className = "workspace-history-title-dir";
      dirSub.textContent = relativePathBetweenAbsolute(state.workspaceDir || "/", item.cwd) || item.cwd;
      titleCell.appendChild(dirSub);
    }
    titleCell.tabIndex = 0;
    // Hover/focus shows the full detail in a floating window that does not affect row height.
    attachWorkspaceHistoryTooltip(titleCell, {
      title: item.title || "",
      dir: searchAllWorkspaces && item.cwd ? item.cwd : "",
    });
    // Click/tap on touch devices also toggles the floating tooltip.
    titleCell.addEventListener("click", () => {
      titleCell.focus();
    });

    const timeCell = document.createElement("td");
    timeCell.className = "workspace-history-time-cell";
    timeCell.textContent = formatDateLikeMonthDayTime(item.updatedAt);

    const actionCell = document.createElement("td");
    actionCell.className = "session-action-cell";
    if (item.type === "terminal" && item.terminal) {
      const openLink = createActionLink("进入", buildTerminalUrl(item.terminal.path, item.terminal.id), "mini-button accent");
      openLink.addEventListener("click", () => rememberPreferredSession(item.terminal.path, item.terminal.id));
      actionCell.appendChild(openLink);
    } else if (item.type === "archive" && item.archive) {
      const command = archiveCommand(item.archive);
      const workingPath = resolveWorkspaceHistoryPath(archiveWorkingPath(item.archive));
      const runLink = createActionLink("恢复", buildTerminalUrl(workingPath, "", { fresh: true, runCommand: command }), "mini-button accent");
      runLink.addEventListener("click", (event) => {
        openFreshTerminalRunLink(event, workingPath, command, {
          beforeNavigate: () => touchTerminalArchive(archiveIdentity(item.archive)),
          terminalName: item.activeTerminalName,
        });
      });
      actionCell.appendChild(runLink);
    } else if (item.sessionId) {
      const workingPath = resolveWorkspaceHistoryPath(selectedPath);
      const command = resumeCommandFromId(item.sessionId);
      const runLink = createActionLink("恢复", buildTerminalUrl(workingPath, "", { fresh: true, runCommand: command }), "mini-button accent");
      runLink.addEventListener("click", (event) => {
        openFreshTerminalRunLink(event, workingPath, command, {
          terminalName: item.activeTerminalName,
        });
      });
      actionCell.appendChild(runLink);
    } else {
      actionCell.textContent = "—";
    }
    if (item.sessionId) {
      const copySessionIdButton = createActionButton("复制 ID", () => {
        copyWorkspaceHistorySessionId(item.sessionId, copySessionIdButton);
      }, "mini-button");
      copySessionIdButton.setAttribute("aria-label", `复制 Session ID ${item.sessionId}`);
      actionCell.appendChild(copySessionIdButton);
    }
    const forkLink = createWorkspaceHistoryForkLink(item, selectedPath);
    if (forkLink) {
      actionCell.appendChild(forkLink);
    }
    const presetForkButton = createWorkspaceHistoryPresetForkButton(item, selectedPath);
    if (presetForkButton) {
      actionCell.appendChild(presetForkButton);
    }
    if (item.terminal?.id || item.sessionId) {
      actionCell.appendChild(createWorkspaceHistoryMoreButton(item));
    }

    row.append(actionCell, terminalCell, activeTerminalNameCell, sessionCell, sizeCell, titleCell, timeCell);
    workspaceHistoryListEl.appendChild(row);
  });

  let statusText;
  let statusTone = "ok";
  const scopeLabel = searchAllWorkspaces ? "（全部工作区）" : "";
  const dateLabel = state.workspaceHistoryRecentOnly ? "（最近30天）" : "";
  if (query) {
    if (rows.length === 0) {
      statusText = `没有匹配"${query}"的对话（共 ${allRows.length} 条）${scopeLabel}`;
      statusTone = "muted";
    } else {
      statusText = `匹配 ${rows.length}/${allRows.length} 条对话${scopeLabel}`;
    }
  } else if (searchAllWorkspaces) {
    statusText = `全部工作区共 ${visibleRows.length} 条对话${dateLabel}`;
  } else {
    statusText = `共 ${groups.length} 个工作目录，当前 ${visibleRows.length} 条对话${dateLabel}`;
  }
  if (state.workspaceHistoryLoadState === "enriching") {
    updateTableCardStatus(workspaceHistoryStatusEl, workspaceHistoryLoadMessage(), "info");
  } else {
    updateTableCardStatus(workspaceHistoryStatusEl, `加载完成：${statusText}`, statusTone);
  }
}
