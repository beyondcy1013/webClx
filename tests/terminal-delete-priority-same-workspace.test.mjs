import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const sessionsSource = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);

function createDeleteSessionSandbox({ sessions, activeSessionId, currentPath = "workspace-a" }) {
  const selected = [];
  const requestedJson = [];
  const state = {
    sessions: [...sessions],
    activeSessionId,
    loadingSessions: false,
    pendingSessionRefresh: null,
    showAllWorkspaceSessions: true,
    currentPath,
    pendingCreatedSessionIds: new Set(),
  };

  const createSessionButton = { disabled: false };
  const sandbox = {
    console,
    state,
    window: {
      confirm: () => true,
      requestAnimationFrame: (cb) => cb(),
    },
    createSessionButton,
    async requestJson(url, options) {
      requestedJson.push({ url, options });
      if (options?.method === "PUT" && url.endsWith("/hide")) {
        const hiddenId = decodeURIComponent(url.split("/").at(-2));
        const hidden = state.sessions.find((s) => s.id === hiddenId);
        if (hidden) hidden.idle = true;
        return hidden || { id: hiddenId, name: hiddenId };
      }
      return { sessions: state.sessions, path: state.currentPath, display_path: `/${state.currentPath}` };
    },
    shouldDeferSessionListRender: () => false,
    mergePendingSessionRefresh() {},
    normalizeTerminalPath: (value) => String(value || ""),
    sortSessionsByRecentActivity: (sessions) => sessions,
    sessionTimestamp: () => 0,
    updateSessionHiddenState(sessionId, hidden) {
      const target = state.sessions.find((item) => item.id === sessionId);
      if (target) target.idle = Boolean(hidden);
    },
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
    confirmTerminalExperienceSaveAndHide: async () => ({ shouldHide: true }),
    reportTerminalExperienceSaveOutcome() {},
    currentLocationSessionId: () => state.activeSessionId,
    getStoredSessionId: () => "",
    getStoredGlobalSessionId: () => "",
    sessionPath: (session) => session?.path || "",
    visibleSessions: () => state.sessions.filter((session) => !session.idle),
    isIdleSession: (sessionId) => Boolean(state.sessions.find((item) => item.id === sessionId)?.idle),
    isTerminalConnected: () => false,
    activeTerminalContext: null,
    selectSession(sessionId) {
      selected.push(sessionId);
      state.activeSessionId = sessionId;
      const target = state.sessions.find((s) => s.id === sessionId);
      if (target) {
        state.currentPath = target.path;
      }
    },
    disposeTerminalSessionContext() {},
    forgetSessionPreference(pathValue, sessionId) {
      if (state.activeSessionId === sessionId) {
        state.activeSessionId = "";
      }
    },
    announceSessionMutation() {},
    navigateBackWithFallback() {},
    syncAutoContinueHandledErrors() {},
    maybeAutoContinueErroredSessions() {},
  };

  vm.createContext(sandbox);
  vm.runInContext(sessionsSource, sandbox);
  return { sandbox, selected, state, requestedJson };
}

test("deleteSession prioritizes another terminal in the same workspace over other workspaces", async () => {
  const sessions = [
    { id: "ws-b-recent", name: "ws_b_1", path: "workspace-b", idle: false },
    { id: "ws-a-active", name: "ws_a_1", path: "workspace-a", idle: false },
    { id: "ws-a-other", name: "ws_a_2", path: "workspace-a", idle: false },
  ];

  const env = createDeleteSessionSandbox({
    sessions,
    activeSessionId: "ws-a-active",
    currentPath: "workspace-a",
  });

  const sessionToDelete = env.state.sessions.find((s) => s.id === "ws-a-active");
  await env.sandbox.deleteSession(sessionToDelete);

  assert.equal(
    env.state.activeSessionId,
    "ws-a-other",
    "should select the terminal in the same workspace rather than the one in workspace-b",
  );
  assert.equal(env.selected.at(-1), "ws-a-other");
});

test("deleteSession falls back to other workspace terminal if no other terminal exists in same workspace", async () => {
  const sessions = [
    { id: "ws-b-session", name: "ws_b_1", path: "workspace-b", idle: false },
    { id: "ws-a-only", name: "ws_a_1", path: "workspace-a", idle: false },
  ];

  const env = createDeleteSessionSandbox({
    sessions,
    activeSessionId: "ws-a-only",
    currentPath: "workspace-a",
  });

  const sessionToDelete = env.state.sessions.find((s) => s.id === "ws-a-only");
  await env.sandbox.deleteSession(sessionToDelete);

  assert.equal(
    env.state.activeSessionId,
    "ws-b-session",
    "should fallback to another workspace terminal when same workspace has none",
  );
});

test("deleteSession on non-active terminal keeps active terminal selected", async () => {
  const sessions = [
    { id: "ws-a-active", name: "ws_a_1", path: "workspace-a", idle: false },
    { id: "ws-b-bg", name: "ws_b_1", path: "workspace-b", idle: false },
  ];

  const env = createDeleteSessionSandbox({
    sessions,
    activeSessionId: "ws-a-active",
    currentPath: "workspace-a",
  });

  const sessionToDelete = env.state.sessions.find((s) => s.id === "ws-b-bg");
  await env.sandbox.deleteSession(sessionToDelete);

  assert.equal(env.state.activeSessionId, "ws-a-active", "should keep currently active terminal");
});

test("loadSessions prefers surviving terminal in the same workspace when active terminal exited", async () => {
  const previousSessions = [
    { id: "exited-session", name: "ws_a_1", path: "workspace-a", idle: false },
  ];
  const fetchedSessions = [
    { id: "ws-b-recent", name: "ws_b_1", path: "workspace-b", idle: false },
    { id: "ws-a-survivor", name: "ws_a_2", path: "workspace-a", idle: false },
  ];

  const env = createDeleteSessionSandbox({
    sessions: previousSessions,
    activeSessionId: "exited-session",
    currentPath: "workspace-a",
  });

  env.sandbox.requestJson = async () => ({
    sessions: fetchedSessions,
    path: "workspace-a",
    display_path: "/workspace-a",
  });

  await env.sandbox.loadSessions({ preferredSessionId: "exited-session" });

  assert.equal(
    env.state.activeSessionId,
    "ws-a-survivor",
    "loadSessions should pick surviving terminal in workspace-a over ws-b-recent",
  );
});
