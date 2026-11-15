import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import vm from "node:vm";

const require = createRequire(import.meta.url);
class TerminalNode {}
const target = new TerminalNode();
const pageScrolls = [];
const terminalScrolls = [];
const buffer = { type: "normal", viewportY: 10, baseY: 10 };
const sandbox = vm.createContext({
  Node: TerminalNode,
  term: {
    element: { contains: (node) => node === target },
    buffer: { active: buffer },
    scrollLines: (lines) => terminalScrolls.push(lines),
  },
  state: { activeSessionId: "a", terminalTouchSelectionLongPressMs: 2000 },
  terminalTouchSelectionDisabled: false,
  terminalTouchSelectionCandidate: null,
  terminalTouchSelection: null,
  terminalSelectionHandleDrag: null,
  terminalTouchSelectionPolicy: require("../static/terminal-touch-selection-policy.js"),
  terminalViewportRowHeight: () => 10,
  noteTerminalUserScrollIntent() {},
  saveTerminalScrollPositionForSession() {},
  window: {
    setTimeout: () => 1,
    clearTimeout() {},
    scrollBy: (options) => pageScrolls.push(options.top),
  },
});
vm.runInContext(readFileSync(new URL("../static/terminal-focus-selection.js", import.meta.url), "utf8"), sandbox);

function touchEvent(y) {
  const touch = { identifier: 1, target, clientX: 20, clientY: y };
  return {
    touches: [touch], changedTouches: [touch], defaultPrevented: false,
    stopped: false,
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; },
  };
}

const start = touchEvent(100);
sandbox.rememberTerminalTouchScrollGesture(start);
sandbox.rememberTerminalTouchSelectionCandidate(start);
assert.equal(start.defaultPrevented, true, "touchstart must cancel native editor activation");

// At the terminal's bottom, cancelling touchstart must not strand the page:
// the same real gesture moves the outer page instead of re-enabling defaults.
const boundaryMove = touchEvent(70);
assert.equal(sandbox.handleTerminalTouchScrollMove(boundaryMove), true);
assert.deepEqual(pageScrolls, [30]);
assert.deepEqual(terminalScrolls, []);
assert.equal(boundaryMove.defaultPrevented, true);
assert.equal(boundaryMove.stopped, true);

// Move back into terminal history: terminal scrolling owns the gesture again.
const historyMove = touchEvent(90);
assert.equal(sandbox.handleTerminalTouchScrollMove(historyMove), true);
assert.deepEqual(terminalScrolls, [-2]);
assert.deepEqual(pageScrolls, [30]);
assert.equal(sandbox.terminalTouchSelectionCandidate, null, "scrolling cancels pending selection");
assert.equal(historyMove.defaultPrevented, true);

// Selection disabled: leave native touch defaults and boundary panning alone.
sandbox.terminalTouchSelectionDisabled = true;
const nativeStart = touchEvent(100);
sandbox.rememberTerminalTouchScrollGesture(nativeStart);
sandbox.rememberTerminalTouchSelectionCandidate(nativeStart);
assert.equal(nativeStart.defaultPrevented, false);
assert.equal(sandbox.handleTerminalTouchScrollMove(touchEvent(70)), false);
assert.deepEqual(pageScrolls, [30]);

// Native selection belongs to the active input connection while editing.
// Never clear it (even when the DOM range appears to belong to terminal text).
class Editable extends TerminalNode {}
let clears = 0;
const nativeSelection = {
  rangeCount: 1, isCollapsed: false, anchorNode: target, focusNode: target,
  removeAllRanges() { clears++; },
};
sandbox.HTMLTextAreaElement = Editable;
sandbox.HTMLInputElement = Editable;
sandbox.HTMLElement = TerminalNode;
sandbox.document = { activeElement: new Editable(), getSelection: () => nativeSelection };
sandbox.clearNativeTerminalTextSelection();
assert.equal(clears, 0, "active editor selection must remain untouched");
sandbox.document.activeElement = null;
nativeSelection.rangeCount = 0;
sandbox.clearNativeTerminalTextSelection();
assert.equal(clears, 0, "empty selection must not reset native editor selection");
nativeSelection.rangeCount = 1;
sandbox.clearNativeTerminalTextSelection();
assert.equal(clears, 1, "stray terminal DOM selection can be cleared without an active editor");
nativeSelection.anchorNode = new TerminalNode();
sandbox.clearNativeTerminalTextSelection();
assert.equal(clears, 1, "selection outside terminal is not ours to clear");

// Double tap word range policy checks
const policy = sandbox.terminalTouchSelectionPolicy;
assert.equal(
  policy.terminalTouchSelectionIsDoubleTap(
    { clientX: 20, clientY: 50, time: 1000 },
    { clientX: 22, clientY: 51, time: 1150 },
  ),
  true,
  "double tap within 150ms and 3px must be detected",
);
assert.equal(
  policy.terminalTouchSelectionIsDoubleTap(
    { clientX: 20, clientY: 50, time: 1000 },
    { clientX: 20, clientY: 50, time: 1400 },
  ),
  false,
  "taps > 300ms must not be treated as double tap",
);

const pathLine = "cat /home/codes/tools/webClx/static/terminal.js";
const wordRange = policy.terminalTouchSelectionWordRange({ column: 12, row: 0 }, pathLine, 80);
assert.equal(
  pathLine.substring(wordRange.column, wordRange.column + wordRange.length),
  "/home/codes/tools/webClx/static/terminal.js",
  "word range must select entire path",
);
