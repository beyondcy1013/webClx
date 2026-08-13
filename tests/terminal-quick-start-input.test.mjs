import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const quickStartSource = readFileSync(
  new URL("../static/terminal-command-quickstart.js", import.meta.url),
  "utf8",
);
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");

function createQuickStartHarness({ useRealQuickStart = false } = {}) {
  const forwarded = [];
  const started = [];
  const automaticRequests = [];
  const timers = new Map();
  let nextTimerId = 1;
  let now = 1_000;
  let connected = true;
  let replaySettled = true;
  let visibilityState = "visible";

  const sandbox = {
    console,
    NEW_SESSION_QUICK_START_TIMEOUT_MS: 3000,
    Date: {
      now() {
        return now;
      },
    },
    document: {
      get visibilityState() {
        return visibilityState;
      },
    },
    state: {
      activeSessionId: "session-1",
      terminalQuickCommands: [
        { key: "1", label: "codex", command: "codex" },
        { key: "2", label: "claude", command: "claude" },
      ],
      terminalQuickStartDefaultKey: "1",
    },
    pendingNewSessionQuickStart: {
      sessionId: "session-1",
      timer: null,
      choiceKey: "",
      inputTimer: null,
      inputBuffer: "",
    },
    normalizeTerminalQuickText(value) {
      return String(value || "").trim();
    },
    sendTerminalInput(data) {
      forwarded.push(data);
    },
    async requestJson(url, options = {}) {
      if (url === "/api/terminal/auto-typed-input") {
        const body = JSON.parse(options.body);
        automaticRequests.push({
          commandLine: body.command_line,
          sessionId: body.session_id,
        });
        return { data: `${body.command_line}\n` };
      }
      throw new Error(`unexpected request: ${url}`);
    },
    focusTerminalSoon() {},
    updateStatus() {},
    terminalInitialReplaySettled() {
      return replaySettled;
    },
    isTerminalConnected() {
      return connected;
    },
    window: {
      setTimeout(callback, delay = 0) {
        const id = nextTimerId;
        nextTimerId += 1;
        timers.set(id, { callback, delay });
        return id;
      },
      clearTimeout(id) {
        timers.delete(id);
      },
    },
  };

  vm.createContext(sandbox);
  vm.runInContext(quickStartSource, sandbox);
  if (!useRealQuickStart) {
    sandbox.runNewSessionQuickStart = (key) => {
      started.push(String(key));
      sandbox.pendingNewSessionQuickStart = null;
      return true;
    };
  }

  return {
    advanceTime(ms) {
      now += Number(ms) || 0;
    },
    automaticRequests,
    dispatch(data) {
      if (!sandbox.maybeHandleNewSessionQuickStartInput(data)) {
        sandbox.sendTerminalInput(data);
      }
    },
    async fireOnlyTimer() {
      assert.equal(timers.size, 1, "one quick-key confirmation timer should be pending");
      const [[id, timer]] = timers;
      timers.delete(id);
      now += timer.delay;
      timer.callback();
      await Promise.resolve();
      await Promise.resolve();
    },
    forwarded,
    sandbox,
    setConnected(value) {
      connected = Boolean(value);
    },
    setReplaySettled(value) {
      replaySettled = Boolean(value);
    },
    setVisibility(value) {
      visibilityState = String(value || "visible");
    },
    started,
    timers,
  };
}

test("manual input keeps the quick-key prefix when more text follows", () => {
  const harness = createQuickStartHarness();

  harness.dispatch("1");
  harness.dispatch("echo ready");

  assert.deepEqual(harness.started, []);
  assert.deepEqual(harness.forwarded, ["1echo ready"]);
  assert.equal(harness.timers.size, 0);
});

test("a lone quick key launches only after the short confirmation window", async () => {
  const harness = createQuickStartHarness();

  harness.dispatch("2");
  assert.deepEqual(harness.started, []);
  assert.deepEqual(harness.forwarded, []);

  await harness.fireOnlyTimer();
  assert.deepEqual(harness.started, ["2"]);
});

test("a non-quick first character is forwarded without delay", () => {
  const harness = createQuickStartHarness();

  harness.dispatch("p");

  assert.deepEqual(harness.started, []);
  assert.deepEqual(harness.forwarded, ["p"]);
  assert.equal(harness.timers.size, 0);
});

test("a hidden new terminal keeps the configured countdown before starting the default", async () => {
  const harness = createQuickStartHarness({ useRealQuickStart: true });
  harness.setVisibility("hidden");

  harness.sandbox.armNewSessionQuickStart("session-1");

  assert.deepEqual(harness.automaticRequests, []);
  assert.equal(harness.timers.size, 1);
  assert.equal([...harness.timers.values()][0].delay, 3000);

  await harness.fireOnlyTimer();
  assert.deepEqual(harness.automaticRequests, [
    { commandLine: "codex", sessionId: "session-1" },
  ]);
});

test("connection and replay activation do not reset the quick-start deadline", () => {
  const harness = createQuickStartHarness({ useRealQuickStart: true });

  harness.sandbox.armNewSessionQuickStart("session-1");
  const [timerId] = harness.timers.keys();
  harness.advanceTime(1000);
  harness.sandbox.activateNewSessionQuickStart();

  assert.deepEqual([...harness.timers.keys()], [timerId]);
  assert.equal([...harness.timers.values()][0].delay, 3000);
});

test("the countdown starts before connection and replay settle", async () => {
  const harness = createQuickStartHarness({ useRealQuickStart: true });
  harness.setConnected(false);
  harness.setReplaySettled(false);

  harness.sandbox.armNewSessionQuickStart("session-1");

  assert.equal(harness.timers.size, 1);
  await harness.fireOnlyTimer();
  assert.deepEqual(harness.automaticRequests, [
    { commandLine: "codex", sessionId: "session-1" },
  ]);
});

test("manual input typed during initial replay is restored and sent as one command", () => {
  const harness = createQuickStartHarness();
  harness.setReplaySettled(false);

  harness.dispatch("p");
  harness.dispatch("rintf ready\\r");
  assert.deepEqual(harness.forwarded, []);

  harness.setReplaySettled(true);
  harness.sandbox.activateNewSessionQuickStart();
  assert.deepEqual(harness.started, []);
  assert.deepEqual(harness.forwarded, ["printf ready\\r"]);
});

test("terminal page loads the quick-start fix with a fresh asset version", () => {
  assert.match(terminalHtml, /terminal-command-quickstart\.js\?v=20260803b/);
});
