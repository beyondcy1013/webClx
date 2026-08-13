// webClx terminal quota dialog and usage rendering helpers.
// Extracted from terminal.js as global declarations.
// Quota constants/cache live here; there is no top-level DOM setup.

// ---- 套餐用量查询 (quota) ----
// 平台枚举与后端 `src/quota.rs::QuotaPlatform` 对齐，按此在 filter 与
// render 阶段分派。当前只覆盖 ZHIPU（bigmodel.cn）与 MINIMAX
// （minimaxi.com / minimax.io）。新增平台时需要同时更新这里的
// PRESET_URL_FILTERS 与 renderQuotaReport。
const QUOTA_PLATFORM_LABELS = {
  ZHIPU: "智谱 GLM Coding Plan",
  MINIMAX: "MiniMax TokenPlan",
};
const PRESET_URL_FILTERS = [
  // 智谱：所有 bigmodel.cn 域名都算进来，包含 open.bigmodel.cn、
  // bigmodel.cn 与本地代理 /api/codex-proxy/zhipu/。
  (url) => /bigmodel\.cn|\/api\/codex-proxy\/zhipu\//i.test(url),
  // MiniMax：国内 minimaxi.com 与国际 minimax.io 都参与；同时兼容走本地
  // codex-proxy minimax 通路的预设。
  (url) => /minimaxi\.com|minimax\.io|\/api\/codex-proxy\/minimax\//i.test(url),
];
const quotaIsQuotaPreset = (preset) => {
  if (!preset || !preset.base_url) return false;
  if (!preset.api_key) return false;
  return PRESET_URL_FILTERS.some((fn) => fn(preset.base_url));
};
const quotaDetectPlatform = (baseUrl, fallback = "ZHIPU") => {
  const url = String(baseUrl || "").toLowerCase();
  if (/minimaxi\.com|minimax\.io|\/api\/codex-proxy\/minimax\//.test(url)) return "MINIMAX";
  if (/bigmodel\.cn|\/api\/codex-proxy\/zhipu\//.test(url)) return "ZHIPU";
  return fallback;
};

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
        detail = `重置: ${quotaEsc(quotaFormatResetTime(l.nextResetTime))}`;
        if (winText) detail = `${winText}; ${detail}`;
      } else if (l.type === "TIME_LIMIT") {
        windowLabel = quotaEsc(`MCP (${quotaUnitLabel(l.unit, l.number)})`);
        const dd = (l.usageDetails || []).map((d) => `${d.modelCode}: ${quotaFmtNum(d.usage)}`);
        detail = `${quotaFmtNum(l.currentValue)} / ${quotaFmtNum(l.usage)} (剩余 ${quotaFmtNum(l.remaining)})` + (dd.length ? `; ${dd.map((x) => quotaEsc(x)).join(", ")}` : "") + `; 重置: ${quotaEsc(quotaFormatResetTime(l.nextResetTime))}`;
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
    const url = "/api/quota/query" + (qs.toString() ? "?" + qs.toString() : "");
    const data = await requestJson(url);
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
    if (terminalQuotaConfigStatusEl) terminalQuotaConfigStatusEl.textContent = cfg.api_key_masked ? `当前 Key: ${cfg.api_key_masked}` : "";
  } catch (error) {
    if (terminalQuotaConfigStatusEl) terminalQuotaConfigStatusEl.textContent = `读取配置失败: ${error.message || error}`;
  }
}

let quotaApiPresetsCache = null;
async function fetchQuotaApiPresets() {
  try {
    const resp = await requestJson("/api/auth/api-presets");
    quotaApiPresetsCache = (resp.presets || []).filter(quotaIsQuotaPreset);
    return quotaApiPresetsCache;
  } catch (error) {
    quotaApiPresetsCache = null;
    return null;
  }
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
  return populateQuotaKeyDropdown(await fetchQuotaApiPresets(), "SAVED");
}

// 把下拉里第一个匹配平台（ZHIPU / MINIMAX）的预设设为选中；缺省
// `SAVED` 或找不到匹配项时回到「已保存配置」（空值）。
function populateQuotaKeyDropdown(presets, defaultProvider) {
  if (!terminalQuotaKeySelectEl) return;
  if (!presets || presets.length === 0) {
    terminalQuotaKeySelectEl.innerHTML = '<option value="">无可用预设</option>';
    return;
  }
  // value 形如 "<idx>"，留空表示用已保存配置查询。
  let selectIdx = "";
  if (defaultProvider === "ZHIPU" || defaultProvider === "MINIMAX") {
    const idx = presets.findIndex((p) => quotaDetectPlatform(p.base_url) === defaultProvider);
    if (idx >= 0) selectIdx = String(idx);
  }
  terminalQuotaKeySelectEl.innerHTML =
    '<option value="">已保存配置</option>' +
    presets
      .map((p, i) => {
        const masked = p.masked_api_key || String(p.api_key).slice(0, 8) + "***";
        const platform = quotaDetectPlatform(p.base_url);
        const platformLabel = QUOTA_PLATFORM_LABELS[platform] || platform;
        const label = `[${platformLabel}] ${(p.name || "未命名")}  |  ${masked}`;
        return '<option value="' + i + '">' +
          label.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;") +
          "</option>";
      })
      .join("");
  terminalQuotaKeySelectEl.value = selectIdx;
}

function selectedQuotaOverrideParams() {
  if (!terminalQuotaKeySelectEl || !quotaApiPresetsCache) return {};
  const raw = terminalQuotaKeySelectEl.value;
  if (raw === "") return {};
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
      const defaultProvider = cfg?.default_provider || "SAVED";
      populateQuotaKeyDropdown(presets, defaultProvider);
      // populateQuotaKeyDropdown 已设置好下拉 value，这里读出选中项
      // 的 override 参数：默认平台匹配到预设就用预设查，否则回退已保存配置。
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
    loadQuotaConfigIntoInputs();
    loadQuotaApiPresetsIntoDropdown();
  }
}

async function saveTerminalQuotaConfig() {
  if (!terminalQuotaSaveConfigBtnEl) return;
  const apiKey = terminalQuotaApiKeyInputEl?.value?.trim() || "";
  const baseUrl = terminalQuotaBaseUrlInputEl?.value?.trim() || "";
  const defaultProvider = (terminalQuotaDefaultProviderEl?.value || "SAVED").trim();
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
      }),
    });
    if (terminalQuotaConfigStatusEl) {
      const providerLabel = QUOTA_PLATFORM_LABELS[res.default_provider]
        || (res.default_provider === "SAVED" ? "已保存配置" : res.default_provider);
      terminalQuotaConfigStatusEl.textContent =
        `已保存。当前 Key: ${res.api_key_masked || "?"} · 默认显示: ${quotaEsc(providerLabel)}`;
    }
    // 保存的是「默认配置」，把快速切换下拉复位到默认平台对应项再查询。
    populateQuotaKeyDropdown(quotaApiPresetsCache, res.default_provider || "SAVED");
    refreshTerminalQuota();
  } catch (error) {
    if (terminalQuotaConfigStatusEl) {
      terminalQuotaConfigStatusEl.textContent = `保存失败: ${error.message || error}`;
    }
  } finally {
    terminalQuotaSaveConfigBtnEl.disabled = false;
  }
}
