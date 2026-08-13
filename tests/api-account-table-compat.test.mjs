import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const apiManagerSource = readFileSync(
  new URL("../static/app-api-manager.js", import.meta.url),
  "utf8",
);
const configOverrideSource = readFileSync(
  new URL("../static/app-config-override.js", import.meta.url),
  "utf8",
);

assert.match(
  indexHtml,
  /<th>凭据<\/th>/,
  "the shared Codex_API table should use a neutral credential heading",
);
assert.match(
  configOverrideSource,
  /function renderApiPresetTableHeader[\s\S]*\{ label: "凭据", sortKey: "api_key" \}/,
  "the dynamic Codex_API table header should keep the neutral credential heading",
);
assert.match(
  apiManagerSource,
  /const isChatgptOauth = preset\.access_mode === "chatgpt_oauth"/,
  "OAuth proxy presets should be identified by their persisted access mode",
);
assert.match(
  apiManagerSource,
  /`OAuth Token \$\{preset\.masked_access_token \|\| "已保存"\}`/,
  "OAuth proxy presets should reuse the credential column with a masked OAuth token",
);
assert.match(
  apiManagerSource,
  /getValue: \(preset\) => preset\?\.access_mode === "chatgpt_oauth"[\s\S]*preset\?\.masked_access_token[\s\S]*preset\?\.masked_api_key/,
  "credential sorting should use the value shown for each access mode",
);
assert.match(
  apiManagerSource,
  /isChatgptOauth[\s\S]*\? "OAuth 代理"/,
  "OAuth proxy presets should identify their local entry mode in the existing column",
);
assert.match(
  apiManagerSource,
  /access_mode: editingPreset\?\.access_mode \|\| null/,
  "editing through the shared form should preserve the preset access mode",
);
assert.match(
  apiManagerSource,
  /apiKeyInputEl\.readOnly = preset\.access_mode === "chatgpt_oauth"/,
  "OAuth proxy credentials should not be overwritten by the ordinary API key editor",
);
assert.match(
  apiManagerSource,
  /const canDuplicate = Boolean\(presetId\)[\s\S]*editingPreset\?\.access_mode !== "chatgpt_oauth"/,
  "OAuth proxy presets should not be duplicated without their imported credentials",
);
assert.match(
  apiManagerSource,
  /function mergeModelIntoOverrides\(overrides\)[\s\S]*return \[\{ key: API_MODEL_CONFIG_KEY, value: modelValue \}, \.\.\.others\];/,
  "API presets should persist their dedicated model field in config_overrides",
);
assert.match(
  apiManagerSource,
  /function extractModelFromOverrides\(overrides\)[\s\S]*for \(let index = list\.length - 1; index >= 0; index -= 1\)/,
  "duplicate legacy model overrides should resolve with the last effective value",
);
assert.match(
  apiManagerSource,
  /const presetModel = extractModelFromOverrides\(configOverrides\);[\s\S]*apiModelInputEl\.value = presetModel;[\s\S]*overridesWithoutModel\(configOverrides\)/,
  "editing an API preset should split model from the generic override editor",
);
assert.match(
  apiManagerSource,
  /if \(!isChatgptOauth && !model\)[\s\S]*先填写模型名称/,
  "ordinary API presets should require a model while OAuth presets remain exempt",
);
assert.match(
  apiManagerSource,
  /const configOverrides = isChatgptOauth[\s\S]*\? overridesWithoutModel\(rawConfigOverrides\)[\s\S]*: mergeModelIntoOverrides\(rawConfigOverrides\)/,
  "OAuth presets should not persist a model override from the ordinary API form",
);
