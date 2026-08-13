import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const sessionsSource = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);
const managerSource = readFileSync(
  new URL("../src/terminal/manager.rs", import.meta.url),
  "utf8",
);

function sourceBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start);
  assert.ok(start >= 0 && end > start, `missing source block: ${startMarker}`);
  return source.slice(start, end);
}

const explicitSessionLookup = sourceBetween(
  managerSource,
  "    fn get_session(",
  "    fn get_or_create_latest(",
);
assert.match(
  explicitSessionLookup,
  /sessions_by_id[\s\S]*get\(session_id\)[\s\S]*(?:with_context|ok_or_else)[\s\S]*不存在/,
  "an explicit websocket session id should fail when its stored session was deleted",
);
assert.doesNotMatch(
  explicitSessionLookup,
  /create_session_locked\(/,
  "an explicit websocket session id must never create a replacement session",
);

function createLoadSessionsSandbox({ previousSessions, fetchedSessions, activeSessionId }) {
  const selected = [];
  const state = {
    sessions: previousSessions,
    activeSessionId,
    loadingSessions: false,
    pendingSessionRefresh: null,
    showAllWorkspaceSessions: true,
    currentPath: "project",
    pendingCreatedSessionIds: new Set(),
  };
  const sandbox = {
    console,
    state,
    window: {
      requestAnimationFrame(callback) {
        callback();
      },
    },
    async requestJson() {
      return { sessions: fetchedSessions, path: "project", display_path: "/project" };
    },
    shouldDeferSessionListRender: () => false,
    mergePendingSessionRefresh() {},
    normalizeTerminalPath: (value) => String(value || ""),
    sortSessionsByRecentActivity: (sessions) => sessions,
    pruneTerminalSessionContexts() {},
    maybePlayTerminalCompletionSound() {},
    migrateLegacyIdleSessionIds: async () => {},
    syncCurrentPathDisplay() {},
    cancelNewSessionQuickStart() {},
    renderSessions() {},
    clearActiveSession() {
      state.activeSessionId = "";
    },
    closeSocket() {},
    updateStatus() {},
    updateSessionStatus() {},
    currentLocationSessionId: () => activeSessionId,
    getStoredSessionId: () => "",
    getStoredGlobalSessionId: () => "",
    sessionPath: (session) => session.path,
    visibleSessions: () => state.sessions.filter((session) => !session.idle),
    isIdleSession: (sessionId) => Boolean(state.sessions.find((item) => item.id === sessionId)?.idle),
    isTerminalConnected: () => false,
    activeTerminalContext: null,
    selectSession(sessionId) {
      selected.push(sessionId);
      state.activeSessionId = sessionId;
    },
    syncAutoContinueHandledErrors() {},
    maybeAutoContinueErroredSessions() {},
  };
  vm.createContext(sandbox);
  vm.runInContext(sessionsSource, sandbox);
  return { sandbox, selected, state };
}

const stale = createLoadSessionsSandbox({
  previousSessions: [{ id: "deleted", name: "project_2", path: "project", idle: false }],
  fetchedSessions: [{ id: "survivor", name: "project_1", path: "project", idle: false }],
  activeSessionId: "deleted",
});
await stale.sandbox.loadSessions({ preferredSessionId: "deleted" });
assert.deepEqual(
  stale.state.sessions.map((session) => session.id),
  ["survivor"],
  "a stale browser must trust the server list and drop a session deleted elsewhere",
);
assert.deepEqual(
  stale.selected,
  ["survivor"],
  "a stale browser should select a surviving session instead of reconnecting the deleted id",
);

const pending = createLoadSessionsSandbox({
  previousSessions: [{ id: "fresh", name: "project_2", path: "project", idle: false }],
  fetchedSessions: [{ id: "survivor", name: "project_1", path: "project", idle: false }],
  activeSessionId: "fresh",
});
pending.state.pendingCreatedSessionIds.add("fresh");
await pending.sandbox.loadSessions({ preferredSessionId: "fresh" });
assert.deepEqual(
  pending.state.sessions.map((session) => session.id),
  ["survivor", "fresh"],
  "a session created by this page should survive one list response that raced its creation",
);
assert.deepEqual(pending.selected, ["fresh"]);

console.log("deleted terminal reconnect regression tests passed");
