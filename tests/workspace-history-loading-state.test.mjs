import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(new URL("../static/app-workspace-history.js", import.meta.url), "utf8");
const navigationSource = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
  "utf8",
);
const settingsSource = readFileSync(
  new URL("../static/app-settings-load-save.js", import.meta.url),
  "utf8",
);

function functionSource(script, name) {
  const start = script.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const bodyStart = script.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < script.length; index += 1) {
    if (script[index] === "{") depth += 1;
    if (script[index] === "}") depth -= 1;
    if (depth === 0) return script.slice(start, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function flushPromises() {
  await Promise.resolve();
  await new Promise((resolve) => setImmediate(resolve));
}

function createHarness() {
  const requests = new Map();
  const renders = [];
  const statuses = [];
  const refreshButton = { disabled: false };
  const createNode = () => ({
    children: [],
    value: "",
    disabled: false,
    _textContent: "",
    set textContent(value) {
      this._textContent = String(value);
      if (value === "") {
        this.children = [];
      }
    },
    get textContent() {
      return this._textContent;
    },
    appendChild(child) {
      this.children.push(child);
      return child;
    },
    append(...children) {
      this.children.push(...children);
    },
  });
  const historyList = createNode();
  const pathSelect = createNode();
  const state = {
    codexConversationRequestToken: 0,
    sessions: [],
    terminalArchives: [],
    codexConversations: [],
    workspaceDir: "/home/codes",
    workspaceHistory: [
      { path: "/home/codes/webClx", last_opened_at: 1 },
      { path: "/home/codes/other", last_opened_at: 0 },
    ],
    workspaceHistorySelectedPath: "/home/codes/webClx",
    workspaceHistorySearchAllWorkspaces: false,
  };
  const context = vm.createContext({
    console,
    state,
    workspaceHistoryRefreshButton: refreshButton,
    workspaceHistoryStatusEl: {},
    workspaceHistoryListEl: historyList,
    workspaceHistoryPathSelectEl: pathSelect,
    workspaceHistoryOpenButton: null,
    workspaceHistoryTerminalButton: null,
    workspaceHistoryDeleteButton: null,
    document: { createElement: createNode },
    requestJson(url) {
      if (!requests.has(url)) {
        requests.set(url, deferred());
      }
      return requests.get(url).promise;
    },
    updateTableCardStatus(_element, message, tone) {
      statuses.push({ message, tone });
    },
    updateStatus(_element, message, tone) {
      statuses.push({ message, tone });
    },
    sortSessionsByRecentActivity: (sessions) => sessions,
    activeTerminalSessions: (sessions) => sessions,
    sortTerminalArchives: (archives) => archives,
    normalizeAbsolutePath: (path) => String(path || "").replace(/\/$/, "") || "/",
    relativePathBetweenAbsolute(basePath, targetPath) {
      return String(targetPath).replace(`${String(basePath).replace(/\/$/, "")}/`, "");
    },
    sessionWorkspacePath: (session) => String(session?.display_path || session?.path || ""),
    archiveWorkspacePath: (archive) => String(archive?.cwd || ""),
    archiveResumeId: (archive) => String(archive?.resume_id || archive?.id || "").trim(),
  });
  vm.runInContext(source, context);
  const renderWorkspaceHistory = context.renderWorkspaceHistory;
  context.renderWorkspaceHistory = () => {
    renders.push({
      loadState: state.workspaceHistoryLoadState,
      conversations: state.codexConversations.length,
    });
  };

  return {
    context,
    historyList,
    pathSelect,
    refreshButton,
    renderWorkspaceHistory,
    renders,
    requests,
    state,
    statuses,
  };
}

test("workspace history waits for settings before its first directory request", () => {
  const refreshes = [];
  const renders = [];
  const state = {
    activeTab: "workspace-history",
    activeSettingsTab: "system",
    workspaceHistorySettingsReady: false,
  };
  const context = vm.createContext({
    state,
    tabButtons: [],
    workspaceViewEl: null,
    workspaceHistoryViewEl: null,
    sessionsViewEl: null,
    terminalArchivesViewEl: null,
    authViewEl: null,
    apiViewEl: null,
    claudeViewEl: null,
    settingsViewEl: null,
    desktopViewEl: null,
    workspaceDirInputEl: {},
    window: { requestAnimationFrame() {} },
    setTabPanelActive() {},
    setActiveSettingsTab() {},
    loadSessions() {},
    loadTerminalArchives() {},
    renderWorkspaceHistory() {
      renders.push(state.workspaceHistorySettingsReady);
    },
    prioritizeWorkspaceHistoryCurrentDirectory() {},
    refreshWorkspaceHistoryConversations() {
      refreshes.push(state.workspaceHistorySettingsReady);
    },
    ensureAuthPresetsLoaded() {},
    ensureApiPresetsLoaded() {},
    ensureClaudePresetsLoaded() {},
    syncTabUrl() {},
  });

  vm.runInContext(functionSource(navigationSource, "markWorkspaceHistorySettingsReady"), context);
  vm.runInContext(functionSource(navigationSource, "setActiveTab"), context);

  context.setActiveTab("workspace-history");
  assert.equal(renders.length, 1);
  assert.equal(refreshes.length, 0, "initial tab activation must not request with an unknown workspace root");

  context.markWorkspaceHistorySettingsReady();
  assert.deepEqual(refreshes, [true], "settings readiness should start the first scoped request once");

  context.markWorkspaceHistorySettingsReady();
  assert.equal(refreshes.length, 1, "reloading settings must not duplicate the initial request");

  context.setActiveTab("workspace");
  context.setActiveTab("workspace-history");
  assert.equal(refreshes.length, 2, "returning to the ready history tab should refresh normally");
  assert.match(
    settingsSource,
    /renderWorkspaceHistory\(\);\s*markWorkspaceHistorySettingsReady\(\);/,
    "successful settings loading should release the workspace-history request gate",
  );
});

test("workspace history shows core and enrichment progress before reporting loaded", async () => {
  const harness = createHarness();
  const refreshPromise = harness.context.refreshWorkspaceHistoryConversations();

  assert.equal(harness.state.workspaceHistoryLoadState, "loading");
  assert.equal(harness.refreshButton.disabled, true);
  assert.match(harness.statuses.at(-1).message, /0\/3/);

  harness.requests.get("/api/terminal/sessions?path=webClx").resolve({
    sessions: [
      { id: "terminal-1", path: "webClx", display_path: "/home/codes/webClx" },
      { id: "terminal-other", path: "other", display_path: "/home/codes/other" },
    ],
  });
  await flushPromises();
  assert.match(harness.statuses.at(-1).message, /1\/3/);

  harness.requests.get("/api/terminal/resume-archives").resolve({ archives: [] });
  harness.requests.get("/api/terminal/codex-conversations?cwd=%2Fhome%2Fcodes%2FwebClx").resolve({
    conversations: [
      { session_id: "conversation-1", cwd: "/home/codes/webClx" },
      { session_id: "conversation-other", cwd: "/home/codes/other" },
    ],
  });
  await flushPromises();

  assert.equal(harness.state.workspaceHistoryLoadState, "enriching");
  assert.ok(
    harness.renders.some((render) => render.loadState === "enriching" && render.conversations === 1),
    "core conversation rows should render before terminal detail requests finish",
  );
  assert.match(harness.statuses.at(-1).message, /0\/2/);

  harness.requests.get("/api/terminal/sessions/terminal-1/agent-session").resolve({
    resume_id: "conversation-1",
  });
  harness.requests.get("/api/terminal/sessions/terminal-1/input-history").resolve({ entries: [] });
  await refreshPromise;

  assert.equal(harness.state.workspaceHistoryLoadState, "loaded");
  assert.deepEqual(harness.state.sessions.map((session) => session.id), ["terminal-1"]);
  assert.deepEqual(
    harness.state.codexConversations.map((conversation) => conversation.session_id),
    ["conversation-1"],
  );
  assert.equal(harness.refreshButton.disabled, false);
  assert.ok(harness.renders.some((render) => render.loadState === "loaded"));
});

test("workspace history keeps a visible error state when a core request fails", async () => {
  const harness = createHarness();
  const refreshPromise = harness.context.refreshWorkspaceHistoryConversations();

  harness.requests.get("/api/terminal/sessions?path=webClx").reject(new Error("会话接口不可用"));
  await refreshPromise;

  assert.equal(harness.state.workspaceHistoryLoadState, "error");
  assert.match(harness.state.workspaceHistoryLoadError, /会话接口不可用/);
  assert.equal(harness.refreshButton.disabled, false);
  assert.deepEqual(harness.renders.at(-1), { loadState: "error", conversations: 0 });
  assert.deepEqual(harness.statuses.at(-1), { message: "历史记录加载失败：会话接口不可用", tone: "warn" });
});

test("workspace history prefers the browser current directory on first open", () => {
  const harness = createHarness();
  harness.state.currentWorkspaceDirectoryPath = "/home/codes/webClx";
  harness.state.workspaceHistorySelectedPath = "";

  harness.context.syncWorkspaceHistoryPathSelect([
    { path: "/home/codes/deleted", lastActivity: 2 },
    { path: "/home/codes/webClx", lastActivity: 1 },
  ]);

  assert.equal(harness.state.workspaceHistorySelectedPath, "/home/codes/webClx");
  assert.equal(harness.pathSelect.children.find((option) => option.selected)?.value, "/home/codes/webClx");
});

test("workspace history keeps conversations when the selected directory no longer exists", async () => {
  const harness = createHarness();
  harness.state.workspaceHistorySelectedPath = "/home/codes/deleted";
  const refreshPromise = harness.context.refreshWorkspaceHistoryConversations();

  harness.requests
    .get("/api/terminal/sessions?path=deleted")
    .reject(new Error("路径不存在: No such file or directory (os error 2)"));
  harness.requests.get("/api/terminal/resume-archives").resolve({ archives: [] });
  harness.requests.get("/api/terminal/codex-conversations?cwd=%2Fhome%2Fcodes%2Fdeleted").resolve({
    conversations: [{ session_id: "conversation-deleted", cwd: "/home/codes/deleted" }],
  });
  await refreshPromise;

  assert.equal(harness.state.workspaceHistoryLoadState, "loaded");
  assert.equal(harness.state.sessions.length, 0);
  assert.deepEqual(
    harness.state.codexConversations.map((conversation) => conversation.session_id),
    ["conversation-deleted"],
  );
});

test("workspace history loads all directories only after an explicit all-workspace search", async () => {
  const harness = createHarness();
  harness.state.workspaceHistorySearchAllWorkspaces = true;
  const refreshPromise = harness.context.refreshWorkspaceHistoryConversations();

  assert.ok(harness.requests.has("/api/terminal/sessions?all=true"));
  assert.ok(harness.requests.has("/api/terminal/codex-conversations"));
  assert.equal(harness.requests.has("/api/terminal/sessions?path=webClx"), false);

  harness.requests.get("/api/terminal/sessions?all=true").resolve({ sessions: [] });
  harness.requests.get("/api/terminal/resume-archives").resolve({ archives: [] });
  harness.requests.get("/api/terminal/codex-conversations").resolve({ conversations: [] });
  await refreshPromise;

  assert.equal(harness.state.workspaceHistoryLoadState, "loaded");
});

test("workspace history labels a completed empty result instead of looking unfinished", () => {
  const harness = createHarness();
  Object.assign(harness.state, {
    workspaceHistory: [],
    workspaceHistoryLoadState: "loaded",
    workspaceHistoryLoadCompleted: 0,
    workspaceHistoryLoadTotal: 0,
    workspaceHistoryLoadError: "",
  });

  harness.renderWorkspaceHistory();

  assert.equal(
    harness.historyList.children[0].children[0].textContent,
    "历史记录已加载，但还没有历史工作区记录或 Codex 对话。",
  );
  assert.deepEqual(harness.statuses.at(-1), {
    message: "加载完成：0 个工作目录，0 条对话。",
    tone: "muted",
  });
});

test("workspace history deletion removes only the matching local conversation state", () => {
  const harness = createHarness();
  harness.state.codexConversations = [
    { session_id: "target-session", cwd: "/home/codes/webClx" },
    { session_id: "kept-session", cwd: "/home/codes/webClx" },
  ];
  harness.state.terminalArchives = [
    { resume_id: "target-session", cwd: "/home/codes/webClx" },
    { resume_id: "kept-session", cwd: "/home/codes/webClx" },
  ];

  harness.context.removeWorkspaceHistoryConversationLocally("target-session");

  assert.deepEqual(
    harness.state.codexConversations.map((conversation) => conversation.session_id),
    ["kept-session"],
  );
  assert.deepEqual(
    harness.state.terminalArchives.map((archive) => archive.resume_id),
    ["kept-session"],
  );
  assert.equal(harness.renders.length, 1, "deletion should rerender once without refetching data");
  assert.equal(harness.requests.size, 0, "local removal should not start any network refresh");
});
