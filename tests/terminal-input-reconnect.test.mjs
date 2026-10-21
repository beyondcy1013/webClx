import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/terminal-input-transport.js", import.meta.url),
  "utf8",
);

const timers = new Map();
let nextTimerId = 1;
const warnings = [];
const interruptCounts = { interrupt: 0, refresh: 0 };
const sandbox = {
  console,
  URL,
  URLSearchParams,
  WebSocket: { OPEN: 1 },
  window: {
    location: { protocol: "http:", host: "localhost", href: "http://localhost/terminal" },
    history: { replaceState() {} },
    setTimeout(callback) {
      const id = nextTimerId++;
      timers.set(id, callback);
      return id;
    },
    clearTimeout(id) {
      timers.delete(id);
    },
  },
  state: { currentPath: "project", activeSessionId: "A", pendingRunCommand: "" },
  document: { visibilityState: "visible" },
  TERMINAL_EMPTY_CONTEXT_ID: "__empty__",
  TERMINAL_INPUT_FLUSH_DELAY_MS: 8,
  interruptTerminalBacklogReplayForInput() {
    interruptCounts.interrupt += 1;
  },
  refreshTerminalInputVisibilityAfterUserInput() {
    interruptCounts.refresh += 1;
  },
  updateSessionStatus(message, level) {
    warnings.push({ message, level });
  },
  terminalContextSocketOpen(context) {
    return context?.socket?.readyState === 1;
  },
};

function createContext(sessionId, readyState) {
  const sent = [];
  return {
    sessionId,
    pendingInputQueue: [],
    pendingInputChars: 0,
    inputFlushTimer: null,
    socket: {
      readyState,
      send(payload) {
        sent.push(JSON.parse(payload));
      },
    },
    sent,
  };
}

const contextA = createContext("A", 3);
const contextB = createContext("B", 1);
sandbox.activeTerminalContext = contextA;
vm.createContext(sandbox);
vm.runInContext(source, sandbox);

sandbox.sendTerminalInput("abc");
sandbox.flushTerminalInputQueue(contextA);
assert.equal(contextA.sent.length, 0, "disconnected input must not be discarded or sent");
assert.equal(contextA.pendingInputChars, 3);

sandbox.activeTerminalContext = contextB;
sandbox.state.activeSessionId = "B";
sandbox.sendTerminalInput("x", { flush: true });
assert.deepEqual(contextB.sent, [{ type: "input", data: "x" }]);
assert.equal(contextA.sent.length, 0, "switching sessions must not cross-send A's pending input to B");

contextA.socket.readyState = 1;
sandbox.flushTerminalInputQueue(contextA);
assert.deepEqual(contextA.sent, [{ type: "input", data: "abc" }]);
assert.equal(contextA.pendingInputChars, 0);

sandbox.flushTerminalInputQueue(contextA);
assert.equal(contextA.sent.length, 1, "a reconnect flush must deliver pending input exactly once");
assert.deepEqual(warnings, []);

console.log("terminal input reconnect regression tests passed");

contextB.sent.length = 0;
const counts = interruptCounts;
counts.interrupt = 0;
counts.refresh = 0;
assert.equal(sandbox.sendTerminalInput("\u001b[O", { terminalReport: true, flush: true }), true);
assert.deepEqual(contextB.sent, [{ type: "input", data: "\u001b[O" }]);
assert.equal(counts.interrupt, 0, "stale xterm report replies must not interrupt backlog replay");
assert.equal(counts.refresh, 0, "stale xterm report replies must not refresh input visibility");
assert.equal(sandbox.sendTerminalInput("y", { flush: true }), true);
assert.equal(counts.interrupt, 1, "real input should still interrupt backlog replay");
assert.equal(counts.refresh, 1, "real input should still refresh input visibility");

// The original failure: a bracketed paste lost its closing marker at 64 KiB.
const largePaste = '\x1b[200~' + '测😀\n'.repeat(20000) + '\x1b[201~';
contextB.sent.length = 0;
assert.equal(sandbox.sendTerminalInput('older'), true);
assert.equal(sandbox.sendTerminalInput(largePaste, { flush: true }), true);
assert.deepEqual(contextB.sent.map(item => item.data), ['older', largePaste]);
assert.equal(contextB.pendingInputChars, 0);

contextB.socket.readyState = 3;
assert.equal(sandbox.sendTerminalInput('queued'), true);
assert.equal(sandbox.sendTerminalInput(largePaste), false);
assert.equal(contextB.pendingInputChars, 6);
assert.deepEqual(Array.from(contextB.pendingInputQueue), ['queued']);
contextB.socket.readyState = 1;
sandbox.flushTerminalInputQueue(contextB);
assert.equal(contextB.sent.at(-1).data, 'queued');

// Exercise the actual dialog helpers, including false-success and retry paths.
const statuses = [];
let closed = 0;
Object.assign(sandbox, {
  term: { modes: { bracketedPasteMode: true } },
  ensureTerminalReadyForInput: () => true,
  cancelNewSessionQuickStart() {},
  prepareTerminalPasteText: text => text.replace(/\r\n|\r|\n/g, '\r'),
  wrapBracketedTerminalPaste: text => '\x1b[200~' + text + '\x1b[201~',
  hasTerminalPasteLineBreak: text => /[\r\n]/.test(text),
  refreshTerminalInputVisibilityAfterPaste() {},
  focusTerminalAfterSoftKeyboardInput() {},
  updateStatus: (text, level) => statuses.push({ text, level }),
  closeTerminalPasteDialog: () => closed++,
  terminalPasteTextEl: { value: '测😀\n'.repeat(20000) },
  MOBILE_KEY_SEQUENCES: { enter: '\r' },
});
vm.runInContext(readFileSync(new URL('../static/terminal-paste.js', import.meta.url), 'utf8'), sandbox);
const draft = sandbox.terminalPasteTextEl.value;
contextB.sent.length = 0;
assert.equal(sandbox.submitTerminalPasteDialogAndSend(), true);
assert.deepEqual(contextB.sent.map(item => item.data), [
  '\x1b[200~' + draft.replace(/\n/g, '\r') + '\x1b[201~', '\r',
]);
assert.equal(closed, 1);

for (const failure of ['disconnected', 'send throws']) {
  contextB.sent.length = 0;
  statuses.length = 0;
  const originalSend = contextB.socket.send;
  if (failure === 'disconnected') contextB.socket.readyState = 3;
  else contextB.socket.send = () => { throw new Error('socket failure'); };
  assert.equal(sandbox.submitTerminalPasteDialogAndSend(), false, failure);
  assert.equal(closed, 1, 'failed paste must keep dialog open');
  assert.equal(sandbox.terminalPasteTextEl.value, draft);
  assert.equal(contextB.pendingInputChars, 0, 'failed paste must not replay later');
  assert.equal(contextB.sent.length, 0, 'failed paste must not send Enter');
  assert.equal(statuses.some(status => status.level === 'ok'), false);
  contextB.socket.readyState = 1;
  contextB.socket.send = originalSend;
}
assert.equal(sandbox.sendPastedText('single line'), true);
assert.equal(contextB.sent.at(-1).data, '\x1b[200~single line\x1b[201~');
console.log('long paste integrity and dialog failure regression tests passed');

statuses.length = 0;
const sendBeforeEnterFailure = contextB.socket.send;
contextB.socket.send = payload => {
  if (JSON.parse(payload).data === '\r') throw new Error('Enter failed');
  sendBeforeEnterFailure(payload);
};
assert.equal(sandbox.submitTerminalPasteDialogAndSend(), false);
assert.equal(closed, 1);
assert.match(statuses.at(-1).text, /勿重复粘贴/);
assert.equal(statuses.at(-1).level, 'warn');
contextB.socket.send = sendBeforeEnterFailure;
