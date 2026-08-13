import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");
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
  agentHtml,
  /id="agent-terminal-profiles"[\s\S]*id="agent-terminal-profile-dialog"[\s\S]*agent-terminal-profiles\.js/,
  "Agent page should expose the terminal-agent list and editor",
);
assert.match(
  agentHtml,
  /agent-empty-state[\s\S]*data-agent-sidebar-toggle[\s\S]*document\.addEventListener\("click"[\s\S]*data-agent-sidebar-toggle/,
  "terminal agents should remain reachable from the empty mobile Agent page",
);
assert.match(
  profilesJs,
  /makeButton\("打开", "open"[\s\S]*makeButton\("新建", "launch"[\s\S]*openProfileInAgent\(profile\)[\s\S]*launchProfileInAgent\(profile\)/,
  "profile cards should separate opening an existing session from creating a new one",
);
assert.match(
  profilesJs,
  /function openProfileInAgent\(profile\)[\s\S]*if \(!session\) \{[\s\S]*launchProfileInAgent\(profile\)[\s\S]*return true/,
  "opening a profile without an existing session should launch its first session",
);
assert.match(
  profilesJs,
  /function agentSessionFrameUrl\(session\)[\s\S]*embedded[\s\S]*path[\s\S]*session[\s\S]*function profileLaunchFrameUrl\(profile\)[\s\S]*agent_profile/,
  "existing agent sessions should open directly while explicit launches carry the profile ID",
);
assert.match(
  profilesJs,
  /function profileOwnerKey\(profileId\)[\s\S]*terminal-agent-profile:[\s\S]*function sessionsForProfile\(profile\)[\s\S]*owner_key[\s\S]*display_path[\s\S]*profile\?\.cwd/,
  "profile sessions should use durable owner metadata with a legacy cwd fallback",
);
assert.match(
  profilesJs,
  /function sessionRecency\(session\)[\s\S]*last_opened_at[\s\S]*function restoreLastAgentSession\(\)[\s\S]*sessionRecency\(right\.session\)[\s\S]*openProfileInAgent/,
  "opening the Agent page should restore the most recently opened profile session without launching",
);
assert.match(
  profilesJs,
  /agent-terminal-pending[\s\S]*event\.origin !== window\.location\.origin[\s\S]*event\.source !== state\.activeTerminalFrame[\s\S]*message\.status === "ready"/,
  "the Agent page should hide stale terminal output until its own launch succeeds",
);
assert.doesNotMatch(
  profilesJs,
  /terminalManagerUrl|textContent = "终端管理"|target = "_top"/,
  "Agent conversations must not escape into normal terminal management",
);
assert.match(
  profilesJs,
  /populateProfileSessionSwitcher[\s\S]*sessionsForProfile\(profile\)[\s\S]*aria-label", "切换此智能体的会话"[\s\S]*showProfileInAgent\(profile, \{ session: selected \}\)/,
  "Agent page should switch only among the selected profile's conversations",
);
assert.match(
  agentHtml,
  /agent-terminal-toolbar[\s\S]*agent-terminal-frame/,
  "the Agent page should style an embedded terminal conversation surface",
);
assert.match(
  readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8"),
  /terminal-embedded-agent \.terminal-page-nav \{ display: none; \}[\s\S]*URLSearchParams[\s\S]*get\("embedded"\) === "agent"/,
  "embedded terminal mode should hide the duplicate top navigation",
);
assert.match(
  agentHtml,
  /id="agent-terminal-profile-description"[\s\S]*maxlength="240"/,
  "the terminal-agent editor should expose an editable description",
);
assert.match(
  agentHtml,
  /id="agent-terminal-profile-agent-type"[\s\S]*value="native"[\s\S]*原生智能体[\s\S]*value="codex"[\s\S]*Codex[\s\S]*value="claude"[\s\S]*Claude/,
  "the profile editor should let users choose native, Codex, or Claude",
);
assert.match(
  profilesJs,
  /profile\.description \|\| "暂无说明"[\s\S]*agent-terminal-profile-description[\s\S]*description: byId\("agent-terminal-profile-description"\)/,
  "profile cards and saves should use the editable description",
);
assert.match(
  profilesJs,
  /function profileAgentType\(profile\)[\s\S]*return "codex"[\s\S]*agent_type: byId\("agent-terminal-profile-agent-type"\)\.value/,
  "legacy profiles should remain Codex profiles and saves should persist the selected engine",
);
assert.match(
  profilesJs,
  /nativeSessionsForProfile\(profile\)[\s\S]*nativeAgentController\(\)[\s\S]*openSession\(session\.id\)[\s\S]*createSession\(profile\.id\)/,
  "native profiles should open and create native Agent sessions instead of terminal frames",
);
assert.match(
  agentHtml,
  /createProfileSession\(profileId\)[\s\S]*\/api\/agent\/sessions[\s\S]*profile_id: profileId[\s\S]*webClxNativeAgent/,
  "the native Agent controller should create profile-owned sessions through the native session API",
);
assert.match(
  profilesJs,
  /function profilePresets\(profile\)[\s\S]*state\.claudePresets[\s\S]*requestJson\("\/api\/auth\/claude-presets"\)/,
  "Claude profiles should load Claude presets while native and Codex profiles use Codex_API presets",
);
assert.doesNotMatch(
  profilesJs,
  /profile\.preset_selector} · \$\$\{profile\.skill_name} · \$\{profile\.cwd}/,
  "profile cards should not expose preset IDs, skill names, or paths",
);
assert.match(
  profilesJs,
  /action === "open"[\s\S]*title = "打开最近的智能体会话"[\s\S]*action === "launch"[\s\S]*title = "新建智能体会话"/,
  "open and new-session actions should have distinct accessible names",
);
assert.match(
  profilesJs,
  /function profilePresetId\(profile\)[\s\S]*unique_contains[\s\S]*compatible\.length === 1[\s\S]*profilePresetId\(profile\)/,
  "name-matched default profiles should resolve back to their preset in the editor",
);
assert.match(
  terminalAgentJs,
  /agent_profile[\s\S]*\/api\/agent\/terminal-profiles\/[\s\S]*executeSpecifiedPreset\(\{[\s\S]*action: "launch"[\s\S]*projectPath: profile\.project_path[\s\S]*ownerKey: `terminal-agent-profile:\$\{profile\.id\}`[\s\S]*launchTerminal: launchTerminalSpecifiedPreset/,
  "terminal entrypoint should launch the selected profile through shared preset/session logic",
);
assert.match(
  terminalAgentJs,
  /profileAgentType\(profile\)[\s\S]*specifiedPresetListEndpoint\(agentType\)[\s\S]*agent: agentType[\s\S]*specifiedPresetModel\(preset, agentType\)/,
  "terminal profiles should route Codex and Claude through their matching preset families",
);
assert.match(
  terminalAgentJs,
  /webclx-agent-terminal-launch[\s\S]*presetName: preset\.name[\s\S]*specifiedPresetModel\(preset, agentType\)[\s\S]*reportTerminalAgentLaunchFailure/,
  "the terminal should report the created session model or launch failure to its Agent parent",
);
assert.match(
  routesRs,
  /\/api\/agent\/terminal-profiles[\s\S]*list_terminal_profiles[\s\S]*create_terminal_profile[\s\S]*update_terminal_profile[\s\S]*delete_terminal_profile/,
  "Agent routes should expose profile CRUD",
);
assert.match(
  agentRs,
  /id: "proxy_settings_agent"[\s\S]*preset_selector: "miniMax"[\s\S]*skill_name: "mihomo-proxy-ops"[\s\S]*id: "work_agent"[\s\S]*skill_name: "autopilot"/,
  "proxy settings and work agent should be available as default terminal-agent profiles",
);
for (const toolName of [
  "list_files",
  "search_files",
  "read_file",
  "apply_patch",
  "git_diff",
  "create_checkpoint",
  "run_verification",
]) {
  assert.match(
    agentRs,
    new RegExp(`"name": "${toolName}"`),
    `built-in Agent should expose ${toolName}`,
  );
}
assert.match(
  agentHtml,
  /内置 Agent 可检查和修改工作区、运行验证，也能按需使用专项 skill/,
  "Agent page should describe built-in engineering work instead of requiring Codex",
);
