import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const historySource = readFileSync(
  new URL("../static/app-workspace-history.js", import.meta.url),
  "utf8",
);
const navigationSource = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
  "utf8",
);
const workspaceSource = readFileSync(
  new URL("../static/app-workspace-browser.js", import.meta.url),
  "utf8",
);
const appSource = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const terminalRoutesSource = readFileSync(
  new URL("../src/routes/terminal.rs", import.meta.url),
  "utf8",
);
const terminalSource = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalManagerSource = readFileSync(
  new URL("../src/terminal/manager.rs", import.meta.url),
  "utf8",
);

function functionSource(source, name) {
  const asyncStart = source.indexOf(`async function ${name}(`);
  const start = asyncStart >= 0 ? asyncStart : source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const bodyStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

function createSelect(values) {
  const select = {
    children: values.map((value) => ({ value, selected: false })),
    value: values[0] || "",
    get options() {
      return this.children;
    },
    insertBefore(node, before) {
      this.children = this.children.filter((item) => item !== node);
      const index = before ? this.children.indexOf(before) : -1;
      this.children.splice(index >= 0 ? index : this.children.length, 0, node);
      return node;
    },
  };
  return select;
}

test("workspace history search preserves the text being edited", () => {
  const input = { value: "你 ", selectionStart: 2, selectionEnd: 2 };
  const state = {
    workspaceHistorySearchQuery: "",
    workspaceHistorySearchDebounceId: 0,
    workspaceHistorySearchToken: 0,
  };
  const context = vm.createContext({
    state,
    workspaceHistorySearchInputEl: input,
    workspaceHistorySearchSubmitButton: null,
    workspaceHistorySearchClearButton: null,
    renderWorkspaceHistory() {},
    window: {
      clearTimeout() {},
      setTimeout() {
        return 1;
      },
    },
  });
  vm.runInContext(
    functionSource(historySource, "setWorkspaceHistorySearchControlsBusy"),
    context,
  );
  vm.runInContext(
    functionSource(historySource, "scheduleWorkspaceHistorySearch"),
    context,
  );

  context.scheduleWorkspaceHistorySearch(input.value);

  assert.equal(input.value, "你 ");
  assert.equal(input.selectionStart, 2);
  assert.equal(state.workspaceHistorySearchQuery, "你 ");
});

test("history tab click moves the global current directory option to the top", () => {
  const currentPath = "/home/codes/webClx";
  const pathSelect = createSelect([
    "/home/codes/other",
    currentPath,
    "/home/codes/third",
  ]);
  const state = {
    currentWorkspaceDirectoryPath: currentPath,
    workspaceHistorySelectedPath: "/home/codes/other",
  };
  const context = vm.createContext({
    state,
    workspaceHistoryPathSelectEl: pathSelect,
    normalizeAbsolutePath: (path) => String(path || "").replace(/\/$/, "") || "/",
  });
  vm.runInContext(
    functionSource(historySource, "prioritizeWorkspaceHistoryCurrentDirectory"),
    context,
  );

  assert.equal(context.prioritizeWorkspaceHistoryCurrentDirectory(), true);
  assert.deepEqual(
    Array.from(pathSelect.options, (option) => option.value),
    [currentPath, "/home/codes/other", "/home/codes/third"],
  );
  assert.equal(pathSelect.value, currentPath);
  assert.equal(state.workspaceHistorySelectedPath, currentPath);
});

test("ordinary history grouping keeps activity order instead of globally prioritizing current", () => {
  const currentPath = "/home/codes/webClx";
  const state = {
    currentWorkspaceDirectoryPath: currentPath,
    workspaceHistory: [
      { path: currentPath, last_opened_at: 1 },
      { path: "/home/codes/other", last_opened_at: 10 },
    ],
    sessions: [],
    terminalArchives: [],
    codexConversations: [],
  };
  const context = vm.createContext({
    state,
    normalizeAbsolutePath: (path) => String(path || "").replace(/\/$/, "") || "/",
    activeTerminalSessions: () => [],
    sessionWorkspacePath: () => "",
    archiveWorkspacePath: () => "",
    conversationWorkspacePathFromCwd: () => "",
    currentWorkspaceDirectoryPath: () => currentPath,
  });
  vm.runInContext(functionSource(historySource, "workspaceHistoryGroups"), context);

  assert.deepEqual(
    Array.from(context.workspaceHistoryGroups(), (group) => group.path),
    ["/home/codes/other", currentPath],
  );
});

test("history groups include a terminal current directory without changing activity order", () => {
  const currentPath = "/home/codes/new-project";
  const state = {
    currentWorkspaceDirectoryPath: currentPath,
    workspaceHistory: [
      { path: "/home/codes/other", last_opened_at: 10 },
    ],
    sessions: [],
    terminalArchives: [],
    codexConversations: [],
  };
  const context = vm.createContext({
    state,
    normalizeAbsolutePath: (path) => String(path || "").replace(/\/$/, "") || "/",
    activeTerminalSessions: () => [],
    sessionWorkspacePath: () => "",
    archiveWorkspacePath: () => "",
    conversationWorkspacePathFromCwd: () => "",
    currentWorkspaceDirectoryPath: () => currentPath,
  });
  vm.runInContext(functionSource(historySource, "workspaceHistoryGroups"), context);

  assert.deepEqual(
    Array.from(context.workspaceHistoryGroups(), (group) => group.path),
    ["/home/codes/other", currentPath],
  );
});

test("returning from a terminal refreshes the global directory from its live cwd", async () => {
  const events = [];
  const currentPath = "/home/codes/new-project";
  const state = {
    activeTab: "workspace-history",
    returnTerminalSessionId: "session-A",
    currentWorkspaceDirectoryPath: "/home/codes/webClx",
    workspaceHistorySelectedPath: "/home/codes/webClx",
    workspaceHistorySettingsReady: true,
  };
  const context = vm.createContext({
    state,
    normalizeAbsolutePath: (path) => String(path || "").replace(/\/$/, "") || "/",
    async requestJson(url) {
      events.push(["request", url]);
      return { path: "new-project", display_path: currentPath };
    },
    renderWorkspaceHistory() {
      events.push(["render", state.workspaceHistorySelectedPath]);
    },
    prioritizeWorkspaceHistoryCurrentDirectory() {
      events.push(["prioritize", state.workspaceHistorySelectedPath]);
      return true;
    },
    async refreshWorkspaceHistoryConversations() {
      events.push(["refresh", state.workspaceHistorySelectedPath]);
    },
  });
  vm.runInContext(
    functionSource(historySource, "refreshCurrentWorkspaceDirectoryFromTerminal"),
    context,
  );

  assert.equal(await context.refreshCurrentWorkspaceDirectoryFromTerminal(), true);
  assert.equal(state.currentWorkspaceDirectoryPath, currentPath);
  assert.equal(state.workspaceHistorySelectedPath, currentPath);
  assert.deepEqual(events, [
    ["request", "/api/terminal/sessions/session-A/current-directory"],
    ["render", currentPath],
    ["prioritize", currentPath],
    ["refresh", currentPath],
    ["prioritize", currentPath],
  ]);
});

test("entering history uses the global directory and invokes dropdown prioritization", () => {
  const events = [];
  const currentPath = "/home/codes/webClx";
  const state = {
    activeTab: "workspace",
    activeSettingsTab: "system",
    currentWorkspaceDirectoryPath: currentPath,
    workspaceHistorySelectedPath: "/home/codes/other",
    workspaceHistorySettingsReady: true,
  };
  const context = vm.createContext({
    state,
    tabButtons: [],
    workspaceViewEl: null,
    workspaceHistoryViewEl: null,
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
      events.push(["render", state.workspaceHistorySelectedPath]);
    },
    prioritizeWorkspaceHistoryCurrentDirectory() {
      events.push(["prioritize", state.workspaceHistorySelectedPath]);
    },
    refreshWorkspaceHistoryConversations() {
      events.push(["refresh", state.workspaceHistorySelectedPath]);
    },
    ensureAuthPresetsLoaded() {},
    ensureApiPresetsLoaded() {},
    ensureClaudePresetsLoaded() {},
    loadCodexCommonConfig() {},
    syncTabUrl() {},
  });
  vm.runInContext(functionSource(navigationSource, "setActiveTab"), context);

  context.setActiveTab("workspace-history");

  assert.equal(state.workspaceHistorySelectedPath, currentPath);
  assert.deepEqual(events, [
    ["render", currentPath],
    ["prioritize", currentPath],
    ["refresh", currentPath],
  ]);
});

test("successful directory loading updates the global current directory", () => {
  assert.match(
    workspaceSource,
    /state\.currentWorkspaceDirectoryPath\s*=\s*normalizeAbsolutePath\(directory\.display_path\)/,
  );
});

test("workspace startup refreshes the global directory after loading the browser directory", () => {
  assert.match(
    functionSource(appSource, "init"),
    /await loadDirectory\(\);\s*await refreshCurrentWorkspaceDirectoryFromTerminal\(\);/,
  );
});

test("terminal current-directory API reads the live managed session cwd", () => {
  assert.match(
    terminalRoutesSource,
    /\/api\/terminal\/sessions\/\{session_id\}\/current-directory[\s\S]*get\(terminal::current_session_directory\)/,
  );
  assert.match(
    terminalSource,
    /pub async fn current_session_directory\([\s\S]*current_working_directory\(&session_id\)/,
  );
  assert.match(
    terminalManagerSource,
    /fn current_working_directory\([\s\S]*tmux_pane_current_path/,
  );
});
