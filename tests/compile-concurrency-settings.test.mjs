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
const settingsApiRs = readFileSync(
  new URL("../crates/settings_core/src/api.rs", import.meta.url),
  "utf8",
);
const compileServiceRs = readFileSync(
  new URL("../src/compile_service.rs", import.meta.url),
  "utf8",
);

const systemPanel = appHtml.slice(
  appHtml.indexOf('id="settings-panel-system"'),
  appHtml.indexOf('id="settings-panel-terminal"'),
);

assert.match(
  systemPanel,
  /id="compile-max-concurrency-input"[\s\S]*?min="1"[\s\S]*?max="32"[\s\S]*?placeholder="5"/,
  "the System tab should expose the bounded compile concurrency setting",
);
assert.match(appJs, /const DEFAULT_COMPILE_MAX_CONCURRENCY\s*=\s*5/);
assert.match(appJs, /compileMaxConcurrencyInputEl/);
assert.match(loadSaveJs, /compile_max_concurrency:\s*nextCompileMaxConcurrency/);
assert.match(loadSaveJs, /settings\.compile_max_concurrency/);
assert.match(eventBindingsJs, /compileMaxConcurrencyInputEl/);
assert.match(
  settingsApiRs,
  /"system"\s*=>\s*&\[[\s\S]*?"compile_max_concurrency"/,
  "remote System-tab settings should own compile_max_concurrency",
);
assert.match(
  compileServiceRs,
  /compile_max_concurrency\(\)[\s\S]*?--max-concurrency/,
  "the compile API must pass the persisted limit to each worker",
);

console.log("compile concurrency settings contract tests passed");
