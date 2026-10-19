// webClx 工作区历史子系统：从 app.js 抽出，保持全局函数声明。
// 依赖 app.js 运行时已初始化的全局：state、workspaceHistory*El、requestJson、updateStatus。
// 必须在 app.js 之前用 <script defer> 加载，函数声明为全局，app.js 内调用方无需改动。

const WORKSPACE_HISTORY_COLUMN_WIDTHS_KEY = "webclx:workspace-history-column-widths";
const WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS = [96, 48, 96, 180, 64, 180, 96];

function readWorkspaceHistoryColumnWidths() {
  try {
    const raw = window.localStorage.getItem(WORKSPACE_HISTORY_COLUMN_WIDTHS_KEY);
    const widths = JSON.parse(raw || "null");
    if (!Array.isArray(widths) || widths.length !== WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS.length) {
      return null;
    }
    const normalized = widths.map((width, index) => {
      const value = Number(width);
      return Number.isFinite(value)
        ? Math.max(WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS[index], Math.round(value))
        : null;
    });
    return normalized.every((width) => width !== null) ? normalized : null;
  } catch {
    return null;
  }
}

function storeWorkspaceHistoryColumnWidths(widths) {
  try {
    window.localStorage.setItem(WORKSPACE_HISTORY_COLUMN_WIDTHS_KEY, JSON.stringify(widths));
  } catch {
    // Keep the current layout usable when localStorage is unavailable.
  }
}

function workspaceHistoryColumnElements() {
  if (typeof document === "undefined" || typeof document.querySelector !== "function") {
    return null;
  }
  const table = document.querySelector(".workspace-history-table");
  if (!table) {
    return null;
  }
  const columns = Array.from(table.querySelectorAll("col[data-workspace-history-column]"));
  const headers = Array.from(table.querySelectorAll("thead th"));
  return columns.length === WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS.length
    && headers.length === columns.length
    ? { table, columns, headers }
    : null;
}

function workspaceHistoryColumnWidths(elements) {
  const saved = readWorkspaceHistoryColumnWidths();
  if (saved) {
    return saved;
  }
  return elements.headers.map((header, index) => Math.max(
    WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS[index],
    Math.round(header.getBoundingClientRect().width),
  ));
}

function setupWorkspaceHistoryColumnResizing() {
  const elements = workspaceHistoryColumnElements();
  if (!elements || elements.table.dataset.columnResizeReady === "true") {
    return;
  }

  const widths = workspaceHistoryColumnWidths(elements);
  const applyWidths = (nextWidths) => {
    nextWidths.forEach((width, index) => {
      elements.columns[index].style.width = `${Math.round(width)}px`;
    });
  };
  applyWidths(widths);

  elements.headers.forEach((header, index) => {
    const handle = document.createElement("span");
    handle.className = "table-column-resize-handle";
    handle.dataset.columnIndex = String(index);
    handle.setAttribute("role", "separator");
    handle.setAttribute("aria-orientation", "vertical");
    handle.setAttribute("aria-label", `调整${header.textContent.trim()}列宽`);
    handle.tabIndex = 0;
    header.appendChild(handle);

    handle.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const startX = event.clientX;
      const startWidths = elements.columns.map((column) => column.getBoundingClientRect().width);
      const nextIndex = index + 1;
      const hasNextColumn = nextIndex < elements.columns.length;
      const startHandle = handle;
      const finish = () => {
        document.removeEventListener("pointermove", move);
        document.removeEventListener("pointerup", finish);
        document.removeEventListener("pointercancel", finish);
        document.body.classList.remove("is-resizing-table-column");
        startHandle.classList.remove("is-dragging");
        storeWorkspaceHistoryColumnWidths(elements.columns.map((column) => Math.round(column.getBoundingClientRect().width)));
      };
      const move = (moveEvent) => {
        const delta = moveEvent.clientX - startX;
        if (!hasNextColumn) {
          applyWidths([
            ...startWidths.slice(0, index),
            Math.max(WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS[index], startWidths[index] + delta),
          ]);
          return;
        }
        const currentWidth = Math.max(WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS[index], startWidths[index] + delta);
        const nextWidth = Math.max(WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS[nextIndex], startWidths[nextIndex] - (currentWidth - startWidths[index]));
        const adjustedDelta = currentWidth - startWidths[index];
        const nextWidths = [...startWidths];
        nextWidths[index] = startWidths[index] + adjustedDelta;
        nextWidths[nextIndex] = startWidths[nextIndex] - adjustedDelta;
        if (nextWidth !== nextWidths[nextIndex]) {
          nextWidths[index] = startWidths[index] + (startWidths[nextIndex] - nextWidth);
          nextWidths[nextIndex] = nextWidth;
        }
        applyWidths(nextWidths);
      };
      document.body.classList.add("is-resizing-table-column");
      startHandle.classList.add("is-dragging");
      document.addEventListener("pointermove", move);
      document.addEventListener("pointerup", finish);
      document.addEventListener("pointercancel", finish);
    });

    handle.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
        return;
      }
      event.preventDefault();
      const direction = event.key === "ArrowRight" ? 16 : -16;
      const currentWidths = elements.columns.map((column) => column.getBoundingClientRect().width);
      const nextIndex = index + 1;
      if (nextIndex >= elements.columns.length) {
        const nextWidth = Math.max(WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS[index], currentWidths[index] + direction);
        currentWidths[index] = nextWidth;
        applyWidths(currentWidths);
        storeWorkspaceHistoryColumnWidths(currentWidths.map((width) => Math.round(width)));
        return;
      }
      const nextWidth = currentWidths[nextIndex] - direction;
      if (nextWidth < WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS[nextIndex]) {
        return;
      }
      currentWidths[index] = Math.max(WORKSPACE_HISTORY_COLUMN_MIN_WIDTHS[index], currentWidths[index] + direction);
      currentWidths[nextIndex] = nextWidth;
      applyWidths(currentWidths);
      storeWorkspaceHistoryColumnWidths(currentWidths.map((width) => Math.round(width)));
    });
  });
  elements.table.dataset.columnResizeReady = "true";
}

setupWorkspaceHistoryColumnResizing();

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

const MAX_WORKSPACE_HISTORY_SEARCH_HISTORY = 12;

function readWorkspaceHistorySearchHistory() {
  try {
    const raw = window.localStorage.getItem(WORKSPACE_HISTORY_SEARCH_HISTORY_STORAGE_KEY);
    const values = JSON.parse(raw || "[]");
    if (!Array.isArray(values)) {
      return [];
    }
    return values
      .map((value) => String(value || "").trim())
      .filter(Boolean)
      .filter((value, index, list) => list.indexOf(value) === index)
      .slice(0, MAX_WORKSPACE_HISTORY_SEARCH_HISTORY);
  } catch {
    return [];
  }
}

function renderWorkspaceHistorySearchHistory() {
  if (!workspaceHistorySearchHistoryEl) {
    return;
  }
  workspaceHistorySearchHistoryEl.textContent = "";
  readWorkspaceHistorySearchHistory().forEach((value) => {
    const option = document.createElement("option");
    option.value = value;
    workspaceHistorySearchHistoryEl.appendChild(option);
  });
}

function rememberWorkspaceHistorySearch(query) {
  const value = String(query || "").trim();
  if (!value) {
    return;
  }
  const next = [value, ...readWorkspaceHistorySearchHistory().filter((item) => item !== value)]
    .slice(0, MAX_WORKSPACE_HISTORY_SEARCH_HISTORY);
  try {
    window.localStorage.setItem(WORKSPACE_HISTORY_SEARCH_HISTORY_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Search remains usable when localStorage is unavailable.
  }
  renderWorkspaceHistorySearchHistory();
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

async function refreshCurrentWorkspaceDirectoryFromTerminal(options = null) {
  const restoreWorkspacePath = Boolean(options?.restoreWorkspacePath);
  const explicitSessionId = String(
    restoreWorkspacePath
      ? state.initialReturnTerminalSessionId || ""
      : state.returnTerminalSessionId || "",
  ).trim();
  const storedSessionId =
    restoreWorkspacePath && !state.initialWorkspacePathProvided
      ? String(getStoredGlobalSessionId() || "").trim()
      : "";
  const sessionId = explicitSessionId || storedSessionId;
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
    if (restoreWorkspacePath) {
      state.returnTerminalSessionId = sessionId;
      state.currentPath = String(directory?.path || "");
    }

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
  [workspaceHistoryPathCopyButton, workspaceHistoryTerminalButton, workspaceHistoryDeleteButton].forEach((button) => {
    if (button) {
      button.disabled = disabled;
    }
  });
}

function workspaceHistoryLoadMessage() {
  const completed = Number(state.workspaceHistoryLoadCompleted) || 0;
  const total = Number(state.workspaceHistoryLoadTotal) || 0;
  const itemCount = Number(state.workspaceHistoryLoadItemCount) || 0;
  const current = String(state.workspaceHistoryLoadCurrent || "历史数据");
  if (state.workspaceHistoryLoadState === "loading") {
    return `正在读取：${current}（数据源 ${completed}/${total}，已获取 ${itemCount} 条）…`;
  }
  if (state.workspaceHistoryLoadState === "enriching") {
    return `正在补充活动终端详情：${current}（${completed}/${total}，已获取 ${itemCount} 条）…`;
  }
  if (state.workspaceHistoryLoadState === "error") {
    return `历史记录加载失败：${state.workspaceHistoryLoadError || "未知错误"}`;
  }
  return "";
}

function setWorkspaceHistoryLoadState(
  loadState,
  {
    completed = 0,
    total = 0,
    itemCount = state.workspaceHistoryLoadItemCount || 0,
    current = "",
    pending = [],
    error = "",
  } = {},
) {
  state.workspaceHistoryLoadState = loadState;
  state.workspaceHistoryLoadCompleted = completed;
  state.workspaceHistoryLoadTotal = total;
  state.workspaceHistoryLoadItemCount = itemCount;
  state.workspaceHistoryLoadCurrent = current;
  state.workspaceHistoryLoadPending = Array.isArray(pending) ? pending : [];
  state.workspaceHistoryLoadError = error;
  const message = workspaceHistoryLoadMessage();
  const isBusy = loadState === "loading" || loadState === "enriching";
  const progressPanel = typeof workspaceHistoryProgressPanelEl !== "undefined"
    ? workspaceHistoryProgressPanelEl
    : null;
  const currentWorkEl = typeof workspaceHistoryCurrentWorkEl !== "undefined"
    ? workspaceHistoryCurrentWorkEl
    : null;
  const progressLabelEl = typeof workspaceHistoryProgressLabelEl !== "undefined"
    ? workspaceHistoryProgressLabelEl
    : null;
  const progressEl = typeof workspaceHistoryProgressEl !== "undefined"
    ? workspaceHistoryProgressEl
    : null;
  if (progressPanel) {
    progressPanel.hidden = !isBusy;
  }
  if (currentWorkEl) {
    currentWorkEl.textContent = isBusy
      ? `当前工作：${current || "整理历史记录"}`
      : "";
  }
  if (progressLabelEl) {
    progressLabelEl.textContent = `${completed}/${total} · ${itemCount} 条`;
  }
  if (progressEl) {
    const percent = total > 0 ? Math.min(100, Math.round((completed / total) * 100)) : 0;
    progressEl.value = percent;
    progressEl.setAttribute("aria-valuetext", `${completed}/${total} 个数据源，${itemCount} 条数据`);
  }
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
  const recentQuery = state.workspaceHistoryRecentOnly ? "?recent_days=7" : "";
  if (state.workspaceHistorySearchAllWorkspaces) {
    return {
      path: "",
      sessions: "/api/terminal/sessions?all=true&history=true",
      conversations: `/api/terminal/codex-conversations${recentQuery}`,
    };
  }

  const path = workspaceHistoryLoadPath();
  const relativePath = relativePathBetweenAbsolute(state.workspaceDir || "/", path);
  return {
    path,
    sessions: `/api/terminal/sessions?path=${encodeURIComponent(relativePath)}&history=true`,
    conversations: `/api/terminal/codex-conversations?cwd=${encodeURIComponent(path)}${recentQuery ? "&recent_days=7" : ""}`,
  };
}

async function refreshWorkspaceHistoryConversations() {
  const requestToken = ++state.codexConversationRequestToken;
  state.workspaceHistoryDetailAbortController?.abort();
  const detailAbortController = typeof AbortController === "function"
    ? new AbortController()
    : { signal: undefined, abort() {} };
  state.workspaceHistoryDetailAbortController = detailAbortController;
  const loadTimeoutId = typeof window !== "undefined" && window.setTimeout
    ? window.setTimeout(() => {
      detailAbortController.abort();
    }, WORKSPACE_HISTORY_LOAD_TIMEOUT_MS)
    : null;
  const requestUrls = workspaceHistoryCoreRequestUrls();
  if (workspaceHistoryRefreshButton) {
    workspaceHistoryRefreshButton.disabled = true;
  }
  const coreRequestTotal = 3;
  let completedCoreRequests = 0;
  let loadedCoreItems = 0;
  const coreRequestLabels = ["活动终端", "终端归档", "Codex 对话"];
  let pendingCoreRequests = [...coreRequestLabels];
  setWorkspaceHistoryLoadState("loading", {
    total: coreRequestTotal,
    itemCount: 0,
    current: coreRequestLabels.join("、"),
    pending: pendingCoreRequests,
  });
  renderWorkspaceHistory();

  const loadCoreData = async (url, label, missingDirectoryFallback = null) => {
    let loadedItemCount = 0;
    try {
      const response = await requestJson(url, { signal: detailAbortController.signal });
      loadedItemCount = Array.isArray(response?.sessions)
        ? response.sessions.length
        : Array.isArray(response?.archives)
          ? response.archives.length
          : Array.isArray(response?.conversations)
            ? response.conversations.length
            : 0;
      loadedCoreItems += loadedItemCount;
      return response;
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
        pendingCoreRequests = pendingCoreRequests.filter((pendingLabel) => pendingLabel !== label);
        setWorkspaceHistoryLoadState("loading", {
          completed: completedCoreRequests,
          total: coreRequestTotal,
          itemCount: loadedCoreItems,
          current: loadedItemCount > 0
            ? `${label}（${loadedItemCount} 条）${pendingCoreRequests.length ? `，等待 ${pendingCoreRequests.join("、")}` : ""}`
            : pendingCoreRequests.join("、") || "整理读取结果",
          pending: pendingCoreRequests,
        });
      }
    }
  };

  try {
    const [sessionsResponse, archivesResponse, conversationsResponse] = await Promise.all([
      loadCoreData(requestUrls.sessions, "活动终端", requestUrls.path ? { sessions: [] } : null),
      loadCoreData("/api/terminal/resume-archives", "终端归档"),
      loadCoreData(requestUrls.conversations, "Codex 对话"),
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
      setWorkspaceHistoryLoadState("enriching", {
        total: terminalDetailCount,
        current: "终端 Session ID、输入历史",
        pending: ["终端 Session ID", "输入历史"],
      });
      renderWorkspaceHistory();
      await Promise.all([
        hydrateWorkspaceHistoryTerminalSessionIds(
          requestToken,
          reportTerminalDetailLoaded,
          detailAbortController.signal,
        ),
        hydrateWorkspaceHistoryTerminalInputHistory(
          requestToken,
          reportTerminalDetailLoaded,
          detailAbortController.signal,
        ),
      ]);
    }
    if (requestToken !== state.codexConversationRequestToken) {
      return;
    }
    setWorkspaceHistoryLoadState("loaded");
    renderWorkspaceHistory();
  } catch (error) {
    if (requestToken === state.codexConversationRequestToken) {
      const loadError = detailAbortController.signal?.aborted
        ? `读取历史记录超时（${Math.round(WORKSPACE_HISTORY_LOAD_TIMEOUT_MS / 1000)} 秒），请点击“刷新对话”重试。`
        : error.message || "读取对话列表失败。";
      setWorkspaceHistoryLoadState("error", {
        error: loadError,
      });
      renderWorkspaceHistory();
    }
  } finally {
    if (loadTimeoutId !== null && typeof window !== "undefined" && window.clearTimeout) {
      window.clearTimeout(loadTimeoutId);
    }
    if (requestToken === state.codexConversationRequestToken && workspaceHistoryRefreshButton) {
      workspaceHistoryRefreshButton.disabled = false;
    }
  }
}

const WORKSPACE_HISTORY_LOAD_TIMEOUT_MS = 30000;

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

const WORKSPACE_HISTORY_DETAIL_CONCURRENCY = 4;

async function runWorkspaceHistoryWithConcurrency(items, worker) {
  let cursor = 0;
  const runners = Array.from(
    { length: Math.min(WORKSPACE_HISTORY_DETAIL_CONCURRENCY, items.length) },
    async () => {
      while (cursor < items.length) {
        const item = items[cursor];
        cursor += 1;
        await worker(item);
      }
    },
  );
  await Promise.all(runners);
}

async function hydrateWorkspaceHistoryTerminalSessionIds(
  requestToken,
  onSettled = null,
  signal = null,
) {
  const terminalSessions = activeTerminalSessions(state.sessions).slice(0, 24);
  await runWorkspaceHistoryWithConcurrency(terminalSessions, async (session) => {
    try {
      const response = await requestJson(
        `/api/terminal/sessions/${encodeURIComponent(session.id)}/agent-session`,
        { signal },
      );
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
  });
}

async function hydrateWorkspaceHistoryTerminalInputHistory(
  requestToken,
  onSettled = null,
  signal = null,
) {
  const terminalSessions = activeTerminalSessions(state.sessions).slice(0, 24);
  await runWorkspaceHistoryWithConcurrency(terminalSessions, async (session) => {
    try {
      const payload = await requestJson(
        `/api/terminal/sessions/${encodeURIComponent(session.id)}/input-history`,
        { signal },
      );
      if (requestToken !== state.codexConversationRequestToken) {
        return;
      }
      session.input_history_text = workspaceHistoryInputHistoryText(payload.entries || []);
    } catch {
      session.input_history_text = "";
    } finally {
      onSettled?.();
    }
  });
}

function openWorkspaceHistoryTerminal(path) {
  const absolutePath = resolveWorkspaceHistoryPath(path);
  const scopeRoot = workspaceAccessRoot();
  if (!absolutePath || !isAbsolutePathWithinScope(absolutePath, scopeRoot)) {
    updateTableCardStatus(workspaceHistoryStatusEl, "这个历史目录不在当前工作区允许范围内，无法新建终端。", "warn");
    return;
  }

  recordWorkspaceHistory(absolutePath);
  openNewTerminal(absolutePath, workspaceHistoryTerminalButton);
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

function openWorkspaceHistorySessionDialog(item, fallbackPath, sessionAction, trigger) {
  const sessionId = String(item?.sessionId || "").trim();
  if (!sessionId) {
    updateTableCardStatus(workspaceHistoryStatusEl, "这个会话没有可用的 Session ID。", "warn");
    return;
  }
  const workingPath = resolveWorkspaceHistoryPath(
    item?.cwd || item?.terminal?.path || fallbackPath,
  );
  openTerminalNewSessionDialog(trigger, workingPath, {
    agent: item?.agentProgram || "codex",
    launchTerminal: openFreshTerminalSession,
    sessionAction,
    sessionId,
    sourceTerminalName: item?.activeTerminalName || "",
    updatePageStatus: showToast,
  });
}

function createWorkspaceHistoryPrimaryAction(item, fallbackPath) {
  if (item?.type === "terminal") {
    if (!item.terminal?.id) {
      return null;
    }
    return createActionButton("进入", () => openSession(item.terminal), "mini-button accent");
  }
  if (item?.type === "archive" && item.archive) {
    const archive = item.archive;
    const command = archiveCommand(archive);
    const archiveId = archiveIdentity(archive);
    const workingPath = resolveWorkspaceHistoryPath(archiveWorkingPath(item.archive));
    const runLink = createActionLink(
      "运行",
      buildTerminalUrl(workingPath, "", { fresh: true, runCommand: command }),
      "mini-button accent",
    );
    runLink.addEventListener("click", (event) => {
      openFreshTerminalRunLink(event, workingPath, command, {
        beforeNavigate: () => touchTerminalArchive(archiveIdentity(item.archive)),
        terminalName: item.activeTerminalName
          ? `${item.activeTerminalName}_resume`
          : "",
      });
    });
    return runLink;
  }
  if (!item?.sessionId) {
    return null;
  }
  let button;
  if (item?.type === "archive" || !workspaceHistoryArchiveForItem(item)) {
    button = createActionButton("恢复", () => {
      openWorkspaceHistorySessionDialog(item, fallbackPath, "resume", button);
    }, "mini-button accent");
    return button;
  }
  const selectedPath = fallbackPath;
  const workingPath = resolveWorkspaceHistoryPath(selectedPath);
  const command = resumeCommandFromId(item.sessionId);
  const runLink = createActionLink(
    "运行",
    buildTerminalUrl(workingPath, "", { fresh: true, runCommand: command }),
    "mini-button accent",
  );
  runLink.addEventListener("click", (event) => {
    openFreshTerminalRunLink(event, workingPath, command, {
      terminalName: item.activeTerminalName,
    });
  });
  return runLink;
}

function workspaceHistoryTerminalBaseName(name) {
  const currentName = String(name || "").trim();
  const numberedName = currentName.match(/^(.*?_\d+)(?:_.*)?$/);
  return numberedName?.[1] || currentName;
}

function workspaceHistoryTerminalNameFromItem(item) {
  const title = String(item?.title || "").trim();
  if (!title || title === "无对话摘要" || title === "活动终端") {
    return "";
  }

  return sessionRenameSavedName(title.replace(/\s+/g, "_")).slice(0, 120);
}

function buildWorkspaceHistoryReplacementTerminalName(item) {
  const currentName = String(item?.activeTerminalName || item?.terminal?.name || "").trim();
  if (!currentName) {
    return { currentName, nextName: "" };
  }
  const conversationName = workspaceHistoryTerminalNameFromItem(item);
  if (!conversationName) {
    return { currentName, nextName: "" };
  }

  const baseName = workspaceHistoryTerminalBaseName(currentName);
  return {
    currentName,
    nextName: `${baseName}_${conversationName}`.slice(0, 160),
  };
}

async function replaceWorkspaceHistoryTerminalName(item, button) {
  const { currentName, nextName } = buildWorkspaceHistoryReplacementTerminalName(item);
  if (!currentName) {
    updateTableCardStatus(workspaceHistoryStatusEl, "没有可替换名称的终端。", "warn");
    return;
  }
  if (!nextName) {
    updateTableCardStatus(workspaceHistoryStatusEl, "没有可用于替换终端名的对话内容。", "warn");
    return;
  }

  const confirmed = window.confirm(
    `确定将当前终端名称由：\n${currentName}\n改为：\n${nextName} 吗？`,
  );
  if (!confirmed) {
    return;
  }

  button.disabled = true;
  updateTableCardStatus(workspaceHistoryStatusEl, `正在替换终端名 ${currentName}…`, "info");
  try {
    if (item.terminal?.id) {
      const renamed = await requestJson(
        `/api/terminal/sessions/${encodeURIComponent(item.terminal.id)}`,
        {
          method: "PUT",
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify({ name: nextName }),
        },
      );
      announceSessionMutation("renamed", renamed);
      state.sessions = state.sessions.map((session) => (
        session.id === renamed.id ? { ...session, ...renamed } : session
      ));
    } else {
      await persistWorkspaceHistoryArchiveName(item, nextName);
    }

    try {
      await refreshWorkspaceHistoryConversations();
    } catch {
      renderWorkspaceHistory();
    }
    updateTableCardStatus(workspaceHistoryStatusEl, `终端已改名为 ${nextName}。`, "ok");
  } catch (error) {
    updateTableCardStatus(
      workspaceHistoryStatusEl,
      `替换终端名称失败：${error.message}`,
      "warn",
    );
  } finally {
    button.disabled = false;
  }
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

  const availablePresets = (Array.isArray(presets) ? presets.filter((preset) => preset?.id) : [])
    .slice()
    .sort((left, right) => Number(right.favorite === true) - Number(left.favorite === true));
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
    if (preset.favorite) {
      const favorite = document.createElement("span");
      favorite.className = "workspace-history-preset-favorite";
      favorite.textContent = "★";
      favorite.title = "已收藏";
      title.appendChild(favorite);
    }
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

function createWorkspaceHistoryMoreButton(item, fallbackPath) {
  let button;
  const actions = [];
  if (workspaceHistoryForkSupported(item) && workspaceHistoryForkCommand(item?.sessionId)) {
    actions.push({
      label: "fork",
      handler: () => openWorkspaceHistorySessionDialog(item, fallbackPath, "fork", button),
    });
  }
  if (item?.sessionId) {
    actions.push({
      label: "复制 ID",
      handler: () => copyWorkspaceHistorySessionId(item.sessionId),
    });
  }
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
  button.textContent = "…";
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
  // Hide conversations older than 7 days unless the user is searching (search ignores the date cap).
  const cutoffMs = state.workspaceHistoryRecentOnly && !query ? Date.now() - 7 * 86400000 : 0;
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
    cell.textContent = `最近7天没有对话，取消勾选"仅7天"可查看全部。`;
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
    const sessionId = String(item.sessionId || "").trim();
    if (sessionId) {
      const sessionValue = document.createElement("span");
      sessionValue.className = "workspace-history-session-value";
      sessionValue.textContent = sessionId;
      const copySessionIdButton = createActionButton("复制", () => {
        copyWorkspaceHistorySessionId(sessionId, copySessionIdButton);
      }, "mini-button workspace-history-session-copy");
      copySessionIdButton.title = "复制完整 Session ID";
      copySessionIdButton.setAttribute("aria-label", `复制 Session ID ${sessionId}`);
      const replaceTerminalNameButton = createActionButton("替换终端名", () => {
        replaceWorkspaceHistoryTerminalName(item, replaceTerminalNameButton);
      }, "mini-button workspace-history-session-copy");
      replaceTerminalNameButton.title = "用这条对话内容替换终端名";
      replaceTerminalNameButton.setAttribute(
        "aria-label",
        `用对话内容替换终端名：${item.title || ""}`,
      );
      sessionCell.append(sessionValue, copySessionIdButton, replaceTerminalNameButton);
    } else {
      sessionCell.textContent = "—";
    }
    sessionCell.title = sessionId || "没有检测到 Codex session ID";

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
    const primaryAction = createWorkspaceHistoryPrimaryAction(item, selectedPath);
    if (primaryAction) {
      actionCell.appendChild(primaryAction);
    } else {
      actionCell.textContent = "—";
    }
    if (item.terminal?.id || item.sessionId) {
      actionCell.appendChild(createWorkspaceHistoryMoreButton(item, selectedPath));
    }

    row.append(actionCell, terminalCell, activeTerminalNameCell, sessionCell, sizeCell, titleCell, timeCell);
    workspaceHistoryListEl.appendChild(row);
  });

  let statusText;
  let statusTone = "ok";
  const scopeLabel = searchAllWorkspaces ? "（全部工作区）" : "";
  const dateLabel = state.workspaceHistoryRecentOnly ? "（最近7天）" : "";
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
