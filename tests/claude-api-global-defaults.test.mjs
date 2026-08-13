import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const configOverrideJs = readFileSync(
  new URL("../static/app-config-override.js", import.meta.url),
  "utf8",
);
const settingsLoadSaveJs = readFileSync(
  new URL("../static/app-settings-load-save.js", import.meta.url),
  "utf8",
);
const settingsEventBindingsJs = readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);

const claudeViewStart = indexHtml.indexOf('id="claude-view"');
const settingsViewStart = indexHtml.indexOf('id="settings-view"');
assert.ok(
  claudeViewStart >= 0 && settingsViewStart > claudeViewStart,
  "Claude_API view should exist",
);
const claudeViewHtml = indexHtml.slice(claudeViewStart, settingsViewStart);

assert.match(
  claudeViewHtml,
  /id="claude-api-global-defaults"[\s\S]*id="claude-default-config-list"/,
  "Claude_API should expose its shared settings.json env defaults",
);
assert.match(
  claudeViewHtml,
  /id="claude-default-config-reset"[\s\S]*id="claude-default-config-save"/,
  "Claude defaults should provide reset and save actions",
);
assert.match(
  claudeViewHtml,
  /id="claude-default-config-status"[\s\S]*role="status"[\s\S]*aria-live="polite"/,
  "Claude default save feedback should be announced",
);
assert.equal(
  (indexHtml.match(/id="claude-default-config-list"/g) || []).length,
  1,
  "Claude defaults should have one authoritative DOM owner",
);

assert.match(
  appJs,
  /claudeDefaultConfigSaveButtonEl[\s\S]*claude-default-config-save[\s\S]*claudeDefaultConfigStatusEl/,
  "app initialization should bind Claude default controls",
);
assert.match(
  configOverrideJs,
  /function renderClaudeDefaultConfigEntries\([\s\S]*claudeDefaultConfigListEl/,
  "Claude defaults should use a dedicated editor",
);
assert.match(
  settingsLoadSaveJs,
  /async function saveClaudeDefaultConfigEntries\([\s\S]*requestJson\("\/api\/settings",[\s\S]*method:\s*"PUT"[\s\S]*claude_default_config_entries:\s*entries/,
  "Claude defaults should save only their settings layer",
);
assert.match(
  settingsEventBindingsJs,
  /claudeDefaultConfigResetButtonEl\?\.addEventListener\("click"[\s\S]*cloneDefaultClaudeDefaultConfigEntries/,
  "Claude defaults should reset without mutating presets",
);
assert.match(
  settingsEventBindingsJs,
  /claudeDefaultConfigSaveButtonEl\?\.addEventListener\("click"[\s\S]*saveClaudeDefaultConfigEntries/,
  "Claude defaults should have a dedicated save action",
);

console.log("Claude_API global default config controls verified");
