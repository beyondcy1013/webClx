import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const appJs = readEntryScriptBundle("index.html");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

const editStart = appJs.indexOf("function editApiPreset(presetId) {");
const editEnd = appJs.indexOf("async function saveApiPreset()", editStart);
assert.ok(editStart >= 0 && editEnd > editStart, "Codex_API edit function should exist");

const editBody = appJs.slice(editStart, editEnd);
const savedChoiceAssignment = editBody.indexOf(
  "apiApplyUpstreamProxyOnSwitchInputEl.checked = Boolean(preset.apply_upstream_proxy_on_switch);",
);
const preserveSavedChoice = editBody.indexOf("state.apiApplyProxyManuallyChanged = true;");
const recommendationSync = editBody.indexOf("syncApiApplyProxyRecommendation();");

assert.ok(savedChoiceAssignment >= 0, "the editor should load the saved local-entry choice");
assert.ok(
  preserveSavedChoice > savedChoiceAssignment && preserveSavedChoice < recommendationSync,
  "editing a Codex_API preset must preserve its saved direct/local-entry choice before applying recommendations",
);

assert.match(
  indexHtml,
  /id="api-responses-proxy-input"[\s\S]*<option value="direct">不转换（上游已支持 Responses）<\/option>/,
  "the no-conversion option must have an explicit persisted value",
);
assert.match(
  appJs,
  /const responsesProxy = apiResponsesProxyInputEl\?\.value \|\| "direct";/,
  "saving no conversion must submit the explicit direct mode instead of null",
);
