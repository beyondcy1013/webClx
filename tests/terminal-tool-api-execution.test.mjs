import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const toolActionsSource = readFileSync(
  new URL("../static/terminal-tool-actions.js", import.meta.url),
  "utf8",
);
const quickStartSource = readFileSync(
  new URL("../static/terminal-command-quickstart.js", import.meta.url),
  "utf8",
);
const mobileKeysSource = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const inputTransportSource = readFileSync(
  new URL("../static/terminal-input-transport.js", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");

function createHarness() {
  const apiCalls = [];
  const bufferReadLimits = [];
  const createCalls = [];
  const renameCalls = [];
  const websocketInputs = [];
  const codexTaskCalls = [];
  const codexTaskResults = [];
  const renderListeners = new Map();
  const createTerminal = (sessionId, text = "") => ({
    sessionId,
    text,
    onRender(callback) {
      const listeners = renderListeners.get(sessionId) || new Set();
      listeners.add(callback);
      renderListeners.set(sessionId, listeners);
      return { dispose: () => listeners.delete(callback) };
    },
  });
  const sourceTerminal = createTerminal(
    "session-old",
    "│  Session:              019d1ba6-f772-7452-a391-6553ccbc0a50              │",
  );
  const newTerminal = createTerminal("session-new");
  const contexts = new Map([
    ["session-old", { sessionId: "session-old", term: sourceTerminal }],
    ["session-new", { sessionId: "session-new", term: newTerminal }],
  ]);
  let replaySettled = false;

  const sandbox = {
    console,
    Date,
    Element: class {},
    terminalImePolicy: {
      terminalImeFunctionAction() {
        return { kind: "none" };
      },
    },
    TERMINAL_TOOL_ROOTS: [{ key: "tools", label: "利器" }],
    TERMINAL_TOOL_ACTION_TYPES: [],
    terminalToolMenuStatusEl: null,
    terminalToolMenuEl: null,
    state: {
      activeSessionId: "session-old",
      currentPath: "webClx",
      sessions: [
        { id: "session-old", name: "webClx_15", path: "webClx" },
      ],
      terminalToolEntries: [],
      terminalSlashCommands: [
        { key: "fork", label: "/fork", action: "send_slash_command", command: "/fork" },
      ],
    },
    async createSession(options = {}) {
      createCalls.push(options);
      sandbox.state.activeSessionId = "session-new";
      const session = { id: "session-new", name: "webClx_16", path: options.path };
      sandbox.state.sessions.push(session);
      return session;
    },
    ensureTerminalSessionCache() {
      return { get: (sessionId) => contexts.get(sessionId) || null };
    },
    terminalContextSocketOpen(candidate) {
      return contexts.get(candidate?.sessionId) === candidate;
    },
    terminalInitialReplaySettled(candidate) {
      return contexts.get(candidate?.sessionId) === candidate && replaySettled;
    },
    isTerminalConnected() {
      return true;
    },
    readTerminalBufferTailTextFrom(terminal, maxLines = 240) {
      bufferReadLimits.push(maxLines);
      return String(terminal?.text || "").split("\n").slice(-maxLines).join("\n");
    },
    extractLatestResumeCommand(text) {
      const matches = Array.from(String(text || "").matchAll(/codex resume ([0-9a-f-]{36})/gi));
      return matches.length ? `codex resume ${matches.at(-1)[1]}` : "";
    },
    extractLatestResumeInfo(text) {
      const matches = Array.from(String(text || "").matchAll(/codex resume ([0-9a-f-]{36})/gi));
      if (matches.length) {
        return { id: matches.at(-1)[1], program: "codex" };
      }
      const banner = Array.from(
        String(text || "").matchAll(/Session:\s*([0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})/gi),
      );
      return banner.length
        ? { id: banner.at(-1)[1].toLowerCase(), program: "codex" }
        : { id: "", program: "codex" };
    },
    sessionPath(session) {
      return session?.path || sandbox.state.currentPath;
    },
    isIdleSession() {
      return false;
    },
    async sendTerminalAutoTypedInput(command, options) {
      apiCalls.push({ command, options });
      return true;
    },
    async executeSpecifiedPreset(options) {
      codexTaskCalls.push(options);
      const record = {
        id: "ct-test",
        mode: options.mode,
        status: "succeeded",
        preset: { id: options.presetId, name: "Test preset", model: "gpt-test" },
        actual_model: "gpt-test",
        result: "任务完成",
        terminal_closed: options.mode === "terminal",
      };
      options.onProgress?.({ ...record, status: "running" });
      return record;
    },
    showTerminalCodexTaskResult(record, options) {
      codexTaskResults.push({ record, options });
    },
    terminalCodexTaskStatusLabel(status) {
      return status;
    },
    async requestJson(url, options) {
      const body = JSON.parse(options.body);
      renameCalls.push({ url, method: options.method, body });
      const sessionId = decodeURIComponent(url.split("/").at(-1));
      const current = sandbox.state.sessions.find((session) => session.id === sessionId);
      return { ...current, name: body.name };
    },
    announceSessionMutation() {},
    sortSessionsByRecentActivity(sessions) {
      return sessions;
    },
    renderSessions() {},
    sendTerminalInput(data) {
      websocketInputs.push(data);
    },
    sendTerminalInputToSession(data, sessionId) {
      websocketInputs.push({ data, sessionId });
      return true;
    },
    focusTerminalAfterSoftKeyboardInput() {},
    cancelNewSessionQuickStart() {},
    window: {
      setTimeout,
      clearTimeout,
      requestAnimationFrame(callback) {
        callback();
      },
      innerHeight: 900,
    },
    document: {
      documentElement: { clientWidth: 1200 },
      createElement() {
        return {};
      },
    },
  };

  vm.createContext(sandbox);
  vm.runInContext(
    `let mobileKeySendQueue = Promise.resolve();
     const MOBILE_SLASH_COMMAND_ENTER_DELAY_MS = 5;
     const MOBILE_SLASH_COMMAND_CONFIRM_DELAY_MS = 1;
     const MOBILE_TEXT_COMMAND_ENTER_DELAY_MS = 5;
     const MOBILE_KEY_SEQUENCES = { enter: "\\r" };`,
    sandbox,
  );
  vm.runInContext(mobileKeysSource, sandbox);
  vm.runInContext(toolActionsSource, sandbox);

  return {
    apiCalls,
    bufferReadLimits,
    codexTaskCalls,
    codexTaskResults,
    createCalls,
    renameCalls,
    sandbox,
    emitSourceText(text) {
      sourceTerminal.text = text;
      for (const listener of renderListeners.get("session-old") || []) {
        listener({ start: 0, end: 1 });
      }
    },
    setReplaySettled(value) {
      replaySettled = Boolean(value);
    },
    websocketInputs,
  };
}

test("new-terminal tool waits for replay settlement before advancing", async () => {
  const harness = createHarness();
  const execution = { sessionId: "session-old" };
  let completed = false;

  const pending = harness.sandbox
    .executeTerminalToolAction({ kind: "create_terminal" }, execution)
    .then(() => {
      completed = true;
    });

  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.equal(completed, false, "WebSocket open alone must not advance the workflow");

  harness.setReplaySettled(true);
  await pending;
  assert.equal(execution.sessionId, "session-new");
});

test("tool commands use the auto-typed API for the stable target session", async () => {
  const harness = createHarness();
  const execution = { sessionId: "session-new" };
  harness.setReplaySettled(true);

  await harness.sandbox.executeTerminalToolAction(
    { kind: "send_command", value: "webclx run api preset-1 -- codex" },
    execution,
  );

  assert.deepEqual(harness.websocketInputs, []);
  assert.deepEqual(JSON.parse(JSON.stringify(harness.apiCalls)), [
    {
      command: "webclx run api preset-1 -- codex",
      options: { sessionId: "session-new", throwOnError: true },
    },
  ]);
});

test("designated preset is deferred and passed to a Codex terminal task", async () => {
  const harness = createHarness();
  const execution = {
    sourcePath: "webClx",
    presetId: "",
    deferPresetApply: true,
  };

  await harness.sandbox.executeTerminalToolAction(
    { kind: "switch_api_preset", value: "preset-grok" },
    execution,
  );
  await harness.sandbox.executeTerminalToolAction(
    { kind: "codex_terminal", value: "检查项目并汇报" },
    execution,
  );

  assert.equal(execution.presetId, "preset-grok");
  assert.deepEqual(harness.renameCalls, [], "deferred selection must not call the apply endpoint");
  assert.equal(harness.codexTaskCalls.length, 1);
  assert.deepEqual(
    {
      mode: harness.codexTaskCalls[0].mode,
      presetId: harness.codexTaskCalls[0].presetId,
      cwd: harness.codexTaskCalls[0].cwd,
      task: harness.codexTaskCalls[0].task,
    },
    {
      mode: "terminal",
      presetId: "preset-grok",
      cwd: "webClx",
      task: "检查项目并汇报",
    },
  );
  assert.equal(harness.codexTaskResults[0].record.result, "任务完成");
});

test("Codex tool action requires an explicit preceding preset selection", async () => {
  const harness = createHarness();
  await assert.rejects(
    harness.sandbox.executeTerminalToolAction(
      { kind: "codex_exec", value: "检查项目" },
      { sourcePath: "webClx", deferPresetApply: true },
    ),
    /指定预设/,
  );
});

test("targeted slash input writes only to the frozen session socket", () => {
  const sourceMessages = [];
  const activeMessages = [];
  const sourceContext = {
    sessionId: "session-old",
    socket: { send: (message) => sourceMessages.push(JSON.parse(message)) },
  };
  const activeContext = {
    sessionId: "session-new",
    socket: { send: (message) => activeMessages.push(JSON.parse(message)) },
  };
  const sandbox = {
    activeTerminalContext: activeContext,
    ensureTerminalSessionCache() {
      return {
        get(sessionId) {
          return sessionId === sourceContext.sessionId ? sourceContext : null;
        },
      };
    },
    terminalContextSocketOpen(context) {
      return context === sourceContext || context === activeContext;
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(inputTransportSource, sandbox);

  assert.equal(sandbox.sendTerminalInputToSession("/fork", "session-old"), true);
  assert.deepEqual(sourceMessages, [{ type: "input", data: "/fork" }]);
  assert.deepEqual(activeMessages, []);
  assert.equal(sandbox.sendTerminalInputToSession("\r", "missing"), false);
});

test("fork tool reads source UUID, creates a terminal, and runs codex fork there", async () => {
  const harness = createHarness();
  harness.setReplaySettled(true);
  const execution = {
    sessionId: "session-old",
    sourceSessionId: "session-old",
    sourceSessionName: "webClx_15",
    sourcePath: "webClx",
  };

  await harness.sandbox.executeTerminalToolAction({ kind: "fork_session" }, execution);

  // 源终端保持不变：不应向源终端发送任何 /fork 或输入。
  assert.deepEqual(JSON.parse(JSON.stringify(harness.websocketInputs)), [],
    "fork must not mutate the source terminal");

  assert.equal(execution.sessionId, "session-new");
  assert.deepEqual(JSON.parse(JSON.stringify(harness.createCalls)), [
    {
      autoSelect: true,
      suppressLoadingStatus: true,
      pushHistoryOnSelect: true,
      throwOnError: true,
      path: "webClx",
      origin: "workflow",
      ownerKey: "",
    },
  ]);
  assert.deepEqual(JSON.parse(JSON.stringify(harness.apiCalls)), [
    {
      command: "codex fork 019d1ba6-f772-7452-a391-6553ccbc0a50",
      options: { sessionId: "session-new", throwOnError: true },
    },
  ]);
  assert.deepEqual(JSON.parse(JSON.stringify(harness.renameCalls)), [
    {
      url: "/api/terminal/sessions/session-new",
      method: "PUT",
      body: { path: "webClx", name: "webClx_15_fork" },
    },
  ]);
});

test("fork resume waiting times out without creating a terminal", async () => {
  const harness = createHarness();
  harness.setReplaySettled(true);
  const sourceContext = harness.sandbox.ensureTerminalSessionCache().get("session-old");

  await assert.rejects(
    harness.sandbox.waitForTerminalToolResumeCommand(
      sourceContext,
      "codex resume 019d1ba6-f772-7452-a391-6553ccbc0a50",
      20,
    ),
    /resume.*超时/i,
  );
  assert.deepEqual(harness.createCalls, []);
});

test("fork resume waiting accepts the rendered baseline Session within the latest 20 lines", async () => {
  const harness = createHarness();
  const sourceContext = harness.sandbox.ensureTerminalSessionCache().get("session-old");
  const baselineCommand = "codex resume 019f971b-6e12-74e0-bb97-73293ed6d4c8";
  const initialBufferText = harness.sandbox.readTerminalBufferTailTextFrom(
    sourceContext.term,
    20,
  );
  const pending = harness.sandbox.waitForTerminalToolResumeCommand(
    sourceContext,
    baselineCommand,
    100,
    {
      allowBaseline: true,
      initialBufferText,
      maxLines: 20,
    },
  );

  harness.emitSourceText([
    "Token usage: total=298,446 input=269,724 output=28,722",
    `To continue this session, run ${baselineCommand}`,
    "MCP client for `openchatcut` failed to start",
    "handshaking with MCP server failed",
    "Send message error Transport",
    "HTTP request failed",
    "error sending request for url",
    "when send initialize request",
    "MCP startup incomplete (failed: openchatcut)",
    "",
    "Improve documentation in @filename",
  ].join("\n"));

  assert.equal(await pending, baselineCommand);
  assert.equal(harness.bufferReadLimits.at(-1), 20);
});

test("terminal page cache-busts every script changed by the API execution fix", () => {
  assert.match(terminalHtml, /terminal-settings\.js\?v=20260812c/);
  assert.match(terminalHtml, /terminal-focus-selection\.js\?v=20260803b/);
  assert.match(terminalHtml, /terminal-input-transport\.js\?v=20260801a/);
  assert.match(terminalHtml, /specified-preset-actions\.js\?v=20260812a/);
  assert.match(terminalHtml, /terminal-command-quickstart\.js\?v=20260803b/);
  assert.match(terminalHtml, /terminal-tools\.js\?v=20260725a/);
  assert.match(terminalHtml, /terminal-specified-task\.js\?v=20260812a/);
  assert.match(terminalHtml, /terminal-tool-actions\.js\?v=20260727i/);
  assert.match(terminalHtml, /terminal-settings-loader\.js\?v=20260803f/);
  assert.match(terminalHtml, /terminal-sessions\.js\?v=20260731a/);
  assert.match(terminalHtml, /terminal-mobile-keys\.js\?v=20260812a/);
  assert.match(terminalHtml, /terminal\.js\?v=20260810a/);
});

test("auto-typed API failures can propagate to the tool workflow", async () => {
  const sandbox = {
    console: { warn() {} },
    state: { activeSessionId: "session-new" },
    normalizeTerminalQuickText(value) {
      return String(value || "").trim();
    },
    async requestJson() {
      throw new Error("API unavailable");
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(quickStartSource, sandbox);

  await assert.rejects(
    sandbox.sendTerminalAutoTypedInput("codex", {
      sessionId: "session-new",
      throwOnError: true,
    }),
    /API unavailable/,
  );
  assert.equal(
    await sandbox.sendTerminalAutoTypedInput("codex", { sessionId: "session-new" }),
    false,
  );
});

test("codex_launch resolves preset and launches with configured fields", async () => {
  const harness = createHarness();
  harness.setReplaySettled(true);

  const launchCalls = [];
  const originalExecuteSpecifiedPreset = harness.sandbox.executeSpecifiedPreset;
  harness.sandbox.specifiedPresetListEndpoint = () => "/api/auth/api-presets";
  harness.sandbox.launchTerminalSpecifiedPreset = async () => ({ id: "session-new", name: "代理设置" });
  harness.sandbox.resolveSpecifiedPreset = (presets, { selector, match }) => {
    const normalized = String(selector || "").trim().toLowerCase();
    const matches = presets.filter((preset) =>
      String(preset?.name || "").trim().toLowerCase().includes(normalized),
    );
    if (matches.length === 0) throw new Error(`没有找到匹配 ${selector} 的 Codex API 预设。`);
    if (matches.length > 1) throw new Error(`找到多个 ${selector} 预设，请保留唯一匹配项。`);
    return matches[0];
  };
  harness.sandbox.requestJson = async (url) => {
    if (url.includes("api-presets")) {
      return { presets: [{ id: "api-1776989731419", name: "MiniMax3" }] };
    }
    return {};
  };
  harness.sandbox.executeSpecifiedPreset = async (options) => {
    launchCalls.push(options);
    return { applied: {}, launchResult: { id: "session-new", name: "代理设置" } };
  };

  const execution = {};
  await harness.sandbox.executeTerminalToolAction({
    kind: "codex_launch",
    value: "$mihomo-proxy-ops 请检查当前代理配置，并根据当前环境完成代理设置。",
    preset_selector: "miniMax",
    preset_match: "unique_contains",
    cwd: "/home/system",
    project_path: "/home/system",
    terminal_name: "代理设置",
    session_action: "new",
  }, execution);

  assert.equal(launchCalls.length, 1);
  const launch = launchCalls[0];
  assert.equal(launch.action, "launch");
  assert.equal(launch.agent, "codex");
  assert.equal(launch.presetId, "api-1776989731419");
  assert.equal(launch.cwd, "/home/system");
  assert.equal(launch.projectPath, "/home/system");
  assert.equal(launch.sessionAction, "new");
  assert.equal(launch.terminalName, "代理设置");
  assert.equal(launch.quickStart, false);
  assert.equal(launch.origin, "agent");
  assert.equal(launch.ownerKey, "");
  assert.equal(
    launch.task,
    "$mihomo-proxy-ops\n\n请仅加载上述技能及必要上下文，然后待命等待用户进一步指令。不要主动检查、修改或执行任何工作。",
  );
  assert.equal(execution.sessionId, "session-new");
});

test("function_command dispatches to runTerminalFunctionCommand by key", async () => {
  const harness = createHarness();
  const executedCommands = [];
  harness.sandbox.runTerminalFunctionCommand = (command, options) => {
    executedCommands.push({ command, options });
    return true;
  };
  harness.sandbox.state.terminalFunctionCommands = [
    { key: "toggle_soft_keyboard", label: "软键盘", action: "toggle_soft_keyboard" },
  ];
  harness.sandbox.state.terminalSlashCommands = [];

  const execution = { sessionId: "session-old" };
  await harness.sandbox.executeTerminalToolAction({
    kind: "function_command",
    value: "toggle_soft_keyboard",
    command_key: "toggle_soft_keyboard",
  }, execution);

  assert.equal(executedCommands.length, 1);
  assert.equal(executedCommands[0].command.key, "toggle_soft_keyboard");
  assert.equal(executedCommands[0].options.sessionId, "session-old");
});

test("function_command throws when command key is not found", async () => {
  const harness = createHarness();
  harness.sandbox.runTerminalFunctionCommand = () => false;
  harness.sandbox.state.terminalFunctionCommands = [];
  harness.sandbox.state.terminalSlashCommands = [];

  await assert.rejects(
    harness.sandbox.executeTerminalToolAction({
      kind: "function_command",
      value: "nonexistent",
      command_key: "nonexistent",
    }, {}),
    /找不到功能命令/,
  );
});

test("run_workflow executes target entry actions inline with cycle detection", async () => {
  const harness = createHarness();
  harness.setReplaySettled(true);
  harness.sandbox.state.terminalToolEntries = [
    {
      id: "inner_wf",
      root_key: "tools",
      parent_id: null,
      kind: "action",
      label: "内部工作流",
      actions: [{ kind: "send_command", value: "echo inner" }],
    },
  ];
  harness.sandbox.apiCalls = [];
  const ctx = { sessionId: "session-new", workflowStack: ["outer_wf"] };
  await harness.sandbox.executeTerminalToolAction(
    { kind: "run_workflow", value: "inner_wf", target_entry_id: "inner_wf" },
    ctx,
  );
  assert.equal(harness.apiCalls.length, 1);
  assert.equal(harness.apiCalls[0].command, "echo inner");
  assert.deepEqual(JSON.parse(JSON.stringify(ctx.workflowStack)), ["outer_wf"]);
  await harness.sandbox.executeTerminalToolAction(
    { kind: "run_workflow", value: "inner_wf", target_entry_id: "inner_wf" },
    ctx,
  );
  assert.equal(harness.apiCalls.length, 2, "a finished nested workflow may run again");
});

test("run_workflow rejects cycles", async () => {
  const harness = createHarness();
  harness.setReplaySettled(true);
  harness.sandbox.state.terminalToolEntries = [
    {
      id: "self_wf",
      root_key: "tools",
      parent_id: null,
      kind: "action",
      label: "自引用",
      actions: [{ kind: "create_terminal", value: "", seconds: 0 }],
    },
  ];
  await assert.rejects(
    harness.sandbox.executeTerminalToolAction(
      { kind: "run_workflow", value: "self_wf", target_entry_id: "self_wf" },
      { workflowStack: ["self_wf"] },
    ),
    /循环/,
  );
});

test("switch_api_preset records a temporary selection without applying shared config", async () => {
  const harness = createHarness();
  const applyCalls = [];
  harness.sandbox.state.apiPresets = [
    { id: "preset-old", name: "OldPreset", active: true },
    { id: "preset-new", name: "NewPreset", active: false },
  ];
  const originalExecuteSpecifiedPreset = harness.sandbox.executeSpecifiedPreset;
  harness.sandbox.executeSpecifiedPreset = async (options) => {
    if (options.action === "apply") {
      applyCalls.push(options);
      return {};
    }
    return originalExecuteSpecifiedPreset(options);
  };

  const execution = { sourcePath: "webClx", presetId: "", deferPresetApply: false };
  await harness.sandbox.executeTerminalToolAction(
    { kind: "switch_api_preset", value: "preset-new" },
    execution,
  );

  assert.equal(execution.previousPresetId, "preset-old");
  assert.equal(execution.presetId, "preset-new");
  assert.equal(applyCalls.length, 0);
});

test("switch_api_preset_revert restores the previous temporary selection", async () => {
  const harness = createHarness();
  const applyCalls = [];
  const originalExecuteSpecifiedPreset = harness.sandbox.executeSpecifiedPreset;
  harness.sandbox.executeSpecifiedPreset = async (options) => {
    if (options.action === "apply") {
      applyCalls.push(options);
      return {};
    }
    return originalExecuteSpecifiedPreset(options);
  };

  const execution = { sourcePath: "webClx", previousPresetId: "preset-old" };
  await harness.sandbox.executeTerminalToolAction(
    { kind: "switch_api_preset_revert", value: "" },
    execution,
  );

  assert.equal(execution.presetId, "preset-old");
  assert.equal(applyCalls.length, 0);
});

test("switch_api_preset_revert throws when no previous preset is recorded", async () => {
  const harness = createHarness();
  await assert.rejects(
    harness.sandbox.executeTerminalToolAction(
      { kind: "switch_api_preset_revert", value: "" },
      {},
    ),
    /没有记录上一次的预设/,
  );
});
