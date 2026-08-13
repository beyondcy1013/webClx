import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const terminalSource = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalPageSource = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);
const homePageSource = readFileSync(
  new URL("../static/app-session-actions.js", import.meta.url),
  "utf8",
);

function functionBlock(source, signature, nextSignature) {
  const start = source.indexOf(signature);
  const end = nextSignature
    ? source.indexOf(nextSignature, start + signature.length)
    : source.length;
  assert.ok(start >= 0, `missing ${signature}`);
  assert.ok(end > start, `missing boundary ${nextSignature || "<eof>"}`);
  return source.slice(start, end);
}

test("terminal deletion requires an explicit target-id confirmation header", () => {
  const handler = functionBlock(
    terminalSource,
    "pub async fn delete_session(",
    "pub async fn list_resume_archives(",
  );

  assert.match(terminalSource, /const TERMINAL_DELETE_CONFIRM_HEADER: &str = "x-webclx-confirm-session";/);
  assert.match(handler, /headers: HeaderMap/);
  assert.match(
    handler,
    /if let Err\(error\) = require_terminal_delete_confirmation\(&headers, &session_id\)/,
  );
  assert.match(
    terminalSource,
    /fn require_terminal_delete_confirmation\([\s\S]*TERMINAL_DELETE_CONFIRM_HEADER[\s\S]*confirmed_session_id != session_id[\s\S]*Err\(AppError::bad_request/,
  );
});

test("terminal deletion emits request, success, and failure audit events", () => {
  const handler = functionBlock(
    terminalSource,
    "pub async fn delete_session(",
    "pub async fn list_resume_archives(",
  );

  assert.match(handler, /ConnectInfo\(client_addr\): ConnectInfo<SocketAddr>/);
  assert.match(handler, /terminal_delete_audit_context\(&state, &headers, client_addr\)/);
  assert.match(handler, /"terminal session delete requested"/);
  assert.match(handler, /"terminal session deleted"/);
  assert.match(handler, /"terminal session delete failed"/);
  assert.match(handler, /requester = %audit\.requester/);
  assert.match(handler, /client_addr = %audit\.client_addr/);
  assert.match(handler, /user_agent = %audit\.user_agent/);
  assert.match(handler, /request_source = %audit\.request_source/);
  assert.match(handler, /target_session_id = %session_id/);
  assert.match(handler, /target_session_name = %session\.name/);
});

test("both terminal WebUI delete actions bind confirmation to the selected session", () => {
  const terminalDelete = functionBlock(
    terminalPageSource,
    "async function deleteSession(session)",
    "function refreshTerminalViewportLayout(",
  );
  const homeDelete = functionBlock(
    homePageSource,
    "async function deleteSession(session)",
    "",
  );

  assert.match(
    terminalDelete,
    /"X-WebClx-Confirm-Session": session\.id[\s\S]*"X-WebClx-Delete-Source": "terminal-page"/,
  );
  assert.match(
    homeDelete,
    /"X-WebClx-Confirm-Session": session\.id[\s\S]*"X-WebClx-Delete-Source": "home-sessions"/,
  );
});
