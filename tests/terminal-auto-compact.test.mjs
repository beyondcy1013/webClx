import assert from "node:assert/strict";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalJs = readEntryScriptBundle("terminal.html");

assert.match(
  terminalJs,
  /autoCompactLastSent: readStoredTerminalAutoCompactLastSent\(\)/,
  "terminal state should retain the last automatic compact position",
);
assert.match(
  terminalJs,
  /function terminalOutputLineCount\(sessionId\)[\s\S]*?function maybeAutoCompactCompletedSession/,
  "auto compact should be able to compare output positions",
);
assert.match(
  terminalJs,
  /linesSinceCompact < 20[\s\S]*?return false;/,
  "a terminal must not compact again within twenty output lines",
);
assert.match(
  terminalJs,
  /if \(terminalRecentLinesContainCompacted\(session\.id, 20\)\)[\s\S]*?return false;/,
  "nearby completed compact output must suppress another automatic compact",
);
assert.match(
  terminalJs,
  /function terminalRecentLinesContainQueuedInput\(sessionId, lineCount = 20\)[\s\S]*?Messages to be submitted at end of turn[\s\S]*?Queued follow-up inputs/,
  "auto compact should detect nearby Codex queued user input (both legacy and current Codex headers)",
);
assert.match(
  terminalJs,
  /if \(terminalRecentLinesContainQueuedInput\(session\.id, 20\)\)[\s\S]*?return false;/,
  "nearby queued user input must suppress automatic compaction",
);
assert.match(
  terminalJs,
  /if \(session\.id === state\.activeSessionId\)[\s\S]*?autoCompactHandledSessions\.delete\(session\.id\)[\s\S]*?return false;/,
  "the terminal currently being viewed must never receive automatic compact input",
);
assert.match(
  terminalJs,
  /2 秒后自动压缩[\s\S]*?setTimeout\([\s\S]*?2000[\s\S]*?resolveAutoCompactAfterDelay/,
  "automatic compact input should have an explicit two-second cancellable delay",
);
assert.match(
  terminalJs,
  /持续 5 秒后再进入压缩倒计时[\s\S]*?setTimeout\([\s\S]*?5000/,
  "a terminal must remain pending review for five seconds before the compact countdown",
);
assert.match(terminalJs, /已持续“待查看”5 秒，2 秒后自动压缩/);
assert.match(
  terminalJs,
  /autoCompactLastSent\.set\(sessionId, \{[\s\S]*?lineCount: terminalOutputLineCount\(sessionId\)/,
  "successful automatic compaction should record its output position",
);
assert.match(
  terminalJs,
  /terminalRecentLinesContainCompacted\(sessionId, 20\)[\s\S]*?return null;[\s\S]*?sendTerminalAutoTypedInput\("\/compact"/,
  "the send-time guard must recheck nearby compact completion before submitting",
);
assert.match(
  terminalJs,
  /terminalRecentLinesContainQueuedInput\(sessionId, 20\)[\s\S]*?return null;[\s\S]*?sendTerminalAutoTypedInput\("\/compact"/,
  "the send-time guard must recheck nearby queued user input before submitting",
);
assert.match(
  terminalJs,
  /terminalSessionHasRecentUserInput\(sessionId\)[\s\S]*?return null;[\s\S]*?sendTerminalAutoTypedInput\("\/compact"/,
  "the send-time guard must recheck recent user typing before submitting",
);
assert.match(
  terminalJs,
  /function isSessionCompactCapableAgent\(session\) \{[\s\S]*?!agent\.includes\("cline"\)[\s\S]*?!agent\.includes\("agy"\)[\s\S]*?!agent\.includes\("codebuddy"\)/,
  "Cline, AGY and CodeBuddy sessions must not enter the Codex-style compact lifecycle",
);
assert.match(
  terminalJs,
  /function syncAutoCompactCompletionStatuses\(\) \{[\s\S]*?previousStates\.get\(sessionId\) === "compacting"[\s\S]*?currentState !== "compacting"[\s\S]*?autoCompactStatusSessionIds\.add\(sessionId\)/,
  "the compacted marker must follow the backend compacting state ending",
);
assert.doesNotMatch(
  terminalJs,
  /autoCompactCompactingSessionIds/,
  "the selector must not fabricate a compacting state locally",
);
assert.match(
  terminalJs,
  /已发送 \/compact，等待压缩开始/,
  "sending /compact must only report delivery, not a running compaction",
);
assert.match(
  terminalJs,
  /!isSessionCompactCapableAgent\(session\) \|\| isSessionAgyTerminal\(session\)/,
  "auto compact candidate filtering must recheck the live agent type",
);
assert.match(
  terminalJs,
  /!isSessionCompactCapableAgent\(latest\) \|\| isSessionAgyTerminal\(latest\)/,
  "the delayed compact sender must recheck the live agent type",
);

console.log("terminal auto compact guard checks passed");
