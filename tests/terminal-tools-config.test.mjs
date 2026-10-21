import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const terminalSettingsJs = readFileSync(
  new URL("../static/terminal-settings.js", import.meta.url),
  "utf8",
);
const context = vm.createContext({ globalThis: {} });
vm.runInContext(terminalSettingsJs, context);

const settings = context.globalThis.WebClxTerminalSettings;
assert.ok(settings, "shared terminal settings should initialize");
assert.deepEqual(
  Array.from(settings.TERMINAL_TOOL_ROOTS, (item) => [item.key, item.label]),
  [["tools", "工作流"]],
  "the compatible root registry should expose the workflow key",
);

const configuredEntries = [
  {
    id: "existing",
    root_key: "tools",
    parent_id: null,
    kind: "action",
    label: "现有利器",
    sort_order: 10,
    actions: [{ kind: "send_command", value: "codex" }],
  },
];
const entriesWithBuiltins = settings.ensureBuiltInTerminalToolEntries(configuredEntries);
assert.deepEqual(
  JSON.parse(JSON.stringify(Array.from(entriesWithBuiltins, (entry) => [
    entry.id,
    entry.label,
    entry.actions.map((action) => action.kind),
  ]))),
  [
    ["existing", "现有利器", ["send_command"]],
    ["fork_session", "fork", ["fork_session"]],
    ["proxy_settings_workflow", "代理设置", ["codex_launch"]],
  ],
  "saved tool entries should be preserved and receive built-in proxy and fork workflows",
);
assert.equal(
  settings.ensureBuiltInTerminalToolEntries(entriesWithBuiltins)
    .filter((entry) => entry.id === "fork_session").length,
  1,
  "the built-in fork button should be deduplicated by its stable id",
);

const mergedForkSlashCommand = settings.ensureBuiltInTerminalSlashCommands([
  { key: "fork", label: "旧 fork", action: "send_text", command: "/status" },
]).find((command) => command.key === "fork");
assert.deepEqual(
  { ...mergedForkSlashCommand },
  { key: "fork", label: "/fork", action: "send_slash_command", command: "/fork", shortcut: "" },
  "the built-in fork slash command should replace an incompatible saved command",
);

const normalized = settings.normalizeTerminalToolEntries([
  {
    id: " folder ",
    root_key: "tools",
    parent_id: null,
    kind: "folder",
    label: " 常用 ",
    sort_order: 20,
    actions: [],
  },
  {
    id: "workflow",
    root_key: "tools",
    parent_id: "folder",
    kind: "action",
    label: "启动",
    sort_order: 10,
    actions: [
      { kind: "switch_api_preset", value: " preset-a " },
      { kind: "wait", seconds: 1.5 },
      { kind: "create_terminal", value: "ignored" },
      { kind: "codex_terminal", value: " 检查项目并汇报 " },
    ],
  },
]);

assert.equal(normalized.length, 2);
assert.equal(normalized[0].id, "folder");
assert.equal(normalized[1].parent_id, "folder");
assert.equal(normalized[1].actions[0].value, "preset-a");
assert.equal(normalized[1].actions[2].value, "");
assert.equal(normalized[1].actions[3].value, "检查项目并汇报");

const normalizedFork = settings.normalizeTerminalToolEntries([
  {
    id: "fork_session",
    root_key: "tools",
    parent_id: null,
    kind: "action",
    label: "fork",
    sort_order: 30,
    actions: [{ kind: "fork_session", value: "ignored", seconds: 0 }],
  },
]);
assert.equal(normalizedFork.length, 1);
assert.deepEqual(
  Array.from(normalizedFork[0].actions, (action) => ({ ...action })),
  [{ kind: "fork_session", value: "", seconds: 0 }],
  "fork_session should be a typed action with no user-supplied parameter",
);

assert.equal(
  settings.normalizeTerminalToolEntries([
    { id: "one", root_key: "tools", parent_id: "two", kind: "folder", label: "一" },
    { id: "two", root_key: "tools", parent_id: "one", kind: "folder", label: "二" },
  ]).length,
  0,
  "cyclic trees should be rejected as a whole",
);

// --- codex_launch action contract ---

const proxyWorkflowRaw = {
  id: "proxy_settings_workflow",
  root_key: "tools",
  parent_id: null,
  kind: "action",
  label: "代理设置",
  sort_order: 20,
  actions: [{
    kind: "codex_launch",
    value: "$mihomo-proxy-ops 请检查当前代理配置，并根据当前环境完成代理设置。",
    preset_selector: "miniMax",
    preset_match: "unique_contains",
    cwd: "/home/system",
    project_path: "/home/system",
    terminal_name: "代理设置",
    session_action: "new",
  }],
};

const normalizedProxy = settings.normalizeTerminalToolEntries([proxyWorkflowRaw]);
assert.equal(normalizedProxy.length, 1, "codex_launch should be accepted");
assert.equal(normalizedProxy[0].id, "proxy_settings_workflow");
const proxyAction = normalizedProxy[0].actions[0];
assert.equal(proxyAction.kind, "codex_launch");
assert.equal(proxyAction.preset_selector, "miniMax");
assert.equal(proxyAction.preset_match, "unique_contains");
assert.equal(proxyAction.cwd, "/home/system");
assert.equal(proxyAction.project_path, "/home/system");
assert.equal(proxyAction.terminal_name, "代理设置");
assert.equal(proxyAction.session_action, "new");

// Empty required fields should reject.
assert.equal(
  settings.normalizeTerminalToolEntries([{
    ...proxyWorkflowRaw,
    actions: [{ ...proxyWorkflowRaw.actions[0], preset_selector: "  " }],
  }]).length,
  0,
  "empty preset_selector should be rejected",
);
assert.equal(
  settings.normalizeTerminalToolEntries([{
    ...proxyWorkflowRaw,
    actions: [{ ...proxyWorkflowRaw.actions[0], preset_match: "bogus" }],
  }]).length,
  0,
  "invalid preset_match should be rejected",
);
assert.equal(
  settings.normalizeTerminalToolEntries([{
    ...proxyWorkflowRaw,
    actions: [{ ...proxyWorkflowRaw.actions[0], session_action: "bogus" }],
  }]).length,
  0,
  "invalid session_action should be rejected",
);
assert.equal(
  settings.normalizeTerminalToolEntries([{
    ...proxyWorkflowRaw,
    actions: [{ ...proxyWorkflowRaw.actions[0], value: "  " }],
  }]).length,
  0,
  "empty task value should be rejected",
);
assert.equal(
  settings.normalizeTerminalToolEntries([{
    ...proxyWorkflowRaw,
    actions: [{ ...proxyWorkflowRaw.actions[0], cwd: "home/system" }],
  }]).length,
  0,
  "relative cwd should be rejected",
);

// Built-in proxy_settings_workflow should be present and deduplicated.
const entriesWithProxyBuiltIn = settings.ensureBuiltInTerminalToolEntries([{
  id: "other",
  root_key: "tools",
  parent_id: null,
  kind: "action",
  label: "其他",
  sort_order: 10,
  actions: [{ kind: "create_terminal", value: "", seconds: 0 }],
}]);
const proxyBuiltIn = entriesWithBuiltins.find((entry) => entry.id === "proxy_settings_workflow");
assert.ok(proxyBuiltIn, "built-in proxy_settings_workflow should be present");
assert.equal(proxyBuiltIn.actions[0].kind, "codex_launch");
assert.equal(proxyBuiltIn.actions[0].preset_selector, "miniMax");
assert.equal(proxyBuiltIn.actions[0].cwd, "/home/system");
assert.equal(proxyBuiltIn.actions[0].project_path, "/home/system");
assert.equal(proxyBuiltIn.actions[0].session_action, "new");
assert.equal(
  settings.ensureBuiltInTerminalToolEntries([proxyWorkflowRaw])
    .filter((entry) => entry.id === "proxy_settings_workflow").length,
  1,
  "built-in proxy workflow should be deduplicated by stable id",
);

// --- function_command action contract ---

const functionCommandEntries = settings.normalizeTerminalToolEntries([{
  id: "test_function",
  root_key: "tools",
  parent_id: null,
  kind: "action",
  label: "测试功能命令",
  sort_order: 10,
  actions: [{ kind: "function_command", value: "toggle_soft_keyboard" }],
}]);
assert.equal(functionCommandEntries.length, 1);
assert.equal(functionCommandEntries[0].actions[0].kind, "function_command");
assert.equal(functionCommandEntries[0].actions[0].command_key, "toggle_soft_keyboard");

assert.equal(
  settings.normalizeTerminalToolEntries([{
    id: "bad_fc",
    root_key: "tools",
    parent_id: null,
    kind: "action",
    label: "无效功能",
    sort_order: 10,
    actions: [{ kind: "function_command", value: "  " }],
  }]).length,
  0,
  "empty function_command key should be rejected",
);

// --- run_workflow action contract ---

const runWorkflowEntries = settings.normalizeTerminalToolEntries([
  {
    id: "target_wf",
    root_key: "tools",
    parent_id: null,
    kind: "action",
    label: "目标工作流",
    sort_order: 10,
    actions: [{ kind: "create_terminal", value: "", seconds: 0 }],
  },
  {
    id: "caller_wf",
    root_key: "tools",
    parent_id: null,
    kind: "action",
    label: "调用工作流",
    sort_order: 20,
    actions: [{ kind: "run_workflow", value: "target_wf" }],
  },
]);
assert.equal(runWorkflowEntries.length, 2);
assert.equal(runWorkflowEntries[1].actions[0].kind, "run_workflow");
assert.equal(runWorkflowEntries[1].actions[0].target_entry_id, "target_wf");

assert.equal(
  settings.normalizeTerminalToolEntries([{
    id: "bad_rw",
    root_key: "tools",
    parent_id: null,
    kind: "action",
    label: "无效嵌套",
    sort_order: 10,
    actions: [{ kind: "run_workflow", value: "bad id with spaces" }],
  }]).length,
  0,
  "invalid target_entry_id should be rejected",
);


// --- switch_api_preset_revert action contract ---

const revertEntries = settings.normalizeTerminalToolEntries([{
  id: "test_revert",
  root_key: "tools",
  parent_id: null,
  kind: "action",
  label: "回切预设",
  sort_order: 10,
  actions: [
    { kind: "switch_api_preset", value: "preset-new" },
    { kind: "switch_api_preset_revert", value: "" },
  ],
}]);
assert.equal(revertEntries.length, 1);
assert.equal(revertEntries[0].actions[1].kind, "switch_api_preset_revert");
assert.equal(revertEntries[0].actions[1].value, "");
