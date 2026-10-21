import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const terminalRs = readFileSync(
  new URL("../src/terminal.rs", import.meta.url),
  "utf8",
);

test("auto-typed commands submit through a separate semantic Enter key", () => {
  const start = terminalRs.indexOf("pub async fn send_auto_typed_input(");
  const end = terminalRs.indexOf("pub async fn send_session_message(", start);
  assert.ok(start >= 0 && end > start, "auto-typed handler must exist");
  const handler = terminalRs.slice(start, end);

  assert.match(
    handler,
    /trim_end_matches\(\['\\r', '\\n'\]\)/,
    "the prepared trailing newline must not be written with the command body",
  );
  assert.match(
    handler,
    /send_session_input_silent\(&session_id_for_send, command_body\)/,
    "the command body must be written separately",
  );
  assert.match(
    handler,
    /send_session_enter_key\(&session_id_for_send\)/,
    "submission must use the same semantic Enter path as the soft keyboard",
  );
  assert.doesNotMatch(
    handler,
    /send_session_input_silent\(&session_id_for_send, "\\r"\.to_string\(\)\)/,
    "auto-typed submissions must not retry by writing a raw PTY carriage return",
  );
});
