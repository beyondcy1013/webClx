// 会话标签、排序、视图刷新与交互保护模块。
// 由 app.js 拆出，在 app.js 之前以 <script defer> 加载，
// 通过共享全局作用域向 app.js 提供下列函数，无需修改调用方。
// 依赖的全局（state.*、sharedAnnounceSessionMutation、normalizeAbsolutePath 等）
// 均为 app.js 顶层声明，加载顺序保证可用。

function sessionLocationLabel(session) {
  return normalizeAbsolutePath(session.display_path || displayPath(session.path));
}

function sessionWorkspacePath(session) {
  return sessionLocationLabel(session || {});
}

function archiveWorkspacePath(archive) {
  return resolveAbsolutePath(state.workspaceDir || "/", archiveWorkingPath(archive));
}

function sessionDirectoryLabel(session) {
  const sessionPath = sessionLocationLabel(session);
  const currentPath = state.currentDirectory?.display_path
    ? normalizeAbsolutePath(state.currentDirectory.display_path)
    : "";

  if (currentPath && sessionPath === currentPath) {
    return "当前目录";
  }

  const parts = splitPathParts(sessionPath);
  return parts.length > 0 ? parts[parts.length - 1] : "/";
}

function sessionTimestamp(session, snakeKey, camelKey) {
  return sharedSessionTimestamp(session, snakeKey, camelKey);
}

function sortSessionsByRecentActivity(sessions) {
  return sharedSortSessionsByRecentActivity(sessions);
}

function sortSessionsByPath(sessions) {
  if (!Array.isArray(sessions)) {
    return [];
  }
  return [...sessions].sort((left, right) => {
    const pathA = (left?.path || "").toLowerCase();
    const pathB = (right?.path || "").toLowerCase();
    return pathA.localeCompare(pathB);
  });
}

function sortSessionsByStatus(sessions) {
  if (!Array.isArray(sessions)) {
    return [];
  }
  return [...sessions].sort((left, right) => {
    const statusA = (left?.status || left?.activityState || left?.activity_state || "").toString().toLowerCase();
    const statusB = (right?.status || right?.activityState || right?.activity_state || "").toString().toLowerCase();
    return statusA.localeCompare(statusB);
  });
}

function sortAndRenderDirectorySessions(sortMode) {
  let sorted = [...state.directorySessions];
  if (sortMode === "path") {
    sorted = sortSessionsByPath(sorted);
  } else if (sortMode === "status") {
    sorted = sortSessionsByStatus(sorted);
  }
  state.directorySessions = sorted;
  renderDirectorySessions();
}
globalThis.sortAndRenderDirectorySessions = sortAndRenderDirectorySessions;

function activeTerminalSessions(sessions) {
  return (Array.isArray(sessions) ? sessions : []).filter((session) => !session?.idle);
}

function announceSessionMutation(action, session = {}) {
  sharedAnnounceSessionMutation(action, session, state.currentPath);
}

function refreshSessionViews({ preserveCurrentList = false, silentSessionsStatus = false } = {}) {
  loadDirectorySessions({ preserveCurrentList });
  if (state.activeTab === "workspace") {
    loadSessions({ preserveCurrentList, silentStatus: silentSessionsStatus });
  }
}

function mergePendingSessionViewsRefresh(nextRequest = {}) {
  const nextPreserveCurrentList = Boolean(nextRequest.preserveCurrentList);
  const nextSilentSessionsStatus = Boolean(nextRequest.silentSessionsStatus);

  if (!pendingSessionViewsRefresh) {
    pendingSessionViewsRefresh = {
      preserveCurrentList: nextPreserveCurrentList,
      silentSessionsStatus: nextSilentSessionsStatus,
    };
    return;
  }

  pendingSessionViewsRefresh = {
    preserveCurrentList:
      pendingSessionViewsRefresh.preserveCurrentList || nextPreserveCurrentList,
    silentSessionsStatus:
      pendingSessionViewsRefresh.silentSessionsStatus || nextSilentSessionsStatus,
  };
}

function armSessionViewsRefreshTimer() {
  if (sessionViewsRefreshTimer !== null) {
    return;
  }

  sessionViewsRefreshTimer = window.setTimeout(() => {
    sessionViewsRefreshTimer = null;
    const nextRefresh = pendingSessionViewsRefresh || {};
    pendingSessionViewsRefresh = null;
    refreshSessionViews(nextRefresh);
  }, SESSION_VIEWS_REFRESH_DELAY_MS);
}

function scheduleSessionViewsRefresh(nextRequest = {}) {
  mergePendingSessionViewsRefresh(nextRequest);
  armSessionViewsRefreshTimer();
}

function restartPendingSessionViewsRefresh() {
  if (sessionViewsRefreshTimer === null) {
    return;
  }

  window.clearTimeout(sessionViewsRefreshTimer);
  sessionViewsRefreshTimer = null;
  armSessionViewsRefreshTimer();
}

function scheduleSessionSelectInteractionFlush(callback) {
  if (sessionSelectInteractionFlushTimer !== null) {
    window.clearTimeout(sessionSelectInteractionFlushTimer);
  }

  sessionSelectInteractionFlushTimer = window.setTimeout(() => {
    sessionSelectInteractionFlushTimer = null;
    window.requestAnimationFrame(callback);
  }, 120);
}

function bindSessionSelectInteractionGuard(selectEl, { setBlocked, flushPending }) {
  if (!selectEl) {
    return;
  }

  const beginInteraction = () => {
    if (sessionSelectInteractionFlushTimer !== null) {
      window.clearTimeout(sessionSelectInteractionFlushTimer);
      sessionSelectInteractionFlushTimer = null;
    }
    setBlocked(true);
    restartPendingSessionViewsRefresh();
  };

  const endInteraction = () => {
    setBlocked(false);
    scheduleSessionSelectInteractionFlush(flushPending);
  };

  selectEl.addEventListener("pointerdown", beginInteraction);
  selectEl.addEventListener("mousedown", beginInteraction);
  selectEl.addEventListener("touchstart", beginInteraction, { passive: true });
  selectEl.addEventListener("focus", beginInteraction);
  selectEl.addEventListener("keydown", (event) => {
    if ([" ", "Enter", "ArrowDown", "ArrowUp"].includes(event.key)) {
      beginInteraction();
    }
    if (["Escape", "Enter"].includes(event.key)) {
      window.setTimeout(endInteraction, 120);
    }
  });
  selectEl.addEventListener("blur", endInteraction);
  selectEl.addEventListener("change", endInteraction);
}
