import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const settingsCore = readFileSync(
  new URL("../crates/settings_core/src/lib.rs", import.meta.url),
  "utf8"
);
const terminalSettings = readFileSync(
  new URL("../static/terminal-settings.js", import.meta.url),
  "utf8"
);
const manager = readFileSync(
  new URL("../src/terminal/manager.rs", import.meta.url),
  "utf8"
);
const routes = readFileSync(
  new URL("../src/routes/terminal.rs", import.meta.url),
  "utf8"
);
const terminalHandler = readFileSync(
  new URL("../src/terminal.rs", import.meta.url),
  "utf8"
);
const mobileKeys = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8"
);

test("default slash commands include /model in backend and frontend", () => {
  assert.match(
    settingsCore,
    /key:\s*"model"\.to_string\(\)[\s\S]*?label:\s*"\/model"\.to_string\(\)[\s\S]*?action:\s*"send_slash_command"\.to_string\(\)[\s\S]*?command:\s*"\/model"\.to_string\(\)/,
    "settings_core must declare /model in default_terminal_slash_commands"
  );
  assert.match(
    terminalSettings,
    /key:\s*"model",\s*label:\s*"\/model",\s*action:\s*"send_slash_command",\s*command:\s*"\/model"/,
    "terminal-settings.js must declare /model in default slash commands"
  );
});

test("backend slash command execution isolates and restores composer drafts", () => {
  const start = manager.indexOf("pub fn send_terminal_slash_command_with_delay(");
  assert.ok(start > 0, "send_terminal_slash_command_with_delay must exist in manager.rs");
  const fnBody = manager.slice(start, manager.indexOf("fn send_session_enter_key_or_direct(", start));
  assert.match(
    fnBody,
    /let draft_stash = self\.isolate_session_composer_draft\(session_id\);/,
    "slash commands must isolate user drafts before sending commands"
  );
  assert.match(
    fnBody,
    /self\.restore_session_composer_draft\(stash\);/,
    "slash commands must restore drafts once submission completes"
  );
  assert.match(
    fnBody,
    /self\.send_session_enter_key_or_direct\(session_id\)\?;/,
    "slash commands must submit Enter via the semantic enter key path"
  );
});

test("slash command route is registered and handled", () => {
  assert.match(
    routes,
    /\/api\/terminal\/sessions\/\{session_id\}\/slash-command/,
    "routes/terminal.rs must mount /api/terminal/sessions/{session_id}/slash-command"
  );
  assert.match(
    terminalHandler,
    /pub async fn send_session_slash_command\(/,
    "terminal.rs must export send_session_slash_command handler"
  );
});

test("frontend sendSlashCommand posts to slash-command endpoint with fallback", () => {
  const start = mobileKeys.indexOf("function sendSlashCommand(");
  assert.ok(start > 0, "sendSlashCommand must exist in terminal-mobile-keys.js");
  const fnBody = mobileKeys.slice(start, mobileKeys.indexOf("function runTerminalSlashCommandByKey(", start));
  assert.match(
    fnBody,
    /requestJson\(\s*`\/api\/terminal\/sessions\/\$\{encodeURIComponent\(requestSessionId\)\}\/slash-command`/,
    "frontend must invoke the slash-command endpoint"
  );
  assert.match(
    fnBody,
    /if \(res\?\.ok\)\s*\{\s*return;?\s*\}/,
    "frontend must return on successful endpoint dispatch"
  );
  assert.match(
    fnBody,
    /sendInput\(commandLine\)/,
    "frontend must preserve PTY input fallback for offline / mock test harnesses"
  );
});

test("slash commands bypass bracketed paste and settle for slash menu", () => {
  assert.match(
    manager,
    /if !bracketed_paste \|\| data\.trim_start\(\)\.starts_with\('\/'\) \{\s*return data\.to_string\(\);\s*\}/,
    "prepare_terminal_message_body must bypass bracketed paste for slash commands"
  );
  assert.match(
    manager,
    /if data\.trim_start\(\)\.starts_with\('\/'\) \{\s*return Duration::from_millis\(TERMINAL_SLASH_COMMAND_ENTER_DELAY_MS\);\s*\}/,
    "terminal_message_paste_settle_delay must settle before the first Enter for slash commands"
  );
});
