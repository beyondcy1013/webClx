import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const apiManager = readFileSync(new URL("../static/app-api-manager.js", import.meta.url), "utf8");
const accountManager = readFileSync(new URL("../static/app-codex-account-manager.js", import.meta.url), "utf8");
const configOverride = readFileSync(new URL("../static/app-config-override.js", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const specifiedTask = readFileSync(new URL("../static/terminal-specified-task.js", import.meta.url), "utf8");
const workspaceHistory = readFileSync(new URL("../static/app-workspace-history.js", import.meta.url), "utf8");
const workspaceDesignate = readFileSync(new URL("../static/app-designate-preset.js", import.meta.url), "utf8");
const authRoutes = readFileSync(new URL("../src/routes/auth.rs", import.meta.url), "utf8");
const authCore = readFileSync(new URL("../crates/auth_core/src/models.rs", import.meta.url), "utf8");
const authSummary = readFileSync(new URL("../crates/auth_core/src/lib.rs", import.meta.url), "utf8");

test("API preset favorite state is persisted and returned", () => {
  assert.match(authCore, /pub struct StoredApiPreset[\s\S]*pub favorite: bool/);
  assert.match(authCore, /pub struct ApiPresetSummary[\s\S]*pub favorite: bool/);
  assert.match(authSummary, /favorite: preset\.favorite/);
  assert.match(authRoutes, /\/api\/auth\/api-presets\/\{preset_id\}\/favorite/);
});

test("LLM_API exposes and toggles a favorite column", () => {
  assert.match(indexHtml, /id="codex-account-list"[\s\S]*?<\/table>/);
  assert.match(indexHtml, /<th class="api-preset-favorite-cell">收藏<\/th>/);
  assert.match(accountManager, /function toggleApiAccountFavorite\(account\)/);
  assert.match(accountManager, /api-presets\/\$\{encodeURIComponent\(account\.id\)\}\/favorite/);
  assert.match(accountManager, /account\.favorite \? "★" : "☆"/);
  assert.match(accountManager, /aria-pressed/);
  assert.match(configOverride, /\{ label: "收藏", sortKey: "favorite", className: "api-preset-favorite-cell" \}/);
  assert.match(apiManager, /api-preset-favorite-button/);
  assert.match(apiManager, /preset\.favorite \? "★" : "☆"/);
  assert.match(apiManager, /aria-pressed/);
});

test("visible unified account table puts favorite API presets first", () => {
  assert.match(
    accountManager,
    /orderedAccounts\.filter\(\(account\) => account\.kind === "api" && account\.favorite === true\)[\s\S]*orderedAccounts\.filter\(\(account\) => account\.kind !== "api" \|\| account\.favorite !== true\)/,
  );
});

test("all API preset launch selectors place favorites first", () => {
  const favoriteSort = /\.sort\(\(left, right\) => Number\(right\.favorite === true\) - Number\(left\.favorite === true\)\)/;
  assert.match(specifiedTask, favoriteSort);
  assert.match(workspaceHistory, favoriteSort);
  assert.match(workspaceDesignate, favoriteSort);
});
