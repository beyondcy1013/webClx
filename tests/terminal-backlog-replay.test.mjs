import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalCss = readFileSync(new URL("../static/styles-terminal.css", import.meta.url), "utf8");
const terminalTmuxRs = readFileSync(new URL("../src/terminal/tmux.rs", import.meta.url), "utf8");
const terminalSessionRs = readFileSync(new URL("../src/terminal/session.rs", import.meta.url), "utf8");

assert.match(
  terminalRs,
  /send_backlog_replay_control\(&mut sender,\s*"start"\)[\s\S]*send_binary_chunks\(&mut sender,\s*&backlog\)[\s\S]*send_backlog_replay_control\(&mut sender,\s*"end"\)/,
  "terminal websocket should bracket the initial backlog replay with start/end controls",
);

assert.match(
  terminalRs,
  /#\[serde\(rename = "terminal_backlog_replay"\)\][\s\S]*#\[serde\(tag = "type", rename_all = "snake_case"\)\][\s\S]*struct TerminalBacklogReplayControl/,
  "terminal backlog replay controls should serialize with the frontend protocol type instead of the Rust struct name",
);

assert.match(
  terminalRs,
  /#\[serde\(rename = "terminal_connection_error"\)\][\s\S]*#\[serde\(tag = "type", rename_all = "snake_case"\)\][\s\S]*struct TerminalConnectionError/,
  "terminal connection errors should serialize with the frontend protocol type",
);

assert.match(
  terminalJs,
  /if \(handleTerminalBacklogReplayControl\(message, token, context\)\) \{[\s\S]*return true;[\s\S]*\}/,
  "terminal page should handle backlog replay control messages before session mutation messages",
);

assert.match(
  terminalJs,
  /function endTerminalBacklogReplay\([\s\S]*context = activeTerminalContext[\s\S]*restoreTerminalScrollPositionForSession\(context\.sessionId, \{ defaultToBottom: true \}\);[\s\S]*terminalHost\?\.classList\.remove\("terminal-host-replaying"\)/,
  "terminal page should restore the selected session scroll position before revealing a replayed terminal",
);

assert.match(
  terminalJs,
  /function beginTerminalBacklogReplay\(context = activeTerminalContext\) \{[\s\S]*context\.backlogReplayActive = true;[\s\S]*hideTerminalCursorCorrection\(\);[\s\S]*terminalHost\?\.classList\.add\("terminal-host-replaying"\)/,
  "backlog replay should clear cursor correction before replay frames start changing the terminal buffer",
);

assert.match(
  terminalJs,
  /context\.term\.write\(nextItem\.bytes, \(\) => \{[\s\S]*if \(!context\.backlogReplayActive\) \{[\s\S]*syncTerminalCursorCorrection\(\);[\s\S]*\}[\s\S]*drainTerminalOutputQueue\(context\);[\s\S]*\}\);/,
  "backlog replay should not sync cursor correction from unstable intermediate replay frames",
);

assert.match(
  terminalJs,
  /function scheduleTerminalRenderRefresh\(context = activeTerminalContext\) \{[\s\S]*context\.renderRefreshFrame = window\.requestAnimationFrame\([\s\S]*context\.term\.refresh\(0, rows - 1\)/,
  "terminal output should coalesce an explicit full-viewport refresh for renderers that miss paint invalidation",
);

assert.match(
  terminalJs,
  /context\.term\.write\(nextItem\.bytes, \(\) => \{[\s\S]*scheduleTerminalRenderRefresh\(context\);[\s\S]*drainTerminalOutputQueue\(context\);[\s\S]*\}\);/,
  "every completed terminal write should schedule a visible refresh without requiring user input",
);

assert.match(
  terminalJs,
  /function endTerminalBacklogReplay\([\s\S]*scheduleTerminalRenderRefresh\(context\);[\s\S]*terminalHost\?\.classList\.remove\("terminal-host-replaying"\)/,
  "finishing backlog replay should refresh the terminal before revealing it",
);

assert.match(
  terminalJs,
  /function queueTerminalBacklogReplayEnd\(token, context = activeTerminalContext\) \{[\s\S]*flushCodexStatusOutputTransformer\(context\)[\s\S]*context\.outputQueue\.push\(\{ kind: "backlog_replay_end", token \}\);[\s\S]*drainTerminalOutputQueue\(context\);[\s\S]*\}/,
  "backlog replay end should flush transformed output and stay queued behind already-received replay bytes",
);

assert.match(
  terminalJs,
  /if \(nextItem\.kind === "backlog_replay_end"\) \{[\s\S]*endTerminalBacklogReplay\(\{\}, context\);[\s\S]*drainTerminalOutputQueue\(context\);[\s\S]*return;[\s\S]*\}/,
  "the queued replay end marker should run only after earlier terminal output items have rendered",
);

assert.match(
  terminalJs,
  /function queueTerminalOutput\(bytes, token, context = activeTerminalContext\) \{[\s\S]*transformTerminalSynchronizedOutput\(bytes, context\)[\s\S]*const replay = context\.backlogReplayActive && !context\.backlogReplayEndQueued;[\s\S]*replay[\s\S]*transformTerminalCodexStatusOutput\(synchronizedBytes, context\)[\s\S]*: synchronizedBytes;[\s\S]*context\.outputQueue\.push\(\{ kind: "output", bytes: transformedBytes, token, replay \}\);[\s\S]*drainTerminalOutputQueue\(context\);[\s\S]*\}/,
  "backlog output may be compacted after synchronized redraws while live TUI bytes remain unchanged",
);

assert.match(
  terminalJs,
  /function interruptTerminalBacklogReplayForInput\(\) \{[\s\S]*context\.backlogReplayInterrupted = true;[\s\S]*context\.outputQueue = context\.outputQueue\.filter\(\(item\) => !item\.replay\);[\s\S]*endTerminalBacklogReplay\(\{ preserveInterrupted: true \}, context\);[\s\S]*\}/,
  "typing during a large replay should drop queued replay output so live input echo is not stuck behind old history",
);

assert.match(
  terminalJs,
  /function terminalInitialReplaySettled\(context = activeTerminalContext\) \{[\s\S]*!context\.initialReplayPending[\s\S]*!context\.backlogReplayActive[\s\S]*!context\.backlogReplayEndQueued[\s\S]*!context\.outputWriteInFlight[\s\S]*!context\.outputQueue\.some\(\(item\) => item\.replay \|\| item\.kind === "backlog_replay_end"\)/,
  "terminal startup actions should wait until the initial replay has fully drained from the xterm write queue",
);

assert.match(
  terminalJs,
  /function interruptTerminalBacklogReplayForInput\(\) \{[\s\S]*if \(context\.initialReplayPending\) \{[\s\S]*context\.initialReplayPending = false;[\s\S]*context\.backlogReplayInterrupted = true;[\s\S]*context\.outputQueue = context\.outputQueue\.filter\(\(item\) => !item\.replay\);[\s\S]*\}/,
  "typing before the replay start control arrives should still interrupt the pending initial replay",
);

assert.match(
  terminalJs,
  /function sendTerminalInput\(data, options = \{\}\) \{[\s\S]*interruptTerminalBacklogReplayForInput\(\);[\s\S]*(?:flushTerminalInputQueue|queueTerminalInput)\(/,
  "terminal input should interrupt pending backlog replay before sending live PTY input",
);

assert.match(
  terminalCss,
  /\.terminal-host\.terminal-host-replaying \.xterm \{\s*opacity: 0;\s*\}/,
  "hidden backlog replay should keep xterm interactive so typing can interrupt replay",
);

assert.doesNotMatch(
  terminalCss,
  /\.terminal-host\.terminal-host-replaying \.xterm \{[\s\S]*visibility:\s*hidden/,
  "hidden backlog replay must not use visibility:hidden because that prevents focusing xterm during a switch",
);

assert.match(
  terminalJs,
  /scrollback:\s*normalizeTerminalScrollbackLines\(state\.terminalScrollbackLines\)/,
  "terminal page should create xterm with the configured scrollback line limit",
);

assert.match(
  terminalRs,
  /const MAX_BACKLOG_BYTES:\s*usize\s*=\s*32\s*\*\s*1024\s*\*\s*1024;/,
  "terminal backend should retain a larger byte backlog for reconnect fallback",
);

assert.match(
  terminalSessionRs,
  /const TERMINAL_OUTPUT_CHANNEL_CAPACITY:\s*usize\s*=\s*4096;[\s\S]*broadcast::channel\(TERMINAL_OUTPUT_CHANNEL_CAPACITY\)/,
  "terminal live output broadcast should have enough room for short high-volume bursts",
);

assert.match(
  terminalSessionRs,
  /struct TerminalOutputChunk \{[\s\S]*seq: u64,[\s\S]*bytes: Vec<u8>,[\s\S]*\}[\s\S]*pub\(super\) fn backlog_chunks_after\(&self, seq: u64\) -> Vec<TerminalOutputChunk>/,
  "terminal output chunks should carry sequence numbers so lagged websocket consumers can recover from backlog",
);

assert.match(
  terminalRs,
  /let mut last_output_seq_sent = live_output_start_seq;[\s\S]*Err\(broadcast::error::RecvError::Lagged\(skipped\)\) => \{[\s\S]*session\.backlog_chunks_after\(last_output_seq_sent\)[\s\S]*send_terminal_output_chunk\(&mut sender, &recovered\)\.await[\s\S]*last_output_seq_sent = recovered\.seq;/,
  "terminal websocket lag should recover missing live output from the session backlog instead of silently skipping chunks",
);

assert.doesNotMatch(
  terminalRs,
  /Err\(broadcast::error::RecvError::Lagged\(skipped\)\) => \{\s*warn!\("terminal websocket lagged, skipped \{skipped\} chunks"\);\s*\}/,
  "terminal websocket lag must not be handled by only logging skipped chunks",
);

assert.match(
  terminalTmuxRs,
  /const TMUX_HISTORY_LIMIT:\s*&str\s*=\s*"100000";[\s\S]*"history-limit"[\s\S]*TMUX_HISTORY_LIMIT/,
  "tmux sessions should retain enough history to back long terminal replays",
);

assert.match(
  terminalTmuxRs,
  /const INITIAL_TMUX_SNAPSHOT_LINE_LIMIT:\s*u32\s*=\s*800;[\s\S]*pub\(super\) fn capture_tmux_initial_pane_snapshot\(session_id: &str\)[\s\S]*format!\("-\{INITIAL_TMUX_SNAPSHOT_LINE_LIMIT\}"\)/,
  "initial terminal websocket replay should use a bounded tmux snapshot instead of sending the entire 100000-line history",
);

assert.match(
  terminalSessionRs,
  /fn initial_backend_snapshot\(session_id: &str\) -> Option<Vec<u8>> \{[\s\S]*capture_tmux_initial_pane_snapshot\(session_id\)/,
  "attaching a tmux session should seed the browser from the bounded initial snapshot",
);

assert.match(
  terminalRs,
  /const MAX_INITIAL_BACKLOG_BYTES:\s*usize\s*=\s*1024\s*\*\s*1024;[\s\S]*capture_tmux_initial_pane_snapshot\(&session\.id\)[\s\S]*unwrap_or_else\(\|\| session\.backlog_tail_snapshot\(MAX_INITIAL_BACKLOG_BYTES\)\)/,
  "websocket reconnect fallback should cap live backlog replay so input echo is not stuck behind a huge initial write",
);
