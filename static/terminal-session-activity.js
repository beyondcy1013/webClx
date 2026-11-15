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

  const ALLOCATED_GROUP_COLORS_KEY = "webclx:allocated-group-colors";
  const allocatedTerminalGroupColors = new Set();

  function loadAllocatedGroupColors() {
    try {
      const stored = globalThis.sessionStorage?.getItem(ALLOCATED_GROUP_COLORS_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        if (Array.isArray(parsed)) {
          for (const c of parsed) {
            const normalized = normalizeTerminalGroupColor(c);
            if (normalized) allocatedTerminalGroupColors.add(normalized);
          }
        }
      }
    } catch (_) {}
  }

  function saveAllocatedGroupColors() {
    try {
      const list = [...allocatedTerminalGroupColors].slice(-120);
      globalThis.sessionStorage?.setItem(ALLOCATED_GROUP_COLORS_KEY, JSON.stringify(list));
    } catch (_) {}
  }

  function recordAllocatedGroupColor(color) {
    const normalized = normalizeTerminalGroupColor(color);
    if (!normalized) return;
    allocatedTerminalGroupColors.add(normalized);
    saveAllocatedGroupColors();
  }

  function clearAllocatedGroupColors() {
    allocatedTerminalGroupColors.clear();
    try {
      globalThis.sessionStorage?.removeItem(ALLOCATED_GROUP_COLORS_KEY);
    } catch (_) {}
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

  function sessionProjectPath(session) {
    const raw = String(session?.icon_project_path || session?.path || session?.cwd || "").trim();
    if (!raw) return "";
    return raw.replaceAll("\\", "/").replace(/\/+$/, "").toLowerCase();
  }

  function hslToHex(hue, saturation = 76, lightness = 52) {
    const s = saturation / 100;
    const l = lightness / 100;
    const c = (1 - Math.abs(2 * l - 1)) * s;
    const hPrime = (((hue % 360) + 360) % 360) / 60;
    const x = c * (1 - Math.abs((hPrime % 2) - 1));
    const m = l - c / 2;
    let r = 0, g = 0, b = 0;
    if (hPrime < 1) { r = c; g = x; b = 0; }
    else if (hPrime < 2) { r = x; g = c; b = 0; }
    else if (hPrime < 3) { r = 0; g = c; b = x; }
    else if (hPrime < 4) { r = 0; g = x; b = c; }
    else if (hPrime < 5) { r = x; g = 0; b = c; }
    else { r = c; g = 0; b = x; }
    const toHex = (val) => Math.round((val + m) * 255).toString(16).padStart(2, "0");
    return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
  }

  function resolveGroupColors(sessions) {
    if (!Array.isArray(sessions) || sessions.length === 0) return new Map();
    const activeTokens = activeGroupTokens(sessions);
    if (activeTokens.length === 0) return new Map();

    loadAllocatedGroupColors();
    const tokenMembers = new Map();
    const tokenExplicitColors = new Map();

    for (const session of sessions) {
      const token = sessionGroupToken(session);
      if (!token || !activeTokens.includes(token)) continue;
      if (!tokenMembers.has(token)) tokenMembers.set(token, []);
      tokenMembers.get(token).push(session);
      const color = normalizeTerminalGroupColor(session?.group_color || session?.groupColor);
      if (color && !tokenExplicitColors.has(token)) {
        tokenExplicitColors.set(token, color);
      }
    }

    const tokenProjects = new Map();
    for (const [token, members] of tokenMembers) {
      const counts = new Map();
      for (const m of members) {
        const p = sessionProjectPath(m);
        counts.set(p, (counts.get(p) || 0) + 1);
      }
      let dominant = "";
      let maxCount = -1;
      for (const [p, count] of counts) {
        if (count > maxCount) {
          maxCount = count;
          dominant = p;
        }
      }
      tokenProjects.set(token, dominant);
    }

    const projectTokens = new Map();
    for (const token of activeTokens) {
      const project = tokenProjects.get(token) || "";
      if (!projectTokens.has(project)) projectTokens.set(project, []);
      projectTokens.get(project).push(token);
    }

    const resolved = new Map();
    for (const [project, tokens] of projectTokens) {
      const projectUsedColors = new Set();
      for (const token of tokens) {
        const explicit = tokenExplicitColors.get(token);
        if (explicit) {
          if (!projectUsedColors.has(explicit.toLowerCase())) {
            resolved.set(token, explicit);
            projectUsedColors.add(explicit.toLowerCase());
            continue;
          }
          const baseHue = terminalGroupHue(token);
          let assigned = "";
          for (let attempt = 0; attempt < 360; attempt++) {
            const hue = Math.round((baseHue + attempt * 137.508) % 360);
            const hex = hslToHex(hue, 76, 52).toLowerCase();
            if (!projectUsedColors.has(hex)) {
              assigned = hex;
              break;
            }
          }
          const finalColor = assigned || explicit;
          resolved.set(token, finalColor);
          projectUsedColors.add(finalColor.toLowerCase());
        } else {
          const baseHue = terminalGroupHue(token);
          let chosenHue = baseHue;
          for (let attempt = 0; attempt < 360; attempt++) {
            const h = Math.round((baseHue + attempt * 137.508) % 360);
            const hslStr = `hsl(${h} 76% 52%)`;
            const hex = hslToHex(h, 76, 52).toLowerCase();
            if (!projectUsedColors.has(hslStr) && !projectUsedColors.has(hex)) {
              chosenHue = h;
              break;
            }
          }
          const finalColor = `hsl(${chosenHue} 76% 52%)`;
          resolved.set(token, finalColor);
          projectUsedColors.add(finalColor);
          projectUsedColors.add(hslToHex(chosenHue, 76, 52).toLowerCase());
        }
      }
    }

    return resolved;
  }

  // A group's color is assigned when it is created and persists with its
  // backend state; membership changes elsewhere cannot reassign it.
  // When multiple groups belong to the same project, each group is assigned
  // a distinct color so they can be easily differentiated by background.
  function terminalGroupColor(session, sessions) {
    const token = sessionGroupToken(session);
    if (!token) return "";
    if (Array.isArray(sessions)) {
      const activeTokens = activeGroupTokens(sessions);
      if (!activeTokens.includes(token)) return "";
      const resolved = resolveGroupColors(sessions);
      return resolved.get(token) || "";
    }
    const normalized = normalizeTerminalGroupColor(session?.group_color || session?.groupColor);
    if (normalized) return normalized;
    const hue = terminalGroupHue(token);
    return `hsl(${hue} 76% 52%)`;
  }

  function defaultUnusedTerminalGroupColor(session, sessions) {
    loadAllocatedGroupColors();
    const usedColors = new Set(allocatedTerminalGroupColors);

    // 收集所有当前会话中已经存在的组颜色（全局防冲突）
    if (Array.isArray(sessions)) {
      for (const s of sessions) {
        if (!s?.group_token) continue;
        const color = normalizeTerminalGroupColor(s?.group_color || s?.groupColor);
        if (color) {
          usedColors.add(color.toLowerCase());
          allocatedTerminalGroupColors.add(color.toLowerCase());
        }
      }
    }

    const defaultColor = "#4f8cff";
    if (!usedColors.has(defaultColor)) {
      recordAllocatedGroupColor(defaultColor);
      return defaultColor;
    }

    // 避免随机数冲突：优先从当前未被占用的色相槽位中按黄金角步进稳定选取
    const token = sessionGroupToken(session) || `group-${allocatedTerminalGroupColors.size + 1}`;
    const baseHue = terminalGroupHue(token);
    for (let attempt = 0; attempt < 360; attempt++) {
      const hue = Math.round((baseHue + attempt * 137.508) % 360);
      const hex = hslToHex(hue, 76, 52).toLowerCase();
      if (!usedColors.has(hex)) {
        // 关键：选出后立即保存锁定到已获取颜色池中，后续再次取色绝不重复！
        recordAllocatedGroupColor(hex);
        return hex;
      }
    }

    // 备用兜底循环
    for (let h = 0; h < 360; h += 17) {
      const hex = hslToHex(h, 76, 52).toLowerCase();
      if (!usedColors.has(hex)) {
        recordAllocatedGroupColor(hex);
        return hex;
      }
    }

    return "#e22828";
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

  function sessionModelFamilyLabel(session) {
    const explicit = String(session?.model_family || session?.modelFamily || "").trim();
    if (explicit) {
      return explicit;
    }
    const statusModel = String(
      session?.codex_status?.model ||
      (typeof state !== "undefined" && typeof state?.sessionContextCache?.get === "function"
        ? state.sessionContextCache.get(session?.id)?.model
        : null) ||
      ""
    ).trim();
    if (statusModel) {
      const lower = statusModel.toLowerCase();
      if (lower.includes("deepseek")) return "deepseek";
      if (lower.includes("glm") || lower.includes("zhipu")) return "glm";
      if (lower.includes("claude") || lower.includes("sonnet") || lower.includes("opus") || lower.includes("haiku")) return "claude";
      if (lower.includes("gpt") || lower.includes("o1") || lower.includes("o3")) return "gpt";
      if (lower.includes("gemini")) return "gemini";
      if (lower.includes("qwen")) return "qwen";
      if (lower.includes("kimi") || lower.includes("moonshot")) return "kimi";
      return statusModel;
    }
    const text = `${session?.name || ""} ${session?.codex_api_preset_name || ""} ${session?.title || ""}`.toLowerCase();
    if (text.includes("deepseek")) return "deepseek";
    if (text.includes("glm") || text.includes("zhipu")) return "glm";
    if (text.includes("claude") || text.includes("sonnet") || text.includes("opus") || text.includes("haiku")) return "claude";
    if (text.includes("gpt") || text.includes("openai") || text.includes("o1") || text.includes("o3")) return "gpt";
    if (text.includes("gemini")) return "gemini";
    if (text.includes("qwen")) return "qwen";
    if (text.includes("kimi") || text.includes("moonshot")) return "kimi";
    return "";
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
    sessionModelFamilyLabel,
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
    defaultUnusedTerminalGroupColor,
    recordAllocatedGroupColor,
    clearAllocatedGroupColors,
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
