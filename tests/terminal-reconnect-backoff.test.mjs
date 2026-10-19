import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const connectionSource = readFileSync(
  new URL("../static/terminal-layout-connection.js", import.meta.url),
  "utf8",
);
const outputSource = readFileSync(
  new URL("../static/terminal-output-scroll.js", import.meta.url),
  "utf8",
);

function createSandbox() {
  const timers = new Map();
  let nextTimerId = 1;
  const contexts = new Map();
  const connects = [];
  const documentListeners = new Map();
  const windowListeners = new Map();

  const addListener = (map) => (type, callback) => {
    if (!map.has(type)) map.set(type, []);
    map.get(type).push(callback);
  };
  const emitDocument = (type) =>
    (documentListeners.get(type) || []).forEach((callback) => callback());
  const emitWindow = (type) =>
    (windowListeners.get(type) || []).forEach((callback) => callback());

  const documentStub = {
    visibilityState: "visible",
    addEventListener: addListener(documentListeners),
  };
  const sandbox = {
    console,
    document: documentStub,
    navigator: { onLine: true },
    window: {
      setTimeout(callback, delay) {
        const id = nextTimerId++;
        timers.set(id, { callback, delay });
        return id;
      },
      clearTimeout(id) {
        timers.delete(id);
      },
      addEventListener: addListener(windowListeners),
    },
    state: {},
    WebSocket: { CONNECTING: 0, OPEN: 1 },
    activeTerminalContext: null,
    TERMINAL_SERVER_PROBE: undefined,
    syncActiveTerminalContextAliases() {},
    updateStatus() {},
    loadSessions() {},
    fetch() {
      return Promise.reject(new Error("no network in tests"));
    },
    redirectToLogin() {},
    forEachTerminalSessionContext(callback) {
      contexts.forEach(callback);
    },
    // Replaced below; the real function is only invoked via the reconnect timer.
    connectTerminal(context) {
      connects.push(context.sessionId);
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(connectionSource, sandbox);
  sandbox.connectTerminal = (context) => {
    connects.push(context.sessionId);
  };

  let pendingContext = null;
  const originalSetTimeout = sandbox.window.setTimeout;
  sandbox.window.setTimeout = (callback, delay) => {
    const id = originalSetTimeout(callback, delay);
    timers.get(id).context = pendingContext;
    return id;
  };

  return {
    sandbox,
    timers,
    contexts,
    connects,
    documentStub,
    emitDocument,
    emitWindow,
    schedule(context, options) {
      pendingContext = context;
      try {
        sandbox.scheduleReconnect(context, options);
      } finally {
        pendingContext = null;
      }
      const entry = [...timers.values()].find((timer) => timer.context === context);
      return entry ? entry.delay : null;
    },
    fire(context) {
      const entry = [...timers.entries()].find(([, timer]) => timer.context === context);
      assert.ok(entry, "expected a scheduled reconnect timer");
      timers.delete(entry[0]);
      entry[1].callback();
    },
  };
}

function makeContext(sessionId, extra = {}) {
  return {
    sessionId,
    disposed: false,
    disconnected: true,
    reconnectTimer: null,
    reconnectAttempts: 0,
    reconnectDeferred: false,
    ...extra,
  };
}

test("per-context backoff grows exponentially and is capped at 30s", () => {
  const env = createSandbox();
  const context = makeContext("active-session");
  env.sandbox.activeTerminalContext = context;
  env.contexts.set("active-session", context);

  const delays = [];
  for (let index = 0; index < 12; index += 1) {
    const delay = env.schedule(context);
    assert.notEqual(delay, null);
    delays.push(delay);
    env.fire(context);
    assert.deepEqual(env.connects, Array(index + 1).fill("active-session"));
  }
  const jitter = delays[0] - 1500;
  assert.ok(jitter >= 0 && jitter < 1200, "active-context jitter stays deterministic");
  assert.equal(delays[1], 3000 + jitter);
  assert.equal(delays[2], 6000 + jitter);
  assert.ok(delays.at(-1) <= 30000, "delay is capped at 30s");
  assert.ok(delays.at(-1) === delays.at(-2), "the cap is stable");
  assert.ok(
    env.contexts.get("active-session").reconnectAttempts > 10,
    "attempts keep counting past the cap",
  );
});

test("background contexts have an independent attempt counter and base", () => {
  const env = createSandbox();
  const active = makeContext("a");
  const background = makeContext("b");
  env.sandbox.activeTerminalContext = active;
  env.contexts.set("a", active);
  env.contexts.set("b", background);

  const activeDelay = env.schedule(active);
  const backgroundDelay = env.schedule(background);
  env.fire(active);
  env.fire(background);
  assert.ok(backgroundDelay >= 1800, "inactive contexts keep the larger base");
  assert.ok(activeDelay >= 1500 && activeDelay < 1800 + 1200);
  // Second failure doubles the base for each context independently while the
  // deterministic jitter stays the same.
  assert.equal(env.schedule(background), backgroundDelay + 1800);
  assert.equal(env.schedule(active), activeDelay + 1500);
  assert.equal(background.reconnectAttempts, 2);
  assert.equal(active.reconnectAttempts, 2);
});

test("hidden pages and offline devices defer instead of retrying", () => {
  const env = createSandbox();
  const context = makeContext("c");
  env.sandbox.activeTerminalContext = context;
  env.contexts.set("c", context);

  env.documentStub.visibilityState = "hidden";
  assert.equal(env.schedule(context), null);
  assert.equal(context.reconnectDeferred, true);
  assert.equal(context.reconnectAttempts, 0, "a deferred attempt must not consume backoff");

  env.documentStub.visibilityState = "visible";
  env.sandbox.navigator.onLine = false;
  context.reconnectDeferred = false;
  assert.equal(env.schedule(context), null);
  assert.equal(context.reconnectDeferred, true);

  // Even the "immediate" flag cannot connect while offline.
  assert.equal(env.schedule(context, { immediate: true }), null);
  assert.equal(env.connects.length, 0);

  // Coming back online reconnects the deferred context immediately.
  env.sandbox.navigator.onLine = true;
  env.emitWindow("online");
  const entry = [...env.timers.values()].at(-1);
  assert.ok(entry, "online must reschedule the deferred context");
  assert.equal(entry.delay, 0, "the active deferred context reconnects immediately");
  entry.context = context;
  env.fire(context);
  assert.deepEqual(env.connects, ["c"]);
});

test("a timer armed while visible rechecks the environment when it fires", () => {
  const env = createSandbox();
  const context = makeContext("sleepy");
  env.sandbox.activeTerminalContext = context;
  env.contexts.set("sleepy", context);

  const delay = env.schedule(context);
  assert.notEqual(delay, null);
  env.documentStub.visibilityState = "hidden";
  // The visibility listener cancels sleeping timers and marks the context
  // deferred without consuming another attempt.
  env.emitDocument("visibilitychange");
  assert.equal(context.reconnectTimer, null);
  assert.equal(context.reconnectDeferred, true);
  assert.equal(env.timers.size, 0);
  assert.equal(env.connects.length, 0);

  env.documentStub.visibilityState = "visible";
  env.emitDocument("visibilitychange");
  const entry = [...env.timers.values()].at(-1);
  assert.ok(entry, "returning to foreground reschedules the context");
  assert.equal(entry.delay, 0);
  entry.context = context;
  env.fire(context);
  assert.deepEqual(env.connects, ["sleepy"]);
});

test("foreground resumes cancel a sleeping long-delay timer and retry now", () => {
  const env = createSandbox();
  const context = makeContext("long-wait");
  env.sandbox.activeTerminalContext = context;
  env.contexts.set("long-wait", context);

  // Burn attempts until the delay is large.
  env.schedule(context);
  env.fire(context);
  env.schedule(context);
  env.fire(context);
  env.schedule(context);
  env.fire(context);
  const longDelay = env.schedule(context);
  assert.ok(longDelay > 5000, `expected a large pending delay, got ${longDelay}`);

  // A foreground-visible signal must cancel the 30s wait, not skip it.
  env.emitDocument("visibilitychange");
  const entry = [...env.timers.values()].at(-1);
  assert.ok(entry, "a visible resume re-arms the reconnect");
  assert.equal(entry.delay, 0, "the active context retries immediately");
  entry.context = context;
  env.fire(context);
  assert.equal(env.connects.length, 4);
});

test("a disposed context never schedules a reconnect", () => {
  const env = createSandbox();
  const context = makeContext("dead", { disposed: true });
  env.sandbox.activeTerminalContext = context;
  env.contexts.set("dead", context);
  assert.equal(env.schedule(context), null);
  assert.equal(env.timers.size, 0);
});

test("backoff resets only after a completed replay, not on socket open", () => {
  const openHandler = connectionSource.slice(
    connectionSource.indexOf('contextSocket.addEventListener("open"'),
    connectionSource.indexOf('contextSocket.addEventListener("message"'),
  );
  assert.doesNotMatch(
    openHandler,
    /resetTerminalReconnectFailures|reconnectAttempts = 0/,
    "a bare socket open must not reset reconnect backoff",
  );
});

test("the production endTerminalBacklogReplay resets only on clean completion", () => {
  const resets = [];
  const sandbox = {
    console,
    window: {
      setTimeout() { return 0; },
      clearTimeout() {},
      requestAnimationFrame() {},
    },
    document: { visibilityState: "visible" },
    terminalHost: null,
    activeSessionPageScrollRestore: null,
    activeTerminalContext: null,
    terminalInitialReplayPending: false,
    terminalBacklogReplayActive: false,
    terminalBacklogReplayEndQueued: false,
    terminalBacklogReplayInterrupted: false,
    terminalOutputQueue: [],
    terminalOutputWriteInFlight: false,
    terminalOutputWriteId: 0,
    terminalOutputTail: "",
    terminalInputTail: "",
    pendingDeviceAttributeResponses: 0,
    state: {},
    resetTerminalReconnectFailures() {
      resets.push(true);
    },
    completeTerminalContextReconnect() {},
    syncActiveTerminalContextAliases() {},
    maybeRunTerminalStartupActions() {},
    hideTerminalSwitchPlaceholder() {},
    restoreSessionPageScrollIfActive() {},
    scheduleSessionPageScrollRestore() {},
    syncTerminalCursorCorrection() {},
    restoreTerminalScrollPositionForSession() {},
    scheduleTerminalBottomAnchorAfterReplay() {},
    scheduleTerminalRenderRefresh() {},
    updateTerminalScrollBottomButton() {},
    hideTerminalCursorCorrection() {},
    clearTerminalOutputDrainTimer() {},
    drainTerminalOutputQueue() {},
    scrollTerminalToBottom() {},
    scheduleSessionPageScrollRestoreFallback() {},
  };
  const fnStart = outputSource.indexOf("function endTerminalBacklogReplay(");
  const fnEnd = outputSource.indexOf("function scheduleTerminalBottomAnchorAfterReplay(", fnStart);
  assert.ok(fnStart >= 0 && fnEnd > fnStart);
  vm.createContext(sandbox);
  vm.runInContext(outputSource.slice(fnStart, fnEnd), sandbox);

  // Clean completion of an idle replay resets the backoff.
  const clean = { reconnectAttempts: 7, disposed: false, backlogReplayActive: false };
  sandbox.endTerminalBacklogReplay({}, clean);
  assert.equal(clean.reconnectAttempts, 0, "clean replay end resets the counter");
  assert.equal(resets.length, 1, "clean replay end resets the global counter");

  // A preserved-interruption end is not proof of a healthy connection.
  const interrupted = {
    reconnectAttempts: 5,
    disposed: false,
    backlogReplayActive: false,
    backlogReplayInterrupted: true,
  };
  sandbox.endTerminalBacklogReplay({ preserveInterrupted: true }, interrupted);
  assert.equal(interrupted.reconnectAttempts, 5, "interrupted replay keeps the backoff");
  assert.equal(resets.length, 1, "interrupted replay does not reset the global counter");

  // An uninterrupted full replay also resets.
  const full = {
    reconnectAttempts: 3,
    disposed: false,
    backlogReplayActive: true,
    backlogReplayEndQueued: false,
    backlogReplayInterrupted: false,
    outputStreamGap: true,
    sessionId: "s",
    followOutput: false,
  };
  sandbox.activeTerminalContext = full;
  sandbox.endTerminalBacklogReplay({}, full);
  assert.equal(full.reconnectAttempts, 0, "uninterrupted replay resets the counter");
  assert.equal(resets.length, 2);
});
