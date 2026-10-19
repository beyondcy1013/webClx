import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const navigationJs = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
  "utf8",
);
const accountManagerJs = readFileSync(
  new URL("../static/app-codex-account-manager.js", import.meta.url),
  "utf8",
);

assert.match(
  indexHtml,
  /id="tab-api"[^>]*data-tab="api"[^>]*>\s*LLM_API\s*<\/button>/,
  "LLM_API should remain the single first-level account tab",
);
assert.match(
  indexHtml,
  /class="tabs segmented-tabs account-type-tabs"[\s\S]*?data-kind="oauth"[^>]*>Codex_OAuth<\/button>[\s\S]*?data-kind="api"[^>]*>Codex_API<\/button>[\s\S]*?data-kind="agy"[^>]*>AGY<\/button>/,
  "Codex_OAuth, Codex_API, and AGY should stay as LLM_API sub-tabs",
);
assert.match(indexHtml, /<h2>LLM_API 账号<\/h2>/);
assert.doesNotMatch(indexHtml, /id="tab-auth"/, "Codex_OAuth should not become a top-level tab");

assert.match(accountManagerJs, /window\.location\.pathname\.toLowerCase\(\)[\s\S]*?setAccountKind\("api"\)/);
assert.match(
  accountManagerJs,
  /\["#api", "#auth", "#oauth", "#claude"\]\.includes\(window\.location\.hash\.toLowerCase\(\)\)/,
  "legacy OAuth hashes should select the OAuth sub-tab without creating a top-level route",
);
assert.match(appJs, /path === "\/codex_api\/api"[\s\S]*?return "api"/);
assert.doesNotMatch(navigationJs, /activeTab === "auth"/);
assert.match(accountManagerJs, /let activeKind = "oauth"/);
assert.match(accountManagerJs, /el\.hidden = kinds\.length > 0 && !kinds\.includes\(activeKind\);/);
assert.match(accountManagerJs, /function showOauthEditor[\s\S]*?setAccountKind\("oauth"\)/);
assert.match(
  accountManagerJs,
  /installOauthEditor\(\)[\s\S]*?apiView\?\.querySelector\("\.auth-layout-merged"\)/,
  "the OAuth editor must remain available under the LLM_API OAuth sub-tab",
);

console.log("Codex OAuth/API tab separation tests passed");
