import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const appJs = readEntryScriptBundle("index.html");
const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

function functionSource(source, name) {
  const functionStart = source.indexOf(`function ${name}(`);
  assert.notEqual(functionStart, -1, `missing function ${name}`);
  const bodyStart = source.indexOf("{", functionStart);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(functionStart, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

assert.match(
  appJs,
  /function workspaceHistoryForkCommand\(sessionId\)[\s\S]*?\^\[0-9a-f\][\s\S]*?`codex fork \$\{normalizedSessionId\}`/,
  "workspace history should validate the session UUID before building a native Codex fork command",
);

assert.match(
  appJs,
  /function workspaceHistoryForkTerminalName\(item\)[\s\S]*?activeTerminalName[\s\S]*?`\$\{terminalName\}_fork`/,
  "workspace history should derive the fork terminal name from the historical terminal name",
);

assert.match(
  appJs,
  /function workspaceHistoryForkSupported\(item\)[\s\S]*?item\?\.agentProgram === "codex"/,
  "workspace history should expose fork only for rows identified as Codex conversations",
);

assert.match(
  appJs,
  /function createWorkspaceHistoryForkLink\(item, fallbackPath\)[\s\S]*?createActionLink\([\s\S]*?"fork",[\s\S]*?fresh: true, runCommand: forkCommand[\s\S]*?openFreshTerminalRunLink\(event, workingPath, forkCommand, \{[\s\S]*?terminalName: workspaceHistoryForkTerminalName\(item\)/,
  "workspace history fork links should reuse fresh-session navigation",
);

assert.match(
  appJs,
  /function workspaceHistoryPresetForkCommand\(sessionId, presetId\)[\s\S]*?return forkCommand/,
  "workspace history preset fork should launch native Codex after the shared preset is applied",
);

assert.match(
  appJs,
  /async function launchWorkspaceHistoryPresetFork\(\)[\s\S]*?executeSpecifiedPreset\(\{[\s\S]*?action: "launch"[\s\S]*?presetId: selectedPresetId[\s\S]*?sessionAction: "fork"[\s\S]*?sessionId: target\.item\.sessionId[\s\S]*?sourceTerminalName: target\.item\.activeTerminalName/,
  "workspace history preset fork should pass session and naming inputs through the shared preset action",
);

assert.match(
  appJs,
  /function createWorkspaceHistoryPresetForkButton\(item, fallbackPath\)[\s\S]*?textContent = "模型"[\s\S]*?aria-label", "指定大模型"[\s\S]*?openWorkspaceHistoryPresetForkDialog/,
  "workspace history should expose an accessible model-selection button",
);

assert.match(
  appJs,
  /const forkLink = createWorkspaceHistoryForkLink\(item, selectedPath\);[\s\S]*?actionCell\.appendChild\(forkLink\)[\s\S]*?const presetForkButton = createWorkspaceHistoryPresetForkButton\(item, selectedPath\);[\s\S]*?actionCell\.appendChild\(presetForkButton\)[\s\S]*?actionCell\.appendChild\(createWorkspaceHistoryMoreButton\(item\)\)/,
  "model selection should render beside fork and before the more menu",
);

assert.match(
  appHtml,
  /app-workspace-history\.js\?v=20260801b/,
  "workspace history should load the fork button implementation with a fresh cache key",
);

assert.match(
  appHtml,
  /app-core-event-bindings\.js\?v=20260806a/,
  "workspace history should load the preset dialog bindings with a fresh cache key",
);

assert.match(
  appHtml,
  /id="workspace-history-preset-dialog"[\s\S]*?id="workspace-history-preset-list"[\s\S]*?id="workspace-history-preset-submit"/,
  "workspace history should include the preset selection dialog",
);

{
  const context = vm.createContext({});
  for (const name of [
    "workspaceHistoryForkCommand",
    "workspaceHistoryForkTerminalName",
    "workspaceHistoryPresetForkCommand",
  ]) {
    vm.runInContext(functionSource(appJs, name), context);
  }

  assert.equal(
    vm.runInContext('workspaceHistoryForkCommand(" 019f2350-db5f-7cf0-b476-1cf14855b05d ")', context),
    "codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d",
  );
  assert.equal(vm.runInContext('workspaceHistoryForkCommand("   ")', context), "");
  assert.equal(
    vm.runInContext('workspaceHistoryForkCommand("id; touch /tmp/injected")', context),
    "",
    "non-UUID history metadata must never enter a shell command",
  );
  assert.equal(
    vm.runInContext('workspaceHistoryForkTerminalName({ activeTerminalName: "webClx_15" })', context),
    "webClx_15_fork",
  );
  assert.equal(
    vm.runInContext('workspaceHistoryForkTerminalName({ activeTerminalName: "" })', context),
    "",
    "a history row without a recorded terminal name should keep the fresh automatic name",
  );
  assert.equal(
    vm.runInContext(
      'workspaceHistoryPresetForkCommand("019f2350-db5f-7cf0-b476-1cf14855b05d", "api-primary")',
      context,
    ),
    "codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d",
  );
  assert.equal(
    vm.runInContext(
      'workspaceHistoryPresetForkCommand("019f2350-db5f-7cf0-b476-1cf14855b05d", "api\'quoted")',
      context,
    ),
    "codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d",
    "preset ids are sent through the HTTP apply endpoint and must not enter the shell command",
  );
}

{
  const navigations = [];
  const context = vm.createContext({
    buildTerminalUrl: (path, sessionId, options) => JSON.stringify({ path, sessionId, options }),
    createActionLink: (label, href, className) => ({
      label,
      href,
      className,
      addEventListener(_type, listener) {
        this.listener = listener;
      },
    }),
    openFreshTerminalRunLink: (...args) => navigations.push(args),
    resolveWorkspaceHistoryPath: (path) => `/resolved/${path}`,
  });
  for (const name of [
    "workspaceHistoryForkCommand",
    "workspaceHistoryForkTerminalName",
    "workspaceHistoryForkSupported",
    "createWorkspaceHistoryForkLink",
  ]) {
    vm.runInContext(functionSource(appJs, name), context);
  }

  const link = vm.runInContext(
    `createWorkspaceHistoryForkLink({
      sessionId: "019f2350-db5f-7cf0-b476-1cf14855b05d",
      cwd: "webClx",
      agentProgram: "codex",
      activeTerminalName: "webClx_15"
    }, "fallback")`,
    context,
  );
  assert.equal(link.label, "fork");
  assert.deepEqual(JSON.parse(link.href), {
    path: "/resolved/webClx",
    sessionId: "",
    options: {
      fresh: true,
      runCommand: "codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d",
    },
  });
  const event = { button: 0 };
  link.listener(event);
  assert.equal(navigations.length, 1);
  assert.equal(navigations[0][0], event);
  assert.equal(navigations[0][1], "/resolved/webClx");
  assert.equal(navigations[0][2], "codex fork 019f2350-db5f-7cf0-b476-1cf14855b05d");
  assert.deepEqual(
    JSON.parse(JSON.stringify(navigations[0][3])),
    { terminalName: "webClx_15_fork" },
  );

  assert.equal(
    vm.runInContext(
      'createWorkspaceHistoryForkLink({ sessionId: "invalid; reboot", cwd: "webClx", agentProgram: "codex" }, "fallback")',
      context,
    ),
    null,
  );
  assert.equal(
    vm.runInContext(
      `createWorkspaceHistoryForkLink({
        sessionId: "019f2350-db5f-7cf0-b476-1cf14855b05d",
        cwd: "webClx",
        agentProgram: "claude"
      }, "fallback")`,
      context,
    ),
    null,
    "Claude history rows must not expose a Codex fork command",
  );
}
