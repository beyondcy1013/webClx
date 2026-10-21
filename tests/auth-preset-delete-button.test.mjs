import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readEntryScriptBundle("index.html");

const authTableMatch = indexHtml.match(
  /<table class="entry-table auth-table">[\s\S]*?<thead>([\s\S]*?)<\/thead>/,
);

assert.ok(authTableMatch, "Codex_OAuth preset table header should exist");

const authHeaders = Array.from(authTableMatch[1].matchAll(/<th>(.*?)<\/th>/g), (match) =>
  match[1].trim(),
);

assert.equal(
  authHeaders.at(-1),
  "删除",
  "Codex_OAuth preset table should expose delete as the last column",
);

const renderAuthPresetsMatch = appJs.match(
  /function renderAuthPresets\(presets\) \{[\s\S]*?\n\}/,
);

assert.ok(renderAuthPresetsMatch, "renderAuthPresets should exist");
assert.match(
  renderAuthPresetsMatch[0],
  /createPresetDeleteButton\(\(\) => deleteAuthPreset\(preset\.id, preset\.name\)\)/,
  "renderAuthPresets should render a delete button that calls deleteAuthPreset",
);
