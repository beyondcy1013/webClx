(function attachTerminalSessionActivity(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalSessionActivity = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalSessionActivity() {
  const TERMINAL_RECENT_OUTPUT_ACTIVE_MS = 15000;
  const TERMINAL_SESSION_SORT_MODES = Object.freeze(["workspace", "agent", "status"]);
  const TERMINAL_SESSION_SORT_MODE_LABELS = Object.freeze({
    workspace: "工作区",
    agent: "Agent 类型",
    status: "状态",
  });
  const TERMINAL_SESSION_STATUS_ORDER = Object.freeze({
    error: 0,
    retrying: 1,
    working: 2,
    building: 3,
    completed: 4,
    recent_output: 5,
    agent: 6,
    idle: 7,
  });

  function compareTerminalSessionText(left, right, { emptyLast = false } = {}) {
    const leftValue = String(left || "").trim();
    const rightValue = String(right || "").trim();
    if (emptyLast && Boolean(leftValue) !== Boolean(rightValue)) {
      return leftValue ? -1 : 1;
    }
    return leftValue.localeCompare(rightValue, undefined, {
      numeric: true,
      sensitivity: "base",
    });
  }

  function sessionTimestamp(session, snakeKey, camelKey) {
    const rawValue = Number(session?.[snakeKey] ?? session?.[camelKey] ?? 0);
    return Number.isFinite(rawValue) && rawValue > 0 ? rawValue : 0;
  }

  function sessionOrdinal(sessionId) {
    const match = /^s(\d+)$/.exec(sessionId || "");
    return match ? Number(match[1]) : 0;
  }

  function sortSessionsByRecentActivity(sessions) {
    if (!Array.isArray(sessions)) {
      return [];
    }

    return [...sessions].sort((left, right) => {
      return (
        sessionTimestamp(right, "last_opened_at", "lastOpenedAt") -
          sessionTimestamp(left, "last_opened_at", "lastOpenedAt") ||
        sessionTimestamp(right, "created_at", "createdAt") -
          sessionTimestamp(left, "created_at", "createdAt") ||
        sessionOrdinal(right?.id || "") - sessionOrdinal(left?.id || "") ||
        String(right?.id || "").localeCompare(String(left?.id || ""))
      );
    });
  }

  function sessionActivityState(session) {
    return String(session?.activity_state || session?.activityState || "idle");
  }

  function sessionActivityText(session) {
    return String(session?.activity_label || session?.activityLabel || "").trim();
  }

  function sessionActivityLabel(session, now = Date.now()) {
    const stateValue = sessionActivityState(session);
    const activityLabel = sessionActivityText(session);

    if (stateValue === "error") {
      return "错误";
    }
    if (stateValue === "retrying") {
      return "重试中";
    }
    if (stateValue === "working") {
      return "工作中";
    }
    if (stateValue === "building") {
      return "编译中";
    }
    if (stateValue === "completed") {
      return "待查看";
    }
    if (stateValue === "recent_output") {
      return "输出中";
    }
    if (stateValue === "agent") {
      return "空闲";
    }
    if (activityLabel && activityLabel !== "空闲" && activityLabel.toLowerCase() !== "codex") {
      return activityLabel;
    }
    return "空闲";
  }

  function sessionActivityAgentLabel(session) {
    return String(session?.activity_agent || session?.activityAgent || "").trim();
  }

  function terminalSessionWorkspaceLabel(session) {
    return String(
      session?.display_path || session?.displayPath || session?.path || "/",
    ).trim();
  }

  function terminalSessionName(session) {
    return String(session?.name || session?.id || "").trim();
  }

  function terminalSessionStatusRank(session) {
    const stateValue = sessionActivityState(session).toLowerCase();
    return TERMINAL_SESSION_STATUS_ORDER[stateValue] ?? Number.MAX_SAFE_INTEGER;
  }

  function normalizeTerminalSessionSortMode(mode) {
    return TERMINAL_SESSION_SORT_MODES.includes(mode) ? mode : "";
  }

  function nextTerminalSessionSortMode(mode) {
    const normalizedMode = normalizeTerminalSessionSortMode(mode);
    const currentIndex = TERMINAL_SESSION_SORT_MODES.indexOf(normalizedMode);
    return TERMINAL_SESSION_SORT_MODES[(currentIndex + 1) % TERMINAL_SESSION_SORT_MODES.length];
  }

  function terminalSessionSortModeLabel(mode) {
    return TERMINAL_SESSION_SORT_MODE_LABELS[normalizeTerminalSessionSortMode(mode)] || "最近活动";
  }

  function compareTerminalSessionWorkspace(left, right) {
    return compareTerminalSessionText(
      terminalSessionWorkspaceLabel(left),
      terminalSessionWorkspaceLabel(right),
    );
  }

  function compareTerminalSessionAgent(left, right) {
    return compareTerminalSessionText(
      sessionActivityAgentLabel(left),
      sessionActivityAgentLabel(right),
      { emptyLast: true },
    );
  }

  function compareTerminalSessionStatus(left, right) {
    return (
      terminalSessionStatusRank(left) - terminalSessionStatusRank(right) ||
      compareTerminalSessionText(sessionActivityState(left), sessionActivityState(right))
    );
  }

  function compareTerminalSessionName(left, right) {
    return (
      compareTerminalSessionText(terminalSessionName(left), terminalSessionName(right)) ||
      compareTerminalSessionText(left?.id, right?.id)
    );
  }

  function sortTerminalSessions(sessions, mode) {
    if (!Array.isArray(sessions)) {
      return [];
    }

    const normalizedMode = normalizeTerminalSessionSortMode(mode);
    if (!normalizedMode) {
      return sortSessionsByRecentActivity(sessions);
    }

    if (normalizedMode === "workspace") {
      const workspaceGroups = new Map();
      sessions.forEach((session) => {
        const workspace = terminalSessionWorkspaceLabel(session);
        if (!workspaceGroups.has(workspace)) {
          workspaceGroups.set(workspace, []);
        }
        workspaceGroups.get(workspace).push(session);
      });
      return [...workspaceGroups.values()].flat();
    }

    return [...sessions].sort((left, right) => {
      if (normalizedMode === "agent") {
        return (
          compareTerminalSessionAgent(left, right) ||
          compareTerminalSessionWorkspace(left, right) ||
          compareTerminalSessionStatus(left, right) ||
          compareTerminalSessionName(left, right)
        );
      }
      return (
        compareTerminalSessionStatus(left, right) ||
        compareTerminalSessionAgent(left, right) ||
        compareTerminalSessionWorkspace(left, right) ||
        compareTerminalSessionName(left, right)
      );
    });
  }

  function sessionAfterOutputViewed(session) {
    const stateValue = sessionActivityState(session);
    if (stateValue !== "completed" && stateValue !== "recent_output") {
      return session;
    }

    const agentLabel = sessionActivityAgentLabel(session);
    return {
      ...session,
      activity_state: agentLabel ? "agent" : "idle",
      activity_label: agentLabel || "空闲",
    };
  }

  function sessionActivityAgentPrefix(session, displayMode = "hidden") {
    const agentLabel = sessionActivityAgentLabel(session);
    return displayMode === "prefix" && agentLabel ? `[${agentLabel}] ` : "";
  }

  function sessionActivityAgentSuffix(session, displayMode = "hidden") {
    const agentLabel = sessionActivityAgentLabel(session);
    return displayMode === "suffix" && agentLabel ? ` [${agentLabel}]` : "";
  }

  function isSessionBusy(session) {
    return sessionActivityLabel(session) !== "空闲";
  }

  function sessionActivityPrefix(session) {
    return `[${sessionActivityLabel(session)}] `;
  }

  function terminalSessionActivityPrefix(session) {
    return isSessionBusy(session) ? `[${sessionActivityLabel(session)}] ` : "[空闲] ";
  }

  function sessionErrorContinueKey(session) {
    const sessionId = String(session?.id || "");
    const keyword = String(session?.activity_error_keyword || session?.activityErrorKeyword || "error");
    const signature = String(session?.activity_error_signature || session?.activityErrorSignature || "");
    return sessionId && signature ? `${sessionId}\n${keyword}\n${signature}` : "";
  }

  function isSessionErrorState(session) {
    const stateValue = sessionActivityState(session);
    return stateValue === "error" || stateValue === "retrying";
  }

  return {
    TERMINAL_RECENT_OUTPUT_ACTIVE_MS,
    TERMINAL_SESSION_SORT_MODES,
    isSessionBusy,
    isSessionErrorState,
    sessionActivityAgentLabel,
    sessionActivityAgentPrefix,
    sessionActivityAgentSuffix,
    sessionAfterOutputViewed,
    sessionActivityLabel,
    sessionActivityPrefix,
    sessionActivityState,
    sessionActivityText,
    sessionErrorContinueKey,
    sessionOrdinal,
    sessionTimestamp,
    nextTerminalSessionSortMode,
    normalizeTerminalSessionSortMode,
    sortSessionsByRecentActivity,
    sortTerminalSessions,
    terminalSessionSortModeLabel,
    terminalSessionActivityPrefix,
  };
});
