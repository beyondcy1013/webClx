import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const apiManagerSource = readFileSync(
  new URL("../static/app-api-manager.js", import.meta.url),
  "utf8",
);
const configOverrideSource = readFileSync(
  new URL("../static/app-config-override.js", import.meta.url),
  "utf8",
);
const presetTableSource = readFileSync(
  new URL("../static/app-preset-table.js", import.meta.url),
  "utf8",
);
const authStyles = readFileSync(new URL("../static/styles-auth.css", import.meta.url), "utf8");
const responsiveStyles = readFileSync(
  new URL("../static/styles-responsive.css", import.meta.url),
  "utf8",
);

const apiHeaderRenderer = configOverrideSource.match(
  /function renderApiPresetTableHeader[\s\S]*?\n}\n\nfunction renderClaudePresetTableHeader/,
)?.[0] || "";

assert.match(
  indexHtml,
  /id="api-preset-search"[^>]+type="search"/,
  "Codex API presets should provide a dedicated name/model/URL search field",
);
assert.match(
  indexHtml,
  /id="api-preset-selection-mode"[^>]+aria-pressed="false"/,
  "preset selection should be an explicit mode instead of a permanent leading column",
);
assert.match(
  indexHtml,
  /id="api-preset-mobile-list"[^>]+aria-live="polite"/,
  "mobile users should receive a dedicated compact preset list",
);

for (const legacyActionColumn of ["切换", "临切", "测试", "编辑", "删除"]) {
  assert.doesNotMatch(
    apiHeaderRenderer,
    new RegExp(`label: "${legacyActionColumn}"`),
    `the desktop table should not keep a dedicated ${legacyActionColumn} column`,
  );
}
assert.match(
  apiHeaderRenderer,
  /\{ label: "序号" \}[\s\S]*\{ label: "操作", className: "api-preset-operation-cell" \}[\s\S]*\{ label: "状态指示"[\s\S]*\{ label: "名字"[\s\S]*\{ label: "Base URL"/,
  "the consolidated action column should follow the sequence and precede status and identity columns",
);

assert.match(
  presetTableSource,
  /function createPresetActionMenu\(/,
  "desktop and mobile preset rows should share one accessible action menu renderer",
);
assert.match(
  presetTableSource,
  /aria-haspopup["']?,\s*["']menu["']/,
  "the more-actions trigger should expose its menu semantics",
);
assert.match(
  apiManagerSource,
  /function apiPresetRowActions\(preset\)[\s\S]*切换并启动[\s\S]*测试[\s\S]*编辑[\s\S]*删除/,
  "all secondary row actions should come from one shared descriptor list",
);
assert.match(
  apiManagerSource,
  /function renderApiPresetMobileList\([\s\S]*apiPresetRowActions\(preset\)/,
  "the compact mobile list should reuse the shared row action definitions",
);
assert.match(
  apiManagerSource,
  /apiPresetSearchInputEl\.addEventListener\("input"[\s\S]*renderApiPresets\(state\.apiPresets\)/,
  "search should filter the already-loaded preset collection without another request",
);

assert.match(
  authStyles,
  /\.api-preset-mobile-list\s*\{[^}]*display:\s*none;/,
  "the compact list should stay out of the desktop layout",
);
assert.match(
  responsiveStyles,
  /@media \(max-width:\s*760px\)[\s\S]*#api-view \.api-desktop-table-wrap\s*\{[^}]*display:\s*none;[\s\S]*#api-view \.api-preset-mobile-list\s*\{[^}]*display:\s*block;/,
  "phone layouts should replace the wide table with the compact list",
);
assert.match(
  responsiveStyles,
  /\.api-mobile-preset-summary[\s\S]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+auto\s+auto;/,
  "mobile identity and two stable action targets should share one compact row",
);

console.log("Codex API compact preset UI contract verified");
