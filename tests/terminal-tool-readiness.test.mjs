import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { test } from 'node:test';

function fn(file, name) {
  const source = readFileSync(new URL(`../static/${file}`, import.meta.url), 'utf8');
  const start = source.search(new RegExp(`(?:async )?function ${name}\\(`));
  assert.ok(start >= 0);
  const end = source.indexOf('\nfunction ', start + 1);
  const asyncEnd = source.indexOf('\nasync function ', start + 1);
  return source.slice(start, Math.min(...[end, asyncEnd, source.length].filter(n => n > start)));
}
function harness({ requestJson = null, state = null } = {}) {
  const context = {
    connectionToken: 1, outputWriteId: 0, outputQueue: [],
    outputWriteInFlight: false, initialReplayPending: false,
    backlogReplayActive: false, backlogReplayEndQueued: false,
    socket: { readyState: 1 },
  };
  let now = 0;
  const callbacks = [];
  context.term = { write: (_, cb) => callbacks.push(cb) };
  const sandbox = {
    activeTerminalContext: null, Date: { now: () => now },
    TERMINAL_TOOL_SESSION_READY_TIMEOUT_MS: 30000,
    TERMINAL_TOOL_SESSION_LIVE_PROBE_INTERVAL_MS: 3000,
    TERMINAL_TOOL_SESSION_LIVE_PROBE_GRACE_MS: 5000,
    ensureTerminalSessionCache: () => new Map([['target', context]]),
    terminalContextSocketOpen: c => c.socket?.readyState === 1,
    waitForTerminalToolDelay: async ms => { now += ms; },
    clearTerminalOutputDrainTimer() {}, syncActiveTerminalContextAliases() {},
    scheduleTerminalRenderRefresh() {}, scheduleTerminalOutputDrain() {},
    terminalShouldStickToBottomForOutput: () => false,
    mergeQueuedTerminalOutputItem: item => item,
    endTerminalBacklogReplay: (_, c) => { c.backlogReplayActive = false; c.backlogReplayEndQueued = false; },
    handleTerminalBacklogReplayControl: () => false,
    resetTerminalBacklogReplay() {}, resetTerminalOutputQueue() {},
    terminalScrollPositions: new Map(),
  };
  if (typeof requestJson === "function") sandbox.requestJson = requestJson;
  if (state) sandbox.state = state;
  for (const [file, name] of [
    ['terminal-input-transport.js', 'terminalInitialReplaySettled'],
    ['terminal-tool-actions.js', 'probeTerminalToolSessionMissing'],
    ['terminal-tool-actions.js', 'waitForTerminalToolSessionReady'],
    ['terminal-output-scroll.js', 'followBackgroundTerminalOutput'],
    ['terminal-output-scroll.js', 'drainTerminalOutputQueue'],
    ['terminal-layout-connection.js', 'handleServerControlMessage'],
  ]) vm.runInNewContext(fn(file, name), sandbox);
  return { context, sandbox, callbacks, elapsed: () => now };
}

test('busy live write does not starve workflow after ordered replay completion', async () => {
  const { context: c, sandbox: s, callbacks, elapsed } = harness();
  c.backlogReplayActive = true;
  c.backlogReplayEndQueued = true;
  c.outputQueue = [
    { kind: 'output', replay: true, token: 1, bytes: new Uint8Array([65]) },
    { kind: 'backlog_replay_end', token: 1 },
    { kind: 'output', replay: false, token: 1, bytes: new Uint8Array([66]) },
  ];
  s.drainTerminalOutputQueue(c);
  assert.equal(s.terminalInitialReplaySettled(c), false);
  callbacks.shift()(); // replay callback drains end marker, then starts live write
  assert.equal(c.outputWriteInFlight, true);
  assert.equal(c.outputReplayWriteInFlight, false);
  await s.waitForTerminalToolSessionReady('target');
  assert.equal(elapsed(), 0);
});

test('pending, queued and interrupted in-flight replay still gate readiness', () => {
  const { context: c, sandbox: s } = harness();
  for (const field of ['initialReplayPending', 'backlogReplayActive', 'backlogReplayEndQueued', 'outputReplayWriteInFlight']) {
    c[field] = true;
    assert.equal(s.terminalInitialReplaySettled(c), false, field);
    c[field] = false;
  }
  c.outputQueue = [{ replay: true }];
  assert.equal(s.terminalInitialReplaySettled(c), false);
  c.outputQueue = [{ kind: 'backlog_replay_end' }];
  assert.equal(s.terminalInitialReplaySettled(c), false);
});

test('stale write callback cannot settle a new connection replay', () => {
  const { context: c, sandbox: s, callbacks } = harness();
  c.outputQueue = [{ kind: 'output', replay: true, token: 1 }];
  s.drainTerminalOutputQueue(c);
  c.connectionToken++;
  c.initialReplayPending = true;
  callbacks.shift()();
  assert.equal(c.outputReplayWriteInFlight, true);
  assert.equal(s.terminalInitialReplaySettled(c), false);
});

test('server failure reaches workflow immediately; stale connection errors are ignored', async () => {
  const { context: c, sandbox: s, elapsed } = harness();
  const message = JSON.stringify({ type: 'terminal_connection_error', message: 'session preparation timed out' });
  s.handleServerControlMessage(message, 0, c);
  assert.equal(c.connectionError, undefined);
  s.handleServerControlMessage(message, 1, c);
  await assert.rejects(s.waitForTerminalToolSessionReady('target'), /session preparation timed out/);
  assert.equal(elapsed(), 0);
});

test('timeout reports actual blocked phase and disposed context exits early', async () => {
  const { context: c, sandbox: s } = harness();
  c.initialReplayPending = true;
  await assert.rejects(s.waitForTerminalToolSessionReady('target', 200), /等待首屏回放开始，200ms/);
  c.initialReplayPending = false;
  c.disconnected = true;
  await assert.rejects(s.waitForTerminalToolSessionReady('target', 200), /等待连接/);
  c.disposed = true;
  await assert.rejects(s.waitForTerminalToolSessionReady('target'), /已释放/);
});

test('a target session deleted on the server fails fast instead of burning the timeout', async () => {
  const { context: c, sandbox: s } = harness({
    requestJson: async () => ({ sessions: [{ id: 'other' }] }),
    state: { pendingCreatedSessionIds: new Set() },
  });
  c.disconnected = true;
  await assert.rejects(s.waitForTerminalToolSessionReady('target', 30000), /已被删除/);
});

test('a target session still listed by the server keeps waiting until the timeout', async () => {
  const { context: c, sandbox: s } = harness({
    requestJson: async () => ({ sessions: [{ id: 'target' }] }),
    state: { pendingCreatedSessionIds: new Set() },
  });
  c.disconnected = true;
  await assert.rejects(s.waitForTerminalToolSessionReady('target', 6000), /等待连接/);
});

test('a freshly created session missing from the list is not treated as deleted', async () => {
  const { context: c, sandbox: s } = harness({
    requestJson: async () => ({ sessions: [{ id: 'other' }] }),
    state: { pendingCreatedSessionIds: new Set(['target']) },
  });
  c.disconnected = true;
  await assert.rejects(s.waitForTerminalToolSessionReady('target', 6000), /等待连接/);
});

test('a failed session-list probe never proves the target was deleted', async () => {
  const { context: c, sandbox: s } = harness({
    requestJson: async () => { throw new Error('probe failed'); },
    state: { pendingCreatedSessionIds: new Set() },
  });
  c.disconnected = true;
  await assert.rejects(s.waitForTerminalToolSessionReady('target', 6000), /等待连接/);
});
