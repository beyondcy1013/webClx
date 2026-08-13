import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalStaticDir = new URL("../static/", import.meta.url);
const terminalStaticNames = [
  "terminal.js",
  "terminal-common.js",
  "terminal-output-scroll.js",
  "terminal-input-transport.js",
  "terminal-layout-connection.js",
  "terminal-navigation-layout.js",
  "terminal-sessions.js",
  "terminal-session-render.js",
  "terminal-mobile-keys.js",
  "terminal-paste.js",
  "terminal-focus-selection.js",
  "terminal-resume-agent.js",
  "terminal-cursor-correction.js",
  "terminal-dialogs.js",
  "terminal-quota.js",
  "terminal-command-quickstart.js",
  "terminal-settings-loader.js",
  "terminal-shell-settings.js",
  "terminal-auto-continue.js",
  "terminal-settings.js",
  "terminal-session-activity.js",
  "terminal-session-storage.js",
  "terminal-session-cache.js",
  "terminal-ime-policy.js",
  "terminal-touch-selection-policy.js",
  "terminal-selection-geometry.js",
  "terminal-cursor-guard.js",
  "terminal-resume-extract.js",
];
// At runtime all terminal-*.js files are deferred scripts sharing one global
// scope, so layout/ordering assertions apply across files. This concatenation
// is safe for indexOf (literal) and the single-function-body regexes below;
// the blob-handler assertion is anchored to its own file to avoid backtracking.
const terminalJs = terminalStaticNames
  .map((name) => readFileSync(new URL(name, terminalStaticDir), "utf8"))
  .join("\n");
const outputScrollJs = readFileSync(new URL("terminal-output-scroll.js", terminalStaticDir), "utf8");
const layoutConnectionJs = readFileSync(new URL("terminal-layout-connection.js", terminalStaticDir), "utf8");

assert.match(
  terminalJs,
  /outputQueue: \[\],[\s\S]*outputWriteInFlight: false,[\s\S]*outputDrainTimer: null,/,
  "each terminal context should keep its own write queue instead of flooding xterm's internal queue",
);

assert.match(
  outputScrollJs,
  /function resetTerminalOutputQueue\(context = activeTerminalContext\) \{[\s\S]*clearTerminalOutputDrainTimer\(context\);[\s\S]*context\.outputQueue = \[\];[\s\S]*context\.outputWriteInFlight = false;[\s\S]*\}/,
  "reconnecting a session should reset only the output queue owned by that context",
);

assert.match(
  outputScrollJs,
  /function scheduleTerminalOutputDrain\(context\) \{[\s\S]*context\.outputDrainTimer = window\.setTimeout\(\(\) => \{[\s\S]*drainTerminalOutputQueue\(context\);[\s\S]*\}, TERMINAL_LIVE_OUTPUT_COALESCE_MS\);[\s\S]*\}/,
  "live terminal output should wait briefly so adjacent websocket chunks can merge before xterm paints",
);

assert.match(
  outputScrollJs,
  /function queueTerminalOutput\(bytes, token, context = activeTerminalContext\) \{[\s\S]*transformTerminalSynchronizedOutput\(bytes, context\)[\s\S]*const replay = context\.backlogReplayActive && !context\.backlogReplayEndQueued;[\s\S]*replay[\s\S]*transformTerminalCodexStatusOutput\(synchronizedBytes, context\)[\s\S]*: synchronizedBytes;[\s\S]*context\.outputQueue\.push\(\{ kind: "output", bytes: transformedBytes, token, replay \}\);[\s\S]*if \([\s\S]*replay \|\|[\s\S]*context !== activeTerminalContext \|\|[\s\S]*document\.visibilityState === "hidden"[\s\S]*\) \{[\s\S]*drainTerminalOutputQueue\(context\);[\s\S]*\} else \{[\s\S]*scheduleTerminalOutputDrain\(context\);[\s\S]*\}[\s\S]*\}/,
  "live websocket bytes should preserve synchronized Codex coordinates while backlog output may be compacted",
);

// The output-queue helpers were extracted to terminal-output-scroll.js; the merge
// constants stayed in terminal.js. Verify each in its real location (the previous
// single-file regex was already non-matching after the extraction). The live
// ceiling must fit a complete wide-screen Codex TUI redraw, not merely exceed
// the 8 KiB backend PTY read buffer (src/terminal/session.rs). A 204-column
// production redraw was observed arriving as 65,536 + 63,000 bytes; splitting
// those chunks across term.write() calls exposes a half-painted frame.
assert.match(
  terminalJs,
  /const TERMINAL_REPLAY_OUTPUT_MERGE_MAX_BYTES = 256 \* 1024;[\s\S]*?const TERMINAL_LIVE_OUTPUT_MERGE_MAX_BYTES = 256 \* 1024;/,
  "the live output merge ceiling must fit a complete wide-screen Codex redraw so it is not torn across write boundaries",
);
const liveOutputMergeMatch = terminalJs.match(
  /const TERMINAL_LIVE_OUTPUT_MERGE_MAX_BYTES = (\d+) \* 1024;/,
);
assert.ok(liveOutputMergeMatch, "the live output merge ceiling should remain explicit");
const liveOutputMergeMaxBytes = Number(liveOutputMergeMatch[1]) * 1024;
assert.match(
  outputScrollJs,
  /function mergeQueuedTerminalOutputItem\(firstItem, context = activeTerminalContext\) \{[\s\S]*const maxBytes = firstItem\.replay[\s\S]*TERMINAL_REPLAY_OUTPUT_MERGE_MAX_BYTES[\s\S]*TERMINAL_LIVE_OUTPUT_MERGE_MAX_BYTES[\s\S]*candidate\.kind !== "output"[\s\S]*candidate\.token !== firstItem\.token[\s\S]*candidate\.replay !== firstItem\.replay[\s\S]*totalBytes \+ candidate\.bytes\.length > maxBytes[\s\S]*return \{ \.\.\.firstItem, bytes \};[\s\S]*\}/,
  "adjacent terminal output chunks should be merged before xterm writes without crossing control frames or connection tokens",
);

const mergeOutputStart = outputScrollJs.indexOf(
  "function mergeQueuedTerminalOutputItem(firstItem, context = activeTerminalContext)",
);
const mergeOutputEnd = outputScrollJs.indexOf(
  "\nfunction drainTerminalOutputQueue",
  mergeOutputStart,
);
assert.ok(
  mergeOutputStart >= 0 && mergeOutputEnd > mergeOutputStart,
  "the terminal output merge helper should remain independently testable",
);
const mergeOutputSource = outputScrollJs.slice(mergeOutputStart, mergeOutputEnd);
const createMergeOutput = new Function(
  "TERMINAL_REPLAY_OUTPUT_MERGE_MAX_BYTES",
  "TERMINAL_LIVE_OUTPUT_MERGE_MAX_BYTES",
  `${mergeOutputSource}; return mergeQueuedTerminalOutputItem;`,
);
const mergeWideCodexOutput = createMergeOutput(256 * 1024, liveOutputMergeMaxBytes);
const wideCodexContext = {
  outputQueue: [
    {
      kind: "output",
      bytes: new Uint8Array(63_000),
      token: 7,
      replay: false,
    },
  ],
};
const mergedWideCodexFrame = mergeWideCodexOutput(
  {
    kind: "output",
    bytes: new Uint8Array(65_536),
    token: 7,
    replay: false,
  },
  wideCodexContext,
);
assert.equal(
  mergedWideCodexFrame.bytes.length,
  65_536 + 63_000,
  "one observed wide-screen Codex redraw should reach xterm as one atomic write",
);
assert.equal(
  wideCodexContext.outputQueue.length,
  0,
  "all chunks from the same wide-screen Codex redraw should be consumed by the merge",
);

assert.doesNotMatch(
  terminalJs,
  /TERMINAL_(?:RESUME_)?LIVE_OUTPUT_INPUT_(?:DROP_THRESHOLD|KEEP)_BYTES|trimQueuedLiveOutputForInput|queuedLiveOutputBytesForToken/,
  "terminal input must not discard queued live output because that makes high-volume program output disappear from browser history",
);

assert.match(
  outputScrollJs,
  /nextItem = mergeQueuedTerminalOutputItem\(nextItem, context\);[\s\S]*context\.outputWriteInFlight = true;[\s\S]*context\.term\.write\(nextItem\.bytes/,
  "terminal output queue should coalesce adjacent chunks before each xterm write",
);

assert.match(
  layoutConnectionJs,
  /const buffer = event\.data instanceof Blob \? await event\.data\.arrayBuffer\(\) : event\.data;[\s\S]*?if \(context\.disposed \|\| token !== context\.connectionToken\) \{[\s\S]*?return;[\s\S]*?\}[\s\S]*?queueTerminalOutput\(bytes, token, context\);/,
  "terminal websocket should re-check the connection token after async Blob conversion and before queueing output",
);

assert.match(
  terminalJs,
  /const TERMINAL_INPUT_FLUSH_DELAY_MS = 8;[\s\S]*const TERMINAL_LIVE_OUTPUT_COALESCE_MS = 8;[\s\S]*let terminalInputQueue = \[\];[\s\S]*let terminalInputFlushTimer = null;/,
  "terminal input should keep a short client-side queue so bursts of typed characters do not become one websocket message per key",
);

const sendTerminalInputStart = terminalJs.indexOf("function sendTerminalInput(data, options = {})");
const sendTerminalInputInterrupt = terminalJs.indexOf(
  "interruptTerminalBacklogReplayForInput();",
  sendTerminalInputStart,
);
const sendTerminalInputTrimLive = terminalJs.indexOf(
  "trimQueuedLiveOutputForInput();",
  sendTerminalInputStart,
);
const sendTerminalInputFlushCondition = terminalJs.indexOf(
  "if (options.flush || data.length >= 1024 || /[\\r\\n\\u0003\\u0004]/.test(data))",
  sendTerminalInputStart,
);
const sendTerminalInputFlush = terminalJs.indexOf("flushTerminalInputQueue();", sendTerminalInputStart);
const sendTerminalInputQueue = terminalJs.indexOf("queueTerminalInput(data);", sendTerminalInputStart);
assert.ok(
  sendTerminalInputStart >= 0 &&
    sendTerminalInputInterrupt > sendTerminalInputStart &&
    sendTerminalInputTrimLive === -1 &&
    sendTerminalInputFlushCondition > sendTerminalInputInterrupt &&
    sendTerminalInputFlush > sendTerminalInputFlushCondition &&
    sendTerminalInputQueue > sendTerminalInputFlush,
  "ordinary typed characters and cursor-key bursts should be batched briefly without trimming live output, while Enter/control submissions still flush immediately",
);

const runPendingCommandStart = terminalJs.indexOf("function runPendingTerminalCommand()");
const runPendingCommandGate = terminalJs.indexOf("if (!terminalInitialReplaySettled())", runPendingCommandStart);
const runPendingCommandClear = terminalJs.indexOf('state.pendingRunCommand = "";', runPendingCommandStart);
const runPendingCommandEnd = terminalJs.indexOf("\n}\n", runPendingCommandStart);
const runPendingCommandBody = terminalJs.slice(runPendingCommandStart, runPendingCommandEnd);
assert.ok(
  runPendingCommandStart >= 0 &&
    runPendingCommandGate > runPendingCommandStart &&
    runPendingCommandClear > runPendingCommandGate,
  "run-command startup should wait until the initial terminal replay has drained",
);
assert.match(
  runPendingCommandBody,
  /await sendTerminalAutoTypedInput\(command\)/,
  "run-command startup should use the backend preparation path so the current terminal environment is loaded",
);
assert.doesNotMatch(
  runPendingCommandBody,
  /sendTerminalInput\(command\)|MOBILE_KEY_SEQUENCES\.enter/,
  "run-command startup must not bypass current-environment preparation with direct terminal input",
);

const websocketOpenStart = terminalJs.search(/contextSocket\.addEventListener\("open",\s*(?:async\s*)?\(\) => \{/);
const websocketOpenFocus = terminalJs.indexOf("focusTerminalIfAllowed();", websocketOpenStart);
const websocketOpenStartup = terminalJs.indexOf("maybeRunTerminalStartupActions();", websocketOpenStart);
const websocketOpenAutoContinue = terminalJs.indexOf("maybeAutoContinueErroredSessions();", websocketOpenStart);
assert.ok(
  websocketOpenStart >= 0 &&
    websocketOpenFocus > websocketOpenStart &&
    websocketOpenStartup > websocketOpenFocus &&
    websocketOpenAutoContinue > websocketOpenStartup,
  "websocket open should defer quick-start/run-command work through the replay-settled startup gate",
);

assert.ok(
  terminalJs.includes("function restoreTerminalScrollPositionForSession(sessionId, { defaultToBottom = false } = {})"),
  "terminal page should be able to restore the last scroll position for the selected session",
);

const endReplayStart = terminalJs.indexOf("function endTerminalBacklogReplay(");
assert.ok(
  endReplayStart >= 0 &&
    terminalJs.indexOf("restoreTerminalScrollPositionForSession(context.sessionId, { defaultToBottom: true });", endReplayStart) > endReplayStart &&
    terminalJs.indexOf('terminalHost?.classList.remove("terminal-host-replaying");', endReplayStart) > endReplayStart &&
    terminalJs.indexOf("context.backlogReplayActive = false;", endReplayStart) > endReplayStart &&
    terminalJs.indexOf("hideTerminalSwitchPlaceholder();", endReplayStart) > endReplayStart &&
    terminalJs.indexOf("restoreSessionPageScrollIfActive();", endReplayStart) > endReplayStart &&
    terminalJs.indexOf("scheduleSessionPageScrollRestore();", endReplayStart) > endReplayStart,
  "backlog replay completion should reveal the terminal and remove the switch placeholder before final page-bottom settle",
);

const beginPageScrollRestoreStart = terminalJs.indexOf("function beginSessionPageScrollRestore(sessionId, connectionId = null)");
assert.ok(
  beginPageScrollRestoreStart >= 0 &&
    terminalJs.indexOf("snapshot: { atBottom: true },", beginPageScrollRestoreStart) > beginPageScrollRestoreStart &&
    terminalJs.indexOf("activeSessionPageScrollRestore = restoreState;", beginPageScrollRestoreStart) > beginPageScrollRestoreStart &&
    terminalJs.indexOf("return restoreState;", beginPageScrollRestoreStart) > beginPageScrollRestoreStart,
  "session switching should always create a page-bottom restore state so the page returns to the bottom after xterm replacement and replay settle, regardless of where the user had scrolled before switching",
);

const selectSessionStart = terminalJs.indexOf("function selectSession(sessionId, { connect = true, pushHistory = false } = {})");
assert.ok(
  selectSessionStart >= 0 &&
    terminalJs.indexOf("saveTerminalScrollPositionForSession(state.activeSessionId);", selectSessionStart) > selectSessionStart &&
    terminalJs.indexOf("const pageScrollRestore = beginSessionPageScrollRestore(session.id);", selectSessionStart) > selectSessionStart &&
    terminalJs.indexOf("state.activeSessionId = session.id;", selectSessionStart) > selectSessionStart &&
    terminalJs.indexOf("renderSessions();", selectSessionStart) > selectSessionStart &&
    terminalJs.indexOf("restoreSessionPageScrollIfActive(pageScrollRestore);", selectSessionStart) > selectSessionStart &&
    terminalJs.indexOf("connectTerminal();", selectSessionStart) > selectSessionStart,
  "session dropdown switches should restore the page bottom only after the active session has changed",
);

const connectTerminalStart = terminalJs.indexOf("function connectTerminal(targetContext = null)");
assert.ok(
  connectTerminalStart >= 0 &&
    terminalJs.indexOf("targetContext || activateTerminalSessionContext(state.activeSessionId)", connectTerminalStart) > connectTerminalStart &&
    terminalJs.indexOf("if (terminalContextSocketOpen(context))", connectTerminalStart) > connectTerminalStart &&
    terminalJs.indexOf("restoreCachedTerminalViewport(context);", connectTerminalStart) > connectTerminalStart &&
    terminalJs.indexOf("const token = ++context.connectionToken;", connectTerminalStart) > connectTerminalStart &&
    terminalJs.indexOf("bindSessionPageScrollRestoreToConnection(context.sessionId, token);", connectTerminalStart) > connectTerminalStart &&
    terminalJs.indexOf('updateStatus(`正在连接 ${session ? session.name : "终端"}…`, "info");', connectTerminalStart) > connectTerminalStart &&
    terminalJs.indexOf('showTerminalSwitchPlaceholder(`正在打开 ${session ? session.name : "终端"}…`);', connectTerminalStart) > connectTerminalStart &&
    terminalJs.indexOf("fitTerminal({ force: true });", connectTerminalStart) > connectTerminalStart &&
    terminalJs.indexOf("restoreSessionPageScrollIfActive();", connectTerminalStart) > connectTerminalStart,
  "terminal switching should reuse an open cached context and only show replay UI when a reconnect is required",
);
assert.doesNotMatch(
  terminalJs.slice(connectTerminalStart, selectSessionStart),
  /closeSocket\(|term\.reset\(\)/,
  "ordinary dropdown switching must keep the previous context alive and avoid resetting its xterm",
);

const restorePageScrollStart = terminalJs.indexOf("function restoreSessionPageScrollIfActive(restoreState = activeSessionPageScrollRestore)");
assert.ok(
  restorePageScrollStart >= 0 &&
    terminalJs.indexOf("if (terminalBacklogReplayActive)", restorePageScrollStart) > restorePageScrollStart &&
    terminalJs.indexOf("return;", restorePageScrollStart) > restorePageScrollStart &&
    terminalJs.indexOf("restorePageScrollSnapshotForLayout(restoreState.snapshot);", restorePageScrollStart) > restorePageScrollStart,
  "page-bottom restoration should not run while hidden backlog replay is still changing terminal state",
);

const showPlaceholderStart = terminalJs.indexOf('function showTerminalSwitchPlaceholder(text = "正在打开终端…")');
assert.ok(
  showPlaceholderStart >= 0 &&
    terminalJs.indexOf('const snapshotText = String(text || "正在打开终端…").trimEnd();', showPlaceholderStart) > showPlaceholderStart &&
    terminalJs.indexOf('terminalSwitchPlaceholderEl = document.createElement("pre");', showPlaceholderStart) > showPlaceholderStart &&
    terminalJs.indexOf('terminalSwitchPlaceholderEl.className = "terminal-switch-placeholder";', showPlaceholderStart) > showPlaceholderStart &&
    terminalJs.indexOf('terminalHost.classList.add("terminal-host-switching");', showPlaceholderStart) > showPlaceholderStart,
  "session switches should not show the previous terminal's visible text while the target session replays",
);

const resetReplayStart = terminalJs.indexOf("function resetTerminalBacklogReplay(context = activeTerminalContext)");
assert.ok(
  resetReplayStart >= 0 &&
    terminalJs.indexOf('terminalHost?.classList.remove("terminal-host-replaying");', resetReplayStart) > resetReplayStart &&
    terminalJs.indexOf("hideTerminalSwitchPlaceholder();", resetReplayStart) > resetReplayStart &&
    terminalJs.indexOf("updateTerminalScrollBottomButton();", resetReplayStart) > resetReplayStart,
  "resetting replay state should also remove the switch placeholder so interrupted switches cannot leave the terminal visually stuck",
);

const connectionErrorStart = terminalJs.indexOf('if (message?.type === "terminal_connection_error")');
assert.ok(
  connectionErrorStart >= 0 &&
    terminalJs.indexOf("resetTerminalBacklogReplay(context);", connectionErrorStart) > connectionErrorStart &&
    terminalJs.indexOf("resetTerminalOutputQueue(context);", connectionErrorStart) > connectionErrorStart &&
    terminalJs.indexOf('updateStatus(message.message || "终端连接失败。", "warn");', connectionErrorStart) > connectionErrorStart &&
    terminalJs.indexOf("return true;", connectionErrorStart) > connectionErrorStart,
  "terminal connection errors should clear hidden replay/switching state instead of leaving the page blank",
);

const clearConnectingStatusStart = terminalJs.indexOf("function clearConnectingStatusForSession(sessionId)");
assert.ok(
  clearConnectingStatusStart >= 0 &&
    terminalJs.indexOf('startsWith("正在连接")', clearConnectingStatusStart) > clearConnectingStatusStart &&
    terminalJs.indexOf("statusText.includes(sessionName)", clearConnectingStatusStart) > clearConnectingStatusStart &&
    terminalJs.indexOf('updateStatus("", "ok");', clearConnectingStatusStart) > clearConnectingStatusStart,
  "server-side connection confirmation should clear only the matching connecting status instead of hiding later quick-start prompts",
);

const scheduleSessionEventRefreshStart = terminalJs.indexOf("function scheduleSessionEventRefresh(");
assert.ok(
  scheduleSessionEventRefreshStart >= 0 &&
    terminalJs.indexOf("delayMs = TERMINAL_SESSION_EVENT_REFRESH_DELAY_MS", scheduleSessionEventRefreshStart) > scheduleSessionEventRefreshStart &&
    terminalJs.indexOf("if (sessionEventRefreshTimer !== null)", scheduleSessionEventRefreshStart) > scheduleSessionEventRefreshStart &&
    terminalJs.indexOf("if (delayMs > 0)", scheduleSessionEventRefreshStart) > scheduleSessionEventRefreshStart &&
    terminalJs.indexOf("window.clearTimeout(sessionEventRefreshTimer);", scheduleSessionEventRefreshStart) > scheduleSessionEventRefreshStart &&
    terminalJs.indexOf("sessionEventRefreshTimer = window.setTimeout(runRefresh, 0);", scheduleSessionEventRefreshStart) > scheduleSessionEventRefreshStart,
  "session connection/open events should be able to refresh immediately instead of waiting behind the passive session-event delay",
);

const connectedEventStart = terminalJs.indexOf('if (message.action === "connected")');
assert.ok(
  connectedEventStart >= 0 &&
    terminalJs.indexOf("if (connectedSessionId && context === activeTerminalContext)", connectedEventStart) > connectedEventStart &&
    terminalJs.indexOf("clearConnectingStatusForSession(connectedSessionId);", connectedEventStart) > connectedEventStart &&
    terminalJs.indexOf("preferredSessionId: connectedSessionId,", connectedEventStart) > connectedEventStart &&
    terminalJs.indexOf("preserveCurrentList: true,", connectedEventStart) > connectedEventStart &&
    terminalJs.indexOf("forcePreferredSession: true,", connectedEventStart) > connectedEventStart &&
    terminalJs.indexOf("0,", connectedEventStart) > connectedEventStart,
  "active-session connected events should refresh the just-created terminal instead of being ignored when the id already matches",
);

const openedEventStart = terminalJs.indexOf('message.action === "opened" &&');
assert.ok(
  openedEventStart >= 0 &&
    terminalJs.indexOf("clearConnectingStatusForSession(message.session_id);", openedEventStart) > openedEventStart &&
    terminalJs.indexOf("preferredSessionId: state.activeSessionId,", openedEventStart) > openedEventStart &&
    terminalJs.indexOf("preserveCurrentList: true,", openedEventStart) > openedEventStart &&
    terminalJs.indexOf("forcePreferredSession: true,", openedEventStart) > openedEventStart &&
    terminalJs.indexOf("0,", openedEventStart) > openedEventStart,
  "active-session opened events should also refresh the current terminal state promptly",
);

const explicitTargetStart = terminalJs.indexOf("const explicitTargetSessionIds = [");
assert.ok(
  explicitTargetStart >= 0 &&
    terminalJs.indexOf("stableCurrentSessionId", explicitTargetStart) > explicitTargetStart &&
    terminalJs.indexOf("forcePreferredSession || pushHistoryOnSelect", explicitTargetStart) > explicitTargetStart &&
    terminalJs.indexOf("[preferredSessionId, locationSessionId]", explicitTargetStart) > explicitTargetStart &&
    terminalJs.indexOf("[locationSessionId, preferredSessionId]", explicitTargetStart) > explicitTargetStart &&
    terminalJs.indexOf("const explicitTargetSessionId = explicitTargetSessionIds.find", explicitTargetStart) > explicitTargetStart &&
    terminalJs.indexOf("const pathPreferenceAllowed = !explicitTargetSessionId", explicitTargetStart) > explicitTargetStart &&
    terminalJs.indexOf("if (pathPreferenceAllowed && state.currentPath)", explicitTargetStart) > explicitTargetStart,
  "newly created or explicitly selected terminal sessions should win over the stale current URL before history is updated",
);

const createSessionStart = terminalJs.indexOf("async function createSession(");
assert.ok(
  createSessionStart >= 0 &&
    terminalJs.indexOf("prepareFreshTerminalDisplay(session);", createSessionStart) > createSessionStart &&
    terminalJs.indexOf("prepareFreshTerminalDisplay(session);", createSessionStart) <
      terminalJs.indexOf("selectSession(session.id, {", createSessionStart) &&
    terminalJs.indexOf("insertOrReplaceSession(session);", createSessionStart) > createSessionStart &&
    terminalJs.indexOf("selectSession(session.id, {", createSessionStart) > createSessionStart &&
    terminalJs.indexOf("connect: true,", createSessionStart) > createSessionStart &&
    terminalJs.indexOf("pushHistory: pushHistoryOnSelect,", createSessionStart) > createSessionStart &&
    terminalJs.indexOf("window.requestAnimationFrame(() => {", createSessionStart) > createSessionStart &&
    terminalJs.indexOf("preferredSessionId: session.id,", createSessionStart) > createSessionStart &&
    terminalJs.indexOf("preserveCurrentList: true,", createSessionStart) > createSessionStart &&
    terminalJs.indexOf("forcePreferredSession: true,", createSessionStart) > createSessionStart,
  "freshly created terminal sessions should clear old terminal output before selecting the new session and should stay selected even when the current URL still references an older session",
);

const prepareFreshDisplayStart = terminalJs.indexOf("function prepareFreshTerminalDisplay(session)");
assert.ok(
  prepareFreshDisplayStart >= 0 &&
    terminalJs.indexOf("disposeTerminalSessionContext(session.id);", prepareFreshDisplayStart) > prepareFreshDisplayStart &&
    terminalJs.indexOf("state.hasConnectedOnce = true;", prepareFreshDisplayStart) > prepareFreshDisplayStart,
  "explicit new-terminal creation should discard only a stale context with the same new id while preserving background terminals",
);

assert.match(
  terminalJs,
  /let term = null;[\s\S]*let fitAddon = null;[\s\S]*let terminalInstanceEventDisposables = \[\];/,
  "terminal instance and addons should be replaceable so explicit fresh creation can discard xterm's internal write queue",
);

assert.match(
  terminalJs,
  /function replaceTerminalInstance\(sessionId = state\.activeSessionId, \{ forceNew = false \} = \{\}\) \{[\s\S]*disposeTerminalSessionContext\(normalizedSessionId\);[\s\S]*activateTerminalSessionContext\(normalizedSessionId\)[\s\S]*scheduleTerminalSizeSettle\(\);[\s\S]*\}/,
  "explicit replacement should target one session context without disposing unrelated cached terminals",
);

// A list request that raced this page's create response may omit the fresh id.
// Only locally created ids awaiting their first server-list confirmation may
// be carried over. Missing ordinary ids are deleted sessions and must be
// dropped so a stale browser cannot reconnect and recreate them.
const activeAwaitingConfirmationAnchor = terminalJs.indexOf("activeAwaitingConfirmation");
assert.ok(
  activeAwaitingConfirmationAnchor >= 0 &&
    terminalJs.indexOf("state.pendingCreatedSessionIds.has(activeId)", activeAwaitingConfirmationAnchor) > activeAwaitingConfirmationAnchor &&
    terminalJs.indexOf("!isIdleSession(activeId)", activeAwaitingConfirmationAnchor) > activeAwaitingConfirmationAnchor &&
    terminalJs.indexOf("!fetchedSessionIds.has(activeId)", activeAwaitingConfirmationAnchor) > activeAwaitingConfirmationAnchor &&
    terminalJs.indexOf("state.sessions.find((session) => session.id === activeId)", activeAwaitingConfirmationAnchor) > activeAwaitingConfirmationAnchor &&
    terminalJs.indexOf("fetchedSessions.push(previousActive);", activeAwaitingConfirmationAnchor) > activeAwaitingConfirmationAnchor &&
    terminalJs.indexOf("state.pendingCreatedSessionIds.delete(activeId);", activeAwaitingConfirmationAnchor) >
      terminalJs.indexOf("fetchedSessions.push(previousActive);", activeAwaitingConfirmationAnchor) &&
    terminalJs.indexOf("state.sessions = sortSessionsByRecentActivity(fetchedSessions);", activeAwaitingConfirmationAnchor) >
      terminalJs.indexOf("state.pendingCreatedSessionIds.delete(activeId);", activeAwaitingConfirmationAnchor),
  "loadSessions should carry over only a locally created session awaiting confirmation, then trust later server lists",
);
