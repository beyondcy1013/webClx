(() => {
  const list = document.getElementById("agy-preset-list");
  const name = document.getElementById("agy-preset-name");
  const input = document.getElementById("agy-preset-input");
  const status = document.getElementById("agy-status");
  let oauthSession = null;
  const setStatus = (message, tone = "muted") => {
    if (status) {
      status.textContent = message;
      status.dataset.tone = tone;
      status.hidden = !message;
    }
    const globalStatus = document.getElementById("codex-account-status");
    if (globalStatus && document.getElementById("agy-editor-panel")?.hidden !== false) {
      globalStatus.textContent = message;
      globalStatus.dataset.tone = tone === "error" ? "warn" : tone === "success" ? "ok" : tone;
      globalStatus.hidden = !message;
    }
  };
  const request = async (url, options = {}) => { const response = await fetch(url, { headers: { "Content-Type": "application/json", ...(options.headers || {}) }, ...options }); const body = await response.json().catch(() => ({})); if (!response.ok) throw new Error(body.error || body.message || `请求失败 (${response.status})`); return body; };
  const esc = (value) => { const node = document.createElement("span"); node.textContent = value == null ? "" : String(value); return node.innerHTML; };
  const formatTime = (seconds) => {
    if (!seconds) return "-";
    const date = new Date(Number(seconds) * 1000);
    if (Number.isNaN(date.getTime())) return "-";
    const pad = (n) => String(n).padStart(2, "0");
    return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
  };
  const copyText = async (text) => {
    if (!text) return false;
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
        return true;
      }
    } catch (_) {}
    try {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      const ok = document.execCommand("copy");
      document.body.removeChild(textarea);
      return ok;
    } catch (_) {
      return false;
    }
  };
  const findWeeklyResetTime = (summary) => {
    const groups = Array.isArray(summary?.groups) ? summary.groups : [];
    for (const g of groups) {
      if (Array.isArray(g?.buckets)) {
        for (const b of g.buckets) {
          const w = String(b?.window || b?.bucketId || b?.id || "").toLowerCase();
          const rt = b?.resetTime || b?.reset_time;
          if (w.includes("weekly") && rt) {
            return rt;
          }
        }
      }
    }
    return "";
  };

  const extractGeminiFromModels = (models, window, summary) => {
    if (!models || typeof models !== "object") return null;
    let fraction = null;
    let resetTime = null;
    for (const [key, val] of Object.entries(models)) {
      const lower = key.toLowerCase();
      if (!lower.includes("gemini") || lower.includes("argon") || val?.disabled === true) continue;
      const qi = val?.quotaInfo || val?.quota_info;
      if (qi) {
        const frac = qi.remainingFraction != null ? qi.remainingFraction : qi.remaining_fraction;
        if (typeof frac === "number" && Number.isFinite(frac) && frac >= 0) {
          fraction = fraction == null ? frac : Math.min(fraction, frac);
        }
        const rt = qi.resetTime || qi.reset_time;
        if (rt && !resetTime) {
          resetTime = rt;
        }
      }
    }
    if (fraction == null && !resetTime) return null;
    if (window === "weekly") {
      const weeklyReset = findWeeklyResetTime(summary) || resetTime || "";
      return {
        bucketId: "gemini-weekly",
        window: "weekly",
        displayName: "Weekly Limit Remaining",
        remainingFraction: 1.0,
        resetTime: weeklyReset,
      };
    }
    return {
      bucketId: "gemini-5h",
      window: "5h",
      displayName: "Five Hour Limit Remaining",
      remainingFraction: fraction,
      resetTime: resetTime || "",
    };
  };

  const extractArgonFromModels = (models, window, summary) => {
    if (!models || typeof models !== "object") return null;
    let fraction = null;
    let resetTime = null;
    for (const [key, val] of Object.entries(models)) {
      if (!key.toLowerCase().includes("argon") || val?.disabled === true) continue;
      const qi = val?.quotaInfo || val?.quota_info;
      if (qi) {
        const frac = qi.remainingFraction != null ? qi.remainingFraction : qi.remaining_fraction;
        if (typeof frac === "number" && Number.isFinite(frac) && frac >= 0) {
          fraction = fraction == null ? frac : Math.min(fraction, frac);
        }
        const rt = qi.resetTime || qi.reset_time;
        if (rt && !resetTime) {
          resetTime = rt;
        }
      }
    }
    if (fraction == null && !resetTime) return null;
    const weeklyReset = resetTime || findWeeklyResetTime(summary) || "";
    if (window === "weekly") {
      return {
        bucketId: "argon-weekly",
        window: "weekly",
        displayName: "Gemini 4 Argon Weekly Limit",
        remainingFraction: fraction,
        resetTime: weeklyReset,
      };
    }
    return null;
  };

  const classifyGroup = (group) => {
    const text = [
      group?.displayName,
      group?.name,
      group?.description,
      ...(Array.isArray(group?.buckets) ? group.buckets.map((b) => b?.bucketId || b?.id || b?.displayName || b?.name) : [])
    ].filter(Boolean).join(" ").toLowerCase();
    if (text.includes("argon")) return "argon";
    if (text.includes("gemini")) return "gemini";
    if (text.includes("claude") || text.includes("3p") || text.includes("gpt")) return "claude";
    return "other";
  };

  const formatQuotaItems = (summary, provider, models) => {
    const groups = Array.isArray(summary?.groups) ? summary.groups : [];
    const bucketByProviderAndWindow = (window) => {
      let b = groups
        .filter((group) => classifyGroup(group) === provider)
        .flatMap((group) => Array.isArray(group?.buckets) ? group.buckets : [])
        .find((bucket) => String(bucket?.window || bucket?.bucketId || bucket?.id || "").toLowerCase().includes(window));
      if (!b && provider === "gemini" && models) {
        b = extractGeminiFromModels(models, window, summary);
      }
      if (!b && provider === "argon" && models) {
        b = extractArgonFromModels(models, window, summary);
      }
      return b;
    };
    const formatBucket = (bucket, window) => {
      const fraction = bucket?.remainingFraction != null ? bucket.remainingFraction : bucket?.remaining_fraction;
      if (typeof fraction !== "number" || !Number.isFinite(fraction)) {
        return { value: "-", percent: null, resetTitle: "" };
      }
      const percent = Math.min(100, Math.max(0, fraction * 100));
      const rt = bucket?.resetTime || bucket?.reset_time;
      const resetText = rt ? `复位时间: ${formatNumericDate(new Date(rt))}` : "";
      const resetTitle = [bucket?.description, resetText].filter(Boolean).join(" | ");
      return { value: `${percent.toFixed(1)}%`, percent, resetTitle };
    };

    if (provider === "argon") {
      const wBucket = bucketByProviderAndWindow("weekly");
      const h5Bucket = bucketByProviderAndWindow("5h");
      if (!wBucket && !h5Bucket) return [];
      const res = [];
      if (wBucket) {
        res.push({ label: "周", ...formatBucket(wBucket, "weekly") });
      }
      if (h5Bucket) {
        res.push({ label: "5h", ...formatBucket(h5Bucket, "5h") });
      }
      return res;
    }

    return ["weekly", "5h"].map((window) => {
      const label = window === "weekly" ? "周" : "5h";
      return { label, ...formatBucket(bucketByProviderAndWindow(window), window) };
    });
  };

  const formatQuotaCell = (summary, models) => {
    if (summary && typeof summary === "object") {
      if (!models && summary.models) models = summary.models;
      if (summary.quotaSummary) summary = summary.quotaSummary;
    }
    if (!summary && !models) return null;
    const claudeItems = formatQuotaItems(summary, "claude", models);
    const geminiItems = formatQuotaItems(summary, "gemini", models);
    const argonItems = formatQuotaItems(summary, "argon", models);
    const hasAny = [...claudeItems, ...geminiItems, ...argonItems].some((item) => item?.percent != null);
    if (!hasAny) return null;
    const renderRow = (label, items) => {
      if (!items || !items.length) return "";
      const itemsHtml = items.map(quotaItemHtml).join('<span class="agy-quota-sep">·</span>');
      return `<div class="agy-time-row"><span class="agy-quota-window agy-model-label">${esc(label)}</span>${itemsHtml}</div>`;
    };
    const cRow = renderRow("Claude", claudeItems);
    const gRow = renderRow("Gemini", geminiItems);
    const aRow = argonItems.length && argonItems.some((i) => i.percent != null) ? renderRow("Argon", argonItems) : "";
    return `<div class="agy-time-lines">${cRow}${gRow}${aRow}</div>`;
  };

  const getProviderResetInfo = (summary, provider, models) => {
    const groups = Array.isArray(summary?.groups) ? summary.groups : [];
    const bucketByProviderAndWindow = (window) => {
      let b = groups
        .filter((group) => classifyGroup(group) === provider)
        .flatMap((group) => Array.isArray(group?.buckets) ? group.buckets : [])
        .find((bucket) => String(bucket?.window || bucket?.bucketId || bucket?.id || "").toLowerCase().includes(window));
      if (!b && provider === "gemini" && models) {
        b = extractGeminiFromModels(models, window, summary);
      }
      if (!b && provider === "argon" && models) {
        b = extractArgonFromModels(models, window, summary);
      }
      return b;
    };

    const now = Date.now();
    const parse = (window, label) => {
      const b = bucketByProviderAndWindow(window);
      if (!b) return null;
      const rt = b.resetTime || b.reset_time;
      const d = rt ? new Date(rt) : null;
      const valid = d && !Number.isNaN(d.getTime());
      const diff = valid ? d.getTime() - now : -1;
      return {
        label,
        bucket: b,
        date: valid ? d : null,
        diff,
        isReset: diff <= 0,
      };
    };

    const h5 = parse("5h", "5h");
    const weekly = parse("weekly", "周");
    if (!h5 && !weekly) return null;
    return { h5, weekly };
  };

  const formatNumericDate = (date) => {
    if (!date || Number.isNaN(date.getTime())) return "-";
    const pad = (n) => String(n).padStart(2, "0");
    const m = pad(date.getMonth() + 1);
    const d = pad(date.getDate());
    const hh = pad(date.getHours());
    const mm = pad(date.getMinutes());
    const ss = pad(date.getSeconds());
    return `${m}-${d} ${hh}:${mm}:${ss}`;
  };

  const formatCountdown = (diffMs) => {
    if (diffMs <= 0) return "已重置";
    const totalSeconds = Math.max(1, Math.floor(diffMs / 1000));
    const days = Math.floor(totalSeconds / 86400);
    const hours = Math.floor((totalSeconds % 86400) / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    if (days > 0) return `${days}天${hours}小时`;
    if (hours > 0) return `${hours}小时${minutes}分`;
    return `${minutes}分`;
  };

  const formatResetTimeCell = (summary, models) => {
    if (summary && typeof summary === "object") {
      if (!models && summary.models) models = summary.models;
      if (summary.quotaSummary) summary = summary.quotaSummary;
    }
    const c = getProviderResetInfo(summary, "claude", models);
    const g = getProviderResetInfo(summary, "gemini", models);
    const a = getProviderResetInfo(summary, "argon", models);
    if (!c && !g && !a) return null;
    const renderRow = (label, info) => {
      if (!info) return "";
      const parts = [];
      if (info.weekly?.date) {
        parts.push(`周 ${formatNumericDate(info.weekly.date)}`);
      } else if (label !== "Argon") {
        parts.push("周 -");
      }
      if (info.h5?.date) {
        parts.push(`5h ${formatNumericDate(info.h5.date)}`);
      } else if (label !== "Argon") {
        parts.push("5h -");
      }
      if (!parts.length) return "";
      const text = parts.join(" · ");
      const full5h = info.h5?.date ? `5h: ${formatNumericDate(info.h5.date)}` : (label === "Argon" ? "" : "5h: 无数据");
      const fullW = info.weekly?.date ? `周: ${formatNumericDate(info.weekly.date)}` : (label === "Argon" ? "" : "周: 无数据");
      const title = [label + " 复位时间", fullW, full5h].filter(Boolean).join(" | ");
      return `<div class="agy-time-row" title="${esc(title)}"><span class="agy-quota-window agy-model-label">${esc(label)}</span><span class="mono-text">${esc(text)}</span></div>`;
    };
    const cRow = renderRow("Claude", c);
    const gRow = renderRow("Gemini", g);
    const aRow = a ? renderRow("Argon", a) : "";
    return `<div class="agy-time-lines">${cRow}${gRow}${aRow}</div>`;
  };

  const formatCountdownCell = (summary, models) => {
    if (summary && typeof summary === "object") {
      if (!models && summary.models) models = summary.models;
      if (summary.quotaSummary) summary = summary.quotaSummary;
    }
    const c = getProviderResetInfo(summary, "claude", models);
    const g = getProviderResetInfo(summary, "gemini", models);
    const a = getProviderResetInfo(summary, "argon", models);
    if (!c && !g && !a) return null;
    const renderRow = (label, info) => {
      if (!info) return "";
      const parts = [];
      if (info.weekly) {
        parts.push(`周 ${formatCountdown(info.weekly.diff)}`);
      } else if (label !== "Argon") {
        parts.push("周 -");
      }
      if (info.h5) {
        parts.push(`5h ${formatCountdown(info.h5.diff)}`);
      } else if (label !== "Argon") {
        parts.push("5h -");
      }
      if (!parts.length) return "";
      const text = parts.join(" · ");
      const title = `${label} 倒计时 | ${parts.join(" | ")}`;
      return `<div class="agy-time-row" title="${esc(title)}"><span class="agy-quota-window agy-model-label">${esc(label)}</span><span class="mono-text">${esc(text)}</span></div>`;
    };
    const cRow = renderRow("Claude", c);
    const gRow = renderRow("Gemini", g);
    const aRow = a ? renderRow("Argon", a) : "";
    return `<div class="agy-time-lines">${cRow}${gRow}${aRow}</div>`;
  };

  const quotaItemHtml = (entry) => {
    const tone = entry.percent == null ? "" : entry.percent >= 50 ? " ok" : entry.percent >= 20 ? " mid" : " low";
    const bar = entry.percent == null ? "" : `<span class="agy-quota-bar${tone}"><span class="agy-quota-bar-fill" style="width:${entry.percent.toFixed(1)}%"></span></span>`;
    const titleAttr = entry.resetTitle ? ` title="${esc(entry.resetTitle)}"` : "";
    return `<span class="agy-quota-item"${titleAttr}><span class="agy-quota-window">${esc(entry.label)}</span>${bar}${esc(entry.value)}</span>`;
  };
  const renderRow = (item) => {
    const row = document.createElement("tr");
    if (item.current) {
      row.className = "agy-row-current current-row";
    }

    const actionItems = [
      {
        label: "切换到此账号",
        disabled: Boolean(item.current),
        title: item.current ? "已是当前账号" : "",
        handler: () => action(item.id, "apply"),
      },
      {
        label: "查询配额",
        handler: () => quota(item.id),
      },
      {
        label: "测试",
        handler: () => action(item.id, "test"),
      },
      {
        label: "删除",
        danger: true,
        handler: async () => {
          if (!window.confirm("删除这个 AGY 预设？")) return;
          try {
            await request(`/api/agy/presets/${encodeURIComponent(item.id)}`, { method: "DELETE" });
            setStatus("预设已删除", "success");
            await load();
          } catch (error) { setStatus(error.message, "error"); }
        },
      },
    ];

    const actionsCell = document.createElement("td");
    actionsCell.className = "agy-col-actions agy-actions-cell codex-account-action-cell";
    if (typeof createPresetActionMenu === "function") {
      actionsCell.appendChild(
        createPresetActionMenu(actionItems, { label: `${item.name || "账号"} 的操作` }),
      );
    } else {
      const applyBtn = item.current
        ? `<button class="button secondary agy-apply is-current" data-id="${esc(item.id)}" type="button" disabled title="当前已生效账号">当前</button>`
        : `<button class="button primary agy-apply" data-id="${esc(item.id)}" type="button">切换</button>`;
      actionsCell.innerHTML = `<div class="toolbar agy-actions"><button class="button secondary agy-quota" data-id="${esc(item.id)}" type="button">查询配额</button><button class="button secondary agy-test" data-id="${esc(item.id)}" type="button">测试</button>${applyBtn}<button class="button danger agy-delete" data-id="${esc(item.id)}" type="button">删除</button></div>`;
    }

    const cachedItems = item.quota_cache?.ok ? {
      quota: formatQuotaCell(item.quota_cache.quotaSummary, item.quota_cache.models),
      reset: formatResetTimeCell(item.quota_cache.quotaSummary, item.quota_cache.models),
      countdown: formatCountdownCell(item.quota_cache.quotaSummary, item.quota_cache.models),
    } : null;
    const timeTitle = item.quota_cached_at ? `缓存时间：${formatTime(item.quota_cached_at)}` : "尚未缓存";
    const quotaBody = cachedItems?.quota || "-";
    const resetBody = cachedItems?.reset || "-";
    const countdownBody = cachedItems?.countdown || "-";
    const quotaCellHtml = `<td class="agy-quota-cell agy-col-quota" data-quota-id="${esc(item.id)}" title="${esc(timeTitle)}">${quotaBody}</td>`;
    const resetCellHtml = `<td class="agy-reset-cell agy-col-reset" data-quota-id="${esc(item.id)}" title="${esc(timeTitle)}">${resetBody}</td>`;
    const countdownCellHtml = `<td class="agy-countdown-cell agy-col-countdown" data-quota-id="${esc(item.id)}" title="${esc(timeTitle)}">${countdownBody}</td>`;
    const nameTitle = item.email ? `点击复制邮箱: ${esc(item.email)}` : "未解析邮箱";
    const currentBadge = item.current ? ' <span class="agy-current-badge">当前</span>' : "";
    const nameCellHtml = `<td class="agy-col-name agy-name-clickable" data-copy-email="${esc(item.email || "")}" title="${nameTitle}">${esc(item.name || "未命名")}${currentBadge}</td>`;
    const emailCellHtml = `<td class="agy-col-email">${esc(item.email || "未解析")}</td>`;
    const statusCellHtml = `<td class="agy-col-status">${item.valid ? '<span class="status-badge success">可用</span>' : '<span class="status-badge error">无效</span>'}</td>`;
    const quotaAtText = typeof window.formatAccountRefreshedTime === "function"
      ? window.formatAccountRefreshedTime(item.quota_cached_at ? item.quota_cached_at * 1000 : null)
      : formatTime(item.quota_cached_at);
    const quotaAtCellHtml = `<td class="agy-col-quota-at agy-quota-at-cell" data-quota-id="${esc(item.id)}">${esc(quotaAtText)}</td>`;
    const savedCellHtml = `<td class="agy-col-saved agy-saved-cell">${esc(formatTime(item.saved_at))}</td>`;
    const lastSwitchValue = item.last_switch_at ?? item.lastSwitchAt ?? null;
    const lastSwitchText = typeof window.formatAccountRefreshedTime === "function"
      ? window.formatAccountRefreshedTime(lastSwitchValue)
      : formatTime(lastSwitchValue ? Number(lastSwitchValue) / 1000 : null);
    const lastSwitchTitle = lastSwitchText === "-" || lastSwitchText === "—"
      ? "切换时间未记录"
      : `最近切换时间：${lastSwitchText}`;
    const lastSwitchCellHtml = `<td class="agy-col-last-switch agy-last-switch-cell" title="${esc(lastSwitchTitle)}">${esc(lastSwitchText)}</td>`;
    const usageCount = typeof window.windowedUsageCount === "function"
      ? window.windowedUsageCount(item)
      : Number(item.switch_count || 0);
    const usageLabel = typeof window.getAccountUsageWindowLabel === "function"
      ? window.getAccountUsageWindowLabel()
      : "全部（累计）";
    const usageTitle = `「使用」统计窗口：${usageLabel}`;
    const usageCellHtml = `<td class="agy-col-usage agy-usage-cell" title="${esc(usageTitle)}">${esc(String(usageCount))}</td>`;

    row.appendChild(actionsCell);
    row.insertAdjacentHTML(
      "beforeend",
      `${nameCellHtml}${emailCellHtml}${statusCellHtml}${quotaCellHtml}${resetCellHtml}${countdownCellHtml}${quotaAtCellHtml}${savedCellHtml}${lastSwitchCellHtml}${usageCellHtml}`
    );
    return row;
  };
  let lastAgyItems = [];
  function paintAgyRows(items) {
    if (!list) return;
    list.innerHTML = "";
    if (!items.length) {
      list.innerHTML = '<tr><td colspan="11" class="muted-text">暂无 AGY 预设</td></tr>';
      return;
    }
    for (const item of items) {
      list.appendChild(renderRow(item));
    }
  }
  // 切换「使用」统计窗口时用缓存重画，不发起网络请求。
  function renderUsageWindow() {
    paintAgyRows(lastAgyItems);
  }
  async function load() {
    if (!list) return;
    try {
      const items = await request("/api/agy/presets");
      lastAgyItems = Array.isArray(items) ? items : [];
      paintAgyRows(lastAgyItems);
    } catch (error) { setStatus(error.message, "error"); }
  }
  const importItemsFromParsed = (parsed) => {
    if (Array.isArray(parsed)) return parsed.length ? parsed : null;
    if (!parsed || typeof parsed !== "object") return null;
    if (parsed.email || parsed.refresh_token || parsed.refreshToken) return [parsed];
    if (Array.isArray(parsed.accounts) && parsed.accounts.length) return parsed.accounts;
    if (Array.isArray(parsed.items) && parsed.items.length) return parsed.items;
    return null;
  };
  const describeImportResult = (result) => {
    const imported = Number(result.imported || 0);
    const updated = Number(result.updated || 0);
    const skipped = Number(result.skipped || 0);
    const errors = Array.isArray(result.errors) ? result.errors.filter(Boolean) : [];
    let message = `已导入 ${imported} 个新账号，更新 ${updated} 个账号。`;
    if (skipped) message += `跳过 ${skipped} 个。`;
    if (errors.length) message += errors.join("；");
    return { message, skipped };
  };
  async function savePreset() {
    let parsed;
    try { parsed = JSON.parse(input.value.trim()); }
    catch { setStatus("文本不是合法 JSON", "error"); return; }
    const items = importItemsFromParsed(parsed);
    if (items) {
      if (!window.confirm(`确定导入 ${items.length} 个 AGY 账号吗？相同邮箱会更新已有 refresh token。`)) return;
      try {
        setStatus("正在导入…", "info");
        const result = await request("/api/agy/presets/import", { method: "POST", body: JSON.stringify(items) });
        const { message, skipped } = describeImportResult(result);
        setStatus(message, skipped ? "info" : "success");
        if (!skipped) { name.value = ""; input.value = ""; }
        await load();
      } catch (error) { setStatus(error.message, "error"); }
      return;
    }
    try {
      await request("/api/agy/presets", { method: "POST", body: JSON.stringify({ name: name.value.trim(), token: parsed }) });
      name.value = "";
      input.value = "";
      setStatus("AGY 预设已保存", "success");
      await load();
    } catch (error) { setStatus(error.message, "error"); }
  }
  async function importCurrent() {
    try {
      setStatus("正在读取本机 AGY 登录…", "info");
      const res = await request("/api/agy/current/import", { method: "POST" });
      const label = res?.email || res?.name || res?.id || "";
      setStatus(
        res?.imported
          ? `本机已登录账号已备份到 AGY 预设${label ? `：${label}` : ""}`
          : `已同步本机登录账号到 AGY 预设${label ? `：${label}` : ""}`,
        "success",
      );
      await load();
    } catch (error) {
      setStatus(error.message, "error");
    }
  }
  async function startOauth() { try { setStatus("正在生成 AGY OAuth 登录链接…", "info"); const session = await request("/api/agy/oauth/start", { method: "POST" }); oauthSession = session; const open = document.getElementById("agy-oauth-open"); const panel = document.getElementById("agy-oauth-panel"); if (open) { open.href = session.auth_url; open.hidden = false; } if (panel) panel.hidden = false; window.open(session.auth_url, "agy-oauth", "noopener,noreferrer"); setStatus("请完成 Google 登录，然后粘贴回调 URL 或 code", "info"); } catch (error) { setStatus(error.message, "error"); } }
  async function exchangeOauth() { const raw = document.getElementById("agy-oauth-code")?.value.trim() || ""; if (!oauthSession || !raw) { setStatus("请先启动 OAuth 并粘贴回调 URL 或 code", "error"); return; } try { let code = raw; let state = oauthSession.state; try { const parsed = new URL(raw); code = parsed.searchParams.get("code") || raw; state = parsed.searchParams.get("state") || state; } catch (_) {} await request("/api/agy/oauth/exchange", { method: "POST", body: JSON.stringify({ session_id: oauthSession.session_id, state, code }) }); setStatus("AGY OAuth 登录成功，账号已保存", "success"); oauthSession = null; document.getElementById("agy-oauth-panel").hidden = true; document.getElementById("agy-oauth-open").hidden = true; document.getElementById("agy-oauth-code").value = ""; await load(); } catch (error) { setStatus(error.message, "error"); } }
  async function action(id, type) {
    try {
      const res = await request(`/api/agy/presets/${encodeURIComponent(id)}/${type}`, { method: type === "test" ? "POST" : "PUT" });
      if (res && res.ok === false) {
        throw new Error(res.error || "操作失败");
      }
      setStatus(type === "apply" ? "已切换。新启动的 agy 将使用该账号，现有会话不变。" : "结构测试通过", "success");
      await load();
    } catch (error) {
      setStatus(error.message, "error");
    }
  }
  async function quota(id) {
    const cells = {
      quota: list?.querySelector(`.agy-quota-cell[data-quota-id="${CSS.escape(id)}"]`),
      reset: list?.querySelector(`.agy-reset-cell[data-quota-id="${CSS.escape(id)}"]`),
      countdown: list?.querySelector(`.agy-countdown-cell[data-quota-id="${CSS.escape(id)}"]`),
      quotaAt: list?.querySelector(`.agy-quota-at-cell[data-quota-id="${CSS.escape(id)}"]`),
    };
    const { quotaAt, ...quotaCells } = cells;
    Object.values(quotaCells).forEach((cell) => { if (cell) { cell.textContent = "查询中…"; cell.dataset.tone = "info"; } });
    if (quotaAt) quotaAt.textContent = "查询中…";
    try {
      setStatus("正在查询 AGY 剩余额度…", "info");
      const result = await request(`/api/agy/presets/${encodeURIComponent(id)}/quota`);
      if (!result.ok) {
        const detail = typeof result.error === "string" ? result.error : result.error?.message || JSON.stringify(result.error || "查询失败");
        throw new Error(detail);
      }
      const quotaHtml = formatQuotaCell(result.quotaSummary, result.models);
      const resetHtml = formatResetTimeCell(result.quotaSummary, result.models);
      const countdownHtml = formatCountdownCell(result.quotaSummary, result.models);
      const nowTitle = `缓存时间：${formatNumericDate(new Date())}`;
      if (cells.quota) {
        cells.quota.innerHTML = quotaHtml || "-";
        cells.quota.dataset.tone = quotaHtml ? "success" : "info";
        cells.quota.title = nowTitle;
      }
      if (cells.reset) {
        cells.reset.innerHTML = resetHtml || "-";
        cells.reset.dataset.tone = resetHtml ? "success" : "info";
        cells.reset.title = nowTitle;
      }
      if (cells.countdown) {
        cells.countdown.innerHTML = countdownHtml || "-";
        cells.countdown.dataset.tone = countdownHtml ? "success" : "info";
        cells.countdown.title = nowTitle;
      }
      if (cells.quotaAt) {
        cells.quotaAt.textContent = typeof window.formatAccountRefreshedTime === "function"
          ? window.formatAccountRefreshedTime(Date.now())
          : formatNumericDate(new Date());
      }
      setStatus(quotaHtml ? "AGY 剩余额度已更新" : "接口已返回配额数据，但没有可展示的配额分组。", quotaHtml ? "success" : "info");
    } catch (error) {
      Object.values(quotaCells).forEach((cell) => {
        if (cell) { cell.innerHTML = `<div class="agy-quota-lines agy-quota-error">${esc(error.message)}</div>`; cell.dataset.tone = "error"; }
      });
      setStatus(error.message, "error");
    }
  }
  list?.addEventListener("click", async (event) => {
    const nameCell = event.target.closest(".agy-name-clickable");
    if (nameCell) {
      const selection = window.getSelection();
      const userSelectedText = !!selection
        && !selection.isCollapsed
        && nameCell.contains(selection.anchorNode)
        && nameCell.contains(selection.focusNode);
      if (userSelectedText || event.detail > 1) return;
      const email = nameCell.dataset.copyEmail;
      if (!email) {
        setStatus("该账号未解析到有效邮箱", "info");
        return;
      }
      const ok = await copyText(email);
      if (ok) {
        setStatus(`已复制邮箱: ${email}`, "success");
      } else {
        setStatus("复制邮箱失败，请手动复制", "error");
      }
      return;
    }
    const button = event.target.closest("button[data-id]");
    if (!button || button.disabled || button.classList.contains("is-current")) return;
    const id = button.dataset.id;
    if (button.classList.contains("agy-quota")) await quota(id);
    else if (button.classList.contains("agy-delete")) {
      if (!window.confirm("删除这个 AGY 预设？")) return;
      try {
        await request(`/api/agy/presets/${encodeURIComponent(id)}`, { method: "DELETE" });
        setStatus("预设已删除", "success");
        await load();
      } catch (error) { setStatus(error.message, "error"); }
    } else if (button.classList.contains("agy-test")) await action(id, "test");
    else if (button.classList.contains("agy-apply")) await action(id, "apply");
  });
  const importStatus = document.getElementById("agy-import-dialog-status");
  const setImportStatus = (message, tone = "muted") => {
    if (!importStatus) return;
    importStatus.textContent = message;
    importStatus.dataset.tone = tone;
    importStatus.hidden = !message;
  };
  const extractBalancedJson = (text, start) => {
    let depth = 0;
    let inString = false;
    let escaped = false;
    for (let i = start; i < text.length; i += 1) {
      const ch = text[i];
      if (inString) {
        if (escaped) escaped = false;
        else if (ch === "\\") escaped = true;
        else if (ch === '"') inString = false;
        continue;
      }
      if (ch === '"') inString = true;
      else if (ch === "{") depth += 1;
      else if (ch === "}") {
        depth -= 1;
        if (!depth) return text.slice(start, i + 1);
      }
    }
    return null;
  };
  const scanMixedAccountText = (text) => {
    const events = [];
    const emailPattern = /([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})(?=\s*----)/g;
    for (const match of text.matchAll(emailPattern)) {
      events.push({ at: match.index, type: "email", value: match[1] });
    }
    for (let i = text.indexOf("{"); i >= 0; ) {
      const blob = extractBalancedJson(text, i);
      if (!blob) { i = text.indexOf("{", i + 1); continue; }
      try {
        const parsed = JSON.parse(blob);
        if (parsed && typeof parsed === "object" && (parsed.refresh_token || parsed.refreshToken)) {
          events.push({ at: i, type: "json", value: parsed });
        }
      } catch (_) {}
      i = text.indexOf("{", i + blob.length);
    }
    events.sort((a, b) => a.at - b.at);
    const items = [];
    let pendingEmail = "";
    for (const event of events) {
      if (event.type === "email") { pendingEmail = event.value; continue; }
      const item = { ...event.value };
      if (typeof item.email !== "string" || !item.email.includes("@")) {
        if (!pendingEmail) continue;
        item.email = pendingEmail;
      }
      items.push(item);
    }
    return items;
  };
  const parseImportText = (raw) => {
    const text = String(raw || "").replace(/^\uFEFF/, "").trim();
    if (!text) throw new Error("请先提供账号 JSON");
    let parsed;
    try { parsed = JSON.parse(text); } catch { parsed = null; }
    if (typeof parsed === "string") {
      try { parsed = JSON.parse(parsed); } catch { parsed = null; }
    }
    if (parsed) {
      let items = null;
      if (Array.isArray(parsed)) items = parsed;
      else if (
        parsed &&
        typeof parsed === "object" &&
        (parsed.email ||
          parsed.id_token_email ||
          parsed.refresh_token ||
          parsed.refreshToken ||
          (parsed.token &&
            typeof parsed.token === "object" &&
            (parsed.token.refresh_token || parsed.token.refreshToken)) ||
          (parsed.format === "webclx-native-login" &&
            parsed.files?.[".gemini/antigravity-cli/antigravity-oauth-token"]))
      ) {
        items = [parsed];
      }
      else if (parsed && Array.isArray(parsed.accounts)) items = parsed.accounts;
      else if (parsed && Array.isArray(parsed.items)) items = parsed.items;
      if (Array.isArray(items) && items.length) return items;
    }
    const items = scanMixedAccountText(text);
    if (items.length) return items;
    throw new Error("需要包含 email 和 refresh_token 的 JSON 数组");
  };
  const openImportDialog = () => {
    const dialog = document.getElementById("agy-import-dialog");
    setImportStatus("");
    if (!dialog) return;
    if (typeof dialog.showModal === "function") {
      if (!dialog.open) dialog.showModal();
    } else dialog.setAttribute("open", "");
    document.getElementById("agy-import-text")?.focus();
  };
  const closeImportDialog = () => {
    const dialog = document.getElementById("agy-import-dialog");
    if (!dialog) return;
    if (typeof dialog.close === "function" && dialog.open) dialog.close();
    else dialog.removeAttribute("open");
  };
  async function readImportFile(file) {
    if (!file) return;
    const text = await file.text();
    const area = document.getElementById("agy-import-text");
    if (area) area.value = text;
    setImportStatus(`已读取 ${file.name || "文件"}，确认后导入`, "info");
  }
  async function readImportClipboard() {
    try {
      const text = await navigator.clipboard?.readText?.();
      if (!text || !text.trim()) {
        setImportStatus("剪贴板是空的，请粘贴到文本框", "error");
        return;
      }
      const area = document.getElementById("agy-import-text");
      if (area) area.value = text;
      setImportStatus("已从剪贴板读入，确认后导入", "info");
    } catch {
      setImportStatus("无法读取剪贴板，请直接粘贴到文本框", "error");
    }
  }
  async function importRefreshAccounts() {
    const area = document.getElementById("agy-import-text");
    let items;
    try { items = parseImportText(area?.value || ""); }
    catch (error) { setImportStatus(error.message, "error"); return; }
    if (!window.confirm(`确定导入 ${items.length} 个 AGY 账号吗？相同邮箱会更新已有 refresh token。`)) return;
    try {
      setImportStatus("正在导入…", "info");
      const result = await request("/api/agy/presets/import", { method: "POST", body: JSON.stringify(items) });
      const imported = Number(result.imported || 0);
      const updated = Number(result.updated || 0);
      const skipped = Number(result.skipped || 0);
      const errors = Array.isArray(result.errors) ? result.errors.filter(Boolean) : [];
      let message = `已导入 ${imported} 个新账号，更新 ${updated} 个账号。`;
      if (skipped) message += `跳过 ${skipped} 个。`;
      if (errors.length) message += errors.join("；");
      setStatus(message, skipped ? "info" : "success");
      setImportStatus(message, skipped ? "info" : "success");
      if (!skipped && area) area.value = "";
      await load();
      if (!skipped) closeImportDialog();
    } catch (error) { setImportStatus(error.message, "error"); }
  }
  const exportFileName = () => {
    const now = new Date();
    const pad = (value) => String(value).padStart(2, "0");
    return `antigravity_accounts_${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}.json`;
  };
  const loadExportAccounts = async () => {
    if (!window.confirm("确定导出全部 AGY 账号吗？导出内容包含 refresh token。")) return null;
    setStatus("正在准备导出…", "info");
    const result = await request("/api/agy/presets/export", { method: "POST", body: "{}" });
    const accounts = Array.isArray(result.accounts) ? result.accounts : [];
    if (!accounts.length) throw new Error("没有可导出的账号");
    return {
      accounts,
      skipped: Number(result.skipped || 0),
      text: `${JSON.stringify(accounts, null, 2)}\n`,
    };
  };
  const skippedNote = (skipped) => skipped ? `另有 ${skipped} 个账号没有可导出的 refresh token。` : "";
  const downloadExport = (text, fileName) => {
    const blob = new Blob([text], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = fileName;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  };
  async function exportAccountsFile() {
    try {
      const payload = await loadExportAccounts();
      if (!payload) return;
      const fileName = exportFileName();
      downloadExport(payload.text, fileName);
      setStatus(`已导出 ${payload.accounts.length} 个账号到 ${fileName}。${skippedNote(payload.skipped)}`, "success");
    } catch (error) { setStatus(error.message, "error"); }
  }
  async function exportAccountsClipboard() {
    try {
      const payload = await loadExportAccounts();
      if (!payload) return;
      const copied = await copyText(payload.text);
      if (!copied) {
        downloadExport(payload.text, exportFileName());
        setStatus("复制到剪贴板失败，已改为下载 JSON 文件。", "error");
        return;
      }
      setStatus(`已复制 ${payload.accounts.length} 个账号到剪贴板。${skippedNote(payload.skipped)}`, "success");
    } catch (error) { setStatus(error.message, "error"); }
  }
  document.getElementById("agy-export-file")?.addEventListener("click", exportAccountsFile);
  document.getElementById("agy-export-clipboard")?.addEventListener("click", exportAccountsClipboard);
  document.getElementById("agy-import-open")?.addEventListener("click", openImportDialog);
  document.getElementById("agy-import-cancel")?.addEventListener("click", closeImportDialog);
  document.getElementById("agy-import-clipboard")?.addEventListener("click", readImportClipboard);
  document.getElementById("agy-import-file-button")?.addEventListener("click", () => {
    document.getElementById("agy-import-file")?.click();
  });
  document.getElementById("agy-import-file")?.addEventListener("change", async (event) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    try { await readImportFile(file); } catch (error) { setImportStatus(error.message || "读取文件失败", "error"); }
  });
  document.getElementById("agy-import-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    importRefreshAccounts();
  });
  document.getElementById("agy-save")?.addEventListener("click", savePreset);
  document.getElementById("agy-import-current")?.addEventListener("click", importCurrent);
  document.getElementById("agy-oauth-start")?.addEventListener("click", startOauth);
  document.getElementById("agy-oauth-exchange")?.addEventListener("click", exchangeOauth);
  document.getElementById("agy-clear")?.addEventListener("click", () => { name.value = ""; input.value = ""; setStatus(""); });
  document.getElementById("agy-refresh")?.addEventListener("click", load);
  document.getElementById("agy-quota-all")?.addEventListener("click", async () => {
    setStatus("正在刷新所有 AGY 账号额度…", "info");
    try {
      const items = await request("/api/agy/presets");
      if (!items.length) { setStatus("暂无 AGY 预设", "info"); return; }
      await Promise.all(items.map((item) => quota(item.id)));
      setStatus("所有 AGY 账号额度已刷新完成", "success");
    } catch (error) { setStatus(error.message, "error"); }
  });
  document.getElementById("agy-test-all")?.addEventListener("click", async () => { setStatus("正在测试…"); try { const items = await request("/api/agy/presets"); await Promise.all(items.map((item) => action(item.id, "test"))); } catch (error) { setStatus(error.message, "error"); } });
  const viewEl = document.getElementById("agy-view");
  const toggleEmailBtn = document.getElementById("agy-toggle-email-col");
  const updateEmailColVisibility = (show) => {
    if (!toggleEmailBtn) return;
    if (viewEl) viewEl.classList.toggle("show-email-col", show);
    const tableWrap = document.getElementById("agy-table-wrap");
    if (tableWrap) tableWrap.classList.toggle("show-email-col", show);
    const apiView = document.getElementById("api-view");
    if (apiView) apiView.classList.toggle("show-email-col", show);
    toggleEmailBtn.textContent = show ? "隐藏邮箱" : "显示邮箱";
  };
  const storedShowEmail = localStorage.getItem("agy_show_email_col") === "true";
  updateEmailColVisibility(storedShowEmail);
  toggleEmailBtn?.addEventListener("click", () => {
    const nextShow = !viewEl?.classList.contains("show-email-col");
    localStorage.setItem("agy_show_email_col", String(nextShow));
    updateEmailColVisibility(nextShow);
  });

  async function detectCurrent() {
    setStatus("正在从本地 token 文件重新读取当前 AGY 账号…", "info");
    try {
      const res = await request("/api/agy/presets/detect-current", { method: "POST" });
      await load();
      if (res?.imported && (res?.email || res?.name || res?.id)) {
        setStatus(`已读取本机 AGY 登录并备份到账号列表：${res.name || res.email || res.id}（${res.email || res.id}）`, "success");
      } else if (res?.matched && res?.name) {
        setStatus(`已重新读取本地 AGY 当前生效账号：${res.name}（${res.email || res.id}）`, "success");
      } else if (res?.email) {
        setStatus(`已重新读取本地 AGY 当前生效凭据：${res.email}（未在 AGY 账号列表中）`, "info");
      } else {
        setStatus("已重新读取本地 AGY 配置：当前无有效 token 文件。", "muted");
      }
    } catch (error) {
      setStatus(`重新读取当前 AGY 账号失败：${error.message}`, "error");
    }
  }
  document.getElementById("agy-detect-current")?.addEventListener("click", detectCurrent);

  const optionsNote = document.getElementById("agy-options-note");
  const setOptionsNote = (message, tone = "muted") => {
    if (!optionsNote) return;
    optionsNote.textContent = message;
    optionsNote.dataset.tone = tone;
    optionsNote.hidden = !message;
  };
  async function refreshOptions() {
    const dangerousBox = document.getElementById("agy-option-dangerous");
    const autoEditsBox = document.getElementById("agy-option-auto-edits");
    if (!dangerousBox && !autoEditsBox) return;
    try {
      const res = await request("/api/agy/options");
      if (dangerousBox && typeof res.dangerous === "boolean") dangerousBox.checked = res.dangerous;
      if (autoEditsBox && typeof res.autoEdits === "boolean") autoEditsBox.checked = res.autoEdits;
      setOptionsNote("");
    } catch {
      // 读取失败时保留当前勾选状态。
    }
  }
  async function saveOptions() {
    const dangerousBox = document.getElementById("agy-option-dangerous");
    const autoEditsBox = document.getElementById("agy-option-auto-edits");
    if (!dangerousBox && !autoEditsBox) return;
    const dangerous = dangerousBox?.checked === true;
    const autoEdits = autoEditsBox?.checked === true;
    try {
      await request("/api/agy/options", {
        method: "PUT",
        body: JSON.stringify({ dangerous, auto_edits: autoEdits }),
      });
      const notes = [];
      notes.push(
        dangerous
          ? "危险也不问已开启：allow 追加 \"*\" 且 toolPermission=always-proceed"
          : "危险也不问已关闭：移除 \"*\" 和 toolPermission，其余 allow 保留",
      );
      notes.push(
        autoEdits
          ? "自动批准编辑已开启：executionMode=accept-edits"
          : "自动批准编辑已关闭：恢复默认逐个确认",
      );
      const text = notes.join("；");
      setOptionsNote(text, "ok");
      setStatus(text, "success");
    } catch (error) {
      const errText = error.message || "保存 AGY 选项失败。";
      setOptionsNote(errText, "error");
      setStatus(errText, "error");
      await refreshOptions();
    }
  }
  document.getElementById("agy-option-dangerous")?.addEventListener("change", saveOptions);
  document.getElementById("agy-option-auto-edits")?.addEventListener("change", saveOptions);
  refreshOptions();

  window.agyManager = { load, renderRow, renderUsageWindow, parseImportText, detectCurrent, importCurrent, setStatus, refreshOptions, saveOptions };
})();
