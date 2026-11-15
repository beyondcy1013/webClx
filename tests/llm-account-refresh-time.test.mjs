import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const stylesAuth = readFileSync(new URL("../static/styles-auth.css", import.meta.url), "utf8");
const accountManagerSource = readFileSync(
  new URL("../static/app-codex-account-manager.js", import.meta.url),
  "utf8",
);
const datetimeFormatSource = readFileSync(
  new URL("../static/app-datetime-format.js", import.meta.url),
  "utf8",
);
const agyManagerSource = readFileSync(
  new URL("../static/app-agy-manager.js", import.meta.url),
  "utf8",
);
const codebuddyManagerSource = readFileSync(
  new URL("../static/app-codebuddy-manager.js", import.meta.url),
  "utf8",
);

// 1. Verify index.html contains the "刷新时间" column header at the end of the codex-account-table
assert.match(
  indexHtml,
  /<th class="codex-account-refreshed-column">刷新时间<\/th>/,
  "index.html must include the 刷新时间 column in codex-account-table",
);
assert.match(
  indexHtml,
  /<th>使用<\/th>\s*<th class="codex-account-refreshed-column">刷新时间<\/th>/,
  "刷新时间 column must be placed at the end right after 使用",
);

// 2. Verify styles-auth.css styles the new column and cell
assert.match(
  stylesAuth,
  /\.codex-account-table\s+\.codex-account-refreshed-column/,
  "styles-auth.css must style .codex-account-refreshed-column",
);
assert.match(
  stylesAuth,
  /\.codex-account-table\s+\.codex-account-refreshed-cell/,
  "styles-auth.css must style .codex-account-refreshed-cell",
);

// 3. Evaluate formatAccountRefreshedTime from app-datetime-format.js
const fnScope = {};
const runner = new Function("exports", `${datetimeFormatSource}\nexports.formatAccountRefreshedTime = formatAccountRefreshedTime;`);
runner(fnScope);
const { formatAccountRefreshedTime } = fnScope;

assert.equal(typeof formatAccountRefreshedTime, "function", "formatAccountRefreshedTime must be a function");

const now = Date.now();
// 1 day and 20 minutes ago
const oneDayTwentyMinsAgo = now - (86400000 + 20 * 60000);
const formatted1 = formatAccountRefreshedTime(oneDayTwentyMinsAgo);
assert.match(formatted1, /^1天20分钟前 \d{2}-\d{2} \d{2}:\d{2}:\d{2}$/, "Must format as 1天20分钟前 MM-DD HH:mm:ss");
assert.doesNotMatch(formatted1, /\b\d{4}\b/, "Must NOT contain year in formatted string");

// Just now
const justNow = formatAccountRefreshedTime(now);
assert.match(justNow, /^刚刚 \d{2}-\d{2} \d{2}:\d{2}:\d{2}$/, "Must format as 刚刚 MM-DD HH:mm:ss");
assert.doesNotMatch(justNow, /\b\d{4}\b/, "Must NOT contain year");

// 5 minutes ago
const fiveMinsAgo = formatAccountRefreshedTime(now - 5 * 60000);
assert.match(fiveMinsAgo, /^5分钟前 \d{2}-\d{2} \d{2}:\d{2}:\d{2}$/, "Must format as 5分钟前 MM-DD HH:mm:ss");

// 2 hours 10 minutes ago
const twoHoursTenMinsAgo = formatAccountRefreshedTime(now - (2 * 3600000 + 10 * 60000));
assert.match(twoHoursTenMinsAgo, /^2小时10分钟前 \d{2}-\d{2} \d{2}:\d{2}:\d{2}$/, "Must format as 2小时10分钟前 MM-DD HH:mm:ss");

// 1 day 2 hours ago
const oneDayTwoHoursAgo = formatAccountRefreshedTime(now - (86400000 + 2 * 3600000));
assert.match(oneDayTwoHoursAgo, /^1天2小时前 \d{2}-\d{2} \d{2}:\d{2}:\d{2}$/, "Must format as 1天2小时前 MM-DD HH:mm:ss");

// 1 day 2 hours 20 mins ago
const oneDayTwoHoursTwentyMinsAgo = formatAccountRefreshedTime(now - (86400000 + 2 * 3600000 + 20 * 60000));
assert.match(oneDayTwoHoursTwentyMinsAgo, /^1天2小时20分钟前 \d{2}-\d{2} \d{2}:\d{2}:\d{2}$/, "Must format as 1天2小时20分钟前 MM-DD HH:mm:ss");

// 3 days ago
const threeDaysAgo = formatAccountRefreshedTime(now - 3 * 86400000);
assert.match(threeDaysAgo, /^3天前 \d{2}-\d{2} \d{2}:\d{2}:\d{2}$/, "Must format as 3天前 MM-DD HH:mm:ss");

// Empty / null
assert.equal(formatAccountRefreshedTime(null), "—");
assert.equal(formatAccountRefreshedTime(0), "—");
assert.equal(formatAccountRefreshedTime(""), "—");

// 4. Verify app-codex-account-manager.js does NOT display year in formatCreditsDate
assert.doesNotMatch(
  accountManagerSource,
  /formatCreditsDate[\s\S]*?getFullYear\(\)/,
  "formatCreditsDate must not use getFullYear() to save space",
);

// 5. Verify app-codex-account-manager.js appends accountRefreshedCell
assert.match(
  accountManagerSource,
  /accountRefreshedCell\(account\)/,
  "renderAccounts must append accountRefreshedCell",
);
assert.match(
  accountManagerSource,
  /cell\.colSpan = activeKind === "devin" \? 19 : 17;/,
  "empty row colSpan must count the 最近切换时间 column",
);

// 6. Verify CodeBuddy credit refresh records and passes refreshed_at
assert.match(
  codebuddyManagerSource,
  /recordAccountRefreshed\("codebuddy"/,
  "CodeBuddy credits refresh must record refresh time on success",
);
assert.match(
  accountManagerSource,
  /refreshed_at:\s*refreshedAt/,
  "CodeBuddy accounts mapping must include refreshed_at",
);

// 7. Verify AGY formatTime does not use getFullYear or toLocaleString with year
assert.doesNotMatch(
  agyManagerSource,
  /formatTime\s*=\s*\(seconds\)\s*=>\s*seconds\s*\?\s*new Date\(seconds \* 1000\)\.toLocaleString\(\)\s*:\s*"-"/,
  "AGY formatTime must not use full toLocaleString with year",
);

// 8. 最近切换时间列：表头、样式、渲染与各账号类型的数据映射
assert.match(
  indexHtml,
  /<th class="codex-account-refreshed-column">刷新时间<\/th>\s*<th class="codex-account-last-switch-column">最近切换时间<\/th>/,
  "统一账号表格必须在刷新时间之后追加最近切换时间列",
);
assert.match(
  indexHtml,
  /<th class="agy-col-saved">保存时间<\/th>\s*<th class="agy-col-last-switch">最近切换时间<\/th>/,
  "AGY 表格也必须追加最近切换时间列",
);
assert.match(
  stylesAuth,
  /\.codex-account-table\s+\.codex-account-last-switch-column/,
  "styles-auth.css 必须样式化最近切换时间表头",
);
assert.match(
  stylesAuth,
  /\.codex-account-table\s+\.codex-account-last-switch-cell/,
  "styles-auth.css 必须样式化最近切换时间单元格",
);
assert.match(
  accountManagerSource,
  /accountRefreshedCell\(account\),\s*lastSwitchCell\(account\),/,
  "renderAccounts 必须在刷新时间单元格之后追加最近切换时间单元格",
);
assert.match(
  accountManagerSource,
  /function lastSwitchCell\(account\)[\s\S]*?account\.last_switch_at \?\? account\.lastSwitchAt/,
  "最近切换时间单元格必须兼容 snake_case 与 camelCase 字段",
);
for (const pattern of [
  /last_switch_at: preset\.last_switch_at,/,
  /last_switch_at: meta\.lastSwitchAt \|\| 0,/,
  /last_switch_at: meta\.last_switch_at \|\| 0,/,
]) {
  assert.match(accountManagerSource, pattern, `账号映射必须透传 last_switch_at: ${pattern}`);
}
assert.match(
  agyManagerSource,
  /lastSwitchCellHtml/,
  "AGY renderRow 必须渲染最近切换时间单元格",
);
assert.match(
  agyManagerSource,
  /colspan="11"/,
  "AGY 空列表行必须覆盖使用列",
);

// 9. 「使用」统计窗口：共用下拉、窗口计数与各账号类型的数据透传
const usageBarIndex = indexHtml.indexOf('id="llm-usage-window-bar"');
const usageTableIndex = indexHtml.indexOf('class="table-wrap auth-table-wrap codex-account-desktop-wrap"');
assert.ok(
  usageBarIndex > 0 && usageTableIndex > usageBarIndex,
  "统计窗口下拉必须位于统一账号表格上方",
);
assert.doesNotMatch(
  indexHtml,
  /id="llm-usage-window-bar"[^>]*data-account-kinds/,
  "统计窗口下拉不得带 data-account-kinds，避免被子 tab 过滤隐藏",
);
assert.match(
  indexHtml,
  /app-codex-account-manager\.js\?v=20261111-codearts-table/,
  "app-codex-account-manager.js 缓存键必须更新",
);
assert.match(
  indexHtml,
  /app-agy-manager\.js\?v=20261112-gemini-quota/,
  "app-agy-manager.js 缓存键必须更新",
);
assert.match(
  indexHtml,
  /styles-auth\.css\?v=20261110-usage-window/,
  "styles-auth.css 缓存键必须更新",
);
assert.match(
  indexHtml,
  /id="llm-usage-window-bar"[^>]*>[\s\S]*?<select id="account-usage-window"[\s\S]*?<option value="48h" selected>最近 48 小时<\/option>/,
  "账号表格上方必须有默认 48 小时的共用统计窗口下拉",
);
assert.match(
  indexHtml,
  /<option value="all">全部（累计）<\/option>/,
  "下拉必须保留「全部」档以回看累计总数",
);
assert.match(
  indexHtml,
  /<th class="agy-col-usage">使用<\/th>/,
  "AGY 表格必须新增使用列",
);
assert.match(
  accountManagerSource,
  /const DEFAULT_ACCOUNT_USAGE_WINDOW = "48h";/,
  "默认窗口必须是 48 小时",
);
assert.match(
  accountManagerSource,
  /usageCountCell\(account\),/,
  "使用列必须由窗口计数单元格渲染",
);
assert.match(
  accountManagerSource,
  /usageWindowSelect\.addEventListener\("change"[\s\S]*?renderAccounts\(allAccounts\)/,
  "切换窗口必须按缓存重渲染而不重新请求",
);
for (const pattern of [
  /switch_history: preset\.switch_history \|\| \[\],/,
  /switch_history: meta\.switchHistory \|\| \[\],/,
  /switch_history: meta\.switch_history \|\| \[\],/,
]) {
  assert.match(accountManagerSource, pattern, `账号映射必须透传 switch_history: ${pattern}`);
}
assert.match(agyManagerSource, /usageCellHtml/, "AGY renderRow 必须渲染使用单元格");
assert.match(agyManagerSource, /renderUsageWindow/, "AGY 必须支持按同一窗口重渲染");

// 从真实源码提取窗口配置与计数函数做行为验证（不是副本）。
const usageWindowsBlock = accountManagerSource.match(/const ACCOUNT_USAGE_WINDOWS = \{[\s\S]*?\n  \};/)?.[0];
const usageCountFn = accountManagerSource.match(/  function windowedUsageCount\(account\) \{[\s\S]*?\n  \}/)?.[0];
assert.ok(usageWindowsBlock && usageCountFn, "必须能提取窗口配置与 windowedUsageCount 源码");
const usageHarness = new Function(`
  ${usageWindowsBlock}
  ${usageCountFn}
  let currentWindow = "48h";
  function accountUsageWindow() { return currentWindow; }
  return { windowedUsageCount, setWindow: (w) => { currentWindow = w; } };
`);
const { windowedUsageCount, setWindow } = usageHarness();
const usageNow = Date.now();
const account = {
  switch_count: 9,
  switch_history: [
    usageNow - 10 * 60 * 1000,        // 10 分钟前
    usageNow - 30 * 60 * 60 * 1000,   // 30 小时前
    usageNow - 5 * 24 * 3600 * 1000,  // 5 天前
    usageNow - 20 * 24 * 3600 * 1000, // 20 天前
  ],
  last_switch_at: usageNow - 10 * 60 * 1000,
};
setWindow("24h");
assert.equal(windowedUsageCount(account), 1, "24 小时窗口只统计最近一次切换");
setWindow("48h");
assert.equal(windowedUsageCount(account), 2, "48 小时窗口统计 10 分钟与 30 小时前两次");
setWindow("7d");
assert.equal(windowedUsageCount(account), 3, "7 天窗口统计三次");
setWindow("30d");
assert.equal(windowedUsageCount(account), 4, "30 天窗口统计全部历史");
setWindow("all");
assert.equal(windowedUsageCount(account), 9, "全部档返回累计 switch_count");
// 旧账号只有 last_switch_at：窗口内计 1，窗口外计 0，不与后端播种的历史重复计数。
setWindow("48h");
assert.equal(
  windowedUsageCount({ switch_count: 3, switch_history: [], last_switch_at: usageNow - 60 * 1000 }),
  1,
  "旧账号最近一次切换在窗口内应计 1",
);
assert.equal(
  windowedUsageCount({ switch_count: 3, switch_history: [], last_switch_at: usageNow - 3 * 24 * 3600 * 1000 }),
  0,
  "旧账号最近一次切换在窗口外应计 0",
);

console.log("All llm-account-refresh-time tests passed successfully!");
