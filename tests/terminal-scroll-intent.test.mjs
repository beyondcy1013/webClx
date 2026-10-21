import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

class Viewport {
  scrollTop = 0;
  listeners = new Map();
  addEventListener(type, listener) { this.listeners.set(type, listener); }
  removeEventListener(type, listener) {
    if (this.listeners.get(type) === listener) this.listeners.delete(type);
  }
}

const viewport = new Viewport();
const buffer = { viewportY: 100, baseY: 100 };
const term = {
  element: { querySelector: () => viewport },
  buffer: { active: buffer },
  _core: { _renderService: { dimensions: { actualCellHeight: 10 } } },
  scrollToBottom() { buffer.viewportY = buffer.baseY; },
  scrollToLine(line) { buffer.viewportY = line; },
};
const context = { sessionId: "a", term, followOutput: true, scrollIntentRevision: 0 };
const sandbox = vm.createContext({
  HTMLElement: Viewport,
  state: { activeSessionId: "a" },
  activeTerminalContext: context,
  terminalSessionCache: new Map([["a", context]]),
  terminalScrollPositions: new Map(),
  terminalBacklogReplayActive: false,
  terminalBacklogReplayInterrupted: false,
  terminalScrollSaveSuppressionDepth: 0,
  terminalScrollSaveSuppressedUntil: 0,
  terminalScrollLayoutRestoreToken: 0,
  terminalViewportScrollEl: null,
  term,
  document: { addEventListener() {}, getElementById() { return null; } },
  window: { requestAnimationFrame() {}, setTimeout() {} },
});
for (const file of ["terminal-navigation-layout.js", "terminal-output-scroll.js"]) {
  vm.runInContext(readFileSync(new URL(`../static/${file}`, import.meta.url), "utf8"), sandbox);
}
// Keep rendering outside this scroll-state test; all save/restore decisions
// below invoke the production helpers with xterm's logical buffer coordinates.
sandbox.scheduleTerminalViewportDomSync = () => {};
sandbox.updateTerminalScrollBottomButton = () => {};
sandbox.syncTerminalSelectionHandles = () => {};

sandbox.saveTerminalScrollPositionForSession("a");
context.outputWriteInFlight = true;
buffer.baseY = 120;
sandbox.saveTerminalScrollPositionForSession("a");
assert.equal(context.followOutput, true, "switching during a write preserves bottom following");
assert.equal(sandbox.terminalScrollPositions.get("a").atBottom, true);
assert.equal(sandbox.terminalShouldStickToBottomForOutput(context), true);

sandbox.terminalScrollSaveSuppressionDepth = 1;
sandbox.noteTerminalUserScrollIntent();
buffer.viewportY = 20;
sandbox.saveTerminalScrollPositionForSession("a", { userIntent: true });
assert.equal(context.followOutput, false, "explicit scrolling wins during layout/write suppression");
buffer.viewportY = 30;
sandbox.saveTerminalScrollPositionForSession("a");
assert.equal(sandbox.terminalScrollPositions.get("a").scrollTop, 200,
  "switching during a write preserves the saved reading position");
assert.equal(sandbox.terminalShouldStickToBottomForOutput(context), false);

sandbox.noteTerminalUserScrollIntent();
buffer.viewportY = 40;
sandbox.saveTerminalScrollPositionForSession("a");
assert.equal(sandbox.terminalScrollPositions.get("a").scrollTop, 400,
  "a switch before the wheel's next frame saves the newer reading position");

context.outputWriteInFlight = false;
sandbox.terminalScrollSaveSuppressionDepth = 0;
const saved = sandbox.terminalScrollPositions.get("a");
sandbox.handleTerminalViewportScroll({ currentTarget: new Viewport() });
assert.equal(sandbox.terminalScrollPositions.get("a"), saved,
  "an old terminal's scroll event cannot save the active terminal");

buffer.viewportY = buffer.baseY;
context.followOutput = true;
const snapshot = sandbox.captureTerminalScrollSnapshotForSession("a");
assert.equal(sandbox.captureTerminalScrollSnapshotForSession("b"), null,
  "a selection must not label the previous context's viewport as the new session");
sandbox.noteTerminalUserScrollIntent();
buffer.viewportY = 20;
sandbox.saveTerminalScrollPositionForSession("a", { userIntent: true });
sandbox.restoreTerminalScrollSnapshot(snapshot);
assert.equal(buffer.viewportY, 20, "an older layout restore cannot undo newer user scrolling");

// A page-resume snapshot can be captured before the first wheel/touch intent,
// when no revision exists yet. The first user scroll must still invalidate it.
context.scrollIntentRevision = undefined;
buffer.viewportY = buffer.baseY;
context.followOutput = true;
const firstScrollSnapshot = sandbox.captureTerminalScrollSnapshotForSession("a");
sandbox.noteTerminalUserScrollIntent();
buffer.viewportY = 10;
sandbox.saveTerminalScrollPositionForSession("a", { userIntent: true });
sandbox.restoreTerminalScrollSnapshot(firstScrollSnapshot);
assert.equal(buffer.viewportY, 10, "a pre-intent resume snapshot cannot undo the first user scroll");

// Switching rebinds the listener and removes it from the previous viewport.
sandbox.bindTerminalViewportScroll();
const nextViewport = new Viewport();
term.element.querySelector = () => nextViewport;
sandbox.bindTerminalViewportScroll();
assert.equal(viewport.listeners.has("scroll"), false);
assert.equal(nextViewport.listeners.get("scroll"), sandbox.handleTerminalViewportScroll);

console.log("terminal scroll intent tests passed");
