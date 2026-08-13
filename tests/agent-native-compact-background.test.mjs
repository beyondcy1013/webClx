import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const agentSource = readFileSync(new URL("../src/agent.rs", import.meta.url), "utf8");
const routeSource = readFileSync(new URL("../src/routes/agent.rs", import.meta.url), "utf8");
const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");

for (const toolName of [
  "start_background_command",
  "read_background_command",
  "write_background_command",
  "terminate_background_command",
]) {
  assert.match(agentSource, new RegExp(`\\"name\\": \\"${toolName}\\"`), `missing ${toolName} tool`);
}

assert.match(agentSource, /pub context_summary: Option<String>/);
assert.match(agentSource, /pub compacted_messages: u64/);
assert.match(agentSource, /model_context_window/);
assert.match(agentSource, /model_auto_compact_token_limit/);
assert.match(agentSource, /ContextStatus/);
assert.match(agentSource, /CompactStart/);
assert.match(agentSource, /CompactDone/);

assert.match(routeSource, /\/api\/agent\/sessions\/\{session_id\}\/context/);
assert.match(routeSource, /\/api\/agent\/sessions\/\{session_id\}\/compact/);
assert.match(routeSource, /\/api\/agent\/sessions\/\{session_id\}\/commands/);
assert.match(routeSource, /\/api\/agent\/sessions\/\{session_id\}\/commands\/\{command_id\}\/stdin/);
assert.match(routeSource, /\/api\/agent\/sessions\/\{session_id\}\/commands\/\{command_id\}\/terminate/);

assert.match(agentHtml, /id="agent-context-status"/);
assert.match(agentHtml, /id="agent-context-progress"/);
assert.match(agentHtml, /id="agent-compact-btn"[^>]*>压缩</);
assert.match(agentHtml, /\/api\/agent\/sessions\/\$\{currentSessionId\}\/context/);
assert.match(agentHtml, /\/api\/agent\/sessions\/\$\{currentSessionId\}\/compact/);
assert.match(agentHtml, /event\.type === "context_status"/);
assert.match(agentHtml, /event\.type === "compact_start"/);
assert.match(agentHtml, /event\.type === "compact_done"/);

console.log("native Agent compact, context status, and background command contracts passed");
