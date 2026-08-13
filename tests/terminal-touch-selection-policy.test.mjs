import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const {
  TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX,
  TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
  terminalTouchScrollStep,
  terminalTouchSelectionContextMenuAction,
  terminalTouchSelectionInitialRange,
  terminalTouchSelectionMoveAction,
} = require("../static/terminal-touch-selection-policy.js");
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalFocusSelectionJs = readFileSync(
  new URL("../static/terminal-focus-selection.js", import.meta.url),
  "utf8",
);
const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);

assert.equal(
  TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
  2000,
  "touch text selection should require at least a two-second long press",
);

assert.match(
  terminalJs,
  /let terminalTouchSelectionDisabled = false;/,
  "terminal touch text selection should be allowed by default so long-press copy is available immediately",
);

assert.equal(
  terminalTouchSelectionMoveAction({
    elapsedMs: TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS - 1,
    offsetX: TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX + 4,
    offsetY: 0,
  }),
  "cancel",
  "dragging before long press should cancel touch selection instead of selecting text",
);

assert.equal(
  terminalTouchSelectionMoveAction({
    elapsedMs: TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
    offsetX: TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX + 4,
    offsetY: 0,
  }),
  "cancel",
  "a drag should cancel a pending long press even when its move event arrives at the threshold",
);

assert.equal(
  terminalTouchSelectionMoveAction({
    elapsedMs: 2500,
    offsetX: TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX + 4,
    offsetY: 0,
    longPressMs: 3000,
  }),
  "cancel",
  "a configured longer press threshold should delay touch text selection",
);

assert.equal(
  terminalTouchSelectionMoveAction({
    elapsedMs: 3000,
    offsetX: TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX + 4,
    offsetY: 0,
    longPressMs: 3000,
  }),
  "cancel",
  "configured long-press timing must not turn an in-progress drag into text selection",
);

assert.equal(
  terminalTouchSelectionContextMenuAction({
    elapsedMs: 600,
    longPressMs: TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
  }),
  "ignore",
  "an early browser contextmenu event must not bypass the configured long-press delay",
);

assert.equal(
  terminalTouchSelectionContextMenuAction({
    elapsedMs: TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
    longPressMs: TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
  }),
  "select",
  "contextmenu may preserve selection once the configured long press has completed",
);

let slowDragRemainder = 0;
let slowDragLines = 0;
for (const deltaPixels of Array.from({ length: 10 }, () => -2)) {
  const step = terminalTouchScrollStep({
    deltaPixels,
    remainderPixels: slowDragRemainder,
    rowHeight: 20,
  });
  slowDragRemainder = step.remainderPixels;
  slowDragLines += step.lines;
}
assert.equal(
  slowDragLines,
  -1,
  "one row of accumulated touch movement should scroll exactly one terminal line",
);
assert.equal(
  slowDragRemainder,
  0,
  "touch scrolling should retain only the unconsumed pixel remainder",
);

assert.equal(
  terminalTouchSelectionMoveAction({
    elapsedMs: TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS + 100,
    offsetX: TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX - 1,
    offsetY: 0,
  }),
  "keep",
  "small movement should keep the long-press candidate alive",
);

assert.deepEqual(
  terminalTouchSelectionInitialRange({ column: 80, row: 12 }, 80),
  { column: 79, row: 12, length: 1 },
  "long press should create a visible one-cell selection at the pressed terminal cell",
);

assert.match(
  terminalFocusSelectionJs,
  /function handleTerminalTouchSelectionMove\(event\) \{[\s\S]*if \(terminalTouchSelection\?\.anchor\) \{[\s\S]*event\.preventDefault\(\);[\s\S]*event\.stopPropagation\(\);[\s\S]*return;[\s\S]*\}/,
  "ordinary touchmove after long press should not expand the initial one-cell selection",
);

assert.doesNotMatch(
  terminalFocusSelectionJs,
  /function handleTerminalTouchSelectionMove\(event\) \{[\s\S]*applyTerminalTouchSelectionDrag/,
  "long-press touchmove should not call the old drag-expansion path",
);

assert.match(
  terminalFocusSelectionJs,
  /function preventNativeTerminalTouchSelection\(event\) \{[\s\S]*document\.getSelection\(\)\?\.removeAllRanges\(\);[\s\S]*event\.preventDefault\(\);[\s\S]*event\.stopPropagation\(\);[\s\S]*\}/,
  "terminal touch selection should suppress browser-native text selection so only the xterm handles appear",
);

assert.match(
  terminalJs,
  /document\.addEventListener\("selectstart", preventNativeTerminalTouchSelection, \{ capture: true \}\);/,
  "terminal page should prevent native selectstart while a touch-selection candidate is active",
);

assert.match(
  terminalJs,
  /terminalHost\.addEventListener\("contextmenu", handleTerminalContextMenuSelection\);/,
  "terminal contextmenu should use the guarded touch-selection handler instead of starting selection twice",
);

assert.match(
  terminalFocusSelectionJs,
  /terminalTouchSelectionContextMenuAction\([\s\S]*?\) === "ignore"[\s\S]*?event\.preventDefault\(\);[\s\S]*?event\.stopPropagation\(\);[\s\S]*?return;/,
  "touch contextmenu should honor the long-press policy before beginning terminal selection",
);

assert.match(
  terminalFocusSelectionJs,
  /function handleTerminalTouchScrollMove\(event\) \{[\s\S]*terminalTouchScrollStep\([\s\S]*term\.scrollLines\(step\.lines\);[\s\S]*clearTerminalTouchSelectionCandidate\(touch\.identifier\);[\s\S]*event\.preventDefault\(\);[\s\S]*event\.stopPropagation\(\);/,
  "terminal touch scrolling should accumulate pixels, scroll through xterm, and cancel long press",
);

assert.match(
  terminalStyles,
  /@media \(pointer: coarse\) \{[\s\S]*?\.terminal-host \.xterm \.xterm-rows,[\s\S]*?\.terminal-host \.terminal-codex-status-compact-overlay > \*[\s\S]*?pointer-events:\s*none;/,
  "coarse-pointer terminal touches should target stable containers instead of replaced DOM rows",
);
