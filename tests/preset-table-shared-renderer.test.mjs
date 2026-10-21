import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readEntryScriptBundle("index.html");
const stylesCss = [
  "../static/styles.css",
  "../static/styles-base.css",
  "../static/styles-auth.css",
  "../static/styles-responsive.css",
].map((path) => readFileSync(new URL(path, import.meta.url), "utf8")).join("\n");

assert.match(appJs, /function renderPresetTable\(/, "preset tables should share a table body renderer");
assert.match(appJs, /function renderPresetTableHeader\(/, "preset tables should share dynamic header rendering");

const renderTableCalls = appJs.match(/renderPresetTable\(\{/g) || [];
assert.ok(
  renderTableCalls.length >= 3,
  "Codex_OAuth, Codex_API, and Claude_API should all render through renderPresetTable",
);

assert.match(
  appJs,
  /function renderAuthPresets\(presets\) \{[\s\S]*?collectPresetConfigKeys\(presets\)[\s\S]*?renderAuthPresetTableHeader\(configKeys, \{/,
  "Codex_OAuth preset table should render dynamic config.toml override columns",
);

assert.match(
  appJs,
  /function renderAuthPresetTableHeader\(configKeys, options = \{\}\) \{[\s\S]*?renderPresetTableHeader\(\{[\s\S]*?configTitlePrefix: "config\.toml: "/,
  "Codex_OAuth dynamic headers should identify config.toml override columns",
);

assert.match(
  appJs,
  /function renderAuthPresets\(presets\) \{[\s\S]*?const configValues = buildPresetConfigValueMap\(preset\)[\s\S]*?const configCells = buildPresetConfigCells\(configKeys, configValues\)/,
  "Codex_OAuth preset rows should include config override values",
);

assert.match(
  appJs,
  /deleteAuthPreset\(preset\.id, preset\.name\)/,
  "Codex_OAuth delete action should remain wired",
);
assert.match(
  appJs,
  /deleteApiPreset\(preset\.id, preset\.name\)/,
  "Codex_API delete action should remain wired",
);
assert.match(
  appJs,
  /api-terminal-env-input/,
  "Codex_API preset form should include terminal env input wiring",
);
assert.match(
  appJs,
  /terminal_env: parseTerminalStartupEnvInput/,
  "Codex_API save payload should include terminal env entries",
);
assert.match(
  appJs,
  /terminal_startup_script: apiTerminalStartupScriptInputEl\.value/,
  "Codex_API save payload should include terminal startup script",
);
assert.match(
  appJs,
  /function apiPresetHasTerminalStartupSettings\(preset\) \{[\s\S]*?normalizeTerminalStartupEnvVars\(preset\?\.terminal_env\)[\s\S]*?preset\?\.terminal_startup_script/,
  "Codex_API terminal startup editor should detect saved env vars or scripts",
);
assert.match(
  appJs,
  /function editApiPreset\(presetId\) \{[\s\S]*?if \(apiTerminalStartupDetailsEl\) \{[\s\S]*?apiTerminalStartupDetailsEl\.open = apiPresetHasTerminalStartupSettings\(preset\);[\s\S]*?\}/,
  "Codex_API terminal startup editor should auto-expand only when the preset has env vars or a script",
);
assert.match(
  appJs,
  /baseLabels: \[[\s\S]*"启动环境"[\s\S]*"启动脚本"/,
  "Codex_API preset table should display terminal startup fields",
);
assert.match(
  appJs,
  /querySelectorAll\("\.config-override-row"\)/,
  "config override editor should collect compact table rows",
);
assert.match(
  appJs,
  /function createConfigOverrideTable[\s\S]*?"键名"[\s\S]*?"键值"/,
  "config override editor should render key/value columns in a compact table",
);
assert.match(
  appJs,
  /placeholder = "model 或 features\.goals"/,
  "config override key input should hint dotted second-level config keys",
);
assert.doesNotMatch(
  appJs,
  /function createConfigOverrideCard/,
  "config override editor should no longer render each item as a separate card",
);
assert.match(
  appJs,
  /deleteClaudePreset\(preset\.id, preset\.name\)/,
  "Claude_API delete action should remain wired",
);
assert.doesNotMatch(
  indexHtml,
  /id="api-upstream-proxy-toggle"/,
  "Codex_API should no longer expose a global upstream proxy toggle",
);
assert.doesNotMatch(
  appJs,
  /handleApiUpstreamProxyToggleChange/,
  "Codex_API should no longer save a global upstream proxy toggle",
);
assert.match(
  indexHtml,
  /id="api-apply-upstream-proxy-on-switch"/,
  "Codex_API preset editor should keep the per-preset upstream proxy option",
);
assert.match(
  appJs,
  /function syncApiApplyProxyRecommendation/,
  "Codex_API preset editor should recommend the per-preset upstream proxy option when known providers need it",
);
assert.doesNotMatch(
  appJs,
  /syncApiApplyProxyForcedState/,
  "Codex_API preset editor should no longer force-lock the per-preset upstream proxy option",
);
assert.match(
  appJs,
  /claude-upstream-proxy-toggle/,
  "Claude upstream proxy toggle should be wired",
);
assert.match(
  indexHtml,
  /id="claude-current-file"/,
  "Claude_API page should include the current settings file status target used by app.js",
);
assert.match(
  indexHtml,
  /id="claude-current-target"/,
  "Claude_API page should include the current Claude summary target used by app.js",
);
assert.match(
  indexHtml,
  /id="claude-preset-file"/,
  "Claude_API page should include the preset file status target used by app.js",
);
assert.match(
  appJs,
  /updateUpstreamProxySettings/,
  "proxy toggle changes should save through the backend",
);
assert.match(
  stylesCss,
  /\.auth-table-wrap\s*\{[\s\S]*touch-action:\s*pan-x pan-y;/,
  "preset tables should allow vertical page scrolling while preserving horizontal table scrolling",
);
assert.doesNotMatch(
  stylesCss,
  /\.auth-table-wrap\s*\{[\s\S]*touch-action:\s*pan-x;\s*[\s\S]*?\}/,
  "preset tables should not restrict touch gestures to horizontal panning only",
);
assert.match(
  stylesCss,
  /\.active-auth-row td\s*\{[^}]*font-weight:\s*700;/,
  "the active preset row should use bold text in addition to the current arrow",
);
assert.match(
  stylesCss,
  /\.active-auth-row\s*\{[^}]*filter:\s*drop-shadow\(0 1px 2px rgba\(31, 43, 38, 0\.08\)\);/,
  "the active preset row should have a subtle continuous shadow",
);
assert.match(
  stylesCss,
  /:root\[data-theme="dark"\] \.active-auth-row\s*\{[^}]*filter:\s*drop-shadow\(0 1px 3px rgba\(223, 138, 82, 0\.28\)\);/,
  "the active preset row should use a visible warm shadow in dark mode",
);
assert.match(
  stylesCss,
  /\.auth-table \.active-auth-row td\s*\{[^}]*color:\s*#713b20;/,
  "the active preset row should use high-contrast warm text in light mode",
);
assert.match(
  stylesCss,
  /:root\[data-theme="dark"\] \.auth-table \.active-auth-row td\s*\{[^}]*color:\s*#ffc799;/,
  "the active preset row should use high-contrast warm text in dark mode",
);
assert.match(
  stylesCss,
  /\.api-mobile-preset-row\.active-auth-row \.api-mobile-preset-name,[\s\S]*?\.api-mobile-preset-row\.active-auth-row \.api-mobile-preset-meta\s*\{[^}]*color:\s*#713b20;[^}]*font-weight:\s*700;/,
  "the active mobile preset should use bold high-contrast warm text in light mode",
);
assert.match(
  stylesCss,
  /:root\[data-theme="dark"\] \.api-mobile-preset-row\.active-auth-row \.api-mobile-preset-name,[\s\S]*?:root\[data-theme="dark"\] \.api-mobile-preset-row\.active-auth-row \.api-mobile-preset-meta\s*\{[^}]*color:\s*#ffc799;/,
  "the active mobile preset should use high-contrast warm text in dark mode",
);
