import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const require = createRequire(import.meta.url);
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalStyles = readFileSync(new URL("../static/styles-terminal.css", import.meta.url), "utf8");
const {
  cursorCorrectionMarkerGeometry,
  detectBottomStatusCursorCorrection,
} = require("../static/terminal-cursor-guard.js");

assert.deepEqual(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 4,
    rows: 24,
    columns: 80,
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "› 继续修复终端光标";
      if (index === 22) return "";
      if (index === 23) return "Esc cancel   Ctrl+C quit   82% context left";
      return "";
    }),
  }),
  { row: 21, column: 18 },
  "Codex bottom status cursor should use terminal cell width for CJK input",
);

assert.deepEqual(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 4,
    rows: 24,
    columns: 80,
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "› continue editing";
      if (index === 22) return "";
      if (index === 23) return "Esc cancel   Ctrl+C quit   82% context left";
      return "";
    }),
  }),
  { row: 21, column: 18 },
  "ASCII Codex input cursor correction should stay unchanged",
);

assert.deepEqual(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 2,
    rows: 24,
    columns: 80,
    placeholderRanges: [{ row: 21, startColumn: 2, endColumn: 23 }],
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "› Explain this codebase";
      if (index === 22) return "";
      if (index === 23) return "Esc cancel   Ctrl+C quit   82% context left";
      return "";
    }),
  }),
  { row: 21, column: 2 },
  "empty Codex placeholder text should not move the corrected cursor to the placeholder end",
);

assert.equal(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 11,
    rows: 24,
    columns: 80,
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "build finished";
      if (index === 22) return "";
      if (index === 23) return "root@host:~#";
      return "";
    }),
  }),
  null,
  "ordinary shell prompts at the last row should keep the real cursor",
);

assert.equal(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 4,
    rows: 24,
    columns: 80,
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "generic app input";
      if (index === 22) return "";
      if (index === 23) return "Press ? for help";
      return "";
    }),
  }),
  null,
  "generic bottom help layouts should not be visually corrected without a Codex input prompt",
);

assert.equal(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 4,
    rows: 24,
    columns: 80,
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "› 继续修复终端光标";
      if (index === 22) return "";
      if (index === 23) return "working... 82% context left";
      return "";
    }),
  }),
  null,
  "Codex busy status lines should not toggle the visual cursor correction",
);

assert.equal(
  detectBottomStatusCursorCorrection({
    cursorRow: 21,
    cursorColumn: 6,
    rows: 24,
    columns: 80,
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "› normal input cursor";
      if (index === 22) return "";
      if (index === 23) return "Esc cancel   Ctrl+C quit";
      return "";
    }),
  }),
  null,
  "no correction is needed when the terminal cursor is already on the input row",
);

assert.equal(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 4,
    rows: 24,
    columns: 80,
    applicationCursorRows: [21],
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "› continue editing";
      if (index === 22) return "";
      if (index === 23) return "Esc cancel   Ctrl+C quit   82% context left";
      return "";
    }),
  }),
  null,
  "Codex input lines that already draw an application cursor should not get a second end cursor",
);

assert.deepEqual(
  cursorCorrectionMarkerGeometry({ cellWidth: 16, cellHeight: 21, column: 8, row: 3 }),
  {
    width: 2,
    height: 21,
    x: 128,
    y: 63,
  },
  "visual cursor correction should use a narrow bar instead of a full-cell block",
);

assert.match(
  terminalJs,
  /function syncTerminalSoftKeyboardCursor\(\)[\s\S]*terminalSoftKeyboardCursorTarget\(\)[\s\S]*terminalCellDimensions\(\)[\s\S]*marker\.hidden = false;[\s\S]*marker\.style\.transform = `translate\(\$\{x\}px, \$\{y\}px\)`;/,
  "soft-keyboard mode should draw a visual cursor from xterm buffer geometry instead of focusing the helper textarea",
);

assert.match(
  terminalJs,
  /function focusTerminalAfterSoftKeyboardInput\(\) \{[\s\S]*syncTerminalImePolicy\(\);[\s\S]*syncTerminalSoftKeyboardCursor\(\);[\s\S]*\}/,
  "soft-keyboard input should refresh the visual cursor without changing system keyboard focus state",
);

assert.match(
  terminalStyles,
  /\.terminal-soft-keyboard-cursor \{[\s\S]*animation:\s*terminal-soft-keyboard-cursor-blink 1s steps\(1, end\) infinite;[\s\S]*\}/,
  "soft-keyboard visual cursor should blink independently of xterm focus",
);

assert.match(
  terminalStyles,
  /@keyframes terminal-soft-keyboard-cursor-blink \{[\s\S]*background:\s*var\(--terminal-cursor\);[\s\S]*background:\s*var\(--terminal-bg\);[\s\S]*\}/,
  "soft-keyboard visual cursor should cover a non-blinking native cursor during the off phase",
);

assert.match(
  terminalHtml,
  /styles-terminal\.css\?v=20260804b/,
  "terminal page should load the split terminal stylesheet with its current cache key",
);

assert.equal(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 4,
    rows: 24,
    columns: 80,
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "› stale Codex input left after TUI exit";
      if (index === 22) return "";
      if (index === 23) return "[root@openeuler stockInfo]#";
      return "";
    }),
  }),
  null,
  "a shell prompt at the bottom row must not be mistaken for a Codex status line even when a stale › input row remains above",
);

assert.equal(
  detectBottomStatusCursorCorrection({
    cursorRow: 23,
    cursorColumn: 5,
    rows: 24,
    columns: 80,
    lines: Array.from({ length: 24 }, (_, index) => {
      if (index === 21) return "› stale Codex input";
      if (index === 22) return "";
      if (index === 23) return "[root@openeuler stockInfo]# ~";
      return "";
    }),
  }),
  null,
  "a shell prompt followed by a typed command must not trigger cursor correction",
);

assert.match(
  terminalJs,
  /function terminalSelectionBlockingCursorCorrection\(\)[\s\S]*terminalSelectionHandleDrag !== null[\s\S]*term\.hasSelection\(\)/,
  "cursor correction must be blocked while a text selection is active",
);

assert.match(
  terminalJs,
  /function syncTerminalCursorCorrection\(\) \{[\s\S]*terminalSelectionBlockingCursorCorrection\(\)[\s\S]*return;[\s\S]*\}/,
  "syncTerminalCursorCorrection must bail out during an active selection instead of toggling theme every frame",
);
