import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appCoreJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const apiManagerJs = readFileSync(new URL("../static/app-api-manager.js", import.meta.url), "utf8");
const claudeManagerJs = readFileSync(new URL("../static/app-claude-manager.js", import.meta.url), "utf8");
const managerJs = `${apiManagerJs}\n${claudeManagerJs}`;
const appJs = readEntryScriptBundle("index.html");

assert.match(
  indexHtml,
  /id="api-test-all-presets"[\s\S]*?>测试所有账号<\/button>/,
  "Codex_API saved presets toolbar should expose a test-all button",
);
assert.doesNotMatch(
  indexHtml,
  /id="api-preset-test-results"/,
  "Codex_API test results should use row state and the shared popup instead of a persistent batch panel",
);
assert.match(
  indexHtml,
  /id="claude-test-all-presets"[\s\S]*?>测试所有账号<\/button>/,
  "Claude_API saved presets toolbar should expose a test-all button",
);
assert.doesNotMatch(
  indexHtml,
  /id="claude-preset-test-results"/,
  "Claude_API test results should use row state and the shared popup instead of a persistent batch panel",
);

assert.match(
  appJs,
  /function apiPresetRowActions\(preset\)[\s\S]*label: "测试"[\s\S]*handler: \(\) => testApiPreset\(preset\.id, preset\.name\)/,
  "Codex_API preset action menus should expose a single-preset test action",
);
assert.match(
  appJs,
  /function renderApiPresetTableHeader\(configKeys, options = \{\}\) \{[\s\S]*baseLabels: \[[\s\S]*\{ label: "序号" \}[\s\S]*\{ label: "操作", className: "api-preset-operation-cell" \}[\s\S]*\{ label: "状态指示"[\s\S]*\{ label: "名字"[\s\S]*trailingLabels: \[\s*\{ label: "保存时间"/,
  "Codex_API preset table should place consolidated row actions after the sequence column",
);
assert.doesNotMatch(
  appJs,
  /function renderApiPresets\(presets\) \{[\s\S]*decorateRow: \(row, preset\) => makePresetRowClickable\(row, preset, \(\) => applyApiPreset\(preset\.id\)\)/,
  "Codex_API preset rows should not switch presets when clicking non-button cells",
);
assert.match(
  appJs,
  /const CODEX_API_AUTO_PROXY_MATCH_PROVIDERS[\s\S]*id: "deepseek"[\s\S]*id: "minimax"/,
  "Codex_API frontend should use the data-driven provider list for the per-preset local proxy recommendation",
);
assert.match(
  appJs,
  /function apiBaseUrlMatchesAutoProxyProvider[\s\S]*CODEX_API_AUTO_PROXY_MATCH_PROVIDERS/,
  "Codex_API provider matching should consume the shared provider list",
);
assert.match(
  appJs,
  /function syncApiApplyProxyRecommendation\(\)[\s\S]*apiApplyUpstreamProxyOnSwitchInputEl\.checked = true/,
  "Codex_API frontend should auto-check the per-preset local proxy option when compatibility rules recommend it",
);
assert.match(
  appJs,
  /apiApplyUpstreamProxyOnSwitchInputEl\.disabled = preset\.access_mode === "chatgpt_oauth";/,
  "Codex_API frontend should only lock the required local proxy option for ChatGPT OAuth presets",
);
assert.match(
  appJs,
  /function apiApplyProxyRecommendationWarningMessage[\s\S]*本机入口或转换模式[\s\S]*不启用本机入口[\s\S]*function warnApiApplyProxyRecommendationIfNeeded/,
  "Codex_API frontend should warn when a recommended local entry option is unchecked",
);
assert.match(
  appJs,
  /function apiApplyProxyRecommendationWarningMessage[\s\S]*本机入口[\s\S]*function confirmApiApplyProxyRecommendationBeforeSave[\s\S]*window\.confirm/,
  "Codex_API frontend should warn again on save when a recommended local entry option is unchecked",
);
assert.match(
  appJs,
  /function confirmEditedPresetOverwrite[\s\S]*window\.confirm[\s\S]*覆盖原条目[\s\S]*另存为新条目/,
  "edited presets should save over the current preset after warning that new copies use the save-as-new button",
);
assert.match(
  managerJs,
  /function saveApiPresetWithMode\(forceNewPreset = false\)[\s\S]*confirmEditedPresetOverwrite[\s\S]*function saveClaudePresetWithMode\(forceNewPreset = false\)[\s\S]*confirmEditedPresetOverwrite/,
  "split preset managers should confirm before overwriting edited presets",
);
assert.match(
  appJs,
  /function saveApiPresetWithMode\(forceNewPreset = false\)[\s\S]*const isEditing = Boolean\(editingPresetId\) && !forceNewPreset;[\s\S]*confirmEditedPresetOverwrite/,
  "Codex_API edited presets should only overwrite when not using the explicit save-as-new path",
);
assert.doesNotMatch(
  appJs,
  /确定：新增一条并保留原预设/,
  "edited preset save confirmation should no longer map OK to creating a new preset",
);
assert.match(
  appCoreJs,
  /button\.addEventListener\("click", \(event\) => \{[\s\S]*?event\.stopPropagation\(\);[\s\S]*?handler\(\);[\s\S]*?\}\);/,
  "preset row action buttons should stop propagation so row-click switching does not swallow test/edit/delete actions",
);
assert.match(
  appJs,
  /createActionButton\("测试", \(\) => testClaudePreset\(preset\.id, preset\.name\), "mini-button"\)/,
  "Claude_API preset rows should expose a single-preset test action",
);
assert.match(
  appJs,
  /function renderClaudePresetTableHeader\(configKeys, options = \{\}\) \{[\s\S]*baseLabels: \[[\s\S]*"切换"[\s\S]*"测试"[\s\S]*"OpenCode"[\s\S]*"编辑"[\s\S]*"删除"[\s\S]*"状态指示"[\s\S]*"协议转换"/,
  "Claude_API preset table should render one action button per action column",
);
assert.match(
  claudeManagerJs,
  /access_mode: claudeAccessModeInputEl\?\.value \|\| "direct"[\s\S]*use_local_proxy: \["anthropic_relay", "openai_chat", "openai_responses"\]\.includes/,
  "Claude_API edited presets should submit the selected protocol conversion mode",
);
assert.match(
  indexHtml,
  /id="claude-access-mode-input"[\s\S]*不转换（直连 Anthropic Messages）[\s\S]*Anthropic 无转换中转[\s\S]*OpenAI Chat → Anthropic Messages/,
  "Claude_API preset editor should expose protocol conversion choices",
);
assert.doesNotMatch(
  appJs,
  /function renderClaudePresets\(presets\) \{[\s\S]*decorateRow: \(row, preset\) => makePresetRowClickable\(row, preset, \(\) => applyClaudePreset\(preset\.id\)\)/,
  "Claude_API preset rows should not switch presets when clicking non-button cells",
);

assert.match(
  appJs,
  /requestJson\(`\/api\/auth\/api-presets\/\$\{encodeURIComponent\(presetId\)\}\/test`,\s*\{\s*method: "POST"/,
  "Codex_API single test should call its backend test endpoint",
);
assert.match(
  appJs,
  /requestJson\("\/api\/auth\/api-presets\/test-all", \{\s*method: "POST"/,
  "Codex_API test-all should call its backend batch test endpoint",
);
assert.match(
  appJs,
  /async function testAllApiPresets\(\)[\s\S]*results\.forEach\(\(raw\) => \{[\s\S]*state\.apiPresetTestResults\.set\(normalized\.preset_id, normalized\)[\s\S]*renderApiPresets\(state\.apiPresets\)/,
  "Codex_API test-all should store every result and rerender row status",
);
assert.match(
  appJs,
  /requestJson\(`\/api\/auth\/claude-presets\/\$\{encodeURIComponent\(presetId\)\}\/test`,\s*\{\s*method: "POST"/,
  "Claude_API single test should call its backend test endpoint",
);
assert.match(
  appJs,
  /requestJson\("\/api\/auth\/claude-presets\/test-all", \{\s*method: "POST"/,
  "Claude_API test-all should call its backend batch test endpoint",
);
assert.match(
  appJs,
  /async function testAllClaudePresets\(\)[\s\S]*results\.forEach\(\(raw\) => \{[\s\S]*state\.claudePresetTestResults\.set\(normalized\.preset_id, normalized\)[\s\S]*renderClaudePresets\(state\.claudePresets\)/,
  "Claude_API test-all should store every result and rerender row status",
);
