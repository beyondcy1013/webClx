import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalRs = ["../src/terminal.rs", "../src/terminal/websocket.rs"]
  .map((file) => readFileSync(new URL(file, import.meta.url), "utf8")).join("\n");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalCss = readFileSync(new URL("../static/styles-terminal.css", import.meta.url), "utf8");
const terminalTmuxRs = readFileSync(new URL("../src/terminal/tmux.rs", import.meta.url), "utf8");
const terminalSessionRs = readFileSync(new URL("../src/terminal/session.rs", import.meta.url), "utf8");

assert.match(
  terminalRs,
  /send_backlog_replay_control\([\s\S]*"start"[\s\S]*\)[\s\S]*send_binary_chunks\(\s*&mut sender,[\s\S]*&replay\.bytes[\s\S]*send_backlog_replay_control\([\s\S]*"end"/,
  "terminal websocket should bracket the initial backlog replay with start/end controls",
);

assert.match(
  terminalRs,
  /session\.current_viewport_size\(\)[\s\S]*send_terminal_viewport_size\(&mut sender, size\)[\s\S]*send_backlog_replay_control\([\s\S]*"start"/,
  "a reconnecting xterm must adopt the shared PTY grid before parsing its initial replay",
);
assert.match(
  terminalRs,
  /Ok\(TerminalStreamEvent::ViewportSize\(size\)\)[\s\S]*send_terminal_viewport_size\(&mut sender, size\)[\s\S]*Ok\(TerminalStreamEvent::Output\(chunk\)\)/,
  "live viewport controls and terminal bytes should retain their shared broadcast order",
);
assert.match(
  terminalSessionRs,
  /enum TerminalStreamEvent \{[\s\S]*Output\(TerminalOutputChunk\),[\s\S]*ViewportSize\(TerminalViewportSize\),[\s\S]*\}/,
  "shared PTY size changes should travel through the same stream as terminal output",
);
assert.match(
  terminalSessionRs,
  /broadcaster\.send\(TerminalStreamEvent::ViewportSize\(size\)\)[\s\S]*\.resize\(PtySize \{[\s\S]*rows: size\.rows,[\s\S]*cols: size\.cols/,
  "clients must receive the canonical grid before the PTY starts producing output for that grid",
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
  /function beginTerminalBacklogReplay\(context = activeTerminalContext[^)]*\) \{[\s\S]*context\.backlogReplayActive = true;[\s\S]*hideTerminalCursorCorrection\(\);[\s\S]*terminalHost\?\.classList\.add\("terminal-host-replaying"\)/,
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
  /function terminalInitialReplaySettled\(context = activeTerminalContext\) \{[\s\S]*!context\.initialReplayPending[\s\S]*!context\.backlogReplayActive[\s\S]*!context\.backlogReplayEndQueued[\s\S]*!context\.outputReplayWriteInFlight[\s\S]*!context\.outputQueue\.some\(\(item\) => item\.replay \|\| item\.kind === "backlog_replay_end"\)/,
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
  /let mut last_output_seq_sent = replay_last_seq;[\s\S]*Err\(broadcast::error::RecvError::Lagged\(skipped\)\) => \{[\s\S]*session\.backlog_chunks_after\(last_output_seq_sent\)[\s\S]*send_terminal_output_chunk\(&mut sender, &recovered[\s\S]*last_output_seq_sent = recovered\.seq;/,
  "terminal websocket lag should recover missing live output from the session backlog instead of silently skipping chunks",
);

assert.match(
  terminalRs,
  /initial_replay_for_socket\(&session\)\.await[\s\S]*let mut last_output_seq_sent = replay_last_seq;/,
  "initial replay must hand live output off after the snapshot boundary so dynamic terminal bytes are neither lost nor applied twice",
);
assert.match(
  terminalRs,
  /let seq_before = session\.current_output_seq\(\);[\s\S]*capture_tmux_initial_pane_snapshot_async\(&session\.id, line_limit\)\.await[\s\S]*session\.backlog_chunks_after\(seq_before\)[\s\S]*request_tmux_client_refresh\(&session\.id\)\.await/,
  "initial replay should watermark before the tmux capture, append post-watermark live chunks, then force a client redraw",
);
assert.match(
  terminalRs,
  /TERMINAL_INITIAL_SNAPSHOT_LINE_LIMIT\.load\(Ordering::SeqCst\)[\s\S]*capture_tmux_initial_pane_snapshot_async\(&session\.id, line_limit\)/,
  "the per-connection tmux snapshot should use the configured scrollback line limit",
);
assert.match(
  terminalRs,
  /session\.backlog_chunks_after\(seq_before\)[\s\S]*bytes\.extend_from_slice\(&chunk\.bytes\)[\s\S]*last_output_seq = chunk\.seq/,
  "live chunks recorded after the watermark must be appended after the fresh snapshot and advance the live boundary",
);
assert.match(
  terminalRs,
  /fn initial_replay_for_socket[\s\S]*session\.is_alive\(\)[\s\S]*Ok\(buffered_replay_for_socket\(session\)\)/,
  "non-alive sessions or failed captures must fall back to the buffered replay",
);
assert.match(
  terminalTmuxRs,
  /async fn request_tmux_client_refresh\(session_id: &str\)[\s\S]*"list-clients"[\s\S]*"#\{client_name\}"[\s\S]*"refresh-client"[\s\S]*"-t"[\s\S]*client_name/,
  "the refresh backstop must target each attached client name reported by list-clients",
);
assert.match(
  terminalRs,
  /fn buffered_replay_for_socket[\s\S]*session\.backlog_tail_snapshot_with_last_seq\(MAX_INITIAL_REPLAY_BYTES\)[\s\S]*last_output_seq/,
  "the buffered fallback replay must include output recorded after the attach-time snapshot before advancing its live boundary",
);
assert.doesNotMatch(
  terminalRs,
  /let live_output_start_seq = session\.current_output_seq\(\);\s*let backlog = initial_backlog_for_socket\(&session\);/,
  "capturing the live sequence before the snapshot duplicates concurrent cursor-control output",
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
  /pub\(super\) fn capture_tmux_initial_pane_snapshot_with_limit\([\s\S]*line_limit:\s*u32,[\s\S]*format!\("-\{line_limit\}"\)/,
  "initial terminal websocket replay should use the configured scrollback lines",
);

assert.match(
  terminalSessionRs,
  /fn initial_backend_snapshot\(session_id: &str\) -> Option<Vec<u8>> \{[\s\S]*capture_tmux_initial_pane_snapshot_with_limit\(session_id, line_limit\)/,
  "attaching a tmux session should seed the browser from the bounded initial snapshot",
);

assert.match(
  terminalRs,
  /const MAX_INITIAL_REPLAY_BYTES:\s*usize\s*=\s*8\s*\*\s*1024\s*\*\s*1024;[\s\S]*backlog_tail_snapshot_with_last_seq\(MAX_INITIAL_REPLAY_BYTES\)/,
  "websocket reconnect fallback should cap live backlog replay so input echo is not stuck behind a huge initial write",
);

// ── 断线 seq 水位增量续传（App 切换回来不再清空终端全量重拉） ──────────────

assert.match(
  terminalRs,
  /struct TerminalSessionQuery \{[\s\S]*seq_framed[\s\S]*resume_seq:\s*Option<u64>[\s\S]*resume_instance:\s*Option<u64>/,
  "terminal websocket query should carry the client's seq watermark and stream instance",
);

assert.match(
  terminalRs,
  /fn delta_replay_chunks[\s\S]*resume_instance != session\.output_instance_id\(\)[\s\S]*resume_seq > session\.current_output_seq\(\)[\s\S]*first\.seq == resume_seq \+ 1/,
  "delta resume must require a matching stream instance, an in-range seq watermark, and a contiguous backlog tail",
);

assert.match(
  terminalRs,
  /fn delta_replay_chunks[\s\S]*if !query\.seq_framed \{\s*return None;/,
  "delta resume must be gated on seq-frame negotiation so clients without framing never receive framed output",
);

assert.match(
  terminalRs,
  /send_output_framing_control\(&mut sender\)[\s\S]*send_backlog_replay_control\([\s\S]*"start"/,
  "the seq-framing handshake must precede every binary output frame on the socket",
);

assert.match(
  terminalRs,
  /enum InitialReplayPlan \{[\s\S]*Full\(TerminalInitialReplay\)[\s\S]*Delta \{[\s\S]*chunks: Vec<TerminalOutputChunk>/,
  "reconnect should choose between a full snapshot replay and a delta chunk resume",
);

assert.match(
  terminalSessionRs,
  /output_instance_id:\s*u64/,
  "each terminal session must carry a fresh output-stream instance id so rebuilt sessions cannot be mistaken for the old stream",
);

assert.match(
  terminalJs,
  /query\.set\("seq_framed", "1"\)[\s\S]*query\.set\("resume_seq", String\(context\.lastOutputSeq\)\)[\s\S]*query\.set\("resume_instance", String\(context\.outputInstanceId\)\)/,
  "terminal page should advertise seq framing and pass its applied watermark when reconnecting",
);

assert.match(
  terminalJs,
  /const resumeWanted =[\s\S]*context\.disconnected[\s\S]*context\.hasLoadedOutput[\s\S]*!context\.outputStreamGap[\s\S]*context\.pendingDeltaReplay = resumeWanted/,
  "terminal reconnect should defer the buffer reset until the server answers whether delta resume is honored",
);

assert.match(
  terminalJs,
  /if \(resumeAttempted && !deltaReplay\) \{[\s\S]*resetTerminalContextInstance\(context\);/,
  "a refused delta resume must still rebuild the terminal before full replay bytes arrive",
);

assert.match(
  terminalJs,
  /context\.seqFramingActive[\s\S]*new DataView\(bytes\.buffer, bytes\.byteOffset, bytes\.byteLength\)\.getBigUint64\(0, false\)[\s\S]*bytes\.subarray\(8\)/,
  "framed binary output must strip the 8-byte seq header before writing to xterm",
);

assert.match(
  terminalJs,
  /context\.outputStreamGap = true;[\s\S]*context\.outputQueue = context\.outputQueue\.filter\(\(item\) => !item\.replay\)/,
  "an interrupted replay must mark the buffer as an invalid delta-resume baseline",
);
