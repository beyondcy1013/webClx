import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const navigationSource = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
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

function createHarness(activeTab = "workspace") {
  const renderedSelections = [];
  const state = {
    activeTab,
    activeSettingsTab: "system",
    currentWorkspaceDirectoryPath: "/home/codes/webClx",
    workspaceDir: "/home/codes",
    workspaceHistorySelectedPath: "/home/codes/other",
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
    normalizeAbsolutePath: (path) => String(path || "").replace(/\/$/, "") || "/",
    resolveAbsolutePath: (basePath, relativePath) => {
      return `${String(basePath).replace(/\/$/, "")}/${String(relativePath).replace(/^\//, "")}`
        .replace(/\/$/, "");
    },
    setTabPanelActive() {},
    setActiveSettingsTab() {},
    loadSessions() {},
    loadTerminalArchives() {},
    renderWorkspaceHistory() {
      renderedSelections.push(state.workspaceHistorySelectedPath);
    },
    prioritizeWorkspaceHistoryCurrentDirectory() {},
    refreshWorkspaceHistoryConversations() {},
    ensureAuthPresetsLoaded() {},
    ensureApiPresetsLoaded() {},
    ensureClaudePresetsLoaded() {},
    loadCodexCommonConfig() {},
    syncTabUrl() {},
  });

  vm.runInContext(functionSource(navigationSource, "setActiveTab"), context);
  return { context, renderedSelections, state };
}

test("entering workspace history locates the browser current directory", () => {
  const harness = createHarness("workspace");

  harness.context.setActiveTab("workspace-history");

  assert.equal(harness.state.workspaceHistorySelectedPath, "/home/codes/webClx");
  assert.deepEqual(harness.renderedSelections, ["/home/codes/webClx"]);
});

test("rerendering the active history tab preserves its manual directory selection", () => {
  const harness = createHarness("workspace-history");

  harness.context.setActiveTab("workspace-history");

  assert.equal(harness.state.workspaceHistorySelectedPath, "/home/codes/other");
  assert.deepEqual(harness.renderedSelections, ["/home/codes/other"]);
});
