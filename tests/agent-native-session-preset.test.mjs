import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");
const agentRs = readFileSync(new URL("../src/agent.rs", import.meta.url), "utf8");

assert.match(
  agentRs,
  /pub struct AgentSession \{[\s\S]*pub api_preset_id: String/,
  "native Agent sessions should persist their selected API preset",
);
assert.match(
  agentRs,
  /resolve_llm_credential\(&state, &session\.model, &session\.api_preset_id\)/,
  "chat should resolve credentials from the session preset",
);
assert.match(
  agentHtml,
  /id="agent-session-preset-select"/,
  "native Agent chat should expose a per-session preset selector",
);
assert.match(
  agentHtml,
  /async function changePreset\(presetId\)[\s\S]*api_preset_id: presetId[\s\S]*model/,
  "changing a session preset should persist both the route and its matching model",
);

console.log("native Agent session preset contract checks passed");
