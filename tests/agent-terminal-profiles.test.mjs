import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const profilesHtml = readFileSync(
  new URL("../static/agent-profiles.html", import.meta.url),
  "utf8",
);
const profilesJs = readFileSync(
  new URL("../static/agent-terminal-profiles.js", import.meta.url),
  "utf8",
);
const terminalAgentJs = readFileSync(
  new URL("../static/terminal-work-agent.js", import.meta.url),
  "utf8",
);
const routesRs = readFileSync(new URL("../src/routes/agent.rs", import.meta.url), "utf8");
const agentRs = readFileSync(new URL("../src/agent.rs", import.meta.url), "utf8");

assert.match(
  profilesHtml,
  /DeepSeek Agent[\s\S]*href="\/agent"[\s\S]*id="agent-terminal-profiles"[\s\S]*id="agent-terminal-profile-dialog"/,
  "the retained control-plane page should lead to Harness and expose profile CRUD",
);
assert.match(profilesHtml, /agent-terminal-profiles\.js/);
assert.match(
  profilesHtml,
  /id="agent-terminal-profile-agent-type"[\s\S]*value="codex"[\s\S]*Codex[\s\S]*value="claude"[\s\S]*Claude/,
  "new terminal profiles should support Codex and Claude",
);
assert.doesNotMatch(
  profilesHtml,
  /value="native"|原生智能体/,
  "the profile editor must not create new webClx-native Agents",
);
assert.doesNotMatch(
  profilesHtml,
  /agent-chat-area|agent-empty-state|agent-summary-dialog|agent-approvals-dialog/,
  "the control-plane page should not retain the native chat surface",
);

assert.doesNotMatch(
  profilesJs,
  /nativeSessions|nativeAgentController|webClxNativeAgent|\/api\/agent\/sessions/,
  "profile management must not depend on the retired native Agent API",
);
assert.match(
  profilesJs,
  /function isLegacyNativeProfile\(profile\)[\s\S]*agent_type[\s\S]*=== "native"/,
  "stored native profiles should remain detectable for migration",
);
assert.match(
  profilesJs,
  /legacy-native[\s\S]*已由 DeepSeek Harness 接管[\s\S]*makeHarnessLink/,
  "stored native profiles should render a migration notice and Harness command",
);
assert.match(
  profilesJs,
  /makeButton\("打开", "open"[\s\S]*makeButton\("新建", "launch"[\s\S]*openProfileInAgent\(profile\)[\s\S]*launchProfileInAgent\(profile\)/,
  "Codex and Claude profile cards should retain open/new actions",
);
assert.match(
  profilesJs,
  /function profilePresets\(profile\)[\s\S]*state\.claudePresets[\s\S]*state\.codexPresets/,
  "Claude and Codex profiles should keep separate preset families",
);
assert.match(
  profilesJs,
  /requestJson\("\/api\/agent\/terminal-profiles"\)[\s\S]*requestJson\("\/api\/agent\/api-presets"\)[\s\S]*requestJson\("\/api\/auth\/claude-presets"\)[\s\S]*requestJson\("\/api\/agent\/skills"\)[\s\S]*requestJson\("\/api\/terminal\/sessions\?all=true"\)/,
  "the page should initialize only from retained control-plane APIs",
);
assert.match(
  profilesJs,
  /profile\.description \|\| "暂无说明"[\s\S]*description: byId\("agent-terminal-profile-description"\)/,
  "profile cards and saves should retain editable descriptions",
);

assert.match(
  terminalAgentJs,
  /agentType === "native"[\s\S]*DeepSeek Harness/,
  "legacy native profile launches from terminal URLs should direct users to Harness",
);
assert.match(
  terminalAgentJs,
  /agent_profile[\s\S]*\/api\/agent\/terminal-profiles\/[\s\S]*executeSpecifiedPreset\(\{[\s\S]*action: "launch"[\s\S]*ownerKey: `terminal-agent-profile:\$\{profile\.id\}`/,
  "Codex and Claude profiles should keep using shared preset/session launch logic",
);

assert.match(
  routesRs,
  /\/api\/agent\/terminal-profiles[\s\S]*list_terminal_profiles[\s\S]*create_terminal_profile[\s\S]*update_terminal_profile[\s\S]*delete_terminal_profile/,
  "Agent routes should retain profile CRUD",
);
assert.match(
  agentRs,
  /id: "proxy_settings_agent"[\s\S]*skill_name: "mihomo-proxy-ops"[\s\S]*id: "work_agent"[\s\S]*skill_name: "autopilot"/,
  "default Codex terminal profiles should remain available",
);

console.log("terminal Agent profile migration contract passed");
