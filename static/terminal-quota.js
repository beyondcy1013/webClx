// webClx terminal quota dialog and usage rendering helpers.
// Extracted from terminal.js as global declarations.
// Quota constants/cache live here; there is no top-level DOM setup.

// ---- 套餐用量查询 (quota) ----
// 平台枚举与后端 `src/quota.rs::QuotaPlatform` 对齐，按此在 filter 与
// render 阶段分派。当前覆盖 ZHIPU（bigmodel.cn / zcode.z.ai）、MINIMAX
// （minimaxi.com / minimax.io）和 OPENROUTER（openrouter.ai）。新增平台时需要同时更新这里的
// PRESET_URL_FILTERS 与 renderQuotaReport。
const QUOTA_PLATFORM_LABELS = {
  ZHIPU: "智谱 GLM Coding Plan",
  MINIMAX: "MiniMax TokenPlan",
  GROK: "Grok Build 余量",
  OPENROUTER: "OpenRouter Credits",
};
const PRESET_URL_FILTERS = [
  // 智谱：兼容旧的 bigmodel.cn、GLM 5.3 的 zcode.z.ai 模型入口，
  // 以及本地代理 /api/codex-proxy/zhipu/。
  (url) => /bigmodel\.cn|zcode\.z\.ai|\/api\/codex-proxy\/zhipu\//i.test(url),
  // MiniMax：国内 minimaxi.com 与国际 minimax.io 都参与；同时兼容走本地
  // codex-proxy minimax 通路的预设。
  (url) => /minimaxi\.com|minimax\.io|\/api\/codex-proxy\/minimax\//i.test(url),
  (url) => /grok\.com|x\.ai/i.test(url),
  (url) => /openrouter\.ai/i.test(url),
];
const quotaIsQuotaPreset = (preset) => {
  if (!preset || !preset.base_url) return false;
  if (!preset.api_key) return false;
  return PRESET_URL_FILTERS.some((fn) => fn(preset.base_url));
};
const quotaDetectPlatform = (baseUrl, fallback = "ZHIPU") => {
  const url = String(baseUrl || "").toLowerCase();
  if (/minimaxi\.com|minimax\.io|\/api\/codex-proxy\/minimax\//.test(url)) return "MINIMAX";
  if (/grok\.com|x\.ai/.test(url)) return "GROK";
  if (/openrouter\.ai/.test(url)) return "OPENROUTER";
  if (/bigmodel\.cn|zcode\.z\.ai|\/api\/codex-proxy\/zhipu\//.test(url)) return "ZHIPU";
  return fallback;
};
// 与 quotaDetectPlatform 相同的识别规则，但不做 ZHIPU 兜底：识别不出时
// 返回空串，供「指定账号」列表如实标注非识别平台的预设。
const quotaDetectPlatformStrict = (baseUrl) => quotaDetectPlatform(baseUrl, "");

// 快速切换下拉里「指定账号」条目的固定 value；预设条目用数字下标，
// 已保存配置留空，互不冲突。
const QUOTA_CUSTOM_OPTION_VALUE = "custom";

// option 的 label 与 value 都要过同一套转义，防注入。
const quotaEscapeOptionLabel = (label) =>
  String(label)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

// 智谱监测接口的 HTTP 仍是 200，但业务失败会返回 success=false。
// 把这类响应转换成异常，避免渲染出“0 次调用 / 0 token”的假结果。
function quotaUpstreamErrorMessage(payload) {
  if (!payload || typeof payload !== "object") return null;
  const code = Number(payload.code);
  const message = String(payload.msg || payload.message || "").trim();
  if (payload.success === false || (Number.isFinite(code) && code !== 0 && message)) {
    return message || `上游返回错误码 ${code}`;
  }
  return null;
}

const UNIT_LABELS = { 1: "sec", 2: "min", 3: "hour", 5: "month", 6: "week" };
const quotaUnitLabel = (unit, number) => {
  const u = UNIT_LABELS[unit] || `unit${unit}`;
  return number === 1 ? u : `${number} ${u}`;
};
const quotaFormatResetTime = (ts) => {
  if (!ts) return "-";
  const d = new Date(ts);
  const pad = (v) => String(v).padStart(2, "0");
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
};
const quotaFormatResetCountdown = (ts, now = Date.now()) => {
  const resetTime = Number(ts);
  if (!Number.isFinite(resetTime) || resetTime <= 0) return "-";
  const remainingMs = resetTime - Number(now);
  if (remainingMs <= 0) return "已到重置时间";
  const totalMinutes = Math.max(1, Math.ceil(remainingMs / 60_000));
  const days = Math.floor(totalMinutes / 1_440);
  const hours = Math.floor((totalMinutes % 1_440) / 60);
  const minutes = totalMinutes % 60;
  const parts = [];
  if (days) parts.push(`${days}天`);
  if (days || hours) parts.push(`${hours}小时`);
  parts.push(`${minutes}分钟`);
  return parts.join("");
};

function quotaMdTable(headers, rows) {
  const esc = (s) => String(s ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");
  const lines = [];
  lines.push(`| ${headers.map(esc).join(" | ")} |`);
  lines.push(`| ${headers.map(() => "---").join(" | ")} |`);
  rows.forEach((r) => {
    lines.push(`| ${r.map(esc).join(" | ")} |`);
  });
  return lines.join("\n");
}

function quotaFmtNum(n) {
  return Number(n ?? 0).toLocaleString("en-US");
}

const quotaEsc = (s) => String(s ?? "")
  .replace(/&/g, "&amp;")
  .replace(/</g, "&lt;")
  .replace(/>/g, "&gt;")
  .replace(/"/g, "&quot;")
  .replace(/'/g, "&#39;");

// Build a real HTML <table> so it renders (not markdown pipe text).
function quotaHtmlTable(headers, rows, cls = "") {
  const th = headers.map((h) => `<th>${quotaEsc(h)}</th>`).join("");
  const trs = rows.map((r) => `<tr>${r.map((c) => `<td>${c}</td>`).join("")}</tr>`).join("");
  return `<table class="${cls}"><thead><tr>${th}</tr></thead><tbody>${trs}</tbody></table>`;
}

// Text-style progress bar for a quota limit, similar to OpenAI's usage window.
// pct is a number 0-100 (clamped), always interpreted as 已用百分比
// (consumed share): filled cells grow from the left, empty track on the
// right represents 剩余百分比. Thresholds follow standard "used" semantics:
// high pct = bad (red), mid = amber, otherwise default accent.
// Returns an HTML <span> with fill + track spans.
function quotaBar(pct) {
  const v = Math.max(0, Math.min(100, Number(pct) || 0));
  const cells = 10; // 10 segments x 10% = 100%
  const filled = Math.round((v / 100) * cells);
  // fill / track 用同一字符 █，仅靠 CSS 颜色区分；避免混合字符
  // (█ + ▱) 在某些等宽字体下高度不一致、整行错位。
  const fill = "█".repeat(filled);
  const track = "█".repeat(cells - filled);
  let level = "low";
  if (v >= 90) level = "high";
  else if (v >= 70) level = "mid";
  return `<span class="quota-bar quota-bar-${level}" title="已用 ${v.toFixed(0)}%">` +
    `<span class="quota-bar-fill">${fill}</span><span class="quota-bar-track">${track}</span>` +
    `</span>`;
}

function renderQuotaReport(data) {
  if (!terminalQuotaBodyEl) return;
  if (!data) {
    terminalQuotaBodyEl.innerHTML = `<p class="meta-text">查询失败，请检查 API Key 设置。</p>`;
    return;
  }
  const platform = data.platform || "ZHIPU";

  // 后端目前把智谱三路监测响应透传回来。任何一路携带业务错误时都说明
  // 这不是可渲染的用量数据；优先给出明确错误，而不是把空对象汇总成 0。
  const upstreamError = [data.model_usage, data.tool_usage, data.quota_limit]
    .map(quotaUpstreamErrorMessage)
    .find(Boolean);
  if (upstreamError) {
    terminalQuotaBodyEl.innerHTML =
      `<h3>上游查询失败</h3><p class="meta-text">${quotaEsc(upstreamError)}</p>` +
      `<p class="meta-text">请在套餐下拉切换到可用账号，或打开「设置」更新当前 API Key / Base URL。</p>`;
    return;
  }

  let html = "";
  const parts = [];

  if (platform === "MINIMAX") {
    // MiniMax TokenPlan remains 暂无统一字段约定，先把整段 JSON 渲染成
    // key/value 表，并保留对常见字段（remains / currentValue /
    // remaining / window / plan）的友好提示。
    const remains = data.remains ?? data;
    html += renderMinimaxRemains(remains);
    terminalQuotaBodyEl.innerHTML = html;
    return;
  }

  if (platform === "OPENROUTER") {
    html += renderOpenRouterCredits(data.credits);
    if (data.free_usage) {
      html += renderOpenRouterFreeUsage(data.free_usage);
    }
    terminalQuotaBodyEl.innerHTML = html;
    return;
  }

  if (platform === "GROK") {
    html += renderGrokQuota(data.billing, data.subscription);
    terminalQuotaBodyEl.innerHTML = html;
    return;
  }

  const win = data.window || {};
  const winText = (win.start && win.end) ? `${quotaEsc(quotaFormatResetTime(win.start))} ~ ${quotaEsc(quotaFormatResetTime(win.end))}` : "";
  // Quota limits
  const ql = data.quota_limit;
  if (ql) {
    // 排序：Token 限额排在 MCP 之前（用户预期 Token 在上、MCP 在下）。
    // 当 API 返回顺序不同时保持稳定的展示顺序。
    const limitOrder = { TOKENS_LIMIT: 0, TIME_LIMIT: 1 };
    const limits = (ql.limits || []).slice().sort((a, b) =>
      (limitOrder[a.type] ?? 2) - (limitOrder[b.type] ?? 2));
    const rows = limits.map((l) => {
      const percentage = Number(l.percentage);
      const safePercentage = Number.isFinite(percentage) ? percentage : 0;
      const pct = `${safePercentage}%`;
      let windowLabel, detail;
      if (l.type === "TOKENS_LIMIT") {
        windowLabel = quotaEsc(`Token (${quotaUnitLabel(l.unit, l.number)})`);
        detail = `重置: ${quotaEsc(quotaFormatResetTime(l.nextResetTime))}; 还差: ${quotaEsc(quotaFormatResetCountdown(l.nextResetTime))}`;
        if (winText) detail += `; ${winText}`;
      } else if (l.type === "TIME_LIMIT") {
        windowLabel = quotaEsc(`MCP (${quotaUnitLabel(l.unit, l.number)})`);
        detail = `重置: ${quotaEsc(quotaFormatResetTime(l.nextResetTime))}; 还差: ${quotaEsc(quotaFormatResetCountdown(l.nextResetTime))}; ${quotaFmtNum(l.currentValue)} / ${quotaFmtNum(l.usage)} (剩余 ${quotaFmtNum(l.remaining)})`;
      } else {
        windowLabel = quotaEsc(l.type || "-");
        detail = "-";
      }
      const bar = quotaBar(safePercentage);
      // 与 MiniMax 行（API 返回 remaining_percent）现在也用"已用 X%"
      // 对齐，前缀保持显式标注。
      return [windowLabel, `${bar} 已用 ${pct}`, detail];
    });
    if (rows.length) {
      parts.push(`<h3>配额与限额 (Quota & limits)</h3><div class="quota-table-scroll">${quotaHtmlTable(["限额类型", "已用", "明细"], rows, "quota-limits-table")}</div>`);
    }
    if (ql.level) parts.push(`<p>套餐等级: ${quotaEsc(ql.level)}</p>`);
  }

  // Model usage
  const md = data.model_usage;
  if (md) {
    const totals = md.totalUsage || {};
    const list = (md.modelSummaryList || []).slice().sort((a, b) => (a.sortOrder || 0) - (b.sortOrder || 0));
    const sum = list.reduce((s, m) => s + (m.totalTokens || 0), 0);
    if (list.length) {
      const rows = list.map((m) => [
        quotaEsc(m.modelName),
        quotaFmtNum(m.totalTokens),
        sum > 0 ? ((m.totalTokens / sum) * 100).toFixed(1) + "%" : "-",
      ]);
      rows.push([`<strong>SUM</strong>`, `<strong>${quotaFmtNum(sum)}</strong>`, "<strong>100.0%</strong>"]);
      parts.push(`<h3>模型用量 (Model usage)</h3>${quotaHtmlTable(["Model", "Total Tokens", "Share"], rows)}`);
    }
    parts.push(`<p>窗口调用次数: ${quotaFmtNum(totals.totalModelCallCount)} · 窗口 Token 总量: ${quotaFmtNum(totals.totalTokensUsage)}</p>`);
  }

  // Tool usage
  const td = data.tool_usage;
  if (td) {
    const totals = td.totalUsage || {};
    const list = (td.toolSummaryList || []).slice().sort((a, b) => (a.sortOrder || 0) - (b.sortOrder || 0));
    if (list.length) {
      const rows = list.map((t) => [quotaEsc(t.toolCode), quotaEsc(t.toolName || t.toolNameI18n || "-"), quotaFmtNum(t.totalUsageCount)]);
      parts.push(`<h3>工具用量 (Tool usage / MCP)</h3>${quotaHtmlTable(["Tool Code", "Tool Name", "Count"], rows)}`);
    }
    parts.push(`<p>MCP 总调用: ${quotaFmtNum(totals.totalSearchMcpCount)}</p>`);
  }


  html += parts.join("");
  terminalQuotaBodyEl.innerHTML = html;
}

// MiniMax TokenPlan 状态码：`status` 是该模型对当前账号的可用性，
// 而不是剩余配额百分比（剩余由同名 `_remaining_percent` 字段表达）。
// 语义推断自 2026-07-09 真实响应（general 94%/status=1、
// video 100%/status=3）：3 = 模型未对该账号开通/启用。`2=预警`、
// `0/4+` 等其它取值尚未在真实样本里观察到，仍走 default 兜底。
const MINIMAX_STATUS_TEXT = {
  1: "已开通",
  2: "预警",
  3: "未开通",
};
const minimaxStatusLabel = (status) => {
  const key = Number(status);
  return MINIMAX_STATUS_TEXT[key] ? `${MINIMAX_STATUS_TEXT[key]}(${key})` : `未知(${quotaEsc(status ?? "-")})`;
};

// 把 ms 时间戳格式化为「MM-DD HH:mm」，与 ZHIPU 的 `quotaFormatResetTime`
// 风格一致；解析失败回退到「-」。
const minimaxFormatTime = (ms) => {
  if (ms === null || ms === undefined) return "-";
  const d = new Date(Number(ms));
  if (Number.isNaN(d.valueOf())) return "-";
  const pad = (v) => String(v).padStart(2, "0");
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
};

// 把「距离重置剩余毫秒数」格式化为「Xd Yh Zm」倒计时。
// 注意：MiniMax 返回的 remains_time / weekly_remains_time 单位是毫秒，
// 与同对象里的 start_time/end_time 一致（而非秒）。早期版本误把毫秒当
// 秒处理，导致天数被放大约 1000 倍（193d / 4984d 这种离谱值）。
const minimaxFormatCountdown = (ms) => {
  if (ms === null || ms === undefined) return "-";
  // ms → sec，先用 floor 截断到整秒再做拆分。
  const total = Math.max(0, Math.floor(Number(ms) || 0) / 1000);
  if (!total) return "已重置";
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const mins = Math.floor((total % 3600) / 60);
  const parts = [];
  if (days) parts.push(`${days}d`);
  if (days || hours) parts.push(`${hours}h`);
  parts.push(`${mins}m`);
  return parts.join(" ");
};

// 构建一行迷你配额卡：进度条 + 百分比 + 状态码 + 重置时间 + 倒计时。
// 用于 5 小时 / 周 两套限额。`remainsMs` 是 MiniMax 返回的剩余毫秒数
// （remains_time / weekly_remains_time），由 minimaxFormatCountdown 换算。
function renderMinimaxQuotaRow(scope, pct, status, endTs, remainsMs) {
  // MiniMax API 返回 *_remaining_percent；进度条统一用"已用百分比"语义
  // （填充在左代表已用，未填充在右代表剩余），所以这里把剩余反相成已用。
  const remain = Math.max(0, Math.min(100, Number(pct ?? 0)));
  const used = 100 - remain;
  const statusHtml = `<small>${quotaEsc(minimaxStatusLabel(status))}</small>`;
  return `<div class="quota-minimax-cell">` +
    `<div class="quota-minimax-head"><strong>${quotaEsc(scope)}</strong> ` +
    `${quotaBar(used)} <span class="quota-minimax-pct">已用 ${used.toFixed(0)}%（剩 ${remain.toFixed(0)}%）</span></div>` +
    `<div class="quota-minimax-meta">状态 ${statusHtml} · 重置 ${quotaEsc(minimaxFormatTime(endTs))}` +
    ` · 倒计时 <strong>${quotaEsc(minimaxFormatCountdown(remainsMs))}</strong></div>` +
    `</div>`;
}

// Render MiniMax TokenPlan `/v1/token_plan/remains` 的响应。
// 实际接口字段约定：
//   base_resp: { status_code, status_msg }       0=成功，其它=错误
//   model_remains: [                             每个模型两套限额
//     { model_name, current_interval_*, end_time, remains_time,
//       current_weekly_*, weekly_start_time, weekly_end_time,
//       weekly_remains_time, weekly_boost_permille, ... }
//   ]
// 把每个模型拆成两行（5 小时 + 周），让用户一眼看清两套配额与重置倒计时；
// 若响应格式有变（缺 model_remains 等），回退到键值表兜底。
function renderMinimaxRemains(remains) {
  if (!remains || typeof remains !== "object") {
    return `<p class="meta-text">MiniMax 返回为空。</p>`;
  }

  let html = "";

  const list = Array.isArray(remains.model_remains) ? remains.model_remains : null;
  if (!list || list.length === 0) {
    // 兼容未来字段调整：回落整段键值表，避免漏掉新字段。
    html += `<h3>MiniMax TokenPlan 余额明细</h3>` +
      `<div class="quota-table-scroll">${renderQuotaKeyValueTable(remains)}</div>`;
    return html;
  }

  // 按 model_name 排序，相同模型相邻，方便用户看自己关注的模型。
  const sorted = list.slice().sort((a, b) => String(a.model_name || "").localeCompare(String(b.model_name || "")));
  const rows = [];
  sorted.forEach((m) => {
    const name = m.model_name || "未知模型";
    const boost = m.weekly_boost_permille !== undefined && m.weekly_boost_permille !== null
      ? `${(Number(m.weekly_boost_permille) / 10).toFixed(0)}%`
      : null;
    const intervalCell = renderMinimaxQuotaRow(
      "5 小时",
      m.current_interval_remaining_percent,
      m.current_interval_status,
      m.end_time,
      m.remains_time,
    );
    const weeklyCell = renderMinimaxQuotaRow(
      "周",
      m.current_weekly_remaining_percent,
      m.current_weekly_status,
      m.weekly_end_time,
      m.weekly_remains_time,
    );
    const notes = boost
      ? `<small>周配额 Boost ${quotaEsc(boost)}</small>`
      : `<small>-</small>`;
    rows.push([
      `<strong>${quotaEsc(name)}</strong>`,
      intervalCell,
      `<small>${quotaEsc(formatMinimaxMsShort(m.start_time))} ~ ${quotaEsc(formatMinimaxMsShort(m.end_time))}</small>`,
    ]);
    rows.push([
      "",
      weeklyCell,
      notes +
        `<br><small>${quotaEsc(formatMinimaxMsShort(m.weekly_start_time))} ~ ${quotaEsc(formatMinimaxMsShort(m.weekly_end_time))}</small>`,
    ]);
  });

  html += `<h3>MiniMax TokenPlan 余额明细</h3>`;
  html += `<div class="quota-table-scroll">${quotaHtmlTable(
    ["模型", "限额（进度条 / 状态 / 重置）", "窗口 / 备注"],
    rows,
    "quota-minimax-table",
  )}</div>`;
  scheduleMinimaxResetRefresh(list);
  return html;
}

// Render OpenRouter GET /api/v1/key. A key can be unlimited (`limit` is null),
// so remaining amount comes from the official payload instead of being derived.
function renderOpenRouterCredits(payload) {
  if (!payload || typeof payload !== "object") {
    return `<p class="meta-text">OpenRouter 返回为空。</p>`;
  }

  let data = typeof payload.data === "object" && payload.data !== null ? payload.data : payload;
  const limit = data.limit === null || data.limit === undefined ? null : Number(data.limit);
  const remaining = data.limit_remaining === null || data.limit_remaining === undefined
    ? null
    : Number(data.limit_remaining);
  const usage = Number(data.usage);
  const hasLimit = Number.isFinite(limit);
  const hasRemaining = Number.isFinite(remaining);
  const hasUsage = Number.isFinite(usage);
  const usedPercent = hasLimit && hasUsage && limit > 0
    ? Math.min(100, Math.max(0, (usage / limit) * 100))
    : 0;
  const formatUsd = (amount) => `$${amount.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;

  const rows = [
    ["额度上限", hasLimit ? formatUsd(limit) : "无限制"],
    ["已用", hasUsage ? formatUsd(usage) : "-"],
    ["剩余", hasRemaining ? `<strong>${formatUsd(remaining)}</strong>` : (hasLimit ? "-" : "无限制")],
    ["使用进度", `${quotaBar(usedPercent)} 已用 ${usedPercent.toFixed(1)}%`],
  ];

  Object.entries(data).forEach(([key, value]) => {
    if (/^limit$|^limit_remaining$|^usage$/.test(key)) return;
    let display = "-";
    if (typeof value === "number") display = quotaFmtNum(value);
    else if (typeof value === "string") display = quotaEsc(value);
    else display = `<code>${quotaEsc(JSON.stringify(value))}</code>`;
    rows.push([quotaEsc(key), display]);
  });

  return `<h3>OpenRouter 当前 Key 额度</h3><div class="quota-table-scroll">${quotaHtmlTable(
    ["项目", "值"], rows,
  )}</div>`;
}

function renderOpenRouterFreeUsage(usage) {
  const used = Math.max(0, Number(usage.used ?? 0));
  const limit = Number(usage.limit);
  if (!Number.isFinite(limit) || limit <= 0) return "";
  const remaining = Number.isFinite(Number(usage.remaining))
    ? Math.max(0, Number(usage.remaining))
    : Math.max(0, limit - used);
  const percent = Math.min(100, (used / limit) * 100);
  const day = usage.day || "-";
  return `<h3>OpenRouter 免费模型次数（本地代理）</h3>` +
    `<div class="quota-table-scroll">${quotaHtmlTable(["项目", "值"], [
      ["日期", quotaEsc(day)],
      ["已用", `<strong>${quotaFmtNum(used)}</strong> 次`],
      ["剩余", `${quotaFmtNum(remaining)} / ${quotaFmtNum(limit)} 次`],
      ["使用进度", `${quotaBar(percent)} 已用 ${percent.toFixed(1)}%`],
    ])}</div><p class="meta-text">${quotaEsc(
      usage.note ||
        "统计通过 webClx 本地代理发出的成功 :free 模型请求；其他客户端流量不会计入。",
    )}</p>`;
}

// Render Grok Build `/billing?format=credits` and `/user?include=subscription`.
// Backend currently returns no `creditUsagePercent`; show period, credits, and
// subscription, and surface raw billing fields so future fields stay visible.
function renderGrokQuota(billingEnvelope, subscription) {
  if (!billingEnvelope || typeof billingEnvelope !== "object") {
    return `<p class="meta-text">Grok 返回为空。</p>`;
  }
  const config = billingEnvelope.config ?? {};
  const period = config.currentPeriod || {};
  const cents = (value) => {
    const n = Number(value?.val ?? value);
    return Number.isFinite(n) ? n : null;
  };
  const formatUsd = (amount) =>
    amount === null ? "-" : `$${(Math.abs(amount) / 100).toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    })}`;
  const prepaid = cents(config.prepaidBalance);
  const rows = [
    ["订阅", quotaEsc(subscription?.subscriptionTier || "-")],
    ["账户", quotaEsc(subscription?.email || "-")],
    ["计费模式", config.isUnifiedBillingUser === true ? "统一用量池" : "-"],
    ["窗口", `${quotaEsc(quotaFormatResetTime(period.start))} ~ ${quotaEsc(quotaFormatResetTime(period.end))}`],
    ["重置", quotaEsc(quotaFormatResetTime(period.end))],
    ["倒计时", quotaEsc(quotaFormatResetCountdown(Date.parse(period.end)))],
    ["预付 Credits", formatUsd(prepaid)],
    ["On-demand 已用", formatUsd(cents(config.onDemandUsed))],
    ["On-demand 上限", formatUsd(cents(config.onDemandCap))],
  ];
  const extra = Object.entries(config)
    .filter(([key]) => !/^(currentPeriod|onDemandCap|onDemandUsed|prepaidBalance|isUnifiedBillingUser|billingPeriodStart|billingPeriodEnd)$/.test(key))
    .map(([key, value]) => [quotaEsc(key), `<code>${quotaEsc(JSON.stringify(value))}</code>`]);
  return `<h3>Grok Build 用量与 Credits</h3><div class="quota-table-scroll">${quotaHtmlTable(
    ["项目", "值"], [...rows, ...extra],
  )}</div><p class="meta-text">官方接口本次未返回 creditUsagePercent，暂无法计算套餐余量百分比。</p>`;
}

// MiniMax 的 5 小时 / 周 窗口到点会重新计数。`end_time` /
// `weekly_end_time` 都是 ms 时间戳。在每个 reset 时刻起一个 setTimeout，
// 让前端到点自动重查一次，避免"到了时间没复位"。setTimeout 上限是
// 32 位有符号整数（约 24.8 天），周窗口 7d 在范围内；若未来出现更大窗口
// 超过上限则降级为到点时立即重查（delay 已是 0 也无害）。
const minimaxResetTimers = new Set();
function scheduleMinimaxResetRefresh(list) {
  cancelMinimaxResetRefresh();
  if (!Array.isArray(list)) return;
  const MAX_TIMEOUT = 2_147_483_647;
  list.forEach((m) => {
    [m && m.end_time, m && m.weekly_end_time].forEach((ts) => {
      const ms = Number(ts);
      if (!Number.isFinite(ms) || ms <= 0) return;
      const delay = Math.max(0, ms - Date.now());
      // 跨越已过去 ⇒ 立即重查一次；超过上限就 clamp 到上限，到点再查一次。
      const safeDelay = Math.min(delay, MAX_TIMEOUT);
      const id = window.setTimeout(() => {
        minimaxResetTimers.delete(id);
        if (terminalQuotaDialogEl && terminalQuotaDialogEl.open) {
          refreshTerminalQuota(selectedQuotaOverrideParams());
        }
      }, safeDelay);
      minimaxResetTimers.add(id);
    });
  });
}
function cancelMinimaxResetRefresh() {
  minimaxResetTimers.forEach((id) => window.clearTimeout(id));
  minimaxResetTimers.clear();
}

// 同 ZHIPU 的 `quotaFormatResetTime`，但接受 ms；解析失败返回 "-"。
function formatMinimaxMsShort(ms) {
  return minimaxFormatTime(ms);
}

// 当 MiniMax 响应不符合预期（缺 model_remains）时，回落用 key/value 表
// 兜底，避免直接丢弃数据。敏感字段自动隐藏。
function renderQuotaKeyValueTable(obj) {
  const rows = Object.entries(obj).map(([k, v]) => {
    let display;
    if (v === null || v === undefined) {
      display = "-";
    } else if (typeof v === "number") {
      display = quotaFmtNum(v);
    } else if (typeof v === "string") {
      display = /key|token|secret/i.test(k) ? "<em>已隐藏</em>" : quotaEsc(v);
    } else {
      display = `<code>${quotaEsc(JSON.stringify(v))}</code>`;
    }
    return [quotaEsc(k), display];
  });
  return quotaHtmlTable(["字段", "值"], rows);
}

async function refreshTerminalQuota({ apiKey = "", baseUrl = "" } = {}) {
  if (!terminalQuotaBodyEl) return;
  terminalQuotaBodyEl.innerHTML = `<p class="meta-text">查询中…</p>`;
  try {
    const qs = new URLSearchParams();
    if (apiKey) qs.set("api_key", apiKey);
    if (baseUrl) qs.set("base_url", baseUrl);
    let data;
    if (quotaDetectPlatform(baseUrl) === "OPENROUTER") {
      try {
        const response = await fetch("https://openrouter.ai/api/v1/key", {
          headers: { Authorization: `Bearer ${apiKey}` },
        });
        if (!response.ok) throw new Error(`OpenRouter key HTTP ${response.status}`);
        const payload = await response.json();
        // GET /key returns the raw `{data}` envelope; the renderer consumes the
        // same normalized shape as the backend proxy.
        data = { ...payload, credits: payload.credits ?? payload };
        data.platform = "OPENROUTER";
        data.base_url = baseUrl || "https://openrouter.ai/api/v1";
      } catch (directError) {
        // Keep the backend proxy path for deployments that block direct browser calls.
        const url = "/api/quota/query" + (qs.toString() ? "?" + qs.toString() : "");
        data = await requestJson(url);
      }
    } else {
      const url = "/api/quota/query" + (qs.toString() ? "?" + qs.toString() : "");
      data = await requestJson(url);
    }
    renderQuotaReport(data);
    if (terminalQuotaKeyStatusEl) {
      if (apiKey) {
        const masked = String(apiKey).slice(0, 4) + "***" + String(apiKey).slice(-4);
        terminalQuotaKeyStatusEl.textContent = "已用: " + masked;
      } else {
        terminalQuotaKeyStatusEl.textContent = "已用: 已保存配置";
      }
    }
  } catch (error) {
    terminalQuotaBodyEl.innerHTML = `<p class="meta-text">查询失败: ${quotaEsc(error.message || error)}</p>`;
  }
}

async function loadQuotaConfigIntoInputs() {
  try {
    const cfg = await requestJson("/api/quota/config");
    quotaConfigCache = cfg;
    if (terminalQuotaApiKeyInputEl) terminalQuotaApiKeyInputEl.value = cfg.api_key || "";
    if (terminalQuotaBaseUrlInputEl) terminalQuotaBaseUrlInputEl.value = cfg.base_url || "";
    // 缺省回退到 SAVED，老配置（没有该字段）保持旧行为。
    if (terminalQuotaDefaultProviderEl) {
      const value = cfg.default_provider || "SAVED";
      const hasOption = Array.from(terminalQuotaDefaultProviderEl.options || []).some(
        (o) => o.value === value,
      );
      terminalQuotaDefaultProviderEl.value = hasOption ? value : "SAVED";
    }
    // 「指定账号」下拉由 loadQuotaCustomPresetOptions 先填充好选项，这里
    // 只回填当前选择；id 已失效（预设被删）时回到「未指定」。
    if (terminalQuotaCustomPresetSelectEl) {
      const customId = String(cfg.custom_preset_id || "").trim();
      const hasCustomOption = customId && Array.from(terminalQuotaCustomPresetSelectEl.options || []).some(
        (o) => o.value === customId,
      );
      terminalQuotaCustomPresetSelectEl.value = hasCustomOption ? customId : "";
    }
    if (terminalQuotaConfigStatusEl) terminalQuotaConfigStatusEl.textContent = cfg.api_key_masked ? `当前 Key: ${cfg.api_key_masked}` : "";
  } catch (error) {
    if (terminalQuotaConfigStatusEl) terminalQuotaConfigStatusEl.textContent = `读取配置失败: ${error.message || error}`;
  }
}

let quotaApiPresetsCache = null;
// 「指定账号」不受平台域名过滤，需要保留完整预设列表用于按 id 解析。
let quotaAllApiPresetsCache = null;
// 最近一次读取的 /api/quota/config；selectedQuotaOverrideParams 用它的
// custom_preset_id 解析「指定账号」条目对应的 api_key/base_url。
let quotaConfigCache = null;
async function fetchQuotaApiPresets() {
  try {
    const resp = await requestJson("/api/auth/api-presets");
    quotaAllApiPresetsCache = resp.presets || [];
    quotaApiPresetsCache = quotaAllApiPresetsCache.filter(quotaIsQuotaPreset);
    return quotaApiPresetsCache;
  } catch (error) {
    quotaApiPresetsCache = null;
    quotaAllApiPresetsCache = null;
    return null;
  }
}

// 按 id 在完整预设列表里解析「指定账号」；未指定或预设已被删除时返回 null。
function resolveQuotaCustomPreset(customPresetId) {
  const id = String(customPresetId || "").trim();
  if (!id || !Array.isArray(quotaAllApiPresetsCache)) return null;
  return quotaAllApiPresetsCache.find((p) => p && p.id === id) || null;
}

async function loadQuotaApiPresetsIntoDropdown() {
  if (!terminalQuotaPresetSelectEl) return;
  const presets = await fetchQuotaApiPresets();
  if (!presets) {
    terminalQuotaPresetSelectEl.innerHTML = '<option value="">读取预设失败</option>';
    return;
  }
  terminalQuotaPresetSelectEl.innerHTML =
    '<option value="">-- 选择预设以填充 (' + presets.length + ' 个) --</option>' +
    presets
      .map((p, i) => {
        const platform = quotaDetectPlatform(p.base_url);
        const platformLabel = QUOTA_PLATFORM_LABELS[platform] || platform;
        const masked = p.masked_api_key || String(p.api_key).slice(0, 8) + "***";
        const label = `[${platformLabel}] ${p.name}  |  ${masked}  |  ${p.base_url}`;
        return '<option value="' + i + '">' +
          label.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;") +
          "</option>";
      })
      .join("");
}

async function loadQuotaKeyDropdown() {
  return populateQuotaKeyDropdown(
    await fetchQuotaApiPresets(),
    "SAVED",
    quotaConfigCache && quotaConfigCache.custom_preset_id,
  );
}

// 「指定账号」下拉列出全部 Codex_API 预设（不过滤平台类型）。识别不出
// 平台的预设如实标注「其他」，查询仍按后端默认算法（智谱监测接口）尝试。
async function loadQuotaCustomPresetOptions() {
  if (!terminalQuotaCustomPresetSelectEl) return;
  const presets = await fetchQuotaApiPresets();
  if (!presets) {
    terminalQuotaCustomPresetSelectEl.innerHTML = '<option value="">读取预设失败</option>';
    return;
  }
  const all = Array.isArray(quotaAllApiPresetsCache) ? quotaAllApiPresetsCache : [];
  terminalQuotaCustomPresetSelectEl.innerHTML =
    '<option value="">未指定</option>' +
    all
      .map((p) => {
        const masked = p.masked_api_key || String(p.api_key || "").slice(0, 8) + "***";
        const platform = quotaDetectPlatformStrict(p.base_url);
        const platformLabel = platform
          ? (QUOTA_PLATFORM_LABELS[platform] || platform)
          : "其他";
        const label = `[${platformLabel}] ${p.name || "未命名"}  |  ${masked}  |  ${p.base_url || "-"}`;
        return '<option value="' + quotaEscapeOptionLabel(p.id || "") + '">' +
          quotaEscapeOptionLabel(label) + "</option>";
      })
      .join("");
}

// 组装快速切换下拉：第一个条目是「已保存配置」——即设置面板保存的（内置
// 默认为 GLM/智谱）key 配置，label 直接标注平台与掩码 key，让这份不走
// Codex_API 预设的 GLM key 在账号列表里实名可见；其后是「指定账号」条目
// （已配置且能解析到预设时）与按平台自动过滤出的预设。defaultProvider
// 决定默认选中：ZHIPU / MINIMAX / OPENROUTER 选中对应平台的第一个预设，
// 没有匹配预设但已保存配置就是该平台时选中「已保存配置」；CUSTOM 选中
// 「指定账号」；缺省或找不到匹配项时回到「已保存配置」（空值）。
function populateQuotaKeyDropdown(presets, defaultProvider, customPresetId) {
  if (!terminalQuotaKeySelectEl) return;
  const list = Array.isArray(presets) ? presets : [];
  const savedPlatform = quotaConfigCache && quotaConfigCache.base_url
    ? quotaDetectPlatform(quotaConfigCache.base_url)
    : "";
  const savedPlatformLabel = savedPlatform
    ? (QUOTA_PLATFORM_LABELS[savedPlatform] || savedPlatform)
    : "";
  const savedMasked = quotaConfigCache && quotaConfigCache.api_key_masked
    ? quotaConfigCache.api_key_masked
    : "";
  const savedLabelParts = ["已保存配置"];
  if (savedPlatformLabel) savedLabelParts.push(`[${savedPlatformLabel}]`);
  if (savedMasked) savedLabelParts.push(savedMasked);
  const savedOption =
    '<option value="">' + quotaEscapeOptionLabel(savedLabelParts.join("  ")) + "</option>";
  const customPreset = resolveQuotaCustomPreset(customPresetId);
  if (list.length === 0 && !customPreset) {
    // 没有任何预设也要保留已保存配置条目，遗留 GLM key 配置仍可直接查询。
    terminalQuotaKeySelectEl.innerHTML = savedOption;
    terminalQuotaKeySelectEl.value = "";
    return;
  }
  let customOption = "";
  if (customPreset) {
    const masked = customPreset.masked_api_key || String(customPreset.api_key || "").slice(0, 8) + "***";
    const label = `指定账号: ${customPreset.name || "未命名"}  |  ${masked}  |  ${customPreset.base_url || "-"}`;
    customOption =
      '<option value="' + QUOTA_CUSTOM_OPTION_VALUE + '">' +
      quotaEscapeOptionLabel(label) + "</option>";
  }
  // value 形如 "<idx>"；"custom" 表示指定账号；留空表示用已保存配置查询。
  let selectValue = "";
  if (defaultProvider === "CUSTOM" && customPreset) {
    selectValue = QUOTA_CUSTOM_OPTION_VALUE;
  } else if (defaultProvider === "ZHIPU" || defaultProvider === "MINIMAX" || defaultProvider === "OPENROUTER") {
    const idx = list.findIndex((p) => quotaDetectPlatform(p.base_url) === defaultProvider);
    if (idx >= 0) selectValue = String(idx);
    else if (savedPlatform === defaultProvider) selectValue = "";
  }
  terminalQuotaKeySelectEl.innerHTML =
    savedOption +
    customOption +
    list
      .map((p, i) => {
        const masked = p.masked_api_key || String(p.api_key).slice(0, 8) + "***";
        const platform = quotaDetectPlatform(p.base_url);
        const platformLabel = QUOTA_PLATFORM_LABELS[platform] || platform;
        const label = `[${platformLabel}] ${(p.name || "未命名")}  |  ${masked}`;
        return '<option value="' + i + '">' + quotaEscapeOptionLabel(label) + "</option>";
      })
      .join("");
  terminalQuotaKeySelectEl.value = selectValue;
}

function selectedQuotaOverrideParams() {
  if (!terminalQuotaKeySelectEl || !quotaApiPresetsCache) return {};
  const raw = terminalQuotaKeySelectEl.value;
  if (raw === "") return {};
  // 「指定账号」条目按配置里记录的预设 id 从完整列表解析，不受平台过滤。
  if (raw === QUOTA_CUSTOM_OPTION_VALUE) {
    const customPreset = resolveQuotaCustomPreset(quotaConfigCache && quotaConfigCache.custom_preset_id);
    if (!customPreset) return {};
    return { apiKey: customPreset.api_key || "", baseUrl: customPreset.base_url || "" };
  }
  const idx = Number(raw);
  if (Number.isNaN(idx) || idx < 0 || idx >= quotaApiPresetsCache.length) return {};
  const preset = quotaApiPresetsCache[idx];
  return { apiKey: preset.api_key || "", baseUrl: preset.base_url || "" };
}

async function refreshQuotaBySelectedKey() {
  await refreshTerminalQuota(selectedQuotaOverrideParams());
}

function onQuotaPresetSelectChange() {
  if (!terminalQuotaPresetSelectEl || !quotaApiPresetsCache) return;
  const idx = Number(terminalQuotaPresetSelectEl.value);
  if (Number.isNaN(idx) || idx < 0) return;
  const preset = quotaApiPresetsCache[idx];
  if (!preset) return;
  if (terminalQuotaApiKeyInputEl) terminalQuotaApiKeyInputEl.value = preset.api_key || "";
  if (terminalQuotaBaseUrlInputEl) terminalQuotaBaseUrlInputEl.value = preset.base_url || "";
}

function openTerminalQuotaDialog() {
  if (!terminalQuotaDialogEl) return;
  cancelMinimaxResetRefresh();
  if (terminalQuotaSettingsPanelEl) terminalQuotaSettingsPanelEl.hidden = true;
  resetTerminalImeFocusContext();
  if (terminalQuotaBodyEl) terminalQuotaBodyEl.innerHTML = `<p class="meta-text">查询中…</p>`;
  if (typeof terminalQuotaDialogEl.showModal === "function") {
    terminalQuotaDialogEl.showModal();
  } else {
    terminalQuotaDialogEl.hidden = false;
  }
  // 关键：先拿配置 + 预设，按「默认平台」选好下拉项后，再用选中项的
  // api_key/base_url 去查询。否则 refreshTerminalQuota() 不带参数会用
  // 已保存配置（智谱），导致「默认显示 MiniMax 却查智谱」。
  // 任一请求失败都退回已保存配置查询，保持可用。
  Promise.all([requestJson("/api/quota/config").catch(() => null), fetchQuotaApiPresets()])
    .then(([cfg, presets]) => {
      quotaConfigCache = cfg;
      const defaultProvider = cfg?.default_provider || "SAVED";
      populateQuotaKeyDropdown(presets, defaultProvider, cfg?.custom_preset_id);
      // populateQuotaKeyDropdown 已设置好下拉 value，这里读出选中项
      // 的 override 参数：默认平台（含「指定账号」）匹配到预设就用预设查，
      // 否则回退已保存配置。
      refreshTerminalQuota(selectedQuotaOverrideParams());
    })
    .catch(() => refreshTerminalQuota());
}

function closeTerminalQuotaDialog() {
  if (!terminalQuotaDialogEl) return;
  cancelMinimaxResetRefresh();
  if (terminalQuotaDialogEl.open) {
    terminalQuotaDialogEl.close();
  } else {
    terminalQuotaDialogEl.hidden = true;
  }
  restoreTerminalFocusAfterDialogClose();
}

function toggleTerminalQuotaSettingsPanel() {
  if (!terminalQuotaSettingsPanelEl) return;
  const willOpen = terminalQuotaSettingsPanelEl.hidden;
  terminalQuotaSettingsPanelEl.hidden = !willOpen;
  if (willOpen) {
    // 「指定账号」选项要先就位，loadQuotaConfigIntoInputs 才能回填当前选择。
    loadQuotaCustomPresetOptions().finally(() => loadQuotaConfigIntoInputs());
    loadQuotaApiPresetsIntoDropdown();
  }
}

async function saveTerminalQuotaConfig() {
  if (!terminalQuotaSaveConfigBtnEl) return;
  const apiKey = terminalQuotaApiKeyInputEl?.value?.trim() || "";
  const baseUrl = terminalQuotaBaseUrlInputEl?.value?.trim() || "";
  const defaultProvider = (terminalQuotaDefaultProviderEl?.value || "SAVED").trim();
  const customPresetId = (terminalQuotaCustomPresetSelectEl?.value || "").trim();
  terminalQuotaSaveConfigBtnEl.disabled = true;
  if (terminalQuotaConfigStatusEl) terminalQuotaConfigStatusEl.textContent = "保存中…";
  try {
    const res = await requestJson("/api/quota/config", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        api_key: apiKey,
        base_url: baseUrl,
        default_provider: defaultProvider,
        custom_preset_id: customPresetId || null,
      }),
    });
    // 同步本地缓存，selectedQuotaOverrideParams 立即能解析新指定的账号，
    // 「已保存配置」条目的实名 label（平台 + 掩码 key）也随保存刷新。
    quotaConfigCache = Object.assign({}, quotaConfigCache, {
      custom_preset_id: res.custom_preset_id ?? (customPresetId || null),
      api_key_masked: res.api_key_masked || (quotaConfigCache && quotaConfigCache.api_key_masked) || "",
      base_url: res.base_url || (quotaConfigCache && quotaConfigCache.base_url) || "",
    });
    if (terminalQuotaConfigStatusEl) {
      const providerLabelMap = {
        SAVED: "已保存配置",
        CUSTOM: "指定账号",
        ZHIPU: QUOTA_PLATFORM_LABELS.ZHIPU,
        MINIMAX: QUOTA_PLATFORM_LABELS.MINIMAX,
        OPENROUTER: QUOTA_PLATFORM_LABELS.OPENROUTER,
      };
      const providerLabel = providerLabelMap[res.default_provider] || res.default_provider;
      terminalQuotaConfigStatusEl.textContent =
        `已保存。当前 Key: ${res.api_key_masked || "?"} · 默认显示: ${quotaEsc(providerLabel)}`;
    }
    // 保存的是「默认配置」，把快速切换下拉复位到默认项（含「指定账号」）
    // 再按选中项查询，保证展示与数据一致。
    populateQuotaKeyDropdown(
      quotaApiPresetsCache,
      res.default_provider || "SAVED",
      quotaConfigCache.custom_preset_id,
    );
    refreshTerminalQuota(selectedQuotaOverrideParams());
  } catch (error) {
    if (terminalQuotaConfigStatusEl) {
      terminalQuotaConfigStatusEl.textContent = `保存失败: ${error.message || error}`;
    }
  } finally {
    terminalQuotaSaveConfigBtnEl.disabled = false;
  }
}
