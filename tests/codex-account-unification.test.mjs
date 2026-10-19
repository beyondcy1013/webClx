import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const authCore = readFileSync(new URL("../crates/auth_core/src/lib.rs", import.meta.url), "utf8");
const authStorage = readFileSync(new URL("../crates/auth_core/src/storage.rs", import.meta.url), "utf8");
const authBackend = readFileSync(new URL("../src/auth.rs", import.meta.url), "utf8");
const authRoutes = readFileSync(new URL("../src/routes/auth.rs", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const accountManager = readFileSync(new URL("../static/app-codex-account-manager.js", import.meta.url), "utf8");
const authStyles = readFileSync(new URL("../static/styles-auth.css", import.meta.url), "utf8");

assert.match(
  authStorage,
  /write_login_auth_file[\s\S]{0,220}oauth_auth\.openai_api_key = None/,
  "OAuth auth.json writes must always clear an API key",
);
assert.match(
  authStorage,
  /normalize_auth_preset[\s\S]{0,160}preset\.auth\.openai_api_key = None/,
  "legacy OAuth presets must be normalized away from mixed API credentials",
);
assert.match(
  authCore,
  /if current_mode != CurrentAuthMode::Api[\s\S]{0,220}Some\(false\)/,
  "API accounts must not remain active when the authoritative mode is OAuth",
);
assert.match(authBackend, /pub async fn list_codex_accounts/);
const listCodexAccountsBody = authBackend.slice(
  authBackend.indexOf("pub async fn list_codex_accounts"),
  authBackend.indexOf("pub async fn apply_codex_account"),
);
assert.doesNotMatch(
  listCodexAccountsBody,
  /accounts\.sort_by/,
  "the unified account list must preserve the manual OAuth/API preset order",
);
assert.match(authBackend, /pub async fn apply_codex_account/);
assert.match(authRoutes, /"\/api\/auth\/codex-accounts"/);
assert.match(authRoutes, /"\/api\/auth\/codex-accounts\/\{kind\}\/\{preset_id\}\/apply"/);
assert.match(indexHtml, /id="codex-account-list"/);
assert.match(
  indexHtml,
  /codex-account-table[\s\S]*?<th>测试结果<\/th>[\s\S]*?id="codex-account-list"/,
  "the unified LLM API account table should expose a dedicated test result column",
);
assert.match(
  accountManager,
  /async function testAccount\(account\)[\s\S]*?account\.kind === "claude"[\s\S]*?await testClaudePreset\(account\.id, account\.name\)[\s\S]*?renderAccounts\(allAccounts\)/,
  "Claude account tests should visibly rerender the unified account table",
);
assert.match(
  accountManager,
  /createCurrentIndicatorCell\(false, \{[\s\S]*?testResult: accountTestResult\(account\)[\s\S]*?showUntested: true/,
  "every unified account row should display its test state in the result column",
);
assert.doesNotMatch(indexHtml, /id="codex-account-mobile-list"/);
assert.doesNotMatch(indexHtml, /id="tab-auth"/);
assert.match(indexHtml, /data-kind="oauth"[^>]*>Codex_OAuth<\/button>/);
assert.match(indexHtml, /data-kind="api"[^>]*>Codex_API<\/button>/);
assert.match(indexHtml, />\s*LLM_API 账号\s*</);
assert.match(accountManager, /installOauthEditor\(\)/);
assert.match(accountManager, /accountList\.insertAdjacentElement\("afterend", editor\)/);
assert.match(accountManager, /requestJson\("\/api\/auth\/codex-accounts"\)/);
assert.match(accountManager, /requestJson\("\/api\/auth\/api-presets"\)/);
assert.match(
  accountManager,
  /orderCodexAccountsByPresetTables\(codexResponse\?\.accounts, apiPresetResponse\?\.presets\)/,
  "the unified Codex_API table should follow the manually persisted API preset order",
);
assert.match(
  accountManager,
  /actionCell\.appendChild\(\s*createPresetActionMenu\(actionItems/,
  "row actions should collapse into a single popup-menu trigger so the sticky column stays narrow on mobile",
);
assert.match(
  accountManager,
  /createPresetActionMenu\(actionItems, \{ label: `\$\{account\.name \|\| "账号"\} 的操作` \}\)/,
);
assert.match(
  accountManager,
  /const actionItems = \[\s*\{\s*label: "切换到此账号",\s*disabled: Boolean\(account\.active\)/,
  "every account kind, including CodeBuddy/Devin, must expose a switch entry in the row menu",
);
assert.match(
  accountManager,
  /label: "删除", danger: true/,
  "the destructive delete entry must be styled as a danger menu item",
);
assert.match(
  accountManager,
  /row\.append\(\s*selectionCell,\s*actionCell/,
  "the unified account table should keep selection before row actions",
);
assert.match(
  accountManager,
  /account\.kind === "api"[\s\S]*label: "测试"/,
  "Codex API rows should expose a test entry in the shared account table",
);
assert.match(
  authStyles,
  /@media \(max-width: 720px\)[\s\S]*\.codex-account-desktop-wrap\s*\{[\s\S]*display:\s*block;[\s\S]*overflow-x:\s*auto;[\s\S]*\.codex-account-table th:first-child,[\s\S]*position:\s*sticky;/,
  "mobile Codex accounts should use a horizontally scrollable table with a sticky operation column",
);
assert.match(
  accountManager,
  /\/api\/auth\/codex-accounts\/\$\{encodeURIComponent\(account\.kind\)\}\/\$\{encodeURIComponent\(account\.id\)\}\/apply/,
);
