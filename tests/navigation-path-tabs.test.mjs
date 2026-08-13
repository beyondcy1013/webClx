import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalNavigationJs = readFileSync(
  new URL("../static/terminal-navigation-layout.js", import.meta.url),
  "utf8",
);
const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const appNavigationJs = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
  "utf8",
);
const settingsCategoriesJs = readFileSync(
  new URL("../static/app-settings-categories.js", import.meta.url),
  "utf8",
);

function functionSource(source, name) {
  const start = source.indexOf(`function ${name}(`);
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

const terminalContext = vm.createContext({
  URLSearchParams,
  state: { currentPath: "webClx" },
  normalizeTerminalPath: (value) => String(value || ""),
});
vm.runInContext(functionSource(terminalNavigationJs, "buildWorkspaceUrl"), terminalContext);

assert.equal(
  vm.runInContext('buildWorkspaceUrl("/codex_api")', terminalContext),
  "/codex_api?path=webClx",
  "terminal Codex_API navigation should preserve the workspace path without falling back to /",
);
assert.match(
  terminalHtml,
  /id="top-nav-api"[^>]+href="\/codex_api"[^>]+data-home-path="\/codex_api"/,
  "terminal Codex_API link should expose its path route to the navigation synchronizer",
);
for (const [id, path] of [
  ["workspace", "/workspace"],
  ["workspace-history", "/workspace_history"],
  ["auth", "/codex_oauth"],
  ["claude", "/claude_api"],
  ["settings", "/settings"],
]) {
  assert.match(
    terminalHtml,
    new RegExp(`id="top-nav-${id}"[^>]+href="${path}"[^>]+data-home-path="${path}"`),
    `terminal ${id} link should use its semantic path`,
  );
}
for (const path of ["/workspace", "/codex_api", "/claude_api", "/settings", "/desktop"]) {
  assert.match(agentHtml, new RegExp(`href="${path}"`), `Agent navigation should link to ${path}`);
}
for (const [name, html] of [
  ["main", appHtml],
  ["terminal", terminalHtml],
  ["agent", agentHtml],
]) {
  assert.match(
    html,
    /href="\/downloads"[^>]*>\s*编译产物\s*<\/a>/,
    `${name} navigation should link to the artifact downloads child page`,
  );
}
assert.match(
  agentHtml,
  /href="\/workspace_history"[^>]*>历史工作区<\/a>/,
  "Agent navigation should expose the same workspace history entry as other top-level pages",
);
assert.match(
  terminalJs,
  /document\.querySelectorAll\("\[data-home-path\]"\)/,
  "terminal navigation should select the same data-home-path attribute used by the template",
);
assert.doesNotMatch(
  terminalNavigationJs,
  /homeHash|data-home-hash/,
  "terminal navigation should not read the removed hash-route attribute",
);
assert.match(
  terminalHtml,
  /terminal-navigation-layout\.js\?v=20260803b/,
  "terminal navigation changes should use a fresh asset version",
);

const appContext = vm.createContext({
  state: {
    activeTab: "api",
    activeSettingsTab: "system",
  },
});
vm.runInContext(functionSource(appNavigationJs, "currentTabPathname"), appContext);

assert.equal(
  vm.runInContext("currentTabPathname()", appContext),
  "/codex_api",
  "Codex_API tab state should map to the /codex_api path",
);
assert.equal(
  vm.runInContext(
    'state.activeTab = "settings"; state.activeSettingsTab = "system"; currentTabPathname()',
    appContext,
  ),
  "/settings",
  "the default settings tab should use the /settings root path",
);
assert.equal(
  vm.runInContext(
    'state.activeTab = "settings"; state.activeSettingsTab = "proxy"; currentTabPathname()',
    appContext,
  ),
  "/settings/proxy",
  "non-default settings tabs should remain nested below /settings",
);
assert.equal(
  vm.runInContext('state.activeTab = "workspace"; currentTabPathname()', appContext),
  "/workspace",
  "workspace tab state should map to the /workspace path",
);
for (const [tab, path] of [
  ["workspace-history", "/workspace_history"],
  ["auth", "/codex_oauth"],
  ["claude", "/claude_api"],
]) {
  assert.equal(
    vm.runInContext(`state.activeTab = "${tab}"; currentTabPathname()`, appContext),
    path,
    `${tab} tab state should map to ${path}`,
  );
}
assert.match(
  appJs,
  /path\.slice\("\/settings\/"\.length\)/,
  "settings deep links should remove the complete leading path before normalization",
);
assert.match(
  appHtml,
  /app-navigation-tabs\.js\?v=20260725e/,
  "app navigation changes should use a fresh asset version",
);

function initialTab(pathname) {
  const context = vm.createContext({
    window: {
      location: {
        pathname,
        hash: "",
      },
    },
  });
  vm.runInContext(functionSource(appJs, "getInitialTab"), context);
  return vm.runInContext("getInitialTab()", context);
}

function initialSettingsTab(pathname, hash = "") {
  const context = vm.createContext({
    window: {
      location: {
        pathname,
        hash,
      },
    },
  });
  vm.runInContext(settingsCategoriesJs, context);
  vm.runInContext(functionSource(appJs, "getInitialSettingsTab"), context);
  return vm.runInContext("getInitialSettingsTab()", context);
}

assert.equal(initialTab("/codex_api"), "api", "new Codex_API paths should open the API tab");
assert.equal(initialTab("/api"), "api", "legacy Codex_API paths should remain compatible");
assert.equal(initialTab("/workspace"), "workspace", "workspace paths should open the workspace tab");
assert.equal(
  initialTab("/workspace_history"),
  "workspace-history",
  "workspace history paths should open the history tab",
);
assert.equal(initialTab("/codex_oauth"), "auth", "Codex OAuth paths should open the OAuth tab");
assert.equal(initialTab("/claude_api"), "claude", "Claude API paths should open the Claude tab");

for (const tab of [
  "system",
  "terminal",
  "soft-keyboard",
  "shortcuts",
  "appearance",
  "auto-continue-tasks",
  "compile",
  "model",
  "agent",
  "config-files",
  "proxy",
  "frpc",
  "frps",
  "preset-sync",
  "update",
]) {
  assert.equal(
    initialSettingsTab(`/settings/${tab}`),
    tab,
    `refreshing /settings/${tab} should preserve the selected settings page`,
  );
}
assert.equal(
  initialSettingsTab("/settings", "#settings/compile"),
  "compile",
  "legacy settings hashes should keep selecting their settings page",
);

const settingsContext = vm.createContext({});
vm.runInContext(settingsCategoriesJs, settingsContext);
assert.equal(
  vm.runInContext('normalizeSettingsTab("workspace")', settingsContext),
  "system",
  "legacy settings workspace paths should open the system category",
);
assert.equal(
  vm.runInContext('normalizeSettingsTab("terminal")', settingsContext),
  "terminal",
  "the terminal settings path should remain a first-class leaf route",
);
