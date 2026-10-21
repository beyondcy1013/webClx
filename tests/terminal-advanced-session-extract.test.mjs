import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/terminal-resume-agent.js", import.meta.url),
  "utf8",
);

function createHarness(bufferText) {
  const context = { sessionId: "terminal-1", term: {} };
  const terminalResumeExtract = {
    resumeCommandFromId(id, program) {
      return program === "claude" ? `claude --resume ${id}` : `codex resume ${id}`;
    },
    extractLatestResumeInfo: () => ({ id: "", program: "codex" }),
  };
  const sandbox = {
    console,
    AbortSignal: { timeout: () => undefined },
    fetch: async (url) => { throw new Error("unexpected global fetch"); },
    requestJson: async () => ({}),
    runTerminalSlashCommandByKey: async () => true,
    waitForTerminalToolResumeCommand: async () => "",
    terminalResumeExtract,
    ensureTerminalSessionCache() {
      return { get: () => context };
    },
    readTerminalBufferTailTextFrom() {
      return bufferText;
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  return { context, sandbox };
}

test("complete Session detection uses screen, status, then backend fallback", async () => {
  const screenId = "019d1ba6-f772-7452-a391-6553ccbc0a50";
  const screenHarness = createHarness("screen-hit");
  screenHarness.sandbox.terminalResumeExtract.extractLatestResumeInfo = () => ({
    id: screenId,
    program: "codex",
  });
  screenHarness.sandbox.probeTerminalStatusResumeId = async () => {
    throw new Error("status must not run after a screen hit");
  };
  screenHarness.sandbox.detectAgentResumeIdForSession = async () => {
    throw new Error("backend must not run after a screen hit");
  };
  const screenDetected = await screenHarness.sandbox.detectAgentResumeIdComplete(
    "terminal-1",
    screenHarness.context,
  );
  assert.equal(screenDetected.resumeId, screenId);
  assert.equal(screenDetected.source, "terminal_buffer");

  const statusId = "019d2091-73ef-7522-a073-e5a4b8195fe7";
  const statusHarness = createHarness("no screen Session");
  const statusCalls = [];
  const statusAgentChecks = [];
  statusHarness.sandbox.fetch = async (url) => {
    statusAgentChecks.push(url);
    return { ok: true, json: async () => ({ resume_id: statusId }) };
  };
  statusHarness.sandbox.probeTerminalStatusResumeId = async (context, initialText) => {
    statusCalls.push({ context, initialText });
    return {
      resumeId: statusId,
      command: `codex resume ${statusId}`,
      program: "codex",
      source: "terminal_status",
    };
  };
  statusHarness.sandbox.detectAgentResumeIdForSession = async () => {
    throw new Error("backend must not run after a status hit");
  };
  const statusDetected = await statusHarness.sandbox.detectAgentResumeIdComplete(
    "terminal-1",
    statusHarness.context,
  );
  assert.equal(statusDetected.resumeId, statusId);
  assert.equal(statusDetected.source, "terminal_status");
  assert.deepEqual(statusCalls, [{ context: statusHarness.context, initialText: "no screen Session" }]);
  assert.deepEqual(await statusHarness.sandbox.terminalStatusAgentAvailable("terminal-1"), true);
  assert.deepEqual(statusAgentChecks, [
    "/api/terminal/sessions/terminal-1/agent-session",
  ]);

  const backendId = "019f741e-6bb4-7a03-ac43-80226f0aaced";
  const backendHarness = createHarness("no automatic Session");
  const backendCalls = [];
  backendHarness.sandbox.terminalStatusProbeAllowed = async () => false;
  backendHarness.sandbox.fetch = async () => {
    throw new Error("status must not run without a live agent process");
  };
  backendHarness.sandbox.probeTerminalStatusResumeId = async () => null;
  backendHarness.sandbox.detectAgentResumeIdForSession = async (sessionId, options) => {
    backendCalls.push({ sessionId, options });
    return {
      resumeId: backendId,
      command: `codex resume ${backendId}`,
      program: "codex",
      source: "codex_history_screen_match",
    };
  };
  const backendDetected = await backendHarness.sandbox.detectAgentResumeIdComplete(
    "terminal-1",
    backendHarness.context,
  );
  assert.equal(backendDetected.resumeId, backendId);
  assert.deepEqual(
    JSON.parse(JSON.stringify(backendCalls)),
    [{ sessionId: "terminal-1", options: { complete: true } }],
  );
});

test("status probe gate allows an attached Codex/Claude even without a known resume id", async () => {
  const harness = createHarness("no screen Session");

  // Codex 在回合间关闭 rollout FD：轻量端点拿不到 resume_id，但进程还挂着，
  // /status 探测必须照常进行。
  harness.sandbox.fetch = async () => ({
    ok: true,
    json: async () => ({ resume_id: null, agents: ["Codex"] }),
  });
  assert.deepEqual(await harness.sandbox.terminalStatusAgentAvailable("terminal-1"), true);

  harness.sandbox.fetch = async () => ({
    ok: true,
    json: async () => ({ resume_id: null, agents: ["Claude"] }),
  });
  assert.deepEqual(await harness.sandbox.terminalStatusAgentAvailable("terminal-1"), true);

  // 非 Codex/Claude 的 agent（如 CodeBuddy）没有 /status 命令，不注入。
  harness.sandbox.fetch = async () => ({
    ok: true,
    json: async () => ({ resume_id: null, agents: ["CodeBuddy"] }),
  });
  assert.deepEqual(await harness.sandbox.terminalStatusAgentAvailable("terminal-1"), false);

  // 空面板（纯 shell 提示符）不允许探测。
  harness.sandbox.fetch = async () => ({
    ok: true,
    json: async () => ({ resume_id: null, agents: [] }),
  });
  assert.deepEqual(await harness.sandbox.terminalStatusAgentAvailable("terminal-1"), false);

  // 旧后端未返回 agents 字段时，行为退回 resume_id 判断。
  harness.sandbox.fetch = async () => ({
    ok: true,
    json: async () => ({ resume_id: null }),
  });
  assert.deepEqual(await harness.sandbox.terminalStatusAgentAvailable("terminal-1"), false);
});
