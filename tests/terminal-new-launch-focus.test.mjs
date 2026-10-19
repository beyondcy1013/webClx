import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const focusSource = readFileSync(
  new URL("../static/terminal-focus-selection.js", import.meta.url),
  "utf8",
);
const sessionsSource = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);

test("a direct terminal launch settles focus into xterm after replay", () => {
  assert.match(
    focusSource,
    /let pendingNewTerminalLaunchFocusSessionId = "";[\s\S]*function focusTerminalAfterNewTerminalLaunch\(sessionId, sourceControl = null\)/,
  );
  assert.match(
    focusSource,
    /if \(sourceControl instanceof HTMLElement && document\.activeElement === sourceControl\) \{\s*sourceControl\.blur\(\);\s*\}/,
  );
  assert.match(
    focusSource,
    /if \(\!terminalContextSocketOpen\(activeTerminalContext\)\) \{\s*return;\s*\}\s*if \(!terminalInitialReplaySettled\(activeTerminalContext\)\) \{\s*return;\s*\}/,
  );
  assert.match(
    focusSource,
    /terminalActionKeyboardPolicyActive = false;\s*terminalActionEditableFocusBlocked = false;\s*term\.focus\(\);/,
  );
  assert.match(
    focusSource,
    /window\.setTimeout\(restore, 280\);\s*window\.setTimeout\(restore, 800\);/,
  );
  assert.match(
    sessionsSource,
    /selectSession\(session\.id, \{\s*connect: true,\s*pushHistory: pushHistoryOnSelect,\s*\}\);\s*focusTerminalAfterNewTerminalLaunch\(session\.id, createSessionButton\);/,
    "new-terminal selection should request focus after the connection/replay path starts",
  );

  assert.match(
    terminalHtml,
    /terminal-focus-selection\.js\?v=20260929-new-focus[\s\S]*terminal-sessions\.js\?v=20260929-new-focus/,
    "both changed scripts should use refreshed browser cache keys",
  );
});
