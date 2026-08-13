import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const sessionsSource = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);

let resolveSessionList;
const selected = [];
const state = {
  sessions: [
    { id: "A", name: "A", path: "project", idle: false },
    { id: "B", name: "B", path: "project", idle: false },
  ],
  activeSessionId: "B",
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
  requestJson() {
    return new Promise((resolve) => {
      resolveSessionList = resolve;
    });
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
  currentLocationSessionId: () => state.activeSessionId,
  getStoredSessionId: () => "",
  getStoredGlobalSessionId: () => "",
  sessionPath: (session) => session.path,
  visibleSessions: () => state.sessions.filter((session) => !session.idle),
  isIdleSession: (sessionId) => Boolean(state.sessions.find((item) => item.id === sessionId)?.idle),
  isTerminalConnected: () => true,
  activeTerminalContext: { sessionId: "A" },
  selectSession(sessionId, options) {
    selected.push({ sessionId, pushHistory: Boolean(options?.pushHistory) });
    state.activeSessionId = sessionId;
  },
  syncAutoContinueHandledErrors() {},
  maybeAutoContinueErroredSessions() {},
};

vm.createContext(sandbox);
vm.runInContext(sessionsSource, sandbox);

// A queued dropdown refresh for B starts. While its list request is in flight,
// the user explicitly switches to A. The old request must not restore B when it
// returns, even though it inherited pushHistoryOnSelect from the earlier click.
const refresh = sandbox.loadSessions({
  preferredSessionId: "B",
  preserveCurrentList: true,
  pushHistoryOnSelect: true,
});
state.activeSessionId = "A";
resolveSessionList({
  sessions: state.sessions,
  path: "project",
  display_path: "/project",
});
await refresh;

assert.equal(
  state.activeSessionId,
  "A",
  "a stale session-list refresh must not override a newer user selection",
);
assert.ok(
  !selected.some((entry) => entry.sessionId === "B"),
  "the stale refresh must never select terminal B again",
);
assert.ok(
  selected.every((entry) => !entry.pushHistory),
  "a stale refresh that keeps A active must not add a duplicate browser-history entry",
);

console.log("terminal session selection race regression tests passed");
