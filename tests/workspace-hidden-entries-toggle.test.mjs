import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const browserJs = readFileSync(
  new URL("../static/app-workspace-browser.js", import.meta.url),
  "utf8",
);
const coreBindingsJs = readFileSync(
  new URL("../static/app-core-event-bindings.js", import.meta.url),
  "utf8",
);
const settingsLoadSaveJs = readFileSync(
  new URL("../static/app-settings-load-save.js", import.meta.url),
  "utf8",
);
const settingsCoreLib = readFileSync(
  new URL("../crates/settings_core/src/lib.rs", import.meta.url),
  "utf8",
);

test("workspace browser renders the hidden-entries checkbox above the entry table", () => {
  assert.match(
    indexHtml,
    /<div class="workspace-filter-strip">[\s\S]*id="workspace-show-hidden-input"[\s\S]*显示隐藏文件[\s\S]*<\/div>[\s\S]*<div class="table-wrap">[\s\S]*id="entry-list"/,
  );
});

test("hidden-entries checkbox defaults to unchecked", () => {
  const input = indexHtml.match(/<input id="workspace-show-hidden-input"[^>]*>/);
  assert.ok(input, "workspace-show-hidden-input should exist");
  assert.doesNotMatch(input[0], /\bchecked\b/);
  assert.match(appJs, /showDotEntries:\s*false,/);
  assert.match(settingsCoreLib, /fn default_show_dot_entries\(\) -> bool \{\s*false\s*\}/);
});

test("toggling the checkbox persists show_dot_entries and reloads the directory", () => {
  assert.match(coreBindingsJs, /workspaceShowHiddenInputEl\.addEventListener\("change"/);
  assert.match(coreBindingsJs, /persistWorkspaceShowHidden\(Boolean\(event\.target\.checked\)\)/);
  assert.match(
    browserJs,
    /async function persistWorkspaceShowHidden[\s\S]*show_dot_entries:\s*Boolean\(nextShowHidden\)[\s\S]*await loadDirectory\(\)/,
  );
});

test("workspace checkbox stays in sync with the global settings toggle", () => {
  assert.match(
    browserJs,
    /function syncWorkspaceShowHiddenInput\(\)[\s\S]*workspaceShowHiddenInputEl\.checked[\s\S]*showDotEntriesInputEl\.checked/,
  );
  assert.match(
    settingsLoadSaveJs,
    /state\.showDotEntries = Boolean\(settings\.show_dot_entries\);\s*if \(workspaceShowHiddenInputEl\) \{\s*workspaceShowHiddenInputEl\.checked = state\.showDotEntries;/,
  );
});

test("a failed toggle restores the previous checkbox state", () => {
  assert.match(
    browserJs,
    /const previous = Boolean\(state\.showDotEntries\);[\s\S]*catch \(error\) \{\s*state\.showDotEntries = previous;/,
  );
});
