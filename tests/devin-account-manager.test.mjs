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
assert.match(indexHtml, /id="devin-import-desktop-toolbar"[^>]*data-account-kinds="devin"/);
assert.match(indexHtml, /id="devin-detect-desktop"[^>]*data-account-kinds="devin"/);
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
assert.match(devinManagerJs, /\/api\/auth\/devin-accounts\/detect-desktop/);
assert.match(devinManagerJs, /\/api\/auth\/devin-accounts\/import-desktop-current/);

// 7. Codex Account Manager JS integration
assert.match(codexAccountManagerJs, /ACCOUNT_KINDS\s*=\s*\[[^\]]*"devin"[^\]]*\]/);
assert.match(codexAccountManagerJs, /devin:\s*"devin_presets"/);
assert.match(codexAccountManagerJs, /"devin_presets"/);
assert.match(codexAccountManagerJs, /WebClxDevinManager\.loadAccounts/);
assert.match(codexAccountManagerJs, /WebClxDevinManager\.switchAccount/);
assert.match(codexAccountManagerJs, /WebClxDevinManager\.deleteAccount/);
assert.match(codexAccountManagerJs, /syncDevinOptionsBarVisibility/);
assert.match(codexAccountManagerJs, /account\.clientType === "desktop"/);
assert.match(codexAccountManagerJs, /"Devin Desktop"/);
assert.match(codexAccountManagerJs, /desktopAction\(account, "backup"\)/);
assert.match(codexAccountManagerJs, /desktopAction\(account, "restore"\)/);
const desktopActionStart = codexAccountManagerJs.indexOf(
  'const actionItems = account.kind === "devin" && account.clientType === "desktop"',
);
const desktopBranch = codexAccountManagerJs.indexOf('      const status =', desktopActionStart);
assert.ok(desktopActionStart >= 0);
assert.ok(desktopBranch > desktopActionStart);
assert.doesNotMatch(
  codexAccountManagerJs.slice(desktopActionStart, desktopBranch).split(': [')[0],
  /applyAccount\(account\)/,
);

console.log("devin-account-manager assertions passed successfully!");

// Execute the production quota cell with a minimal DOM and mocked transport.
const { runInNewContext } = await import("node:vm");
let quotaCalls = 0;
let quotaResponse = { overage_balance: 0, queried_at: Date.now() };
const fakeDocument = {
  createElement() {
    return { children: [], listeners: {}, dataset: {}, replaceChildren(...children) { this.children = children; }, append(...children) { this.children.push(...children); },
      setAttribute() {}, addEventListener(name, handler) { this.listeners[name] = handler; } };
  },
};
const fakeWindow = { addEventListener() {} };
runInNewContext(devinManagerJs, {
  document: fakeDocument, window: fakeWindow,
  requestJson: async () => { quotaCalls++; return quotaResponse; },
});
const quotaManager = fakeWindow.WebClxDevinManager;
const quotaAccount = { kind: "devin", clientType: "cli", id: "quota-test", name: "Test" };
const flush = () => new Promise((resolve) => setImmediate(resolve));
const zeroCell = quotaManager.quotaCell(quotaAccount);
await flush();
assert.equal(zeroCell.children[0].textContent, "0.00 ");
quotaManager.quotaCell(quotaAccount);
await flush();
assert.equal(quotaCalls, 1, "fresh cached balance avoids repeated requests on table redraw");
quotaResponse = { overage_balance: 7.65, queried_at: Date.now() };
await zeroCell.children[1].listeners.click();
await flush();
assert.equal(quotaCalls, 2);
assert.equal(zeroCell.children[0].textContent, "7.65 ");
quotaResponse = {};
zeroCell.children[1].listeners.click();
await flush();
assert.equal(zeroCell.children[0].textContent, "查询失败 ");
assert.equal(quotaManager.quotaCell({ ...quotaAccount, clientType: "desktop" }).textContent, "不支持查询");
assert.equal(quotaCalls, 3, "Desktop summary never queries using CLI credentials");
console.log("Devin quota cell behavior passed");

quotaResponse = { overage_balance: -0.04, queried_at: Date.now(), is_quota_plan: true,
  has_quota_allocation: true, daily_percentage: 0, weekly_percentage: 83 };
zeroCell.children[1].listeners.click();
await flush();
const includedCell = quotaManager.quotaCell(quotaAccount, true);
await flush();
assert.equal(includedCell.children[0].textContent, "日 100% / 周 17% ");
assert.equal(zeroCell.children[0].textContent, "-0.04 ");
quotaResponse = { ...quotaResponse, hide_daily_quota: true };
includedCell.children[1].listeners.click();
await flush();
assert.equal(includedCell.children[0].textContent, "周 17% ");

quotaResponse = { ...quotaResponse, hide_daily_quota: false,
  daily_reset_at: new Date(Date.now() + 3660000).toISOString(),
  weekly_reset_at: new Date(Date.now() - 60000).toISOString() };
includedCell.children[1].listeners.click();
await flush();
const resetLines = includedCell.children[2].children;
assert.match(resetLines[0].textContent, /日额度：还有1小时 .*分钟；重设时间：/);
assert.match(resetLines[1].textContent, /周额度：已到重设时间，请刷新额度；重设时间：/);
quotaResponse = { ...quotaResponse, hide_daily_quota: true, weekly_reset_at: "invalid" };
includedCell.children[1].listeners.click();
await flush();
assert.equal(includedCell.children[2].children.length, 1);
assert.equal(includedCell.children[2].children[0].textContent, "周额度：重设时间未知");
console.log("Devin quota reset times and countdown passed");

// Subscription metadata uses a separate request and persists on the server.
let subscriptionCalls = 0;
let subscriptionError = false;
let subscriptionResponse = { slug: "pro", status: "active", current_period_end: Math.floor(Date.now() / 1000) + 172800,
  cancel_at: null, end_of_cycle_slug: null, queried_at: Date.now() };
const subscriptionWindow = { addEventListener() {} };
runInNewContext(devinManagerJs, {
  document: fakeDocument, window: subscriptionWindow,
  requestJson: async (url, options) => {
    subscriptionCalls++;
    assert.equal(url, "/api/auth/devin-accounts/subscription-test/subscription/refresh");
    assert.equal(options.method, "POST");
    if (subscriptionError) throw new Error("订阅接口返回 HTTP 401");
    return subscriptionResponse;
  },
});
const subscriptionManager = subscriptionWindow.WebClxDevinManager;
const subscriptionAccount = { kind: "devin", clientType: "cli", id: "subscription-test" };
const subscriptionCell = subscriptionManager.subscriptionCell(subscriptionAccount);
subscriptionManager.subscriptionCell(subscriptionAccount);
await flush();
assert.equal(subscriptionCalls, 1, "simultaneous renders merge requests");
assert.match(subscriptionCell.children[0].textContent, /pro · 有效/);
assert.match(subscriptionCell.children[2].textContent, /续费时间：.*还有 2 天/);
subscriptionManager.subscriptionCell(subscriptionAccount);
await flush();
assert.equal(subscriptionCalls, 1, "redraw reuses the successful subscription snapshot");

subscriptionResponse = { ...subscriptionResponse, cancel_at: subscriptionResponse.current_period_end - 86400 };
await subscriptionCell.children[1].listeners.click();
assert.match(subscriptionCell.children[2].textContent, /套餐结束时间：.*还有 1 天/);
subscriptionResponse = { ...subscriptionResponse, end_of_cycle_slug: "free" };
await subscriptionCell.children[1].listeners.click();
assert.match(subscriptionCell.children[2].textContent, /套餐变更时间：.*转为 free/);

subscriptionResponse = { ...subscriptionResponse, status: "canceled", current_period_end: null, cancel_at: null, end_of_cycle_slug: null };
await subscriptionCell.children[1].listeners.click();
assert.match(subscriptionCell.children[0].textContent, /已取消/);
assert.equal(subscriptionCell.children[2].textContent, "接口未提供套餐到期时间", "missing date clears old date");
subscriptionError = true;
await subscriptionCell.children[1].listeners.click();
assert.match(subscriptionCell.children[0].textContent, /查询失败；上次结果：pro · 已取消/);
assert.match(subscriptionCell.children[0].title, /HTTP 401.*上次成功/);
const beforeDesktop = subscriptionCalls;
assert.equal(subscriptionManager.subscriptionCell({ ...subscriptionAccount, clientType: "desktop" }).textContent, "不支持查询");
assert.equal(subscriptionCalls, beforeDesktop);

const persistedWindow = { addEventListener() {} };
runInNewContext(devinManagerJs, {
  document: fakeDocument, window: persistedWindow,
  requestJson: async () => { throw new Error("offline"); },
});
const persistedCell = persistedWindow.WebClxDevinManager.subscriptionCell({ ...subscriptionAccount,
  subscription: { ...subscriptionResponse, queried_at: Date.now() - 120000 } });
await flush();
assert.match(persistedCell.children[0].textContent, /查询失败；上次结果：pro · 已取消/);
console.log("Devin subscription expiry, refresh, cache and failure behavior passed");
