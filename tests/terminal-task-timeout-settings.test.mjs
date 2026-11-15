import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const loadSaveJs = readFileSync(
  new URL("../static/app-settings-load-save.js", import.meta.url),
  "utf8",
);
const eventBindingsJs = readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);
const settingsCoreRs = readFileSync(
  new URL("../crates/settings_core/src/lib.rs", import.meta.url),
  "utf8",
);
const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");

const terminalPanel = appHtml.slice(
  appHtml.indexOf('id="settings-panel-terminal"'),
  appHtml.indexOf('id="settings-panel-compile"'),
);

assert.match(
  terminalPanel,
  /id="terminal-task-timeout-secs-input"[\s\S]*?min="10"[\s\S]*?max="120"[\s\S]*?placeholder="30"/,
  "the Terminal tab should expose the bounded terminal task timeout",
);
assert.match(appJs, /const DEFAULT_TERMINAL_TASK_TIMEOUT_SECS\s*=\s*30/);
assert.match(appJs, /terminalTaskTimeoutSecsInputEl/);
assert.match(loadSaveJs, /settings\.terminal_task_timeout_secs/);
assert.match(loadSaveJs, /terminal_task_timeout_secs:\s*nextTerminalTaskTimeoutSecs/);
assert.match(eventBindingsJs, /terminalTaskTimeoutSecsInputEl/);
assert.match(
  appHtml,
  /app-settings-load-save\.js\?v=20261114-terminal-task-timeout[\s\S]*app-settings-event-bindings\.js\?v=20261114-terminal-task-timeout[\s\S]*app\.js\?v=20261114-terminal-task-timeout/,
  "changed settings modules should receive matching cache versions",
);
assert.match(settingsCoreRs, /DEFAULT_TERMINAL_TASK_TIMEOUT_SECS:\s*u64\s*=\s*30/);
assert.match(terminalRs, /terminal_task_timeout_secs\(\)/);

console.log("terminal task timeout settings contract tests passed");
