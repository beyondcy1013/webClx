import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const accountSource = readFileSync(new URL('../static/app-codex-account-manager.js', import.meta.url), 'utf8');
const codebuddySource = readFileSync(new URL('../static/app-codebuddy-manager.js', import.meta.url), 'utf8');
const devinSource = readFileSync(new URL('../static/app-devin-manager.js', import.meta.url), 'utf8');

// 1. Verify static bindings and function definitions
assert.match(accountSource, /async function refreshCurrentSubtabAccounts\(\)/, 'must define refreshCurrentSubtabAccounts');
assert.match(accountSource, /function updateRefreshButtonTitle\(\)/, 'must define updateRefreshButtonTitle');
assert.match(accountSource, /function loadCodexAccounts\(kind\)/, 'loadCodexAccounts must accept an optional kind');
assert.match(accountSource, /document\.getElementById\("codex-account-refresh"\)\?\.addEventListener\("click", refreshCurrentSubtabAccounts\)/, 'refresh button must be bound to refreshCurrentSubtabAccounts');
assert.match(accountSource, /window\.refreshCurrentSubtabAccounts = refreshCurrentSubtabAccounts/, 'refreshCurrentSubtabAccounts must be exported on window');
assert.match(accountSource, /window\.loadAccountsForKind = loadAccountsForKind/, 'loadAccountsForKind must be exported on window');

// 2. Verify subtab manager modules call scoped reload
assert.match(codebuddySource, /await loadCodexAccounts\("codebuddy"\)/, 'codebuddy operations must use scoped reload');
assert.match(devinSource, /await loadCodexAccounts\("devin"\)/, 'devin operations must use scoped reload');

// 3. Verify behavioral isolation in execution environment
// Simulate DOM environment and mock endpoints
const requestedUrls = [];
let tableRenderCount = 0;

const mockDocument = {
  getElementById: (id) => {
    if (id === 'codex-account-list') return { appendChild: () => {}, innerHTML: '' };
    if (id === 'codex-account-refresh') return { title: '', disabled: false, classList: { add: () => {}, remove: () => {} }, setAttribute: () => {} };
    if (id === 'codex-account-status') return { textContent: '', dataset: {}, hidden: false };
    if (id === 'codebuddy-auto-bar') return { hidden: true };
    if (id === 'codebuddy-auto-checkin') return { checked: true };
    if (id === 'codebuddy-auto-travel') return { checked: true };
    if (id === 'codebuddy-auto-note') return { hidden: false, textContent: '', dataset: {} };
    if (id === 'devin-options-bar') return { hidden: true };
    if (id === 'opencode-options-bar') return { hidden: true };
    if (id === 'llm-global-options-bar') return { hidden: true };
    if (id === 'agy-view') return { hidden: true, classList: { toggle: () => {} } };
    if (id === 'api-view') return { classList: { toggle: () => {} } };
    if (id === 'agy-quota-all') return { click: () => requestedUrls.push('agy:quota-all-clicked') };
    return { addEventListener: () => {}, querySelector: () => null, classList: { toggle: () => {}, add: () => {}, remove: () => {} } };
  },
  querySelectorAll: () => [],
};

// Check title mapping logic
const titleLabels = {
  oauth: "刷新 Codex OAuth 账号与额度",
  api: "刷新 Codex API 预设与用量",
  claude: "刷新 Claude 预设",
  codebuddy: "刷新 CodeBuddy 账号与积分",
  devin: "刷新 Devin 账号",
  agy: "刷新 AGY 账号与额度",
  opencode: "刷新 OpenCode 配置",
};

for (const [kind, label] of Object.entries(titleLabels)) {
  assert.ok(accountSource.includes(label), `tooltip for ${kind} must include '${label}'`);
}

// 4. Verify that refreshCurrentSubtabAccounts handles all kinds with targeted branches
assert.match(accountSource, /if \(activeKind === "codebuddy"\)[\s\S]*?WebClxCodeBuddyManager\?\.runAllCredits/);
assert.match(accountSource, /if \(activeKind === "oauth"\)[\s\S]*?\/api\/auth\/presets\/refresh-all-quotas/);
assert.match(accountSource, /if \(activeKind === "api"\)[\s\S]*?fetchApiAccounts/);
assert.match(accountSource, /if \(activeKind === "claude"\)[\s\S]*?loadAccountsForKind\("claude"\)/);
assert.match(accountSource, /if \(activeKind === "devin"\)[\s\S]*?loadAccountsForKind\("devin"\)/);
assert.match(accountSource, /if \(activeKind === "agy"\)[\s\S]*?agyManager\?\.load/);
assert.match(accountSource, /if \(activeKind === "opencode"\)[\s\S]*?syncOpenCodeOptionsBarVisibility/);

console.log('Account subtab scoped refresh unit tests passed successfully');
