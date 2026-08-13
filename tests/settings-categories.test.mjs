import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readEntryScriptBundle("index.html");
const categoriesJs = readFileSync(
  new URL("../static/app-settings-categories.js", import.meta.url),
  "utf8",
);
const settingsApiRs = readFileSync(
  new URL("../crates/settings_core/src/api.rs", import.meta.url),
  "utf8",
);

const context = vm.createContext({});
vm.runInContext(categoriesJs, context);

assert.equal(
  vm.runInContext("SETTINGS_CATEGORY_REGISTRY.length", context),
  10,
  "settings navigation should expose ten stable top-level categories",
);

for (const [tab, category] of [
  ["system", "system"],
  ["terminal", "terminal"],
  ["soft-keyboard", "input"],
  ["shortcuts", "input"],
  ["tools", "tools"],
  ["appearance", "appearance"],
  ["auto-continue-tasks", "tasks"],
  ["compile", "build"],
  ["agent", "ai"],
  ["config-files", "ai"],
  ["frps", "network"],
  ["update", "maintenance"],
]) {
  assert.equal(
    vm.runInContext(`settingsCategoryForTab("${tab}").key`, context),
    category,
    `${tab} should belong to the ${category} category`,
  );
}

for (const [legacy, current] of [
  ["workspace", "system"],
  ["display", "appearance"],
  ["theme", "appearance"],
  ["font", "appearance"],
]) {
  assert.equal(
    vm.runInContext(`normalizeSettingsTab("${legacy}")`, context),
    current,
    `legacy settings path ${legacy} should remain compatible`,
  );
}

assert.equal(
  (appHtml.match(/data-settings-category="/g) || []).length,
  10,
  "settings HTML should render all ten top-level category buttons",
);
assert.match(
  appHtml,
  /styles-base\.css\?v=20260806b[\s\S]*styles-settings\.css\?v=20260802g[\s\S]*app-terminal-tools-settings\.js\?v=20260727c[\s\S]*app-settings-categories\.js\?v=20260727b[\s\S]*app-navigation-tabs\.js\?v=20260725e[\s\S]*app-core-event-bindings\.js\?v=20260806a[\s\S]*app-settings-load-save\.js\?v=20260730a/,
  "split settings styles should load before the category registry and navigation",
);
assert.match(
  appHtml,
  /id="settings-panel-system"[\s\S]*id="terminal-user-select"[\s\S]*<\/section>[\s\S]*id="settings-panel-terminal"[\s\S]*id="terminal-rename-presets-input"/,
  "terminal behavior controls should live outside the system panel",
);
assert.match(
  appJs,
  /const settingsCategoryButtons = document\.querySelectorAll\([\s\S]*settings-category-button/,
  "app initialization should bind category buttons rather than the removed flat tab list",
);
assert.match(
  settingsApiRs,
  /async fn persist_merged_fields\([\s\S]*save_settings\(manager, request\)\.await\?/,
  "remote settings merge should persist through the authoritative save path",
);
assert.doesNotMatch(
  settingsApiRs,
  /Full implementation would update manager|let _ = merged/,
  "remote settings merge must not retain the previous no-op placeholder",
);

for (const [id, defaultPath] of [
  ["workspace-browser-icon-path-input", "icon.ico"],
  ["terminal-workspace-icon-path-input", "static/favicon.svg"],
]) {
  const escapedDefaultPath = defaultPath.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  assert.match(
    appHtml,
    new RegExp(`id="${id}"[\\s\\S]*?placeholder="${escapedDefaultPath}"`),
    `${id} should be configurable from the Appearance panel`,
  );
}
assert.match(appJs, /workspaceBrowserIconPath:\s*DEFAULT_WORKSPACE_BROWSER_ICON_PATH/);
assert.match(appJs, /terminalWorkspaceIconPath:\s*DEFAULT_TERMINAL_WORKSPACE_ICON_PATH/);

// --- Workflow builder terminology and editor controls ---

assert.match(
  appHtml,
  /data-settings-category="tools"[\s\S]*?>\s*工作流\s*<\/button>/,
  "the tools category button should display 工作流",
);
assert.match(
  appHtml,
  /id="settings-panel-tools"[\s\S]*?工作流搭建/,
  "the tools panel heading should be 工作流搭建",
);
assert.match(
  categoriesJs,
  /key: "tools",\s+label: "工作流"/,
  "the settings registry should label the tools category 工作流",
);
