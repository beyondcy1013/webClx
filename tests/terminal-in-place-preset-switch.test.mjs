import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const mobileKeysSource = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const switchModuleUrl = new URL(
  "../static/terminal-in-place-preset-switch.js",
  import.meta.url,
);
const switchSource = existsSync(switchModuleUrl) ? readFileSync(switchModuleUrl, "utf8") : "";
const permanentSwitchSource = readFileSync(
  new URL("../static/terminal-permanent-preset-switch.js", import.meta.url),
  "utf8",
);

test("project commands separate permanent switching from temporary preset actions", () => {
  assert.match(
    terminalHtml,
    /id="terminal-project-command-select"[\s\S]*?<option value="permanent_switch_preset">永久切换预设<\/option>/,
  );
  assert.match(
    terminalHtml,
    /id="terminal-project-command-select"[\s\S]*?<option value="switch_preset_in_terminal">原地切换预设\+恢复<\/option>[\s\S]*?<option value="switch_preset_in_terminal_new_session">原地切换预设新会话<\/option>/,
  );
  assert.match(
    mobileKeysSource,
    /action === "permanent_switch_preset"[\s\S]*?openTerminalPermanentPresetSwitchDialog\(\)/,
  );
  assert.match(
    mobileKeysSource,
    /action === "switch_preset_in_terminal"[\s\S]*?sessionAction: "resume"[\s\S]*?action === "switch_preset_in_terminal_new_session"[\s\S]*?sessionAction: "new"/,
  );
  assert.match(
    terminalHtml,
    /terminal-in-place-preset-switch\.js\?v=/,
  );
});

test("the permanent project command is the explicit shared apply path", async () => {
  const calls = [];
  const elements = new Map();
  const element = (id, overrides = {}) => {
    const value = {
      id,
      dataset: {},
      disabled: false,
      hidden: false,
      addEventListener() {},
      ...overrides,
    };
    elements.set(id, value);
    return value;
  };
  element("terminal-permanent-preset-dialog", { open: true, close() { this.open = false; } });
  element("terminal-permanent-preset-form");
  element("terminal-permanent-preset-agent", { value: "codex" });
  element("terminal-permanent-preset-select", { value: "preset-permanent" });
  element("terminal-permanent-preset-path");
  element("terminal-permanent-preset-status");
  element("terminal-permanent-preset-submit");
  element("terminal-permanent-preset-close");
  const sandbox = {
    document: { getElementById: (id) => elements.get(id) || null },
    state: {
      activeSessionId: "terminal-source",
      currentPath: "/fallback",
      sessions: [{ id: "terminal-source", path: "/home/codes/webClx" }],
    },
    sessionPath: (session) => session.path,
    specifiedPresetAgent: (agent) => agent,
    async executeSpecifiedPreset(options) {
      calls.push(options);
      return { name: "Permanent" };
    },
    updateStatus(message, tone) {
      calls.push({ message, tone });
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(permanentSwitchSource, sandbox);

  await sandbox.submitTerminalPermanentPresetSwitch();

  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    {
      action: "apply",
      agent: "codex",
      presetId: "preset-permanent",
      projectPath: "/home/codes/webClx",
    },
    { message: "已永久切换到 Permanent。", tone: "ok" },
  ]);
});

test("the permanent project command keeps deferred switches visible as pending", async () => {
  const calls = [];
  const elements = new Map();
  const element = (id, overrides = {}) => {
    const value = {
      id,
      dataset: {},
      disabled: false,
      hidden: false,
      addEventListener() {},
      ...overrides,
    };
    elements.set(id, value);
    return value;
  };
  const dialog = element("terminal-permanent-preset-dialog", {
    open: true,
    close() { this.open = false; },
  });
  element("terminal-permanent-preset-form");
  element("terminal-permanent-preset-agent", { value: "codex" });
  element("terminal-permanent-preset-select", { value: "preset-pending" });
  element("terminal-permanent-preset-path");
  const status = element("terminal-permanent-preset-status", { textContent: "" });
  element("terminal-permanent-preset-submit");
  element("terminal-permanent-preset-close");
  const sandbox = {
    document: { getElementById: (id) => elements.get(id) || null },
    state: {
      activeSessionId: "terminal-source",
      currentPath: "/fallback",
      sessions: [{ id: "terminal-source", path: "/home/codes/webClx" }],
    },
    sessionPath: (session) => session.path,
    specifiedPresetAgent: (agent) => agent,
    async executeSpecifiedPreset(options) {
      calls.push(options);
      return { name: "Pending", deferred: true };
    },
    updateStatus(message, tone) {
      calls.push({ message, tone });
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(permanentSwitchSource, sandbox);

  await sandbox.submitTerminalPermanentPresetSwitch();

  assert.equal(dialog.open, true, "pending switch must remain visible");
  assert.equal(status.dataset.tone, "info");
  assert.match(status.textContent, /临时切换结束.*后.*永久切换/);
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    {
      action: "apply",
      agent: "codex",
      presetId: "preset-pending",
      projectPath: "/home/codes/webClx",
    },
  ]);
});

test("in-terminal switching exits before resuming through webclx run in the same terminal", async () => {
  assert.ok(switchSource, "the in-terminal preset switch module must exist");
  const calls = [];
  let finishExit;
  const exitQueue = new Promise((resolve) => {
    finishExit = () => {
      calls.push("exit-complete");
      resolve();
    };
  });
  const shellLine = {
    translateToString() {
      return calls.includes("exit-complete") ? "[root@host webClx]#" : "Working";
    },
  };
  const sourceContext = {
    term: {
      buffer: {
        active: {
          baseY: 0,
          cursorY: 0,
          getLine() {
            return shellLine;
          },
        },
      },
      onRender() {
        return { dispose() {} };
      },
    },
  };
  const sandbox = {
    console,
    document: { getElementById: () => null },
    mobileKeySendQueue: Promise.resolve(),
    sendSlashCommand(command, options) {
      calls.push(["exit", command, options]);
      sandbox.mobileKeySendQueue = exitQueue;
      return true;
    },
    specifiedPresetLaunchCommand(options) {
      calls.push(["build-resume", options]);
      return `codex resume ${options.sessionId}`;
    },
    specifiedPresetRunCommand(agent, presetId, command) {
      calls.push(["build-run", agent, presetId, command]);
      return `webclx run api '${presetId}' -- ${command}`;
    },
    async sendTerminalAutoTypedInput(command, options) {
      calls.push(["resume", command, options]);
      return true;
    },
    WebClxTerminalCursorGuard: {
      isLikelyShellPrompt(line) {
        return line.endsWith("#");
      },
    },
    window: {
      setTimeout(callback) {
        return setTimeout(callback, 50);
      },
      clearTimeout,
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(switchSource, sandbox);

  const target = {
    agent: "codex",
    cwd: "/home/codes/webClx",
    resumeId: "019f2350-db5f-7cf0-b476-1cf14855b05d",
    sessionId: "terminal-source",
    sourceContext,
  };
  const switching = sandbox.executeTerminalInPlacePresetSwitch(target, {
    id: "preset-new",
    model: "gpt-5.4",
  });

  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    ["exit", "/exit", { sessionId: "terminal-source" }],
  ]);

  finishExit();
  const command = await switching;
  assert.equal(
    command,
    "webclx run api 'preset-new' -- codex resume 019f2350-db5f-7cf0-b476-1cf14855b05d",
  );
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    ["exit", "/exit", { sessionId: "terminal-source" }],
    "exit-complete",
    [
      "build-resume",
      {
        agent: "codex",
        sessionAction: "resume",
        sessionId: "019f2350-db5f-7cf0-b476-1cf14855b05d",
      },
    ],
    [
      "build-run",
      "codex",
      "preset-new",
      "codex resume 019f2350-db5f-7cf0-b476-1cf14855b05d",
    ],
    [
      "resume",
      "webclx run api 'preset-new' -- codex resume 019f2350-db5f-7cf0-b476-1cf14855b05d",
      { sessionId: "terminal-source", throwOnError: true },
    ],
  ]);
});

test("in-terminal new-session switching exits before launching without resume", async () => {
  const calls = [];
  const target = {
    agent: "codex",
    sessionAction: "new",
    sessionId: "terminal-source",
    sourceContext: { term: {} },
    agentExited: true,
  };
  const sandbox = {
    document: { getElementById: () => null },
    specifiedPresetLaunchCommand(options) {
      calls.push(["build-launch", options]);
      return "codex";
    },
    specifiedPresetRunCommand(agent, presetId, command) {
      calls.push(["build-run", agent, presetId, command]);
      return `webclx run api '${presetId}' -- ${command}`;
    },
    async sendTerminalAutoTypedInput(command, options) {
      calls.push(["launch", command, options]);
      return true;
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(switchSource, sandbox);

  const command = await sandbox.executeTerminalInPlacePresetSwitch(target, "preset-new");

  assert.equal(command, "webclx run api 'preset-new' -- codex");
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    ["build-launch", { agent: "codex", sessionAction: "new", sessionId: "" }],
    ["build-run", "codex", "preset-new", "codex"],
    [
      "launch",
      "webclx run api 'preset-new' -- codex",
      { sessionId: "terminal-source", throwOnError: true },
    ],
  ]);
});

test("in-terminal switching keeps retry state after the agent has exited", async () => {
  assert.match(
    switchSource,
    /if \(!target\.agentExited\)[\s\S]*?target\.agentExited = true;[\s\S]*?specifiedPresetRunCommand/,
  );
  assert.doesNotMatch(switchSource, /preparedPresetId|runtimeRunner|runtimeId/);
});

test("Session detection failure never exits the current agent", async () => {
  assert.ok(switchSource, "the in-terminal preset switch module must exist");
  const calls = [];
  const sourceContext = { term: {} };
  const sandbox = {
    console,
    document: { getElementById: () => null },
    state: {
      activeSessionId: "terminal-source",
      currentPath: "webClx",
      sessions: [{ id: "terminal-source", name: "webClx_1", path: "webClx" }],
    },
    sessionPath: (session) => session.path,
    async waitForTerminalToolSessionReady(sessionId) {
      calls.push(["ready", sessionId]);
    },
    ensureTerminalSessionCache() {
      return { get: () => sourceContext };
    },
    async detectAgentResumeIdComplete(sessionId, context) {
      calls.push(["detect", sessionId, context === sourceContext]);
      return null;
    },
    sendSlashCommand() {
      calls.push(["exit"]);
      return true;
    },
    updateStatus(message, tone) {
      calls.push(["status", message, tone]);
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(switchSource, sandbox);

  await sandbox.openTerminalInPlacePresetSwitchDialog();

  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    ["status", "正在读取当前 Session…", "info"],
    ["ready", "terminal-source"],
    ["detect", "terminal-source", true],
    ["status", "无法提取当前 Session，未退出当前会话。", "warn"],
  ]);
});
