import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const appHtml = read("../static/index.html");
const appJs = readEntryScriptBundle("index.html");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalSettingsJs = read("../static/terminal-settings.js");
const terminalStyles = read("../static/styles-terminal.css");
const settingsLibRs = read("../crates/settings_core/src/lib.rs");
const settingsApiRs = read("../crates/settings_core/src/api.rs");
const settingsStorageRs = read("../crates/settings_core/src/storage.rs");

assert.match(
  appHtml,
  /id="terminal-fab-action-color-input"[\s\S]*?type="color"/,
  "Appearance settings should expose a native FAB action color picker",
);
assert.match(
  appHtml,
  /id="terminal-fab-action-opacity-input"[\s\S]*?type="range"[\s\S]*?min="0\.1"[\s\S]*?max="1"[\s\S]*?step="0\.05"/,
  "Appearance settings should expose a bounded FAB action opacity slider",
);
assert.match(
  appHtml,
  /id="terminal-fab-action-opacity-output"/,
  "the FAB opacity slider should show its current percentage",
);

assert.match(terminalSettingsJs, /DEFAULT_TERMINAL_FAB_ACTION_COLOR = "#f59e0b"/);
assert.match(terminalSettingsJs, /DEFAULT_TERMINAL_FAB_ACTION_OPACITY = 0\.5/);
assert.match(terminalSettingsJs, /function normalizeTerminalFabActionColor\(/);
assert.match(terminalSettingsJs, /function normalizeTerminalFabActionOpacity\(/);
assert.match(appJs, /terminalFabActionColor:\s*DEFAULT_TERMINAL_FAB_ACTION_COLOR/);
assert.match(appJs, /terminalFabActionOpacity:\s*DEFAULT_TERMINAL_FAB_ACTION_OPACITY/);
assert.match(appJs, /terminal_fab_action_color:\s*nextTerminalFabActionColor/);
assert.match(appJs, /terminal_fab_action_opacity:\s*nextTerminalFabActionOpacity/);

assert.match(terminalJs, /applyTerminalFabAppearance\(settings\.terminal_fab_action_color,[\s\S]*?settings\.terminal_fab_action_opacity\)/);
assert.match(
  terminalStyles,
  /\.terminal-fab-item \{[\s\S]*?color: var\(--terminal-fab-action-color\);[\s\S]*?opacity: 0;[\s\S]*?animation:/,
  "FAB actions should use the configured color while preserving their entry animation",
);
assert.match(
  terminalStyles,
  /@keyframes terminal-fab-item-in \{[\s\S]*?to \{[\s\S]*?opacity: var\(--terminal-fab-action-opacity\);/,
  "FAB actions should settle at the configured opacity",
);

assert.match(settingsLibRs, /DEFAULT_TERMINAL_FAB_ACTION_COLOR:\s*&str\s*=\s*"#f59e0b"/);
assert.match(settingsLibRs, /DEFAULT_TERMINAL_FAB_ACTION_OPACITY:\s*f32\s*=\s*0\.5/);
for (const field of ["terminal_fab_action_color", "terminal_fab_action_opacity"]) {
  assert.match(settingsLibRs, new RegExp(`${field}:`), `${field} should belong to the settings schema`);
  assert.match(settingsApiRs, new RegExp(`${field}:`), `${field} should flow through the settings API`);
  assert.match(settingsStorageRs, new RegExp(`${field}`), `${field} should persist to the settings file`);
}

console.log("terminal FAB appearance settings contract checks passed");
