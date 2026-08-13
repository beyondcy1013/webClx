import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const designateSource = readFileSync(
  new URL("../static/terminal-designate-preset.js", import.meta.url),
  "utf8",
);
const mobileKeysSource = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const sharedSpecifiedPresetSource = readFileSync(
  new URL("../static/specified-preset-actions.js", import.meta.url),
  "utf8",
);

test("project commands expose designated preset fork and terminal launches", () => {
  assert.match(
    terminalHtml,
    /id="terminal-project-command-select"[\s\S]*?<option value="designate_preset_fork">指定预设\+fork（持久切换）<\/option>[\s\S]*?<option value="designate_preset_terminal">指定预设终端<\/option>/,
  );
  assert.match(terminalHtml, /id="terminal-specified-task-dialog"/);
  assert.match(terminalHtml, /id="terminal-specified-task-session-id"/);
  assert.match(terminalHtml, /terminal-designate-preset\.js\?v=/);
  assert.match(
    mobileKeysSource,
    /action === "designate_preset_fork"[\s\S]*?openTerminalDesignatePresetForkDialog[\s\S]*?action === "designate_preset_terminal"[\s\S]*?openTerminalDesignatePresetDialog/,
  );
});

test("fork designation waits for the resume Session to render before opening the dialog", async () => {
  const calls = [];
  const sourceContext = { sessionId: "terminal-source", term: {} };
  let resolveRenderedCommand;
  const renderedCommand = new Promise((resolve) => {
    resolveRenderedCommand = resolve;
  });
  const detected = [{
    resumeId: "019d1ba6-f772-7452-a391-6553ccbc0a50",
    command: "codex resume 019d1ba6-f772-7452-a391-6553ccbc0a50",
    program: "codex",
  }];
  const sandbox = {
    console,
    Date,
    document: { getElementById: () => null },
    state: {
      activeSessionId: "terminal-source",
      currentPath: "webClx",
      sessions: [{ id: "terminal-source", name: "webClx_7", path: "webClx" }],
    },
    sessionPath: (session) => session.path,
    ensureTerminalSessionCache() {
      return { get: (sessionId) => sessionId === sourceContext.sessionId ? sourceContext : null };
    },
    async waitForTerminalToolSessionReady(sessionId) {
      calls.push({ kind: "ready", sessionId });
    },
    readTerminalBufferTailTextFrom() {
      return "terminal buffer before /fork";
    },
    async waitForTerminalToolResumeCommand(context, baselineCommand, timeoutMs, options) {
      calls.push({ kind: "render-wait", context, baselineCommand, timeoutMs, options });
      return renderedCommand;
    },
    parseResumeInputInfo(command) {
      const resumeId = String(command).split(" ").at(-1);
      return { id: resumeId, command: `codex resume ${resumeId}`, program: "codex" };
    },
    specifiedPresetAgent: (program) => program === "claude" ? "claude" : "codex",
    shortResumeId: (resumeId) => String(resumeId).slice(0, 8),
    updateStatus(message, tone) {
      calls.push({ kind: "status", message, tone });
    },
    async detectAgentResumeIdComplete(sessionId, context) {
      calls.push({ kind: "detect", sessionId });
      assert.equal(context, sourceContext);
      return detected.shift();
    },
    async runTerminalSlashCommandByKey(key, options) {
      calls.push({ kind: "shortcut", key, options });
      return true;
    },
    async openTerminalSpecifiedTaskDialog(trigger, options) {
      calls.push({ kind: "dialog", trigger, options });
    },
    window: {
      setTimeout(callback) {
        callback();
        return 1;
      },
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(designateSource, sandbox);
  const opening = sandbox.openTerminalDesignatePresetForkDialog();

  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(
    calls.filter((call) => call.kind === "dialog").length,
    0,
    "the dialog must remain closed while only process-fd detection has changed",
  );

  resolveRenderedCommand("codex resume 019d1ba6-f772-7452-a391-6553ccbc0a50");
  await opening;

  assert.deepEqual(
    JSON.parse(JSON.stringify(
      calls.filter((call) => ["ready", "detect", "shortcut", "render-wait", "dialog"].includes(call.kind)),
    )),
    [
      { kind: "ready", sessionId: "terminal-source" },
      { kind: "detect", sessionId: "terminal-source" },
      {
        kind: "shortcut",
        key: "fork",
        options: { sessionId: "terminal-source" },
      },
      {
        kind: "render-wait",
        context: sourceContext,
        baselineCommand: "codex resume 019d1ba6-f772-7452-a391-6553ccbc0a50",
        timeoutMs: 20000,
        options: {
          allowBaseline: true,
          initialBufferText: "terminal buffer before /fork",
          maxLines: 20,
        },
      },
      {
        kind: "dialog",
        trigger: null,
        options: {
          agent: "codex",
          lockAgent: true,
          mode: "fixed",
          resetTask: true,
          sessionAction: "resume",
          sessionId: "019d1ba6-f772-7452-a391-6553ccbc0a50",
          showSessionField: true,
          sourcePath: "webClx",
          sourceTerminalName: "webClx_7",
          namingAction: "fork",
          title: "指定预设+fork（持久切换）",
        },
      },
    ],
  );
  assert.match(
    designateSource,
    /runTerminalSlashCommandByKey\("fork", \{ sessionId: sourceSessionId \}\)[\s\S]*?waitForTerminalDesignateForkSession\([\s\S]*?sourceContext[\s\S]*?baseline[\s\S]*?openTerminalDesignatePresetDialog\(\{[\s\S]*?sessionId: forked\.resumeId/,
  );
  assert.match(
    designateSource,
    /waitForTerminalToolResumeCommand\([\s\S]*?sourceContext[\s\S]*?baselineCommand[\s\S]*?parseResumeInputInfo\(renderedCommand\)/,
  );
  assert.match(
    designateSource,
    /sourceTerminalName,[\s\S]*?namingAction: "fork"/,
  );
});

test("designated preset terminal lets the dialog derive the current terminal name", async () => {
  const calls = [];
  const sandbox = {
    state: { currentPath: "webClx" },
    specifiedPresetAgent: (program) => program === "claude" ? "claude" : "codex",
    async openTerminalSpecifiedTaskDialog(trigger, options) {
      calls.push({ trigger, options });
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(designateSource, sandbox);

  await sandbox.openTerminalDesignatePresetDialog();

  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    {
      trigger: null,
      options: {
        agent: "codex",
        lockAgent: false,
        mode: "fixed",
        resetTask: true,
        sessionAction: "new",
          sessionId: "",
          showSessionField: true,
          sourcePath: "webClx",
          title: "指定预设终端",
      },
    },
  ]);
});

test("the shared launch helper waits for terminal startup before returning", () => {
  assert.match(
    sharedSpecifiedPresetSource,
    /const launchResult = await launchTerminal\(String\(options\.cwd \|\| ""\), terminalOptions\)/,
  );
  assert.match(
    designateSource,
    /const dialogOptions = \{[\s\S]*?sessionAction: normalizedSessionId \? "resume" : "new"[\s\S]*?sessionId: normalizedSessionId[\s\S]*?showSessionField: true[\s\S]*?openTerminalSpecifiedTaskDialog\(trigger, dialogOptions\)/,
  );
});

test("project commands expose the designated preset resume option", () => {
  assert.match(
    terminalHtml,
    /<option value="designate_preset_fork">指定预设\+fork（持久切换）<\/option>[\s\S]*?<option value="designate_preset_resume">指定预设\+resume（持久切换）<\/option>[\s\S]*?<option value="designate_preset_terminal">指定预设终端<\/option>/,
  );
  assert.match(
    mobileKeysSource,
    /action === "designate_preset_resume"[\s\S]*?openTerminalDesignatePresetResumeDialog/,
  );
});

test("designated preset resume waits for the shared complete Session detector", async () => {
  const calls = [];
  const sourceContext = { sessionId: "terminal-source", term: {} };
  let resolveStatusRender;
  const statusRender = new Promise((resolve) => {
    resolveStatusRender = resolve;
  });
  const sandbox = {
    console,
    Date,
    document: { getElementById: () => null },
    state: {
      activeSessionId: "terminal-source",
      currentPath: "webClx",
      sessions: [{ id: "terminal-source", name: "webClx_9", path: "webClx" }],
    },
    sessionPath: (session) => session.path,
    ensureTerminalSessionCache() {
      return {
        get: (sessionId) => sessionId === sourceContext.sessionId ? sourceContext : null,
      };
    },
    async waitForTerminalToolSessionReady(sessionId) {
      calls.push({ kind: "ready", sessionId });
    },
    specifiedPresetAgent: (program) => (program === "claude" ? "claude" : "codex"),
    shortResumeId: (resumeId) => String(resumeId).slice(0, 8),
    updateStatus(message, tone) {
      calls.push({ kind: "status", message, tone });
    },
    async detectAgentResumeIdComplete(sessionId, context) {
      calls.push({ kind: "detect", sessionId, context });
      return statusRender;
    },
    async openTerminalSpecifiedTaskDialog(trigger, options) {
      calls.push({ kind: "dialog", trigger, options });
    },
    window: {
      setTimeout(callback) {
        callback();
        return 1;
      },
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(designateSource, sandbox);
  const opening = sandbox.openTerminalDesignatePresetResumeDialog();

  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(
    calls.filter((call) => call.kind === "dialog").length,
    0,
    "the dialog must stay closed until /status output renders a Session id",
  );

  resolveStatusRender({
    resumeId: "019d1ba6-f772-7452-a391-6553ccbc0a50",
    command: "codex resume 019d1ba6-f772-7452-a391-6553ccbc0a50",
    program: "codex",
    source: "terminal_status",
  });
  await opening;

  assert.deepEqual(
    JSON.parse(JSON.stringify(
      calls.filter((call) => ["ready", "detect", "dialog"].includes(call.kind)),
    )),
    [
      { kind: "ready", sessionId: "terminal-source" },
      { kind: "detect", sessionId: "terminal-source", context: sourceContext },
      {
        kind: "dialog",
        trigger: null,
        options: {
          agent: "codex",
          lockAgent: true,
          mode: "fixed",
          resetTask: true,
          sessionAction: "resume",
          sessionId: "019d1ba6-f772-7452-a391-6553ccbc0a50",
          showSessionField: true,
          sourcePath: "webClx",
          sourceTerminalName: "webClx_9",
          namingAction: "resume",
          title: "指定预设+resume（持久切换）",
        },
      },
    ],
  );
  assert.match(
    designateSource,
    /detectTerminalDesignateResumeId\(sourceSessionId, sourceContext\)/,
  );
  assert.match(
    designateSource,
    /return detectAgentResumeIdComplete\(sourceSessionId, sourceContext\);/,
  );
});

test("designated preset resume delegates Session detection exactly once", async () => {
  const calls = [];
  const sandbox = {
    state: {
      activeSessionId: "terminal-source",
      currentPath: "webClx",
      sessions: [{ id: "terminal-source", name: "webClx_2", path: "webClx" }],
    },
    sessionPath: (session) => session.path,
    ensureTerminalSessionCache() {
      return { get: () => ({ sessionId: "terminal-source", term: {} }) };
    },
    async waitForTerminalToolSessionReady() {},
    readTerminalBufferTailTextFrom() {
      return "buffer";
    },
    specifiedPresetAgent: () => "codex",
    shortResumeId: (resumeId) => String(resumeId).slice(0, 8),
    updateStatus() {},
    async detectAgentResumeIdComplete() {
      calls.push({ kind: "detect" });
      return {
        resumeId: "019d1ba6-f772-7452-a391-6553ccbc0a50",
        command: "codex resume 019d1ba6-f772-7452-a391-6553ccbc0a50",
        program: "codex",
        source: "process_fd",
      };
    },
    async openTerminalSpecifiedTaskDialog(trigger, options) {
      calls.push({ kind: "dialog", options });
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(designateSource, sandbox);
  await sandbox.openTerminalDesignatePresetResumeDialog();
  const kinds = calls.map((call) => call.kind);
  assert.deepEqual(kinds, ["detect", "dialog"]);
  assert.equal(calls.at(-1).options.namingAction, "resume");
});
