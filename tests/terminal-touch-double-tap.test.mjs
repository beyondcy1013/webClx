import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import vm from "node:vm";

const require = createRequire(import.meta.url);
const policy = require("../static/terminal-touch-selection-policy.js");
const imePolicy = require("../static/terminal-ime-policy.js");

// 1. Policy unit tests: isDoubleTap
assert.equal(
  policy.terminalTouchSelectionIsDoubleTap(
    { clientX: 100, clientY: 100, time: 1000 },
    { clientX: 105, clientY: 102, time: 1150 },
  ),
  true,
  "taps within 150ms and 6px must be recognized as double tap",
);

assert.equal(
  policy.terminalTouchSelectionIsDoubleTap(
    { clientX: 100, clientY: 100, time: 1000 },
    { clientX: 100, clientY: 100, time: 1350 },
  ),
  false,
  "taps exceeding 300ms must not be recognized as double tap",
);

assert.equal(
  policy.terminalTouchSelectionIsDoubleTap(
    { clientX: 100, clientY: 100, time: 1000 },
    { clientX: 150, clientY: 100, time: 1100 },
  ),
  false,
  "taps exceeding 24px distance must not be recognized as double tap",
);

assert.equal(
  policy.terminalTouchSelectionIsDoubleTap(null, { clientX: 100, clientY: 100, time: 1000 }),
  false,
  "missing first tap must return false",
);

// 2. Policy unit tests: wordRange
const line1 = "echo /home/codes/tools/webClx/static/terminal.js --flag";
const rangePath = policy.terminalTouchSelectionWordRange({ column: 15, row: 0 }, line1, 80);
assert.equal(rangePath.column, 5);
assert.equal(rangePath.length, 43);
assert.equal(
  line1.substring(rangePath.column, rangePath.column + rangePath.length),
  "/home/codes/tools/webClx/static/terminal.js",
  "word selection must select full file path",
);

const rangeFlag = policy.terminalTouchSelectionWordRange({ column: 50, row: 0 }, line1, 80);
assert.equal(
  line1.substring(rangeFlag.column, rangeFlag.column + rangeFlag.length),
  "--flag",
  "word selection must select CLI flag",
);

const lineHash = "commit 6fded87f fix";
const rangeHash = policy.terminalTouchSelectionWordRange({ column: 9, row: 0 }, lineHash, 80);
assert.equal(
  lineHash.substring(rangeHash.column, rangeHash.column + rangeHash.length),
  "6fded87f",
  "word selection must select commit hash",
);

const lineZh = "说明: 双击选词不干扰输入法";
const rangeZh = policy.terminalTouchSelectionWordRange({ column: 6, row: 0 }, lineZh, 80);
assert.equal(
  lineZh.substring(rangeZh.column, rangeZh.column + rangeZh.length),
  "双击选词不干扰输入法",
  "word selection must select contiguous CJK phrase",
);

const rangeSpace = policy.terminalTouchSelectionWordRange({ column: 3, row: 0 }, lineZh, 80);
assert.equal(
  lineZh.substring(rangeSpace.column, rangeSpace.column + rangeSpace.length),
  " ",
  "word selection on space selects space",
);

// 3. VM simulation: Double-tap word selection in terminal-focus-selection.js
class TerminalNode {
  constructor() {
    this.hidden = false;
    this.style = { removeProperty() {} };
    this.addEventListener = () => {};
    this.classList = { add() {}, remove() {} };
    this.getAttribute = () => null;
    this.setAttribute = () => {};
    this.blur = () => blurCalls.push("blur");
  }
}
const target = new TerminalNode();
const selections = [];
const focusCalls = [];
const blurCalls = [];
const timers = new Map();
let nextTimerId = 1;

function fakeSetTimeout(fn, ms) {
  const id = nextTimerId++;
  timers.set(id, { fn, ms });
  return id;
}

function fakeClearTimeout(id) {
  timers.delete(id);
}

function runTimer(id) {
  const t = timers.get(id);
  if (t) {
    timers.delete(id);
    t.fn();
  }
}

const mockBufferLine = {
  translateToString: (trimRight) => "git checkout 6fded87f",
};

const mockBuffer = {
  type: "normal",
  viewportY: 0,
  baseY: 0,
  length: 24,
  getLine: (row) => mockBufferLine,
};

let keyboardVisibleState = false;

const mockScreen = new TerminalNode();
mockScreen.getBoundingClientRect = () => ({ left: 0, top: 0, width: 800, height: 480 });

const mockShell = new TerminalNode();
mockShell.getBoundingClientRect = () => ({ left: 0, top: 0, width: 800, height: 480 });

const sandbox = vm.createContext({
  Node: TerminalNode,
  HTMLElement: TerminalNode,
  HTMLTextAreaElement: TerminalNode,
  HTMLInputElement: TerminalNode,
  term: {
    cols: 80,
    rows: 24,
    textarea: new TerminalNode(),
    element: {
      contains: (node) => node === target,
      querySelector: (sel) => (sel === ".xterm-screen" ? mockScreen : null),
    },
    buffer: { active: mockBuffer },
    select: (col, row, len) => selections.push({ col, row, len }),
    focus: () => focusCalls.push("term.focus"),
    hasSelection: () => selections.length > 0,
    clearSelection: () => selections.length = 0,
    getSelectionPosition: () => ({ startColumn: 0, startRow: 0, endColumn: 5, endRow: 0 }),
    scrollLines: () => {},
  },
  state: {
    activeSessionId: "s1",
    terminalTouchSelectionLongPressMs: 2000,
  },
  terminalTouchSelectionDisabled: false,
  terminalTouchSelectionCandidate: null,
  terminalTouchSelection: null,
  terminalSelectionHandleDrag: null,
  terminalTouchSelectionPolicy: policy,
  terminalImePolicy: imePolicy,
  terminalImeToggleButton: null,
  syncTerminalSoftKeyboardCursor: () => {},
  installTerminalCursorCorrectionImeAnchor: () => {},
  terminalSelectionStartHandle: new TerminalNode(),
  terminalSelectionEndHandle: new TerminalNode(),
  terminalSelectionCopyButton: new TerminalNode(),
  terminalSelectionCopyAllButton: new TerminalNode(),
  terminalSelectionOpenImeButton: new TerminalNode(),
  terminalHost: {
    closest: () => mockShell,
  },
  terminalViewportRowHeight: () => 20,
  blurTerminalHelperTextarea: () => blurCalls.push("blur"),
  terminalNativeKeyboardVisible: () => keyboardVisibleState,
  terminalAutoFocusSuppressedByEditor: () => false,
  terminalSystemImeFocusSuppressed: () => false,
  terminalSystemImeSuppressedBySoftKeyboardMode: () => false,
  terminalSystemImeSuppressedUntil: 0,
  setTerminalSystemImeEnabled: () => {},
  syncTerminalImePolicy: () => {},
  noteTerminalUserScrollIntent: () => {},
  saveTerminalScrollPositionForSession: () => {},
  terminalSelectionRangeFromPoints: (a, f, c) => ({ column: a.column, row: a.row, length: 1 }),
  terminalSelectionPointFromClient: (p, m) => ({ column: Math.floor(p.clientX / 10), row: 0 }),
  clampTerminalSelectionPoint: (p) => p,
  window: {
    WebClxAndroid: { isSystemKeyboardVisible: () => keyboardVisibleState },
    setTimeout: fakeSetTimeout,
    clearTimeout: fakeClearTimeout,
    requestAnimationFrame: (cb) => cb(),
  },
  document: {
    activeElement: null,
    getSelection: () => ({ rangeCount: 0, removeAllRanges() {} }),
  },
  Date: {
    now: () => Date.now(),
  },
});

vm.runInContext(
  readFileSync(new URL("../static/terminal-focus-selection.js", import.meta.url), "utf8"),
  sandbox,
);

function makeTouchStart(id, x, y) {
  const touch = { identifier: id, target, clientX: x, clientY: y };
  return {
    touches: [touch], changedTouches: [touch], defaultPrevented: false, stopped: false,
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; },
  };
}

function makeTouchEnd(id, x, y) {
  const touch = { identifier: id, target, clientX: x, clientY: y };
  return {
    type: "touchend",
    touches: [], changedTouches: [touch], defaultPrevented: false, stopped: false,
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; },
  };
}

// Test A: Single tap when keyboard is closed -> does NOT trigger IME upon release; triggers after disambiguation timeout
keyboardVisibleState = false;
focusCalls.length = 0;
blurCalls.length = 0;
selections.length = 0;

const singleStart = makeTouchStart(1, 150, 10);
sandbox.rememberTerminalTouchSelectionCandidate(singleStart);
assert.equal(singleStart.defaultPrevented, true);

const singleEnd = makeTouchEnd(1, 150, 10);
sandbox.handleTerminalTouchSelectionEnd(singleEnd);
assert.equal(selections.length, 0, "single tap must not select text");
assert.equal(focusCalls.length, 0, "release moment must NOT trigger IME immediately (prevents keyboard flash)");

// Run pending single-tap disambiguation timer
for (const [id] of Array.from(timers)) runTimer(id);
assert.equal(focusCalls.length > 0, true, "single tap focuses after disambiguation timeout when no second tap arrives");

// Test B: Double-tap sequence -> word selection without ever triggering IME!
focusCalls.length = 0;
blurCalls.length = 0;
selections.length = 0;

const start1 = makeTouchStart(2, 152, 10);
sandbox.rememberTerminalTouchSelectionCandidate(start1);
const end1 = makeTouchEnd(2, 152, 10);
sandbox.handleTerminalTouchSelectionEnd(end1);
assert.equal(focusCalls.length, 0, "first tap release must NOT trigger IME");

const start2 = makeTouchStart(3, 152, 10);
sandbox.rememberTerminalTouchSelectionCandidate(start2);
// Ensure second tap cancelled the first tap's single tap timer
assert.equal(timers.size, 1, "only candidate longPressTimer active, single tap timer cancelled on second tap down");

const end2 = makeTouchEnd(3, 152, 10);
sandbox.handleTerminalTouchSelectionEnd(end2);

assert.equal(selections.length, 1, "double tap must select word");
sandbox.syncTerminalSelectionControls();
assert.equal(
  sandbox.terminalSelectionCopyButton.hidden,
  false,
  "double tap must display the copy button",
);
assert.equal(
  sandbox.terminalSelectionCopyAllButton.hidden,
  false,
  "double tap must display the copy-all button in the same menu",
);
assert.equal(
  sandbox.terminalSelectionOpenImeButton.hidden,
  false,
  "double tap must display the open-IME button next to copy",
);
assert.equal(focusCalls.length, 0, "double tap must NEVER focus or trigger IME throughout the whole gesture");
assert.equal(blurCalls.length > 0, true, "double tap when keyboard was initially closed blurs helper");
// x = 152 -> col = 15 -> '6fded87f' in 'git checkout 6fded87f'
const sel = selections[0];
assert.equal(sel.col, 13);
assert.equal(sel.len, 8);
assert.equal(sel.row, 0);

// Test C: Tapping away clears active selection without triggering IME immediately
focusCalls.length = 0;
const clearStart = makeTouchStart(4, 300, 10);
sandbox.rememberTerminalTouchSelectionCandidate(clearStart);
const clearEnd = makeTouchEnd(4, 300, 10);
sandbox.handleTerminalTouchSelectionEnd(clearEnd);
assert.equal(selections.length, 0, "tapping away must clear active selection");
sandbox.syncTerminalSelectionControls();
assert.equal(
  sandbox.terminalSelectionCopyButton.hidden,
  true,
  "tapping away must hide the copy button",
);
assert.equal(
  sandbox.terminalSelectionCopyAllButton.hidden,
  true,
  "tapping away must hide the copy-all button with the rest of the menu",
);
assert.equal(
  sandbox.terminalSelectionOpenImeButton.hidden,
  true,
  "tapping away must hide the open-IME button together with the copy button",
);
assert.equal(focusCalls.length, 0, "tapping away to clear selection must NOT trigger IME immediately");
for (const [id] of Array.from(timers)) runTimer(id);
assert.equal(focusCalls.length > 0, true, "single tap after clearing selection focuses after disambiguation timeout");

// Test D: Double tap when keyboard is ALREADY open -> word selected, keyboard stays open, zero disturbance
keyboardVisibleState = true;
focusCalls.length = 0;
blurCalls.length = 0;
selections.length = 0;

let nativeClears = 0;
sandbox.document.activeElement = new TerminalNode();
sandbox.document.getSelection = () => ({
  rangeCount: 1,
  removeAllRanges() { nativeClears++; },
});

const openStart1 = makeTouchStart(5, 50, 10);
sandbox.rememberTerminalTouchSelectionCandidate(openStart1);
const openEnd1 = makeTouchEnd(5, 50, 10);
sandbox.handleTerminalTouchSelectionEnd(openEnd1);

const openStart2 = makeTouchStart(6, 52, 10);
sandbox.rememberTerminalTouchSelectionCandidate(openStart2);
const openEnd2 = makeTouchEnd(6, 52, 10);
sandbox.handleTerminalTouchSelectionEnd(openEnd2);

assert.equal(selections.length, 1, "double tap with open keyboard must select word");
assert.equal(blurCalls.length, 0, "double tap when keyboard was open must NOT blur helper");
assert.equal(nativeClears, 0, "active editor native selection must NOT be cleared");
// col = 5 -> 'checkout' in 'git checkout 6fded87f'
const sel2 = selections[0];
assert.equal(sel2.col, 4);
assert.equal(sel2.len, 8);

// Test E: Long-press release must preserve selection and NOT trigger IME
sandbox.endTerminalTouchSelection();
sandbox.term.clearSelection();
selections.length = 0;
focusCalls.length = 0;
const longStart = makeTouchStart(7, 50, 10);
sandbox.rememberTerminalTouchSelectionCandidate(longStart);
// Fire long-press timer
for (const [id] of Array.from(timers)) runTimer(id);
assert.equal(selections.length, 1, "long press timer must establish selection");
assert.equal(focusCalls.length, 0, "long press timer must not focus");

const longEnd = makeTouchEnd(7, 50, 10);
sandbox.handleTerminalTouchSelectionEnd(longEnd);
assert.equal(selections.length, 1, "long press release must preserve selection");
assert.equal(focusCalls.length, 0, "long press release must NOT trigger IME");

console.log("All double-tap policy and gesture integration tests passed successfully!");
