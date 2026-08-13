import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const navigationJs = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
  "utf8",
);
const sessionViewsJs = readFileSync(
  new URL("../static/app-session-views.js", import.meta.url),
  "utf8",
);
const workspaceBrowserJs = readFileSync(
  new URL("../static/app-workspace-browser.js", import.meta.url),
  "utf8",
);
const styles = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");

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

const workspaceStart = indexHtml.indexOf('<main id="workspace-view"');
const workspaceEnd = indexHtml.indexOf("</main>", workspaceStart);
const sessionsPanel = indexHtml.indexOf('<section id="sessions-view"');

assert.ok(workspaceStart >= 0 && workspaceEnd > workspaceStart, "workspace view should exist");
assert.ok(
  sessionsPanel > workspaceStart && sessionsPanel < workspaceEnd,
  "terminal management should be rendered inside the workspace view",
);
assert.match(
  indexHtml.slice(sessionsPanel, workspaceEnd),
  /class="panel sessions-panel workspace-sessions-panel"/,
  "integrated terminal management should use the full-width workspace panel",
);
assert.doesNotMatch(indexHtml, /id="tab-sessions"|data-tab="sessions"/);
assert.doesNotMatch(terminalHtml, /href="\/sessions"|top-nav-sessions/);
assert.doesNotMatch(agentHtml, /href="\/sessions"/);

const initialTabContext = vm.createContext({
  window: { location: { pathname: "/sessions", hash: "" } },
});
vm.runInContext(functionSource(appJs, "getInitialTab"), initialTabContext);
assert.equal(
  vm.runInContext("getInitialTab()", initialTabContext),
  "workspace",
  "legacy /sessions links should open the integrated workspace",
);

const pathnameContext = vm.createContext({
  state: { activeTab: "sessions", activeSettingsTab: "system" },
});
vm.runInContext(functionSource(navigationJs, "currentTabPathname"), pathnameContext);
assert.equal(
  vm.runInContext("currentTabPathname()", pathnameContext),
  "/workspace",
  "removed sessions state should canonicalize to the workspace URL",
);

assert.match(
  functionSource(navigationJs, "setActiveTab"),
  /if \(tab === "sessions"\) \{\s*tab = "workspace";\s*\}[\s\S]*if \(tab === "workspace"\) \{\s*loadSessions\(\);/,
);
assert.match(
  sessionViewsJs,
  /function refreshSessionViews\([\s\S]*state\.activeTab === "workspace"[\s\S]*loadSessions/,
);
assert.doesNotMatch(workspaceBrowserJs, /state\.activeTab === "sessions"/);
assert.match(
  styles,
  /\.workspace-sessions-panel\s*\{[\s\S]*grid-column:\s*1 \/ -1;[\s\S]*min-width:\s*0;/,
);
const responsiveStyles = readFileSync(
  new URL("../static/styles-responsive.css", import.meta.url),
  "utf8",
);
assert.match(
  responsiveStyles,
  /\.workspace-sessions-panel \.toolbar\s*\{[\s\S]*grid-template-areas:[\s\S]*"search search search search"[\s\S]*"refresh create picker open"/,
);
assert.match(
  responsiveStyles,
  /\.workspace-sessions-panel \.toolbar > \.button,[\s\S]*white-space:\s*nowrap;/,
);
assert.match(
  responsiveStyles,
  /\.workspace-sessions-panel #sessions-session-list,[\s\S]*\.workspace-sessions-panel \.toolbar > \.workspace-icon-select[\s\S]*grid-area:\s*picker;/,
);

console.log("workspace terminal integration tests passed");
