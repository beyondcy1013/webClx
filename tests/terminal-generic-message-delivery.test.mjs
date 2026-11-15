import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const manager = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");
const routes = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");

test("generic delivery forwards input without composer or Agent allowlist gates", () => {
  const start = manager.indexOf("fn run_generic(");
  assert.ok(start >= 0);
  const body = manager.slice(start, manager.indexOf("fn blocked(", start));
  assert.match(body, /write_body\(/);
  assert.match(body, /write_enter\(/);
  assert.doesNotMatch(body, /styled_snapshot\(|text_snapshot\(|write_clear\(|restore_draft\(|ensure_active\(/);
  assert.equal((body.match(/write_body\(/g) ?? []).length, 1);
  assert.equal((body.match(/write_enter\(/g) ?? []).length, 1);
  assert.match(body, /else\s*\{\s*"unverified"\s*\}/);
});

test("every Agent uses advisory extraction before one native submission", () => {
  const start = manager.indexOf("// Draft isolation is advisory for every Agent");
  const run = manager.slice(start, manager.indexOf("// 5) 恢复阶段", start));
  assert.ok(start >= 0);
  assert.match(run, /match self\.preflight\(PreflightPhase::BeforeIsolate\)/);
  assert.match(run, /Err\(error\) => Some\(format!\("draft extraction unavailable; continuing/);
  assert.match(run, /let mut outcome = self\.run_generic\(text\)\?/);
  assert.doesNotMatch(run, /return Ok\(blocked\)|return Ok\(self\.blocked|preflight\([^\n]*\)\?/);
});

test("immediate messages attempt draft extraction without propagating extraction errors", () => {
  const start = manager.indexOf("pub fn send_session_message(");
  const sender = manager.slice(start, manager.indexOf("fn session_composer_agent", start));
  assert.match(sender, /let draft_stash = if !data\.is_empty\(\) && submit_enters > 0/);
  assert.match(sender, /self\.isolate_session_composer_draft\(&session_id\)\s*\}/);
  assert.doesNotMatch(sender, /isolate_session_composer_draft\(&session_id\)\?/);
  assert.match(manager, /fn isolate_session_composer_draft\([\s\S]*?best_effort_composer_draft\(/);
});

test("immediate messages and independent Enter do not auto-retry unknown TUIs", () => {
  assert.match(routes, /if submitted\s*\|\| state\s*\.terminal_manager\s*\.session_message_uses_single_submission\(&sent\.session_id\)/);
  assert.match(routes, /if submitted\s*\|\| state\s*\.terminal_manager\s*\.session_message_uses_single_submission\(&session_id\)/);
  assert.match(manager, /pub fn session_message_uses_single_submission\([^}]*\{\s*true/);
  assert.match(manager, /pub fn retry_session_message_enter[\s\S]*submit_message_enter_unless_queued[\s\S]*self\.send_session_enter_key\(session_id\)/);
});
