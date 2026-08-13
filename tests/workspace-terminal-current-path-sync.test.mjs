import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const sessionRenderSource = readFileSync(
  new URL("../static/app-home-session-render.js", import.meta.url),
  "utf8",
);
const sessionActionsSource = readFileSync(
  new URL("../static/app-session-actions.js", import.meta.url),
  "utf8",
);
const workspaceBrowserSource = readFileSync(
  new URL("../static/app-workspace-browser.js", import.meta.url),
  "utf8",
);
const coreBindingsSource = readFileSync(
  new URL("../static/app-core-event-bindings.js", import.meta.url),
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

const storedByPath = new Map([["newapi", "newapi-B"]]);
const state = {
  currentPath: "newapi",
  directorySessionId: "",
  preferredSessionId: "other-A",
  returnTerminalSessionId: "other-A",
  sessions: [
    { id: "other-A", path: "webClx" },
    { id: "newapi-A", path: "newapi" },
    { id: "newapi-B", path: "newapi" },
  ],
};
const context = vm.createContext({
  state,
  normalizeRelativePath: (value) => String(value || "").replace(/^\/+|\/+$/g, ""),
  getStoredSessionId: (path) => storedByPath.get(path) || "",
  getStoredGlobalSessionId: () => "other-A",
});
for (const name of [
  "sessionMatchesPath",
  "preferredSessionForCurrentWorkspace",
  "applyCurrentWorkspaceSessionSelection",
]) {
  vm.runInContext(functionSource(sessionRenderSource, name), context);
}

assert.equal(
  context.preferredSessionForCurrentWorkspace(state.sessions)?.id,
  "newapi-B",
  "the terminal-management picker should ignore preferences from another workspace",
);
context.applyCurrentWorkspaceSessionSelection(state.sessions);
assert.equal(state.preferredSessionId, "newapi-B");
assert.equal(state.returnTerminalSessionId, "newapi-B");

state.returnTerminalSessionId = "newapi-A";
assert.equal(
  context.preferredSessionForCurrentWorkspace(state.sessions)?.id,
  "newapi-A",
  "a terminal that opened the current workspace should remain the first matching choice",
);

state.currentPath = "missing";
context.applyCurrentWorkspaceSessionSelection(state.sessions);
assert.equal(state.preferredSessionId, "");
assert.equal(state.returnTerminalSessionId, "");

assert.match(
  sessionActionsSource,
  /applyCurrentWorkspaceSessionSelection\(state\.sessions\)[\s\S]*syncTabUrl\(\)/,
  "session refreshes should reselect and persist the current workspace context in the URL",
);
assert.match(
  workspaceBrowserSource,
  /loadDirectorySessions\(\{ preferredSessionId: state\.returnTerminalSessionId \}\)/,
  "the directory picker should use the same matching terminal preference",
);
assert.match(
  functionSource(coreBindingsSource, "bindCoreEventHandlers"),
  /sessionsSessionListEl\.addEventListener\("change", async \(\) => \{[\s\S]*rememberPreferredSession\(nextSession\.path, nextSession\.id\)[\s\S]*if \(!sessionMatchesPath\(nextSession\)\) \{[\s\S]*await navigateTo\(nextSession\.path\)/,
  "choosing a terminal from terminal management should move the browser to its workspace",
);

console.log("workspace terminal current-path sync tests passed");
