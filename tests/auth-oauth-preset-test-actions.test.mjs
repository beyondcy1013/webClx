import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appCoreJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const authManagerJs = readFileSync(
  new URL("../static/app-auth-manager.js", import.meta.url),
  "utf8",
);
const presetTableJs = readFileSync(
  new URL("../static/app-preset-table.js", import.meta.url),
  "utf8",
);
const authRoutesRs = readFileSync(new URL("../src/routes/auth.rs", import.meta.url), "utf8");
const authRs = readFileSync(new URL("../src/auth.rs", import.meta.url), "utf8");
const appJs = `${appCoreJs}\n${authManagerJs}\n${presetTableJs}`;

assert.match(
  indexHtml,
  /id="auth-test-all-presets"[\s\S]*?>测试所有账号<\/button>/,
  "Codex_OAuth saved presets toolbar should expose a test-all button",
);
assert.match(
  appJs,
  /createActionButton\("测试", \(\) => testAuthPreset\(preset\.id, preset\.name\), "mini-button"\)/,
  "Codex_OAuth preset rows should expose a single-preset test action",
);
assert.match(
  appJs,
  /requestJson\(`\/api\/auth\/presets\/\$\{encodeURIComponent\(presetId\)\}\/test`,\s*\{\s*method: "POST"/,
  "Codex_OAuth single test should call its backend test endpoint",
);
assert.match(
  appJs,
  /requestJson\("\/api\/auth\/presets\/test-all", \{\s*method: "POST"/,
  "Codex_OAuth test-all should call its backend batch test endpoint",
);
assert.match(
  presetTableJs,
  /kind === "auth"[\s\S]*state\.authPresetTestResults\.get\(presetId\)/,
  "shared preset test popup should resolve Codex_OAuth test results",
);
assert.match(
  authRoutesRs,
  /"\/api\/auth\/presets\/test-all"[\s\S]*post\(auth::test_all_auth_presets\)/,
  "backend should expose the Codex_OAuth batch test endpoint",
);
assert.match(
  authRoutesRs,
  /"\/api\/auth\/presets\/\{preset_id\}\/test"[\s\S]*post\(auth::test_auth_preset\)/,
  "backend should expose the Codex_OAuth single test endpoint",
);
assert.match(
  authRs,
  /test_all_auth_presets[\s\S]*test_auth_preset/,
  "auth module should export Codex_OAuth test handlers",
);
