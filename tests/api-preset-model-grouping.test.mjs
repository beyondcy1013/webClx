import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const apiManagerJs = readFileSync(
  new URL("../static/app-api-manager.js", import.meta.url),
  "utf8",
);
const responsiveCss = readFileSync(
  new URL("../static/styles-responsive.css", import.meta.url),
  "utf8",
);

assert.match(
  indexHtml,
  /<select[^>]+id="api-preset-group-mode"[\s\S]*?<option value="base_url"[\s\S]*?<option value="model"/,
  "Codex API table should offer Base URL and model grouping modes",
);

assert.match(
  appJs,
  /apiPresetGroupMode:\s*"base_url"/,
  "Base URL grouping should remain the default",
);

assert.match(
  apiManagerJs,
  /API_PRESET_GROUP_MODE_STORAGE_KEY[\s\S]*localStorage\.getItem[\s\S]*localStorage\.setItem/,
  "the selected grouping mode should persist in browser storage",
);

assert.match(
  apiManagerJs,
  /function apiPresetModel\(preset\)[\s\S]*extractModelFromOverrides\(preset\?\.config_overrides\)/,
  "model grouping should read the dedicated model config override",
);

assert.match(
  apiManagerJs,
  /function apiPresetGroupConfig\(\)[\s\S]*state\.apiPresetGroupMode === "model"[\s\S]*mergeCellKey: "config:model"[\s\S]*mergeCellKey: "base_url"/,
  "the table should merge either the model or Base URL cell for the selected mode",
);

assert.match(
  apiManagerJs,
  /async function moveApiPresetOrder\(presetId, direction\)[\s\S]*apiPresetVisibleOrder\(state\.apiPresets\)[\s\S]*apiPresetGroupKey[\s\S]*persistPresetOrder/,
  "manual move buttons should swap adjacent presets inside grouped visible order",
);

assert.match(
  apiManagerJs,
  /function apiPresetRowActions\(preset\)[\s\S]*apiPresetGroupKey\(previousPreset[\s\S]*apiPresetGroupKey\(nextPreset[\s\S]*label: "上移"[\s\S]*moveApiPresetOrder\(preset\.id, -1\)[\s\S]*label: "下移"[\s\S]*moveApiPresetOrder\(preset\.id, 1\)/,
  "move menu actions should stay disabled at the current group boundaries",
);

assert.match(
  apiManagerJs,
  /apiPresetGroupModeInputEl\.addEventListener\("change"[\s\S]*renderApiPresets\(state\.apiPresets\)/,
  "changing grouping mode should rerender the API table immediately",
);

assert.match(
  responsiveCss,
  /#api-view \.api-top-list-card \.panel-head\.wide[\s\S]*display:\s*block;[\s\S]*#api-view \.api-top-list-card \.toolbar[\s\S]*overflow-x:\s*auto;[\s\S]*white-space:\s*nowrap;/,
  "mobile API table actions should stay horizontal instead of wrapping text vertically",
);

console.log("API preset model grouping controls verified");
