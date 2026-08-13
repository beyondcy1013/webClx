import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const agentSource = readFileSync(new URL("../src/agent.rs", import.meta.url), "utf8");
const llmSource = readFileSync(new URL("../src/llm.rs", import.meta.url), "utf8");
const backgroundSource = readFileSync(
  new URL("../src/agent/background_commands.rs", import.meta.url),
  "utf8",
);
const routeSource = readFileSync(new URL("../src/routes/agent.rs", import.meta.url), "utf8");
const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");

assert.match(llmSource, /pub struct LlmTokenUsage/);
assert.match(llmSource, /pub enum ConversationStreamEvent/);
assert.match(llmSource, /TextDelta/);
assert.match(llmSource, /stream_options/);
assert.match(llmSource, /include_usage/);
assert.match(agentSource, /AssistantDelta/);
assert.match(agentSource, /begin_chat_run/);
assert.match(agentSource, /cancel_chat_run/);
assert.match(routeSource, /\/api\/agent\/sessions\/\{session_id\}\/chat\/stop/);
assert.match(agentHtml, /event\.type === "assistant_delta"/);
assert.match(agentHtml, /\/api\/agent\/sessions\/\$\{currentSessionId\}\/chat\/stop/);

assert.match(agentSource, /last_token_usage/);
assert.match(agentSource, /context_usage_source/);
assert.match(agentSource, /load_hierarchical_agent_instructions/);
assert.match(agentSource, /AGENTS\.md/);

assert.match(backgroundSource, /tmux/);
assert.match(backgroundSource, /BACKGROUND_COMMANDS_FILE/);
assert.match(backgroundSource, /pub rows: u16/);
assert.match(backgroundSource, /pub cols: u16/);
assert.match(backgroundSource, /recover_sessions/);

for (const toolName of [
  "list_mcp_tools",
  "call_mcp_tool",
  "web_search",
  "web_fetch",
  "view_image",
  "run_browser_actions",
]) {
  assert.match(agentSource, new RegExp(`\\"name\\": \\"${toolName}\\"`), `missing ${toolName} tool`);
}

assert.match(agentHtml, /id="agent-attachment-input"[^>]*type="file"/);
assert.match(agentHtml, /accept="image\/\*"/);
assert.match(agentHtml, /attachments/);

console.log("native Agent Codex-parity contracts passed");
