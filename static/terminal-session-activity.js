(function attachTerminalSessionActivity(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalSessionActivity = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalSessionActivity() {
const TERMINAL_RECENT_OUTPUT_ACTIVE_MS = 15000;
const TERMINAL_MODEL_PEAK_WINDOWS = Object.freeze({
  glm: "14:00-18:00",
  deepseek_weekday: ["09:00-12:00", "14:00-18:00"],
});
  const TERMINAL_SESSION_SORT_MODES = Object.freeze(["workspace", "agent", "status"]);
  const TERMINAL_SESSION_SORT_MODE_LABELS = Object.freeze({
    workspace: "工作区",
    agent: "Agent 类型",
    status: "状态",
  });
  const TERMINAL_SESSION_STATUS_ORDER = Object.freeze({
    error: 0,
    orphaned_agent: 1,
    retrying: 2,
    working: 3,
    compacting: 3,
    building: 4,
    completed: 5,
    compacted: 6,
    recent_output: 7,
    agent: 8,
    idle: 9,
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

  function sessionGroupToken(session) {
    return String(session?.group_token || session?.groupToken || "").trim();
  }

  function terminalGroupHue(token) {
    const value = String(token || "group");
    let hash = 2166136261;
    for (const character of value) {
      hash ^= character.codePointAt(0);
      hash = Math.imul(hash, 16777619);
    }
    return (hash >>> 0) % 360;
  }

  function normalizeTerminalGroupColor(value) {
    const color = String(value || "").trim();
    return /^#[0-9a-fA-F]{6}$/.test(color) ? color.toLowerCase() : "";
  }

  function activeGroupTokens(sessions) {
    const counts = new Map();
    if (!Array.isArray(sessions)) return [];
    for (const session of sessions) {
      const token = sessionGroupToken(session);
      if (!token) continue;
      counts.set(token, (counts.get(token) || 0) + 1);
    }
    return [...counts.entries()]
      .filter(([, count]) => count > 1)
      .map(([token]) => token)
      .sort((left, right) => compareTerminalSessionText(left, right));
  }

  // A group's color is assigned when it is created and persists with its
  // backend state; membership changes elsewhere cannot reassign it.
  function terminalGroupColor(session, sessions) {
    const token = sessionGroupToken(session);
    if (!token) return "";
    const tokens = activeGroupTokens(sessions);
    const index = tokens.indexOf(token);
    if (index < 0) return "";
    const normalized = normalizeTerminalGroupColor(session?.group_color || session?.groupColor);
    if (normalized) return normalized;
    const hue = terminalGroupHue(token);
    return `hsl(${hue} 76% 52%)`;
  }

  function terminalGroupedSessionIds(sessions) {
    if (!Array.isArray(sessions)) {
      return new Set();
    }
    const groups = new Map();
    for (const session of sessions) {
      const token = sessionGroupToken(session);
      if (!token) continue;
      if (!groups.has(token)) groups.set(token, []);
      groups.get(token).push(session.id);
    }
    return new Set(
      [...groups.values()]
        .filter((ids) => ids.length > 1)
        .flat()
    );
  }

  // Collapse groups at their highest ranked member, then expand in persisted order.
  function sessionGroupOrder(session) {
    const value = Number(session?.group_order ?? session?.groupOrder ?? 0);
    return Number.isFinite(value) && value > 0 ? value : 0;
  }

  function compareTerminalGroupMembers(left, right) {
    return sessionGroupOrder(left) - sessionGroupOrder(right)
      || sessionOrdinal(left?.id || "") - sessionOrdinal(right?.id || "")
      || (String(left?.id || "") < String(right?.id || "") ? -1
        : String(left?.id || "") > String(right?.id || "") ? 1 : 0);
  }

  function keepTerminalGroupsTogether(sessions) {
    const groups = new Map();
    for (const session of sessions) {
      const token = sessionGroupToken(session);
      if (!token) continue;
      if (!groups.has(token)) groups.set(token, []);
      groups.get(token).push(session);
    }
    for (const members of groups.values()) {
      members.sort(compareTerminalGroupMembers);
    }
    return sessions.flatMap((session) => {
      const token = sessionGroupToken(session);
      if (!token) return [session];
      const members = groups.get(token) || [];
      groups.delete(token);
      return members;
    });
  }

  function sortSessionsByRecentActivity(sessions) {
    if (!Array.isArray(sessions)) return [];
    return keepTerminalGroupsTogether([...sessions].sort((left, right) =>
      sessionTimestamp(right, "last_opened_at", "lastOpenedAt") -
        sessionTimestamp(left, "last_opened_at", "lastOpenedAt") ||
      sessionTimestamp(right, "created_at", "createdAt") -
        sessionTimestamp(left, "created_at", "createdAt") ||
      sessionOrdinal(right?.id || "") - sessionOrdinal(left?.id || "") ||
      (String(right?.id || "") < String(left?.id || "") ? -1 : String(right?.id || "") > String(left?.id || "") ? 1 : 0)
    ));
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
    const resetAtText = String(
      session?.activity_error_auto_continue_at || session?.activityErrorAutoContinueAt || "",
    ).trim();
    const resetAtMillis = resetAtText
      ? Date.parse(resetAtText.includes("T") ? resetAtText : resetAtText.replace(" ", "T"))
      : Number.NaN;

    if (
      (stateValue === "error" || stateValue === "retrying")
      && Number.isFinite(resetAtMillis)
      && resetAtMillis > now
    ) {
      return "等待重置";
    }
    if (stateValue === "error") {
      return "错误";
    }
    if (stateValue === "orphaned_agent") {
      return "Agent 异常";
    }
    if (stateValue === "retrying") {
      return "重试中";
    }
    if (stateValue === "working") {
      return "工作中";
    }
    if (stateValue === "compacting") {
      return "压缩中";
    }
    if (stateValue === "building") {
      return "编译中";
    }
    if (stateValue === "completed") {
      return "待查看";
    }
    if (stateValue === "compacted") {
      return "已压缩";
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

  function terminalModelPeakFamily(session) {
    const family = String(
      session?.model_family || session?.modelFamily || "",
    )
      .trim()
      .toLowerCase();
    if (family.includes("glm") || family.includes("zhipu")) {
      return "glm";
    }
    if (family.includes("deepseek")) return "deepseek";
    return "";
  }

  function terminalSessionInModelPeakWindow(session, date = new Date()) {
    const family = terminalModelPeakFamily(session);
    if (!family) return false;
    const minutes = date.getHours() * 60 + date.getMinutes();
    const inWindow = ([start, end]) => {
      const [sh, sm] = String(start).split(":").map(Number);
      const [eh, em] = String(end).split(":").map(Number);
      const startMinute = sh * 60 + sm;
      const endMinute = eh * 60 + em;
      return minutes >= startMinute && minutes < endMinute;
    };
    if (family === "glm") {
      return inWindow(TERMINAL_MODEL_PEAK_WINDOWS.glm.split("-"));
    }
    const weekday = date.getDay();
    if (weekday === 0 || weekday === 6) return false;
    return TERMINAL_MODEL_PEAK_WINDOWS.deepseek_weekday.some((window) => {
      return inWindow(window.split("-"));
    });
  }

  function terminalModelPeakTransitionKey(sessions) {
    return (Array.isArray(sessions) ? sessions : [])
      .map((session) => {
        const active = terminalSessionInModelPeakWindow(session) ? "1" : "0";
        return `${session?.id || ""}:${terminalModelPeakFamily(session)}:${active}`;
      })
      .join("\n");
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
      return keepTerminalGroupsTogether([...workspaceGroups.values()].flat());
    }

    return keepTerminalGroupsTogether([...sessions].sort((left, right) => {
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
    }));
  }

  function sessionAfterOutputViewed(session) {
    const stateValue = sessionActivityState(session);
    if (stateValue !== "completed" && stateValue !== "compacted" && stateValue !== "recent_output") {
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
    sessionGroupOrder,
    sessionGroupToken,
    sessionOrdinal,
    terminalGroupHue,
    normalizeTerminalGroupColor,
    sessionTimestamp,
    terminalGroupColor,
    terminalGroupedSessionIds,
    nextTerminalSessionSortMode,
    normalizeTerminalSessionSortMode,
    sortSessionsByRecentActivity,
    sortTerminalSessions,
    terminalSessionSortModeLabel,
    terminalSessionActivityPrefix,
    terminalModelPeakFamily,
    terminalModelPeakTransitionKey,
    terminalSessionInModelPeakWindow,
  };
});
