import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const {
  clampTerminalSelectionPoint,
  terminalSelectionRangeFromPoints,
  terminalSelectionPointFromClient,
} = require("../static/terminal-selection-geometry.js");

assert.deepEqual(
  terminalSelectionRangeFromPoints({ column: 12, row: 8 }, { column: 4, row: 8 }, 80),
  { column: 4, row: 8, length: 8 },
  "dragging the start handle left should keep the lower point as selection start",
);

assert.deepEqual(
  terminalSelectionRangeFromPoints({ column: 72, row: 8 }, { column: 5, row: 10 }, 80),
  { column: 72, row: 8, length: 93 },
  "multi-row selection length should span wrapped terminal cells",
);

assert.deepEqual(
  clampTerminalSelectionPoint({ column: -3, row: 30 }, 80, 24),
  { column: 0, row: 24 },
  "selection points should stay within terminal buffer bounds",
);

assert.deepEqual(
  terminalSelectionPointFromClient(
    { clientX: 49, clientY: 58 },
    {
      left: 10,
      top: 20,
      width: 800,
      height: 400,
      columns: 80,
      rows: 20,
      viewportY: 100,
      maxRow: 140,
    },
  ),
  { column: 4, row: 101 },
  "client coordinates should map to the nearest terminal cell boundary",
);
