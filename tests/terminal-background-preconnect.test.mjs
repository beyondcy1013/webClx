import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const preconnectSource = readFileSync(
  new URL("../static/terminal-preconnect.js", import.meta.url),
  "utf8",
);
const sessionsSource = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);
const terminalSource = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");

function createSandbox({ rendererType = "canvas" } = {}) {
  let nextTimerId = 1;
  const timers = new Map();
  const contexts = new Map();
  contexts.set("A", {
    sessionId: "A",
    socket: { readyState: 1 },
    disposed: false,
  });
  const connected = [];
  const state = {
    activeSessionId: "A",
    sessions: [
      { id: "A", idle: false },
      { id: "B", idle: false },
      { id: "C", idle: false },
      { id: "D", idle: false },
      { id: "E", idle: false },
      { id: "F", idle: false },
      { id: "idle", idle: true },
    ],
  };
  const sandbox = {
    console,
    document: { visibilityState: "visible" },
    state,
    WebSocket: { CONNECTING: 0, OPEN: 1 },
    window: {
      setTimeout(callback, delay) {
        const id = nextTimerId++;
        timers.set(id, { callback, delay });
        return id;
      },
      clearTimeout(id) {
        timers.delete(id);
      },
    },
    ensureTerminalSessionCache() {
      return {
        get(sessionId) {
          return contexts.get(sessionId) || null;
        },
      };
    },
    getOrCreateBackgroundTerminalContext(sessionId) {
      if (!contexts.has(sessionId)) {
        contexts.set(sessionId, {
          sessionId,
          socket: null,
          disposed: false,
        });
      }
      return contexts.get(sessionId);
    },
    isIdleSession(sessionId) {
      return Boolean(state.sessions.find((session) => session.id === sessionId)?.idle);
    },
    terminalContextSocketOpen(context) {
      return context?.socket?.readyState === 1;
    },
    terminalContextSocketConnecting(context) {
      return context?.socket?.readyState === 0;
    },
    connectTerminal(context) {
      connected.push(context.sessionId);
      context.socket = { readyState: 1 };
    },
    terminalRendererType() {
      return rendererType;
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(preconnectSource, sandbox);

  function runTimersByDelay(delay) {
    const due = [...timers.entries()].filter(([, timer]) => timer.delay === delay);
    for (const [id, timer] of due) {
      timers.delete(id);
      timer.callback();
    }
  }

  return { connected, contexts, runTimersByDelay, sandbox, state, timers };
}

const first = createSandbox();
first.sandbox.runTerminalBackgroundPreconnect();
assert.deepEqual(
  [...first.timers.values()].map((timer) => timer.delay),
  [0, 300, 600, 900],
  "background websocket handshakes should be staggered",
);
for (const delay of [0, 300, 600, 900]) {
  first.runTimersByDelay(delay);
}
assert.deepEqual(first.connected, ["B", "C", "D", "E"]);
assert.equal(first.state.activeSessionId, "A", "preconnect must not change the foreground session");
assert.ok(!first.contexts.has("F"), "automatic preconnect must keep a real four-session cap");
assert.ok(!first.contexts.has("idle"), "idle terminal sessions must never be preconnected");

first.sandbox.runTerminalBackgroundPreconnect();
assert.equal(first.timers.size, 0, "a full preconnect pool must not schedule extra sessions");

first.state.activeSessionId = "B";
first.sandbox.runTerminalBackgroundPreconnect();
assert.deepEqual(
  [...first.timers.values()].map((timer) => timer.delay),
  [0],
  "moving a preconnected session to foreground should free one automatic preconnect slot",
);
first.runTimersByDelay(0);
assert.deepEqual(first.connected, ["B", "C", "D", "E", "F"]);

const stale = createSandbox();
stale.sandbox.runTerminalBackgroundPreconnect();
stale.state.sessions.find((session) => session.id === "B").idle = true;
stale.runTimersByDelay(0);
assert.ok(!stale.connected.includes("B"), "delayed work must recheck idle state before connecting");

const hidden = createSandbox();
hidden.sandbox.runTerminalBackgroundPreconnect();
hidden.sandbox.document.visibilityState = "hidden";
hidden.runTimersByDelay(0);
assert.ok(!hidden.connected.includes("B"), "a hidden page must not start delayed preconnect work");

const android = createSandbox({ rendererType: "dom" });
android.sandbox.runTerminalBackgroundPreconnect();
assert.equal(android.timers.size, 0, "Android must not auto-preconnect background sessions");
assert.equal(android.connected.length, 0, "Android creates no background sockets automatically");
android.sandbox.scheduleTerminalBackgroundPreconnect();
assert.equal(android.timers.size, 0, "Android hover/selector warmup is a no-op");
assert.match(
  preconnectSource,
  /terminalRendererType\(\) !== "dom"/,
  "the Android dom renderer disables automatic background preconnect",
);

const scheduled = createSandbox();
scheduled.sandbox.scheduleTerminalBackgroundPreconnect();
scheduled.sandbox.scheduleTerminalBackgroundPreconnect();
assert.deepEqual(
  [...scheduled.timers.values()].map((timer) => timer.delay),
  [50],
  "repeated load and hover triggers should coalesce into one scheduler timer",
);
scheduled.sandbox.cancelTerminalBackgroundPreconnect();
assert.equal(scheduled.timers.size, 0, "page teardown should cancel pending preconnect work");

assert.doesNotMatch(
  sessionsSource,
  /selectSession\(targetSession\.id,[\s\S]*scheduleTerminalBackgroundPreconnect\(\)/,
  "session list refreshes and tab navigation must not warm background sockets without user intent",
);
assert.match(
  terminalSource,
  /function bindSessionActivitySafeSelect\(selectEl\)[\s\S]*pointerenter[\s\S]*scheduleTerminalBackgroundPreconnect/,
  "hovering a session selector should warm likely switch targets",
);
assert.match(
  terminalSource,
  /function markSessionDropdownInteracting\(\)[\s\S]*scheduleTerminalBackgroundPreconnect\(\)/,
  "opening or focusing a session selector should warm likely switch targets",
);
assert.match(
  terminalSource,
  /beforeunload[\s\S]*cancelTerminalBackgroundPreconnect\(\)/,
  "page teardown should cancel delayed background connection work",
);
assert.ok(
  terminalHtml.indexOf("/assets/terminal-layout-connection.js") <
    terminalHtml.indexOf("/assets/terminal-preconnect.js"),
  "preconnect should load after its websocket helpers",
);
assert.ok(
  terminalHtml.indexOf("/assets/terminal-preconnect.js") <
    terminalHtml.indexOf("/assets/terminal.js"),
  "preconnect should load before terminal event bindings",
);

console.log("terminal background preconnect tests passed");
