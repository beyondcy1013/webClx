import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const terminalSessionActivity = require("../static/terminal-session-activity.js");
const terminalRs = ["../src/terminal.rs", "../src/terminal/websocket.rs"]
  .map((file) => readFileSync(new URL(file, import.meta.url), "utf8")).join("\n");
const terminalInputTransport = readFileSync(
  new URL("../static/terminal-input-transport.js", import.meta.url),
  "utf8",
);
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalManagerRs = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");
const socketLoopStart = terminalRs.indexOf("async fn handle_socket(");
const socketLoopEnd = terminalRs.indexOf("async fn send_terminal_output_chunk", socketLoopStart);
const socketLoop = terminalRs.slice(socketLoopStart, socketLoopEnd);
const getForConnectionStart = terminalManagerRs.indexOf("pub(super) fn get_for_connection");
const getForConnectionEnd = terminalManagerRs.indexOf("fn create_session_inner", getForConnectionStart);
const getForConnectionSource = terminalManagerRs.slice(getForConnectionStart, getForConnectionEnd);

assert.ok(socketLoopStart >= 0 && socketLoopEnd > socketLoopStart, "terminal socket loop should exist");
assert.match(
  socketLoop,
  /let mut pending_output_viewed_mark = false;[\s\S]*output_viewed_mark_interval\.tick\(\), if pending_output_viewed_mark[\s\S]*manager\.mark_session_output_viewed_in_memory\(&session\.id\);[\s\S]*pending_output_viewed_mark = false;/,
  "connected terminal output should receive an in-memory trailing viewed acknowledgement after a burst",
);
assert.match(
  socketLoop,
  /send_terminal_output_chunk\(&mut sender, &chunk[^)]*\)\.await\.is_err\(\)[\s\S]{0,320}if output_visible\.load\([\s\S]*pending_output_viewed_mark = true;/,
  "successfully delivered live output should schedule a viewed acknowledgement only for a visible socket",
);
assert.match(
  socketLoop,
  /send_terminal_output_chunk\(&mut sender, &recovered[^)]*\)\.await\.is_err\(\)[\s\S]{0,320}if output_visible\.load\([\s\S]*pending_output_viewed_mark = true;/,
  "successfully delivered recovered output should schedule a viewed acknowledgement only for a visible socket",
);
assert.match(
  terminalRs,
  /enum ClientMessage \{[\s\S]*Visibility \{ visible: bool \}/,
  "the terminal websocket protocol should accept foreground/background visibility changes",
);
assert.match(
  terminalInputTransport,
  /function terminalContextOutputVisible\(context\)[\s\S]*context === activeTerminalContext[\s\S]*document\.visibilityState === "visible"/,
  "a selected terminal in a hidden browser tab must not be treated as viewed",
);
assert.match(
  terminalJs,
  /document\.addEventListener\("visibilitychange"[\s\S]{0,500}syncActiveTerminalContextOutputVisibility\(\)/,
  "browser visibility changes must be synchronized to the terminal websocket",
);
assert.match(
  terminalRs,
  /ClientMessage::Visibility \{ visible \} => \{[\s\S]{0,900}output_visible\.swap\(visible,[\s\S]{0,900}if visible \{[\s\S]{0,500}manager\.mark_session_opened\(&session_id\);[\s\S]{0,500}manager\.mark_session_output_viewed\(&session_id\)[\s\S]{0,900}else if was_visible \{[\s\S]{0,500}manager\.mark_session_output_viewed\(&session_id\)/,
  "visibility transitions should acknowledge newly foregrounded output and persist output viewed before switching away",
);
assert.match(
  getForConnectionSource,
  /self\.get_session\([\s\S]*output_visible,[\s\S]*self\.get_or_create_latest\([\s\S]*output_visible,[\s\S]*fn get_session\([\s\S]*output_visible: bool[\s\S]*let opened = output_visible && mark_session_opened_locked\(&mut state, &stored\.id\)[\s\S]*fn get_or_create_latest\([\s\S]*output_visible: bool[\s\S]*let opened = output_visible && mark_session_opened_locked\(&mut state, &stored\.id\)/,
  "background preconnect sockets must not update recent-use timestamps until they become visible",
);
assert.match(
  socketLoop,
  /'socket_loop: loop \{[\s\S]*\n\s*\}[\s\S]*if output_visible\.load\([\s\S]*manager\.mark_session_output_viewed\(&session\.id\);[\s\S]*\}/,
  "closing a visible socket should persist its final viewed acknowledgement even after an in-memory acknowledgement",
);
assert.doesNotMatch(
  socketLoop,
  /last_output_viewed_mark\.elapsed\(\)/,
  "viewed acknowledgement should not depend on another output chunk arriving after the throttle interval",
);
assert.equal(
  terminalSessionActivity.sessionActivityLabel(
    { activity_state: "idle", last_output_at: 1000 },
    1000,
  ),
  "空闲",
  "a viewed session should stay idle even when its last output is inside the recent-output window",
);
assert.equal(
  terminalSessionActivity.sessionActivityLabel(
    { activity_state: "recent_output", last_output_at: 1000 },
    1000,
  ),
  "输出中",
  "only the backend recent-output state should render as active output",
);
assert.match(
  terminalManagerRs,
  /let now = current_timestamp_millis\(\);\s*if last_output_at > last_viewed_output_at[\s\S]*now\.saturating_sub\(last_output_at\) <= TERMINAL_RECENT_OUTPUT_ACTIVE_MS[\s\S]*TerminalActivitySnapshot::recent_output\(last_output_at\)/,
  "backend recent output should require output newer than the viewed timestamp",
);

console.log("terminal output viewed acknowledgement tests passed");
