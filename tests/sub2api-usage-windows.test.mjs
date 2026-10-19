import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const manager = readFileSync(new URL("../static/app-sub2api-manager.js", import.meta.url), "utf8");
const routes = readFileSync(new URL("../src/routes/auth.rs", import.meta.url), "utf8");
const backend = readFileSync(new URL("../src/auth.rs", import.meta.url), "utf8");
const sub2api = readFileSync(new URL("../src/auth/sub2api.rs", import.meta.url), "utf8");

// 后端必须暴露 sub2api 账号表格同款批量用量接口。
assert.match(
  routes,
  /\/api\/auth\/sub2api\/usage-batch"[\s\S]{0,120}query_sub2api_accounts_usage_batch/,
  "usage-batch route must be registered",
);
assert.match(backend, /pub async fn query_sub2api_accounts_usage_batch/);
assert.match(
  sub2api,
  /admin\/accounts\/usage\/batch/,
  "backend must proxy POST /admin/accounts/usage/batch",
);

// 账号行渲染后必须自动加载用量窗口（学习 sub2api 表格挂载行为）。
assert.match(
  manager,
  /updateSelectedCount\(\);\s*void autoLoadUsageWindows\(\);/,
  "renderAccounts must auto-load usage windows",
);
assert.match(manager, /sub2api\/usage-batch/, "frontend must call the batch endpoint");

// 批量接口不可用时要回退到逐账号查询，Anthropic 走 passive、OpenAI 走主动。
assert.match(manager, /loadUsageWindowsIndividually/);
assert.match(manager, /source:\s*"passive"|"passive"\s*:\s*undefined|\? "passive"/);
assert.match(manager, /platform === "anthropic" \|\| platform === "claude"/);
assert.match(manager, /platform === "openai" && kind === "oauth"/);

// 用量单元格必须渲染 5h/7d 以及 sub2api 的 Sonnet/Fable 附加窗口。
for (const field of ["five_hour", "seven_day", "seven_day_sonnet", "seven_day_fable"]) {
  assert.ok(manager.includes(`usage.${field}`), `missing ${field} rendering`);
}

// 单击单元格仍是主动 force 查询；不支持的类型显示占位符而不是报错。
assert.match(manager, /queryUsageInto\(usageEl, baseUrl, \{ force: true \}\)/);
assert.match(manager, /does not support usage query/);

// 后端 UsageInfo 必须透传 sub2api 的附加窗口与降级错误字段。
for (const field of ["seven_day_sonnet", "seven_day_fable", "window_stats"]) {
  assert.ok(sub2api.includes(field), `Sub2ApiUsageInfo missing ${field}`);
}

// 统一账号表（codex_oauth / codex_api 页签）必须有同款用量窗口列。
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const accountManager = readFileSync(
  new URL("../static/app-codex-account-manager.js", import.meta.url),
  "utf8",
);
const presetOps = readFileSync(new URL("../static/app-preset-ops.js", import.meta.url), "utf8");
const authManagerJs = readFileSync(new URL("../static/app-auth-manager.js", import.meta.url), "utf8");
assert.match(indexHtml, /<th class="usage-window-column">用量窗口<\/th>/);
assert.match(accountManager, /usageWindowCell\(account\)/);
// 窗口行渲染成 sub2api UsageProgressBar 同款：徽标+迷你进度条+百分比+倒计时。
assert.match(accountManager, /usageWindowBar\("5h", quota\.hourly_percentage/);
assert.match(accountManager, /usageWindowBar\("7d", quota\.weekly_percentage/);
assert.match(accountManager, /formatUsageCountdown[\s\S]{0,400}待刷新/);
// 操作行对齐 sub2api：查询 / 次数 / 重置 / 点数 / 邀请。
assert.match(accountManager, /usageActionButton\("查询"/);
assert.match(accountManager, /次数 \$\{resetCount === null \? "—" : resetCount\}/);
assert.match(accountManager, /usageActionButton\("重置", \(\) => consumeOauthResetCredit/);
assert.match(accountManager, /codex-reset-credit/);
assert.match(accountManager, /codex-referral/);
assert.match(accountManager, /点数 \$\{quota\.codex_points \|\| "—"\}/);
assert.match(routes, /codex-reset-credit"[\s\S]{0,80}consume_auth_preset_reset_credit/);
assert.match(routes, /codex-referral"[\s\S]{0,120}get_auth_preset_codex_referral[\s\S]{0,60}send_auth_preset_codex_referral/);
assert.match(backend, /consume_auth_preset_reset_credit/);
assert.match(backend, /send_auth_preset_codex_referral/);
// API 行自动查询覆盖智谱 / MiniMax / OpenRouter 三类可识别平台。
for (const platform of ["ZHIPU", "MINIMAX", "OPENROUTER"]) {
  assert.ok(accountManager.includes(`"${platform}"`), `missing ${platform} usage branch`);
}
assert.match(accountManager, /\/api\/quota\/query\?/);
// oauth 行单击触发既有 refresh-quota 主动查询。
assert.match(accountManager, /refreshAuthPresetQuota\(account\.id/);
// 统一账号表按全局函数裸调用 auth 操作，app-preset-ops.js 必须提供
// testAuthPreset / refreshAuthPresetQuota 包装并委托给 authManager。
assert.match(accountManager, /testAuthPreset\(account\.id/);
assert.match(
  presetOps,
  /function testAuthPreset\(presetId, presetName\)[\s\S]{0,120}authManager\.testAuthPreset\(presetId, presetName\)/,
  "missing global testAuthPreset wrapper delegating to authManager",
);
assert.match(
  presetOps,
  /function refreshAuthPresetQuota\(presetId, presetName, statusElement\)[\s\S]{0,140}authManager\.refreshAuthPresetQuota\(presetId, presetName, statusElement\)/,
  "missing global refreshAuthPresetQuota wrapper delegating to authManager",
);
assert.match(
  presetOps,
  /function deleteAuthPreset\(presetId, presetName, statusElement\)[\s\S]{0,140}authManager\.deleteAuthPreset\(presetId, presetName, statusElement\)/,
  "missing global deleteAuthPreset wrapper delegating to authManager",
);

// 工具栏必须有「查询用量」按钮，仅在有勾选时可用，并对选中账号执行查询。
// 该按钮属于子页签操作条（llm-global-options-bar），不在顶部共享工具栏。
const globalOptionsBar = indexHtml.slice(
  indexHtml.indexOf('id="llm-global-options-bar"'),
  indexHtml.indexOf('class="table-wrap auth-table-wrap codex-account-desktop-wrap"'),
);
assert.match(indexHtml, /id="codex-account-query-usage"[^>]*disabled[^>]*>查询用量</);
assert.match(
  globalOptionsBar,
  /id="codex-account-query-usage"[^>]*data-account-kinds="oauth api"[^>]*>查询用量</,
  "query-usage button must live inside the sub-tab options bar",
);
assert.match(accountManager, /codex-account-query-usage"\)\?\.addEventListener\("click", queryUsageForSelected\)/);
assert.match(accountManager, /async function queryUsageForSelected/);
const bulkQueryBody = accountManager.slice(
  accountManager.indexOf("async function queryUsageForSelected"),
);
assert.match(
  bulkQueryBody,
  /selectedAccountIds\.has\(accountKey\(account\)\)[\s\S]{0,1200}refreshAuthPresetQuota[\s\S]{0,600}queryApiUsage/,
  "bulk query must cover selected oauth (refresh-quota) and api (quota query) accounts",
);

// 刷新失败必须以 toast 透出：refreshAuthPresetQuota 返回 { ok:false, error }，
// 统一表单元格展示「刷新失败」+错误 title，单击与批量查询都弹 showToast。
assert.match(authManagerJs, /return \{ ok: false, error: message \}/);
assert.match(accountManager, /state\.authRefreshErrorsByPresetId\?\.get\(account\.id\)/);
assert.match(accountManager, /textContent = "刷新失败"/);
assert.match(accountManager, /showToast\(`\$\{account\.name \|\| "账号"\} 用量刷新失败：/);
assert.match(accountManager, /showToast\(\s*`用量查询失败 \$\{failed\} 个：/);
assert.match(bulkQueryBody, /result\.ok === false[\s\S]{0,120}throw new Error/);

console.log("sub2api usage windows: batch endpoint, auto-load, passive fallback and rendering passed");
