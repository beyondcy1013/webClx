import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const devinManagerJs = readFileSync(
  new URL("../static/app-devin-manager.js", import.meta.url),
  "utf8",
);
const codexAccountManagerJs = readFileSync(
  new URL("../static/app-codex-account-manager.js", import.meta.url),
  "utf8",
);

// 1. Script tag in index.html
assert.match(indexHtml, /<script defer src="\/assets\/app-devin-manager\.js\?v=[^"]+"><\/script>/);

// 2. Devin tab in account-type-tabs
assert.match(
  indexHtml,
  /<button class="tab-button account-type-tab" data-kind="devin" role="tab" aria-selected="false">Devin<\/button>/,
);

// 3. Toolbar buttons
assert.match(indexHtml, /id="codex-account-add-devin"[^>]*data-account-kinds="devin"/);
assert.match(indexHtml, /id="devin-import-current-toolbar"[^>]*data-account-kinds="devin"/);
assert.match(indexHtml, /id="codex-account-export"[^>]*data-account-kinds="[^"]*devin[^"]*"/);

// 4. Global options bar for Devin
assert.match(indexHtml, /id="devin-options-bar"/);
assert.match(indexHtml, /id="devin-option-bypass"/);
assert.match(indexHtml, /id="devin-options-note"/);

// 5. Devin Add Dialog
assert.match(indexHtml, /<dialog id="devin-add-dialog"/);
assert.match(indexHtml, /id="devin-add-token"/);
assert.match(indexHtml, /id="devin-add-email"/);
assert.match(indexHtml, /id="devin-add-org-id"/);
assert.match(indexHtml, /id="devin-add-plan"/);
assert.match(indexHtml, /id="devin-add-name"/);
assert.match(indexHtml, /id="devin-import-current-btn"/);

// 6. Devin Manager JS exports and endpoints
assert.match(devinManagerJs, /window\.WebClxDevinManager\s*=\s*Object\.freeze/);
assert.match(devinManagerJs, /loadAccounts/);
assert.match(devinManagerJs, /switchAccount/);
assert.match(devinManagerJs, /deleteAccount/);
assert.match(devinManagerJs, /importCurrentAccount/);
assert.match(devinManagerJs, /refreshCliOptions/);
assert.match(devinManagerJs, /saveCliOptions/);
assert.match(devinManagerJs, /\/api\/auth\/devin-accounts/);
assert.match(devinManagerJs, /\/api\/auth\/devin-options/);
assert.match(devinManagerJs, /\/api\/auth\/devin-accounts\/import-current/);

// 7. Codex Account Manager JS integration
assert.match(codexAccountManagerJs, /ACCOUNT_KINDS\s*=\s*\[[^\]]*"devin"[^\]]*\]/);
assert.match(codexAccountManagerJs, /devin:\s*"devin_presets"/);
assert.match(codexAccountManagerJs, /"devin_presets"/);
assert.match(codexAccountManagerJs, /WebClxDevinManager\.loadAccounts/);
assert.match(codexAccountManagerJs, /WebClxDevinManager\.switchAccount/);
assert.match(codexAccountManagerJs, /WebClxDevinManager\.deleteAccount/);
assert.match(codexAccountManagerJs, /syncDevinOptionsBarVisibility/);

console.log("devin-account-manager assertions passed successfully!");
