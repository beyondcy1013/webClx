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
  const sandbox = {
    console,
    terminalResumeExtract: {
      resumeCommandFromId(id, program) {
        return program === "claude" ? `claude --resume ${id}` : `codex resume ${id}`;
      },
    },
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
  screenHarness.sandbox.extractLatestResumeInfo = () => ({ id: screenId, program: "codex" });
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
  statusHarness.sandbox.extractLatestResumeInfo = () => ({ id: "", program: "codex" });
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

  const backendId = "019f741e-6bb4-7a03-ac43-80226f0aaced";
  const backendHarness = createHarness("no automatic Session");
  const backendCalls = [];
  backendHarness.sandbox.extractLatestResumeInfo = () => ({ id: "", program: "codex" });
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
