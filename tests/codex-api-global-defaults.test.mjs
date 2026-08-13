import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const settingsLoadSaveJs = readFileSync(
  new URL("../static/app-settings-load-save.js", import.meta.url),
  "utf8",
);
const settingsEventBindingsJs = readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);
const configOverrideJs = readFileSync(
  new URL("../static/app-config-override.js", import.meta.url),
  "utf8",
);
const workspaceRoutesRs = readFileSync(
  new URL("../src/routes/workspace.rs", import.meta.url),
  "utf8",
);

const apiViewStart = indexHtml.indexOf('id="api-view"');
const apiViewEnd = indexHtml.indexOf('id="claude-view"');
assert.ok(apiViewStart >= 0 && apiViewEnd > apiViewStart, "Codex_API view should exist");
const apiViewHtml = indexHtml.slice(apiViewStart, apiViewEnd);

assert.match(
  apiViewHtml,
  /id="codex-api-global-defaults"[\s\S]*id="codex-default-config-list"/,
  "Codex_API should expose the shared default config editor inside its own tab",
);
assert.match(
  apiViewHtml,
  /id="codex-default-config-reset"[\s\S]*id="codex-default-config-save"/,
  "the default layer should provide explicit reset and save actions",
);
assert.match(
  apiViewHtml,
  /id="codex-common-approval-never"[\s\S]*type="checkbox"[\s\S]*id="codex-common-sandbox-full-access"[\s\S]*type="checkbox"/,
  "Codex_API should expose independent supported approval and sandbox checkboxes",
);
assert.match(
  apiViewHtml,
  /id="codex-common-config-refresh"[\s\S]*id="codex-common-config-save"[\s\S]*id="codex-common-config-status"/,
  "the current config controls should provide explicit refresh, save, and status elements",
);
assert.doesNotMatch(
  apiViewHtml,
  /tools\.shell|confirm_commands/,
  "the UI must not expose the unsupported tools.shell.confirm_commands key",
);
assert.match(
  apiViewHtml,
  /<th[^>]*>作用域<\/th>[\s\S]*id="codex-default-config-list"/,
  "generic defaults should distinguish their config scope",
);
assert.match(
  apiViewHtml,
  /id="codex-default-config-status"[\s\S]*role="status"[\s\S]*aria-live="polite"/,
  "save feedback should be announced to assistive technology",
);
assert.equal(
  (indexHtml.match(/id="codex-default-config-list"/g) || []).length,
  1,
  "the default config editor should have one authoritative DOM owner",
);

assert.match(
  appJs,
  /codexDefaultConfigSaveButtonEl[\s\S]*codex-default-config-save[\s\S]*codexDefaultConfigStatusEl[\s\S]*codex-default-config-status/,
  "app initialization should bind the Codex_API default save controls",
);
assert.match(
  settingsLoadSaveJs,
  /async function saveCodexDefaultConfigEntries\([\s\S]*requestJson\("\/api\/settings",[\s\S]*method:\s*"PUT"[\s\S]*codex_default_config_entries:\s*entries/,
  "saving Codex_API defaults should submit the shared default layer through settings",
);
assert.match(
  settingsLoadSaveJs,
  /saveCodexDefaultConfigEntries\([\s\S]*codexConfigScopeForKey\(entry\.key\)\.kind === "provider"[\s\S]*属于预设 Provider/,
  "provider-owned keys should be rejected from the generic common default layer",
);
assert.match(
  settingsLoadSaveJs,
  /async function loadCodexCommonConfig\([\s\S]*requestJson\("\/api\/settings\/codex-common-config"/,
  "the fixed controls should read the active terminal user's current config",
);
assert.match(
  settingsLoadSaveJs,
  /async function saveCodexCommonConfig\([\s\S]*requestJson\("\/api\/settings\/codex-common-config",[\s\S]*method:\s*"PUT"[\s\S]*approval_never:[\s\S]*sandbox_full_access:/,
  "the fixed controls should update both supported top-level settings",
);
assert.match(
  settingsEventBindingsJs,
  /codexDefaultConfigResetButtonEl\?\.addEventListener\("click"[\s\S]*cloneDefaultCodexDefaultConfigEntries/,
  "reset should restore the built-in rows without touching presets",
);
assert.match(
  settingsEventBindingsJs,
  /codexDefaultConfigSaveButtonEl\?\.addEventListener\("click"[\s\S]*saveCodexDefaultConfigEntries/,
  "the dedicated save button should persist only the default layer",
);
assert.match(
  configOverrideJs,
  /function codexConfigScopeForKey\([\s\S]*model_providers[\s\S]*provider[\s\S]*table[\s\S]*root/,
  "default rows should classify provider-owned, table, and top-level keys",
);
assert.match(
  configOverrideJs,
  /\["键名", "键值", "作用域", ""\][\s\S]*config-override-scope-cell[\s\S]*codexConfigScopeForKey/,
  "preset override rows should display the same config scope distinction",
);
assert.match(
  workspaceRoutesRs,
  /"\/api\/settings\/codex-common-config"[\s\S]*get\(config_files::read_codex_common_config\)[\s\S]*put\(config_files::save_codex_common_config\)/,
  "the backend should expose a dedicated current-config endpoint",
);

console.log("Codex_API global default config controls verified");
